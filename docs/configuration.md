# Configuration

This document describes the environment variables read by the server-side relay and the UCI options read by the relay on the OpenWrt router.

## Environment variables

The server-side relay is configured with environment variables. Set exactly one of `LISTEN` and `CONNECT`; an empty value counts as unset.

| Variable         | Default  | Description                                                                            |
| ---------------- | -------- | -------------------------------------------------------------------------------------- |
| `IFACE`          | required | Interface to send and receive mDNS on                                                  |
| `LISTEN`         |          | Address to accept the tunnel connection on (`<ip>:<port>`)                             |
| `CONNECT`        |          | Tunnel address to connect to (`<ip>:<port>`)                                           |
| `SERVICES`       | required | Comma-separated list of service names to relay                                         |
| `ADVERTISE_IP`   |          | Replaces the address in A records with this IPv4 address and drops AAAA records        |
| `ADVERTISE_PORT` |          | Replaces the port in SRV records, for when the host port differs from the service port |
| `LOG_LEVEL`      | `info`   | One of `error`, `warn`, `info`, `debug`                                                |

## OpenWrt UCI options

The router-side relay reads the `main` section of `/etc/config/mdns-tunnel`. The init script turns these options into the environment variables above and passes them to the relay.

| Option           | Default           | Description                                                          |
| ---------------- | ----------------- | -------------------------------------------------------------------- |
| `enabled`        | `0`               | The relay starts only when set to `1`                                |
| `network`        | `lan`             | Network to send and receive mDNS on                                  |
| `listen_network` | `wg0`             | Network to accept the tunnel connection on; its IPv4 address is used |
| `listen_port`    | `5354`            | Tunnel port                                                          |
| `service`        | `_hap._tcp.local` | Service to relay; add several with `list`                            |
| `log_level`      | `info`            | One of `error`, `warn`, `info`, `debug`                              |
