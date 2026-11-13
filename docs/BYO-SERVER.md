# Bring Your Own Server

SHADOW tunnels your device through a server **you** control. Websites see that server's IP. You need a VPS (or any Linux box with a public IP) and about five minutes.

## Option A: the LOTUS installer (recommended)

The script lives at `server/lotus-server-install.sh`. In the app, use **Servers → Add → Set up my own server → Save install script…**

1. **Get a VPS**: Debian 12/13 or Ubuntu 22.04/24.04, x86-64 or ARM64, 512 MB RAM or more, in the country you want your exit IP to be in.
2. **Run it on the server.** The quickest way is to download it straight from the release while logged in to the VPS:
   ```
   curl -fsSLO https://github.com/HighSkai/Lotus/releases/download/v1.0.0/lotus-server-install.sh
   sudo bash lotus-server-install.sh
   ```
   Or upload the copy saved from the app (from PowerShell on your PC; Windows 10/11 include `ssh` and `scp`):
   ```
   scp lotus-server-install.sh root@YOUR_SERVER_IP:
   ssh root@YOUR_SERVER_IP "bash lotus-server-install.sh"
   ```
   Add `--with-hysteria2` for a second, UDP-based profile. Add `--tune` to enable BBR without being asked.
3. **Copy the `lotus://…` line** it prints at the end, then in LOTUS: **Add server → From clipboard**.

### What it does

- Verifies the sing-box 1.14.4 archive against a pinned SHA-256 digest before installing anything (`/opt/lotus/sing-box`).
- Picks a camouflage site that supports TLS 1.3 + HTTP/2 and answers fastest from your server (override with `--sni host`).
- Generates a REALITY X25519 key pair, a UUID and a short ID (and for Hysteria2 a password plus a self-signed ECDSA certificate that the client pins).
- Writes `/etc/lotus/config.json` (readable only by root and the service user). It rejects tunnel traffic to the server's own private and loopback addresses, so clients can't reach services bound to localhost.
- Installs a hardened systemd unit `lotus-sing-box.service`: dedicated `lotus-sb` user, only `CAP_NET_BIND_SERVICE`, read-only system, no home access.
- **Firewall, additive only**: if `ufw` or `firewalld` is active it *asks* before adding an allow rule. Any other firewall (custom nftables/iptables) is left untouched and you get the exact command to run. Cloud firewalls (security groups) must be opened in your provider's panel.
- **Validates for real**: starts a client on the server, performs a REALITY handshake against itself, and fetches a page through it.
- Prints the `lotus://` link and a standard `vless://` link for other apps. `--show` prints them again.

### Remove

```
sudo bash lotus-server-install.sh --uninstall
```

This stops and deletes the service, its user, `/opt/lotus`, `/etc/lotus`, `/var/lib/lotus`, the sysctl tuning file, and exactly the firewall rules it added.

## Option B: an existing server

Already run Xray, sing-box, 3x-ui, Marzban, Hysteria or similar? Just paste its share link (`vless://`, `trojan://`, `ss://`, `hysteria2://`, `tuic://`, `anytls://`, `naive+https://`), a WireGuard `.conf`, or a sing-box JSON outbound. LOTUS validates it and tells you exactly why if something is unsupported or insecure.

## Option C: manual

**Servers → Add → Enter details manually** offers forms for every supported protocol. A minimal REALITY server config, if you write your own:

```json
{
  "inbounds": [{
    "type": "vless", "listen": "::", "listen_port": 443,
    "users": [{ "uuid": "<uuid>", "flow": "xtls-rprx-vision" }],
    "tls": { "enabled": true, "server_name": "<site>",
      "reality": { "enabled": true, "handshake": { "server": "<site>", "server_port": 443 },
                   "private_key": "<private key>", "short_id": ["<hex>"] } }
  }],
  "outbounds": [{ "type": "direct" }]
}
```

Generate keys with `sing-box generate reality-keypair`, `sing-box generate uuid`, `sing-box generate rand 8 --hex`.

## Choosing a camouflage site (SNI)

Good choices support TLS 1.3 and HTTP/2, are not behind Cloudflare, don't redirect, and are ideally hosted near your VPS (the same provider or network is best). An observer comparing the SNI with who owns your VPS's IP is the main residual risk of REALITY, so a plausible pairing helps.

## Is a domain or certificate required?

No, for REALITY and the installer's Hysteria2 setup. Yes, for Trojan, VLESS+TLS, NaiveProxy and AnyTLS with normal TLS (or provide the server's certificate or a key pin in Advanced).
