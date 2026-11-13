//! Typed data model shared by the desktop app and the privileged service.
//!
//! The service never accepts raw engine configuration from a client. Everything
//! it runs is generated from these types after [`crate::validate`] has checked
//! them, so a local client cannot smuggle file paths, log outputs or other
//! privileged engine options into a process that runs as SYSTEM.

use serde::{Deserialize, Serialize};

/// A user-supplied exit server ("Bring Your Own Server").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    /// Unix milliseconds.
    #[serde(default)]
    pub created_at: u64,
}

impl ServerProfile {
    pub fn new(name: impl Into<String>, protocol: Protocol) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            protocol,
            created_at: now_ms(),
        }
    }

    pub fn server(&self) -> &str {
        self.protocol.server()
    }

    pub fn port(&self) -> u16 {
        self.protocol.port()
    }
}

pub fn new_profile_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Protocol {
    Vless(VlessConfig),
    Trojan(TrojanConfig),
    Shadowsocks(ShadowsocksConfig),
    Hysteria2(Hysteria2Config),
    Tuic(TuicConfig),
    #[serde(rename = "anytls")]
    AnyTls(AnyTlsConfig),
    Naive(NaiveConfig),
    #[serde(rename = "wireguard")]
    WireGuard(WireGuardConfig),
}

impl Protocol {
    pub fn server(&self) -> &str {
        match self {
            Protocol::Vless(c) => &c.server,
            Protocol::Trojan(c) => &c.server,
            Protocol::Shadowsocks(c) => &c.server,
            Protocol::Hysteria2(c) => &c.server,
            Protocol::Tuic(c) => &c.server,
            Protocol::AnyTls(c) => &c.server,
            Protocol::Naive(c) => &c.server,
            Protocol::WireGuard(c) => &c.server,
        }
    }

    pub fn port(&self) -> u16 {
        match self {
            Protocol::Vless(c) => c.port,
            Protocol::Trojan(c) => c.port,
            Protocol::Shadowsocks(c) => c.port,
            Protocol::Hysteria2(c) => c.port,
            Protocol::Tuic(c) => c.port,
            Protocol::AnyTls(c) => c.port,
            Protocol::Naive(c) => c.port,
            Protocol::WireGuard(c) => c.port,
        }
    }

    /// True when the protocol carries traffic over UDP/QUIC natively.
    pub fn is_udp_native(&self) -> bool {
        matches!(
            self,
            Protocol::Hysteria2(_) | Protocol::Tuic(_) | Protocol::WireGuard(_)
        ) || matches!(self, Protocol::Naive(n) if n.quic)
    }

    /// Short human label, e.g. "VLESS · REALITY · Vision".
    pub fn label(&self) -> String {
        match self {
            Protocol::Vless(c) => {
                let mut parts = vec!["VLESS".to_string()];
                match &c.tls {
                    Some(t) if t.reality.is_some() => parts.push("REALITY".into()),
                    Some(_) => parts.push("TLS".into()),
                    None => {}
                }
                if c.flow.as_deref() == Some(VISION_FLOW) {
                    parts.push("Vision".into());
                }
                if let Some(t) = c.transport.label() {
                    parts.push(t.into());
                }
                parts.join(" · ")
            }
            Protocol::Trojan(c) => match c.transport.label() {
                Some(t) => format!("Trojan · {t}"),
                None => "Trojan".into(),
            },
            Protocol::Shadowsocks(c) => {
                if c.method.starts_with("2022-") {
                    "Shadowsocks 2022".into()
                } else {
                    "Shadowsocks".into()
                }
            }
            Protocol::Hysteria2(_) => "Hysteria2 · QUIC".into(),
            Protocol::Tuic(_) => "TUIC · QUIC".into(),
            Protocol::AnyTls(c) => match &c.tls.reality {
                Some(_) => "AnyTLS · REALITY".into(),
                None => "AnyTLS".into(),
            },
            Protocol::Naive(c) => {
                if c.quic {
                    "NaiveProxy · QUIC".into()
                } else {
                    "NaiveProxy · HTTP/2".into()
                }
            }
            Protocol::WireGuard(_) => "WireGuard".into(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Protocol::Vless(_) => "vless",
            Protocol::Trojan(_) => "trojan",
            Protocol::Shadowsocks(_) => "shadowsocks",
            Protocol::Hysteria2(_) => "hysteria2",
            Protocol::Tuic(_) => "tuic",
            Protocol::AnyTls(_) => "anytls",
            Protocol::Naive(_) => "naive",
            Protocol::WireGuard(_) => "wireguard",
        }
    }
}

pub const VISION_FLOW: &str = "xtls-rprx-vision";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VlessConfig {
    pub server: String,
    pub port: u16,
    pub uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub transport: Transport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrojanConfig {
    pub server: String,
    pub port: u16,
    pub password: String,
    pub tls: TlsConfig,
    #[serde(default)]
    pub transport: Transport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShadowsocksConfig {
    pub server: String,
    pub port: u16,
    pub method: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Hysteria2Config {
    pub server: String,
    pub port: u16,
    pub password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs: Option<Obfs>,
    pub tls: TlsConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up_mbps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Obfs {
    /// "salamander" or "gecko".
    pub kind: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TuicConfig {
    pub server: String,
    pub port: u16,
    pub uuid: String,
    pub password: String,
    #[serde(default = "default_tuic_cc")]
    pub congestion_control: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp_relay_mode: Option<String>,
    pub tls: TlsConfig,
}

fn default_tuic_cc() -> String {
    "bbr".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnyTlsConfig {
    pub server: String,
    pub port: u16,
    pub password: String,
    pub tls: TlsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NaiveConfig {
    pub server: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub tls: TlsConfig,
    #[serde(default)]
    pub quic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireGuardConfig {
    pub server: String,
    pub port: u16,
    pub private_key: String,
    pub peer_public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_shared_key: Option<String>,
    /// Interface addresses with prefix, e.g. "10.66.0.2/32".
    pub local_address: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved: Option<[u8; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepalive: Option<u16>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TlsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alpn: Vec<String>,
    /// uTLS ClientHello fingerprint ("chrome", "firefox", ...). Required for REALITY.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reality: Option<RealityConfig>,
    /// Extra trusted certificate (PEM). Used for self-hosted servers with a private CA
    /// or a self-signed certificate; normal web-PKI verification still applies to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    /// Base64 SHA-256 of the server's SubjectPublicKeyInfo (key pinning).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_key_sha256: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealityConfig {
    pub public_key: String,
    #[serde(default)]
    pub short_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Tcp,
    Ws {
        #[serde(default)]
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
    Grpc {
        service_name: String,
    },
    Http {
        #[serde(default)]
        path: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        host: Vec<String>,
    },
    #[serde(rename = "httpupgrade")]
    HttpUpgrade {
        #[serde(default)]
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
}

impl Transport {
    pub fn label(&self) -> Option<&'static str> {
        match self {
            Transport::Tcp => None,
            Transport::Ws { .. } => Some("WebSocket"),
            Transport::Grpc { .. } => Some("gRPC"),
            Transport::Http { .. } => Some("HTTP/2"),
            Transport::HttpUpgrade { .. } => Some("HTTPUpgrade"),
        }
    }
}

// ---------------------------------------------------------------------------
// Connection options
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Lotus,
    Shadow,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DnsProviderId {
    #[default]
    Cloudflare,
    Quad9,
    Google,
    Mullvad,
    Adguard,
    Custom,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DnsTransport {
    /// DNS-over-HTTPS (HTTP/2 over TCP 443).
    #[default]
    Doh,
    /// DNS-over-HTTPS over HTTP/3 (QUIC, UDP 443).
    Doh3,
    /// DNS-over-TLS (TCP 853).
    Dot,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CustomResolver {
    /// e.g. `https://dns.example.net/dns-query` (DoH) or `tls://dns.example.net` (DoT).
    pub url: String,
    /// IP address used to reach the resolver, so no plaintext bootstrap lookup is needed.
    pub ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LotusOptions {
    pub provider: DnsProviderId,
    pub transport: DnsTransport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<CustomResolver>,
    pub block_trackers: bool,
    pub block_malware: bool,
    /// Block plaintext DNS (port 53) and DoT (853) to any server other than LOTUS,
    /// so applications with hard-coded resolvers cannot leak queries.
    pub block_plain_dns: bool,
    /// Re-enable protection automatically after a reboot.
    pub resume_on_boot: bool,
}

impl Default for LotusOptions {
    fn default() -> Self {
        Self {
            provider: DnsProviderId::Cloudflare,
            transport: DnsTransport::Doh,
            custom: None,
            block_trackers: true,
            block_malware: false,
            block_plain_dns: true,
            resume_on_boot: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ipv6Mode {
    /// Route IPv6 through the tunnel; if the server has no IPv6 it is unreachable, never leaked.
    #[default]
    Tunnel,
    /// Refuse IPv6 entirely (AAAA answers suppressed, IPv6 packets rejected).
    Disable,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuicPolicy {
    /// Block QUIC (UDP 443) only when the tunnel itself runs over TCP, so browsers
    /// fall back to HTTP/2 instead of tunnelling QUIC inside TCP.
    #[default]
    Auto,
    Block,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ShadowOptions {
    pub kill_switch: bool,
    pub allow_lan: bool,
    pub auto_reconnect: bool,
    pub resume_on_boot: bool,
    pub ipv6: Ipv6Mode,
    /// Resolver used for DNS inside the tunnel (queried through the exit server).
    pub dns_provider: DnsProviderId,
    pub block_trackers: bool,
    pub quic: QuicPolicy,
    /// TUN MTU; `None` uses the engine default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u16>,
}

impl Default for ShadowOptions {
    fn default() -> Self {
        Self {
            kill_switch: true,
            allow_lan: true,
            auto_reconnect: true,
            resume_on_boot: false,
            ipv6: Ipv6Mode::Tunnel,
            dns_provider: DnsProviderId::Cloudflare,
            block_trackers: false,
            quic: QuicPolicy::Auto,
            mtu: None,
        }
    }
}
