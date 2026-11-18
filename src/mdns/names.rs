use std::collections::HashMap;
use std::time::{Duration, Instant};

use log::info;
use simple_dns::Packet;
use simple_dns::rdata::RData;

use super::packet::{normalize, records};

const NAME_TTL: Duration = Duration::from_hours(1);

/// Configured services plus the instance and host names learned from their records.
#[derive(Debug)]
pub struct NameTable {
    services: Vec<String>,
    instances: HashMap<String, Instant>,
    hosts: HashMap<String, Instant>,
}

impl NameTable {
    /// `services` must already be normalized, as `Config` does.
    pub fn new(services: Vec<String>) -> Self {
        Self {
            services,
            instances: HashMap::new(),
            hosts: HashMap::new(),
        }
    }

    pub fn services(&self) -> &[String] {
        &self.services
    }

    pub fn contains(&self, name: &str) -> bool {
        self.instances.contains_key(name) || self.hosts.contains_key(name)
    }

    /// PTR targets first, then SRV targets: one packet can teach both.
    pub fn learn(&mut self, packet: &Packet, now: Instant) {
        for record in records(packet) {
            if let RData::PTR(ptr) = &record.rdata
                && self.services.contains(&normalize(&record.name))
            {
                remember(&mut self.instances, "instance", normalize(&ptr.0), now);
            }
        }

        for record in records(packet) {
            if let RData::SRV(srv) = &record.rdata
                && self.instances.contains_key(&normalize(&record.name))
            {
                remember(&mut self.hosts, "host", normalize(&srv.target), now);
            }
        }
    }

    pub fn expire(&mut self, now: Instant) {
        let is_fresh =
            |_: &String, last_seen: &mut Instant| now.duration_since(*last_seen) < NAME_TTL;

        self.instances.retain(is_fresh);
        self.hosts.retain(is_fresh);
    }
}

fn remember(names: &mut HashMap<String, Instant>, kind: &str, name: String, now: Instant) {
    if !names.contains_key(&name) {
        info!("learned {kind} name {name}");
    }

    names.insert(name, now);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdns::test_packets::{
        HOST, INSTANCE, SERVICE, ha_response, parse, record_packet, srv,
    };

    fn hap_names() -> NameTable {
        NameTable::new(vec![SERVICE.into()])
    }

    fn instance() -> String {
        INSTANCE.to_ascii_lowercase()
    }

    #[test]
    fn learns_instance_and_host_from_one_packet() {
        let mut names = hap_names();
        names.learn(&parse(&ha_response()), Instant::now());
        assert!(names.contains(&instance()));
        assert!(names.contains(HOST));
    }

    #[test]
    fn ignores_srv_for_unknown_instance() {
        let mut names = hap_names();
        names.learn(
            &parse(&record_packet(srv(INSTANCE, HOST, 21063))),
            Instant::now(),
        );
        assert!(!names.contains(HOST));
    }

    #[test]
    fn ignores_ptr_for_other_service() {
        let mut names = NameTable::new(vec!["_airplay._tcp.local".into()]);
        names.learn(&parse(&ha_response()), Instant::now());
        assert!(!names.contains(&instance()));
        assert!(!names.contains(HOST));
    }

    #[test]
    fn names_expire_after_ttl() {
        let now = Instant::now();
        let mut names = hap_names();
        names.learn(&parse(&ha_response()), now);
        names.expire(now + NAME_TTL);
        assert!(!names.contains(HOST));
    }

    #[test]
    fn seeing_a_name_again_refreshes_it() {
        let now = Instant::now();
        let mut names = hap_names();
        names.learn(&parse(&ha_response()), now);
        names.learn(&parse(&ha_response()), now + NAME_TTL / 2);
        names.expire(now + NAME_TTL);
        assert!(names.contains(HOST));
    }
}
