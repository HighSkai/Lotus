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
