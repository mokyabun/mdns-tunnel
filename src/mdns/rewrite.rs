use std::net::Ipv4Addr;

use simple_dns::Packet;
use simple_dns::rdata::{A, RData};

use super::packet::record_sections_mut;

/// What LAN clients must use to reach HA; empty on the OpenWrt side.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Advertise {
    pub ip: Option<Ipv4Addr>,
    pub port: Option<u16>,
}

pub fn rewrite_packet(packet: &mut Packet, advertise: &Advertise) {
    // A QU question makes the responder answer by unicast, which never reaches the relay.
    for question in &mut packet.questions {
        question.unicast_response = false;
    }

    for section in record_sections_mut(packet) {
        if advertise.ip.is_some() {
            section.retain(|record| !matches!(record.rdata, RData::AAAA(_)));
        }

        for record in section {
            rewrite_rdata(&mut record.rdata, advertise);
        }
    }
}

fn rewrite_rdata(rdata: &mut RData, advertise: &Advertise) {
    match rdata {
        RData::A(a) => {
            if let Some(ip) = advertise.ip {
                *a = A::from(ip);
            }
        }
        RData::SRV(srv) => {
            if let Some(port) = advertise.port {
                srv.port = port;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdns::packet::records;
    use crate::mdns::test_packets::{build, ha_response, iphone_qu_query, parse};

    const ADVERTISE_IP: Ipv4Addr = Ipv4Addr::new(10, 8, 0, 2);

    fn oci_advertise(port: Option<u16>) -> Advertise {
        Advertise {
            ip: Some(ADVERTISE_IP),
            port,
        }
    }

    fn rewritten_ha_response(advertise: Advertise) -> Vec<u8> {
        let bytes = ha_response();
        let mut packet = parse(&bytes);
        rewrite_packet(&mut packet, &advertise);
        build(&packet)
    }

    fn has_rdata(packet: &Packet, matches: impl Fn(&RData) -> bool) -> bool {
        records(packet).any(|record| matches(&record.rdata))
    }

    #[test]
    fn clears_qu_bit() {
        let bytes = iphone_qu_query();
        let mut packet = parse(&bytes);
        rewrite_packet(&mut packet, &Advertise::default());
        assert!(
            packet
                .questions
                .iter()
                .all(|question| !question.unicast_response)
        );
    }

    #[test]
    fn replaces_a_and_drops_aaaa() {
        let bytes = rewritten_ha_response(oci_advertise(None));
        let packet = parse(&bytes);
        let addresses: Vec<Ipv4Addr> = records(&packet)
            .filter_map(|record| match &record.rdata {
                RData::A(a) => Some(Ipv4Addr::from(a.address)),
                _ => None,
            })
            .collect();
        assert_eq!(addresses, [ADVERTISE_IP]);
        assert!(!has_rdata(&packet, |rdata| matches!(rdata, RData::AAAA(_))));
        assert!(has_rdata(&packet, |rdata| matches!(rdata, RData::NSEC(_))));
        assert!(has_rdata(&packet, |rdata| matches!(rdata, RData::TXT(_))));
    }

    #[test]
    fn replaces_srv_port_only_when_configured() {
        let srv_port = |advertise| {
            let bytes = rewritten_ha_response(advertise);
            records(&parse(&bytes))
                .find_map(|record| match &record.rdata {
                    RData::SRV(srv) => Some(srv.port),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(srv_port(oci_advertise(None)), 21063);
        assert_eq!(srv_port(oci_advertise(Some(21064))), 21064);
    }

    #[test]
    fn empty_advertise_keeps_records_unchanged() {
        let bytes = ha_response();
        assert_eq!(
            rewritten_ha_response(Advertise::default()),
            build(&parse(&bytes))
        );
    }
}
