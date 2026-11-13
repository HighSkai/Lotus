//! Scrubbing of engine log lines before they are kept in memory or shown.
//!
//! The engine runs at `warn` level, so per-connection destinations are not
//! logged in the first place. On top of that every line has the server address,
//! credentials and any IP address replaced before it leaves the service.

use crate::model::{Protocol, ServerProfile};

pub struct Redactor {
    secrets: Vec<(String, &'static str)>,
}

impl Redactor {
    pub fn new(profile: Option<&ServerProfile>) -> Self {
        let mut secrets: Vec<(String, &'static str)> = Vec::new();
        if let Some(p) = profile {
            secrets.push((p.server().to_string(), "<server>"));
            let mut add = |s: &str| {
                if s.len() >= 4 {
                    secrets.push((s.to_string(), "<secret>"));
                }
            };
            match &p.protocol {
                Protocol::Vless(c) => add(&c.uuid),
                Protocol::Trojan(c) => add(&c.password),
                Protocol::Shadowsocks(c) => add(&c.password),
                Protocol::Hysteria2(c) => add(&c.password),
                Protocol::Tuic(c) => {
                    add(&c.uuid);
                    add(&c.password)
                }
                Protocol::AnyTls(c) => add(&c.password),
                Protocol::Naive(c) => {
                    add(&c.username);
                    add(&c.password)
                }
                Protocol::WireGuard(c) => {
                    add(&c.private_key);
                    if let Some(k) = &c.pre_shared_key {
                        add(k)
                    }
                }
            }
        }
        // Longest first so overlapping values are fully replaced.
        secrets.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
        Self { secrets }
    }

    pub fn scrub(&self, line: &str) -> String {
        let mut out = strip_ansi(line);
        for (s, repl) in &self.secrets {
            if !s.is_empty() {
                out = out.replace(s.as_str(), repl);
            }
        }
        mask_ips(&out)
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Replace IPv4 addresses and IPv6-looking tokens with placeholders.
fn mask_ips(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, token) in s.split_inclusive(|c: char| c.is_whitespace() || "()[],\"'=<>".contains(c)).enumerate() {
        let _ = i;
        let (core, tail) = split_trailing(token);
        let bare = core.rsplit_once(':').filter(|(h, p)| !h.contains(':') && p.chars().all(|c| c.is_ascii_digit())).map(|(h, _)| h).unwrap_or(core);
        if bare.parse::<std::net::Ipv4Addr>().is_ok() {
            out.push_str(&core.replacen(bare, "<ip>", 1));
        } else if core.matches(':').count() >= 2 && core.trim_matches(|c| c == '[' || c == ']').split(':').all(|g| g.len() <= 4 && g.chars().all(|c| c.is_ascii_hexdigit())) {
            out.push_str("<ip6>");
        } else {
            out.push_str(core);
        }
        out.push_str(tail);
    }
    out
}

fn split_trailing(token: &str) -> (&str, &str) {
    match token.char_indices().last() {
        Some((i, c)) if c.is_whitespace() || "()[],\"'=<>".contains(c) => (&token[..i], &token[i..]),
        _ => (token, ""),
    }
}
