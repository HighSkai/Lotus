# Protocols: research and decisions

This page records what was evaluated (November 2026), what LOTUS uses, and why. The goal from the brief: **strong encryption, with a handshake and transport that resemble ordinary internet traffic as closely as practical, without weakening verification or adding layers that don't pay for themselves.**

## Engine: sing-box 1.14.4

Candidates: sing-box, Xray-core, mihomo, standalone WireGuard/wireguard-go, Hysteria's own client.

sing-box was chosen because it is the only maintained engine that combines all of:

- a mature **Windows TUN** stack with strict routing, IPv4+IPv6, and built-in WFP rules against Windows' multi-homed DNS leaks (1.14 added `dns_mode: hijack` with WFP filters);
- the protocols worth having: VLESS/REALITY/Vision, Hysteria2 (with Chrome QUIC fingerprint parroting since 1.14), TUIC, AnyTLS, NaiveProxy (Chromium network stack, official Windows builds bundle `libcronet.dll`), Shadowsocks-2022, Trojan, WireGuard endpoints;
- encrypted DNS client types (DoH, DoH/3, DoT) with rule-based blocking, which LOTUS mode reuses so there is one engine to verify and update;
- a local controller API for health checks and statistics;
- stable releases with published SHA-256 digests (1.14.4, 2026-10-24).

Xray-core has some exclusive features (XHTTP transport, VLESS Encryption with ML-KEM-768, post-quantum REALITY signatures), but its TUN and DNS story on Windows is far less mature, and running two engines would double the attack surface. LOTUS **refuses XHTTP and VLESS-Encryption links with an explanation** instead of silently mis-importing them.

The engine is pinned by version and digest (`scripts/fetch-engine.ps1`, `vendor/LOCK`), and the service re-hashes it before every launch while holding a write-deny handle (no swap between check and start). Generated configurations are tested against the real binary (`cargo test -p lotus-core` runs `sing-box check` on every protocol and option combination).

## SHADOW default: VLESS + REALITY + XTLS-Vision (TCP 443)

Still the best practical default, for these reasons:

| Property | How |
|---|---|
| Handshake looks ordinary | Real TLS 1.3 ClientHello in a current browser shape (uTLS `chrome`, which includes the X25519MLKEM768 hybrid key share like current Chrome). The server name is a real, popular site. |
| Resists active probing | Anyone who isn't authenticated (wrong REALITY key or short ID) is transparently relayed to the real site and sees its real certificate. There is no "proxy" answer to find. |
| No domain or certificate needed | REALITY authenticates the server with an X25519 key pair; the client verifies it cryptographically, with no "insecure" mode involved. |
| Hides TLS-in-TLS | XTLS-Vision pads the inner handshake, removing the length signature that tunnelled TLS otherwise leaves, and forwards already-encrypted inner TLS 1.3 records without encrypting them twice, which saves CPU. |
| Performance | Measured ≈ 470–1,160 Mbps through the full Windows TUN path on the test machine (see [TESTING.md](TESTING.md)). |
| Interop | Works with any sing-box or Xray server; LOTUS imports standard `vless://` links. |

Honest caveats:

- uTLS imitates Chrome; it is not Chrome. sing-box's own documentation notes that uTLS has had fingerprinting weaknesses. LOTUS mitigates this by tracking engine updates and defaulting to the `chrome` profile, which follows the current Chrome release in uTLS. Outdated and exotic profiles (`360`, `qq`) are rejected.
- A server whose IP obviously doesn't belong to the camouflage site (for example a cheap VPS claiming to be `www.microsoft.com`) can be noticed by an observer who correlates SNI with IP ownership. The installer picks the lowest-latency qualifying site from a candidate list. Choosing a site hosted near or alongside your VPS is better still (`--sni`).
- No transport is "undetectable". Long-lived, high-volume flows to a single VPS IP are themselves a signal.

## Second option: Hysteria2 (QUIC, UDP)

Added by the installer with `--with-hysteria2`. Use it when TCP struggles (high loss, long distances, mobile networks):

- QUIC with BBR congestion control. The client parrots Chrome's QUIC handshake by default (sing-box ≥ 1.14), so it resembles HTTP/3.
- **Security without "insecure"**: the installer generates a self-signed **ECDSA P-256** certificate (Chrome parroting can't negotiate Ed25519), and the LOTUS client **pins its public key** (`certificate_public_key_sha256`). Only that exact key is accepted. Tested: a wrong pin is refused. (sing-box treats a pin and an embedded trusted certificate as alternative trust anchors and refuses both together; LOTUS validation enforces the same rule.)
- Measured on the test machine through the full TUN path: ≈ 200–340 Mbps, lower than Vision's ≈ 1 Gbps on a clean local path. Hysteria2's advantage is on lossy, long-distance links, not clean ones.
- Salamander/gecko obfuscation is supported but off by default: it turns traffic into random-looking UDP, which looks *less* like ordinary traffic than plain HTTP/3.

## Other supported protocols (import and manual entry)

| Protocol | When it fits | Notes |
|---|---|---|
| NaiveProxy (HTTP/2 or QUIC) | You have a domain and want the most browser-authentic TLS | Uses Chromium's actual network stack (cronet) rather than an imitation. Needs a real certificate. |
| AnyTLS | TLS servers that support it | Padding scheme against traffic-pattern analysis. |
| Trojan (TCP/WS/gRPC) | Existing Trojan servers | Requires valid TLS; TLS-in-TLS is visible without extra measures. |
| VLESS + TLS (TCP/WS/gRPC/HTTP/2/HTTPUpgrade) | CDN-fronted setups | Vision is available on raw TCP. |
| TUIC v5 | Existing TUIC servers | QUIC-based. |
| Shadowsocks-2022 / AEAD | Simple setups | Fully random-looking bytes, which are themselves a known fingerprint. Stream ciphers are rejected. |
| WireGuard | Fast networks where blending in doesn't matter | Excellent crypto and speed, trivially identified by DPI. Configs with `PostUp` scripts are refused. |

**Rejected on purpose:** anything requiring `insecure`/`allowInsecure`; unencrypted VLESS/Trojan; VMess; ShadowsocksR; Shadowsocks stream ciphers and plugins; XHTTP; VLESS Encryption; partial-tunnel WireGuard configs (SHADOW is a full tunnel).

## Choices made for performance

- **No multiplexing** by default (it conflicts with Vision and adds head-of-line blocking).
- **QUIC blocked through TCP tunnels** (Auto): browsers fall back to HTTP/2 instead of carrying QUIC inside TCP, which behaves badly under loss. It's allowed automatically for UDP-native protocols (Hysteria2, TUIC, WireGuard, Naive-QUIC).
- **TUN MTU** stays at the engine default. The userspace stack terminates TCP locally, so the outer path MTU is handled by the kernel's own connection to the server. It's configurable in Advanced.
- **DNS**: 4,096-entry cache in the engine. Resolvers are contacted by IP with SNI, so there are no bootstrap lookups.
- **Server tuning** (installer `--tune`): BBR + fq and larger UDP buffers, reversible.

## Import formats

`lotus://import/<base64url JSON>` (from the installer; carries pins and certificates no standard link can) · `vless://` · `trojan://` · `ss://` (SIP002 and legacy base64) · `hysteria2://` / `hy2://` · `tuic://` · `anytls://` · `naive+https://` / `naive+quic://` · `wireguard://` · WireGuard `.conf` · sing-box JSON (full config, one outbound, or an array; WireGuard endpoint or legacy outbound) · base64 subscription blobs of any of the above.

Every imported profile is validated twice: in the app before saving, and again in the service before use.
