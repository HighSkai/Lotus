//! Built-in encrypted DNS resolvers.
//!
//! Every resolver is contacted by IP address with the provider's TLS server name,
//! so no plaintext bootstrap lookup is ever needed. Certificates are verified
//! normally against the system/web PKI.

use crate::model::{DnsProviderId, DnsTransport, LotusOptions};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Resolver {
    pub name: String,
    pub ip: String,
    pub host: String,
    pub path: String,
    pub transport: DnsTransport,
    /// True when the resolver itself filters malicious domains.
    pub filters_malware: bool,
    /// True when the resolver itself filters ads/trackers.
    pub filters_ads: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderInfo {
    pub id: DnsProviderId,
    pub name: &'static str,
    pub doh3: bool,
    pub dot: bool,
    /// Whether a malware-filtering variant exists.
    pub malware_variant: bool,
    pub note: &'static str,
}

pub const PROVIDERS: &[ProviderInfo] = &[
    ProviderInfo {
        id: DnsProviderId::Cloudflare,
        name: "Cloudflare",
        doh3: true,
        dot: true,
        malware_variant: true,
        note: "Fastest global anycast. Malware filter uses 1.1.1.2.",
    },
    ProviderInfo {
        id: DnsProviderId::Quad9,
        name: "Quad9",
        doh3: false,
        dot: true,
        malware_variant: true,
        note: "Swiss non-profit. Always blocks known malicious domains.",
    },
    ProviderInfo {
        id: DnsProviderId::Mullvad,
        name: "Mullvad",
        doh3: false,
        dot: true,
        malware_variant: true,
        note: "No-logging resolver. Malware filter also blocks ads & trackers.",
    },
    ProviderInfo {
        id: DnsProviderId::Adguard,
        name: "AdGuard",
        doh3: true,
        dot: true,
        malware_variant: true,
        note: "Filtering variant blocks ads, trackers and phishing.",
    },
    ProviderInfo {
        id: DnsProviderId::Google,
        name: "Google",
        doh3: true,
        dot: true,
        malware_variant: false,
        note: "Google Public DNS. No filtering variant.",
    },
];

pub fn provider_info(id: DnsProviderId) -> Option<&'static ProviderInfo> {
    PROVIDERS.iter().find(|p| p.id == id)
}

struct Entry {
    ip: &'static str,
    host: &'static str,
    path: &'static str,
    malware: bool,
    ads: bool,
}

fn entry(id: DnsProviderId, filtered: bool) -> Option<Entry> {
    use DnsProviderId::*;
    let e = match (id, filtered) {
        (Cloudflare, false) => Entry { ip: "1.1.1.1", host: "cloudflare-dns.com", path: "/dns-query", malware: false, ads: false },
        (Cloudflare, true) => Entry { ip: "1.1.1.2", host: "security.cloudflare-dns.com", path: "/dns-query", malware: true, ads: false },
        (Quad9, _) => Entry { ip: "9.9.9.9", host: "dns.quad9.net", path: "/dns-query", malware: true, ads: false },
        (Google, _) => Entry { ip: "8.8.8.8", host: "dns.google", path: "/dns-query", malware: false, ads: false },
        (Mullvad, false) => Entry { ip: "194.242.2.2", host: "dns.mullvad.net", path: "/dns-query", malware: false, ads: false },
        (Mullvad, true) => Entry { ip: "194.242.2.4", host: "base.dns.mullvad.net", path: "/dns-query", malware: true, ads: true },
        (Adguard, false) => Entry { ip: "94.140.14.140", host: "unfiltered.adguard-dns.com", path: "/dns-query", malware: false, ads: false },
        (Adguard, true) => Entry { ip: "94.140.14.14", host: "dns.adguard-dns.com", path: "/dns-query", malware: true, ads: true },
        (Custom, _) => return None,
    };
    Some(e)
}

/// Resolve the upstream for LOTUS mode. Falls back to DoH when the provider
/// does not offer the requested transport.
pub fn lotus_resolver(opts: &LotusOptions) -> Result<Resolver, String> {
    if opts.provider == DnsProviderId::Custom {
        let c = opts
            .custom
            .as_ref()
            .ok_or_else(|| "Custom resolver selected but not configured".to_string())?;
        return custom_resolver(&c.url, &c.ip);
    }
    resolver_for(opts.provider, opts.transport, opts.block_malware)
}

pub fn resolver_for(id: DnsProviderId, transport: DnsTransport, block_malware: bool) -> Result<Resolver, String> {
    let info = provider_info(id).ok_or_else(|| "Unknown DNS provider".to_string())?;
    let e = entry(id, block_malware && info.malware_variant).ok_or_else(|| "Unknown DNS provider".to_string())?;
    let transport = match transport {
        DnsTransport::Doh3 if !info.doh3 => DnsTransport::Doh,
        DnsTransport::Dot if !info.dot => DnsTransport::Doh,
        t => t,
    };
    Ok(Resolver {
        name: info.name.to_string(),
        ip: e.ip.to_string(),
        host: e.host.to_string(),
        path: e.path.to_string(),
        transport,
        filters_malware: e.malware,
        filters_ads: e.ads,
    })
}

pub fn custom_resolver(url: &str, ip: &str) -> Result<Resolver, String> {
    let ip: std::net::IpAddr = ip
        .trim()
        .parse()
        .map_err(|_| "Custom resolver needs a valid IP address".to_string())?;
    let parsed = url::Url::parse(url.trim()).map_err(|_| "Custom resolver URL is not valid".to_string())?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "Custom resolver URL needs a host name".to_string())?
        .trim_matches(|c| c == '[' || c == ']')
        .to_string();
    crate::validate::check_host(&host).map_err(|e| e.to_string())?;
    let transport = match parsed.scheme() {
        "https" => DnsTransport::Doh,
        "h3" => DnsTransport::Doh3,
        "tls" => DnsTransport::Dot,
        other => return Err(format!("Unsupported resolver scheme \"{other}\". Use https://, h3:// or tls://")),
    };
    let path = if transport == DnsTransport::Dot {
        String::new()
    } else if parsed.path().is_empty() || parsed.path() == "/" {
        "/dns-query".to_string()
    } else {
        parsed.path().to_string()
    };
    if parsed.port().is_some() {
        return Err("Custom resolver ports are not supported; use the standard port".into());
    }
    Ok(Resolver {
        name: host.clone(),
        ip: ip.to_string(),
        host,
        path,
        transport,
        filters_malware: false,
        filters_ads: false,
    })
}
