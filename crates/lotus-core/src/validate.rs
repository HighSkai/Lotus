//! Strict validation of server profiles and options.
//!
//! The service re-validates everything it receives, regardless of what the
//! desktop app already checked. Validation is intentionally conservative:
//! unencrypted transports, disabled certificate verification and outdated
//! ciphers are rejected with an explanation instead of being silently accepted.

use crate::model::*;
use base64::Engine as _;
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{field}: {message}")]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

fn err(field: &str, message: impl Into<String>) -> ValidationError {
    ValidationError { field: field.to_string(), message: message.into() }
}

type R = Result<(), ValidationError>;

pub const UTLS_FINGERPRINTS: &[&str] = &["chrome", "firefox", "edge", "safari", "ios", "android", "random", "randomized"];

pub const SS_METHODS: &[&str] = &[
    "2022-blake3-aes-128-gcm",
    "2022-blake3-aes-256-gcm",
    "2022-blake3-chacha20-poly1305",
    "aes-128-gcm",
    "aes-256-gcm",
    "chacha20-ietf-poly1305",
    "xchacha20-ietf-poly1305",
];

pub fn validate_profile(p: &ServerProfile) -> R {
    check_name(&p.name)?;
    if p.id.is_empty() || p.id.len() > 64 || !p.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(err("id", "Invalid profile id"));
    }
    validate_protocol(&p.protocol)
}

pub fn check_name(name: &str) -> R {
    let n = name.trim();
    if n.is_empty() {
        return Err(err("name", "Give the server a name"));
    }
    if n.chars().count() > 64 {
        return Err(err("name", "Name is too long (64 characters max)"));
    }
    if n.chars().any(|c| c.is_control()) {
        return Err(err("name", "Name contains control characters"));
    }
    Ok(())
}

pub fn validate_protocol(p: &Protocol) -> R {
    check_host(p.server()).map_err(|e| err("server", e.message))?;
    if p.port() == 0 {
        return Err(err("port", "Port must be between 1 and 65535"));
    }
    match p {
        Protocol::Vless(c) => {
            check_uuid("uuid", &c.uuid)?;
            let tls = c.tls.as_ref().ok_or_else(|| {
                err("tls", "VLESS without TLS or REALITY is unencrypted. LOTUS requires TLS or REALITY.")
            })?;
            check_tls(tls, TlsKind::Tcp)?;
            match c.flow.as_deref() {
                None | Some("") => {}
                Some(VISION_FLOW) => {
                    if c.transport != Transport::Tcp {
                        return Err(err("flow", "XTLS-Vision works only with the raw TCP transport"));
                    }
                }
                Some(other) => return Err(err("flow", format!("Unsupported flow \"{other}\""))),
            }
            if tls.reality.is_some() && !matches!(c.transport, Transport::Tcp | Transport::Grpc { .. } | Transport::Http { .. }) {
                return Err(err("transport", "REALITY supports TCP, gRPC or HTTP/2 transports only"));
            }
            check_transport(&c.transport)
        }
        Protocol::Trojan(c) => {
            check_secret("password", &c.password)?;
            check_tls(&c.tls, TlsKind::Tcp)?;
            check_transport(&c.transport)
        }
        Protocol::Shadowsocks(c) => {
            if !SS_METHODS.contains(&c.method.as_str()) {
                return Err(err(
                    "method",
                    format!("Cipher \"{}\" is not supported. Use a Shadowsocks-2022 or AEAD cipher.", c.method),
                ));
            }
            check_secret("password", &c.password)?;
            if let Some(len) = ss2022_key_len(&c.method) {
                for part in c.password.split(':') {
                    let key = base64::engine::general_purpose::STANDARD
                        .decode(part)
                        .map_err(|_| err("password", "Shadowsocks-2022 keys must be base64"))?;
                    if key.len() != len {
                        return Err(err("password", format!("This cipher needs a {len}-byte base64 key")));
                    }
                }
            }
            Ok(())
        }
        Protocol::Hysteria2(c) => {
            check_secret("password", &c.password)?;
            check_tls(&c.tls, TlsKind::Quic)?;
            if let Some(o) = &c.obfs {
                if o.kind != "salamander" && o.kind != "gecko" {
                    return Err(err("obfs", "Obfuscation must be salamander or gecko"));
                }
                check_secret("obfs.password", &o.password)?;
            }
            for (f, v) in [("up_mbps", c.up_mbps), ("down_mbps", c.down_mbps)] {
                if let Some(v) = v {
                    if v == 0 || v > 100_000 {
                        return Err(err(f, "Bandwidth must be between 1 and 100000 Mbps"));
                    }
                }
            }
            Ok(())
        }
        Protocol::Tuic(c) => {
            check_uuid("uuid", &c.uuid)?;
            check_secret("password", &c.password)?;
            check_tls(&c.tls, TlsKind::Quic)?;
            if !["bbr", "cubic", "new_reno"].contains(&c.congestion_control.as_str()) {
                return Err(err("congestion_control", "Use bbr, cubic or new_reno"));
            }
            if let Some(m) = &c.udp_relay_mode {
                if m != "native" && m != "quic" {
                    return Err(err("udp_relay_mode", "Use native or quic"));
                }
            }
            Ok(())
        }
        Protocol::AnyTls(c) => {
            check_secret("password", &c.password)?;
            check_tls(&c.tls, TlsKind::Tcp)
        }
        Protocol::Naive(c) => {
            check_secret("username", &c.username)?;
            check_secret("password", &c.password)?;
            check_tls(&c.tls, TlsKind::Naive)
        }
        Protocol::WireGuard(c) => {
            check_wg_key("private_key", &c.private_key)?;
            check_wg_key("peer_public_key", &c.peer_public_key)?;
            if let Some(psk) = &c.pre_shared_key {
                check_wg_key("pre_shared_key", psk)?;
            }
            if c.local_address.is_empty() {
                return Err(err("local_address", "WireGuard needs at least one interface address"));
            }
            for a in &c.local_address {
                check_cidr(a).map_err(|m| err("local_address", m))?;
            }
            if let Some(mtu) = c.mtu {
                if !(1280..=1500).contains(&mtu) {
                    return Err(err("mtu", "MTU must be between 1280 and 1500"));
                }
            }
            if let Some(k) = c.keepalive {
                if k > 3600 {
                    return Err(err("keepalive", "Keepalive must be at most 3600 seconds"));
                }
            }
            Ok(())
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum TlsKind {
    Tcp,
    Quic,
    Naive,
}

fn check_tls(t: &TlsConfig, kind: TlsKind) -> R {
    if let Some(sn) = &t.server_name {
        if !sn.is_empty() {
            check_host(sn).map_err(|e| err("tls.server_name", e.message))?;
        }
    }
    if t.alpn.len() > 8 {
        return Err(err("tls.alpn", "Too many ALPN values"));
    }
    for a in &t.alpn {
        if a.is_empty() || a.len() > 32 || !a.chars().all(|c| c.is_ascii_graphic()) {
            return Err(err("tls.alpn", "Invalid ALPN value"));
        }
    }
    if let Some(fp) = &t.fingerprint {
        if kind != TlsKind::Tcp {
            return Err(err("tls.fingerprint", "Browser fingerprints apply to TCP-based TLS only"));
        }
        if !UTLS_FINGERPRINTS.contains(&fp.as_str()) {
            return Err(err("tls.fingerprint", format!("Unknown or outdated fingerprint \"{fp}\"")));
        }
    }
    if let Some(r) = &t.reality {
        if kind != TlsKind::Tcp {
            return Err(err("tls.reality", "REALITY is available for TCP-based protocols only"));
        }
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(r.public_key.trim_end_matches('='))
            .map_err(|_| err("tls.reality.public_key", "REALITY public key must be base64url"))?;
        if key.len() != 32 {
            return Err(err("tls.reality.public_key", "REALITY public key must be 32 bytes"));
        }
        if r.short_id.len() > 16 || r.short_id.len() % 2 != 0 || !r.short_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(err("tls.reality.short_id", "Short ID must be 0–16 hex characters (even length)"));
        }
        if t.server_name.as_deref().unwrap_or("").is_empty() {
            return Err(err("tls.server_name", "REALITY needs the camouflage server name (SNI)"));
        }
        if t.fingerprint.is_none() {
            return Err(err("tls.fingerprint", "REALITY needs a browser fingerprint (e.g. chrome)"));
        }
        if t.certificate.is_some() || !t.public_key_sha256.is_empty() {
            return Err(err("tls", "REALITY authenticates the server itself; remove certificate options"));
        }
    }
    if kind == TlsKind::Naive && (!t.public_key_sha256.is_empty() || !t.alpn.is_empty()) {
        return Err(err("tls", "NaiveProxy supports server name and certificate only"));
    }
    if let Some(pem) = &t.certificate {
        if pem.len() > 16 * 1024 || !pem.contains("-----BEGIN CERTIFICATE-----") || !pem.contains("-----END CERTIFICATE-----") {
            return Err(err("tls.certificate", "Certificate must be a PEM-encoded X.509 certificate"));
        }
    }
    if t.certificate.is_some() && !t.public_key_sha256.is_empty() {
        return Err(err("tls", "Use either a trusted certificate or a public-key pin, not both"));
    }
    if t.public_key_sha256.len() > 4 {
        return Err(err("tls.public_key_sha256", "Too many pins"));
    }
    for pin in &t.public_key_sha256 {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(pin)
            .map_err(|_| err("tls.public_key_sha256", "Pin must be base64 SHA-256"))?;
        if raw.len() != 32 {
            return Err(err("tls.public_key_sha256", "Pin must be a SHA-256 digest"));
        }
    }
    Ok(())
}

fn check_transport(t: &Transport) -> R {
    let path_ok = |p: &str| p.is_empty() || (p.starts_with('/') && p.len() <= 1024 && p.chars().all(|c| c.is_ascii_graphic()));
    match t {
        Transport::Tcp => Ok(()),
        Transport::Ws { path, host } | Transport::HttpUpgrade { path, host } => {
            if !path_ok(path) {
                return Err(err("transport.path", "Path must start with / and contain no spaces"));
            }
            if let Some(h) = host {
                if !h.is_empty() {
                    check_host(h).map_err(|e| err("transport.host", e.message))?;
                }
            }
            Ok(())
        }
        Transport::Http { path, host } => {
            if !path_ok(path) {
                return Err(err("transport.path", "Path must start with / and contain no spaces"));
            }
            for h in host {
                check_host(h).map_err(|e| err("transport.host", e.message))?;
            }
            Ok(())
        }
        Transport::Grpc { service_name } => {
            if service_name.is_empty()
                || service_name.len() > 128
                || !service_name.chars().all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
            {
                return Err(err("transport.service_name", "Invalid gRPC service name"));
            }
            Ok(())
        }
    }
}

pub fn check_host(h: &str) -> R {
    let h = h.trim();
    if h.is_empty() {
        return Err(err("host", "Address is empty"));
    }
    let unbracketed = h.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = unbracketed.parse::<IpAddr>() {
        if ip.is_unspecified() || ip.is_multicast() || ip == IpAddr::from([255, 255, 255, 255]) {
            return Err(err("host", "That IP address can't be used as a server"));
        }
        return Ok(());
    }
    if h.len() > 253 {
        return Err(err("host", "Host name is too long"));
    }
    let labels: Vec<&str> = h.trim_end_matches('.').split('.').collect();
    for l in &labels {
        if l.is_empty() || l.len() > 63 {
            return Err(err("host", "Host name is not valid"));
        }
        if !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || l.starts_with('-') || l.ends_with('-') {
            return Err(err("host", "Host name is not valid"));
        }
    }
    if labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit())) {
        return Err(err("host", "Host name is not valid"));
    }
    Ok(())
}

fn check_uuid(field: &str, v: &str) -> R {
    uuid::Uuid::parse_str(v.trim()).map(|_| ()).map_err(|_| err(field, "Must be a UUID"))
}

fn check_secret(field: &str, v: &str) -> R {
    if v.is_empty() {
        return Err(err(field, "Required"));
    }
    if v.len() > 512 {
        return Err(err(field, "Too long"));
    }
    if v.chars().any(|c| c.is_control()) {
        return Err(err(field, "Contains control characters"));
    }
    Ok(())
}

fn check_wg_key(field: &str, v: &str) -> R {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(v.trim())
        .map_err(|_| err(field, "WireGuard keys are base64"))?;
    if raw.len() != 32 {
        return Err(err(field, "WireGuard keys are 32 bytes"));
    }
    Ok(())
}

pub fn check_cidr(v: &str) -> Result<(), String> {
    let (ip, len) = match v.split_once('/') {
        Some((ip, len)) => (ip, Some(len)),
        None => (v, None),
    };
    let ip: IpAddr = ip.trim().parse().map_err(|_| format!("\"{v}\" is not an IP address"))?;
    if let Some(len) = len {
        let len: u8 = len.trim().parse().map_err(|_| format!("\"{v}\" has an invalid prefix"))?;
        let max = if ip.is_ipv4() { 32 } else { 128 };
        if len > max {
            return Err(format!("\"{v}\" has an invalid prefix"));
        }
    }
    Ok(())
}

pub fn ss2022_key_len(method: &str) -> Option<usize> {
    match method {
        "2022-blake3-aes-128-gcm" => Some(16),
        "2022-blake3-aes-256-gcm" | "2022-blake3-chacha20-poly1305" => Some(32),
        _ => None,
    }
}

pub fn validate_lotus_options(o: &LotusOptions) -> R {
    if o.provider == DnsProviderId::Custom {
        crate::providers::lotus_resolver(o).map_err(|m| err("custom", m))?;
    }
    Ok(())
}

pub fn validate_shadow_options(o: &ShadowOptions) -> R {
    if o.dns_provider == DnsProviderId::Custom {
        return Err(err("dns_provider", "Custom resolvers are available in LOTUS mode only"));
    }
    if let Some(mtu) = o.mtu {
        if !(1280..=9000).contains(&mtu) {
            return Err(err("mtu", "MTU must be between 1280 and 9000"));
        }
    }
    Ok(())
}
