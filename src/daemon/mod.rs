//! Linux runtime: wires the mDNS socket, the relay and the tunnel together.

mod mdns_socket;

use std::convert::Infallible;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

use anyhow::Context;
use log::{debug, info, warn};

use crate::config::{Config, Role};
use crate::mdns::Relay;
use crate::tunnel::{OnPacket, Timeouts, Tunnel, TunnelWriter};
use mdns_socket::MdnsSocket;

/// mDNS packets are at most 9000 bytes (RFC 6762 section 17).
const MAX_PACKET_SIZE: usize = 9000;

/// A packet waiting for the event thread, tagged with where it came from.
enum Event {
    Local(Vec<u8>),
    Tunnel(Vec<u8>),
}

/// Runs the relay; returns only when the mDNS socket or the event thread fails.
pub fn run(config: Config) -> anyhow::Result<Infallible> {
    let socket = MdnsSocket::open(&config.iface)
        .with_context(|| format!("mDNS socket on {}", config.iface))?;
    let socket = Arc::new(socket);
    let (events, received) = mpsc::channel();

    let tunnel = Tunnel::new(tunnel_events(events.clone()), Timeouts::DEFAULT);
    let relay = Relay::new(config.services, config.advertise);
    let (multicast_socket, writer) = (Arc::clone(&socket), tunnel.writer());
    thread::spawn(move || relay_events(relay, &multicast_socket, &writer, received));
    start_tunnel(tunnel, config.role)?;

    receive_local_packets(&socket, &events)
}

fn start_tunnel(tunnel: Tunnel, role: Role) -> anyhow::Result<()> {
    match role {
        Role::Listen(addr) => {
            let listener =
                TcpListener::bind(addr).with_context(|| format!("tunnel listen on {addr}"))?;
            info!("listening for the tunnel on {addr}");
            thread::spawn(move || tunnel.listen(&listener));
        }
        Role::Connect(addr) => {
            info!("connecting the tunnel to {addr}");
            thread::spawn(move || tunnel.connect(addr));
        }
    }

    Ok(())
}

fn tunnel_events(events: Sender<Event>) -> OnPacket {
    // A dead event thread also fails the receive loop, which ends the process.
    Arc::new(move |packet| {
        let _ = events.send(Event::Tunnel(packet));
    })
}

/// Handles every packet in arrival order. A multicast packet is recorded before the next event
/// is taken, and its echo can only arrive after the send -> the echo is always recognized.
fn relay_events(
    mut relay: Relay,
    socket: &MdnsSocket,
    writer: &TunnelWriter,
    events: Receiver<Event>,
) {
    for event in events {
        let now = Instant::now();

        match event {
            Event::Local(packet) => {
                if let Ok(bytes) = relay.local_to_tunnel(&packet, now) {
                    writer.send(bytes);
                }
            }
            Event::Tunnel(packet) => {
                let Ok(bytes) = relay.tunnel_to_local(packet, now) else {
                    continue;
                };

                match socket.send_multicast(&bytes) {
                    Ok(()) => relay.record_multicast(&bytes, now),
                    Err(err) => warn!("multicast on the mDNS socket failed: {err}"),
                }
            }
        }
    }
}

fn receive_local_packets(
    socket: &MdnsSocket,
    events: &Sender<Event>,
) -> anyhow::Result<Infallible> {
    let mut buf = [0; MAX_PACKET_SIZE];

    loop {
        let (len, from) = socket.recv(&mut buf).context("mDNS receive")?;
        debug!("local packet of {len} bytes from {from}");

        events
            .send(Event::Local(buf[..len].to_vec()))
            .context("the event thread has stopped")?;
    }
}
