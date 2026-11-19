use std::convert::Infallible;
use std::io;
use std::process::ExitCode;

use log::error;
use mdns_tunnel::config::Config;
use mdns_tunnel::logger;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("mdns-tunnel {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let config = match Config::from_env(|var| std::env::var(var).ok()) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("mdns-tunnel: {err}");
            return ExitCode::FAILURE;
        }
    };
    logger::init(config.log_level);

    let Err(err) = run(config);
    error!("{err}");

    // Exiting lets procd or Docker restart the relay.
    ExitCode::FAILURE
}

#[cfg(target_os = "linux")]
fn run(config: Config) -> io::Result<Infallible> {
    mdns_tunnel::daemon::run(config)
}

#[cfg(not(target_os = "linux"))]
fn run(_: Config) -> io::Result<Infallible> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the mDNS socket requires Linux",
    ))
}
