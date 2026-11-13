//! Turn engine log lines into messages a non-technical user can act on.

fn find<'a>(logs: &'a [String], needles: &[&str]) -> Option<&'a String> {
    logs.iter().rev().find(|l| {
        let low = l.to_ascii_lowercase();
        needles.iter().any(|n| low.contains(n))
    })
}

pub fn engine_failure(logs: &[String]) -> String {
    if find(logs, &["only one usage of each socket address", "address already in use", "bind: "]).is_some() {
        return "Another program is already using the DNS port (53) or a port LOTUS needs. Close other DNS/VPN tools and try again.".into();
    }
    if find(logs, &["wintun", "create tun", "configure tun", "open interface"]).is_some() {
        return "Windows refused to create the LOTUS network adapter. Restart the LOTUS service or reboot, then try again.".into();
    }
    if find(logs, &["cronet"]).is_some() {
        return "The NaiveProxy component (libcronet.dll) is missing. Reinstall LOTUS.".into();
    }
    if let Some(l) = find(logs, &["fatal", "error"]) {
        return format!("The networking engine stopped: {}", tidy(l));
    }
    "The networking engine stopped unexpectedly.".into()
}

/// Explain why the proxy outbound could not reach the server.
pub fn outbound_failure(err: &str, logs: &[String]) -> String {
    let all: Vec<String> = logs.iter().cloned().chain(std::iter::once(err.to_string())).collect();
    let has = |n: &[&str]| find(&all, n).is_some();
    if has(&["reality verification failed", "reality: ", "processed invalid connection"]) {
        return "The server rejected the REALITY handshake. Check the public key, short ID and camouflage server name (SNI).".into();
    }
    if has(&["x509", "certificate", "pinned", "public key"]) {
        return "The server's TLS certificate couldn't be verified, so LOTUS didn't connect. Check the server name or certificate pin.".into();
    }
    if has(&["connection refused", "actively refused"]) {
        return "The server refused the connection. Make sure the server software is running on that port.".into();
    }
    if has(&["no such host", "lookup ", "no address"]) {
        return "The server's host name couldn't be resolved.".into();
    }
    if has(&["authentication failed", "invalid user", "bad password", "wrong password", "unauthorized"]) {
        return "The server rejected the credentials (UUID or password).".into();
    }
    if has(&["timeout", "deadline exceeded", "timed out"]) {
        return "The server didn't respond in time. Check the address and port, and that the server firewall allows it.".into();
    }
    if has(&["network is unreachable", "no route"]) {
        return "Your computer has no route to the server. Check your internet connection.".into();
    }
    format!("Couldn't reach the server: {}", tidy(err))
}

fn tidy(l: &str) -> String {
    let l = l.trim();
    let l = l.split_once("engine: ").map(|(_, r)| r).unwrap_or(l);
    let l = l.trim_start_matches("FATAL").trim_start_matches("ERROR").trim();
    let l = l.trim_start_matches(|c: char| c == '[' || c.is_ascii_digit() || c == ']').trim();
    l.chars().take(180).collect()
}
