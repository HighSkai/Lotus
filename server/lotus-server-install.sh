#!/usr/bin/env bash
# =============================================================================
#  LOTUS exit-server installer for Debian / Ubuntu
#
#  Installs sing-box and configures:
#    * VLESS + REALITY + XTLS-Vision on TCP (default 443): no domain or
#      certificate needed; unauthenticated visitors see a real website.
#    * optional Hysteria2 on UDP (--with-hysteria2): QUIC, good on lossy links,
#      self-signed ECDSA certificate that the client pins (no "insecure").
#
#  Safe by design:
#    * the sing-box archive is verified against a pinned SHA-256 digest;
#    * firewall rules are only *added* (ufw/firewalld), after asking; other
#      firewalls are never modified, you get the exact command instead;
#    * everything lives in /opt/lotus, /etc/lotus, /var/lib/lotus and one
#      systemd unit, and `--uninstall` removes all of it.
#
#  Usage:  sudo bash lotus-server-install.sh [options]
#    --port N             TCP port for REALITY (default 443)
#    --sni HOST           camouflage site (default: auto-select)
#    --with-hysteria2     also enable Hysteria2 on UDP
#    --hy2-port N         UDP port for Hysteria2 (default: same as --port)
#    --name NAME          profile name shown in LOTUS (default: hostname)
#    --tune               enable BBR and larger UDP buffers (sysctl drop-in)
#    --yes                don't ask questions (firewall changes are still additive)
#    --show               print the client link of an existing install
#    --uninstall          remove everything this script installed
# =============================================================================
set -euo pipefail

SB_VERSION="1.14.4"
SB_SHA256_amd64="699a1b510aa4396b2176d32d1181920399ebf1e267835da6f3b598d0f7c24968"
SB_SHA256_arm64="436441f76ee2f7fe21b5b2d426d3cb261a480455a704865789ad533e4273ec26"

PREFIX=/opt/lotus
ETC=/etc/lotus
STATE=/var/lib/lotus
UNIT=/etc/systemd/system/lotus-sing-box.service
SYSCTL=/etc/sysctl.d/90-lotus.conf
SVC_USER=lotus-sb
HY2_SNI="lotus.internal"
SNI_CANDIDATES=(www.microsoft.com www.apple.com www.amazon.com www.nvidia.com www.samsung.com www.cisco.com www.oracle.com www.mozilla.org)

PORT=443
HY2_PORT=""
SNI=""
WITH_HY2=0
NAME=""
TUNE=0
YES=0
ACTION=install

c_ok=$'\e[32m'; c_warn=$'\e[33m'; c_err=$'\e[31m'; c_dim=$'\e[2m'; c_b=$'\e[1m'; c_0=$'\e[0m'
[ -t 1 ] || { c_ok=; c_warn=; c_err=; c_dim=; c_b=; c_0=; }
say()  { printf '%s\n' "$*"; }
ok()   { printf '%s✓%s %s\n' "$c_ok" "$c_0" "$*"; }
warn() { printf '%s!%s %s\n' "$c_warn" "$c_0" "$*" >&2; }
die()  { printf '%s✗ %s%s\n' "$c_err" "$*" "$c_0" >&2; exit 1; }
ask()  { # ask "question" -> 0 for yes
  [ "$YES" = 1 ] && return 0
  local a; read -r -p "$1 [Y/n] " a </dev/tty || return 1
  [[ -z "$a" || "$a" =~ ^[Yy] ]]
}

while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT="${2:?}"; shift 2 ;;
    --sni) SNI="${2:?}"; shift 2 ;;
    --with-hysteria2) WITH_HY2=1; shift ;;
    --hy2-port) HY2_PORT="${2:?}"; shift 2 ;;
    --name) NAME="${2:?}"; shift 2 ;;
    --tune) TUNE=1; shift ;;
    --yes|-y) YES=1; shift ;;
    --show) ACTION=show; shift ;;
    --uninstall) ACTION=uninstall; shift ;;
    -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
    *) die "Unknown option: $1 (see --help)" ;;
  esac
done
HY2_PORT="${HY2_PORT:-$PORT}"

[ "$(id -u)" = 0 ] || die "Run as root (sudo bash $0 …)"

valid_port() { [[ "$1" =~ ^[0-9]+$ ]] && [ "$1" -ge 1 ] && [ "$1" -le 65535 ]; }
valid_port "$PORT" || die "Invalid --port"
valid_port "$HY2_PORT" || die "Invalid --hy2-port"
[ -z "$SNI" ] || [[ "$SNI" =~ ^[A-Za-z0-9.-]+$ ]] || die "Invalid --sni"

# ----------------------------------------------------------------------------- show / uninstall
if [ "$ACTION" = show ]; then
  [ -f "$ETC/client.txt" ] || die "No LOTUS server installation found."
  cat "$ETC/client.txt"; exit 0
fi

if [ "$ACTION" = uninstall ]; then
  say "${c_b}Removing the LOTUS exit server…${c_0}"
  systemctl disable --now lotus-sing-box.service 2>/dev/null || true
  rm -f "$UNIT"; systemctl daemon-reload || true
  if [ -f "$ETC/firewall.log" ]; then
    while read -r tool rule; do
      case "$tool" in
        ufw) ufw --force delete allow "$rule" >/dev/null 2>&1 && ok "ufw: removed allow $rule" || true ;;
        firewalld) firewall-cmd --permanent --remove-port="$rule" >/dev/null 2>&1 && ok "firewalld: removed $rule" || true ;;
      esac
    done < "$ETC/firewall.log"
    command -v firewall-cmd >/dev/null && firewall-cmd --reload >/dev/null 2>&1 || true
  fi
  if [ -f "$SYSCTL" ]; then rm -f "$SYSCTL"; sysctl --system >/dev/null 2>&1 || true; ok "Removed network tuning"; fi
  rm -rf "$PREFIX" "$ETC" "$STATE"
  id "$SVC_USER" >/dev/null 2>&1 && userdel "$SVC_USER" 2>/dev/null || true
  ok "Uninstalled. Nothing from LOTUS remains on this server."
  exit 0
fi

# ----------------------------------------------------------------------------- preflight
. /etc/os-release 2>/dev/null || die "Cannot detect the operating system"
case "${ID:-}:${ID_LIKE:-}" in
  debian:*|ubuntu:*|*:*debian*) ;;
  *) die "This installer supports Debian and Ubuntu (found ${PRETTY_NAME:-unknown})." ;;
esac
case "$(uname -m)" in
  x86_64|amd64) ARCH=amd64; SB_SHA256="$SB_SHA256_amd64" ;;
  aarch64|arm64) ARCH=arm64; SB_SHA256="$SB_SHA256_arm64" ;;
  *) die "Unsupported CPU architecture $(uname -m)" ;;
esac
command -v systemctl >/dev/null || die "systemd is required"

if [ -f "$ETC/config.json" ]; then
  warn "LOTUS is already installed here."
  ask "Replace it with new keys? Existing client links will stop working." || { say "Keeping the current install. Use --show to print its link."; exit 0; }
  systemctl stop lotus-sing-box.service 2>/dev/null || true
fi

port_busy() { ss -H -ltn "sport = :$1" 2>/dev/null | grep -q . ; }
uport_busy() { ss -H -lun "sport = :$1" 2>/dev/null | grep -q . ; }
port_busy "$PORT" && die "TCP port $PORT is already in use ($(ss -H -ltnp "sport = :$PORT" | awk '{print $NF}' | head -1)). Choose another with --port."
if [ "$WITH_HY2" = 1 ] && uport_busy "$HY2_PORT"; then die "UDP port $HY2_PORT is already in use. Choose another with --hy2-port."; fi

say "${c_b}Installing dependencies…${c_0}"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends curl ca-certificates openssl tar iproute2 >/dev/null
ok "Dependencies ready"

# ----------------------------------------------------------------------------- sing-box
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
URL="https://github.com/SagerNet/sing-box/releases/download/v${SB_VERSION}/sing-box-${SB_VERSION}-linux-${ARCH}.tar.gz"
say "${c_b}Downloading sing-box ${SB_VERSION} (${ARCH})…${c_0}"
curl -fsSL --proto '=https' --tlsv1.2 -o "$TMP/sb.tgz" "$URL" || die "Download failed"
echo "${SB_SHA256}  $TMP/sb.tgz" | sha256sum -c --quiet - || die "Checksum mismatch: the download was not the expected sing-box release. Nothing was installed."
ok "Archive verified (SHA-256)"
tar -xzf "$TMP/sb.tgz" -C "$TMP"
install -d -m 0755 "$PREFIX"
install -m 0755 "$TMP/sing-box-${SB_VERSION}-linux-${ARCH}/sing-box" "$PREFIX/sing-box"
SB="$PREFIX/sing-box"
"$SB" version | head -1

# ----------------------------------------------------------------------------- camouflage site
pick_sni() {
  local best="" best_ms=999999 h t0 t1 ms
  for h in "${SNI_CANDIDATES[@]}"; do
    t0=$(date +%s%N)
    if timeout 6 openssl s_client -connect "$h:443" -servername "$h" -tls1_3 -alpn h2 </dev/null 2>/dev/null | grep -q "ALPN protocol: h2"; then
      t1=$(date +%s%N); ms=$(( (t1 - t0) / 1000000 ))
      say "  ${c_dim}$h: TLS 1.3 + HTTP/2, ${ms} ms${c_0}" >&2
      if [ "$ms" -lt "$best_ms" ]; then best="$h"; best_ms="$ms"; fi
    else
      say "  ${c_dim}$h: not suitable${c_0}" >&2
    fi
  done
  [ -n "$best" ] || return 1
  echo "$best"
}
if [ -z "$SNI" ]; then
  say "${c_b}Choosing a camouflage site (TLS 1.3 + HTTP/2, closest wins)…${c_0}"
  SNI="$(pick_sni)" || die "No candidate site was reachable. Pass one with --sni."
else
  timeout 6 openssl s_client -connect "$SNI:443" -servername "$SNI" -tls1_3 </dev/null >/dev/null 2>&1 || die "$SNI doesn't accept TLS 1.3 connections from this server."
fi
ok "Camouflage site: $SNI"

# ----------------------------------------------------------------------------- credentials
install -d -m 0750 "$ETC" "$STATE"
id "$SVC_USER" >/dev/null 2>&1 || useradd --system --no-create-home --home-dir "$STATE" --shell /usr/sbin/nologin "$SVC_USER"
chown root:"$SVC_USER" "$ETC"; chown "$SVC_USER":"$SVC_USER" "$STATE"

UUID="$("$SB" generate uuid)"
KP="$("$SB" generate reality-keypair)"
PRIV="$(awk '/PrivateKey/ {print $2}' <<<"$KP")"
PUB="$(awk '/PublicKey/ {print $2}' <<<"$KP")"
SID="$("$SB" generate rand 8 --hex)"
[ -n "$PRIV" ] && [ -n "$PUB" ] || die "Key generation failed"

HY2_JSON=""
if [ "$WITH_HY2" = 1 ]; then
  HY2_PW="$("$SB" generate rand 24 --base64 | tr -d '=+/' )"
  openssl ecparam -name prime256v1 -genkey -noout -out "$ETC/hy2.key" 2>/dev/null
  openssl req -new -x509 -key "$ETC/hy2.key" -out "$ETC/hy2.crt" -days 3650 -subj "/CN=$HY2_SNI" \
    -addext "subjectAltName=DNS:$HY2_SNI" -addext "basicConstraints=critical,CA:FALSE" \
    -addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=serverAuth" 2>/dev/null
  chown root:"$SVC_USER" "$ETC/hy2.key" "$ETC/hy2.crt"; chmod 0640 "$ETC/hy2.key"; chmod 0644 "$ETC/hy2.crt"
  HY2_PIN="$(openssl x509 -in "$ETC/hy2.crt" -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -binary | base64)"
  HY2_JSON=$(cat <<EOF
,
    {
      "type": "hysteria2", "tag": "hy2-in", "listen": "::", "listen_port": $HY2_PORT,
      "users": [{ "password": "$HY2_PW" }],
      "tls": { "enabled": true, "alpn": ["h3"], "certificate_path": "$ETC/hy2.crt", "key_path": "$ETC/hy2.key" }
    }
EOF
)
fi

cat > "$ETC/config.json" <<EOF
{
  "log": { "level": "warn", "timestamp": true },
  "inbounds": [
    {
      "type": "vless", "tag": "reality-in", "listen": "::", "listen_port": $PORT,
      "users": [{ "uuid": "$UUID", "flow": "xtls-rprx-vision" }],
      "tls": {
        "enabled": true,
        "server_name": "$SNI",
        "reality": {
          "enabled": true,
          "handshake": { "server": "$SNI", "server_port": 443 },
          "private_key": "$PRIV",
          "short_id": ["$SID"],
          "max_time_difference": "2m"
        }
      }
    }$HY2_JSON
  ],
  "outbounds": [{ "type": "direct", "tag": "direct" }],
  "route": {
    "rules": [
      { "ip_is_private": true, "action": "reject" }
    ],
    "final": "direct"
  }
}
EOF
chown root:"$SVC_USER" "$ETC/config.json"; chmod 0640 "$ETC/config.json"
"$SB" check -c "$ETC/config.json" || die "Generated configuration failed validation"
ok "Configuration written and validated"

# ----------------------------------------------------------------------------- systemd
cat > "$UNIT" <<EOF
[Unit]
Description=LOTUS exit server (sing-box)
Documentation=https://sing-box.sagernet.org
After=network-online.target
Wants=network-online.target

[Service]
User=$SVC_USER
Group=$SVC_USER
ExecStart=$SB run -c $ETC/config.json -D $STATE
Restart=on-failure
RestartSec=3
LimitNOFILE=1048576
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
PrivateDevices=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictNamespaces=true
LockPersonality=true
ReadWritePaths=$STATE

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
ok "systemd unit installed"

# ----------------------------------------------------------------------------- tuning (optional)
if [ "$TUNE" = 1 ] || { [ "$YES" = 0 ] && ask "Enable BBR congestion control and larger UDP buffers? (recommended, reversible)"; }; then
  cat > "$SYSCTL" <<EOF
# Added by the LOTUS installer; removed by --uninstall.
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
EOF
  sysctl --system >/dev/null 2>&1 && ok "Network tuning applied" || warn "Couldn't apply network tuning (kernel may lack BBR)"
fi

# ----------------------------------------------------------------------------- firewall (additive only)
: > "$ETC/firewall.log"
open_port() { # proto port
  local proto="$1" p="$2"
  if command -v ufw >/dev/null && ufw status 2>/dev/null | grep -q "Status: active"; then
    if ask "ufw is active. Allow $p/$proto through it?"; then
      ufw allow "$p/$proto" >/dev/null && echo "ufw $p/$proto" >> "$ETC/firewall.log" && ok "ufw: allowed $p/$proto"
    else warn "Not changing ufw. Run: ufw allow $p/$proto"; fi
  elif command -v firewall-cmd >/dev/null && firewall-cmd --state >/dev/null 2>&1; then
    if ask "firewalld is running. Open $p/$proto permanently?"; then
      firewall-cmd --permanent --add-port="$p/$proto" >/dev/null && firewall-cmd --reload >/dev/null && echo "firewalld $p/$proto" >> "$ETC/firewall.log" && ok "firewalld: opened $p/$proto"
    else warn "Not changing firewalld. Run: firewall-cmd --permanent --add-port=$p/$proto && firewall-cmd --reload"; fi
  elif command -v nft >/dev/null && nft list ruleset 2>/dev/null | grep -qE "hook input.*policy drop"; then
    warn "An nftables input policy drops traffic. LOTUS won't edit custom rulesets. Allow $proto port $p yourself."
  elif command -v iptables >/dev/null && iptables -S INPUT 2>/dev/null | grep -q -- "-P INPUT DROP"; then
    warn "iptables INPUT policy is DROP. LOTUS won't edit it. Allow $proto port $p yourself, e.g. iptables -I INPUT -p $proto --dport $p -j ACCEPT"
  fi
}
open_port tcp "$PORT"
[ "$WITH_HY2" = 1 ] && open_port udp "$HY2_PORT"
say "${c_dim}Also allow these ports in your VPS provider's cloud firewall/security group if it has one.${c_0}"

# ----------------------------------------------------------------------------- start + validate
systemctl enable --now lotus-sing-box.service >/dev/null 2>&1
sleep 1.5
systemctl is-active --quiet lotus-sing-box.service || { journalctl -u lotus-sing-box --no-pager -n 30; die "Service failed to start"; }
port_busy "$PORT" || die "sing-box isn't listening on TCP $PORT"
ok "Service running"

say "${c_b}Validating with a real client handshake…${c_0}"
TPORT=$(( 20000 + RANDOM % 20000 ))
cat > "$TMP/client.json" <<EOF
{
  "log": { "level": "error" },
  "inbounds": [{ "type": "mixed", "listen": "127.0.0.1", "listen_port": $TPORT }],
  "outbounds": [{
    "type": "vless", "tag": "proxy", "server": "127.0.0.1", "server_port": $PORT,
    "uuid": "$UUID", "flow": "xtls-rprx-vision",
    "tls": { "enabled": true, "server_name": "$SNI", "utls": { "enabled": true, "fingerprint": "chrome" },
             "reality": { "enabled": true, "public_key": "$PUB", "short_id": "$SID" } }
  }]
}
EOF
"$SB" run -c "$TMP/client.json" >/dev/null 2>&1 & CPID=$!
sleep 1
if TRACE="$(curl -fsS --max-time 10 -x "socks5h://127.0.0.1:$TPORT" https://1.1.1.1/cdn-cgi/trace 2>/dev/null)"; then
  ok "REALITY handshake and forwarding work"
else
  kill "$CPID" 2>/dev/null || true
  die "Local REALITY test failed. Check: journalctl -u lotus-sing-box"
fi
kill "$CPID" 2>/dev/null || true; wait "$CPID" 2>/dev/null || true

# ----------------------------------------------------------------------------- client link
IP4="$(curl -4 -fsS --max-time 6 https://1.1.1.1/cdn-cgi/trace 2>/dev/null | awk -F= '/^ip=/{print $2}')" || true
IP6="$(curl -6 -fsS --max-time 6 'https://[2606:4700:4700::1111]/cdn-cgi/trace' 2>/dev/null | awk -F= '/^ip=/{print $2}')" || true
ADDR="${IP4:-$IP6}"
[ -n "$ADDR" ] || die "Couldn't determine this server's public IP"
NAME="${NAME:-$(hostname -s)}"
json_str() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'; }
NJ="$(json_str "$NAME")"

PROFILES="{\"name\":\"$NJ · REALITY\",\"protocol\":{\"type\":\"vless\",\"server\":\"$ADDR\",\"port\":$PORT,\"uuid\":\"$UUID\",\"flow\":\"xtls-rprx-vision\",\"tls\":{\"server_name\":\"$SNI\",\"fingerprint\":\"chrome\",\"reality\":{\"public_key\":\"$PUB\",\"short_id\":\"$SID\"}},\"transport\":{\"type\":\"tcp\"}}}"
if [ "$WITH_HY2" = 1 ]; then
  # The client trusts exactly this server key (SPKI SHA-256 pin) and nothing else.
  PROFILES="$PROFILES,{\"name\":\"$NJ · Hysteria2\",\"protocol\":{\"type\":\"hysteria2\",\"server\":\"$ADDR\",\"port\":$HY2_PORT,\"password\":\"$HY2_PW\",\"tls\":{\"server_name\":\"$HY2_SNI\",\"alpn\":[\"h3\"],\"public_key_sha256\":[\"$HY2_PIN\"]}}}"
fi
BUNDLE="{\"v\":1,\"profiles\":[$PROFILES]}"
LOTUS_LINK="lotus://import/$(printf '%s' "$BUNDLE" | base64 -w0 | tr '+/' '-_' | tr -d '=')"
HOSTPART="$ADDR"; [[ "$ADDR" == *:* ]] && HOSTPART="[$ADDR]"
VLESS_LINK="vless://$UUID@$HOSTPART:$PORT?encryption=none&flow=xtls-rprx-vision&security=reality&sni=$SNI&fp=chrome&pbk=$PUB&sid=$SID&type=tcp#$(printf '%s' "$NAME" | sed 's/ /%20/g')"

{
  echo "LOTUS exit server: $NAME ($ADDR)"
  echo
  echo "Import into LOTUS (copy the whole line):"
  echo "$LOTUS_LINK"
  echo
  echo "Standard VLESS link for other apps:"
  echo "$VLESS_LINK"
} > "$ETC/client.txt"
chmod 0600 "$ETC/client.txt"

say
say "${c_b}${c_ok}Done.${c_0} Your exit IP: ${c_b}$ADDR${c_0}${IP6:+ (IPv6: $IP6)}"
say
say "${c_b}Copy this line into LOTUS → Servers → Add → Paste:${c_0}"
say "$LOTUS_LINK"
say
say "${c_dim}Standard link for other clients:${c_0}"
say "$VLESS_LINK"
say
say "${c_dim}These links contain your server's secret keys; treat them like a password."
say "Show again: sudo bash $0 --show   ·   Remove: sudo bash $0 --uninstall${c_0}"
