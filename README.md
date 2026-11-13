<div align="center">

<img src="docs/assets/hero.png" alt="LOTUS: Private DNS. Your own VPN." width="100%">

### Private DNS and a bring-your-own-server VPN for Windows

[![Release](https://img.shields.io/badge/release-v1.6.1-6e7bff?style=flat-square)](https://github.com/HighSkai/Lotus/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20x64-0078D4?style=flat-square)](#install)
[![License](https://img.shields.io/badge/license-GPL--3.0-b398ff?style=flat-square)](LICENSE)
[![Engine](https://img.shields.io/badge/engine-sing--box%201.14.4-1a1e3a?style=flat-square)](https://github.com/SagerNet/sing-box)
[![Built with](https://img.shields.io/badge/built%20with-Rust%20%C2%B7%20Tauri%202%20%C2%B7%20React-dea584?style=flat-square)](docs/ARCHITECTURE.md)

[**Download**](https://github.com/HighSkai/Lotus/releases/latest) &nbsp;·&nbsp; [Set up your server](docs/BYO-SERVER.md) &nbsp;·&nbsp; [How it works](docs/ARCHITECTURE.md) &nbsp;·&nbsp; [Privacy](docs/PRIVACY-SECURITY.md) &nbsp;·&nbsp; [Test results](docs/TESTING.md)

</div>

---

LOTUS is a small desktop app with one switch and two ways to protect you:

- **LOTUS mode** encrypts every DNS lookup on your PC. Your IP address stays the same.
- **SHADOW mode** sends your traffic through an encrypted tunnel to a server you own, so websites see your server instead of you.

No account, no subscription, no telemetry, and no LOTUS servers in the middle. You pick the resolver and you run the server.

<br>

<p align="center"><img src="docs/assets/features/feature-modes.webp" width="100%" alt="One switch, two modes. LOTUS, shown protected with 374 KB secured: encrypts every DNS lookup, and your IP address stays the same. SHADOW, shown connected with 1.35 GB secured: sends everything through a server you own."></p>

<br>

<p align="center"><img src="docs/assets/features/feature-routing.webp" width="100%" alt="Choose what goes through your server: traffic and DNS, traffic only for faster lookups, or DNS only. Change it while connected; if it can't connect, your previous setting comes back."></p>

<br>

<p align="center"><img src="docs/assets/features/feature-split.webp" width="100%" alt="Split tunnelling: keep games and streaming on your normal connection, or send only the apps and sites you pick through the tunnel. Shown excluding Discord, Steam and netflix.com, with apps open now ready to add."></p>

<br>

<p align="center"><img src="docs/assets/features/feature-console.webp" width="100%" alt="Watch your server at work: the live console shows the server, exit address, latency, open connections and live throughput, with a one-click check that it is really your server."></p>

<br>

<p align="center"><img src="docs/assets/features/feature-privacy.webp" width="100%" alt="Check for leaks in one click: the privacy check tests which resolver answers, the IPv4 and IPv6 exits, routing and the kill switch, and explains every result."></p>

<br>

<p align="center"><img src="docs/assets/features/feature-usage.webp" width="100%" alt="Usage that stays on your PC: data and time by day, month or year, per server and for LOTUS. Only totals are kept, never the sites you visit."></p>

<br>

## Also built in

- **Honest status.** SHADOW says *Connected* only after a real handshake with your server and a test request through Windows' own routing came back from your server's address. LOTUS says *Protected* only once the encrypted resolver has answered.
- **A kill switch that survives crashes.** Windows Filtering Platform rules let only the tunnel through. In testing, killing the engine or the LOTUS service leaked nothing, and the connection came back by itself.
- **Switch without dropping out.** Choosing the other mode leaves your connection on until you press the switch. If the new mode or server can't connect, the previous one comes straight back.
- **Secure by default.** There is no "allow insecure" option. Links that turn off certificate checks or use unencrypted or outdated protocols are refused, with an explanation.
- **Private by design.** No accounts and no analytics. Engine logs stay in memory with addresses removed, and saved servers are encrypted with your Windows account.
- **Bring your own server.** Paste a `vless://`, `ss://`, `trojan://`, `hysteria2://`, `tuic://`, `anytls://` or `naive+https://` link, a WireGuard `.conf` or sing-box JSON, or run the included installer on a fresh VPS.
- **Updates itself.** Signed updates download in the background and install only when you choose *Restart to update*.
- **Light and calm.** Ready about 120 ms after launch, about 5 MB in the tray, and animations you can set to Full, Less or None.

## LOTUS and SHADOW compared

| | **LOTUS** · private DNS | **SHADOW** · your own VPN |
|---|---|---|
| **Protects** | Every DNS lookup, from every app | All traffic: TCP and UDP, IPv4 and IPv6 |
| **Your public IP** | Unchanged, by design | Your server's |
| **Needs a server** | No | Yes: a Debian, Ubuntu or Arch Linux VPS, set up with one script |
| **Encryption** | DNS-over-HTTPS, DNS-over-HTTP/3, DNS-over-TLS | VLESS + REALITY + Vision by default; also Hysteria2, Trojan, Shadowsocks-2022, TUIC, AnyTLS, NaiveProxy, WireGuard |
| **Leak protection** | Every network adapter uses LOTUS; plain DNS to anything else is blocked | Whole-device tunnel, DNS through the tunnel, a kill switch that survives crashes |
| **You choose** | Resolver, protocol, tracker and malware blocking | What goes through the server, and which apps and sites skip it |

LOTUS mode can't hide your IP address. Only relaying your traffic through another machine can, and that's what SHADOW does. More in [LOTUS vs SHADOW](docs/MODES.md).

## How it works

```mermaid
flowchart LR
  subgraph PC["Your PC"]
    UI["LOTUS app<br/>(unprivileged)"] -- "typed commands<br/>over a local pipe" --> SVC["LOTUS service<br/>(LocalSystem)"]
    SVC --> ENG["sing-box engine<br/>(SHA-256 verified)"]
    SVC --> WFP["Windows Filtering Platform<br/>DNS guard · kill switch"]
  end
  ENG -- "LOTUS · DoH / DoH3 / DoT" --> DNS["Encrypted resolver<br/>Cloudflare · Quad9 · Mullvad · AdGuard · Google"]
  ENG -- "SHADOW · VLESS + REALITY + Vision<br/>or Hysteria2 (QUIC)" --> VPS["Your server<br/>exit IP"]
  VPS --> NET(("Internet"))
```

The window you click on has no special permissions. A small Windows service does the privileged work (DNS settings, the tunnel adapter, firewall rules) and accepts only a fixed set of typed commands: never raw engine configuration, file paths or programs to run. Details in [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Install

1. Download **`LOTUS_1.6.1_x64-setup.exe`** from the [latest release](https://github.com/HighSkai/Lotus/releases/latest) and approve the administrator prompt.
2. Open LOTUS, choose **LOTUS** or **SHADOW** at the top, and press the switch.

Requires Windows 10 or 11 (x64). WebView2 comes with Windows 11 and is installed automatically if missing.

> [!NOTE]
> The installer isn't code-signed yet, so SmartScreen may show *"Windows protected your PC"*. Check the SHA-256 published with the release, then choose **More info → Run anyway**.

LOTUS keeps itself up to date from version 1.2.0 on. If you have 1.1.0 or earlier, install the latest version once by hand.

To uninstall, use **Settings → Apps → LOTUS**. It stops the service, restores your original DNS settings and removes every LOTUS firewall rule.

## Set up your own server (SHADOW)

Any VPS or Linux machine with a public IP address, x86-64 or ARM64:

| Distribution | Versions |
|---|---|
| Debian | 12, 13 |
| Ubuntu | 22.04, 24.04 |
| Arch Linux | Rolling, including Manjaro, EndeavourOS and other Arch-based systems |

```bash
curl -fsSLO https://github.com/HighSkai/Lotus/releases/latest/download/lotus-server-install.sh
sudo bash lotus-server-install.sh        # add --with-hysteria2 for a UDP/QUIC profile as well
```

The script:

- checks the sing-box download against a pinned SHA-256
- generates REALITY keys and installs a hardened systemd service
- asks before opening ports, and never overwrites existing firewall rules
- tests the result with a real handshake, then prints your `lotus://` link

Paste the link into **LOTUS → Add server → Paste**. Remove everything with `--uninstall`. The full guide is in [BYO-SERVER.md](docs/BYO-SERVER.md).

## Why VLESS + REALITY + Vision?

Of the maintained options in late 2026, it strikes the best balance:

- **It looks ordinary.** The handshake looks like a current browser's TLS 1.3 visit to a real, popular website.
- **It's hard to probe.** Anyone without your key is quietly passed to that real site and sees its real certificate.
- **It's easy to run.** No domain or certificate needed.
- **It's fast.** Vision avoids encrypting already-encrypted traffic twice; measured close to 1 Gbps through the full Windows tunnel.

Hysteria2 (QUIC, with a browser-like handshake and a pinned server key) is the alternative for lossy connections. The research, including what was rejected and why, is in [PROTOCOLS.md](docs/PROTOCOLS.md).

> [!IMPORTANT]
> No tool makes traffic undetectable, and LOTUS doesn't claim to. It raises the cost of detection and keeps your data encrypted. Use it lawfully and within your network's rules.

## Tested, not assumed

Each result was checked on a real Windows 11 PC with Windows' own tools and packet captures of the network card.

| Check | Result |
|---|---|
| LOTUS: every resolver answers over DoH, DoH3 and DoT, with TLS 1.3 | Pass (16 of 16) |
| LOTUS: plain DNS on the wire while on | None (0 of 1,338 packets) |
| LOTUS: apps with hard-coded DNS (8.8.8.8:53) | Blocked |
| LOTUS: data secured vs a packet capture | Identical, to the byte |
| SHADOW: share of bytes going to your server | 99.7%, nothing anywhere else |
| SHADOW: an unencrypted web request | Not readable on the wire |
| SHADOW: IPv4 and IPv6, TCP and UDP routed via the tunnel | Pass |
| Split tunnelling and "send through your server" choices, judged at both ends | Routed as set, in every mode |
| Kill switch: deliberate bypass attempt (IPv4 and IPv6) | Blocked |
| Engine killed mid-session: leak / recovery | None / about 2 s |
| Service killed mid-session: leak / recovery | None / about 5 s |
| Time to connect, including checks | About 1.9 s |
| Uninstall while connected | DNS restored, no rules left |

LOTUS has also been run against real servers: a Google Cloud Debian VM (exit IP changed, all privacy checks pass, and probing it from outside shows only the camouflage site) and an Arch Linux machine. The full audit, performance figures, and what hasn't been verified yet are in [TESTING.md](docs/TESTING.md).

## Build from source

Requires Windows 10/11 x64, Rust (stable, MSVC), Visual Studio 2022 Build Tools (C++) and Node.js 20+.

```powershell
git clone https://github.com/HighSkai/Lotus.git
cd Lotus
powershell -ExecutionPolicy Bypass -File scripts\build.ps1
```

`build.ps1` fetches the pinned engine (SHA-256 verified), runs the tests (every generated engine configuration is checked by the real `sing-box check`), builds the service and the app, and writes the installer to `target\release\bundle\nsis\`. If the update signing key exists at `%USERPROFILE%\.tauri\lotus-updater.key`, it also signs the installer and writes `latest.json`; upload both with the release so installed copies can update.

<details>
<summary>Development loop and command-line control</summary>

```powershell
powershell -ExecutionPolicy Bypass -File scripts\fetch-engine.ps1
cargo build -p lotus-service
# once, elevated: registers the service from target\debug (development only)
powershell -ExecutionPolicy Bypass -File scripts\dev-install.ps1 -AllowUserControl
cd app; npm install; npx tauri dev
```

Run `cd app; npm run dev` on its own to preview the interface in a browser at http://127.0.0.1:1420 with sample data, no service needed.

Drive the service from a terminal with `lotus-service ctl status | lotus | shadow <link> | test <link> | verify <link> | connections | usage | diag | speed | logs | disconnect`.
</details>

## Project layout

```
crates/lotus-core      profiles, importers, validation, engine configuration, IPC types (no OS calls)
crates/lotus-service   privileged Windows service: engine supervisor, DNS, firewall, named-pipe IPC
app/                   Tauri 2 + React desktop app (unprivileged): tray, encrypted server store, signed updater
server/                exit-server installer (Debian, Ubuntu, Arch Linux)
scripts/               pinned engine fetch, release build, development install, failure tests
docs/                  documentation
```

## Documentation

| Guide | What it covers |
|---|---|
| [LOTUS vs SHADOW](docs/MODES.md) | What each mode protects, and what it doesn't |
| [Bring your own server](docs/BYO-SERVER.md) | The installer, existing servers, manual setup, choosing a camouflage site |
| [Protocols](docs/PROTOCOLS.md) | Research, choices and supported formats |
| [Architecture](docs/ARCHITECTURE.md) | Components, security boundaries, IPC, plans for macOS and Linux |
| [Privacy & security](docs/PRIVACY-SECURITY.md) | What LOTUS contacts, the threat model, limitations |
| [Testing](docs/TESTING.md) | Every test, how it was run, and the results |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | Common problems and fixes |

## License

LOTUS is free software under the [GNU General Public License v3.0 or later](LICENSE).

It builds on [sing-box](https://github.com/SagerNet/sing-box) (GPL-3.0-or-later), [Tauri](https://tauri.app), [React](https://react.dev), [shadcn/ui](https://ui.shadcn.com), [Motion](https://motion.dev) and the [sing-geosite](https://github.com/SagerNet/sing-geosite) rule-set. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
