# Privacy and security

## What LOTUS collects

Nothing. There is no account, no analytics, no telemetry, no crash reporting and no LOTUS server. The only background request is the update check described below, which you can turn off. LOTUS never records the sites you visit.

- **Usage history** (from 1.4.0; switch off or clear in **Settings → Usage**): per day, the time each mode was on and, for SHADOW, the data sent and received through each server. Only those totals are kept, never destinations or sites, in `%ProgramData%\LOTUS\usage.json`, which only SYSTEM and administrators can read. The app shows it on request; it never leaves the PC.
- **Engine logs** run at *warning* level, so per-connection destinations aren't logged. Lines are kept **in memory only** (last 300). Server addresses, UUIDs, passwords, keys and IP addresses are redacted before they're stored. Nothing is written to disk.
- **Network requests LOTUS itself makes:**
  - `https://1.1.1.1/cdn-cgi/trace` (and its IPv6 twin): when connecting, to learn your direct IP (leak reference) and the exit IP and country. In SHADOW the second request travels through your tunnel.
  - `https://1.1.1.1/cdn-cgi/trace`: fetched *by your server* for latency and health checks (every 30 s while SHADOW is connected). When connecting to a server for the first time, its IPv6 twin is fetched once to learn whether the server has IPv6.
  - The privacy check (only when you run it) queries `o-o.myaddr.l.google.com` (which reports the resolver's IP) and Team Cymru's DNS service (IP → network owner).
  - The speed test (only when you run it) uses `speed.cloudflare.com`.
  - Update checks (from 1.2.0; on by default, switch off in **Settings → Updates**): shortly after start and every 6 hours, LOTUS fetches `latest.json` from the latest GitHub release (`github.com`, served from GitHub's download host). The request carries nothing beyond what any HTTPS request reveals, such as your IP address. A newer installer is downloaded from the same release and checked against the LOTUS update key built into the app; it only runs when you choose **Restart to update** and approve the Windows prompt.
- **Live console** (SHADOW): while that screen is open, the app asks the service once a second for the engine's list of open connections (destination and byte counts) to draw it. The list lives in memory only and is never written to disk or logged. **Verify server** makes one handshake with your server, one ordinary HTTPS visit to its port (to show what a stranger sees) and one visit to the camouflage site for comparison.
- Your chosen DNS resolver sees your lookups (encrypted in transit), and in SHADOW your VPS provider sees where your traffic goes. Choose providers you trust.

## Security model

| Threat | Mitigation |
|---|---|
| ISP or Wi-Fi operator reading or modifying DNS | Encrypted DNS with full certificate validation; plaintext DNS to other servers blocked |
| Apps bypassing system DNS | WFP blocks port 53/853 to non-loopback (LOTUS); TUN DNS hijack plus strict routing (SHADOW) |
| Traffic leaking when the tunnel drops | Persistent WFP kill switch, armed before the engine starts, survives engine and service crashes (tested) |
| IPv6 leaks | IPv6 routed into the TUN (or rejected); verified by the privacy check |
| Claiming "connected" when it isn't | Connected only after a real handshake *and* a verified exit IP over OS routing |
| Local malware abusing the SYSTEM service | Closed IPC command set, no raw engine config, no file paths, strict validation, pipe DACL without instance creation |
| Tampered engine binary | Pinned SHA-256 checked before every start, with a write-deny lock held through process creation |
| Active probing of your server | REALITY relays unauthenticated probes to the real camouflage site |
| Stolen profiles file | DPAPI encryption bound to your Windows account |
| Disabled certificate checks | There is no "insecure" option anywhere; links that request it are refused with an explanation |
| Fake or tampered update | Installers are signed with the LOTUS update key (minisign) and verified against the public key built into the app before they run; fetched over HTTPS; installing needs your click and administrator approval |

**Never done:** installing root certificates, TLS interception, HTTPS inspection, running scripts from imported configs (WireGuard `PostUp` and similar are refused), downloading code at runtime (the one exception is a signed LOTUS update, which only installs when you choose to).

## Limitations, stated plainly

- LOTUS mode doesn't hide your IP or encrypt non-DNS traffic.
- Obfuscation raises the cost of detection; it doesn't guarantee it. Traffic analysis (volume, timing, a single long-lived destination) can still reveal tunnel use.
- The service runs as LocalSystem because WFP, TUN adapters and adapter DNS require it. Its surface is kept small (above). Running under a restricted service SID with only the needed privileges is planned hardening.
- With "Allow local network" enabled, the kill switch still permits LAN destinations. Disable it on untrusted networks if that matters to you.
