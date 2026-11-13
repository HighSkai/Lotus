//! Network probes that run over the operating system's normal routing, so they
//! measure exactly what applications experience.

use anyhow::{anyhow, bail, Result};
use futures_util::StreamExt;
use lotus_core::dnsmsg;
use lotus_core::ipc::SpeedResult;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

pub const TRACE_V4: &str = "https://1.1.1.1/cdn-cgi/trace";
pub const TRACE_V6: &str = "https://[2606:4700:4700::1111]/cdn-cgi/trace";
/// URL fetched *by the exit server* for latency tests.
pub const DELAY_URL: &str = "http://cp.cloudflare.com/generate_204";

#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub ip: String,
    pub loc: Option<String>,
    pub colo: Option<String>,
}

fn client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .connect_timeout(timeout)
        .pool_max_idle_per_host(0)
        .build()
        .expect("http client")
}

pub async fn trace(url: &str, timeout: Duration) -> Result<Trace> {
    let body = client(timeout).get(url).send().await?.error_for_status()?.text().await?;
    let mut t = Trace::default();
    for line in body.lines() {
        match line.split_once('=') {
            Some(("ip", v)) => t.ip = v.trim().to_string(),
            Some(("loc", v)) if v != "XX" => t.loc = Some(v.trim().to_string()),
            Some(("colo", v)) => t.colo = Some(v.trim().to_string()),
            _ => {}
        }
    }
    if t.ip.parse::<std::net::IpAddr>().is_err() {
        bail!("unexpected trace response");
    }
    Ok(t)
}

pub async fn dns_query(server: SocketAddr, name: &str, qtype: u16, timeout: Duration) -> Result<dnsmsg::Response> {
    let bind: SocketAddr = if server.is_ipv4() { "0.0.0.0:0".parse()? } else { "[::]:0".parse()? };
    let sock = tokio::net::UdpSocket::bind(bind).await?;
    let id: u16 = rand::random();
    let q = dnsmsg::build_query(id, name, qtype);
    sock.send_to(&q, server).await?;
    let mut buf = [0u8; 4096];
    let n = tokio::time::timeout(timeout, async {
        loop {
            let (n, from) = sock.recv_from(&mut buf).await?;
            if from == server && n >= 2 && u16::from_be_bytes([buf[0], buf[1]]) == id {
                return Ok::<usize, std::io::Error>(n);
            }
        }
    })
    .await
    .map_err(|_| anyhow!("no answer from {server}"))??;
    dnsmsg::parse_response(&buf[..n]).ok_or_else(|| anyhow!("malformed DNS answer"))
}

pub async fn tcp_ping(host: &str, port: u16, timeout: Duration) -> Result<u32> {
    let start = Instant::now();
    let addr = format!("{}:{port}", if host.contains(':') { format!("[{host}]") } else { host.to_string() });
    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addr)).await.map_err(|_| anyhow!("timed out"))??;
    Ok(start.elapsed().as_millis().max(1) as u32)
}

/// On-demand throughput test against Cloudflare's speed test endpoints.
/// Never runs in the background.
pub async fn speed_test() -> SpeedResult {
    let mut r = SpeedResult::default();
    let http = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(20)).build().expect("http client");

    // Latency: best of several small requests on a warm connection.
    let mut best: Option<u32> = None;
    for i in 0..6 {
        let t = Instant::now();
        if http.get("https://speed.cloudflare.com/__down?bytes=0").send().await.and_then(|r| r.error_for_status()).is_ok() && i > 0 {
            let ms = t.elapsed().as_millis() as u32;
            best = Some(best.map_or(ms, |b| b.min(ms)));
        }
    }
    r.latency_ms = best;

    // Download: up to 50 MB or 8 seconds.
    let t = Instant::now();
    match http.get("https://speed.cloudflare.com/__down?bytes=50000000").send().await {
        Ok(resp) => {
            let mut s = resp.bytes_stream();
            while let Some(Ok(chunk)) = s.next().await {
                r.bytes_down += chunk.len() as u64;
                if t.elapsed() > Duration::from_secs(8) {
                    break;
                }
            }
            let secs = t.elapsed().as_secs_f64();
            if r.bytes_down > 0 && secs > 0.0 {
                r.download_mbps = Some(r.bytes_down as f64 * 8.0 / secs / 1e6);
            }
        }
        Err(e) => r.message = format!("Download test failed: {}", e.without_url()),
    }

    // Upload: 10 MB.
    let payload = vec![0x5Au8; 10_000_000];
    let t = Instant::now();
    match http.post("https://speed.cloudflare.com/__up").body(payload).send().await {
        Ok(resp) if resp.status().is_success() => {
            r.bytes_up = 10_000_000;
            r.upload_mbps = Some(r.bytes_up as f64 * 8.0 / t.elapsed().as_secs_f64() / 1e6);
        }
        Ok(resp) => r.message = format!("Upload test failed (HTTP {})", resp.status().as_u16()),
        Err(e) => {
            if r.message.is_empty() {
                r.message = format!("Upload test failed: {}", e.without_url());
            }
        }
    }
    if r.message.is_empty() {
        r.message = "Measured with speed.cloudflare.com over your current connection.".into();
    }
    r
}

/// ASN and organisation for an IP via Team Cymru's DNS interface,
/// queried through `resolver` (so it follows the protected path).
pub async fn asn_of(ip: std::net::IpAddr, resolver: SocketAddr) -> Option<(u32, String)> {
    let name = match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.{}.origin.asn.cymru.com", o[3], o[2], o[1], o[0])
        }
        std::net::IpAddr::V6(v6) => {
            let hex: String = v6.octets().iter().map(|b| format!("{b:02x}")).collect();
            let nibbles: Vec<String> = hex.chars().rev().map(|c| c.to_string()).collect();
            format!("{}.origin6.asn.cymru.com", nibbles.join("."))
        }
    };
    let txt = first_txt(resolver, &name).await?;
    let asn: u32 = txt.split('|').next()?.split_whitespace().next()?.parse().ok()?;
    let org = first_txt(resolver, &format!("AS{asn}.asn.cymru.com"))
        .await
        .and_then(|t| t.rsplit('|').next().map(|s| s.trim().to_string()))
        .unwrap_or_default();
    Some((asn, org))
}

pub async fn first_txt(resolver: SocketAddr, name: &str) -> Option<String> {
    let r = dns_query(resolver, name, dnsmsg::TYPE_TXT, Duration::from_secs(4)).await.ok()?;
    r.answers.into_iter().find_map(|a| match a {
        dnsmsg::Answer::Txt(t) => Some(t),
        _ => None,
    })
}
