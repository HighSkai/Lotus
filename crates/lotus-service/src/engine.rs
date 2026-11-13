//! Supervision of the sing-box engine process.

use crate::paths::Paths;
use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use lotus_core::redact::Redactor;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{oneshot, watch};

pub const EXPECTED_SHA256: &str = env!("LOTUS_ENGINE_SHA256");

/// Recent engine/service messages, already redacted. Memory only.
#[derive(Clone, Default)]
pub struct LogSink(Arc<Mutex<VecDeque<String>>>);

impl LogSink {
    pub fn push(&self, line: String) {
        let mut q = self.0.lock().unwrap();
        if q.len() >= 300 {
            q.pop_front();
        }
        q.push_back(line);
    }
    pub fn snapshot(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().cloned().collect()
    }
    pub fn since(&self, mark: usize) -> Vec<String> {
        let q = self.0.lock().unwrap();
        q.iter().skip(mark.min(q.len())).cloned().collect()
    }
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

pub fn timestamp() -> String {
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}Z")
}

// ---------------------------------------------------------------------------
// Integrity check
// ---------------------------------------------------------------------------

static VERIFIED: Mutex<Option<(u64, SystemTime)>> = Mutex::new(None);

/// Verify the engine digest and return a handle that denies writes to the file
/// until it is dropped (held across process creation to close the TOCTOU window).
pub fn verify_engine(paths: &Paths) -> Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 /* FILE_SHARE_READ */)
        .open(&paths.engine)
        .with_context(|| format!("The networking engine is missing ({}). Reinstall LOTUS.", paths.engine.display()))?;
    let meta = file.metadata()?;
    let key = (meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
    if EXPECTED_SHA256.is_empty() {
        return Ok(file);
    }
    if *VERIFIED.lock().unwrap() == Some(key) {
        return Ok(file);
    }
    let mut hasher = Sha256::new();
    let mut reader = std::io::BufReader::with_capacity(1 << 20, &file);
    std::io::copy(&mut reader, &mut hasher)?;
    let got = hex::encode(hasher.finalize());
    if got != EXPECTED_SHA256 {
        bail!("The networking engine failed its integrity check and was not started. Reinstall LOTUS.");
    }
    *VERIFIED.lock().unwrap() = Some(key);
    Ok(file)
}

// ---------------------------------------------------------------------------
// Controller API client (local only, random port + secret per run)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Clash {
    base: String,
    secret: String,
    http: reqwest::Client,
}

impl Clash {
    pub fn new(port: u16, secret: String) -> Self {
        let http = reqwest::Client::builder().no_proxy().pool_max_idle_per_host(2).build().expect("http client");
        Self { base: format!("http://127.0.0.1:{port}"), secret, http }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.get(format!("{}{}", self.base, path)).bearer_auth(&self.secret)
    }

    pub async fn version(&self) -> Result<String> {
        let v: Value = self.get("/version").timeout(Duration::from_millis(800)).send().await?.error_for_status()?.json().await?;
        Ok(v.get("version").and_then(Value::as_str).unwrap_or("").to_string())
    }

    /// Real round-trip through the given outbound (TCP/TLS handshake to the server
    /// plus an HTTP request from the server to the probe URL).
    pub async fn delay(&self, tag: &str, url: &str, timeout_ms: u32) -> Result<u32> {
        let resp = self
            .get(&format!("/proxies/{tag}/delay"))
            .query(&[("url", url), ("timeout", &timeout_ms.to_string())])
            .timeout(Duration::from_millis(timeout_ms as u64 + 2000))
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            if let Some(d) = v.get("delay").and_then(Value::as_u64) {
                return Ok(d as u32);
            }
        }
        Err(anyhow!("{}", v.get("message").and_then(Value::as_str).unwrap_or("delay test failed")))
    }

    /// Snapshot of live connections (the engine's own bookkeeping).
    pub async fn connections(&self) -> Result<Value> {
        Ok(self.get("/connections").timeout(Duration::from_secs(2)).send().await?.error_for_status()?.json().await?)
    }

    /// Stream per-second traffic counters until the connection ends.
    pub async fn traffic<F: FnMut(u64, u64)>(&self, mut on: F) -> Result<()> {
        let resp = self.get("/traffic").send().await?.error_for_status()?;
        let mut stream = resp.bytes_stream();
        let mut buf = Vec::new();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk?);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                if let Ok(v) = serde_json::from_slice::<Value>(&line) {
                    on(v.get("up").and_then(Value::as_u64).unwrap_or(0), v.get("down").and_then(Value::as_u64).unwrap_or(0));
                }
            }
        }
        Ok(())
    }
}

pub fn new_controller_endpoint() -> Result<(u16, String)> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = l.local_addr()?.port();
    drop(l);
    let secret: String = {
        use rand::RngCore;
        let mut b = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut b);
        hex::encode(b)
    };
    Ok((port, secret))
}

// ---------------------------------------------------------------------------
// Process
// ---------------------------------------------------------------------------

pub struct Engine {
    pub clash: Clash,
    exit_rx: watch::Receiver<Option<String>>,
    kill_tx: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    pub log_mark: usize,
    logs: LogSink,
}

pub struct Spawn<'a> {
    pub paths: &'a Paths,
    #[cfg(windows)]
    pub job: &'a crate::win::job::Job,
    pub config: &'a Value,
    pub clash_port: u16,
    pub clash_secret: String,
    pub logs: LogSink,
    pub redactor: Arc<Redactor>,
}

impl Engine {
    pub async fn spawn(s: Spawn<'_>) -> Result<Engine> {
        let lock = verify_engine(s.paths)?;
        let mut cmd = Command::new(&s.paths.engine);
        cmd.args(["run", "-c", "stdin", "--disable-color", "-D"])
            .arg(&s.paths.work_dir)
            .current_dir(&s.paths.exe_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000 /* CREATE_NO_WINDOW */);
        let mut child = cmd.spawn().context("start networking engine")?;
        drop(lock);
        #[cfg(windows)]
        if let Some(h) = child.raw_handle() {
            s.job.assign(h as _)?;
        }
        let mut stdin = child.stdin.take().context("engine stdin")?;
        stdin.write_all(&serde_json::to_vec(s.config)?).await?;
        stdin.shutdown().await.ok();
        drop(stdin);

        let log_mark = s.logs.len();
        for stream in [child.stdout.take().map(|o| Box::new(o) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|e| Box::new(e) as _)]
            .into_iter()
            .flatten()
        {
            let logs = s.logs.clone();
            let red = s.redactor.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if !l.trim().is_empty() {
                        logs.push(format!("{} engine: {}", timestamp(), red.scrub(&l)));
                    }
                }
            });
        }

        let (exit_tx, exit_rx) = watch::channel(None);
        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            tokio::select! {
                st = child.wait() => {
                    let msg = match st { Ok(s) => format!("engine exited ({s})"), Err(e) => format!("engine wait failed: {e}") };
                    let _ = exit_tx.send(Some(msg));
                }
                _ = kill_rx => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    let _ = exit_tx.send(Some("stopped".into()));
                }
            }
        });
        Ok(Engine { clash: Clash::new(s.clash_port, s.clash_secret), exit_rx, kill_tx: Some(kill_tx), task: Some(task), log_mark, logs: s.logs })
    }

    pub fn has_exited(&self) -> bool {
        self.exit_rx.borrow().is_some()
    }

    /// Resolves when the engine process has exited (for use in `select!`).
    pub async fn exited(&mut self) -> String {
        loop {
            if let Some(m) = self.exit_rx.borrow().clone() {
                return m;
            }
            if self.exit_rx.changed().await.is_err() {
                return "engine monitor ended".into();
            }
        }
    }

    pub fn recent_logs(&self) -> Vec<String> {
        self.logs.since(self.log_mark)
    }

    pub async fn wait_ready(&mut self, timeout: Duration) -> Result<String> {
        let start = Instant::now();
        loop {
            if self.has_exited() {
                bail!("{}", crate::explain::engine_failure(&self.recent_logs()));
            }
            if let Ok(v) = self.clash.version().await {
                return Ok(v);
            }
            if start.elapsed() > timeout {
                bail!("The networking engine did not start in time.");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn stop(mut self) {
        if let Some(k) = self.kill_tx.take() {
            let _ = k.send(());
        }
        if let Some(t) = self.task.take() {
            let _ = tokio::time::timeout(Duration::from_secs(10), t).await;
        }
    }
}

/// Turn the controller API's `/connections` answer into the IPC snapshot.
pub fn parse_connections(v: &Value) -> lotus_core::ipc::ConnectionsSnapshot {
    let num = |v: &Value| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0);
    let text = |v: &Value| v.as_str().unwrap_or("").to_string();
    let mut list: Vec<lotus_core::ipc::ConnInfo> = v
        .get("connections")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| {
                    let m = &c["metadata"];
                    let host = text(&m["host"]);
                    lotus_core::ipc::ConnInfo {
                        id: text(&c["id"]),
                        host: if host.is_empty() { text(&m["destinationIP"]) } else { host },
                        port: num(&m["destinationPort"]) as u16,
                        network: text(&m["network"]),
                        route: c["chains"].as_array().map(|ch| ch.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" > ")).unwrap_or_default(),
                        upload: num(&c["upload"]),
                        download: num(&c["download"]),
                        started: text(&c["start"]),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    list.sort_by(|a, b| (b.upload + b.download).cmp(&(a.upload + a.download)));
    lotus_core::ipc::ConnectionsSnapshot { active: true, connections: list, upload_total: num(&v["uploadTotal"]), download_total: num(&v["downloadTotal"]) }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_engine_connections() {
        // Same shape as sing-box 1.14's /connections (ports arrive as strings).
        let v: serde_json::Value = serde_json::from_str(r#"{"connections":[
            {"chains":["proxy"],"download":9278106,"id":"a","metadata":{"destinationIP":"","destinationPort":"443","dnsMode":"normal","host":"speed.example.net","network":"tcp","processPath":"","sourceIP":"172.19.0.1","sourcePort":"50000","type":"tun/tun-in"},"rule":"final","rulePayload":"","start":"2026-11-13T07:57:07Z","upload":674},
            {"chains":["direct"],"download":10,"id":"b","metadata":{"destinationIP":"192.168.1.10","destinationPort":"80","host":"","network":"udp"},"start":"2026-11-13T07:57:08Z","upload":5}
        ],"downloadTotal":9278116,"memory":0,"uploadTotal":679}"#).unwrap();
        let s = super::parse_connections(&v);
        assert!(s.active);
        assert_eq!(s.connections.len(), 2);
        assert_eq!(s.connections[0].host, "speed.example.net");
        assert_eq!(s.connections[0].port, 443);
        assert_eq!(s.connections[0].route, "proxy");
        assert_eq!(s.connections[1].host, "192.168.1.10");
        assert_eq!(s.connections[1].network, "udp");
        assert_eq!((s.upload_total, s.download_total), (679, 9278116));
    }
}
