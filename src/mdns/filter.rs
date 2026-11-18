use simple_dns::{Name, Packet};

use super::DropReason;
use super::names::NameTable;
use super::packet::{normalize, record_sections_mut};

/// Keeps only questions and records about configured services and learned names.
pub fn filter_packet(packet: &mut Packet, names: &NameTable) -> Result<(), DropReason> {
    let keep = |name: &Name| is_relevant(&normalize(name), names);

    packet.questions.retain(|question| keep(&question.qname));
    for section in record_sections_mut(packet) {
        section.retain(|record| keep(&record.name));
    }

    if is_empty(packet) {
        Err(DropReason::NothingRelevant)
    } else {
        Ok(())
    }
}

fn is_relevant(name: &str, names: &NameTable) -> bool {
    names.contains(name)
        || names
            .services()
            .iter()
            .any(|service| name == service || is_subtype_of(name, service))
}

/// Subtypes look like `_primary._sub._hap._tcp.local`.
fn is_subtype_of(name: &str, service: &str) -> bool {
    name.strip_suffix(service)
        .is_some_and(|prefix| prefix.ends_with("._sub."))
}

fn is_empty(packet: &Packet) -> bool {
    packet.questions.is_empty()
        && packet.answers.is_empty()
        && packet.name_servers.is_empty()
        && packet.additional_records.is_empty()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::mdns::test_packets::{
        SERVICE, airplay_query, build, ha_response, iphone_qu_query, mixed_query, parse,
    };

    fn hap_names() -> NameTable {
        NameTable::new(vec![SERVICE.into()])
    }

    #[test]
    fn keeps_iphone_query_whole() {
        let bytes = iphone_qu_query();
        let mut packet = parse(&bytes);
        filter_packet(&mut packet, &hap_names()).unwrap();
        assert_eq!(build(&packet), build(&parse(&bytes)));
    }

    #[test]
    fn keeps_ha_response_whole_once_names_are_learned() {
        let bytes = ha_response();
        let mut packet = parse(&bytes);
        let mut names = hap_names();
        names.learn(&packet, Instant::now());
        filter_packet(&mut packet, &names).unwrap();
        assert_eq!(build(&packet), build(&parse(&bytes)));
    }

    #[test]
    fn drops_unrelated_packet() {
        let bytes = airplay_query();
        let result = filter_packet(&mut parse(&bytes), &hap_names());
        assert_eq!(result, Err(DropReason::NothingRelevant));
    }

    #[test]
    fn reduces_mixed_packet_to_homekit_part() {
        let bytes = mixed_query();
        let mut packet = parse(&bytes);
        filter_packet(&mut packet, &hap_names()).unwrap();
        assert_eq!(packet.questions.len(), 1);
        assert_eq!(normalize(&packet.questions[0].qname), SERVICE);
        assert_eq!(packet.answers, []);
    }

    #[test]
    fn keeps_subtypes_but_not_lookalikes() {
        let names = hap_names();
        assert!(is_relevant("_primary._sub._hap._tcp.local", &names));
        assert!(!is_relevant("x_hap._tcp.local", &names));
        assert!(!is_relevant("_services._dns-sd._udp.local", &names));
    }
}
