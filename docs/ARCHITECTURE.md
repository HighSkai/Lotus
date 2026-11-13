# Architecture

```
┌──────────────────────────── user session (unprivileged) ───────────────────────────┐
│  lotus.exe  (Tauri 2 host, Rust)                                                  │
│   ├─ WebView2 UI (React + TypeScript): only calls typed LOTUS commands            │
│   ├─ profile store: %LOCALAPPDATA%\app.lotus.desktop\profiles.bin (DPAPI, user)   │
│   ├─ settings.json, tray icon, autostart, single instance                         │
│   └─ service client ── named pipe \\.\pipe\lotus-service (NDJSON, closed commands)│
└───────────────────────────────────────────┬─────────────────────────────────────────┘
                                            │ ACL: SYSTEM, Administrators, INTERACTIVE (read/write data only)
┌──────────────────────────── LocalSystem service ───────────────────────────────────┐
│  lotus-service.exe                                                                │
│   ├─ controller: one session at a time (LOTUS or SHADOW), status broadcast        │
│   ├─ engine supervisor: SHA-256-verified sing-box.exe, config via stdin,          │
│   │   job object (dies with the service), log redaction, local controller API     │
│   ├─ LOTUS: adapter DNS → 127.0.0.1/::1 (backed up first), dynamic WFP DNS guard  │
│   ├─ SHADOW: TUN via engine, persistent WFP kill switch, health/latency monitor   │
│   ├─ network + power events → re-apply DNS / re-check / reconnect                 │
│   ├─ state: %ProgramData%\LOTUS (SYSTEM+Admins only) state.json, session.bin(DPAPI)│
│   └─ diagnostics + on-demand speed test                                           │
└───────────────────────────────────────────┬─────────────────────────────────────────┘
                                            │ stdin (config), stdout/stderr (logs), 127.0.0.1:<random> API + secret
                               sing-box.exe (engine, child process)
```

## Crates and packages

| Path | Role | Platform |
|---|---|---|
| `crates/lotus-core` | Data model, importers, validation, sing-box config builders, IPC types, DNS wire format, log redaction | Any (no OS calls) |
| `crates/lotus-service` | Privileged service. `win/` holds every unsafe Windows call: WFP, IP Helper, DPAPI, ACLs, job objects, named pipes, SCM | Windows today |
| `app/src-tauri` | UI host: IPC client, tray, profile vault, commands | Windows today; Tauri targets macOS/Linux |
| `app/src` | React UI (no runtime dependencies besides React and the Tauri API) | Any |
| `server/` | Exit-server installer | Debian, Ubuntu, Arch Linux (and derivatives) |

## Security boundaries

1. **Web view → host.** The web view gets only window controls and LOTUS commands (`capabilities/main.json`). No shell, filesystem, HTTP or process plugins are exposed. Strict CSP, frozen prototypes, no remote content.
2. **App → service.** The pipe accepts a closed set of typed commands (`lotus_core::ipc::Command`). There's no "run engine with this JSON": the service builds every engine configuration itself from validated types, and the builders never emit file outputs, external UIs, cache files or certificate paths. A local process therefore can't use the SYSTEM service to read or write files or to run programs. Profiles are validated again in the service. Frames are capped at 512 KiB. The pipe DACL grants interactive users read/write-data only (not *create pipe instance*, which prevents squatting), it rejects remote clients, and clients connect with identification-level impersonation only.
3. **Service → engine.** The engine binary must sit next to the service (Program Files, admin-writable only) and match the SHA-256 compiled into the service. A write-deny handle is held from the hash check until the process is created. The configuration travels over stdin and never touches disk. The engine's controller API listens on loopback with a random port and a random secret per run. The engine runs in a kill-on-close job object.
4. **Data at rest.** Server profiles are DPAPI-encrypted for the Windows user. The resume record is DPAPI-encrypted for SYSTEM. `%ProgramData%\LOTUS` gets a protected DACL (SYSTEM + Administrators only).

## Connection lifecycles

**LOTUS**: start engine (DNS listeners on loopback) → prove an encrypted upstream answers → enable the WFP DNS guard → back up and set adapter DNS → flush the cache → *Connected*. Every 10 s and on network changes, new adapters are covered. If the engine dies it restarts (backoff); after 3 failures the original DNS is restored so the user stays online, with a clear message.

**SHADOW**: measure the direct IP (leak reference) → arm the kill switch (persistent WFP) → start the engine (TUN, strict route, DNS hijack) → real handshake through the proxy (delay test) → HTTPS trace through *OS routing* must return the exit IP, not the reference → *Connected*. Then: a latency probe every 30 s, a check after network changes, a reconnect on resume from sleep, and immediate detection of engine exit. Reconnect uses backoff 1→30 s while the kill switch holds. If the first attempt fails, the kill switch is released and an error shown (no lock-out on a bad config).

**Crash and reboot**: the session record (DPAPI) lets a restarted service resume. Kill-switch filters are persistent, so traffic stays blocked while the SCM restarts the service (1 s), and they're re-armed atomically. On a service start with nothing to resume, leftover filters and DNS overrides are removed. `lotus-service restore-network` and uninstall do the same.

## IPC protocol

Newline-delimited JSON over the pipe. Client → `{"id":n,"cmd":"connect_shadow","profile":{…},"options":{…}}`. Service → `{"type":"response","id":n,"result":…}` or `{"type":"status","status":{…}}` pushes (state, mode, exit IP/country, latency, traffic, protections, kill-switch state, error). See `crates/lotus-core/src/ipc.rs`.

## Adding macOS and Linux

Nothing above the `win/` module is Windows-specific. Porting means:

- **Service**: a launchd daemon (macOS) or systemd unit (Linux). The pipe becomes a Unix socket with peer-credential checks. DNS uses `scutil`/NetworkExtension (macOS) or systemd-resolved over D-Bus (Linux). The kill switch uses `pf` anchors (macOS) or nftables tables (Linux), the same persistent-rules design. The sing-box TUN already supports both (`auto_redirect` on Linux).
- **App**: Tauri builds as-is. The profile vault swaps DPAPI for Keychain or Secret Service behind the same `Store` interface.
- **Engine**: official sing-box builds for both, pinned in the same way.
