# Development

This document describes how to run the CI checks locally. The Rust toolchain version is pinned in [rust-toolchain.toml](../rust-toolchain.toml).

## Checks

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked

docker compose -f tests/e2e/compose.yml up -d --build   # e2e environment that mimics the LAN, WireGuard and HA
tests/e2e/e2e.py

docker build --target export --output dist .            # static binary for the host architecture
```
