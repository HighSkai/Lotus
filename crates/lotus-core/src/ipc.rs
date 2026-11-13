//! Protocol between the desktop app (unprivileged) and the LOTUS service (privileged).
//!
//! Transport: a local named pipe (Windows) / Unix socket (future), carrying
//! newline-delimited JSON. Every request has an `id`; the service answers with
//! a `response` carrying the same id, and pushes `status` events at any time.
//! The command set is closed: there is no way to ask the service to run a
//! program, read or write a file, or pass raw engine configuration.

use crate::model::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PIPE_NAME: &str = r"\\.\pipe\lotus-service";
pub const PROTOCOL_VERSION: u32 = 1;
/// Upper bound for one framed message (a profile with an inline certificate is a few KiB).
pub const MAX_FRAME: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Hello { protocol: u32, client: String },
    Status,
    ConnectLotus { options: LotusOptions },
    ConnectShadow { profile: ServerProfile, options: ShadowOptions },
    Disconnect,
    /// Measure handshake + round-trip latency to a server without changing routing.
    TestProfile { profile: ServerProfile },
    /// On-demand throughput test over the current routing.
    SpeedTest,
    /// Leak and routing diagnostics.
    Diagnostics,
    /// Recent engine messages (warnings and errors only, secrets redacted).
    Logs,
    /// Live connections through the running engine (read on demand, never logged).
    Connections,
    /// Check what the server really is: authenticated handshake, camouflage, exit.
    VerifyServer { profile: ServerProfile },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Response {
        id: u64,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<ErrorInfo>,
    },
    Status {
        status: Status,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

impl ErrorInfo {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Disconnecting,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub state: ConnState,
    pub mode: Option<Mode>,
    /// Unix ms when the current session became connected.
    pub connected_since: Option<u64>,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub protocol_label: Option<String>,
    pub exit: Option<ExitInfo>,
    pub latency_ms: Option<u32>,
    pub traffic: Traffic,
    pub error: Option<ErrorInfo>,
    pub protections: Vec<Protection>,
    /// Kill switch filters are installed and currently blocking non-tunnel traffic.
    pub kill_switch_engaged: bool,
    pub service_version: String,
    pub engine_version: String,
    /// Incremented on every change, lets clients drop stale events.
    pub seq: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExitInfo {
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    /// ISO country code as reported by the exit probe.
    pub country: Option<String>,
    /// Probe data-centre code (e.g. FRA).
    pub colo: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Traffic {
    pub up_rate: u64,
    pub down_rate: u64,
    pub up_total: u64,
    pub down_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Protection {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub active: bool,
}

impl Protection {
    pub fn new(id: &str, label: &str, detail: impl Into<String>, active: bool) -> Self {
        Self { id: id.into(), label: label.into(), detail: detail.into(), active }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub tcp_ms: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SpeedResult {
    pub download_mbps: Option<f64>,
    pub upload_mbps: Option<f64>,
    pub latency_ms: Option<u32>,
    pub bytes_down: u64,
    pub bytes_up: u64,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Pass,
    Warn,
    Fail,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Check {
    pub id: String,
    pub title: String,
    pub outcome: CheckOutcome,
    pub summary: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsReport {
    pub mode: Option<Mode>,
    pub state: ConnState,
    pub checks: Vec<Check>,
    pub generated_at: u64,
}

/// One live connection as seen by the engine. Held in memory only while the app asks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConnInfo {
    pub id: String,
    /// Sniffed domain, or the destination IP when there is none.
    pub host: String,
    pub port: u16,
    /// "tcp" or "udp".
    pub network: String,
    /// Outbound chain, e.g. "proxy" (through your server), "direct" or "block".
    pub route: String,
    pub upload: u64,
    pub download: u64,
    /// When the connection opened (RFC 3339).
    pub started: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConnectionsSnapshot {
    pub active: bool,
    pub connections: Vec<ConnInfo>,
    pub upload_total: u64,
    pub download_total: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VerifyReport {
    /// e.g. "VLESS · REALITY · Vision over TCP 443".
    pub protocol: String,
    pub checks: Vec<Check>,
    pub generated_at: u64,
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let mut v = serde_json::to_vec(msg).expect("IPC message serialises");
    v.push(b'\n');
    v
}
