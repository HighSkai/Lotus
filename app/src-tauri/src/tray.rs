//! System tray: live status, connect/disconnect, mode switching, open, quit.

use crate::AppState;
use lotus_core::ipc::ConnState;
use lotus_core::Mode;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const ICON_OFF: &[u8] = include_bytes!("../icons/tray/off.png");
const ICON_LOTUS: &[u8] = include_bytes!("../icons/tray/lotus.png");
const ICON_SHADOW: &[u8] = include_bytes!("../icons/tray/shadow.png");
const ICON_BUSY: &[u8] = include_bytes!("../icons/tray/busy.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/tray/error.png");

pub struct TrayItems {
    status: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    lotus: CheckMenuItem<Wry>,
    shadow: CheckMenuItem<Wry>,
    last_icon: std::sync::Mutex<&'static str>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "LOTUS · Starting…", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open LOTUS", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Turn on", true, None::<&str>)?;
    let lotus = CheckMenuItem::with_id(app, "mode_lotus", "LOTUS · private DNS", true, true, None::<&str>)?;
    let shadow = CheckMenuItem::with_id(app, "mode_shadow", "SHADOW · your server", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit LOTUS", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&status, &sep()?, &open, &toggle, &sep()?, &lotus, &shadow, &sep()?, &quit])?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(ICON_OFF)?)
        .tooltip("LOTUS")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| {
            let app = app.clone();
            match ev.id().as_ref() {
                "open" => crate::commands::show_main(&app),
                "quit" => {
                    tauri::async_runtime::spawn(async move { crate::commands::quit_app(&app).await });
                }
                "toggle" => {
                    tauri::async_runtime::spawn(async move {
                        let state = app.state::<AppState>();
                        let on = state.svc.status().map(|s| !matches!(s.state, ConnState::Disconnected | ConnState::Error)).unwrap_or(false);
                        let r = if on {
                            state.svc.request(lotus_core::ipc::Command::Disconnect, std::time::Duration::from_secs(30)).await.map(|_| ())
                        } else {
                            crate::commands::connect_current(&app).await
                        };
                        if r.is_err() {
                            crate::commands::show_main(&app);
                        }
                    });
                }
                id @ ("mode_lotus" | "mode_shadow") => {
                    let mode = if id == "mode_lotus" { Mode::Lotus } else { Mode::Shadow };
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::commands::switch_mode(&app, mode).await;
                        refresh(&app);
                    });
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                crate::commands::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    app.manage(TrayItems { status, toggle, lotus, shadow, last_icon: std::sync::Mutex::new("off") });
    refresh(app);
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    let (Some(items), Some(state)) = (app.try_state::<TrayItems>(), app.try_state::<AppState>()) else { return };
    let mode = state.store.lock().unwrap().settings.mode;
    let up = state.svc.connected.load(std::sync::atomic::Ordering::SeqCst);
    let st = state.svc.status();
    let conn = st.as_ref().map(|s| s.state).unwrap_or_default();
    let active_mode = st.as_ref().and_then(|s| s.mode).unwrap_or(mode);
    let name = |m: Mode| if m == Mode::Lotus { "LOTUS" } else { "SHADOW" };
    let (text, icon): (String, &'static str) = if !up {
        ("LOTUS · Service not running".into(), "error")
    } else {
        match conn {
            ConnState::Connected if active_mode == Mode::Lotus => ("LOTUS · DNS protected".into(), "lotus"),
            ConnState::Connected => (
                format!("SHADOW · Connected{}", st.as_ref().and_then(|s| s.profile_name.clone()).map(|n| format!(" to {n}")).unwrap_or_default()),
                "shadow",
            ),
            ConnState::Connecting => (format!("{} · Connecting…", name(active_mode)), "busy"),
            ConnState::Reconnecting => (format!("{} · Reconnecting…", name(active_mode)), "busy"),
            ConnState::Disconnecting => (format!("{} · Turning off…", name(active_mode)), "busy"),
            ConnState::Error => (format!("{} · Needs attention", name(active_mode)), "error"),
            ConnState::Disconnected => (format!("{} · Off", name(mode)), "off"),
        }
    };
    let _ = items.status.set_text(&text);
    let on = up && !matches!(conn, ConnState::Disconnected | ConnState::Error);
    let _ = items.toggle.set_text(if on { "Turn off" } else { "Turn on" });
    let _ = items.toggle.set_enabled(up);
    let _ = items.lotus.set_checked(mode == Mode::Lotus);
    let _ = items.shadow.set_checked(mode == Mode::Shadow);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(&text));
        let mut last = items.last_icon.lock().unwrap();
        if *last != icon {
            *last = icon;
            let bytes = match icon {
                "lotus" => ICON_LOTUS,
                "shadow" => ICON_SHADOW,
                "busy" => ICON_BUSY,
                "error" => ICON_ERROR,
                _ => ICON_OFF,
            };
            if let Ok(img) = Image::from_bytes(bytes) {
                let _ = tray.set_icon(Some(img));
            }
        }
    }
}
