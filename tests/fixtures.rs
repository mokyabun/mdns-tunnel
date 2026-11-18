use std::net::Ipv4Addr;
use std::time::Instant;

use mdns_tunnel::mdns::{
    Advertise, DropReason, NameTable, Relay, filter_packet, normalize, rewrite_packet,
};
use simple_dns::Packet;
use simple_dns::rdata::RData;

const SERVICE: &str = "_hap._tcp.local";
const INSTANCE: &str = "hass bridge d5f77c._hap._tcp.local";
const HOST: &str = "6d013e61ebfa49deb8d30ef1605e285a-hap.local";
const ADVERTISE_IP: Ipv4Addr = Ipv4Addr::new(10, 8, 0, 2);

const FIXTURES: &[&str] = &[
    "ha-announcement",
    "ha-response",
    "ha-host-response",
    "ha-browse-query",
    "avahi-ptr-query",
    "avahi-host-query",
    "airplay-response",
];

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}.bin", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

fn hap_names() -> NameTable {
    NameTable::new(vec![SERVICE.into()])
}

fn openwrt() -> Relay {
    Relay::new(vec![SERVICE.into()], Advertise::default())
}

fn oci() -> Relay {
    Relay::new(
        vec![SERVICE.into()],
        Advertise {
            ip: Some(ADVERTISE_IP),
            port: None,
        },
    )
}

fn rdata<'p, 'a>(packet: &'p Packet<'a>) -> impl Iterator<Item = &'p RData<'a>> {
    packet
        .answers
        .iter()
        .chain(&packet.name_servers)
        .chain(&packet.additional_records)
        .map(|record| &record.rdata)
}

fn a_addresses(packet: &Packet) -> Vec<Ipv4Addr> {
    rdata(packet)
        .filter_map(|rdata| match rdata {
            RData::A(a) => Some(Ipv4Addr::from(a.address)),
            _ => None,
        })
        .collect()
}

#[test]
fn every_fixture_parses() {
    for name in FIXTURES {
        let bytes = fixture(name);
        Packet::parse(&bytes).unwrap_or_else(|err| panic!("{name}: {err}"));
    }
}

#[test]
fn names_learned_from_ha_announcement() {
    let bytes = fixture("ha-announcement");
    let mut names = hap_names();
    names.learn(&Packet::parse(&bytes).unwrap(), Instant::now());
    assert!(names.contains(INSTANCE));
    assert!(names.contains(HOST));
}

#[test]
fn filter_keeps_ha_response_whole() {
    let bytes = fixture("ha-response");
    let mut packet = Packet::parse(&bytes).unwrap();
    let before = packet.answers.len() + packet.additional_records.len();
    let mut names = hap_names();
    names.learn(&packet, Instant::now());
    filter_packet(&mut packet, &names).unwrap();
    assert_eq!(
        packet.answers.len() + packet.additional_records.len(),
        before
    );
}

#[test]
fn filter_keeps_ptr_query() {
    let bytes = fixture("avahi-ptr-query");
    let mut packet = Packet::parse(&bytes).unwrap();
    filter_packet(&mut packet, &hap_names()).unwrap();
    assert_eq!(packet.questions.len(), 1);
}

#[test]
fn filter_drops_unrelated_packet() {
    let bytes = fixture("airplay-response");
    let mut packet = Packet::parse(&bytes).unwrap();
    assert_eq!(
        filter_packet(&mut packet, &hap_names()),
        Err(DropReason::NothingRelevant)
    );
}

#[test]
fn filter_reduces_browse_query_to_homekit_question() {
    let bytes = fixture("ha-browse-query");
    let mut packet = Packet::parse(&bytes).unwrap();
    assert!(packet.questions.len() > 1);
    filter_packet(&mut packet, &hap_names()).unwrap();
    let questions: Vec<String> = packet
        .questions
        .iter()
        .map(|q| normalize(&q.qname))
        .collect();
    assert_eq!(questions, [SERVICE]);
}

#[test]
fn rewrite_puts_advertise_ip_in_ha_response() {
    let bytes = fixture("ha-response");
    let mut packet = Packet::parse(&bytes).unwrap();
    assert_eq!(a_addresses(&packet), [Ipv4Addr::new(10, 90, 3, 2)]);
    rewrite_packet(
        &mut packet,
        &Advertise {
            ip: Some(ADVERTISE_IP),
            port: None,
        },
    );
    assert_eq!(a_addresses(&packet), [ADVERTISE_IP]);
    assert!(!rdata(&packet).any(|rdata| matches!(rdata, RData::AAAA(_))));
}

#[test]
fn ha_response_crosses_to_lan_rewritten() {
    let now = Instant::now();
    let (mut lan, mut ha) = (openwrt(), oci());
    let framed = ha.from_local(&fixture("ha-response"), now).unwrap();
    let multicast = lan.from_tunnel(&framed, now).unwrap();
    assert_eq!(
        a_addresses(&Packet::parse(&multicast).unwrap()),
        [ADVERTISE_IP]
    );
}

#[test]
fn lan_relays_host_query_after_learning_from_tunnel() {
    let now = Instant::now();
    let (mut lan, mut ha) = (openwrt(), oci());
    let host_query = fixture("avahi-host-query");
    assert_eq!(
        lan.from_local(&host_query, now),
        Err(DropReason::NothingRelevant)
    );
    let framed = ha.from_local(&fixture("ha-response"), now).unwrap();
    let multicast = lan.from_tunnel(&framed, now).unwrap();
    lan.sent_local(&multicast, now);

    assert!(lan.from_local(&host_query, now).is_ok());
    assert_eq!(lan.from_local(&multicast, now), Err(DropReason::Echo));
}
