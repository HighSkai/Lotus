# Testing

Everything below was run on a real Windows 11 Pro x64 machine on a residential connection with native IPv6, on 2026-11-12. Results come from Windows tools (`Get-DnsClientServerAddress`, `Find-NetRoute`, `Get-NetTCPConnection`, `curl`, `w32tm`, `Resolve-DnsName`) and real network requests, not from LOTUS's own status claims. No connection data was simulated.

**SHADOW exit server:** these tests used a sing-box VLESS + REALITY + Vision server (and a Hysteria2 server) **running on the same PC**, bound to the physical adapter. That exercises the real tunnel, TUN routing, DNS, kill switch and recovery paths. It can't show a *different* exit IP, so exit-IP change against a remote VPS is **not yet verified** (see the last section).

## Automated

`cargo test -p lotus-core` (22 tests, all passing):

- importers: VLESS/REALITY (IPv4 and IPv6 hosts), Trojan, SS SIP002 and legacy, SS-2022 key-length checks, Hysteria2, TUIC, WireGuard `.conf`, sing-box JSON, `lotus://` bundles, base64 subscriptions, share-link round trip;
- refusals: `insecure`/`allowInsecure`, unencrypted VLESS, VMess, XHTTP, WireGuard `PostUp`, stream ciphers, pin + certificate together;
- secrets never appear in import error excerpts or redacted log lines;
- **every generated engine config is validated by the real engine** (`sing-box check`): 11 protocol profiles × default/strict/probe variants, and 31 LOTUS resolver combinations;
- generated configs never contain file outputs, external UI, cache files, certificate paths or `insecure`.

## LOTUS mode (all passed)

| Test | Result |
|---|---|
| Connect from the UI switch | Connected in 1.9 s; status shown only after an encrypted upstream answered |
| Adapter DNS | `Ethernet 2` → `127.0.0.1` / `::1` (Windows reports it) |
| Real lookups work | `github.com` resolved |
| Tracker blocking | `doubleclick.net` → NXDOMAIN |
| Hard-coded DNS blocked | `Resolve-DnsName example.com -Server 8.8.8.8` timed out (WFP) |
| Public IP unchanged | trace IP = the user's own ISP address |
| Resolver identity (privacy check) | Answered by Cloudflare (AS13335), not the ISP's network |
| Only the engine talks to the resolver | single `sing-box` TCP connection to `1.1.1.1:443` |
| Disconnect restores DNS | DHCP DNS back (static `NameServer` empty), plaintext DNS works again, engine stopped |
| DNS latency (median of 8, uncached) | 13 ms via Cloudflare DoH (ISP router: ~3.2 s for the same NXDOMAIN names) |

## SHADOW mode, VLESS + REALITY + Vision (all passed)

| Test | Result |
|---|---|
| Server test (no routing change) | Handshake OK, 44–47 ms |
| Connect from the UI | Connected in 1.9 s, including leak and route verification |
| TUN adapter and routes | `LOTUS Shadow` up; best routes for 1.1.1.1, 8.8.8.8, 2606:4700:4700::1111 all via the TUN |
| Apps use the tunnel | Other apps' sockets sourced from `172.19.0.1`; remote connections owned by the exit server |
| TCP | HTTPS downloads through the tunnel |
| UDP | NTP to time.cloudflare.com via the tunnel; exit server holds UDP sockets |
| DNS through the tunnel | TUN DNS `172.19.0.2`; privacy check: Cloudflare AS13335 answered, not the ISP |
| Hard-coded DNS | Query to 8.8.8.8 captured by the TUN |
| IPv6 | Routed through the TUN |
| Kill switch while connected | `curl --interface <NIC IP>` (deliberate bypass) **blocked**; LAN (router) still reachable with allow-LAN |
| Throughput (25 MB, 6 runs) | 59–145 MB/s through the tunnel vs 84–156 MB/s direct |
| Idle CPU (connected) | service 0.00 %, engine 0.08 % |
| Disconnect | TUN removed, routes back on Ethernet, NIC-bound traffic allowed again, DNS untouched, no engine left running |

## SHADOW mode, Hysteria2 with a pinned key (all passed)

| Test | Result |
|---|---|
| `lotus://` link built by the installer's own shell logic | Imported, handshake OK (16 ms) |
| **Wrong key pin** | **Refused**: certificate trust is never bypassed |
| Connect, TCP + UDP through the TUN | Connected; downloads from `172.19.0.1`; NTP works |
| Kill switch bypass attempt | Blocked |
| Throughput | 26–42 MB/s (userspace QUIC on a clean local path) |

## Failure injection (elevated script `scripts/test-failover.ps1`)

| Event | Before the fix | After the fix |
|---|---|---|
| Engine killed mid-session | Direct traffic blocked, but status said "connected" for ~0.8 s (a network-event race) | State → *reconnecting* in < 130 ms, direct traffic **blocked** throughout, back to *connected* in ~2 s |
| **Service** killed | Leaked for ~2.3 s (dynamic WFP session died with the process) | **No leak**: persistent filters held until the TUN was back; auto-resumed in ~5 s |
| Service stopped and restarted (upgrade path) | n/a | Session resumed automatically from the encrypted record |

## Desktop integration

- Mode switch hands protection between modes (LOTUS → SHADOW without a server turns LOTUS off and restores DNS).
- Close → tray: the window and the **entire WebView2 are released** (0 MB); `lotus.exe` stays at ~5 MB private; reopening recreates the window.
- Second launch focuses the existing instance (single instance).
- Profiles stored encrypted (no plaintext UUID in `profiles.bin`).

## Installer (25 MB; tested as build 0.1.0, same source as v1.0.0 apart from the version number)

| Test | Result |
|---|---|
| Silent per-machine install | 8.4 s; files in `C:\Program Files\LOTUS` (app, service, engine, libcronet, rules, licenses) |
| Service registration | `LotusService`, LocalSystem, automatic start, running from Program Files |
| Service ACL | Reset to the Windows default on install; a normal user's `sc stop LotusService` is denied |
| App cold start | 258 ms to a visible window |
| Installed app, SHADOW to an unreachable server | Clear error ("The server didn't respond in time…"), kill switch released after the failed first attempt, internet unaffected |
| Installed app, SHADOW plus privacy check | Connected; the check ran in the UI with real results |
| **Uninstall while LOTUS was actively overriding DNS** | Service removed; adapter DNS back to DHCP (IPv4 and IPv6, static `NameServer` empty); DNS working; program files and `%ProgramData%\LOTUS` removed; no engine left running; no LOTUS kill-switch or DNS-guard filters remain |
| Reinstall | Service running again |

Note: sing-box itself (sing-tun) registers non-persistent *permit* filters for its own executable so it can listen. They aren't created by LOTUS, only allow the engine binary, and disappear at the next reboot.

## Resource use

| Component | Memory |
|---|---|
| lotus-service (idle or connected) | ~17 MB working set |
| engine, LOTUS mode | ~41 MB |
| engine, SHADOW mode | ~66 MB |
| lotus.exe, window closed | ~5 MB private |
| UI window open (WebView2, 6 processes) | ~240–330 MB private while visible |
| Disconnected | no engine process at all |

## Not yet verified (needs external hardware or infrastructure)

- **Exit-IP change through a remote VPS** and the server installer on a real Debian/Ubuntu VPS. The installer is syntax-checked, and its link builder and Hysteria2 certificate/pin logic were exercised locally. Its apt, systemd and firewall paths weren't run.
- Wi-Fi ↔ Ethernet switching and sleep/resume on hardware (code paths: network-change notifications and the SCM power events → re-verify or reconnect).
- Windows 10 (developed and tested on Windows 11).
- NaiveProxy, TUIC, AnyTLS, Trojan, Shadowsocks and WireGuard against live servers. Their configs are validated by the engine, but no live handshake was done.
- Long-duration soak testing.

One anomaly was observed once: right after a SHADOW connect, a single IPv6 download stalled (1 byte in 8 s). It didn't reproduce in the following 6 IPv4 and IPv6 runs.
