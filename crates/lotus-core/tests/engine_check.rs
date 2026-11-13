//! Validates every generated configuration with the real engine (`sing-box check`).
//! Skips (with a message) when the engine binary has not been fetched yet.

use lotus_core::singbox::*;
use lotus_core::*;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn engine() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LOTUS_ENGINE") {
        return Some(PathBuf::from(p));
    }
    let p = root().join("app/src-tauri/binaries/sing-box-x86_64-pc-windows-msvc.exe");
    p.exists().then_some(p)
}

fn ctx() -> EngineContext {
    let rs = root().join("app/src-tauri/resources/block-trackers.srs");
    EngineContext {
        clash_port: 39090,
        clash_secret: "test-secret".into(),
        tracker_ruleset: rs.exists().then(|| rs.canonicalize().unwrap().to_string_lossy().trim_start_matches(r"\\?\").to_string()),
        dns_relay_port: Some(39092),
    }
}

fn check(name: &str, cfg: &serde_json::Value) {
    let Some(engine) = engine() else {
        eprintln!("skipping engine check for {name}: engine not fetched (run scripts/fetch-engine.ps1)");
        return;
    };
    // libcronet.dll (NaiveProxy) ships next to the engine in the installed app;
    // in the source tree it lives in resources/.
    let res = root().join("app/src-tauri/resources");
    let path = format!("{};{}", res.display(), std::env::var("PATH").unwrap_or_default());
    let mut child = Command::new(engine)
        .env("PATH", path)
        .args(["check", "-c", "stdin", "--disable-color"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn engine");
    child.stdin.take().unwrap().write_all(serde_json::to_string(cfg).unwrap().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "engine rejected {name}:\n{}\n{}\nconfig: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        serde_json::to_string_pretty(cfg).unwrap()
    );
}

const PBK: &str = "uowCJZ1KWCUrql7F9wgtzYtEUXBiJNlbeJjNis8fB1U";
const UUID: &str = "0c7c4baa-301f-4eef-ac81-e785d343d7fa";
const PIN: &str = "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=";
/// Self-signed ECDSA P-256 test certificate (CN/SAN lotus.internal), generated exactly as the server installer does.
const TEST_CERT: &str = include_str!("data/hy2-test.crt");

/// The engine refuses a key pin combined with a trusted certificate at start-up
/// (not at `check` time), so validation must reject the combination up front.
#[test]
fn hysteria2_pin_and_certificate_are_exclusive() {
    let mut p = profiles().into_iter().find(|p| p.name == "hy2-trusted-cert").unwrap();
    if let Protocol::Hysteria2(c) = &mut p.protocol {
        c.tls.public_key_sha256 = vec![PIN.into()];
    }
    let e = lotus_core::validate::validate_profile(&p).unwrap_err();
    assert!(e.message.contains("not both"), "{e}");
}

fn profiles() -> Vec<ServerProfile> {
    let tls = |sn: &str| TlsConfig { server_name: Some(sn.into()), ..Default::default() };
    vec![
        ServerProfile::new(
            "reality",
            Protocol::Vless(VlessConfig {
                server: "vps.example.net".into(),
                port: 443,
                uuid: UUID.into(),
                flow: Some(VISION_FLOW.into()),
                tls: Some(TlsConfig {
                    server_name: Some("www.example.com".into()),
                    fingerprint: Some("chrome".into()),
                    reality: Some(RealityConfig { public_key: PBK.into(), short_id: "b4bccbe34d6b9b16".into() }),
                    ..Default::default()
                }),
                transport: Transport::Tcp,
            }),
        ),
        ServerProfile::new(
            "vless-ws-tls",
            Protocol::Vless(VlessConfig {
                server: "203.0.113.4".into(),
                port: 443,
                uuid: UUID.into(),
                flow: None,
                tls: Some(TlsConfig { fingerprint: Some("firefox".into()), ..tls("cdn.example.com") }),
                transport: Transport::Ws { path: "/ray".into(), host: Some("cdn.example.com".into()) },
            }),
        ),
        ServerProfile::new(
            "trojan-grpc",
            Protocol::Trojan(TrojanConfig {
                server: "example.org".into(),
                port: 443,
                password: "pw".into(),
                tls: tls("example.org"),
                transport: Transport::Grpc { service_name: "tun".into() },
            }),
        ),
        ServerProfile::new(
            "ss2022",
            Protocol::Shadowsocks(ShadowsocksConfig {
                server: "198.51.100.5".into(),
                port: 8388,
                method: "2022-blake3-aes-128-gcm".into(),
                password: "AAECAwQFBgcICQoLDA0ODw==".into(),
            }),
        ),
        ServerProfile::new(
            "hy2-pinned",
            Protocol::Hysteria2(Hysteria2Config {
                server: "203.0.113.10".into(),
                port: 443,
                password: "pw".into(),
                obfs: None,
                tls: TlsConfig { public_key_sha256: vec![PIN.into()], ..tls("lotus.internal") },
                up_mbps: None,
                down_mbps: None,
            }),
        ),
        ServerProfile::new(
            "hy2-trusted-cert",
            Protocol::Hysteria2(Hysteria2Config {
                server: "203.0.113.10".into(),
                port: 443,
                password: "pw".into(),
                obfs: None,
                tls: TlsConfig { certificate: Some(TEST_CERT.into()), ..tls("lotus.internal") },
                up_mbps: None,
                down_mbps: None,
            }),
        ),
        ServerProfile::new(
            "hy2-obfs",
            Protocol::Hysteria2(Hysteria2Config {
                server: "example.org".into(),
                port: 8443,
                password: "pw".into(),
                obfs: Some(Obfs { kind: "salamander".into(), password: "obfs".into() }),
                tls: tls("example.org"),
                up_mbps: Some(50),
                down_mbps: Some(200),
            }),
        ),
        ServerProfile::new(
            "tuic",
            Protocol::Tuic(TuicConfig {
                server: "example.org".into(),
                port: 443,
                uuid: UUID.into(),
                password: "pw".into(),
                congestion_control: "bbr".into(),
                udp_relay_mode: Some("native".into()),
                tls: TlsConfig { alpn: vec!["h3".into()], ..tls("example.org") },
            }),
        ),
        ServerProfile::new(
            "anytls",
            Protocol::AnyTls(AnyTlsConfig { server: "example.org".into(), port: 443, password: "pw".into(), tls: tls("example.org") }),
        ),
        ServerProfile::new(
            "naive",
            Protocol::Naive(NaiveConfig {
                server: "example.org".into(),
                port: 443,
                username: "u".into(),
                password: "p".into(),
                tls: tls("example.org"),
                quic: false,
            }),
        ),
        ServerProfile::new(
            "wireguard",
            Protocol::WireGuard(WireGuardConfig {
                server: "vpn.example.net".into(),
                port: 51820,
                private_key: "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=".into(),
                peer_public_key: "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=".into(),
                pre_shared_key: None,
                local_address: vec!["10.66.0.2/32".into(), "fd00:66::2/128".into()],
                mtu: Some(1420),
                reserved: Some([1, 2, 3]),
                keepalive: Some(25),
            }),
        ),
    ]
}

#[test]
fn shadow_configs_pass_engine_check() {
    let ctx = ctx();
    for p in profiles() {
        let mut o = ShadowOptions::default();
        check(&p.name, &shadow_config(&p, &o, &ctx).unwrap());
        o.ipv6 = Ipv6Mode::Disable;
        o.block_trackers = true;
        o.allow_lan = false;
        o.quic = QuicPolicy::Block;
        o.mtu = Some(1500);
        o.dns_provider = DnsProviderId::Quad9;
        check(&format!("{} (strict)", p.name), &shadow_config(&p, &o, &ctx).unwrap());
        check(&format!("{} (probe)", p.name), &probe_config(&p, &ctx).unwrap());
    }
}

/// Index of the first route rule matching `pred`.
fn rule_at(cfg: &serde_json::Value, pred: impl Fn(&serde_json::Value) -> bool) -> Option<usize> {
    cfg["route"]["rules"].as_array().unwrap().iter().position(pred)
}

#[test]
fn split_and_encrypt_rules_mean_what_the_settings_say() {
    let ctx = ctx();
    let p = profiles().into_iter().next().unwrap(); // VLESS over TCP: QUIC blocked on auto
    let split = |mode| SplitTunnel { mode, apps: vec!["Discord.exe".into()], sites: vec!["netflix.com".into(), "203.0.113.7".into()] };
    let quic = |r: &serde_json::Value| r["network"] == "udp" && r["port"] == 443 && r["action"] == "reject";

    // Default: unchanged behaviour (everything tunnelled, DNS via the server).
    let all = shadow_config(&p, &ShadowOptions::default(), &ctx).unwrap();
    assert_eq!(all["route"]["final"], "proxy");
    assert_eq!(all["dns"]["final"], "dns-remote");
    assert!(all["dns"]["servers"].as_array().unwrap().iter().all(|s| s["tag"] != "dns-direct"));
    assert!(all["dns"].get("reverse_mapping").is_none());

    // Bypass: listed apps/sites go direct *before* the QUIC block, so their QUIC works.
    let o = ShadowOptions { split: split(SplitMode::Bypass), ..Default::default() };
    let c = shadow_config(&p, &o, &ctx).unwrap();
    let app = rule_at(&c, |r| r["process_path_regex"][0] == "(?i)(^|\\\\)Discord\\.exe$" && r["outbound"] == "direct").expect("app bypass rule");
    let site = rule_at(&c, |r| r["domain_suffix"][0] == "netflix.com" && r["ip_cidr"][0] == "203.0.113.7/32" && r["outbound"] == "direct").expect("site bypass rule");
    let q = rule_at(&c, quic).expect("quic block");
    assert!(app < q && site < q);
    assert_eq!(c["route"]["final"], "proxy");
    assert!(c["dns"]["rules"].as_array().unwrap().iter().any(|r| r["domain_suffix"][0] == "netflix.com" && r["server"] == "dns-direct"));
    assert_eq!(c["dns"]["reverse_mapping"], true);

    // Only: listed traffic tunnelled (with the QUIC block), everything else direct and untouched.
    let o = ShadowOptions { split: split(SplitMode::Only), ..Default::default() };
    let c = shadow_config(&p, &o, &ctx).unwrap();
    assert_eq!(c["route"]["final"], "direct");
    assert!(rule_at(&c, |r| r["process_path_regex"].is_array() && quic(r)).is_some());
    assert!(rule_at(&c, |r| r["process_path_regex"].is_array() && r["outbound"] == "proxy").is_some());
    assert!(rule_at(&c, |r| r.get("process_path_regex").is_none() && r.get("domain_suffix").is_none() && quic(r)).is_none(), "QUIC must not be blocked for traffic outside the tunnel");

    // LOTUS's own probes always take the tunnel, ahead of any split rule.
    let probe = rule_at(&c, |r| r["ip_cidr"][0] == "1.1.1.1/32" && r["outbound"] == "proxy").unwrap();
    assert!(probe < rule_at(&c, |r| r["process_path_regex"].is_array()).unwrap());

    // Traffic only: tunnel everything, resolve directly.
    let o = ShadowOptions { encrypt: EncryptScope::Traffic, ..Default::default() };
    let c = shadow_config(&p, &o, &ctx).unwrap();
    assert_eq!((c["route"]["final"].as_str(), c["dns"]["final"].as_str()), (Some("proxy"), Some("dns-direct")));

    // DNS only: traffic direct (no QUIC/IPv6 limits), DNS through a relay that the router counts, split ignored.
    let o = ShadowOptions { encrypt: EncryptScope::Dns, ipv6: Ipv6Mode::Disable, split: split(SplitMode::Bypass), ..Default::default() };
    let c = shadow_config(&p, &o, &ctx).unwrap();
    assert_eq!((c["route"]["final"].as_str(), c["dns"]["final"].as_str()), (Some("direct"), Some("dns-remote")));
    assert!(rule_at(&c, quic).is_none());
    assert!(rule_at(&c, |r| r["ip_version"] == 6).is_none());
    assert!(rule_at(&c, |r| r["process_path_regex"].is_array()).is_none());
    assert_eq!(rule_at(&c, |r| r["inbound"][0] == "dns-relay" && r["outbound"] == "proxy"), Some(0));
    let remote = c["dns"]["servers"].as_array().unwrap().iter().find(|s| s["tag"] == "dns-remote").unwrap();
    assert_eq!((remote["server"].as_str(), remote["server_port"].as_u64()), (Some("127.0.0.1"), Some(39092)));
}

#[test]
fn split_entries_are_validated() {
    use lotus_core::validate::{parse_split_site, validate_split, SplitSite};
    assert_eq!(parse_split_site("10.0.0.0/8"), Ok(SplitSite::Cidr("10.0.0.0/8".into())));
    assert_eq!(parse_split_site("2001:db8::1"), Ok(SplitSite::Cidr("2001:db8::1/128".into())));
    assert_eq!(parse_split_site("sub.example.co.uk"), Ok(SplitSite::Domain("sub.example.co.uk".into())));
    for bad in ["Example.com", "example", "*.example.com", "https://example.com", "10.0.0.0/33", "10.0.0.0/08", "exa mple.com", "-a.com", "a..com"] {
        assert!(parse_split_site(bad).is_err(), "{bad} should be refused");
    }
    let ok = |apps: &[&str]| validate_split(&SplitTunnel { mode: SplitMode::Bypass, apps: apps.iter().map(|s| s.to_string()).collect(), sites: vec![] });
    assert!(ok(&["Discord.exe", "steam (beta)+.EXE"]).is_ok());
    for bad in ["discord", "C:\\x\\discord.exe", "a/b.exe", "x\".exe", " lead.exe", ".exe"] {
        assert!(ok(&[bad]).is_err(), "{bad} should be refused");
    }
    assert!(ok(&vec!["a.exe"; 65]).is_err());
}

#[test]
fn split_and_encrypt_configs_pass_engine_check() {
    let ctx = ctx();
    let split = SplitTunnel {
        mode: SplitMode::Bypass,
        apps: vec!["Discord.exe".into(), "steam (beta)+.exe".into()],
        sites: vec!["netflix.com".into(), "192.168.50.0/24".into(), "2001:db8::/32".into(), "203.0.113.7".into()],
    };
    for p in profiles().into_iter().take(3) {
        for encrypt in [EncryptScope::All, EncryptScope::Traffic, EncryptScope::Dns] {
            for mode in [SplitMode::Off, SplitMode::Bypass, SplitMode::Only] {
                for ipv6 in [Ipv6Mode::Tunnel, Ipv6Mode::Disable] {
                    let o = ShadowOptions { encrypt, ipv6, block_trackers: true, split: SplitTunnel { mode, ..split.clone() }, ..Default::default() };
                    check(&format!("{} {encrypt:?} {mode:?} {ipv6:?}", p.name), &shadow_config(&p, &o, &ctx).unwrap());
                }
            }
        }
        let sites_only = ShadowOptions { split: SplitTunnel { mode: SplitMode::Only, apps: vec![], sites: vec!["example.org".into()] }, ..Default::default() };
        check(&format!("{} only sites", p.name), &shadow_config(&p, &sites_only, &ctx).unwrap());
        let apps_only = ShadowOptions { split: SplitTunnel { mode: SplitMode::Bypass, apps: vec!["game.exe".into()], sites: vec![] }, ..Default::default() };
        check(&format!("{} bypass apps", p.name), &shadow_config(&p, &apps_only, &ctx).unwrap());
    }
}

#[test]
fn lotus_configs_pass_engine_check() {
    let ctx = ctx();
    for provider in [DnsProviderId::Cloudflare, DnsProviderId::Quad9, DnsProviderId::Google, DnsProviderId::Mullvad, DnsProviderId::Adguard] {
        for transport in [DnsTransport::Doh, DnsTransport::Doh3, DnsTransport::Dot] {
            for (trackers, malware) in [(false, false), (true, true)] {
                let o = LotusOptions { provider, transport, block_trackers: trackers, block_malware: malware, ..Default::default() };
                check(&format!("lotus {provider:?} {transport:?}"), &lotus_config(&o, &ctx, &LotusListen::default()).unwrap());
                let relayed = LotusListen { relay_port: Some(39091), ..Default::default() };
                check(&format!("lotus {provider:?} {transport:?} (relay)"), &lotus_config(&o, &ctx, &relayed).unwrap());
            }
        }
    }
    let custom = LotusOptions {
        provider: DnsProviderId::Custom,
        custom: Some(CustomResolver { url: "https://dns.example.net/resolve".into(), ip: "192.0.2.53".into() }),
        ..Default::default()
    };
    check("lotus custom", &lotus_config(&custom, &ctx, &LotusListen::default()).unwrap());
}

#[test]
fn generated_config_never_contains_file_outputs() {
    let ctx = ctx();
    for p in profiles() {
        let s = serde_json::to_string(&shadow_config(&p, &ShadowOptions::default(), &ctx).unwrap()).unwrap();
        for forbidden in ["\"output\"", "external_ui", "cache_file", "certificate_path", "\"insecure\""] {
            assert!(!s.contains(forbidden), "{forbidden} found in {}", p.name);
        }
    }
}
