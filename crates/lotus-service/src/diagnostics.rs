//! Leak and routing diagnostics. Each check explains what it measured and why
//! the result matters; nothing here is simulated.

use crate::controller::Controller;
use crate::probe;
use lotus_core::dnsmsg::{Answer, TYPE_A, TYPE_TXT};
use lotus_core::ipc::*;
use lotus_core::*;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

fn check(id: &str, title: &str, outcome: CheckOutcome, summary: impl Into<String>, detail: impl Into<String>) -> Check {
    Check { id: id.into(), title: title.into(), outcome, summary: summary.into(), detail: detail.into() }
}

#[cfg(windows)]
fn tun_index() -> Option<u32> {
    crate::win::adapters::list().ok()?.into_iter().find(|a| a.friendly_name == singbox::TUN_NAME).map(|a| a.if_index)
}

#[cfg(windows)]
fn system_resolver() -> Option<SocketAddr> {
    let idx = crate::win::adapters::best_interface("1.1.1.1".parse().unwrap())?;
    let list = crate::win::adapters::list().ok()?;
    let a = list.into_iter().find(|a| a.if_index == idx)?;
    a.dns_servers.into_iter().find(|ip| ip.is_ipv4()).map(|ip| SocketAddr::new(ip, 53))
}

fn org(a: &Option<(u32, String)>) -> String {
    match a {
        Some((n, o)) if !o.is_empty() => format!("{o} (AS{n})"),
        Some((n, _)) => format!("AS{n}"),
        None => "an unknown network".into(),
    }
}

pub async fn run(ctrl: &Controller) -> DiagnosticsReport {
    let st = ctrl.status();
    let connected = st.state == ConnState::Connected;
    let mode = if connected { st.mode } else { None };
    let inner = ctrl.inner();
    let origin = inner.origin.lock().unwrap().clone();
    let mut checks = Vec::new();

    #[cfg(windows)]
    let sys_resolver = system_resolver();
    #[cfg(not(windows))]
    let sys_resolver: Option<SocketAddr> = None;
    let resolver: Option<SocketAddr> = match mode {
        Some(Mode::Lotus) => Some("127.0.0.1:53".parse().unwrap()),
        Some(Mode::Shadow) => Some("172.19.0.2:53".parse().unwrap()),
        None => sys_resolver,
    };

    // --- Which resolver really answers? ---------------------------------------
    if let Some(res) = resolver {
        let txt = probe::dns_query(res, "o-o.myaddr.l.google.com", TYPE_TXT, Duration::from_secs(5)).await;
        match txt {
            Ok(r) => {
                let texts: Vec<String> = r.answers.into_iter().filter_map(|a| if let Answer::Txt(t) = a { Some(t) } else { None }).collect();
                let egress = texts.iter().find_map(|t| t.parse::<IpAddr>().ok());
                let ecs = texts.iter().any(|t| t.contains("edns0-client-subnet"));
                let resolver_asn = match egress { Some(ip) => probe::asn_of(ip, res).await, None => None };
                let isp_asn = match origin.ipv4.as_ref().and_then(|s| s.parse::<IpAddr>().ok()) {
                    Some(ip) => probe::asn_of(ip, res).await,
                    None => None,
                };
                let same = matches!((&resolver_asn, &isp_asn), (Some((a, _)), Some((b, _))) if a == b);
                let ecs_note = if ecs { " This resolver forwards part of your IP address (EDNS Client Subnet) to websites' DNS servers." } else { "" };
                let detail = format!(
                    "LOTUS asked Google's diagnostic name o-o.myaddr.l.google.com, which reports the address of the resolver that looked it up: {} from {}. Your internet provider is {}.{ecs_note}",
                    egress.map(|i| i.to_string()).unwrap_or_else(|| "unknown".into()),
                    org(&resolver_asn),
                    org(&isp_asn)
                );
                let (outcome, summary) = match (mode, same) {
                    (Some(_), false) => (CheckOutcome::Pass, format!("DNS answered by {}, not your ISP", org(&resolver_asn))),
                    (Some(_), true) => (CheckOutcome::Fail, "Your internet provider's resolver is answering — DNS leak".to_string()),
                    (None, true) => (CheckOutcome::Warn, "Unprotected: your ISP's resolver sees every site you look up".to_string()),
                    (None, false) => (CheckOutcome::Info, format!("DNS answered by {} (unencrypted while disconnected)", org(&resolver_asn))),
                };
                checks.push(check("dns_resolver", "DNS leak test", outcome, summary, detail));
            }
            Err(e) => checks.push(check("dns_resolver", "DNS leak test", if mode.is_some() { CheckOutcome::Fail } else { CheckOutcome::Info }, "The resolver didn't answer", e.to_string())),
        }
    } else {
        checks.push(check("dns_resolver", "DNS leak test", CheckOutcome::Info, "No DNS server found", "Windows reports no DNS server on the active connection."));
    }

    // --- Can programs bypass it with hard-coded resolvers? --------------------
    let direct = probe::dns_query("8.8.8.8:53".parse().unwrap(), "example.com", TYPE_A, Duration::from_secs(3)).await;
    match mode {
        Some(Mode::Lotus) => {
            let guarded = st.protections.iter().any(|p| p.id == "dns_leak" && p.detail.contains("blocked"));
            match (&direct, guarded) {
                (Err(_), true) => checks.push(check("plain_dns", "Hard-coded DNS", CheckOutcome::Pass, "Blocked", "A plaintext query sent straight to 8.8.8.8 was blocked, so programs can't bypass encrypted DNS.")),
                (Ok(_), true) => checks.push(check("plain_dns", "Hard-coded DNS", CheckOutcome::Fail, "A plaintext query to 8.8.8.8 got through", "The DNS guard should block this. Reconnect LOTUS; if it persists another firewall may be overriding it.")),
                (_, false) => checks.push(check("plain_dns", "Hard-coded DNS", CheckOutcome::Warn, "Not blocked (setting off)", "Programs that ignore Windows' DNS settings can still send plaintext queries. Turn on \u{201c}Block other DNS servers\u{201d}.")),
            }
        }
        Some(Mode::Shadow) => {
            #[cfg(windows)]
            let via_tun = tun_index().is_some() && crate::win::adapters::best_interface("8.8.8.8".parse().unwrap()) == tun_index();
            #[cfg(not(windows))]
            let via_tun = false;
            let outcome = if via_tun { CheckOutcome::Pass } else { CheckOutcome::Fail };
            checks.push(check(
                "plain_dns",
                "Hard-coded DNS",
                outcome,
                if via_tun { "Captured by the tunnel" } else { "Not routed through the tunnel" },
                format!("A plaintext query to 8.8.8.8 {} and Windows routes it via {}.", if direct.is_ok() { "was answered" } else { "got no answer" }, if via_tun { "the LOTUS tunnel, where it's answered over encrypted DNS" } else { "your normal connection" }),
            ));
        }
        None => {}
    }

    // --- IPv4 / IPv6 exit ------------------------------------------------------
    let v4 = probe::trace(probe::TRACE_V4, Duration::from_secs(6)).await;
    let v6 = probe::trace(probe::TRACE_V6, Duration::from_secs(5)).await;
    match (mode, &v4) {
        (Some(Mode::Shadow), Ok(t)) => {
            let local = st.profile_id.is_some() && origin.ipv4.as_deref() == Some(t.ip.as_str());
            if local {
                checks.push(check("ipv4", "IPv4 exit", CheckOutcome::Warn, format!("Exit IP {} equals your own IP", t.ip), "Websites see your normal address. This is expected only when the server runs on your own network."));
            } else {
                checks.push(check("ipv4", "IPv4 exit", CheckOutcome::Pass, format!("Websites see {}{}", t.ip, t.loc.as_ref().map(|l| format!(" ({l})")).unwrap_or_default()), format!("Your own address{} is hidden.", origin.ipv4.as_ref().map(|o| format!(" {o}")).unwrap_or_default())));
            }
        }
        (Some(Mode::Lotus), Ok(t)) => checks.push(check("ipv4", "Public IPv4", CheckOutcome::Info, format!("{} — unchanged by design", t.ip), "LOTUS mode encrypts DNS only. Websites see your normal address; use SHADOW to change it.")),
        (_, Ok(t)) => checks.push(check("ipv4", "Public IPv4", CheckOutcome::Info, t.ip.clone(), "Your current public address.")),
        (_, Err(e)) => checks.push(check("ipv4", "IPv4 exit", if mode == Some(Mode::Shadow) { CheckOutcome::Fail } else { CheckOutcome::Info }, "IPv4 test failed", e.to_string())),
    }
    // A server on this machine/LAN exits with our own addresses by definition.
    let local_exit = matches!((&v4, &origin.ipv4), (Ok(t), Some(o)) if &t.ip == o);
    match (mode, &v6) {
        (Some(Mode::Shadow), Ok(t)) if origin.ipv6.as_deref() == Some(t.ip.as_str()) && local_exit => checks.push(check(
            "ipv6",
            "IPv6 exit",
            CheckOutcome::Warn,
            format!("Exit IPv6 {} equals your own", t.ip),
            "IPv6 is tunnelled, but your server is on your own network, so it exits with your own address.",
        )),
        (Some(Mode::Shadow), Ok(t)) if origin.ipv6.as_deref() == Some(t.ip.as_str()) => {
            checks.push(check("ipv6", "IPv6 leak test", CheckOutcome::Fail, "IPv6 leak: websites see your own IPv6 address", format!("{} is your normal IPv6 address.", t.ip)))
        }
        (Some(Mode::Shadow), Ok(t)) => checks.push(check("ipv6", "IPv6 leak test", CheckOutcome::Pass, format!("IPv6 exits via your server ({})", t.ip), "IPv6 traffic is tunnelled.")),
        (Some(Mode::Shadow), Err(_)) => checks.push(check("ipv6", "IPv6 leak test", CheckOutcome::Pass, "IPv6 blocked — no leak", "Your server has no IPv6 (or IPv6 is disabled), so IPv6 traffic is captured by the tunnel and goes nowhere instead of leaking.")),
        (_, Ok(t)) => checks.push(check("ipv6", "Public IPv6", CheckOutcome::Info, t.ip.clone(), "Your current IPv6 address.")),
        (_, Err(_)) => checks.push(check("ipv6", "Public IPv6", CheckOutcome::Info, "No IPv6 connectivity", "")),
    }

    // --- Routing table and kill switch ----------------------------------------
    #[cfg(windows)]
    if mode == Some(Mode::Shadow) {
        let tun = tun_index();
        let r4 = crate::win::adapters::best_interface("1.1.1.1".parse().unwrap());
        let r6 = crate::win::adapters::best_interface("2606:4700:4700::1111".parse().unwrap());
        let ok4 = tun.is_some() && r4 == tun;
        let ok6 = r6.is_none() || r6 == tun;
        checks.push(check(
            "routing",
            "Tunnel routing",
            if ok4 && ok6 { CheckOutcome::Pass } else { CheckOutcome::Fail },
            if ok4 && ok6 { "Internet routes point to the tunnel" } else { "Some traffic bypasses the tunnel" },
            format!("Windows' best route for IPv4 is {} and for IPv6 is {}.", if ok4 { "the LOTUS adapter" } else { "another adapter" }, if r6.is_none() { "unavailable" } else if r6 == tun { "the LOTUS adapter" } else { "another adapter" }),
        ));
        let ks = inner.kill_switch_active();
        checks.push(check(
            "kill_switch",
            "Kill switch",
            if ks { CheckOutcome::Pass } else { CheckOutcome::Info },
            if ks { "Armed" } else { "Off" },
            if ks { "If the tunnel fails, Windows Filtering Platform rules block direct internet access until it recovers or you disconnect." } else { "If the tunnel fails, traffic falls back to your normal connection." },
        ));
    }

    #[cfg(windows)]
    if mode == Some(Mode::Lotus) {
        let mut missing = Vec::new();
        if let Ok(list) = crate::win::adapters::list() {
            for a in list.iter().filter(|a| a.up && !a.is_loopback()) {
                if crate::win::adapters::get_static_dns(a.guid, false).map(|d| d.trim() != "127.0.0.1").unwrap_or(false) {
                    missing.push(a.friendly_name.clone());
                }
            }
        }
        checks.push(if missing.is_empty() {
            check("dns_config", "Adapter DNS settings", CheckOutcome::Pass, "Every active adapter uses LOTUS", "Your original settings are saved and are restored when you disconnect.")
        } else {
            check("dns_config", "Adapter DNS settings", CheckOutcome::Warn, format!("Not yet applied to: {}", missing.join(", ")), "LOTUS re-applies settings every few seconds; this can show briefly after a network change.")
        });
    }

    DiagnosticsReport { mode, state: st.state, checks, generated_at: now_ms() }
}
