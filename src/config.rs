use std::net::SocketAddr;
use std::str::FromStr;

use anyhow::{anyhow, bail};
use log::LevelFilter;

use crate::mdns::Advertise;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Listen(SocketAddr),
    Connect(SocketAddr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub iface: String,
    pub role: Role,
    pub services: Vec<String>,
    pub advertise: Advertise,
    pub log_level: LevelFilter,
}

impl Config {
    /// Tests pass their own `lookup` instead of touching the process environment.
    pub fn from_env(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let env = Env(lookup);

        Ok(Self {
            iface: env.required("IFACE")?,
            role: role(env.parsed("LISTEN")?, env.parsed("CONNECT")?)?,
            services: services(&env.required("SERVICES")?)?,
            advertise: Advertise {
                ip: env.parsed("ADVERTISE_IP")?,
                port: env.parsed("ADVERTISE_PORT")?,
            },
            log_level: match env.get("LOG_LEVEL") {
                Some(level) => log_level(&level)?,
                None => LevelFilter::Info,
            },
        })
    }
}

struct Env<F>(F);

impl<F: Fn(&str) -> Option<String>> Env<F> {
    /// Empty counts as unset: `LISTEN=` is the same as no `LISTEN`.
    fn get(&self, var: &str) -> Option<String> {
        (self.0)(var).filter(|value| !value.is_empty())
    }

    fn required(&self, var: &str) -> anyhow::Result<String> {
        self.get(var).ok_or_else(|| anyhow!("{var}: not set"))
    }

    fn parsed<T: FromStr>(&self, var: &str) -> anyhow::Result<Option<T>> {
        let Some(value) = self.get(var) else {
            return Ok(None);
        };

        value
            .parse()
            .map(Some)
            .map_err(|_| anyhow!("{var}: invalid value {value:?}"))
    }
}

fn role(listen: Option<SocketAddr>, connect: Option<SocketAddr>) -> anyhow::Result<Role> {
    match (listen, connect) {
        (Some(addr), None) => Ok(Role::Listen(addr)),
        (None, Some(addr)) => Ok(Role::Connect(addr)),
        (Some(_), Some(_)) => bail!("LISTEN: set only one of LISTEN and CONNECT"),
        (None, None) => bail!("LISTEN: set LISTEN or CONNECT"),
    }
}

/// Comma-separated, normalized like learned names: lowercase without a trailing dot.
fn services(value: &str) -> anyhow::Result<Vec<String>> {
    let services: Vec<String> = value
        .split(',')
        .map(|service| service.trim().trim_end_matches('.').to_ascii_lowercase())
        .filter(|service| !service.is_empty())
        .collect();

    if services.is_empty() {
        bail!("SERVICES: no service names");
    }

    Ok(services)
}

fn log_level(value: &str) -> anyhow::Result<LevelFilter> {
    match value {
        "error" => Ok(LevelFilter::Error),
        "warn" => Ok(LevelFilter::Warn),
        "info" => Ok(LevelFilter::Info),
        "debug" => Ok(LevelFilter::Debug),
        _ => bail!("LOG_LEVEL: unknown level {value:?}, expected error, warn, info or debug"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const OPENWRT: &[(&str, &str)] = &[
        ("IFACE", "br-lan"),
        ("LISTEN", "10.8.0.1:5354"),
        ("SERVICES", "_hap._tcp.local"),
    ];

    fn from_vars(vars: &[(&str, &str)]) -> anyhow::Result<Config> {
        let vars: HashMap<&str, &str> = vars.iter().copied().collect();
        Config::from_env(|var| vars.get(var).map(ToString::to_string))
    }

    /// The OpenWrt configuration with some variables replaced; an empty value unsets one.
    fn openwrt_with(overrides: &[(&str, &str)]) -> anyhow::Result<Config> {
        from_vars(&[OPENWRT, overrides].concat())
    }

    /// The variable an error names: messages read `VAR: reason`.
    fn rejected_var(overrides: &[(&str, &str)]) -> String {
        let message = openwrt_with(overrides).unwrap_err().to_string();
        message.split(':').next().unwrap().to_owned()
    }

    #[test]
    fn full_openwrt_config() {
        assert_eq!(
            from_vars(OPENWRT).unwrap(),
            Config {
                iface: "br-lan".into(),
                role: Role::Listen("10.8.0.1:5354".parse().unwrap()),
                services: vec!["_hap._tcp.local".into()],
                advertise: Advertise::default(),
                log_level: LevelFilter::Info,
            }
        );
    }

    #[test]
    fn full_oci_config() {
        let config = from_vars(&[
            ("IFACE", "eth0"),
            ("CONNECT", "10.8.0.1:5354"),
            ("SERVICES", "_HAP._tcp.local., _hap._udp.local"),
            ("ADVERTISE_IP", "10.8.0.2"),
            ("ADVERTISE_PORT", "21064"),
            ("LOG_LEVEL", "debug"),
        ]);
        assert_eq!(
            config.unwrap(),
            Config {
                iface: "eth0".into(),
                role: Role::Connect("10.8.0.1:5354".parse().unwrap()),
                services: vec!["_hap._tcp.local".into(), "_hap._udp.local".into()],
                advertise: Advertise {
                    ip: Some("10.8.0.2".parse().unwrap()),
                    port: Some(21064),
                },
                log_level: LevelFilter::Debug,
            }
        );
    }

    #[test]
    fn rejects_missing_iface() {
        assert_eq!(rejected_var(&[("IFACE", "")]), "IFACE");
    }

    #[test]
    fn rejects_both_listen_and_connect() {
        assert_eq!(rejected_var(&[("CONNECT", "10.8.0.1:5354")]), "LISTEN");
    }

    #[test]
    fn rejects_neither_listen_nor_connect() {
        assert_eq!(rejected_var(&[("LISTEN", "")]), "LISTEN");
    }

    #[test]
    fn rejects_invalid_socket_address() {
        assert_eq!(rejected_var(&[("LISTEN", "10.8.0.1")]), "LISTEN");
        assert_eq!(
            rejected_var(&[("LISTEN", ""), ("CONNECT", "router:5354")]),
            "CONNECT"
        );
    }

    #[test]
    fn rejects_empty_services() {
        assert_eq!(rejected_var(&[("SERVICES", "")]), "SERVICES");
        assert_eq!(rejected_var(&[("SERVICES", " , ")]), "SERVICES");
    }

    #[test]
    fn rejects_invalid_advertise_ip() {
        assert_eq!(rejected_var(&[("ADVERTISE_IP", "fd00::1")]), "ADVERTISE_IP");
    }

    #[test]
    fn rejects_invalid_advertise_port() {
        assert_eq!(
            rejected_var(&[("ADVERTISE_PORT", "70000")]),
            "ADVERTISE_PORT"
        );
    }

    #[test]
    fn rejects_unknown_log_level() {
        assert_eq!(rejected_var(&[("LOG_LEVEL", "trace")]), "LOG_LEVEL");
    }
}
