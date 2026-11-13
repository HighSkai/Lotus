# Troubleshooting

Start with **Settings → Privacy check**. Every line explains what it measured and why it matters.

### "The LOTUS service isn't running"
Use **Repair service** (on the main screen, or Settings → Advanced). It re-registers the service and needs administrator approval. From an elevated terminal: `"C:\Program Files\LOTUS\lotus-service.exe" install`.

### No internet after a crash, or after uninstalling without the uninstaller
From an elevated terminal:
```
"C:\Program Files\LOTUS\lotus-service.exe" restore-network
```
It restores saved DNS settings and removes the kill-switch filters. (Stop the service first if it's running: `sc stop LotusService`.)

### LOTUS: "Another program is already using the DNS port (53)"
Another DNS proxy, ad blocker or VPN is listening on port 53. Close it, or turn off its DNS feature.

### LOTUS: "The encrypted DNS server couldn't be reached"
Your network may block DNS-over-HTTPS to that resolver. Try another provider, or switch the protocol (HTTPS ↔ TLS) in DNS & Privacy. LOTUS never falls back to plaintext DNS.

### SHADOW: "The server rejected the REALITY handshake"
The public key, short ID or camouflage SNI doesn't match the server. Re-import the link from `--show` on the server. Also check that your PC's clock is correct (REALITY allows a ±2 min difference).

### SHADOW: "The server didn't respond in time"
- Check that the server is running: `systemctl status lotus-sing-box`.
- Open the port in your VPS provider's **cloud firewall / security group** (the installer can only change the server's own firewall).
- For Hysteria2, make sure **UDP** is allowed.

### SHADOW: "Traffic was still leaving through your normal connection"
Another VPN or filter driver is overriding routes. Disconnect other VPNs and retry.

### SHADOW: no internet while "Reconnecting…"
That's the kill switch protecting you while the tunnel is down. Turn SHADOW off to restore normal access, or turn off the kill switch in Settings.

### Some apps or games fail in SHADOW
- UDP-heavy apps work best with Hysteria2 or WireGuard.
- Strict routing can break VirtualBox/Hyper-V host-only networking; "Allow local network" covers most LAN needs.

### Where are things?
| What | Where |
|---|---|
| App and service | `C:\Program Files\LOTUS\` |
| Your servers (encrypted) and settings | `%LOCALAPPDATA%\app.lotus.desktop\` |
| Service state (DNS backup, encrypted resume record) | `%ProgramData%\LOTUS\` (administrators only) |
| Engine log | Settings → Advanced → Engine log (memory only) |
| Command-line control | `lotus-service ctl status`, `ctl diag`, `ctl logs` |
