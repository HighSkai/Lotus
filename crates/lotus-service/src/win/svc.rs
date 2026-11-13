//! Service Control Manager integration: install, uninstall and the service entry point.

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;
use windows_service::service::*;
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

pub const SERVICE_NAME: &str = "LotusService";
pub const DISPLAY_NAME: &str = "LOTUS Privacy Service";
const DESCRIPTION: &str = "Runs the LOTUS encrypted DNS resolver and SHADOW tunnel. Restores normal networking when stopped.";

/// Events the SCM delivers to the running service.
#[derive(Debug, Clone, Copy)]
pub enum ScmEvent {
    Stop,
    Shutdown,
    Resume,
    Suspend,
}

static RUNNER: OnceLock<fn(UnboundedSender<ScmEvent>, tokio::sync::mpsc::UnboundedReceiver<ScmEvent>) -> Result<()>> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub fn run_dispatcher(runner: fn(UnboundedSender<ScmEvent>, tokio::sync::mpsc::UnboundedReceiver<ScmEvent>) -> Result<()>) -> Result<()> {
    let _ = RUNNER.set(runner);
    service_dispatcher::start(SERVICE_NAME, ffi_service_main).context("service dispatcher (run this binary via the Service Control Manager, or use `console`)")?;
    Ok(())
}

fn service_main(_args: Vec<OsString>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ScmEvent>();
    let handler_tx = tx.clone();
    let handler = move |ev: ServiceControl| -> ServiceControlHandlerResult {
        match ev {
            ServiceControl::Stop => {
                let _ = handler_tx.send(ScmEvent::Stop);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Shutdown | ServiceControl::Preshutdown => {
                let _ = handler_tx.send(ScmEvent::Shutdown);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::PowerEvent(p) => {
                match p {
                    PowerEventParam::ResumeAutomatic | PowerEventParam::ResumeSuspend => {
                        let _ = handler_tx.send(ScmEvent::Resume);
                    }
                    PowerEventParam::Suspend => {
                        let _ = handler_tx.send(ScmEvent::Suspend);
                    }
                    _ => {}
                }
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let Ok(status) = service_control_handler::register(SERVICE_NAME, handler) else { return };
    let set = |state: ServiceState, accept: ServiceControlAccept, code: u32| {
        let _ = status.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: accept,
            exit_code: ServiceExitCode::Win32(code),
            checkpoint: 0,
            wait_hint: Duration::from_secs(15),
            process_id: None,
        });
    };
    set(ServiceState::Running, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN | ServiceControlAccept::POWER_EVENT, 0);
    let code = match RUNNER.get().map(|r| r(tx, rx)) {
        Some(Ok(())) => 0,
        _ => 1,
    };
    set(ServiceState::Stopped, ServiceControlAccept::empty(), code);
}

pub fn install() -> Result<()> {
    let exe = std::env::current_exe()?;
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
        .context("open Service Control Manager (run as Administrator)")?;
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::START | ServiceAccess::STOP | ServiceAccess::CHANGE_CONFIG | ServiceAccess::DELETE;
    let info = ServiceInfo {
        name: SERVICE_NAME.into(),
        display_name: DISPLAY_NAME.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments: vec!["run".into()],
        dependencies: vec![ServiceDependency::Service("Dnscache".into()), ServiceDependency::Service("BFE".into())],
        account_name: None,
        account_password: None,
    };
    let service = match manager.open_service(SERVICE_NAME, access) {
        Ok(s) => {
            // Upgrade in place: stop, then reconfigure.
            let _ = s.stop();
            wait_for(&s, ServiceState::Stopped, Duration::from_secs(20));
            s.change_config(&info).context("update service configuration")?;
            s
        }
        Err(_) => manager.create_service(&info, access).context("create service")?,
    };
    service.set_description(DESCRIPTION)?;
    service.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 3600)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(1) },
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(3) },
            ServiceAction { action_type: ServiceActionType::Restart, delay: Duration::from_secs(10) },
        ]),
    })?;
    service.set_failure_actions_on_non_crash_failures(true)?;
    reset_service_acl();
    service.start::<&str>(&[]).context("start service")?;
    wait_for(&service, ServiceState::Running, Duration::from_secs(20));
    Ok(())
}

/// Restore the standard Windows service DACL: only SYSTEM and Administrators
/// may start/stop/configure the service; users may only query it. This also
/// removes any permissions a development install may have added.
fn reset_service_acl() {
    const DEFAULT_SDDL: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)";
    let sc = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| r"C:\Windows".into()).join(r"System32\sc.exe");
    let _ = std::process::Command::new(sc).args(["sdset", SERVICE_NAME, DEFAULT_SDDL]).stdout(std::process::Stdio::null()).status();
}

pub fn uninstall() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).context("open Service Control Manager (run as Administrator)")?;
    let service = match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let _ = service.stop();
    wait_for(&service, ServiceState::Stopped, Duration::from_secs(20));
    service.delete().context("delete service")?;
    Ok(())
}

pub fn is_running() -> bool {
    let Ok(manager) = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT) else { return false };
    let Ok(s) = manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) else { return false };
    matches!(s.query_status().map(|st| st.current_state), Ok(ServiceState::Running))
}

fn wait_for(s: &windows_service::service::Service, state: ServiceState, timeout: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        match s.query_status() {
            Ok(st) if st.current_state == state => return,
            Err(_) => return,
            _ => std::thread::sleep(Duration::from_millis(250)),
        }
    }
}
