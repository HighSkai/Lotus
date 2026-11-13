//! LOTUS privileged service.
//!
//! Subcommands:
//!   run              entry point used by the Service Control Manager
//!   console          run in the foreground (development; requires Administrator)
//!   install          register and start the Windows service (Administrator)
//!   uninstall        stop and remove the service, restore networking (Administrator)
//!   restore-network  emergency: restore saved DNS settings (Administrator)
//!   ctl …            command-line client (any user)
//!   version

#![cfg_attr(not(windows), allow(dead_code, unused_imports))]

mod controller;
mod ctl;
mod diagnostics;
mod engine;
mod explain;
mod ipc_server;
mod paths;
mod probe;
mod session;
mod state;
#[cfg(windows)]
mod win;

use anyhow::Result;
use controller::{Controller, SysEvent};
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
use win::svc::ScmEvent;

fn init_tracing(console: bool) {
    let filter = tracing_subscriber::EnvFilter::try_from_env("LOTUS_LOG").unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(if console { "info" } else { "warn" }));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_ansi(false).with_writer(std::io::stderr).try_init();
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("run") => {
            init_tracing(false);
            win::svc::run_dispatcher(service_runner)
        }
        Some("console") => {
            init_tracing(true);
            console()
        }
        Some("install") => win::svc::install().map(|_| println!("LOTUS service installed and started.")),
        Some("uninstall") => uninstall(),
        Some("restore-network") => restore_network(),
        Some("ctl") => ctl::main(&args[1..]),
        Some("version") => {
            println!("lotus-service {} (engine sing-box {})", controller::SERVICE_VERSION, lotus_core::singbox::ENGINE_VERSION);
            Ok(())
        }
        _ => {
            eprintln!("usage: lotus-service <run|console|install|uninstall|restore-network|ctl|version>");
            std::process::exit(2);
        }
    };
    if let Err(e) = res {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The LOTUS service currently supports Windows only. See docs/ARCHITECTURE.md for the platform roadmap.");
    std::process::exit(1);
}

#[cfg(windows)]
fn service_runner(_tx: tokio::sync::mpsc::UnboundedSender<ScmEvent>, mut rx: tokio::sync::mpsc::UnboundedReceiver<ScmEvent>) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(async move {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel::<SysEvent>();
        tokio::spawn(async move {
            let mut stop_tx = Some(stop_tx);
            while let Some(ev) = rx.recv().await {
                match ev {
                    ScmEvent::Stop | ScmEvent::Shutdown => {
                        if let Some(t) = stop_tx.take() {
                            let _ = t.send(());
                        }
                    }
                    ScmEvent::Resume => {
                        let _ = ev_tx.send(SysEvent::Resumed);
                    }
                    ScmEvent::Suspend => {}
                }
            }
        });
        serve(async move {
            let _ = stop_rx.await;
        }, ev_rx)
        .await
    })
}

#[cfg(windows)]
fn console() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    rt.block_on(async {
        let (_ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel::<SysEvent>();
        println!("LOTUS service running in console mode. Press Ctrl+C to stop.");
        serve(async {
            let _ = tokio::signal::ctrl_c().await;
        }, ev_rx)
        .await
    })
}

#[cfg(windows)]
async fn serve(stop: impl std::future::Future<Output = ()>, mut scm_events: tokio::sync::mpsc::UnboundedReceiver<SysEvent>) -> Result<()> {
    let paths = paths::Paths::discover()?;
    paths.prepare()?;
    let ctrl = Controller::new(paths)?;
    ctrl.startup_recovery().await;

    // System events → controller.
    let (net_tx, mut net_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let _monitor = win::netmon::NetMonitor::start(net_tx).map_err(|e| tracing::warn!("network monitor unavailable: {e}")).ok();
    let events = ctrl.inner().events.clone();
    tokio::spawn({
        let events = events.clone();
        async move {
            while net_rx.recv().await.is_some() {
                let _ = events.send(SysEvent::NetworkChanged);
            }
        }
    });
    tokio::spawn(async move {
        while let Some(ev) = scm_events.recv().await {
            let _ = events.send(ev);
        }
    });

    let cancel = CancellationToken::new();
    let server = tokio::spawn(ipc_server::serve(ctrl.clone(), cancel.clone()));
    stop.await;
    tracing::info!("stopping");
    cancel.cancel();
    ctrl.shutdown().await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
    Ok(())
}

#[cfg(windows)]
fn restore_network() -> Result<()> {
    if win::svc::is_running() {
        anyhow::bail!("The LOTUS service is running. Disconnect in the app (or stop the service) first.");
    }
    let had_ks = win::wfp::kill_switch_present();
    win::wfp::kill_switch_disable()?;
    let paths = paths::Paths::discover()?;
    if paths.state_file.exists() {
        let ctrl = Controller::new(paths)?;
        ctrl.inner().restore_dns();
        println!("Saved DNS settings restored.");
    }
    if had_ks {
        println!("Kill switch filters removed.");
    }
    println!("Networking restored.");
    Ok(())
}

#[cfg(windows)]
fn uninstall() -> Result<()> {
    win::svc::uninstall()?;
    let _ = restore_network();
    if let Ok(p) = paths::Paths::discover() {
        state::clear_session(&p.session_file);
        let _ = std::fs::remove_dir_all(&p.work_dir);
        let _ = std::fs::remove_file(&p.state_file);
        let _ = std::fs::remove_dir(&p.data_dir);
    }
    println!("LOTUS service removed and networking restored.");
    Ok(())
}
