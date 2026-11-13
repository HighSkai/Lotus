//! "Is my server really what it says it is?" — checks behind the app's console.
//!
//! 1. An authenticated handshake through the real engine (only a server holding
//!    the matching private key can complete REALITY / Hysteria2 / ... auth).
//! 2. What a stranger sees: a plain TLS visit to the server's port with the
//!    profile's SNI. A REALITY server must hand out the camouflage site's own,
//!    valid certificate.
//! 3. Where traffic exits, when SHADOW is connected through this server.

use crate::controller::Controller;
use crate::probe;
use lotus_core::ipc::*;
use lotus_core::*;

fn check(id: &str, title: &str, outcome: CheckOutcome, summary: impl Into<String>, detail: impl Into<String>) -> Check {
    Check { id: id.into(), title: title.into(), outcome, summary: summary.into(), detail: detail.into() }
}

/// SNI a plain visitor would use, and whether the server is REALITY.
fn tls_target(p: &ServerProfile) -> Option<(String, bool)> {
    let host = || p.server().to_string();
    match &p.protocol {
        Protocol::Vless(c) => c.tls.as_ref().map(|t| (t.server_name.clone().unwrap_or_else(host), t.reality.is_some())),
        Protocol::Trojan(c) => Some((c.tls.server_name.clone().unwrap_or_else(host), false)),
        Protocol::AnyTls(c) => Some((c.tls.server_name.clone().unwrap_or_else(host), c.tls.reality.is_some())),
        Protocol::Naive(c) if !c.quic => Some((c.tls.server_name.clone().unwrap_or_else(host), false)),
        _ => None,
    }
}

fn encryption(p: &Protocol) -> &'static str {
    match p {
        Protocol::Vless(c) if c.tls.as_ref().is_some_and(|t| t.reality.is_some()) => {
            "TLS 1.3 (REALITY): X25519 key exchange authenticated with your server's key, AES-GCM or ChaCha20 for data. XTLS-Vision avoids double-encrypting traffic that is already TLS."
        }
        Protocol::Vless(_) | Protocol::Trojan(_) | Protocol::AnyTls(_) => "TLS 1.3 with the server's certificate verified; all traffic is carried inside it.",
        Protocol::Hysteria2(_) | Protocol::Tuic(_) => "QUIC with TLS 1.3; every packet is encrypted and authenticated.",
        Protocol::Naive(_) => "TLS 1.3 using Chromium's own network stack.",
        Protocol::Shadowsocks(_) => "AEAD encryption with your server's key (Shadowsocks).",
        Protocol::WireGuard(_) => "WireGuard: Curve25519 key exchange, ChaCha20-Poly1305 for data.",
    }
}

pub async fn run(ctrl: &Controller, profile: ServerProfile) -> VerifyReport {
    let proto = &profile.protocol;
    let layer = if proto.is_udp_native() { "UDP" } else { "TCP" };
    let protocol = format!("{} over {layer} {}", proto.label(), profile.port());
    let mut checks = Vec::new();

    // 1. Authenticated handshake with the real engine.
    match ctrl.test_profile(profile.clone()).await {
        Ok(t) if t.ok => checks.push(check(
            "handshake",
            "Authenticated handshake",
            CheckOutcome::Pass,
            format!("Your server accepted LOTUS's key{}", t.latency_ms.map(|ms| format!(" ({ms} ms round trip)")).unwrap_or_default()),
            match tls_target(&profile) {
                Some((_, true)) => "REALITY only completes when the server proves it holds the private key matching this profile's public key, so this is your REALITY server and not an impostor.".to_string(),
                _ => format!("The {} handshake completed with your profile's credentials.", proto.label()),
            },
        )),
        Ok(t) => checks.push(check("handshake", "Authenticated handshake", CheckOutcome::Fail, "The server refused the handshake", t.message)),
        Err(e) => checks.push(check("handshake", "Authenticated handshake", CheckOutcome::Fail, "Couldn't complete the handshake", e.message)),
    }

    // 2. What a stranger sees on the server's port.
    match tls_target(&profile) {
        Some((sni, reality)) => match probe::stranger_view(profile.server(), profile.port(), &sni).await {
            Ok(view) => {
                let short = view.fingerprint.get(..16).unwrap_or(&view.fingerprint).to_string();
                if reality {
                    let real = probe::site_fingerprint(&sni).await.ok();
                    let same = real.as_deref() == Some(view.fingerprint.as_str());
                    checks.push(check(
                        "camouflage",
                        "What strangers see",
                        CheckOutcome::Pass,
                        format!("The real {sni}"),
                        format!(
                            "A visitor without your key gets {sni}'s own valid certificate (SHA-256 {short}…){} and HTTP {}. Nothing reveals a proxy.",
                            if same { ", identical to the one the real site serves" } else { "" },
                            view.status
                        ),
                    ));
                } else {
                    checks.push(check("camouflage", "Certificate", CheckOutcome::Pass, format!("Valid for {sni}"), format!("SHA-256 {short}…, verified against public certificate authorities.")));
                }
            }
            Err(e) => checks.push(check(
                "camouflage",
                "What strangers see",
                if reality { CheckOutcome::Warn } else { CheckOutcome::Info },
                "Couldn't probe the server's port",
                e.to_string(),
            )),
        },
        None => checks.push(check("camouflage", "What strangers see", CheckOutcome::Info, "Not applicable", "This protocol doesn't present a website to visitors (QUIC/UDP or a bare cipher).")),
    }

    // 3. Exit address, when SHADOW runs through this server right now.
    let st = ctrl.status();
    if st.mode == Some(Mode::Shadow) && st.state == ConnState::Connected && st.profile_id.as_deref() == Some(profile.id.as_str()) {
        let exit = st.exit.as_ref().and_then(|e| e.ipv4.clone()).unwrap_or_default();
        let server_ip = tokio::net::lookup_host((profile.server(), profile.port())).await.ok().and_then(|mut a| a.next()).map(|a| a.ip().to_string());
        if server_ip.as_deref() == Some(exit.as_str()) {
            checks.push(check("exit", "Exit address", CheckOutcome::Pass, format!("Websites see {exit}"), "That's your server's address, so traffic is leaving through it."));
        } else {
            checks.push(check("exit", "Exit address", CheckOutcome::Info, format!("Websites see {exit}"), "Different from the address LOTUS connects to; normal when the server's outbound traffic uses another IP."));
        }
    } else {
        checks.push(check("exit", "Exit address", CheckOutcome::Info, "Connect SHADOW through this server to check", ""));
    }

    checks.push(check("encryption", "Encryption", CheckOutcome::Info, protocol.clone(), encryption(proto)));
    VerifyReport { protocol, checks, generated_at: now_ms() }
}
