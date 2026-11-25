//! Platform-independent relay logic: what crosses the tunnel and how it changes on the way.

mod filter;
mod loop_guard;
mod names;
mod packet;
mod rewrite;
#[cfg(test)]
mod test_packets;

use std::fmt;
use std::time::Instant;

use log::debug;
use simple_dns::Packet;

pub use filter::filter_packet;
use loop_guard::LoopGuard;
pub use names::NameTable;
pub use packet::normalize;
pub use rewrite::{Advertise, rewrite_packet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    Malformed,
    Echo,
    NothingRelevant,
}

impl fmt::Display for DropReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "malformed packet",
            Self::Echo => "echo of a relayed packet",
            Self::NothingRelevant => "nothing relevant",
        })
    }
}

/// State shared by both directions of one relay.
#[derive(Debug)]
pub struct Relay {
    names: NameTable,
    advertise: Advertise,
    loop_guard: LoopGuard,
}

impl Relay {
    pub fn new(services: Vec<String>, advertise: Advertise) -> Self {
        Self {
            names: NameTable::new(services),
            advertise,
            loop_guard: LoopGuard::default(),
        }
    }

    /// Turns a packet received on the mDNS socket into the bytes to send through the tunnel.
    pub fn local_to_tunnel(&mut self, bytes: &[u8], now: Instant) -> Result<Vec<u8>, DropReason> {
        self.names.expire(now);

        let result = self.process_local(bytes, now);
        log_drop("local", bytes.len(), result)
    }

    /// Returns the bytes to multicast for a packet received from the tunnel: the packet itself.
    pub fn tunnel_to_local(&mut self, bytes: Vec<u8>, now: Instant) -> Result<Vec<u8>, DropReason> {
        self.names.expire(now);

        let len = bytes.len();
        let result = self.process_tunnel(bytes, now);
        log_drop("tunnel", len, result)
    }

    /// Call after multicasting `bytes` -> their loopback copy counts as an echo.
    pub fn record_multicast(&mut self, bytes: &[u8], now: Instant) {
        self.loop_guard.record(bytes, now);
    }

    fn process_local(&mut self, bytes: &[u8], now: Instant) -> Result<Vec<u8>, DropReason> {
        let mut packet = Packet::parse(bytes).map_err(|_| DropReason::Malformed)?;
        if self.loop_guard.is_echo(bytes, now) {
            return Err(DropReason::Echo);
        }

        // Learn before filtering: an A record that arrives with its SRV must pass.
        self.names.learn(&packet, now);
        filter_packet(&mut packet, &self.names)?;
        rewrite_packet(&mut packet, &self.advertise);

        packet
            .build_bytes_vec_compressed()
            .map_err(|_| DropReason::Malformed)
    }

    fn process_tunnel(&mut self, bytes: Vec<u8>, now: Instant) -> Result<Vec<u8>, DropReason> {
        let packet = Packet::parse(&bytes).map_err(|_| DropReason::Malformed)?;
        self.names.learn(&packet, now);

        Ok(bytes)
    }
}

/// Passes `result` through, logging why the packet from `source` was dropped.
fn log_drop(
    source: &str,
    len: usize,
    result: Result<Vec<u8>, DropReason>,
) -> Result<Vec<u8>, DropReason> {
    if let Err(reason) = &result {
        debug!("dropped {source} packet of {len} bytes: {reason}");
    }

    result
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use simple_dns::rdata::RData;

    use super::*;
    use packet::records;
    use test_packets::{SERVICE, airplay_query, ha_response, host_query, iphone_qu_query, parse};

    const ADVERTISE_IP: Ipv4Addr = Ipv4Addr::new(10, 8, 0, 2);

    fn openwrt() -> Relay {
        Relay::new(vec![SERVICE.into()], Advertise::default())
    }

    fn oci() -> Relay {
        let advertise = Advertise {
            ip: Some(ADVERTISE_IP),
            port: None,
        };
        Relay::new(vec![SERVICE.into()], advertise)
    }

    /// Carries a local packet from one relay to the other, returning what the receiver multicasts.
    fn cross(from: &mut Relay, to: &mut Relay, bytes: &[u8], now: Instant) -> Vec<u8> {
        let framed = from.local_to_tunnel(bytes, now).unwrap();
        let multicast = to.tunnel_to_local(framed, now).unwrap();
        to.record_multicast(&multicast, now);
        multicast
    }

    #[test]
    fn iphone_query_crosses_with_qu_cleared() {
        let multicast = cross(
            &mut openwrt(),
            &mut oci(),
            &iphone_qu_query(),
            Instant::now(),
        );
        let packet = parse(&multicast);
        assert_eq!(packet.questions.len(), 1);
        assert!(!packet.questions[0].unicast_response);
    }

    #[test]
    fn ha_response_crosses_rewritten() {
        let multicast = cross(&mut oci(), &mut openwrt(), &ha_response(), Instant::now());
        let packet = parse(&multicast);
        assert!(records(&packet).any(
            |record| matches!(&record.rdata, RData::A(a) if Ipv4Addr::from(a.address) == ADVERTISE_IP)
        ));
        assert!(!records(&packet).any(|record| matches!(record.rdata, RData::AAAA(_))));
    }

    #[test]
    fn openwrt_relays_host_query_after_learning_from_tunnel() {
        let now = Instant::now();
        let (mut lan, mut ha) = (openwrt(), oci());
        assert_eq!(
            lan.local_to_tunnel(&host_query(), now),
            Err(DropReason::NothingRelevant)
        );

        cross(&mut ha, &mut lan, &ha_response(), now);
        assert!(lan.local_to_tunnel(&host_query(), now).is_ok());
    }

    #[test]
    fn echo_of_relayed_packet_is_dropped() {
        let now = Instant::now();
        let (mut lan, mut ha) = (openwrt(), oci());
        let multicast = cross(&mut ha, &mut lan, &ha_response(), now);
        assert_eq!(lan.local_to_tunnel(&multicast, now), Err(DropReason::Echo));
    }

    #[test]
    fn unrelated_and_malformed_packets_are_dropped() {
        let now = Instant::now();
        let mut lan = openwrt();
        assert_eq!(
            lan.local_to_tunnel(&airplay_query(), now),
            Err(DropReason::NothingRelevant)
        );
        assert_eq!(
            lan.local_to_tunnel(&[1, 2, 3], now),
            Err(DropReason::Malformed)
        );
        assert_eq!(
            lan.tunnel_to_local(vec![1, 2, 3], now),
            Err(DropReason::Malformed)
        );
    }
}
