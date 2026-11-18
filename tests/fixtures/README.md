# mDNS fixtures

UDP payloads captured in the OrbStack test stack (HA `stable` with the HomeKit Bridge, avahi
0.8 on Debian trixie). Bridge instance `HASS Bridge D5F77C._hap._tcp.local`, host
`6d013e61ebfa49deb8d30ef1605e285a-hap.local`, HA container address `10.90.3.2`.

| File                   | Content                                                                        |
| ---------------------- | ------------------------------------------------------------------------------ |
| `ha-announcement.bin`  | HA announcing the bridge at startup: PTR, SRV, TXT, A, NSEC                    |
| `ha-response.bin`      | HA answering a `_hap._tcp` PTR query: PTR, NSEC, SRV, TXT, A                   |
| `ha-host-response.bin` | HA answering an A query for the bridge host: A, NSEC                           |
| `ha-browse-query.bin`  | HA's own zeroconf browse: QU PTR questions for many services, `_hap._tcp` among them |
| `avahi-ptr-query.bin`  | avahi browsing `_hap._tcp`, one QM PTR question                                |
| `avahi-host-query.bin` | avahi resolving the bridge host, one QM A question                             |
| `airplay-response.bin` | avahi announcing an unrelated `_airplay._tcp` service                          |

iPhone captures (QU queries with known answers) are still missing; they need the real router.
