use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex, MutexGuard};

use log::{debug, warn};

use super::frame::{Frame, write_frame};

/// The write half of the current tunnel connection, shared with the daemon's event thread.
#[derive(Debug, Clone, Default)]
pub struct TunnelWriter {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    next_id: u64,
    current: Option<Connection>,
}

#[derive(Debug)]
struct Connection {
    id: u64,
    stream: TcpStream,
}

impl TunnelWriter {
    pub fn send(&self, packet: Vec<u8>) {
        if !self.write(&Frame::Packet(packet)) {
            debug!("tunnel is down, dropped packet");
        }
    }

    pub(super) fn send_keepalive(&self) {
        self.write(&Frame::Keepalive);
    }

    /// Makes `stream` the current connection, closing the one it replaces.
    pub(super) fn attach(&self, stream: TcpStream) -> u64 {
        let mut state = self.lock();
        let id = state.next_id;
        state.next_id += 1;

        if let Some(replaced) = state.current.replace(Connection { id, stream }) {
            replaced.close();
        }

        id
    }

    /// Closes connection `id` unless a newer connection has already replaced it.
    pub(super) fn detach(&self, id: u64) {
        let mut state = self.lock();
        if state
            .current
            .as_ref()
            .is_some_and(|current| current.id == id)
        {
            state.close_current();
        }
    }

    #[cfg(test)]
    pub(super) fn is_connected(&self) -> bool {
        self.lock().current.is_some()
    }

    /// Returns false when there is no connection to write to.
    fn write(&self, frame: &Frame) -> bool {
        let mut state = self.lock();
        let Some(current) = &mut state.current else {
            return false;
        };

        if let Err(err) = write_frame(&mut current.stream, frame) {
            warn!("tunnel write failed: {err}");
            // The reader sees the shutdown and ends the connection.
            state.close_current();
        }

        true
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("tunnel writer lock")
    }
}

impl State {
    fn close_current(&mut self) {
        if let Some(current) = self.current.take() {
            current.close();
        }
    }
}

impl Connection {
    fn close(self) {
        // Fails only when the socket is already gone, which is the goal anyway.
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}
