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
    let ctx = EngineContext { clash_port: port, clash_secret: secret.clone(), tracker_ruleset: inner.paths.tracker_rules_str() };
    let listen = LotusListen { v6: with_v6.then(|| "::1".to_string()), ..Default::default() };
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
    inner.update(|s| {
        s.state = ConnState::Connected;
        s.mode = Some(Mode::Lotus);
        s.connected_since = Some(now_ms());
        s.protections = protections.clone();
        s.error = None;
    });
    inner.log("LOTUS connected");
    spawn_origin_refresh(inner.clone(), true);

    let mut events = inner.events.subscribe();
    let mut reconcile = tokio::time::interval(Duration::from_secs(10));
    let mut health = tokio::time::interval(Duration::from_secs(60));
    health.tick().await;
    reconcile.tick().await;
    let mut health_failures = 0u32;
    loop {
        let restart: bool;
        tokio::select! {
            _ = cancel.cancelled() => { eng.stop().await; return Ok(()); }
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
        inner.update(|s| s.state = ConnState::Reconnecting);
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
                    inner.update(|s| s.state = ConnState::Connected);
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

fn shadow_protections(profile: &ServerProfile, opts: &ShadowOptions, inner: &Inner, ipv6: bool) -> Vec<Protection> {
    let dns = providers::resolver_for(opts.dns_provider, DnsTransport::Doh, false).map(|r| r.name).unwrap_or_default();
    let routing = match (ipv6, opts.ipv6) {
        (true, _) => "All TCP and UDP traffic, IPv4 and IPv6",
        (false, Ipv6Mode::Tunnel) => "All TCP and UDP traffic; IPv6 blocked (your server has no IPv6)",
        (false, Ipv6Mode::Disable) => "All TCP and UDP traffic, IPv6 blocked",
    };
    vec![
        Protection::new("tunnel", "Encrypted tunnel", profile.protocol.label(), true),
        Protection::new("routing", "Full-device routing", routing, true),
        Protection::new("dns", "DNS through tunnel", format!("{dns} DNS-over-HTTPS via your server"), true),
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
    let ctx = EngineContext { clash_port: port, clash_secret: secret.clone(), tracker_ruleset: inner.paths.tracker_rules_str() };
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
    let clash = eng.clash.clone();
    let tinner = inner.clone();
    let traffic = tokio::spawn(async move {
        loop {
            let _ = clash
                .traffic(|up, down| {
                    tinner.update(|s| {
                        s.traffic.up_rate = up;
                        s.traffic.down_rate = down;
                        s.traffic.up_total += up;
                        s.traffic.down_total += down;
                    })
                })
                .await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
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
