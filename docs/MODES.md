# LOTUS and SHADOW

## LOTUS mode: private DNS, same IP

Every website visit starts with a DNS lookup ("where is example.com?"). Normally those lookups go unencrypted to your internet provider, who can log and sell them, and anyone on the path can read or tamper with them.

When LOTUS is on:

- a local resolver (the sing-box engine, listening on `127.0.0.1:53` and `[::1]:53`) answers every lookup;
- it forwards each one **encrypted** to the resolver you choose (Cloudflare, Quad9, Mullvad, AdGuard, Google or your own) over DNS-over-HTTPS, DoH over HTTP/3, or DNS-over-TLS. Certificates are verified normally;
- every active network adapter is pointed at that local resolver. Your original settings are saved first and restored when you turn LOTUS off, if the app or service crashes (on next start), or on uninstall;
- **leak protection**: Windows Filtering Platform rules block plaintext DNS (port 53) and DoT (853) to any other server, so programs with hard-coded resolvers can't bypass it;
- optional **tracker blocking** (on-device list) and **malicious-domain filtering** (resolver-side).

What LOTUS mode does *not* do:

- **It doesn't change your IP address.** Websites, and your ISP, still see your real address. Your ISP can still see *which IP addresses* you connect to and, for most sites, the server name in the TLS handshake (SNI).
- It doesn't encrypt your other traffic. Your HTTPS connections remain end-to-end encrypted as always; LOTUS doesn't touch them.

Encrypting all traffic against your ISP while keeping your own public IP is not possible: hiding traffic from the ISP requires sending it through a relay, and the relay's address then becomes your public IP. That's what SHADOW is for.

## SHADOW mode: full-device tunnel through your own server

SHADOW sends **all** traffic from the device (every app, TCP and UDP, IPv4 and IPv6) through an encrypted tunnel to a server you control. Websites see your server's IP instead of yours. Your ISP sees only an encrypted connection to your server.

- **Bring Your Own Server**: no LOTUS account, no central infrastructure. Your VPS is the exit.
- **Real routing**: a TUN adapter (*LOTUS Shadow*) takes the default routes for IPv4 and IPv6 (strict routing), so nothing slips past it.
- **DNS through the tunnel**: queries are captured and sent as DNS-over-HTTPS *through your server*, so neither your ISP nor your VPS provider can read them.
- **Kill switch** (on by default): persistent firewall rules allow only the tunnel. If the engine crashes, the network changes, or even the LOTUS service itself dies, traffic is blocked rather than leaked, and LOTUS reconnects. Local network access can be allowed.
- **Verified before "Connected"**: LOTUS shows *Connected* only after (1) a real handshake with your server succeeded and (2) an HTTPS request made through Windows' normal routing came back from the exit IP. If traffic would leave outside the tunnel, it refuses to report success.

What SHADOW does *not* do:

- Your VPS provider can see your traffic's destinations (as your ISP otherwise would). Choose a provider you trust.
- It's not anonymity in the Tor sense: your server's IP is stable and tied to your VPS account.
- It's not "undetectable". See [PROTOCOLS.md](PROTOCOLS.md).

## Switching modes

The switcher at the top moves protection between modes. If LOTUS is on and you switch to SHADOW (with a server selected), LOTUS hands over to SHADOW. Without a server it simply turns off and asks you to add one.
