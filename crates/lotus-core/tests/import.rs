use lotus_core::import::{encode_bundle, parse_import, share_link};
use lotus_core::*;

const PBK: &str = "uowCJZ1KWCUrql7F9wgtzYtEUXBiJNlbeJjNis8fB1U";
const UUID: &str = "0c7c4baa-301f-4eef-ac81-e785d343d7fa";

fn reality_link() -> String {
    format!("vless://{UUID}@203.0.113.10:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.example.com&fp=chrome&pbk={PBK}&sid=b4bccbe34d6b9b16&type=tcp#My%20VPS")
}

#[test]
fn vless_reality_vision() {
    let r = parse_import(&reality_link());
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    let p = &r.profiles[0];
    assert_eq!(p.name, "My VPS");
    match &p.protocol {
        Protocol::Vless(c) => {
            assert_eq!(c.server, "203.0.113.10");
            assert_eq!(c.port, 443);
            assert_eq!(c.flow.as_deref(), Some(VISION_FLOW));
            let t = c.tls.as_ref().unwrap();
            assert_eq!(t.server_name.as_deref(), Some("www.example.com"));
            assert_eq!(t.fingerprint.as_deref(), Some("chrome"));
            let re = t.reality.as_ref().unwrap();
            assert_eq!(re.public_key, PBK);
            assert_eq!(re.short_id, "b4bccbe34d6b9b16");
            assert_eq!(c.transport, Transport::Tcp);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(p.protocol.label(), "VLESS · REALITY · Vision");
}

#[test]
fn share_link_roundtrip() {
    let p = parse_import(&reality_link()).profiles.remove(0);
    let again = parse_import(&share_link(&p)).profiles.remove(0);
    assert_eq!(p.protocol, again.protocol);
    assert_eq!(p.name, again.name);
}

#[test]
fn vless_ipv6_host() {
    let link = format!("vless://{UUID}@[2001:db8::7]:8443?security=reality&sni=www.example.com&pbk={PBK}&sid=ab&flow=xtls-rprx-vision#v6");
    let r = parse_import(&link);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert_eq!(r.profiles[0].server(), "2001:db8::7");
    assert_eq!(r.profiles[0].port(), 8443);
}

#[test]
fn plaintext_vless_rejected() {
    let r = parse_import(&format!("vless://{UUID}@203.0.113.10:80?type=tcp#plain"));
    assert!(r.profiles.is_empty());
    assert!(r.issues[0].message.contains("unencrypted"), "{:?}", r.issues);
}

#[test]
fn insecure_links_rejected() {
    for link in [
        "trojan://secret@example.org:443?security=tls&sni=example.org&allowInsecure=1#t",
        "hysteria2://secret@example.org:443/?sni=example.org&insecure=1#h",
        "tuic://0c7c4baa-301f-4eef-ac81-e785d343d7fa:pw@example.org:443?allow_insecure=1#u",
    ] {
        let r = parse_import(link);
        assert!(r.profiles.is_empty(), "{link} imported");
        assert!(r.issues[0].message.contains("never disables TLS verification"), "{:?}", r.issues);
    }
}

#[test]
fn xhttp_and_vmess_explained() {
    let r = parse_import(&format!("vless://{UUID}@example.org:443?security=reality&pbk={PBK}&sni=a.com&type=xhttp#x"));
    assert!(r.issues[0].message.contains("XHTTP"));
    let r = parse_import("vmess://eyJhZGQiOiIxIn0=");
    assert!(r.issues[0].message.contains("VMess"));
}

#[test]
fn shadowsocks_sip002_and_legacy() {
    // SIP002 with percent-encoded 2022 key
    let r = parse_import("ss://2022-blake3-aes-128-gcm:AAECAwQFBgcICQoLDA0ODw%3D%3D@198.51.100.5:8388#ss22");
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    match &r.profiles[0].protocol {
        Protocol::Shadowsocks(c) => {
            assert_eq!(c.method, "2022-blake3-aes-128-gcm");
            assert_eq!(c.password, "AAECAwQFBgcICQoLDA0ODw==");
        }
        _ => panic!(),
    }
    // SIP002 base64url userinfo
    let ui = base64_url("aes-256-gcm:correct horse");
    let r = parse_import(&format!("ss://{ui}@198.51.100.6:443#aead"));
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    // Legacy whole-body base64
    let body = base64_url("chacha20-ietf-poly1305:pw123@198.51.100.7:8000");
    let r = parse_import(&format!("ss://{body}#legacy"));
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert_eq!(r.profiles[0].port(), 8000);
    // Stream cipher refused
    let r = parse_import(&format!("ss://{}@198.51.100.7:8000#rc4", base64_url("rc4-md5:pw")));
    assert!(r.profiles.is_empty());
}

fn base64_url(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s)
}

#[test]
fn ss2022_bad_key_length() {
    let r = parse_import("ss://2022-blake3-aes-256-gcm:AAECAwQFBgcICQoLDA0ODw%3D%3D@198.51.100.5:8388#bad");
    assert!(r.profiles.is_empty());
    assert!(r.issues[0].message.contains("32-byte"));
}

#[test]
fn wireguard_conf() {
    let conf = "[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.66.0.2/32, fd00:66::2/128\nDNS = 1.1.1.1\nMTU = 1420\n\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = vpn.example.net:51820\nPersistentKeepalive = 25\n";
    let r = parse_import(conf);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    match &r.profiles[0].protocol {
        Protocol::WireGuard(c) => {
            assert_eq!(c.server, "vpn.example.net");
            assert_eq!(c.port, 51820);
            assert_eq!(c.local_address, vec!["10.66.0.2/32", "fd00:66::2/128"]);
            assert_eq!(c.mtu, Some(1420));
            assert_eq!(c.keepalive, Some(25));
        }
        _ => panic!(),
    }
}

#[test]
fn wireguard_with_scripts_refused() {
    let conf = "[Interface]\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.66.0.2/32\nPostUp = calc.exe\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nAllowedIPs = 0.0.0.0/0\nEndpoint = 203.0.113.1:51820\n";
    let r = parse_import(conf);
    assert!(r.profiles.is_empty());
    assert!(r.issues[0].message.contains("PostUp"));
}

#[test]
fn singbox_json_full_config() {
    let json = format!(
        r#"{{
        "outbounds": [
            {{"type":"vless","tag":"de-1","server":"203.0.113.10","server_port":443,"uuid":"{UUID}","flow":"xtls-rprx-vision",
              "tls":{{"enabled":true,"server_name":"www.example.com","utls":{{"enabled":true,"fingerprint":"chrome"}},
                     "reality":{{"enabled":true,"public_key":"{PBK}","short_id":"ab12"}}}}}},
            {{"type":"direct","tag":"direct"}},
            {{"type":"hysteria2","tag":"hy","server":"example.org","server_port":443,"password":"pw",
              "tls":{{"enabled":true,"server_name":"example.org","insecure":true}}}}
        ]}}"#
    );
    let r = parse_import(&json);
    assert_eq!(r.profiles.len(), 1, "{:?}", r.issues);
    assert_eq!(r.profiles[0].name, "de-1");
    assert_eq!(r.issues.len(), 1);
}

#[test]
fn lotus_bundle_roundtrip() {
    let p = parse_import(&reality_link()).profiles.remove(0);
    let hy = ServerProfile::new(
        "hy",
        Protocol::Hysteria2(Hysteria2Config {
            server: "203.0.113.10".into(),
            port: 443,
            password: "s3cret-pass".into(),
            obfs: None,
            tls: TlsConfig {
                server_name: Some("lotus.local".into()),
                public_key_sha256: vec!["47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=".into()],
                ..Default::default()
            },
            up_mbps: None,
            down_mbps: None,
        }),
    );
    let link = encode_bundle(&[p.clone(), hy.clone()]);
    assert!(link.starts_with("lotus://import/"));
    let r = parse_import(&link);
    assert!(r.issues.is_empty(), "{:?}", r.issues);
    assert_eq!(r.profiles.len(), 2);
    assert_eq!(r.profiles[1].protocol, hy.protocol);
    assert_ne!(r.profiles[0].id, p.id, "ids are regenerated on import");
}

#[test]
fn base64_subscription() {
    use base64::Engine;
    let sub = base64::engine::general_purpose::STANDARD.encode(format!("{}\ntrojan://pw@example.org:443?sni=example.org#t\n", reality_link()));
    let r = parse_import(&sub);
    assert_eq!(r.profiles.len(), 2, "{:?}", r.issues);
}

#[test]
fn issue_excerpts_hide_secrets() {
    let r = parse_import("trojan://VERY-SECRET@example.org:443?allowInsecure=1#t");
    assert!(!r.issues[0].source.contains("VERY-SECRET"));
    assert!(r.issues[0].source.contains("example.org"));
}

#[test]
fn redactor_scrubs() {
    let p = parse_import(&reality_link()).profiles.remove(0);
    let red = lotus_core::redact::Redactor::new(Some(&p));
    let line = format!("ERROR outbound/vless[proxy]: dial tcp 203.0.113.10:443: uuid {UUID} 192.168.1.4 failed");
    let out = red.scrub(&line);
    assert!(!out.contains("203.0.113.10"), "{out}");
    assert!(!out.contains(UUID), "{out}");
    assert!(!out.contains("192.168.1.4"), "{out}");
}
