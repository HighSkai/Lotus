//! Generation of sing-box (1.14) configurations from validated LOTUS types.
//!
//! Only these builders produce engine configuration. They never emit file
//! outputs, external UIs, cache files or any other option that would let the
//! privileged engine read or write arbitrary paths.

use crate::model::*;
use crate::providers::{self, Resolver};
use serde_json::{json, Map, Value};

/// Engine release the builders target. Bumped together with `scripts/fetch-engine.ps1`.
pub const ENGINE_VERSION: &str = "1.14.4";

/// Addresses of the SHADOW TUN interface (link-local style private ranges).
pub const TUN_V4: &str = "172.19.0.1/30";
pub const TUN_V6: &str = "fdfe:dcba:9876::1/126";
pub const TUN_NAME: &str = "LOTUS Shadow";
pub const PROXY_TAG: &str = "proxy";

/// Plain DNS resolver used only to look up the *server's* host name before the
/// tunnel exists. It is DoH over the physical interface, so nothing leaks in plaintext.
const BOOTSTRAP: (&str, &str) = ("1.1.1.1", "cloudflare-dns.com");

/// Runtime values the service injects (never supplied by clients).
#[derive(Debug, Clone)]
pub struct EngineContext {
    pub clash_port: u16,
    pub clash_secret: String,
    /// Absolute path of the bundled tracker rule-set (`.srs`).
    pub tracker_ruleset: Option<String>,
    /// Loopback port for the SHADOW "DNS only" relay (see `shadow_config`).
    pub dns_relay_port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct LotusListen {
    pub v4: String,
    pub v6: Option<String>,
    pub port: u16,
    /// Loopback port of the resolver relay. When set, the encrypted resolver
    /// is reached through a local direct inbound, so its bytes pass through
    /// the engine's router and show up in the traffic counters (DNS transports
    /// dial on their own and are otherwise never counted).
    pub relay_port: Option<u16>,
}

impl Default for LotusListen {
    fn default() -> Self {
        Self { v4: "127.0.0.1".into(), v6: Some("::1".into()), port: 53, relay_port: None }
    }
}

/// Port the encrypted resolver listens on for a transport.
pub fn resolver_port(t: DnsTransport) -> u16 {
    match t {
        DnsTransport::Dot => 853,
        DnsTransport::Doh | DnsTransport::Doh3 => 443,
    }
}

/// Network the relay must carry for a transport (DoH3 is QUIC).
pub fn resolver_network(t: DnsTransport) -> &'static str {
    match t {
        DnsTransport::Doh3 => "udp",
        DnsTransport::Doh | DnsTransport::Dot => "tcp",
    }
}

fn clash_api(ctx: &EngineContext) -> Value {
    json!({
        "clash_api": {
            "external_controller": format!("127.0.0.1:{}", ctx.clash_port),
            "secret": ctx.clash_secret,
        }
    })
}

pub fn dns_server(tag: &str, r: &Resolver, detour: Option<&str>) -> Value {
    let mut v = match r.transport {
        DnsTransport::Doh => json!({"type": "https", "tag": tag, "server": r.ip, "path": r.path, "tls": {"enabled": true, "server_name": r.host}}),
        DnsTransport::Doh3 => json!({"type": "h3", "tag": tag, "server": r.ip, "path": r.path, "tls": {"enabled": true, "server_name": r.host}}),
        DnsTransport::Dot => json!({"type": "tls", "tag": tag, "server": r.ip, "tls": {"enabled": true, "server_name": r.host}}),
    };
    if let Some(d) = detour {
        v["detour"] = json!(d);
    }
    v
}

fn bootstrap_server() -> Value {
    json!({"type": "https", "tag": "dns-bootstrap", "server": BOOTSTRAP.0, "tls": {"enabled": true, "server_name": BOOTSTRAP.1}})
}

fn tracker_rule_set(ctx: &EngineContext) -> Option<Value> {
    ctx.tracker_ruleset
        .as_ref()
        .map(|p| json!({"type": "local", "tag": "trackers", "format": "binary", "path": p}))
}

// ---------------------------------------------------------------------------
// LOTUS: local encrypted resolver, no tunnel, original public IP preserved.
// ---------------------------------------------------------------------------

pub fn lotus_config(opts: &LotusOptions, ctx: &EngineContext, listen: &LotusListen) -> Result<Value, String> {
    let r = providers::lotus_resolver(opts)?;
    let mut dns_rules = Vec::new();
    let mut rule_sets = Vec::new();
    if opts.block_trackers {
        if let Some(rs) = tracker_rule_set(ctx) {
            rule_sets.push(rs);
            dns_rules.push(json!({"rule_set": ["trackers"], "action": "predefined", "rcode": "NXDOMAIN"}));
        }
    }
    let mut inbounds = vec![json!({"type": "direct", "tag": "dns-in", "listen": listen.v4, "listen_port": listen.port})];
    let mut inbound_tags = vec!["dns-in"];
    if let Some(v6) = &listen.v6 {
        inbounds.push(json!({"type": "direct", "tag": "dns-in6", "listen": v6, "listen_port": listen.port}));
        inbound_tags.push("dns-in6");
    }
    let mut rules = vec![json!({"inbound": inbound_tags, "action": "hijack-dns"})];
    let mut server = dns_server("dns-encrypted", &r, None);
    if let Some(relay) = listen.relay_port {
        inbounds.push(json!({
            "type": "direct",
            "tag": "resolver-relay",
            "listen": "127.0.0.1",
            "listen_port": relay,
            "network": resolver_network(r.transport),
            "override_address": r.ip,
            "override_port": resolver_port(r.transport),
        }));
        rules.push(json!({"inbound": ["resolver-relay"], "action": "route", "outbound": "direct"}));
        server["server"] = json!("127.0.0.1");
        server["server_port"] = json!(relay);
    }
    let mut route = json!({"rules": rules, "final": "direct"});
    if !rule_sets.is_empty() {
        route["rule_set"] = Value::Array(rule_sets);
    }
    Ok(json!({
        "log": {"level": "warn", "timestamp": false},
        "dns": {
            "servers": [server],
            "rules": dns_rules,
            "final": "dns-encrypted",
            "cache_capacity": 4096,
        },
        "inbounds": inbounds,
        "outbounds": [{"type": "direct", "tag": "direct"}],
        "route": route,
        "experimental": clash_api(ctx),
    }))
}

// ---------------------------------------------------------------------------
// SHADOW: full-device tunnel through the user's own server.
// ---------------------------------------------------------------------------

pub fn shadow_config(profile: &ServerProfile, opts: &ShadowOptions, ctx: &EngineContext) -> Result<Value, String> {
    let remote = providers::resolver_for(opts.dns_provider, DnsTransport::Doh, false)?;
    let ipv6 = opts.ipv6 == Ipv6Mode::Tunnel;

    let mut tun = json!({
        "type": "tun",
        "tag": "tun-in",
        "interface_name": TUN_NAME,
        "address": [TUN_V4, TUN_V6],
        "auto_route": true,
        "strict_route": true,
    });
    if let Some(mtu) = opts.mtu {
        tun["mtu"] = json!(mtu);
    }

    let dns_only = opts.encrypt == EncryptScope::Dns;
    let split = if opts.split_in_effect() { Some(SplitMatch::new(&opts.split)?) } else { None };
    let only = split.is_some() && opts.split.mode == SplitMode::Only;

    let mut rules = vec![json!({"action": "sniff"}), json!({"protocol": "dns", "action": "hijack-dns"})];
    // LOTUS's own checks (handshake, routing proof, latency) always take the
    // tunnel, whatever the split settings send elsewhere.
    let mut probes = vec!["1.1.1.1/32"];
    if ipv6 {
        probes.push("2606:4700:4700::1111/128");
    }
    rules.push(json!({"ip_cidr": probes, "network": "tcp", "port": 443, "outbound": PROXY_TAG}));
    if opts.allow_lan {
        rules.push(json!({"ip_is_private": true, "outbound": "direct"}));
    }
    let mut dns_rules = Vec::new();
    let mut rule_sets = Vec::new();
    if opts.block_trackers {
        if let Some(rs) = tracker_rule_set(ctx) {
            rule_sets.push(rs);
            dns_rules.push(json!({"rule_set": ["trackers"], "action": "predefined", "rcode": "NXDOMAIN"}));
            rules.push(json!({"rule_set": ["trackers"], "action": "reject"}));
        }
    }

    let block_quic = !dns_only
        && match opts.quic {
            QuicPolicy::Block => true,
            QuicPolicy::Allow => false,
            QuicPolicy::Auto => !profile.protocol.is_udp_native(),
        };
    // Limits that exist because of the tunnel (QUIC inside TCP, IPv6 the
    // server can't carry) apply to tunnelled traffic only.
    let mut tunnel_limits = Vec::new();
    if block_quic {
        tunnel_limits.push(json!({"network": "udp", "port": 443, "action": "reject"}));
    }
    if !ipv6 && !dns_only {
        tunnel_limits.push(json!({"ip_version": 6, "action": "reject"}));
    }
    match &split {
        Some(m) if only => {
            for matcher in m.matchers() {
                for limit in &tunnel_limits {
                    rules.push(merge(&matcher, limit));
                }
                rules.push(merge(&matcher, &json!({"outbound": PROXY_TAG})));
            }
        }
        Some(m) => {
            for matcher in m.matchers() {
                rules.push(merge(&matcher, &json!({"outbound": "direct"})));
            }
            rules.extend(tunnel_limits);
        }
        None if !dns_only => rules.extend(tunnel_limits),
        None => {}
    }
    let route_final = if dns_only || only { "direct" } else { PROXY_TAG };

    // DNS: through the server unless the user chose "traffic only"; sites
    // that skip the tunnel resolve directly so they get nearby servers.
    let direct = providers::resolver_for(opts.dns_provider, DnsTransport::Doh, false)?;
    let mut remote_server = dns_server("dns-remote", &remote, Some(PROXY_TAG));
    if let (true, Some(relay)) = (dns_only, ctx.dns_relay_port) {
        // Only DNS takes the tunnel: send it via a loopback relay so its bytes
        // pass the router and count as secured.
        remote_server = dns_server("dns-remote", &remote, None);
        remote_server["server"] = json!("127.0.0.1");
        remote_server["server_port"] = json!(relay);
        rules.insert(0, json!({"inbound": ["dns-relay"], "outbound": PROXY_TAG}));
    }
    let mut dns_servers = vec![remote_server, dns_server("dns-direct", &direct, None), bootstrap_server()];
    if let Some(m) = split.as_ref().filter(|_| !only).and_then(|m| m.domain_rule()) {
        dns_rules.push(merge(&m, &json!({"server": "dns-direct"})));
    }
    if let Some(m) = split.as_ref().filter(|_| only && opts.encrypt == EncryptScope::Traffic).and_then(|m| m.domain_rule()) {
        dns_rules.push(merge(&m, &json!({"server": "dns-remote"})));
    }
    let dns_final = if opts.encrypt == EncryptScope::Traffic { "dns-direct" } else { "dns-remote" };
    if dns_final != "dns-direct" && !dns_rules.iter().any(|r| r["server"] == "dns-direct") {
        dns_servers.retain(|s| s["tag"] != "dns-direct");
    }

    let mut inbounds = vec![tun];
    if let (true, Some(relay)) = (dns_only, ctx.dns_relay_port) {
        inbounds.push(json!({
            "type": "direct",
            "tag": "dns-relay",
            "listen": "127.0.0.1",
            "listen_port": relay,
            "network": "tcp",
            "override_address": remote.ip,
            "override_port": resolver_port(DnsTransport::Doh),
        }));
    }

    let (outbound, is_endpoint) = proxy_outbound(profile)?;
    let mut cfg = json!({
        "log": {"level": "warn", "timestamp": false},
        "dns": {
            "servers": dns_servers,
            "rules": dns_rules,
            "final": dns_final,
            "strategy": if ipv6 || dns_only { "prefer_ipv4" } else { "ipv4_only" },
            "cache_capacity": 4096,
        },
        "inbounds": inbounds,
        "outbounds": [{"type": "direct", "tag": "direct"}],
        "route": {
            "rules": rules,
            "final": route_final,
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-bootstrap",
        },
        "experimental": clash_api(ctx),
    });
    if !rule_sets.is_empty() {
        cfg["route"]["rule_set"] = Value::Array(rule_sets);
    }
    if split.as_ref().is_some_and(|m| !m.domains.is_empty()) {
        // Lets site rules match connections the sniffer can't name (plain
        // UDP, games), using the answer the app was given.
        cfg["dns"]["reverse_mapping"] = json!(true);
    }
    attach_proxy(&mut cfg, outbound, is_endpoint);
    Ok(cfg)
}

/// Split-tunnel entries in the engine's terms.
struct SplitMatch {
    /// Case-insensitive match on the program's file name, in any folder.
    apps: Vec<String>,
    domains: Vec<String>,
    cidrs: Vec<String>,
}

impl SplitMatch {
    fn new(s: &SplitTunnel) -> Result<Self, String> {
        crate::validate::validate_split(s).map_err(|e| e.to_string())?;
        let mut m = SplitMatch { apps: Vec::new(), domains: Vec::new(), cidrs: Vec::new() };
        for a in &s.apps {
            let escaped: String = a.chars().flat_map(|c| if c.is_ascii_punctuation() { vec!['\\', c] } else { vec![c] }).collect();
            m.apps.push(format!("(?i)(^|\\\\){escaped}$"));
        }
        for x in &s.sites {
            match crate::validate::parse_split_site(x)? {
                crate::validate::SplitSite::Domain(d) => m.domains.push(d),
                crate::validate::SplitSite::Cidr(c) => m.cidrs.push(c),
            }
        }
        Ok(m)
    }

    /// One rule body per kind: the engine ANDs different kinds within a rule,
    /// so apps and sites need separate rules to mean "app OR site".
    fn matchers(&self) -> Vec<Value> {
        let mut out = Vec::new();
        if !self.apps.is_empty() {
            out.push(json!({"process_path_regex": self.apps}));
        }
        let mut site = Map::new();
        if !self.domains.is_empty() {
            site.insert("domain_suffix".into(), json!(self.domains));
        }
        if !self.cidrs.is_empty() {
            site.insert("ip_cidr".into(), json!(self.cidrs));
        }
        if !site.is_empty() {
            out.push(Value::Object(site));
        }
        out
    }

    fn domain_rule(&self) -> Option<Value> {
        (!self.domains.is_empty()).then(|| json!({"domain_suffix": self.domains}))
    }
}

fn merge(a: &Value, b: &Value) -> Value {
    let mut m = a.as_object().cloned().unwrap_or_default();
    if let Some(b) = b.as_object() {
        for (k, v) in b {
            m.insert(k.clone(), v.clone());
        }
    }
    Value::Object(m)
}

/// A minimal configuration used to test a server without touching routing:
/// no inbounds, just the proxy and the controller for an on-demand delay test.
pub fn probe_config(profile: &ServerProfile, ctx: &EngineContext) -> Result<Value, String> {
    let (outbound, is_endpoint) = proxy_outbound(profile)?;
    let mut cfg = json!({
        "log": {"level": "warn", "timestamp": false},
        "dns": {"servers": [bootstrap_server()], "final": "dns-bootstrap"},
        "outbounds": [{"type": "direct", "tag": "direct"}],
        "route": {"final": PROXY_TAG, "auto_detect_interface": true, "default_domain_resolver": "dns-bootstrap"},
        "experimental": clash_api(ctx),
    });
    attach_proxy(&mut cfg, outbound, is_endpoint);
    Ok(cfg)
}

fn attach_proxy(cfg: &mut Value, outbound: Value, is_endpoint: bool) {
    if is_endpoint {
        cfg["endpoints"] = json!([outbound]);
    } else if let Some(arr) = cfg["outbounds"].as_array_mut() {
        arr.insert(0, outbound);
    }
}

fn tls_json(t: &TlsConfig, quic_default_alpn: bool) -> Value {
    let mut m = Map::new();
    m.insert("enabled".into(), json!(true));
    if let Some(sn) = t.server_name.as_ref().filter(|s| !s.is_empty()) {
        m.insert("server_name".into(), json!(sn));
    }
    if !t.alpn.is_empty() {
        m.insert("alpn".into(), json!(t.alpn));
    } else if quic_default_alpn {
        m.insert("alpn".into(), json!(["h3"]));
    }
    if let Some(fp) = &t.fingerprint {
        m.insert("utls".into(), json!({"enabled": true, "fingerprint": fp}));
    }
    if let Some(r) = &t.reality {
        m.insert("reality".into(), json!({"enabled": true, "public_key": r.public_key, "short_id": r.short_id}));
    }
    if let Some(pem) = &t.certificate {
        let lines: Vec<&str> = pem.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        m.insert("certificate".into(), json!(lines));
    }
    if !t.public_key_sha256.is_empty() {
        m.insert("certificate_public_key_sha256".into(), json!(t.public_key_sha256));
    }
    Value::Object(m)
}

fn transport_json(t: &Transport) -> Option<Value> {
    match t {
        Transport::Tcp => None,
        Transport::Ws { path, host } => {
            let mut v = json!({"type": "ws", "path": if path.is_empty() { "/" } else { path }});
            if let Some(h) = host.as_ref().filter(|h| !h.is_empty()) {
                v["headers"] = json!({"Host": h});
            }
            Some(v)
        }
        Transport::Grpc { service_name } => Some(json!({"type": "grpc", "service_name": service_name})),
        Transport::Http { path, host } => {
            let mut v = json!({"type": "http"});
            if !path.is_empty() {
                v["path"] = json!(path);
            }
            if !host.is_empty() {
                v["host"] = json!(host);
            }
            Some(v)
        }
        Transport::HttpUpgrade { path, host } => {
            let mut v = json!({"type": "httpupgrade", "path": if path.is_empty() { "/" } else { path }});
            if let Some(h) = host.as_ref().filter(|h| !h.is_empty()) {
                v["host"] = json!(h);
            }
            Some(v)
        }
    }
}

/// Returns the proxy definition and whether it is an endpoint (WireGuard).
pub fn proxy_outbound(p: &ServerProfile) -> Result<(Value, bool), String> {
    crate::validate::validate_profile(p).map_err(|e| e.to_string())?;
    let base = |ty: &str, server: &str, port: u16| {
        json!({"type": ty, "tag": PROXY_TAG, "server": server, "server_port": port, "domain_resolver": "dns-bootstrap"})
    };
    let v = match &p.protocol {
        Protocol::Vless(c) => {
            let mut v = base("vless", &c.server, c.port);
            v["uuid"] = json!(c.uuid);
            if let Some(f) = c.flow.as_ref().filter(|f| !f.is_empty()) {
                v["flow"] = json!(f);
            }
            v["packet_encoding"] = json!("xudp");
            if let Some(t) = &c.tls {
                v["tls"] = tls_json(t, false);
            }
            if let Some(t) = transport_json(&c.transport) {
                v["transport"] = t;
            }
            v
        }
        Protocol::Trojan(c) => {
            let mut v = base("trojan", &c.server, c.port);
            v["password"] = json!(c.password);
            v["tls"] = tls_json(&c.tls, false);
            if let Some(t) = transport_json(&c.transport) {
                v["transport"] = t;
            }
            v
        }
        Protocol::Shadowsocks(c) => {
            let mut v = base("shadowsocks", &c.server, c.port);
            v["method"] = json!(c.method);
            v["password"] = json!(c.password);
            v
        }
        Protocol::Hysteria2(c) => {
            let mut v = base("hysteria2", &c.server, c.port);
            v["password"] = json!(c.password);
            if let Some(o) = &c.obfs {
                v["obfs"] = json!({"type": o.kind, "password": o.password});
            }
            if let Some(u) = c.up_mbps {
                v["up_mbps"] = json!(u);
            }
            if let Some(d) = c.down_mbps {
                v["down_mbps"] = json!(d);
            }
            v["tls"] = tls_json(&c.tls, true);
            v
        }
        Protocol::Tuic(c) => {
            let mut v = base("tuic", &c.server, c.port);
            v["uuid"] = json!(c.uuid);
            v["password"] = json!(c.password);
            v["congestion_control"] = json!(c.congestion_control);
            if let Some(m) = &c.udp_relay_mode {
                v["udp_relay_mode"] = json!(m);
            }
            v["tls"] = tls_json(&c.tls, true);
            v
        }
        Protocol::AnyTls(c) => {
            let mut v = base("anytls", &c.server, c.port);
            v["password"] = json!(c.password);
            v["tls"] = tls_json(&c.tls, false);
            v
        }
        Protocol::Naive(c) => {
            let mut v = base("naive", &c.server, c.port);
            v["username"] = json!(c.username);
            v["password"] = json!(c.password);
            if c.quic {
                v["quic"] = json!(true);
            }
            let mut tls = Map::new();
            tls.insert("enabled".into(), json!(true));
            if let Some(sn) = c.tls.server_name.as_ref().filter(|s| !s.is_empty()) {
                tls.insert("server_name".into(), json!(sn));
            }
            if let Some(pem) = &c.tls.certificate {
                tls.insert("certificate".into(), json!(pem.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>()));
            }
            v["tls"] = Value::Object(tls);
            v
        }
        Protocol::WireGuard(c) => {
            let mut peer = json!({
                "address": c.server,
                "port": c.port,
                "public_key": c.peer_public_key,
                "allowed_ips": ["0.0.0.0/0", "::/0"],
                "persistent_keepalive_interval": c.keepalive.unwrap_or(25),
            });
            if let Some(psk) = &c.pre_shared_key {
                peer["pre_shared_key"] = json!(psk);
            }
            if let Some(r) = c.reserved {
                peer["reserved"] = json!(r);
            }
            let v = json!({
                "type": "wireguard",
                "tag": PROXY_TAG,
                "address": c.local_address,
                "private_key": c.private_key,
                "mtu": c.mtu.unwrap_or(1408),
                "peers": [peer],
                "domain_resolver": "dns-bootstrap",
            });
            return Ok((v, true));
        }
    };
    Ok((v, false))
}
