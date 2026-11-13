//! Connection lifecycles for LOTUS and SHADOW.
//!
//! A session never reports `Connected` until it has verified the real data
//! path: for LOTUS a DNS answer obtained through the encrypted resolver, for
//! SHADOW a handshake with the exit server *and* an HTTPS request that went
//! through the operating system's routing and came back from the exit IP.

use crate::controller::{fail, Inner, SysEvent};
use crate::engine::{self, Engine};
use crate::probe;
use crate::state::SessionRecord;
use lotus_core::ipc::*;
use lotus_core::providers;
use lotus_core::singbox::{self, EngineContext, LotusListen};
use lotus_core::*;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;

const BACKOFF: &[u64] = &[1, 2, 4, 8, 15, 30];

fn ipv6_loopback_available() -> bool {
    std::net::UdpSocket::bind("[::1]:0").is_ok()
}

/// A currently free loopback port for the resolver relay. Without one, LOTUS
/// still works; it just can't count the bytes it encrypts.
fn free_loopback_port(network: &str) -> Option<u16> {
    if network == "udp" {
        std::net::UdpSocket::bind("127.0.0.1:0").and_then(|s| s.local_addr()).map(|a| a.port()).ok()
    } else {
        std::net::TcpListener::bind("127.0.0.1:0").and_then(|s| s.local_addr()).map(|a| a.port()).ok()
    }
}

/// Time lookups the resolver can't answer from LOTUS's cache: random names
/// under the reserved example.com zone. This is the round trip a page pays
/// for a name it hasn't visited recently. One lookup now and then can be slow
/// (a lost packet, a busy resolver), so report the middle of three.
async fn lotus_latency() -> Option<u32> {
    let mut samples = Vec::with_capacity(3);
    for _ in 0..3 {
        let name = format!("lotus-{:08x}.example.com", rand::random::<u32>());
        let t = std::time::Instant::now();
        if probe::dns_query("127.0.0.1:53".parse().unwrap(), &name, lotus_core::dnsmsg::TYPE_A, Duration::from_secs(4)).await.is_ok() {
            samples.push(t.elapsed().as_millis().max(1) as u32);
        }
    }
    median(&mut samples)
}

/// Middle value (lower middle for an even count); `None` when empty.
fn median(v: &mut [u32]) -> Option<u32> {
    v.sort_unstable();
    v.get(v.len().saturating_sub(1) / 2).copied()
}

#[cfg(test)]
mod meter_tests {
    use super::TunnelMeter;

    fn c(id: &str, tunnel: bool, up: u64, down: u64) -> (String, bool, u64, u64) {
        (id.into(), tunnel, up, down)
    }

    #[test]
    fn counts_only_tunnel_connections() {
        let mut m = TunnelMeter::new(1.0);
        // First sample: a tunnelled and a direct connection.
        assert_eq!(m.step(&[c("a", true, 100, 1000), c("b", false, 50, 500)], (150, 1500)), (100, 1000));
        // Both grow; only the tunnel's growth counts.
        assert_eq!(m.step(&[c("a", true, 300, 4000), c("b", false, 80, 9000)], (380, 13000)), (200, 3000));
    }

    #[test]
    fn closed_connections_are_shared_by_the_observed_split() {
        let mut m = TunnelMeter::new(1.0);
        m.step(&[c("a", true, 0, 0)], (0, 0));
        // Observed: 300 down tunnelled, 100 direct (75%). Another 400 down came
        // from connections that opened and closed between samples.
        let (_, down) = m.step(&[c("a", true, 0, 300), c("b", false, 0, 100)], (0, 800));
        assert_eq!(down, 300 + 300);
    }

    #[test]
    fn engine_restart_never_goes_negative() {
        let mut m = TunnelMeter::new(0.0);
        m.step(&[c("a", true, 500, 500)], (500, 500));
        // New engine: totals restart from zero.
        assert_eq!(m.step(&[c("z", true, 10, 20)], (10, 20)), (10, 20));
    }
}

#[cfg(test)]
mod tests {
    use super::median;

    #[test]
    fn median_ignores_one_slow_sample() {
        assert_eq!(median(&mut [12, 26, 11]), Some(12));
        assert_eq!(median(&mut [40, 12]), Some(12));
        assert_eq!(median(&mut [9]), Some(9));
        assert_eq!(median(&mut []), None);
    }
}

/// Counts the bytes that took the tunnel when some traffic deliberately
/// doesn't (split tunnelling, DNS only). The engine reports per-connection
/// counters for open connections plus a global total; bytes of connections
/// that opened and closed between two samples show up only in the total, and
/// are split between tunnel and direct in the proportion just observed.
#[derive(Debug)]
pub struct TunnelMeter {
    seen: std::collections::HashMap<String, (u64, u64)>,
    total: Option<(u64, u64)>,
    share: (f64, f64),
}

/// One open connection: id, whether it uses the tunnel, bytes up, bytes down.
pub type ConnSample = (String, bool, u64, u64);

impl TunnelMeter {
    /// `tunnel_share` is the guess for bytes seen before any connection has
    /// been observed: most traffic tunnelled (bypass list) or not (only list).
    pub fn new(tunnel_share: f64) -> Self {
        Self { seen: Default::default(), total: None, share: (tunnel_share, tunnel_share) }
    }

    /// Bytes (up, down) secured since the previous sample.
    pub fn step(&mut self, conns: &[ConnSample], total: (u64, u64)) -> (u64, u64) {
        let (mut tun, mut all) = ((0u64, 0u64), (0u64, 0u64));
        let mut next = std::collections::HashMap::with_capacity(conns.len());
        for (id, tunnel, up, down) in conns {
            let (pu, pd) = self.seen.get(id).copied().unwrap_or((0, 0));
            let (du, dd) = (up.saturating_sub(pu), down.saturating_sub(pd));
            all = (all.0 + du, all.1 + dd);
            if *tunnel {
                tun = (tun.0 + du, tun.1 + dd);
            }
            next.insert(id.clone(), (*up, *down));
        }
        self.seen = next;
        let global = match self.total.replace(total) {
            Some(p) => (total.0.saturating_sub(p.0), total.1.saturating_sub(p.1)),
            None => all,
        };
        if all.0 > 0 {
            self.share.0 = tun.0 as f64 / all.0 as f64;
        }
        if all.1 > 0 {
            self.share.1 = tun.1 as f64 / all.1 as f64;
        }
        let unseen = (global.0.saturating_sub(all.0), global.1.saturating_sub(all.1));
        (tun.0 + (unseen.0 as f64 * self.share.0).round() as u64, tun.1 + (unseen.1 as f64 * self.share.1).round() as u64)
    }
}

fn conn_samples(v: &serde_json::Value) -> (Vec<ConnSample>, (u64, u64)) {
    let num = |v: &serde_json::Value| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
    let conns = v["connections"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| {
                    let tunnel = c["chains"].as_array().is_some_and(|ch| ch.iter().any(|x| x == singbox::PROXY_TAG));
                    (c["id"].as_str().unwrap_or_default().to_string(), tunnel, num(&c["upload"]), num(&c["download"]))
                })
                .collect()
        })
        .unwrap_or_default();
    (conns, (num(&v["uploadTotal"]), num(&v["downloadTotal"])))
}

fn spawn_tunnel_traffic(inner: &Arc<Inner>, clash: engine::Clash, opts: &ShadowOptions) -> tokio::task::JoinHandle<()> {
    let inner = inner.clone();
    let share = if opts.encrypt == EncryptScope::Dns || opts.split.mode == SplitMode::Only { 0.0 } else { 1.0 };
    tokio::spawn(async move {
        let mut meter = TunnelMeter::new(share);
        // Four samples a second: a transfer has to open and close inside
        // 250 ms to be missed, and then only its share is estimated.
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last = std::time::Instant::now();
        let (mut up, mut down) = (0u64, 0u64);
        loop {
            tick.tick().await;
            let Ok(v) = clash.connections().await else { continue };
            let (conns, total) = conn_samples(&v);
            let (u, d) = meter.step(&conns, total);
            up += u;
            down += d;
            // Publish once a second, like the plain counter.
            let secs = last.elapsed().as_secs_f64();
            if secs < 0.95 {
                continue;
            }
            last = std::time::Instant::now();
            let (u, d) = (std::mem::take(&mut up), std::mem::take(&mut down));
            inner.update(|s| {
                s.traffic.up_rate = (u as f64 / secs) as u64;
                s.traffic.down_rate = (d as f64 / secs) as u64;
                s.traffic.up_total += u;
                s.traffic.down_total += d;
            });
        }
    })
}

/// Feed the engine's per-second byte counters into the published status.
/// Totals accumulate across engine restarts within one session.
fn spawn_traffic(inner: &Arc<Inner>, clash: engine::Clash) -> tokio::task::JoinHandle<()> {
    let inner = inner.clone();
    tokio::spawn(async move {
        loop {
            let _ = clash
                .traffic(|up, down| {
                    inner.update(|s| {
                        s.traffic.up_rate = up;
                        s.traffic.down_rate = down;
                        s.traffic.up_total += up;
                        s.traffic.down_total += down;
                    })
                })
                .await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}

async fn debounce(rx: &mut tokio::sync::broadcast::Receiver<SysEvent>, first: SysEvent, wait: Duration) -> SysEvent {
    let mut ev = first;
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(SysEvent::Resumed)) => ev = SysEvent::Resumed,
            Ok(Ok(_)) | Ok(Err(RecvError::Lagged(_))) => {}
            Ok(Err(RecvError::Closed)) | Err(_) => return ev,
        }
    }
}

// ===========================================================================
// LOTUS
// ===========================================================================

pub async fn run_lotus(inner: Arc<Inner>, opts: LotusOptions, cancel: CancellationToken, keep: Arc<AtomicBool>, _resumed: bool) {
    inner.set_connecting(Mode::Lotus, None, false);
    let res = lotus_session(&inner, &opts, &cancel).await;
    inner.cleanup_system();
    finish(&inner, Mode::Lotus, res, &keep);
}

fn finish(inner: &Inner, mode: Mode, res: Result<(), ErrorInfo>, keep: &AtomicBool) {
    *inner.clash.lock().unwrap() = None;
    match res {
        Ok(()) => inner.update(|s| {
            s.state = ConnState::Disconnected;
            s.error = None;
            s.connected_since = None;
            s.latency_ms = None;
            s.traffic = Traffic::default();
            s.protections.clear();
        }),
        Err(e) => {
            inner.log(format!("{mode:?} session ended: {}", e.message));
            inner.update(|s| {
                s.state = ConnState::Error;
                s.mode = Some(mode);
                s.error = Some(e);
                s.connected_since = None;
                s.latency_ms = None;
                s.traffic = Traffic::default();
                s.protections.clear();
            });
            if !keep.load(Ordering::SeqCst) {
                crate::state::clear_session(&inner.paths.session_file);
            }
        }
    }
}

async fn start_lotus_engine(inner: &Inner, opts: &LotusOptions, with_v6: bool) -> Result<Engine, ErrorInfo> {
    let (port, secret) = engine::new_controller_endpoint().map_err(|e| fail("internal", e.to_string()))?;
    let ctx = EngineContext { clash_port: port, clash_secret: secret.clone(), tracker_ruleset: inner.paths.tracker_rules_str(), dns_relay_port: None };
    let network = providers::lotus_resolver(opts).map(|r| singbox::resolver_network(r.transport)).unwrap_or("tcp");
    let listen = LotusListen { v6: with_v6.then(|| "::1".to_string()), relay_port: free_loopback_port(network), ..Default::default() };
    let cfg = singbox::lotus_config(opts, &ctx, &listen).map_err(|e| fail("invalid_options", e))?;
    let mut eng = inner.spawn_engine(&cfg, port, secret, None).await?;
    if let Err(e) = eng.wait_ready(Duration::from_secs(10)).await {
        eng.stop().await;
        return Err(fail("engine", e.to_string()));
    }
    // Prove the encrypted upstream answers before touching system DNS.
    let local: SocketAddr = "127.0.0.1:53".parse().unwrap();
    let mut last = String::new();
    for _ in 0..3 {
        match probe::dns_query(local, "cloudflare.com", lotus_core::dnsmsg::TYPE_A, Duration::from_secs(4)).await {
            Ok(r) if r.rcode == 0 && !r.answers.is_empty() => return Ok(eng),
            Ok(r) => last = format!("resolver answered with code {}", r.rcode),
            Err(e) => last = e.to_string(),
        }
        if eng.has_exited() {
            break;
        }
    }
    let logs = eng.recent_logs();
    eng.stop().await;
    let hint = logs.iter().rev().find(|l| l.contains("ERROR") || l.contains("error")).cloned().unwrap_or(last);
    Err(fail("dns_upstream", format!("The encrypted DNS server couldn't be reached, so your DNS settings were left unchanged. ({})", hint.split_once("engine: ").map(|x| x.1).unwrap_or(&hint))))
}

fn lotus_protections(opts: &LotusOptions) -> Vec<Protection> {
    let r = providers::lotus_resolver(opts).ok();
    let transport = match r.as_ref().map(|r| r.transport) {
        Some(DnsTransport::Doh3) => "DNS-over-HTTPS/3",
        Some(DnsTransport::Dot) => "DNS-over-TLS",
        _ => "DNS-over-HTTPS",
    };
    let name = r.as_ref().map(|r| r.name.clone()).unwrap_or_default();
    let mut v = vec![Protection::new("encrypted_dns", "Encrypted DNS", format!("{name} · {transport}"), true)];
    v.push(Protection::new(
        "dns_leak",
        "DNS leak protection",
        if opts.block_plain_dns { "All adapters use LOTUS; other DNS servers are blocked" } else { "All adapters use LOTUS" },
        true,
    ));
    v.push(Protection::new("trackers", "Tracker blocking", if opts.block_trackers { "Known ad & tracking domains" } else { "Off" }, opts.block_trackers));
    let malware = r.as_ref().map(|r| r.filters_malware).unwrap_or(false);
    v.push(Protection::new("malware", "Malicious-domain filter", if malware { "Filtered by resolver" } else { "Off" }, malware));
    v
}

async fn lotus_session(inner: &Arc<Inner>, opts: &LotusOptions, cancel: &CancellationToken) -> Result<(), ErrorInfo> {
    let with_v6 = ipv6_loopback_available();
    let mut eng = tokio::select! {
        _ = cancel.cancelled() => return Ok(()),
        r = start_lotus_engine(inner, opts, with_v6) => r?,
    };
    *inner.clash.lock().unwrap() = Some(eng.clash.clone());
    if opts.block_plain_dns {
        if let Err(e) = inner.enable_dns_guard() {
            eng.stop().await;
            return Err(e);
        }
    }
    if let Err(e) = inner.apply_dns(with_v6) {
        eng.stop().await;
        return Err(e);
    }
    inner.save_record(SessionRecord { mode: Mode::Lotus, lotus: Some(opts.clone()), profile: None, shadow: None, boot_epoch: inner.boot_epoch(), resume_on_boot: opts.resume_on_boot });
    let protections = lotus_protections(opts);
    let first_latency = lotus_latency().await;
    inner.update(|s| {
        s.state = ConnState::Connected;
        s.mode = Some(Mode::Lotus);
        s.connected_since = Some(now_ms());
        s.protections = protections.clone();
        s.error = None;
        s.latency_ms = first_latency;
    });
    inner.log("LOTUS connected");
    spawn_origin_refresh(inner.clone(), true);
    // Aborted when this session ends, whichever way it ends.
    struct Abort(tokio::task::JoinHandle<()>);
    impl Drop for Abort {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let mut traffic = Abort(spawn_traffic(inner, eng.clash.clone()));

    let mut events = inner.events.subscribe();
    let mut reconcile = tokio::time::interval(Duration::from_secs(10));
    let mut health = tokio::time::interval(Duration::from_secs(60));
    let mut latency = tokio::time::interval(Duration::from_secs(20));
    health.tick().await;
    reconcile.tick().await;
    latency.tick().await;
    let mut health_failures = 0u32;
    loop {
        let restart: bool;
        tokio::select! {
            _ = cancel.cancelled() => { eng.stop().await; return Ok(()); }
            _ = latency.tick() => {
                if let Some(ms) = lotus_latency().await { inner.update(|s| s.latency_ms = Some(ms)); }
                restart = false;
            }
            why = eng.exited() => { inner.log(format!("LOTUS engine stopped: {why}")); restart = true; }
            ev = events.recv() => {
                if let Ok(ev) = ev { debounce(&mut events, ev, Duration::from_millis(1500)).await; }
                let _ = inner.apply_dns(with_v6);
                restart = false;
            }
            _ = reconcile.tick() => { let _ = inner.apply_dns(with_v6); restart = false; }
            _ = health.tick() => {
                let ok = probe::dns_query("127.0.0.1:53".parse().unwrap(), "cloudflare.com", lotus_core::dnsmsg::TYPE_A, Duration::from_secs(5)).await.map(|r| r.rcode == 0).unwrap_or(false);
                health_failures = if ok { 0 } else { health_failures + 1 };
                restart = health_failures >= 2;
            }
        }
        if !restart {
            continue;
        }
        health_failures = 0;
        inner.update(|s| {
            s.state = ConnState::Reconnecting;
            s.latency_ms = None;
            s.traffic.up_rate = 0;
            s.traffic.down_rate = 0;
        });
        traffic.0.abort();
        eng.stop().await;
        let mut attempt = 0usize;
        loop {
            if attempt >= 3 {
                return Err(fail("dns_stopped", "Encrypted DNS stopped working, so LOTUS restored your normal DNS settings to keep you online."));
            }
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = tokio::time::sleep(Duration::from_secs(BACKOFF[attempt])) => {}
            }
            match start_lotus_engine(inner, opts, with_v6).await {
                Ok(e) => {
                    eng = e;
                    *inner.clash.lock().unwrap() = Some(eng.clash.clone());
                    traffic = Abort(spawn_traffic(inner, eng.clash.clone()));
                    let ms = lotus_latency().await;
                    inner.update(|s| {
                        s.state = ConnState::Connected;
                        s.latency_ms = ms;
                    });
                    break;
                }
                Err(e) => inner.log(format!("LOTUS restart failed: {}", e.message)),
            }
            attempt += 1;
        }
    }
}

/// Measure the direct (non-tunnel) public IP. In LOTUS mode it is shown as the
/// unchanged exit; in SHADOW it is the reference used for leak detection.
fn spawn_origin_refresh(inner: Arc<Inner>, publish_as_exit: bool) {
    tokio::spawn(async move {
        let v4 = probe::trace(probe::TRACE_V4, Duration::from_secs(5)).await.ok();
        let v6 = probe::trace(probe::TRACE_V6, Duration::from_secs(4)).await.ok();
        {
            let mut o = inner.origin.lock().unwrap();
            if let Some(t) = &v4 {
                o.ipv4 = Some(t.ip.clone());
                o.country = t.loc.clone();
            }
            o.ipv6 = v6.as_ref().map(|t| t.ip.clone()).or(o.ipv6.clone());
            o.measured = Some(std::time::Instant::now());
        }
        if publish_as_exit {
            inner.update(|s| {
                if s.mode == Some(Mode::Lotus) {
                    s.exit = Some(ExitInfo { ipv4: v4.as_ref().map(|t| t.ip.clone()), ipv6: v6.as_ref().map(|t| t.ip.clone()), country: v4.as_ref().and_then(|t| t.loc.clone()), colo: None });
                }
            });
        }
    });
}

// ===========================================================================
// SHADOW
// ===========================================================================

pub async fn run_shadow(inner: Arc<Inner>, profile: ServerProfile, opts: ShadowOptions, cancel: CancellationToken, keep: Arc<AtomicBool>, resumed: bool) {
    inner.set_connecting(Mode::Shadow, Some(&profile), false);
    *inner.shadow_opts.lock().unwrap() = Some(opts.clone());
    let res = shadow_session(&inner, &profile, &opts, &cancel, resumed).await;
    inner.cleanup_system();
    finish(&inner, Mode::Shadow, res, &keep);
}

struct Live {
    engine: Engine,
    traffic: tokio::task::JoinHandle<()>,
    /// IPv6 is actually tunnelled (the server has IPv6); otherwise it is blocked cleanly.
    ipv6: bool,
}

impl Live {
    async fn stop(self) {
        self.traffic.abort();
        self.engine.stop().await;
    }
}

/// "2 apps and 3 sites", "1 app", ...
pub fn split_summary(s: &SplitTunnel) -> String {
    let n = |k: usize, one: &str, many: &str| format!("{k} {}", if k == 1 { one } else { many });
    match (s.apps.len(), s.sites.len()) {
        (a, 0) => n(a, "app", "apps"),
        (0, x) => n(x, "site", "sites"),
        (a, x) => format!("{} and {}", n(a, "app", "apps"), n(x, "site", "sites")),
    }
}

fn shadow_protections(profile: &ServerProfile, opts: &ShadowOptions, inner: &Inner, ipv6: bool) -> Vec<Protection> {
    let dns = providers::resolver_for(opts.dns_provider, DnsTransport::Doh, false).map(|r| r.name).unwrap_or_default();
    let base = match (ipv6, opts.ipv6) {
        (true, _) => "All TCP and UDP traffic, IPv4 and IPv6",
        (false, Ipv6Mode::Tunnel) => "All TCP and UDP traffic; IPv6 blocked (your server has no IPv6)",
        (false, Ipv6Mode::Disable) => "All TCP and UDP traffic, IPv6 blocked",
    };
    let (label, routing) = if opts.encrypt == EncryptScope::Dns {
        ("DNS-only routing", "Only DNS lookups use the tunnel; other traffic uses your normal connection".to_string())
    } else if opts.split_in_effect() && opts.split.mode == SplitMode::Only {
        ("Split tunnel", format!("Only {} use the tunnel", split_summary(&opts.split)))
    } else if opts.split_in_effect() {
        ("Split tunnel", format!("{base}, except {} that skip the tunnel", split_summary(&opts.split)))
    } else {
        ("Full-device routing", base.to_string())
    };
    let dns_row = if opts.encrypt == EncryptScope::Traffic {
        Protection::new("dns", "Encrypted DNS", format!("{dns} DNS-over-HTTPS, straight from this PC"), true)
    } else {
        Protection::new("dns", "DNS through tunnel", format!("{dns} DNS-over-HTTPS via your server"), true)
    };
    vec![
        Protection::new("tunnel", "Encrypted tunnel", profile.protocol.label(), true),
        Protection::new("routing", label, routing, true),
        dns_row,
        Protection::new("kill_switch", "Kill switch", if opts.kill_switch { "Blocks traffic if the tunnel drops" } else { "Off" }, opts.kill_switch && inner.kill_switch_active()),
        Protection::new("trackers", "Tracker blocking", if opts.block_trackers { "Known ad & tracking domains" } else { "Off" }, opts.block_trackers),
    ]
}

/// Start the engine with IPv6 tunnelled or blocked, and require a real
/// handshake with the exit server. Returns the engine and the measured latency.
async fn start_tunnel(inner: &Arc<Inner>, profile: &ServerProfile, opts: &ShadowOptions, ipv6: bool) -> Result<(Engine, u32), ErrorInfo> {
    let mut eff = opts.clone();
    eff.ipv6 = if ipv6 { Ipv6Mode::Tunnel } else { Ipv6Mode::Disable };
    let (port, secret) = engine::new_controller_endpoint().map_err(|e| fail("internal", e.to_string()))?;
    let relay = if opts.encrypt == EncryptScope::Dns { free_loopback_port("tcp") } else { None };
    let ctx = EngineContext { clash_port: port, clash_secret: secret.clone(), tracker_ruleset: inner.paths.tracker_rules_str(), dns_relay_port: relay };
    let cfg = singbox::shadow_config(profile, &eff, &ctx).map_err(|e| fail("invalid_profile", e))?;
    let mut eng = inner.spawn_engine(&cfg, port, secret, Some(profile)).await?;
    if let Err(e) = eng.wait_ready(Duration::from_secs(12)).await {
        eng.stop().await;
        return Err(fail("engine", e.to_string()));
    }
    match eng.clash.delay(singbox::PROXY_TAG, probe::DELAY_URL, 8000).await {
        Ok(ms) => Ok((eng, ms)),
        Err(e) => {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let msg = crate::explain::outbound_failure(&e.to_string(), &eng.recent_logs());
            eng.stop().await;
            Err(fail("server_unreachable", msg))
        }
    }
}

async fn connect_once(inner: &Arc<Inner>, profile: &ServerProfile, opts: &ShadowOptions) -> Result<Live, ErrorInfo> {
    // 1. The exit server accepts our handshake.
    //
    // IPv6 is only tunnelled when the server itself can reach the IPv6
    // internet. Many VPS images (e.g. Google Cloud by default) have no IPv6;
    // tunnelling it there makes apps that prefer IPv6 fail with "connection
    // closed" instead of falling back to IPv4. Until a server is known to
    // have IPv6, start with IPv6 blocked (always safe, never leaks), test
    // IPv6 egress through the server, and switch it on if it works.
    let want_v6 = opts.ipv6 == Ipv6Mode::Tunnel;
    let known = inner.server_ipv6.lock().unwrap().get(&profile.id).copied();
    let mut ipv6 = want_v6 && known == Some(true);
    let (mut eng, mut latency) = start_tunnel(inner, profile, opts, ipv6).await?;
    if want_v6 && known.is_none() {
        let has_v6 = eng.clash.delay(singbox::PROXY_TAG, probe::IPV6_EGRESS_URL, 5000).await.is_ok();
        inner.server_ipv6.lock().unwrap().insert(profile.id.clone(), has_v6);
        inner.log(if has_v6 { "exit server has IPv6; tunnelling IPv6" } else { "exit server has no IPv6; blocking IPv6 so apps use IPv4" });
        if has_v6 {
            eng.stop().await;
            let (e, ms) = start_tunnel(inner, profile, opts, true).await?;
            eng = e;
            latency = ms;
            ipv6 = true;
        }
    }
    // 2. Windows actually routes ordinary traffic through the tunnel.
    let mut trace = None;
    for _ in 0..3 {
        match probe::trace(probe::TRACE_V4, Duration::from_secs(6)).await {
            Ok(t) => {
                trace = Some(t);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
        if eng.has_exited() {
            break;
        }
    }
    let Some(trace) = trace else {
        let logs = eng.recent_logs();
        eng.stop().await;
        let detail = if logs.is_empty() { String::new() } else { format!(" ({})", crate::explain::engine_failure(&logs)) };
        return Err(fail("routing", format!("The server answered, but Windows traffic isn't flowing through the tunnel{detail}.")));
    };
    let origin_v4 = inner.origin.lock().unwrap().ipv4.clone();
    if origin_v4.as_deref() == Some(trace.ip.as_str()) && !crate::controller::Inner::server_is_local(profile) {
        eng.stop().await;
        return Err(fail("leak", "Traffic was still leaving through your normal connection, so SHADOW was not switched on."));
    }
    inner.update(|s| {
        s.exit = Some(ExitInfo { ipv4: Some(trace.ip.clone()), ipv6: None, country: trace.loc.clone(), colo: trace.colo.clone() });
        s.latency_ms = Some(latency);
    });
    // Background: IPv6 exit and per-second traffic counters.
    {
        let inner = inner.clone();
        tokio::spawn(async move {
            let v6 = probe::trace(probe::TRACE_V6, Duration::from_secs(5)).await.ok();
            inner.update(|s| {
                if let Some(x) = s.exit.as_mut() {
                    x.ipv6 = v6.map(|t| t.ip);
                }
            });
        });
    }
    // With some traffic deliberately outside the tunnel, the engine's totals
    // include it; count only what took the tunnel.
    let traffic = if opts.partial() { spawn_tunnel_traffic(inner, eng.clash.clone(), opts) } else { spawn_traffic(inner, eng.clash.clone()) };
    *inner.clash.lock().unwrap() = Some(eng.clash.clone());
    Ok(Live { engine: eng, traffic, ipv6 })
}

async fn shadow_session(inner: &Arc<Inner>, profile: &ServerProfile, opts: &ShadowOptions, cancel: &CancellationToken, resumed: bool) -> Result<(), ErrorInfo> {
    // Reference IP for leak detection (skipped while a kill switch is blocking direct traffic).
    if !inner.kill_switch_active() && !resumed {
        let v4 = probe::trace(probe::TRACE_V4, Duration::from_secs(3)).await.ok();
        let mut o = inner.origin.lock().unwrap();
        if let Some(t) = v4 {
            o.ipv4 = Some(t.ip);
            o.country = t.loc;
            o.measured = Some(std::time::Instant::now());
        }
    }
    if opts.kill_switch {
        inner.enable_kill_switch(opts.allow_lan)?;
    } else if inner.kill_switch_active() {
        inner.disable_guards();
    }
    let first = tokio::select! {
        _ = cancel.cancelled() => return Ok(()),
        r = connect_once(inner, profile, opts) => r,
    };
    let mut live = match first {
        Ok(l) => Some(l),
        // After a crash/reboot keep trying (the kill switch keeps traffic safe meanwhile).
        Err(e) if resumed && opts.auto_reconnect => {
            inner.log(format!("resume attempt failed: {}", e.message));
            None
        }
        Err(e) => return Err(e),
    };
    let record = SessionRecord {
        mode: Mode::Shadow,
        lotus: None,
        profile: Some(profile.clone()),
        shadow: Some(opts.clone()),
        boot_epoch: inner.boot_epoch(),
        resume_on_boot: opts.resume_on_boot,
    };
    inner.save_record(record);
    if let Some(l) = &live {
        mark_connected(inner, profile, opts, l.ipv6);
    }

    let mut events = inner.events.subscribe();
    let mut health = tokio::time::interval(Duration::from_secs(30));
    health.tick().await;
    let mut failures = 0u32;
    loop {
        if live.is_none() {
            match reconnect(inner, profile, opts, cancel).await {
                Some(l) => {
                    let v6 = l.ipv6;
                    live = Some(l);
                    failures = 0;
                    mark_connected(inner, profile, opts, v6);
                }
                None => return Ok(()), // cancelled
            }
        }
        let l = live.as_mut().unwrap();
        let clash = l.engine.clash.clone();
        let mut lost = false;
        // `biased`: cancellation and engine death always win over the (very
        // frequent) network-change notifications a dying TUN adapter produces.
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                live.take().unwrap().stop().await;
                return Ok(());
            }
            why = l.engine.exited() => {
                inner.log(format!("SHADOW engine stopped: {why}"));
                lost = true;
            }
            _ = health.tick() => {
                tokio::select! {
                    biased;
                    _ = l.engine.exited() => lost = true,
                    r = clash.delay(singbox::PROXY_TAG, probe::DELAY_URL, 6000) => match r {
                        Ok(ms) => { failures = 0; inner.update(|s| s.latency_ms = Some(ms)); }
                        Err(_) => { failures += 1; lost = failures >= 2; }
                    },
                }
            }
            ev = events.recv() => {
                if l.engine.has_exited() {
                    lost = true;
                } else {
                    let ev = match ev { Ok(ev) => ev, Err(_) => SysEvent::NetworkChanged };
                    if ev == SysEvent::Resumed {
                        inner.log("resumed from sleep; re-establishing tunnel");
                        lost = true;
                    } else {
                        // Let the network settle, but keep watching the engine meanwhile.
                        tokio::select! {
                            biased;
                            _ = l.engine.exited() => lost = true,
                            ev = debounce(&mut events, ev, Duration::from_secs(2)) => {
                                if ev == SysEvent::Resumed {
                                    lost = true;
                                } else {
                                    tokio::select! {
                                        biased;
                                        _ = l.engine.exited() => lost = true,
                                        r = clash.delay(singbox::PROXY_TAG, probe::DELAY_URL, 6000) => {
                                            if r.is_err() {
                                                inner.log("network changed and the tunnel stopped responding");
                                                lost = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if lost {
            inner.update(|s| {
                s.state = ConnState::Reconnecting;
                s.latency_ms = None;
                s.traffic.up_rate = 0;
                s.traffic.down_rate = 0;
            });
            live.take().unwrap().stop().await;
            if !opts.auto_reconnect {
                // Policy: no automatic reconnect. Keep the kill switch (if any) until the user acts.
                inner.update(|s| {
                    s.state = ConnState::Error;
                    s.error = Some(fail(
                        "connection_lost",
                        if opts.kill_switch { "The tunnel dropped. The kill switch is blocking internet access until you reconnect or turn SHADOW off." } else { "The tunnel dropped. Your traffic is using your normal connection." },
                    ));
                    s.latency_ms = None;
                    s.traffic.up_rate = 0;
                    s.traffic.down_rate = 0;
                });
                cancel.cancelled().await;
                return Ok(());
            }
        }
    }
}

fn mark_connected(inner: &Inner, profile: &ServerProfile, opts: &ShadowOptions, ipv6: bool) {
    let prot = shadow_protections(profile, opts, inner, ipv6);
    inner.update(|s| {
        s.state = ConnState::Connected;
        s.mode = Some(Mode::Shadow);
        s.error = None;
        s.connected_since = Some(now_ms());
        s.protections = prot;
        s.kill_switch_engaged = opts.kill_switch;
    });
    inner.log("SHADOW connected");
}

async fn reconnect(inner: &Arc<Inner>, profile: &ServerProfile, opts: &ShadowOptions, cancel: &CancellationToken) -> Option<Live> {
    inner.update(|s| {
        s.state = ConnState::Reconnecting;
        s.latency_ms = None;
        s.traffic.up_rate = 0;
        s.traffic.down_rate = 0;
    });
    let mut attempt = 0usize;
    loop {
        let wait = BACKOFF[attempt.min(BACKOFF.len() - 1)];
        tokio::select! {
            _ = cancel.cancelled() => return None,
            _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
        }
        let r = tokio::select! {
            _ = cancel.cancelled() => return None,
            r = connect_once(inner, profile, opts) => r,
        };
        match r {
            Ok(l) => return Some(l),
            Err(e) => {
                inner.log(format!("reconnect attempt {} failed: {}", attempt + 1, e.message));
                inner.update(|s| s.error = Some(e));
            }
        }
        attempt += 1;
    }
}
