//! Linux runtime: wires the mDNS socket, the relay and the tunnel together.

mod mdns_socket;

use std::convert::Infallible;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
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

type SharedRelay = Arc<Mutex<Relay>>;

/// Runs the relay; returns only when the mDNS socket fails.
pub fn run(config: Config) -> anyhow::Result<Infallible> {
    let socket = MdnsSocket::open(&config.iface)
        .with_context(|| format!("mDNS socket on {}", config.iface))?;
    let socket = Arc::new(socket);
    let relay = Arc::new(Mutex::new(Relay::new(config.services, config.advertise)));

    let tunnel = Tunnel::new(multicast_from_tunnel(&relay, &socket), Timeouts::DEFAULT);
    let writer = tunnel.writer();
    start_tunnel(tunnel, config.role)?;

    forward_local_packets(&socket, &relay, &writer)
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

fn multicast_from_tunnel(relay: &SharedRelay, socket: &Arc<MdnsSocket>) -> OnPacket {
    let (relay, socket) = (Arc::clone(relay), Arc::clone(socket));

    Arc::new(move |packet| {
        // Holding the lock across the send makes the receive thread wait until the echo of this
        // packet is recorded.
        let mut relay = relay.lock().expect("relay lock");
        let now = Instant::now();

        let Some(bytes) = relay.from_tunnel(&packet, now) else {
            return;
        };

        match socket.send_multicast(&bytes) {
            Ok(()) => relay.sent_local(&bytes, now),
            Err(err) => warn!("multicast on the mDNS socket failed: {err}"),
        }
    })
}

fn forward_local_packets(
    socket: &MdnsSocket,
    relay: &SharedRelay,
    writer: &TunnelWriter,
) -> anyhow::Result<Infallible> {
    let mut buf = [0; MAX_PACKET_SIZE];

    loop {
        let (len, from) = socket.recv(&mut buf).context("mDNS receive")?;
        debug!("local packet of {len} bytes from {from}");

        let relayed = relay
            .lock()
            .expect("relay lock")
            .from_local(&buf[..len], Instant::now());
        if let Ok(bytes) = relayed {
            writer.send(bytes);
        }
    }
}
