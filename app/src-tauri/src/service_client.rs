//! Persistent connection to the LOTUS service. Reconnects automatically,
//! forwards status pushes to the web view and the tray, and multiplexes
//! request/response pairs by id.

use lotus_core::ipc::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, ErrorInfo>>>>>;

#[derive(Clone)]
pub struct ServiceClient {
    tx: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
    pub last_status: Arc<Mutex<Option<Status>>>,
    pub connected: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone, serde::Serialize)]
pub struct ServiceEvent {
    pub connected: bool,
    pub message: Option<String>,
}

#[cfg(windows)]
fn open_pipe() -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING};
    // Must match the access the service grants interactive users.
    const ACCESS: u32 = 0x0012_008B;
    const SQOS: u32 = 0x0010_0000 | 0x0001_0000; // SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION
    let name: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe { CreateFileW(name.as_ptr(), ACCESS, 0, std::ptr::null(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED | SQOS, std::ptr::null_mut()) };
    if h == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    unsafe { tokio::net::windows::named_pipe::NamedPipeClient::from_raw_handle(h as _) }
}

impl ServiceClient {
    pub fn start(app: AppHandle) -> Self {
        let me = Self {
            tx: Default::default(),
            pending: Default::default(),
            next_id: Arc::new(AtomicU64::new(1)),
            last_status: Default::default(),
            connected: Default::default(),
        };
        let c = me.clone();
        tauri::async_runtime::spawn(async move { c.run(app).await });
        me
    }

    async fn run(self, app: AppHandle) {
        let mut announced_down = false;
        loop {
            match open_pipe() {
                Ok(pipe) => {
                    announced_down = false;
                    self.session(&app, pipe).await;
                    self.connected.store(false, Ordering::SeqCst);
                    *self.tx.lock().unwrap() = None;
                    for (_, p) in self.pending.lock().unwrap().drain() {
                        let _ = p.send(Err(ErrorInfo::new("service_unavailable", "The LOTUS service stopped responding.")));
                    }
                    let _ = app.emit("lotus://service", ServiceEvent { connected: false, message: Some("The LOTUS service disconnected.".into()) });
                    crate::tray::refresh(&app);
                }
                Err(e) => {
                    if !announced_down {
                        announced_down = true;
                        let msg = if e.raw_os_error() == Some(2) { "The LOTUS service isn't running." } else { "Can't reach the LOTUS service." };
                        let _ = app.emit("lotus://service", ServiceEvent { connected: false, message: Some(msg.into()) });
                        crate::tray::refresh(&app);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
    }

    async fn session<S>(&self, app: &AppHandle, stream: S)
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
    {
        let (r, mut w) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);
        let writer = tauri::async_runtime::spawn(async move {
            while let Some(b) = rx.recv().await {
                if w.write_all(&b).await.is_err() {
                    break;
                }
            }
        });
        let hello = Request { id: 0, command: Command::Hello { protocol: PROTOCOL_VERSION, client: format!("lotus-app {}", env!("CARGO_PKG_VERSION")) } };
        if tx.send(encode(&hello)).await.is_err() {
            return;
        }
        *self.tx.lock().unwrap() = Some(tx);
        let mut lines = BufReader::new(r).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(msg) = serde_json::from_str::<ServerMessage>(&line) else { continue };
            match msg {
                ServerMessage::Status { status } => {
                    let first = !self.connected.swap(true, Ordering::SeqCst);
                    *self.last_status.lock().unwrap() = Some(status.clone());
                    if first {
                        let _ = app.emit("lotus://service", ServiceEvent { connected: true, message: None });
                        on_service_up(app);
                    }
                    let _ = app.emit("lotus://status", &status);
                    crate::tray::refresh(app);
                }
                ServerMessage::Response { id: 0, error: Some(e), .. } => {
                    let _ = app.emit("lotus://service", ServiceEvent { connected: false, message: Some(e.message) });
                    break;
                }
                ServerMessage::Response { id, result, error } => {
                    if let Some(p) = self.pending.lock().unwrap().remove(&id) {
                        let _ = p.send(match error {
                            Some(e) => Err(e),
                            None => Ok(result.unwrap_or(Value::Null)),
                        });
                    }
                }
            }
        }
        *self.tx.lock().unwrap() = None;
        writer.abort();
    }

    pub async fn request(&self, command: Command, timeout: Duration) -> Result<Value, ErrorInfo> {
        let tx = self.tx.lock().unwrap().clone().ok_or_else(|| ErrorInfo::new("service_unavailable", "The LOTUS service isn't running. Use Settings → Advanced → Repair service."))?;
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (otx, orx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, otx);
        if tx.send(encode(&Request { id, command })).await.is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(ErrorInfo::new("service_unavailable", "The LOTUS service isn't responding."));
        }
        match tokio::time::timeout(timeout, orx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(ErrorInfo::new("service_unavailable", "The LOTUS service disconnected.")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(ErrorInfo::new("timeout", "The LOTUS service took too long to answer."))
            }
        }
    }

    pub fn status(&self) -> Option<Status> {
        self.last_status.lock().unwrap().clone()
    }
}

/// First contact with the service after launch: honour "connect on launch".
fn on_service_up(app: &AppHandle) {
    static DONE: std::sync::Once = std::sync::Once::new();
    let app = app.clone();
    DONE.call_once(move || {
        let state = app.state::<crate::AppState>();
        let want = state.store.lock().unwrap().settings.connect_on_launch;
        let idle = state.svc.status().map(|s| matches!(s.state, ConnState::Disconnected | ConnState::Error)).unwrap_or(true);
        if want && idle {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::commands::connect_current(&app2).await;
            });
        }
    });
}
