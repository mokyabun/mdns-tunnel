//! The TCP connection between the two relays.

pub mod frame;
mod writer;

use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use log::{info, warn};

use frame::{Frame, HelloError, read_frame, read_hello, write_hello};
pub use writer::TunnelWriter;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Called with every packet that arrives from the peer.
pub type OnPacket = Arc<dyn Fn(Vec<u8>) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub keepalive: Duration,
    /// The connection is closed after this long without any frame from the peer.
    pub idle: Duration,
}

impl Timeouts {
    pub const DEFAULT: Self = Self {
        keepalive: Duration::from_secs(15),
        idle: Duration::from_secs(45),
    };
}

#[derive(Clone)]
pub struct Tunnel {
    writer: TunnelWriter,
    on_packet: OnPacket,
    timeouts: Timeouts,
}

impl Tunnel {
    pub fn new(on_packet: OnPacket, timeouts: Timeouts) -> Self {
        Self {
            writer: TunnelWriter::default(),
            on_packet,
            timeouts,
        }
    }

    pub fn writer(&self) -> TunnelWriter {
        self.writer.clone()
    }

    /// Accepts peers forever; each peer that passes the hello replaces the previous one.
    pub fn listen(self, listener: &TcpListener) -> ! {
        self.spawn_keepalive();

        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    // A peer that stalls in the hello must not block the next accept.
                    let tunnel = self.clone();
                    thread::spawn(move || tunnel.accept(stream));
                }
                Err(err) => warn!("tunnel accept failed: {err}"),
            }
        }
    }

    /// Connects forever, retrying with exponential backoff that resets after a successful hello.
    pub fn connect(self, addr: SocketAddr) -> ! {
        self.spawn_keepalive();

        let mut backoff = MIN_BACKOFF;
        loop {
            match connect(addr, self.timeouts) {
                Ok(stream) => {
                    backoff = MIN_BACKOFF;
                    self.serve(stream);
                }
                Err(err) => warn!("tunnel connection to {addr} failed: {err}"),
            }

            thread::sleep(backoff);
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    fn accept(&self, mut stream: TcpStream) {
        match handshake(&mut stream, self.timeouts) {
            Ok(()) => self.serve(stream),
            Err(err) => warn!("tunnel handshake with {} failed: {err}", peer_name(&stream)),
        }
    }

    /// Runs one connection until it fails.
    fn serve(&self, mut stream: TcpStream) {
        let peer = peer_name(&stream);

        let id = match stream.try_clone() {
            Ok(write_half) => self.writer.attach(write_half),
            Err(err) => {
                warn!("tunnel to {peer} unusable: {err}");
                return;
            }
        };
        info!("tunnel up with {peer}");

        let err = self.read_frames(&mut stream);

        self.writer.detach(id);
        match err.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => {
                info!("tunnel to {peer} down: no frame within the idle timeout");
            }
            _ => info!("tunnel to {peer} down: {err}"),
        }
    }

    fn read_frames(&self, stream: &mut TcpStream) -> io::Error {
        loop {
            match read_frame(stream) {
                Ok(Frame::Keepalive) => {}
                Ok(Frame::Packet(packet)) => (self.on_packet)(packet),
                Err(err) => return err,
            }
        }
    }

    fn spawn_keepalive(&self) {
        let (writer, interval) = (self.writer(), self.timeouts.keepalive);
        thread::spawn(move || {
            loop {
                thread::sleep(interval);
                writer.send_keepalive();
            }
        });
    }
}

fn connect(addr: SocketAddr, timeouts: Timeouts) -> Result<TcpStream, HelloError> {
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    handshake(&mut stream, timeouts)?;
    Ok(stream)
}

fn handshake(stream: &mut TcpStream, timeouts: Timeouts) -> Result<(), HelloError> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(timeouts.idle))?;
    stream.set_write_timeout(Some(timeouts.idle))?;

    write_hello(stream)?;
    read_hello(stream)
}

fn peer_name(stream: &TcpStream) -> String {
    stream
        .peer_addr()
        .map_or_else(|_| "unknown peer".to_owned(), |addr| addr.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};
    use std::time::Instant;

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

    fn wait_until(condition: impl Fn() -> bool) {
        let start = Instant::now();
        while !condition() {
            assert!(start.elapsed() < WAIT, "condition not met within {WAIT:?}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// A tunnel whose received packets come out of the returned channel.
    fn tunnel(timeouts: Timeouts) -> (Tunnel, Receiver<Vec<u8>>) {
        let (tx, rx) = mpsc::channel();
        let on_packet: OnPacket = Arc::new(move |packet| tx.send(packet).unwrap());
        (Tunnel::new(on_packet, timeouts), rx)
    }

    fn listen(timeouts: Timeouts) -> (SocketAddr, TunnelWriter, Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tunnel, received) = tunnel(timeouts);
        let writer = tunnel.writer();
        thread::spawn(move || tunnel.listen(&listener));
        (addr, writer, received)
    }

    /// A raw peer that has completed the hello.
    fn raw_peer(addr: SocketAddr) -> TcpStream {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        write_hello(&mut stream).unwrap();
        read_hello(&mut stream).unwrap();
        stream
    }

    #[test]
    fn frames_pass_both_ways() {
        let (addr, listen_writer, from_connector) = listen(Timeouts::DEFAULT);
        let (connector, from_listener) = tunnel(Timeouts::DEFAULT);
        let connect_writer = connector.writer();
        thread::spawn(move || connector.connect(addr));
        wait_until(|| listen_writer.is_connected() && connect_writer.is_connected());

        connect_writer.send(vec![1, 2, 3]);
        assert_eq!(from_connector.recv_timeout(WAIT).unwrap(), [1, 2, 3]);
        listen_writer.send(vec![4, 5]);
        assert_eq!(from_listener.recv_timeout(WAIT).unwrap(), [4, 5]);
    }

    #[test]
    fn second_connection_replaces_first() {
        let (addr, writer, _received) = listen(Timeouts::DEFAULT);
        let mut first = raw_peer(addr);
        wait_until(|| writer.is_connected());
        let mut second = raw_peer(addr);

        let err = read_frame(&mut first).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        writer.send(vec![9]);
        assert_eq!(read_frame(&mut second).unwrap(), Frame::Packet(vec![9]));
    }

    #[test]
    fn silent_peer_is_dropped_after_idle_timeout() {
        let timeouts = Timeouts {
            keepalive: Duration::from_secs(60),
            idle: Duration::from_millis(200),
        };
        let (addr, writer, _received) = listen(timeouts);
        let mut silent = raw_peer(addr);
        wait_until(|| writer.is_connected());

        let start = Instant::now();
        let err = read_frame(&mut silent).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        assert!(start.elapsed() < WAIT);
        assert!(!writer.is_connected());
    }

    #[test]
    fn keepalives_are_sent() {
        let timeouts = Timeouts {
            keepalive: Duration::from_millis(50),
            idle: Duration::from_secs(5),
        };
        let (addr, _writer, _received) = listen(timeouts);
        let mut peer = raw_peer(addr);
        assert_eq!(read_frame(&mut peer).unwrap(), Frame::Keepalive);
    }
}
