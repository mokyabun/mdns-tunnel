//! Packets shaped like the ones HA and iOS send, for unit tests.

use std::borrow::Cow;
use std::net::{Ipv4Addr, Ipv6Addr};

use simple_dns::rdata::{A, AAAA, NSEC, NsecTypeBitMap, PTR, RData, SRV, TXT};
use simple_dns::{CLASS, Name, Packet, QCLASS, QTYPE, Question, ResourceRecord, TYPE};

pub const SERVICE: &str = "_hap._tcp.local";
pub const INSTANCE: &str = "HASS Bridge._hap._tcp.local";
pub const HOST: &str = "homeassistant.local";
pub const CONTAINER_IP: Ipv4Addr = Ipv4Addr::new(172, 30, 0, 2);

pub fn parse(bytes: &[u8]) -> Packet<'_> {
    Packet::parse(bytes).expect("test packet parses")
}

fn name(name: &str) -> Name<'_> {
    Name::new_unchecked(name)
}

fn record<'a>(owner: &'a str, rdata: RData<'a>) -> ResourceRecord<'a> {
    ResourceRecord::new(name(owner), CLASS::IN, 120, rdata)
}

pub fn ptr<'a>(service: &'a str, instance: &'a str) -> ResourceRecord<'a> {
    record(service, RData::PTR(PTR(name(instance))))
}

pub fn srv<'a>(instance: &'a str, host: &'a str, port: u16) -> ResourceRecord<'a> {
    record(
        instance,
        RData::SRV(SRV {
            priority: 0,
            weight: 0,
            port,
            target: name(host),
        }),
    )
}

fn a(host: &str, ip: Ipv4Addr) -> ResourceRecord<'_> {
    record(host, RData::A(A::from(ip)))
}

fn aaaa(host: &str) -> ResourceRecord<'_> {
    record(
        host,
        RData::AAAA(AAAA::from(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 2))),
    )
}

fn txt(instance: &str) -> ResourceRecord<'_> {
    let txt = TXT::new()
        .with_string("md=HASS Bridge")
        .expect("valid TXT string");
    record(instance, RData::TXT(txt))
}

fn nsec(host: &str) -> ResourceRecord<'_> {
    record(
        host,
        RData::NSEC(NSEC {
            next_name: name(host),
            type_bit_maps: vec![NsecTypeBitMap {
                window_block: 0,
                bitmap: Cow::Owned(vec![0x40, 0, 0, 0x08]),
            }],
        }),
    )
}

fn question(qname: &str, qtype: TYPE, unicast_response: bool) -> Question<'_> {
    Question::new(
        name(qname),
        QTYPE::TYPE(qtype),
        QCLASS::CLASS(CLASS::IN),
        unicast_response,
    )
}

pub fn build(packet: &Packet) -> Vec<u8> {
    packet
        .build_bytes_vec_compressed()
        .expect("test packet builds")
}

pub fn record_packet(record: ResourceRecord) -> Vec<u8> {
    let mut packet = Packet::new_reply(0);
    packet.answers.push(record);
    build(&packet)
}

/// A HomeKit bridge answer: PTR, SRV and TXT, with the host's addresses as additional records.
pub fn ha_response() -> Vec<u8> {
    let mut packet = Packet::new_reply(0);
    packet.answers.push(ptr(SERVICE, INSTANCE));
    packet.answers.push(srv(INSTANCE, HOST, 21063));
    packet.answers.push(txt(INSTANCE));
    packet.additional_records.push(a(HOST, CONTAINER_IP));
    packet.additional_records.push(aaaa(HOST));
    packet.additional_records.push(nsec(HOST));
    build(&packet)
}

/// iOS browsing HomeKit: a QU question with a known answer.
pub fn iphone_qu_query() -> Vec<u8> {
    let mut packet = Packet::new_query(0);
    packet.questions.push(question(SERVICE, TYPE::PTR, true));
    packet.answers.push(ptr(SERVICE, INSTANCE));
    build(&packet)
}

pub fn host_query() -> Vec<u8> {
    let mut packet = Packet::new_query(0);
    packet.questions.push(question(HOST, TYPE::A, true));
    build(&packet)
}

pub fn airplay_query() -> Vec<u8> {
    let mut packet = Packet::new_query(0);
    packet
        .questions
        .push(question("_airplay._tcp.local", TYPE::PTR, false));
    build(&packet)
}

/// One query browsing HomeKit and AirPlay at once, as iOS often sends.
pub fn mixed_query() -> Vec<u8> {
    let mut packet = Packet::new_query(0);
    packet
        .questions
        .push(question("_airplay._tcp.local", TYPE::PTR, true));
    packet.questions.push(question(SERVICE, TYPE::PTR, true));
    packet.answers.push(ptr(
        "_airplay._tcp.local",
        "Living Room._airplay._tcp.local",
    ));
    build(&packet)
}
