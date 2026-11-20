#!/usr/bin/env python3
"""End-to-end test of the relay. Start the stack first (see docs/testing.md)."""

from __future__ import annotations

import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

COMPOSE = ["docker", "compose", "-f", str(Path(__file__).with_name("compose.yml"))]
ADVERTISE_IP = "10.91.2.3"
LAN_ONLY_SERVICE = "_lan-only._tcp"
CAPTURE_SECONDS = 10
# A relay loop would multicast thousands of packets in one capture.
LOOP_PACKETS = 100


def compose(*args: str) -> str:
    return subprocess.run([*COMPOSE, *args], capture_output=True, text=True).stdout


def run_in(service: str, *command: str) -> str:
    return compose("exec", "-T", service, *command)


def start_capture(service: str, iface: str) -> subprocess.Popen:
    command = ["timeout", str(CAPTURE_SECONDS), "tcpdump", "-lni", iface, "udp", "port", "5353"]
    return subprocess.Popen(
        [*COMPOSE, "exec", "-T", service, *command],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )


def packets(capture: subprocess.Popen) -> list[str]:
    return capture.communicate()[0].splitlines()


def count(lines: list[str], text: str) -> int:
    return sum(text in line for line in lines)


def poll(check, timeout: float):
    deadline = time.monotonic() + timeout
    while True:
        result = check()
        if result or time.monotonic() > deadline:
            return result
        time.sleep(2)


def fail(message: str):
    sys.exit(f"FAIL: {message}")


def passed(message: str):
    print(f"ok: {message}")


def browse_hap() -> list[list[str]]:
    output = run_in("lan-client", "avahi-browse", "-rpt", "_hap._tcp")
    return [line.split(";") for line in output.splitlines() if line.startswith("=")]


def check_bridge_address() -> str:
    # HA's first start on a fresh CI runner takes up to a few minutes.
    resolved = poll(browse_hap, timeout=180)
    if not resolved:
        fail("lan-client did not resolve a _hap._tcp service")

    addresses = {fields[7] for fields in resolved}
    if addresses != {ADVERTISE_IP}:
        fail(f"bridge resolved to {sorted(addresses)}, expected {ADVERTISE_IP}")
    passed(f"bridge resolves to {ADVERTISE_IP}")

    return resolved[0][6]


def check_no_ipv6(host: str):
    if run_in("lan-client", "avahi-resolve", "-6", "-n", host).strip():
        fail(f"{host} has an IPv6 address on the LAN")
    passed(f"{host} has no IPv6 address")


def check_service_filter():
    capture = start_capture("sniff-oci", "oci0")
    time.sleep(2)
    # HA browses every type its integrations know, _airplay._tcp among them, from the
    # relay's own address -> only a type HA never asks for tells a leak apart.
    run_in(
        "lan-client",
        "sh",
        "-c",
        f'avahi-publish -s "Living Room" {LAN_ONLY_SERVICE} 7000 & sleep 3; avahi-browse -rt _hap._tcp; kill $!',
    )
    oci = packets(capture)

    # The _hap._tcp browse in the same window proves the capture works.
    if count(oci, "_hap._tcp") == 0:
        fail("no _hap._tcp query reached HA's network")
    leaked = count(oci, LAN_ONLY_SERVICE)
    if leaked:
        fail(f"{leaked} {LAN_ONLY_SERVICE} packets reached HA's network")
    passed(f"_hap._tcp queries reach HA, {LAN_ONLY_SERVICE} stays on the LAN")


def check_no_relay_loop():
    lan_capture = start_capture("lan-client", "lan0")
    oci_capture = start_capture("sniff-oci", "oci0")
    time.sleep(2)
    run_in("lan-client", "avahi-browse", "-rt", "_hap._tcp")
    lan, oci = len(packets(lan_capture)), len(packets(oci_capture))

    summary = f"{lan} packets on lan, {oci} on oci"
    if lan == 0 or oci == 0:
        fail(f"captured nothing ({summary})")
    if lan >= LOOP_PACKETS or oci >= LOOP_PACKETS:
        fail(f"possible relay loop: {summary}")
    passed(f"no relay loop ({summary} in {CAPTURE_SECONDS} s)")


def check_reconnect():
    since = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    compose("restart", "relay-lan")

    if not poll(lambda: "tunnel up" in compose("logs", "--since", since, "relay-oci"), timeout=60):
        fail("relay-oci did not reconnect")
    passed("relay-oci reconnects after relay-lan restarts")


def main():
    # Start from a fresh avahi cache; nothing resolved by an earlier run may count.
    compose("restart", "lan-client")
    time.sleep(2)

    host = check_bridge_address()
    check_no_ipv6(host)
    check_service_filter()
    check_no_relay_loop()
    check_reconnect()


if __name__ == "__main__":
    main()
