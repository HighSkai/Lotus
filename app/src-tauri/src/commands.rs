//! Commands callable from the web view. Each one is narrow and typed; the web
//! view cannot run programs, touch files or reach the network directly.

use crate::store::AppSettings;
use crate::AppState;
use lotus_core::import::{parse_import, share_link, ImportIssue};
use lotus_core::ipc::*;
use lotus_core::providers::{ProviderInfo, PROVIDERS};
use lotus_core::*;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub const SERVER_SCRIPT: &str = include_str!("../../../server/lotus-server-install.sh");

type CmdResult<T> = Result<T, ErrorInfo>;

fn err(code: &str, msg: impl Into<String>) -> ErrorInfo {
    ErrorInfo::new(code, msg)
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub settings: AppSettings,
    pub profiles: Vec<ServerProfile>,
    pub status: Option<Status>,
    pub service_connected: bool,
    pub providers: &'static [ProviderInfo],
    pub version: &'static str,
    pub engine_version: &'static str,
    pub store_warning: Option<String>,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Bootstrap {
    let store = state.store.lock().unwrap();
    Bootstrap {
        settings: store.settings.clone(),
        profiles: store.profiles.clone(),
        status: state.svc.status(),
        service_connected: state.svc.connected.load(std::sync::atomic::Ordering::SeqCst),
        providers: PROVIDERS,
        version: env!("CARGO_PKG_VERSION"),
        engine_version: singbox::ENGINE_VERSION,
        store_warning: store.load_error.clone(),
    }
}

/// The front end calls this once it has painted, so the window never flashes blank.
#[tauri::command]
pub fn app_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Show the window, creating it if it was released to save memory. The web
/// view is only alive while the window is open; in the tray LOTUS idles as a
/// few MB of native code.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    create_main(app);
}

/// Create the (initially invisible) main window; the front end calls
/// `app_ready` once painted, which shows it.
pub fn create_main(app: &AppHandle) {
    let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() else { return };
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if app.get_webview_window("main").is_none() {
            if let Err(e) = tauri::WebviewWindowBuilder::from_config(&app, &cfg).and_then(|b| b.build()) {
                tracing::error!("create window: {e}");
            }
        }
    });
}

fn emit_settings(app: &AppHandle, s: &AppSettings) {
    let _ = app.emit("lotus://settings", s);
    crate::tray::refresh(app);
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> CmdResult<AppSettings> {
    validate::validate_lotus_options(&settings.lotus).map_err(|e| err("invalid", e.to_string()))?;
    validate::validate_shadow_options(&settings.shadow).map_err(|e| err("invalid", e.to_string()))?;
    let autostart_changed;
    {
        let mut store = state.store.lock().unwrap();
        autostart_changed = store.settings.launch_at_startup != settings.launch_at_startup;
        store.settings = settings.clone();
        store.save_settings().map_err(|e| err("io", format!("Couldn't save settings: {e}")))?;
    }
    if autostart_changed {
        let al = app.autolaunch();
        let r = if settings.launch_at_startup { al.enable() } else { al.disable() };
        r.map_err(|e| err("autostart", format!("Couldn't change launch at startup: {e}")))?;
    }
    emit_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub async fn set_mode(app: AppHandle, mode: Mode) -> CmdResult<AppSettings> {
    switch_mode(&app, mode).await
}

pub async fn switch_mode(app: &AppHandle, mode: Mode) -> CmdResult<AppSettings> {
    let state = app.state::<AppState>();
    let settings = {
        let mut store = state.store.lock().unwrap();
        store.settings.mode = mode;
        let _ = store.save_settings();
        store.settings.clone()
    };
    emit_settings(app, &settings);
    // Switching while protected moves the protection to the new mode.
    let active = state.svc.status().map(|s| matches!(s.state, ConnState::Connected | ConnState::Connecting | ConnState::Reconnecting) && s.mode != Some(mode)).unwrap_or(false);
    if active {
        if mode == Mode::Shadow && state.store.lock().unwrap().selected().is_none() {
            state.svc.request(Command::Disconnect, Duration::from_secs(30)).await?;
        } else {
            connect_current(app).await?;
        }
    }
    Ok(settings)
}

pub async fn connect_current(app: &AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let cmd = {
        let store = state.store.lock().unwrap();
        let s = &store.settings;
        match s.mode {
            Mode::Lotus => Command::ConnectLotus { options: s.lotus.clone() },
            Mode::Shadow => {
                let profile = store.selected().cloned().ok_or_else(|| err("no_server", "Add or choose a server for SHADOW first."))?;
                Command::ConnectShadow { profile, options: s.shadow.clone() }
            }
        }
    };
    state.svc.request(cmd, Duration::from_secs(30)).await.map(|_| ())
}

#[tauri::command]
pub async fn connect(app: AppHandle) -> CmdResult<()> {
    connect_current(&app).await
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>) -> CmdResult<()> {
    state.svc.request(Command::Disconnect, Duration::from_secs(40)).await.map(|_| ())
}

#[derive(Serialize)]
pub struct ImportResult {
    pub added: Vec<ServerProfile>,
    pub issues: Vec<ImportIssue>,
    pub profiles: Vec<ServerProfile>,
    pub selected_profile: Option<String>,
}

fn import_into(app: &AppHandle, text: &str) -> CmdResult<ImportResult> {
    if text.len() > 2 * 1024 * 1024 {
        return Err(err("too_large", "That's too large to be a server configuration."));
    }
    let report = parse_import(text);
    let state = app.state::<AppState>();
    let mut store = state.store.lock().unwrap();
    let mut added = Vec::new();
    for p in report.profiles {
        // Skip exact duplicates (same protocol settings).
        if store.profiles.iter().any(|e| e.protocol == p.protocol) {
            continue;
        }
        added.push(p.clone());
        store.profiles.push(p);
    }
    if store.settings.selected_profile.is_none() || store.selected().is_none() {
        store.settings.selected_profile = added.first().map(|p| p.id.clone()).or(store.settings.selected_profile.clone());
        let _ = store.save_settings();
    }
    if !added.is_empty() {
        store.save_profiles().map_err(|e| err("io", format!("Couldn't save servers securely: {e}")))?;
    }
    let mut issues = report.issues;
    if added.is_empty() && issues.is_empty() {
        issues.push(ImportIssue { source: "import".into(), message: "Those servers are already saved.".into() });
    }
    let res = ImportResult { added, issues, profiles: store.profiles.clone(), selected_profile: store.settings.selected_profile.clone() };
    let settings = store.settings.clone();
    drop(store);
    emit_settings(app, &settings);
    Ok(res)
}

#[tauri::command]
pub fn import_text(app: AppHandle, text: String) -> CmdResult<ImportResult> {
    import_into(&app, &text)
}

#[tauri::command]
pub fn import_clipboard(app: AppHandle) -> CmdResult<ImportResult> {
    let text = app.clipboard().read_text().map_err(|_| err("clipboard", "The clipboard doesn't contain text."))?;
    import_into(&app, &text)
}

#[tauri::command]
pub fn save_profile(app: AppHandle, state: State<'_, AppState>, mut profile: ServerProfile) -> CmdResult<Vec<ServerProfile>> {
    profile.name = profile.name.trim().to_string();
    if profile.id.is_empty() {
        profile.id = new_profile_id();
        profile.created_at = now_ms();
    }
    validate::validate_profile(&profile).map_err(|e| err("invalid", e.to_string()))?;
    let mut store = state.store.lock().unwrap();
    match store.profiles.iter_mut().find(|p| p.id == profile.id) {
        Some(p) => *p = profile.clone(),
        None => store.profiles.push(profile.clone()),
    }
    if store.selected().is_none() {
        store.settings.selected_profile = Some(profile.id.clone());
        let _ = store.save_settings();
    }
    store.save_profiles().map_err(|e| err("io", format!("Couldn't save servers securely: {e}")))?;
    let list = store.profiles.clone();
    let settings = store.settings.clone();
    drop(store);
    emit_settings(&app, &settings);
    Ok(list)
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<Vec<ServerProfile>> {
    let mut store = state.store.lock().unwrap();
    store.profiles.retain(|p| p.id != id);
    if store.settings.selected_profile.as_deref() == Some(id.as_str()) {
        store.settings.selected_profile = store.profiles.first().map(|p| p.id.clone());
        let _ = store.save_settings();
    }
    store.save_profiles().map_err(|e| err("io", e.to_string()))?;
    let list = store.profiles.clone();
    let settings = store.settings.clone();
    drop(store);
    emit_settings(&app, &settings);
    Ok(list)
}

#[tauri::command]
pub async fn select_profile(app: AppHandle, id: String) -> CmdResult<AppSettings> {
    let state = app.state::<AppState>();
    let settings = {
        let mut store = state.store.lock().unwrap();
        if !store.profiles.iter().any(|p| p.id == id) {
            return Err(err("not_found", "That server no longer exists."));
        }
        store.settings.selected_profile = Some(id.clone());
        let _ = store.save_settings();
        store.settings.clone()
    };
    emit_settings(&app, &settings);
    // Already connected through SHADOW: move to the newly selected server.
    let st = state.svc.status();
    if let Some(st) = st {
        if st.mode == Some(Mode::Shadow) && matches!(st.state, ConnState::Connected | ConnState::Reconnecting) && st.profile_id.as_deref() != Some(id.as_str()) {
            connect_current(&app).await?;
        }
    }
    Ok(settings)
}

#[tauri::command]
pub async fn test_profile(state: State<'_, AppState>, id: String) -> CmdResult<TestResult> {
    let profile = state.store.lock().unwrap().profiles.iter().find(|p| p.id == id).cloned().ok_or_else(|| err("not_found", "Server not found"))?;
    let v = state.svc.request(Command::TestProfile { profile }, Duration::from_secs(30)).await?;
    serde_json::from_value(v).map_err(|e| err("internal", e.to_string()))
}

#[tauri::command]
pub fn copy_share_link(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let link = state.store.lock().unwrap().profiles.iter().find(|p| p.id == id).map(share_link).ok_or_else(|| err("not_found", "Server not found"))?;
    app.clipboard().write_text(link).map_err(|e| err("clipboard", e.to_string()))
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> CmdResult<()> {
    if text.len() > 64 * 1024 {
        return Err(err("too_large", "Too large"));
    }
    app.clipboard().write_text(text).map_err(|e| err("clipboard", e.to_string()))
}

#[tauri::command]
pub async fn diagnostics(state: State<'_, AppState>) -> CmdResult<Value> {
    state.svc.request(Command::Diagnostics, Duration::from_secs(90)).await
}

/// Live connections of the running session (from the engine, never stored).
#[tauri::command]
pub async fn connections(state: State<'_, AppState>) -> CmdResult<Value> {
    state.svc.request(Command::Connections, Duration::from_secs(5)).await
}

/// Local usage history (totals per day, mode and server).
#[tauri::command]
pub async fn usage(state: State<'_, AppState>) -> CmdResult<Value> {
    state.svc.request(Command::Usage, Duration::from_secs(5)).await
}

#[tauri::command]
pub async fn clear_usage(state: State<'_, AppState>) -> CmdResult<Value> {
    state.svc.request(Command::ClearUsage, Duration::from_secs(5)).await
}

/// Check a saved server: authenticated handshake, what strangers see, exit address.
#[tauri::command]
pub async fn verify_server(state: State<'_, AppState>, id: String) -> CmdResult<Value> {
    let profile = state.store.lock().unwrap().profiles.iter().find(|p| p.id == id).cloned().ok_or_else(|| err("not_found", "Server not found"))?;
    state.svc.request(Command::VerifyServer { profile }, Duration::from_secs(60)).await
}

#[tauri::command]
pub async fn speed_test(state: State<'_, AppState>) -> CmdResult<Value> {
    state.svc.request(Command::SpeedTest, Duration::from_secs(90)).await
}

#[tauri::command]
pub async fn logs(state: State<'_, AppState>) -> CmdResult<Value> {
    state.svc.request(Command::Logs, Duration::from_secs(10)).await
}

#[tauri::command]
pub fn server_script() -> &'static str {
    SERVER_SCRIPT
}

#[tauri::command]
pub async fn save_server_script(app: AppHandle) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_file_name("lotus-server-install.sh").add_filter("Shell script", &["sh"]).save_file(move |p| {
        let _ = tx.send(p);
    });
    let Some(path) = rx.await.ok().flatten() else { return Ok(None) };
    let path = path.into_path().map_err(|e| err("io", e.to_string()))?;
    // LF line endings matter for bash.
    std::fs::write(&path, SERVER_SCRIPT.replace("\r\n", "\n")).map_err(|e| err("io", format!("Couldn't save the script: {e}")))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn repair_service() -> CmdResult<()> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(crate::win::repair_service).await.map_err(|e| err("internal", e.to_string()))?.map_err(|e| err("repair", e.to_string()))
    }
    #[cfg(not(windows))]
    Err(err("unsupported", "Not supported on this platform"))
}

pub fn close_requested(app: &AppHandle) {
    let to_tray = app.state::<AppState>().store.lock().unwrap().settings.close_to_tray;
    if to_tray {
        // Release the web view entirely; it is recreated when the window is opened.
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.destroy();
        }
    } else {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { quit_app(&app).await });
    }
}

#[tauri::command]
pub fn close_window(app: AppHandle) {
    close_requested(&app);
}

pub async fn quit_app(app: &AppHandle) {
    let state = app.state::<AppState>();
    let disconnect = state.store.lock().unwrap().settings.disconnect_on_quit;
    if disconnect {
        let _ = state.svc.request(Command::Disconnect, Duration::from_secs(20)).await;
    }
    state.quitting.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

#[tauri::command]
pub async fn quit(app: AppHandle) {
    quit_app(&app).await
}
