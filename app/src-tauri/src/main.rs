// LOTUS desktop app. Runs unprivileged; all privileged networking happens in
// the LOTUS service, reached over a local named pipe with a closed command set.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod service_client;
mod store;
mod tray;
mod updater;
#[cfg(windows)]
mod win;

use service_client::ServiceClient;
use std::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub store: Mutex<store::Store>,
    pub svc: ServiceClient,
    pub quitting: std::sync::atomic::AtomicBool,
    /// Bumped by every connect or disconnect the user asks for. A switch that
    /// is still waiting for its outcome gives up (and never rolls back) once
    /// the user has asked for something newer.
    pub ops: std::sync::atomic::AtomicU64,
}

fn main() {
    let launched_hidden = std::env::args().any(|a| a == "--hidden");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            commands::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
            let dir = app.path().app_local_data_dir()?;
            let store = store::Store::load(dir);
            let svc = ServiceClient::start(app.handle().clone());
            app.manage(AppState { store: Mutex::new(store), svc, quitting: Default::default(), ops: Default::default() });
            app.manage(updater::Updates::new(app.package_info().version.to_string()));
            tray::create(app.handle())?;
            updater::spawn_background(app.handle().clone());
            if !launched_hidden {
                commands::create_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // The custom title bar decides between hide-to-tray and quit; a
                // system close (Alt+F4) follows the same preference.
                let app = window.app_handle();
                let state = app.state::<AppState>();
                if state.quitting.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                api.prevent_close();
                commands::close_requested(app);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::app_ready,
            commands::save_settings,
            commands::set_mode,
            commands::connect,
            commands::disconnect,
            commands::import_text,
            commands::import_clipboard,
            commands::save_profile,
            commands::delete_profile,
            commands::select_profile,
            commands::test_profile,
            commands::copy_share_link,
            commands::copy_text,
            commands::diagnostics,
            commands::speed_test,
            commands::connections,
            commands::verify_server,
            commands::usage,
            commands::running_apps,
            commands::clear_usage,
            commands::logs,
            commands::save_server_script,
            commands::server_script,
            commands::repair_service,
            commands::close_window,
            commands::quit,
            updater::update_status,
            updater::check_update,
            updater::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LOTUS")
        .run(|app, event| {
            // Closing the window releases the web view but keeps LOTUS in the
            // tray; only an explicit quit (which passes an exit code) ends it.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                let quitting = app.state::<AppState>().quitting.load(std::sync::atomic::Ordering::SeqCst);
                if code.is_none() && !quitting {
                    api.prevent_exit();
                }
            }
        });
}
