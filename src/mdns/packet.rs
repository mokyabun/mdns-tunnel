use simple_dns::{Name, Packet, ResourceRecord};

/// Lowercase without a trailing dot, the form used for every name comparison.
pub fn normalize(name: &Name) -> String {
    name.to_string().to_ascii_lowercase()
}

/// Records in the answer, authority and additional sections.
pub fn records<'p, 'a>(packet: &'p Packet<'a>) -> impl Iterator<Item = &'p ResourceRecord<'a>> {
    packet
        .answers
        .iter()
        .chain(&packet.name_servers)
        .chain(&packet.additional_records)
}

pub fn record_sections_mut<'p, 'a>(
    packet: &'p mut Packet<'a>,
) -> [&'p mut Vec<ResourceRecord<'a>>; 3] {
    [
        &mut packet.answers,
        &mut packet.name_servers,
        &mut packet.additional_records,
    ]
}
