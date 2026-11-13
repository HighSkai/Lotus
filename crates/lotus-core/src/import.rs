//! Importers for the connection formats users commonly receive from server
//! panels and setup scripts.
//!
//! Supported:
//! * `vless://`, `trojan://`, `ss://` (SIP002 and legacy), `hysteria2://` / `hy2://`,
//!   `tuic://`, `anytls://`, `naive+https://` / `naive+quic://`, `wireguard://`
//! * `lotus://` bundles produced by the LOTUS server installer
//! * WireGuard `.conf` files
//! * sing-box JSON (whole configuration, a single outbound, or an array)
//! * base64-encoded subscription blobs containing any of the above
//!
//! Anything that would require disabling certificate verification is refused
//! with an explanation rather than imported in a weakened form.

use crate::model::*;
use crate::validate;
use base64::Engine as _;
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ImportReport {
    pub profiles: Vec<ServerProfile>,
    pub issues: Vec<ImportIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportIssue {
    /// Short excerpt identifying the entry (never contains secrets).
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ImportError(pub String);

fn ierr<T>(m: impl Into<String>) -> Result<T, ImportError> {
    Err(ImportError(m.into()))
}

const INSECURE_MSG: &str = "This link turns off certificate verification (insecure / allowInsecure). \
LOTUS never disables TLS verification. Use REALITY, a valid certificate, or a LOTUS link from the server installer, which pins the server key.";

/// Parse any supported text. Each entry is validated; invalid entries become issues.
pub fn parse_import(text: &str) -> ImportReport {
    let mut report = ImportReport::default();
    parse_into(text.trim(), &mut report, 0);
    if report.profiles.is_empty() && report.issues.is_empty() {
        report.issues.push(ImportIssue {
            source: excerpt(text),
            message: "Nothing recognisable was found. Paste a vless://, ss://, trojan://, hysteria2://, lotus:// link, a WireGuard config or sing-box JSON.".into(),
        });
    }
    report
}

fn parse_into(text: &str, report: &mut ImportReport, depth: u8) {
    if text.is_empty() {
        return;
    }
    if text.contains("[Interface]") {
        push(report, "WireGuard config", parse_wireguard_conf(text, None));
        return;
    }
    if text.starts_with('{') || text.starts_with('[') {
        match parse_singbox_json(text) {
            Ok(list) => {
                for r in list {
                    push(report, "sing-box JSON", r);
                }
            }
            Err(e) => report.issues.push(ImportIssue { source: "JSON".into(), message: e.0 }),
        }
        return;
    }
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if lines.iter().any(|l| l.contains("://")) {
        for line in lines {
            if !line.contains("://") {
                continue;
            }
            if line.to_ascii_lowercase().starts_with("lotus://") {
                match parse_lotus_bundle(line) {
                    Ok(list) => {
                        for r in list {
                            push(report, "lotus:// link", r);
                        }
                    }
                    Err(e) => report.issues.push(ImportIssue { source: "lotus:// link".into(), message: e.0 }),
                }
            } else {
                push(report, &excerpt(line), parse_uri(line));
            }
        }
        return;
    }
    if depth == 0 {
        if let Some(decoded) = decode_b64_text(text) {
            parse_into(decoded.trim(), report, depth + 1);
        }
    }
}

fn push(report: &mut ImportReport, source: &str, r: Result<ServerProfile, ImportError>) {
    match r.and_then(|p| match validate::validate_profile(&p) {
        Ok(()) => Ok(p),
        Err(e) => Err(ImportError(e.to_string())),
    }) {
        Ok(p) => report.profiles.push(p),
        Err(e) => report.issues.push(ImportIssue { source: source.to_string(), message: e.0 }),
    }
}

/// A short, secret-free identifier for an entry: scheme and host only.
fn excerpt(line: &str) -> String {
    let line = line.trim();
    if let Some((scheme, rest)) = line.split_once("://") {
        let after_at = rest.rsplit_once('@').map(|(_, h)| h).unwrap_or("");
        let host: String = after_at.chars().take_while(|c| !"?#/".contains(*c)).take(64).collect();
        if host.is_empty() {
            format!("{scheme}://…")
        } else {
            format!("{scheme}://…@{host}")
        }
    } else {
        "entry".to_string()
    }
}

fn decode_b64_text(text: &str) -> Option<String> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 8 || !compact.chars().all(|c| c.is_ascii_alphanumeric() || "+/=-_".contains(c)) {
        return None;
    }
    let bytes = b64_any(&compact)?;
    String::from_utf8(bytes).ok()
}

/// Decode standard or URL-safe base64, with or without padding.
pub fn b64_any(s: &str) -> Option<Vec<u8>> {
    use base64::engine::general_purpose::*;
    let s = s.trim();
    let unpadded = s.trim_end_matches('=');
    STANDARD
        .decode(s)
        .or_else(|_| STANDARD_NO_PAD.decode(unpadded))
        .or_else(|_| URL_SAFE.decode(s))
        .or_else(|_| URL_SAFE_NO_PAD.decode(unpadded))
        .ok()
}

fn pct(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// Query string parsing that does *not* turn '+' into a space (base64 keys contain '+').
fn query_map(q: Option<&str>) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Some(q) = q {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            m.insert(pct(k).to_ascii_lowercase(), pct(v));
        }
    }
    m
}

fn truthy(v: Option<&String>) -> bool {
    matches!(v.map(|s| s.to_ascii_lowercase()).as_deref(), Some("1" | "true" | "yes"))
}

fn clean_name(raw: &str, server: &str, port: u16) -> String {
    let n: String = raw.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(64).collect();
    if n.is_empty() {
        let fallback = format!("{server}:{port}");
        fallback.chars().take(64).collect()
    } else {
        n
    }
}

struct Parts {
    user: String,
    pass: Option<String>,
    host: String,
    port: u16,
    query: HashMap<String, String>,
    name: String,
}

fn split_uri(uri: &str, default_port: Option<u16>) -> Result<Parts, ImportError> {
    let url = url::Url::parse(uri.trim()).map_err(|e| ImportError(format!("Not a valid link ({e})")))?;
    let host = url
        .host_str()
        .ok_or_else(|| ImportError("Link has no server address".into()))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let port = match url.port().or(default_port) {
        Some(p) => p,
        None => return ierr("Link has no port"),
    };
    Ok(Parts {
        user: pct(url.username()),
        pass: url.password().map(pct),
        host,
        port,
        query: query_map(url.query()),
        name: url.fragment().map(pct).unwrap_or_default(),
    })
}

pub fn parse_uri(uri: &str) -> Result<ServerProfile, ImportError> {
    let lower = uri.trim().to_ascii_lowercase();
    let scheme = lower.split("://").next().unwrap_or("");
    match scheme {
        "vless" => parse_vless(uri),
        "trojan" => parse_trojan(uri),
        "ss" => parse_ss(uri),
        "hysteria2" | "hy2" => parse_hy2(uri),
        "tuic" => parse_tuic(uri),
        "anytls" => parse_anytls(uri),
        "naive+https" | "naive+quic" => parse_naive(uri),
        "wireguard" | "wg" => parse_wg_uri(uri),
        "vmess" => ierr("VMess is a legacy protocol and isn't supported. Use VLESS with REALITY instead."),
        "ssr" => ierr("ShadowsocksR is obsolete and insecure; it isn't supported."),
        "hysteria" => ierr("Hysteria v1 isn't supported. Use Hysteria2."),
        "socks" | "socks5" | "http" | "https" => ierr("Plain proxies are not encrypted and can't be used as a SHADOW server."),
        "lotus" => ierr("Use the import screen for lotus:// links."),
        _ => ierr(format!("Unsupported link type \"{scheme}\"")),
    }
}

fn reject_insecure(q: &HashMap<String, String>) -> Result<(), ImportError> {
    if truthy(q.get("allowinsecure")) || truthy(q.get("insecure")) || truthy(q.get("allow_insecure")) || truthy(q.get("skip-cert-verify")) {
        return ierr(INSECURE_MSG);
    }
    Ok(())
}

fn split_list(v: Option<&String>) -> Vec<String> {
    v.map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default()
}

fn opt(v: Option<&String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn tls_from_query(q: &HashMap<String, String>, security: &str, host: &str) -> Result<Option<TlsConfig>, ImportError> {
    match security {
        "" | "none" => Ok(None),
        "tls" | "xtls" => {
            reject_insecure(q)?;
            Ok(Some(TlsConfig {
                server_name: opt(q.get("sni")).or_else(|| opt(q.get("peer"))).or_else(|| non_ip(host)),
                alpn: split_list(q.get("alpn")),
                fingerprint: opt(q.get("fp")).map(|f| f.to_ascii_lowercase()),
                ..Default::default()
            }))
        }
        "reality" => {
            let public_key = opt(q.get("pbk")).ok_or_else(|| ImportError("REALITY link is missing the public key (pbk)".into()))?;
            let fp = opt(q.get("fp")).map(|f| f.to_ascii_lowercase()).unwrap_or_else(|| "chrome".into());
            Ok(Some(TlsConfig {
                server_name: opt(q.get("sni")),
                alpn: split_list(q.get("alpn")),
                fingerprint: Some(fp),
                reality: Some(RealityConfig { public_key, short_id: q.get("sid").cloned().unwrap_or_default() }),
                ..Default::default()
            }))
        }
        other => ierr(format!("Unsupported security \"{other}\"")),
    }
}

fn non_ip(host: &str) -> Option<String> {
    if host.parse::<std::net::IpAddr>().is_ok() {
        None
    } else {
        Some(host.to_string())
    }
}

fn transport_from_query(q: &HashMap<String, String>) -> Result<Transport, ImportError> {
    let net = q.get("type").map(|s| s.to_ascii_lowercase()).unwrap_or_else(|| "tcp".into());
    match net.as_str() {
        "tcp" | "raw" | "" => {
            if q.get("headertype").map(|s| s.as_str()) == Some("http") {
                return ierr("TCP HTTP header obfuscation isn't supported.");
            }
            Ok(Transport::Tcp)
        }
        "ws" | "websocket" => Ok(Transport::Ws { path: q.get("path").cloned().unwrap_or_else(|| "/".into()), host: opt(q.get("host")) }),
        "grpc" => Ok(Transport::Grpc {
            service_name: opt(q.get("servicename")).ok_or_else(|| ImportError("gRPC link is missing serviceName".into()))?,
        }),
        "http" | "h2" => Ok(Transport::Http { path: q.get("path").cloned().unwrap_or_default(), host: split_list(q.get("host")) }),
        "httpupgrade" => Ok(Transport::HttpUpgrade { path: q.get("path").cloned().unwrap_or_else(|| "/".into()), host: opt(q.get("host")) }),
        "xhttp" | "splithttp" => ierr("The XHTTP transport is Xray-only and not available in the LOTUS engine. Use REALITY + Vision over TCP."),
        "kcp" | "mkcp" | "quic" => ierr(format!("The \"{net}\" transport isn't supported.")),
        other => ierr(format!("Unknown transport \"{other}\"")),
    }
}

fn parse_vless(uri: &str) -> Result<ServerProfile, ImportError> {
    let p = split_uri(uri, Some(443))?;
    let q = &p.query;
    if let Some(enc) = q.get("encryption") {
        if !enc.is_empty() && enc != "none" {
            return ierr("This link uses Xray's VLESS Encryption layer, which the LOTUS engine doesn't implement.");
        }
    }
    let security = q.get("security").map(|s| s.to_ascii_lowercase()).unwrap_or_default();
    let tls = tls_from_query(q, &security, &p.host)?;
    let flow = opt(q.get("flow")).map(|f| if f == "xtls-rprx-vision-udp443" { VISION_FLOW.to_string() } else { f });
    let cfg = VlessConfig {
        server: p.host.clone(),
        port: p.port,
        uuid: p.user.clone(),
        flow,
        tls,
        transport: transport_from_query(q)?,
    };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::Vless(cfg)))
}

fn parse_trojan(uri: &str) -> Result<ServerProfile, ImportError> {
    let p = split_uri(uri, Some(443))?;
    let q = &p.query;
    let security = q.get("security").map(|s| s.to_ascii_lowercase()).unwrap_or_else(|| "tls".into());
    if security == "none" {
        return ierr("Trojan without TLS is unencrypted.");
    }
    let tls = tls_from_query(q, &security, &p.host)?.unwrap_or_default();
    let cfg = TrojanConfig { server: p.host.clone(), port: p.port, password: p.user.clone(), tls, transport: transport_from_query(q)? };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::Trojan(cfg)))
}

fn parse_ss(uri: &str) -> Result<ServerProfile, ImportError> {
    let body = &uri.trim()[5..];
    let (body, name) = match body.split_once('#') {
        Some((b, n)) => (b, pct(n)),
        None => (body, String::new()),
    };
    let (body, query) = match body.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (body, None),
    };
    let q = query_map(query);
    if let Some(plugin) = q.get("plugin") {
        if !plugin.is_empty() {
            return ierr("Shadowsocks plugins (obfs, v2ray-plugin, …) aren't supported.");
        }
    }
    let body = body.trim_end_matches('/');
    let (userinfo, hostport) = if let Some((u, h)) = body.rsplit_once('@') {
        let u = pct(u);
        let decoded = b64_any(&u).and_then(|b| String::from_utf8(b).ok()).filter(|s| s.contains(':'));
        (decoded.unwrap_or(u), h.to_string())
    } else {
        let decoded = b64_any(body)
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| ImportError("Shadowsocks link isn't valid".into()))?;
        let (u, h) = decoded.rsplit_once('@').ok_or_else(|| ImportError("Shadowsocks link isn't valid".into()))?;
        (u.to_string(), h.to_string())
    };
    let (method, password) = userinfo.split_once(':').ok_or_else(|| ImportError("Shadowsocks link is missing method or password".into()))?;
    let (host, port) = split_hostport(&hostport)?;
    let cfg = ShadowsocksConfig { server: host.clone(), port, method: method.to_ascii_lowercase(), password: password.to_string() };
    Ok(ServerProfile::new(clean_name(&name, &host, port), Protocol::Shadowsocks(cfg)))
}

fn split_hostport(s: &str) -> Result<(String, u16), ImportError> {
    let s = s.trim();
    let (h, p) = if let Some(rest) = s.strip_prefix('[') {
        let (h, p) = rest.split_once("]:").ok_or_else(|| ImportError("Invalid address".into()))?;
        (h.to_string(), p)
    } else {
        let (h, p) = s.rsplit_once(':').ok_or_else(|| ImportError("Address is missing a port".into()))?;
        (h.to_string(), p)
    };
    let port: u16 = p.trim().parse().map_err(|_| ImportError("Invalid port".into()))?;
    Ok((h, port))
}

fn parse_hy2(uri: &str) -> Result<ServerProfile, ImportError> {
    let p = split_uri(uri, Some(443)).map_err(|e| {
        if uri.contains(',') || uri.rsplit('@').next().map(|h| h.contains('-')).unwrap_or(false) {
            ImportError("Port hopping ranges aren't supported in links yet; use a single port.".into())
        } else {
            e
        }
    })?;
    let q = &p.query;
    if truthy(q.get("insecure")) {
        return ierr(INSECURE_MSG);
    }
    let password = match &p.pass {
        Some(pass) => format!("{}:{}", p.user, pass),
        None => p.user.clone(),
    };
    let obfs = match q.get("obfs").map(|s| s.as_str()) {
        None | Some("") | Some("none") => None,
        Some(kind) => Some(Obfs {
            kind: kind.to_string(),
            password: q.get("obfs-password").cloned().ok_or_else(|| ImportError("Obfuscation password missing".into()))?,
        }),
    };
    let tls = TlsConfig {
        server_name: opt(q.get("sni")).or_else(|| non_ip(&p.host)),
        alpn: split_list(q.get("alpn")),
        public_key_sha256: split_list(q.get("pinpubsha256")),
        ..Default::default()
    };
    let cfg = Hysteria2Config {
        server: p.host.clone(),
        port: p.port,
        password,
        obfs,
        tls,
        up_mbps: q.get("upmbps").and_then(|v| v.parse().ok()),
        down_mbps: q.get("downmbps").and_then(|v| v.parse().ok()),
    };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::Hysteria2(cfg)))
}

fn parse_tuic(uri: &str) -> Result<ServerProfile, ImportError> {
    let p = split_uri(uri, Some(443))?;
    let q = &p.query;
    reject_insecure(q)?;
    let alpn = split_list(q.get("alpn"));
    let cfg = TuicConfig {
        server: p.host.clone(),
        port: p.port,
        uuid: p.user.clone(),
        password: p.pass.clone().unwrap_or_default(),
        congestion_control: opt(q.get("congestion_control")).unwrap_or_else(|| "bbr".into()),
        udp_relay_mode: opt(q.get("udp_relay_mode")),
        tls: TlsConfig {
            server_name: opt(q.get("sni")).or_else(|| non_ip(&p.host)),
            alpn: if alpn.is_empty() { vec!["h3".into()] } else { alpn },
            ..Default::default()
        },
    };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::Tuic(cfg)))
}

fn parse_anytls(uri: &str) -> Result<ServerProfile, ImportError> {
    let p = split_uri(uri, Some(443))?;
    let q = &p.query;
    let security = q.get("security").map(|s| s.to_ascii_lowercase()).unwrap_or_else(|| "tls".into());
    let tls = tls_from_query(q, &security, &p.host)?.ok_or_else(|| ImportError("AnyTLS requires TLS".into()))?;
    let cfg = AnyTlsConfig { server: p.host.clone(), port: p.port, password: p.user.clone(), tls };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::AnyTls(cfg)))
}

fn parse_naive(uri: &str) -> Result<ServerProfile, ImportError> {
    let quic = uri.trim().to_ascii_lowercase().starts_with("naive+quic");
    let p = split_uri(uri, Some(443))?;
    let cfg = NaiveConfig {
        server: p.host.clone(),
        port: p.port,
        username: p.user.clone(),
        password: p.pass.clone().unwrap_or_default(),
        tls: TlsConfig { server_name: opt(p.query.get("sni")).or_else(|| non_ip(&p.host)), ..Default::default() },
        quic,
    };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::Naive(cfg)))
}

fn parse_wg_uri(uri: &str) -> Result<ServerProfile, ImportError> {
    let p = split_uri(uri, Some(51820))?;
    let q = &p.query;
    let reserved = parse_reserved(q.get("reserved"))?;
    let cfg = WireGuardConfig {
        server: p.host.clone(),
        port: p.port,
        private_key: p.user.clone(),
        peer_public_key: opt(q.get("publickey")).or_else(|| opt(q.get("peer_public_key"))).ok_or_else(|| ImportError("WireGuard link is missing publickey".into()))?,
        pre_shared_key: opt(q.get("presharedkey")),
        local_address: split_list(q.get("address")).into_iter().map(with_prefix).collect(),
        mtu: q.get("mtu").and_then(|v| v.parse().ok()),
        reserved,
        keepalive: q.get("keepalive").and_then(|v| v.parse().ok()),
    };
    Ok(ServerProfile::new(clean_name(&p.name, &p.host, p.port), Protocol::WireGuard(cfg)))
}

fn parse_reserved(v: Option<&String>) -> Result<Option<[u8; 3]>, ImportError> {
    let Some(v) = v.filter(|s| !s.is_empty()) else { return Ok(None) };
    let nums: Vec<u8> = v.split(',').map(|x| x.trim().parse::<u8>()).collect::<Result<_, _>>().map_err(|_| ImportError("Invalid reserved bytes".into()))?;
    if nums.len() != 3 {
        return ierr("Reserved must be three bytes");
    }
    Ok(Some([nums[0], nums[1], nums[2]]))
}

fn with_prefix(a: String) -> String {
    if a.contains('/') {
        a
    } else if a.contains(':') {
        format!("{a}/128")
    } else {
        format!("{a}/32")
    }
}

/// Parse a wg-quick style configuration file.
pub fn parse_wireguard_conf(text: &str, name: Option<&str>) -> Result<ServerProfile, ImportError> {
    let mut section = String::new();
    let mut iface: HashMap<String, String> = HashMap::new();
    let mut peers: Vec<HashMap<String, String>> = Vec::new();
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
            if section == "peer" {
                peers.push(HashMap::new());
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().to_string();
        match section.as_str() {
            "interface" => {
                iface.insert(k, v);
            }
            "peer" => {
                if let Some(p) = peers.last_mut() {
                    p.insert(k, v);
                }
            }
            _ => {}
        }
    }
    if peers.len() != 1 {
        return ierr("WireGuard config must contain exactly one [Peer]");
    }
    let peer = &peers[0];
    for unsupported in ["preup", "postup", "predown", "postdown"] {
        if iface.contains_key(unsupported) {
            return ierr("WireGuard configs with PreUp/PostUp scripts aren't imported, because they would run commands.");
        }
    }
    let endpoint = peer.get("endpoint").ok_or_else(|| ImportError("WireGuard [Peer] has no Endpoint".into()))?;
    let (host, port) = split_hostport(endpoint)?;
    let allowed = split_list(peer.get("allowedips"));
    if !allowed.is_empty() && !allowed.iter().any(|a| a == "0.0.0.0/0" || a == "::/0") {
        return ierr("This WireGuard config routes only some networks (AllowedIPs); SHADOW needs a full-tunnel config with 0.0.0.0/0.");
    }
    let cfg = WireGuardConfig {
        server: host.clone(),
        port,
        private_key: iface.get("privatekey").cloned().ok_or_else(|| ImportError("WireGuard [Interface] has no PrivateKey".into()))?,
        peer_public_key: peer.get("publickey").cloned().ok_or_else(|| ImportError("WireGuard [Peer] has no PublicKey".into()))?,
        pre_shared_key: peer.get("presharedkey").cloned(),
        local_address: split_list(iface.get("address")).into_iter().map(with_prefix).collect(),
        mtu: iface.get("mtu").and_then(|v| v.parse().ok()),
        reserved: None,
        keepalive: peer.get("persistentkeepalive").and_then(|v| v.parse().ok()),
    };
    Ok(ServerProfile::new(clean_name(name.unwrap_or("WireGuard"), &host, port), Protocol::WireGuard(cfg)))
}

// ---------------------------------------------------------------------------
// LOTUS bundles
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub v: u32,
    pub profiles: Vec<BundleProfile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleProfile {
    pub name: String,
    pub protocol: Protocol,
}

pub fn encode_bundle(profiles: &[ServerProfile]) -> String {
    let b = Bundle {
        v: 1,
        profiles: profiles.iter().map(|p| BundleProfile { name: p.name.clone(), protocol: p.protocol.clone() }).collect(),
    };
    let json = serde_json::to_vec(&b).expect("bundle serialises");
    format!("lotus://import/{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json))
}

pub fn parse_lotus_bundle(link: &str) -> Result<Vec<Result<ServerProfile, ImportError>>, ImportError> {
    let body = link.trim();
    let body = &body[body.find("://").map(|i| i + 3).unwrap_or(0)..];
    let body = body.strip_prefix("import/").unwrap_or(body);
    let raw = b64_any(body).ok_or_else(|| ImportError("lotus:// link is damaged (not base64)".into()))?;
    if raw.len() > 256 * 1024 {
        return ierr("lotus:// link is too large");
    }
    let b: Bundle = serde_json::from_slice(&raw).map_err(|e| ImportError(format!("lotus:// link is damaged ({e})")))?;
    if b.v != 1 {
        return ierr("This lotus:// link was made by a newer LOTUS version");
    }
    Ok(b.profiles
        .into_iter()
        .map(|bp| {
            let server = bp.protocol.server().to_string();
            let port = bp.protocol.port();
            Ok(ServerProfile::new(clean_name(&bp.name, &server, port), bp.protocol))
        })
        .collect())
}

// ---------------------------------------------------------------------------
// sing-box JSON
// ---------------------------------------------------------------------------

fn parse_singbox_json(text: &str) -> Result<Vec<Result<ServerProfile, ImportError>>, ImportError> {
    if text.len() > 2 * 1024 * 1024 {
        return ierr("File is too large");
    }
    let v: Value = serde_json::from_str(text).map_err(|e| ImportError(format!("Not valid JSON ({e})")))?;
    let mut items: Vec<Value> = Vec::new();
    match &v {
        Value::Array(a) => items.extend(a.iter().cloned()),
        Value::Object(o) if o.contains_key("outbounds") || o.contains_key("endpoints") => {
            for key in ["outbounds", "endpoints"] {
                if let Some(Value::Array(a)) = o.get(key) {
                    items.extend(a.iter().cloned());
                }
            }
        }
        Value::Object(o) if o.contains_key("type") => items.push(v.clone()),
        _ => return ierr("JSON doesn't look like a sing-box configuration or outbound"),
    }
    let skip = ["direct", "block", "dns", "selector", "urltest", "tor", "socks", "http", "ssh", "tailscale"];
    let mut out = Vec::new();
    for item in items {
        let t = item.get("type").and_then(Value::as_str).unwrap_or("");
        if skip.contains(&t) {
            continue;
        }
        out.push(outbound_to_profile(&item));
    }
    if out.is_empty() {
        return ierr("No supported proxy outbound was found in the JSON");
    }
    Ok(out)
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string).filter(|x| !x.is_empty())
}

fn port_of(v: &Value, k: &str) -> Result<u16, ImportError> {
    v.get(k)
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| ImportError(format!("Missing or invalid \"{k}\"")))
}

fn req(v: &Value, k: &str) -> Result<String, ImportError> {
    s(v, k).ok_or_else(|| ImportError(format!("Missing \"{k}\"")))
}

fn tls_from_json(v: Option<&Value>) -> Result<Option<TlsConfig>, ImportError> {
    let Some(t) = v else { return Ok(None) };
    if !t.get("enabled").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(None);
    }
    if t.get("insecure").and_then(Value::as_bool).unwrap_or(false) {
        return ierr(INSECURE_MSG);
    }
    if t.get("certificate_path").is_some() {
        return ierr("certificate_path refers to a local file; paste the certificate inline instead");
    }
    let certificate = match t.get("certificate") {
        Some(Value::String(c)) => Some(c.clone()),
        Some(Value::Array(lines)) => Some(lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")),
        _ => None,
    };
    let reality = t.get("reality").filter(|r| r.get("enabled").and_then(Value::as_bool).unwrap_or(false)).map(|r| RealityConfig {
        public_key: s(r, "public_key").unwrap_or_default(),
        short_id: s(r, "short_id").unwrap_or_default(),
    });
    let fingerprint = t
        .get("utls")
        .filter(|u| u.get("enabled").and_then(Value::as_bool).unwrap_or(false))
        .map(|u| s(u, "fingerprint").unwrap_or_else(|| "chrome".into()));
    Ok(Some(TlsConfig {
        server_name: s(t, "server_name"),
        alpn: match t.get("alpn") {
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            Some(Value::String(a)) => vec![a.clone()],
            _ => vec![],
        },
        fingerprint,
        reality,
        certificate,
        public_key_sha256: match t.get("certificate_public_key_sha256") {
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            Some(Value::String(a)) => vec![a.clone()],
            _ => vec![],
        },
    }))
}

fn transport_from_json(v: Option<&Value>) -> Result<Transport, ImportError> {
    let Some(t) = v else { return Ok(Transport::Tcp) };
    let ty = t.get("type").and_then(Value::as_str).unwrap_or("");
    let host_str = |t: &Value| -> Option<String> {
        match t.get("host") {
            Some(Value::String(h)) => Some(h.clone()),
            _ => t.get("headers").and_then(|h| h.get("Host")).and_then(Value::as_str).map(str::to_string),
        }
    };
    match ty {
        "ws" => Ok(Transport::Ws { path: s(t, "path").unwrap_or_else(|| "/".into()), host: host_str(t) }),
        "grpc" => Ok(Transport::Grpc { service_name: req(t, "service_name")? }),
        "http" => Ok(Transport::Http {
            path: s(t, "path").unwrap_or_default(),
            host: match t.get("host") {
                Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
                Some(Value::String(h)) => vec![h.clone()],
                _ => vec![],
            },
        }),
        "httpupgrade" => Ok(Transport::HttpUpgrade { path: s(t, "path").unwrap_or_else(|| "/".into()), host: host_str(t) }),
        other => ierr(format!("Transport \"{other}\" isn't supported")),
    }
}

fn outbound_to_profile(v: &Value) -> Result<ServerProfile, ImportError> {
    let t = v.get("type").and_then(Value::as_str).unwrap_or("");
    let name = s(v, "tag").unwrap_or_default();
    let tls = tls_from_json(v.get("tls"))?;
    let need_tls = |tls: Option<TlsConfig>| tls.ok_or_else(|| ImportError(format!("{t} outbound needs TLS enabled")));
    let protocol = match t {
        "vless" => Protocol::Vless(VlessConfig {
            server: req(v, "server")?,
            port: port_of(v, "server_port")?,
            uuid: req(v, "uuid")?,
            flow: s(v, "flow"),
            tls,
            transport: transport_from_json(v.get("transport"))?,
        }),
        "trojan" => Protocol::Trojan(TrojanConfig {
            server: req(v, "server")?,
            port: port_of(v, "server_port")?,
            password: req(v, "password")?,
            tls: need_tls(tls)?,
            transport: transport_from_json(v.get("transport"))?,
        }),
        "shadowsocks" => {
            if v.get("plugin").is_some() {
                return ierr("Shadowsocks plugins aren't supported");
            }
            Protocol::Shadowsocks(ShadowsocksConfig {
                server: req(v, "server")?,
                port: port_of(v, "server_port")?,
                method: req(v, "method")?,
                password: req(v, "password")?,
            })
        }
        "hysteria2" => Protocol::Hysteria2(Hysteria2Config {
            server: req(v, "server")?,
            port: port_of(v, "server_port")?,
            password: req(v, "password")?,
            obfs: v.get("obfs").and_then(|o| Some(Obfs { kind: s(o, "type")?, password: s(o, "password").unwrap_or_default() })),
            tls: need_tls(tls)?,
            up_mbps: v.get("up_mbps").and_then(Value::as_u64).map(|x| x as u32),
            down_mbps: v.get("down_mbps").and_then(Value::as_u64).map(|x| x as u32),
        }),
        "tuic" => Protocol::Tuic(TuicConfig {
            server: req(v, "server")?,
            port: port_of(v, "server_port")?,
            uuid: req(v, "uuid")?,
            password: s(v, "password").unwrap_or_default(),
            congestion_control: s(v, "congestion_control").unwrap_or_else(|| "bbr".into()),
            udp_relay_mode: s(v, "udp_relay_mode"),
            tls: need_tls(tls)?,
        }),
        "anytls" => Protocol::AnyTls(AnyTlsConfig {
            server: req(v, "server")?,
            port: port_of(v, "server_port")?,
            password: req(v, "password")?,
            tls: need_tls(tls)?,
        }),
        "naive" => Protocol::Naive(NaiveConfig {
            server: req(v, "server")?,
            port: port_of(v, "server_port")?,
            username: s(v, "username").unwrap_or_default(),
            password: s(v, "password").unwrap_or_default(),
            tls: need_tls(tls)?,
            quic: v.get("quic").and_then(Value::as_bool).unwrap_or(false),
        }),
        "wireguard" => wireguard_from_json(v)?,
        "vmess" => return ierr("VMess is a legacy protocol and isn't supported."),
        other => return ierr(format!("Outbound type \"{other}\" isn't supported")),
    };
    let server = protocol.server().to_string();
    let port = protocol.port();
    Ok(ServerProfile::new(clean_name(&name, &server, port), protocol))
}

fn wireguard_from_json(v: &Value) -> Result<Protocol, ImportError> {
    let addrs = |key: &str| -> Vec<String> {
        match v.get(key) {
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            Some(Value::String(x)) => vec![x.clone()],
            _ => vec![],
        }
    };
    let reserved = |r: Option<&Value>| -> Option<[u8; 3]> {
        let a = r?.as_array()?;
        if a.len() != 3 {
            return None;
        }
        Some([a[0].as_u64()? as u8, a[1].as_u64()? as u8, a[2].as_u64()? as u8])
    };
    // Endpoint format (1.11+): peers array. Legacy outbound format: flat fields.
    if let Some(Value::Array(peers)) = v.get("peers") {
        if peers.len() != 1 {
            return ierr("Only WireGuard endpoints with exactly one peer are supported");
        }
        let p = &peers[0];
        return Ok(Protocol::WireGuard(WireGuardConfig {
            server: req(p, "address")?,
            port: port_of(p, "port")?,
            private_key: req(v, "private_key")?,
            peer_public_key: req(p, "public_key")?,
            pre_shared_key: s(p, "pre_shared_key"),
            local_address: addrs("address"),
            mtu: v.get("mtu").and_then(Value::as_u64).map(|m| m as u16),
            reserved: reserved(p.get("reserved")),
            keepalive: p.get("persistent_keepalive_interval").and_then(Value::as_u64).map(|k| k as u16),
        }));
    }
    Ok(Protocol::WireGuard(WireGuardConfig {
        server: req(v, "server")?,
        port: port_of(v, "server_port")?,
        private_key: req(v, "private_key")?,
        peer_public_key: req(v, "peer_public_key")?,
        pre_shared_key: s(v, "pre_shared_key"),
        local_address: addrs("local_address"),
        mtu: v.get("mtu").and_then(Value::as_u64).map(|m| m as u16),
        reserved: reserved(v.get("reserved")),
        keepalive: None,
    }))
}

// ---------------------------------------------------------------------------
// Export (share links)
// ---------------------------------------------------------------------------

fn enc(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn host_for_uri(h: &str) -> String {
    if h.contains(':') && !h.starts_with('[') {
        format!("[{h}]")
    } else {
        h.to_string()
    }
}

/// Produce a standard share link where one exists. Profiles that need data no
/// standard link can carry (e.g. a pinned key) are exported as a `lotus://` bundle.
pub fn share_link(p: &ServerProfile) -> String {
    let name = enc(&p.name);
    let mut q: Vec<(String, String)> = Vec::new();
    let tls_q = |q: &mut Vec<(String, String)>, t: &TlsConfig| {
        if let Some(r) = &t.reality {
            q.push(("security".into(), "reality".into()));
            q.push(("pbk".into(), r.public_key.clone()));
            q.push(("sid".into(), r.short_id.clone()));
        } else {
            q.push(("security".into(), "tls".into()));
        }
        if let Some(sn) = &t.server_name {
            q.push(("sni".into(), sn.clone()));
        }
        if let Some(fp) = &t.fingerprint {
            q.push(("fp".into(), fp.clone()));
        }
        if !t.alpn.is_empty() {
            q.push(("alpn".into(), t.alpn.join(",")));
        }
    };
    let transport_q = |q: &mut Vec<(String, String)>, t: &Transport| match t {
        Transport::Tcp => q.push(("type".into(), "tcp".into())),
        Transport::Ws { path, host } => {
            q.push(("type".into(), "ws".into()));
            q.push(("path".into(), path.clone()));
            if let Some(h) = host {
                q.push(("host".into(), h.clone()));
            }
        }
        Transport::Grpc { service_name } => {
            q.push(("type".into(), "grpc".into()));
            q.push(("serviceName".into(), service_name.clone()));
        }
        Transport::Http { path, host } => {
            q.push(("type".into(), "http".into()));
            q.push(("path".into(), path.clone()));
            if !host.is_empty() {
                q.push(("host".into(), host.join(",")));
            }
        }
        Transport::HttpUpgrade { path, host } => {
            q.push(("type".into(), "httpupgrade".into()));
            q.push(("path".into(), path.clone()));
            if let Some(h) = host {
                q.push(("host".into(), h.clone()));
            }
        }
    };
    let qs = |q: &[(String, String)]| q.iter().map(|(k, v)| format!("{}={}", k, enc(v))).collect::<Vec<_>>().join("&");
    match &p.protocol {
        Protocol::Vless(c) => {
            q.push(("encryption".into(), "none".into()));
            if let Some(f) = &c.flow {
                q.push(("flow".into(), f.clone()));
            }
            if let Some(t) = &c.tls {
                tls_q(&mut q, t);
            }
            transport_q(&mut q, &c.transport);
            format!("vless://{}@{}:{}?{}#{}", enc(&c.uuid), host_for_uri(&c.server), c.port, qs(&q), name)
        }
        Protocol::Trojan(c) => {
            tls_q(&mut q, &c.tls);
            transport_q(&mut q, &c.transport);
            format!("trojan://{}@{}:{}?{}#{}", enc(&c.password), host_for_uri(&c.server), c.port, qs(&q), name)
        }
        Protocol::Shadowsocks(c) => {
            let ui = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{}:{}", c.method, c.password));
            format!("ss://{}@{}:{}#{}", ui, host_for_uri(&c.server), c.port, name)
        }
        Protocol::Hysteria2(c) if c.tls.certificate.is_none() && c.tls.public_key_sha256.is_empty() => {
            if let Some(sn) = &c.tls.server_name {
                q.push(("sni".into(), sn.clone()));
            }
            if let Some(o) = &c.obfs {
                q.push(("obfs".into(), o.kind.clone()));
                q.push(("obfs-password".into(), o.password.clone()));
            }
            format!("hysteria2://{}@{}:{}/?{}#{}", enc(&c.password), host_for_uri(&c.server), c.port, qs(&q), name)
        }
        Protocol::Naive(c) => {
            let scheme = if c.quic { "naive+quic" } else { "naive+https" };
            format!("{scheme}://{}:{}@{}:{}#{}", enc(&c.username), enc(&c.password), host_for_uri(&c.server), c.port, name)
        }
        _ => encode_bundle(std::slice::from_ref(p)),
    }
}
