//! App updates: background checks and a manual "Check for updates".
//!
//! Releases publish a `latest.json` next to the installer. The installer is
//! signed with the LOTUS update key and the plugin verifies that signature
//! against the public key baked into the app before anything runs. Installing
//! starts the (elevated) NSIS installer, which restarts the service; the
//! service resumes the active session on its own, and LOTUS relaunches.

use crate::AppState;
use lotus_core::ipc::ErrorInfo;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// First background check shortly after start, then every few hours.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Ready,
    Installing,
    Error,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateInfo {
    pub phase: Phase,
    pub current: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    /// 0.0–1.0 while downloading, when the size is known.
    pub progress: Option<f32>,
    pub error: Option<String>,
    /// Unix ms of the last completed check.
    pub checked_at: Option<u64>,
}

pub struct Updates {
    info: Mutex<UpdateInfo>,
    update: Mutex<Option<Update>>,
    bytes: Mutex<Option<Vec<u8>>>,
    busy: tokio::sync::Mutex<()>,
}

impl Updates {
    pub fn new(current: String) -> Self {
        Self {
            info: Mutex::new(UpdateInfo { current, ..Default::default() }),
            update: Mutex::new(None),
            bytes: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn set(app: &AppHandle, f: impl FnOnce(&mut UpdateInfo)) -> UpdateInfo {
    let updates = app.state::<Updates>();
    let info = {
        let mut i = updates.info.lock().unwrap();
        f(&mut i);
        i.clone()
    };
    let _ = app.emit("lotus://update", &info);
    crate::tray::refresh_update(app, &info);
    info
}

pub fn info(app: &AppHandle) -> UpdateInfo {
    app.state::<Updates>().info.lock().unwrap().clone()
}

fn friendly(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Reqwest(_) | E::Network(_) => "Couldn't reach the update server. Check your connection and try again.".into(),
        E::ReleaseNotFound => "No update information was found for this release.".into(),
        E::Minisign(_) | E::SignedVersionMismatch { .. } | E::MissingSignedVersion => "The update's signature didn't check out, so it was not installed.".into(),
        E::Io(io) if io.raw_os_error() == Some(5) => "The update was cancelled at the administrator prompt.".into(),
        other => format!("Update failed: {other}"),
    }
}

fn auto_enabled(app: &AppHandle) -> bool {
    app.state::<AppState>().store.lock().unwrap().settings.auto_update
}

/// Look for an update. With automatic updates on, a found update is also
/// downloaded so it's ready to install in one click.
pub async fn check(app: &AppHandle) -> UpdateInfo {
    let updates = app.state::<Updates>();
    let Ok(_guard) = updates.busy.try_lock() else { return info(app) };
    if matches!(info(app).phase, Phase::Ready | Phase::Installing) {
        return info(app);
    }
    set(app, |i| {
        i.phase = Phase::Checking;
        i.error = None;
        i.progress = None;
    });
    let found = match app.updater_builder().timeout(Duration::from_secs(30)).build() {
        Ok(u) => u.check().await,
        Err(e) => Err(e),
    };
    match found {
        Ok(None) => set(app, |i| {
            i.phase = Phase::UpToDate;
            i.version = None;
            i.notes = None;
            i.checked_at = Some(now_ms());
        }),
        Ok(Some(update)) => {
            let version = update.version.clone();
            let notes = update.body.clone();
            *updates.update.lock().unwrap() = Some(update);
            *updates.bytes.lock().unwrap() = None;
            set(app, |i| {
                i.phase = Phase::Available;
                i.version = Some(version);
                i.notes = notes;
                i.checked_at = Some(now_ms());
            });
            if auto_enabled(app) {
                let _ = download(app).await;
            }
            info(app)
        }
        Err(e) => {
            tracing::warn!("update check failed: {e}");
            set(app, |i| {
                i.phase = Phase::Error;
                i.error = Some(friendly(&e));
                i.checked_at = Some(now_ms());
            })
        }
    }
}

async fn download(app: &AppHandle) -> Result<(), ErrorInfo> {
    let updates = app.state::<Updates>();
    let Some(update) = updates.update.lock().unwrap().clone() else {
        return Err(ErrorInfo::new("no_update", "There's no update to install."));
    };
    set(app, |i| {
        i.phase = Phase::Downloading;
        i.progress = Some(0.0);
    });
    let mut got = 0usize;
    let mut last = Instant::now();
    let app2 = app.clone();
    let res = update
        .download(
            |chunk, total| {
                got += chunk;
                if last.elapsed() >= Duration::from_millis(150) {
                    last = Instant::now();
                    let p = total.filter(|t| *t > 0).map(|t| (got as f32 / t as f32).min(1.0));
                    set(&app2, |i| i.progress = p);
                }
            },
            || {},
        )
        .await;
    match res {
        Ok(bytes) => {
            *updates.bytes.lock().unwrap() = Some(bytes);
            set(app, |i| {
                i.phase = Phase::Ready;
                i.progress = Some(1.0);
            });
            Ok(())
        }
        Err(e) => {
            tracing::warn!("update download failed: {e}");
            let msg = friendly(&e);
            set(app, |i| {
                i.phase = Phase::Error;
                i.error = Some(msg.clone());
                i.progress = None;
            });
            Err(ErrorInfo::new("update_failed", msg))
        }
    }
}

/// Download (if needed) and run the installer. On success the process exits.
pub async fn install(app: &AppHandle) -> Result<(), ErrorInfo> {
    let updates = app.state::<Updates>();
    let _guard = updates.busy.lock().await;
    if updates.bytes.lock().unwrap().is_none() {
        download(app).await?;
    }
    let (Some(update), Some(bytes)) = (updates.update.lock().unwrap().clone(), updates.bytes.lock().unwrap().take()) else {
        return Err(ErrorInfo::new("no_update", "There's no update to install."));
    };
    set(app, |i| i.phase = Phase::Installing);
    // The installer replaces this executable; let the close handler know we're leaving.
    app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
    if let Err(e) = update.install(&bytes) {
        tracing::warn!("update install failed: {e}");
        app.state::<AppState>().quitting.store(false, Ordering::SeqCst);
        *updates.bytes.lock().unwrap() = Some(bytes);
        let msg = friendly(&e);
        set(app, |i| {
            i.phase = Phase::Ready;
            i.error = Some(msg.clone());
        });
        return Err(ErrorInfo::new("update_failed", msg));
    }
    Ok(())
}

/// Periodic checks while automatic updates are on (the app usually lives in the tray).
pub fn spawn_background(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if auto_enabled(&app) {
                check(&app).await;
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateInfo {
    info(&app)
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> UpdateInfo {
    check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), ErrorInfo> {
    install(&app).await
}
