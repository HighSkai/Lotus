//! The connection controller: single owner of connection state, the engine,
//! system DNS changes and firewall guards.

use crate::engine::{self, Engine, LogSink, Spawn};
use crate::paths::Paths;
use crate::state::{self, SessionRecord, StateStore};
use lotus_core::ipc::*;
use lotus_core::redact::Redactor;
use lotus_core::*;
use serde_json::Value;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch, Semaphore};
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
use crate::win::{adapters, wfp};

pub const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysEvent {
    NetworkChanged,
    Resumed,
}

#[derive(Debug, Clone, Default)]
pub struct Origin {
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub country: Option<String>,
    pub measured: Option<Instant>,
}

struct Session {
    mode: Mode,
    cancel: CancellationToken,
    keep_record: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

pub struct Inner {
    pub paths: Paths,
    status: watch::Sender<Status>,
    session: tokio::sync::Mutex<Option<Session>>,
    #[cfg(windows)]
    dns_guard: Mutex<Option<wfp::DnsGuard>>,
    kill_switch_on: AtomicBool,
    #[cfg(windows)]
    pub job: crate::win::job::Job,
    pub state: Mutex<StateStore>,
    pub logs: LogSink,
    pub events: broadcast::Sender<SysEvent>,
    pub origin: Mutex<Origin>,
    test_slots: Semaphore,
}

#[derive(Clone)]
pub struct Controller(pub Arc<Inner>);

pub fn fail(code: &str, msg: impl Into<String>) -> ErrorInfo {
    ErrorInfo::new(code, msg)
}

impl Controller {
    pub fn new(paths: Paths) -> anyhow::Result<Self> {
        let state = StateStore::load(paths.state_file.clone());
        let status = Status { service_version: SERVICE_VERSION.into(), engine_version: singbox::ENGINE_VERSION.into(), ..Default::default() };
        let (status_tx, _) = watch::channel(status);
        let (events, _) = broadcast::channel(32);
        Ok(Controller(Arc::new(Inner {
            paths,
            status: status_tx,
            session: tokio::sync::Mutex::new(None),
            #[cfg(windows)]
            dns_guard: Mutex::new(None),
            kill_switch_on: AtomicBool::new(false),
            #[cfg(windows)]
            job: crate::win::job::Job::new()?,
            state: Mutex::new(state),
            logs: LogSink::default(),
            events,
            origin: Mutex::new(Origin::default()),
            test_slots: Semaphore::new(2),
        })))
    }

    pub fn inner(&self) -> &Arc<Inner> {
        &self.0
    }

    pub fn subscribe(&self) -> watch::Receiver<Status> {
        self.0.status.subscribe()
    }

    pub fn status(&self) -> Status {
        self.0.status.borrow().clone()
    }

    pub async fn connect_lotus(&self, opts: LotusOptions, resumed: bool) -> Result<(), ErrorInfo> {
        validate::validate_lotus_options(&opts).map_err(|e| fail("invalid_options", e.to_string()))?;
        self.stop_session(false).await;
        let cancel = CancellationToken::new();
        let keep = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(crate::session::run_lotus(self.0.clone(), opts, cancel.clone(), keep.clone(), resumed));
        *self.0.session.lock().await = Some(Session { mode: Mode::Lotus, cancel, keep_record: keep, task });
        Ok(())
    }

    pub async fn connect_shadow(&self, profile: ServerProfile, opts: ShadowOptions, resumed: bool) -> Result<(), ErrorInfo> {
        validate::validate_profile(&profile).map_err(|e| fail("invalid_profile", e.to_string()))?;
        validate::validate_shadow_options(&opts).map_err(|e| fail("invalid_options", e.to_string()))?;
        // When resuming, keep a kill switch left by the previous run in place;
        // the session re-arms it atomically.
        self.stop_session_ex(false, !resumed).await;
        let cancel = CancellationToken::new();
        let keep = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(crate::session::run_shadow(self.0.clone(), profile, opts, cancel.clone(), keep.clone(), resumed));
        *self.0.session.lock().await = Some(Session { mode: Mode::Shadow, cancel, keep_record: keep, task });
        Ok(())
    }

    /// Stop the active session. `keep_record` preserves the resume record
    /// (service stop / system shutdown) instead of forgetting the session.
    pub async fn stop_session(&self, keep_record: bool) {
        self.stop_session_ex(keep_record, true).await
    }

    async fn stop_session_ex(&self, keep_record: bool, cleanup: bool) {
        let s = self.0.session.lock().await.take();
        if let Some(s) = s {
            s.keep_record.store(keep_record, Ordering::SeqCst);
            if !s.task.is_finished() {
                self.0.update(|st| st.state = ConnState::Disconnecting);
            }
            let _ = s.mode;
            s.cancel.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(20), s.task).await;
        }
        if !keep_record {
            state::clear_session(&self.0.paths.session_file);
        }
        // Whatever happened, never leave guards or DNS overrides behind.
        if cleanup {
            self.0.cleanup_system();
        }
        self.0.update(|st| {
            if st.state != ConnState::Error || !keep_record {
                st.state = ConnState::Disconnected;
                st.error = None;
            }
            st.connected_since = None;
            st.latency_ms = None;
            st.traffic = Traffic::default();
            st.protections.clear();
            st.kill_switch_engaged = self.0.kill_switch_active();
        });
    }

    /// Called once at service start.
    pub async fn startup_recovery(&self) {
        // Restore any DNS settings left behind by a crash before doing anything else.
        self.0.restore_dns();
        #[cfg(windows)]
        let leftover_kill_switch = wfp::kill_switch_present();
        #[cfg(not(windows))]
        let leftover_kill_switch = false;
        let rec = state::load_session(&self.0.paths.session_file);
        #[cfg(windows)]
        let boot_now = crate::win::boot_epoch_secs();
        #[cfg(not(windows))]
        let boot_now = 0;
        let resume = rec.as_ref().map(|r| r.boot_epoch.abs_diff(boot_now) < 120 || r.resume_on_boot).unwrap_or(false);
        let resume_shadow_ks = resume && rec.as_ref().map(|r| r.mode == Mode::Shadow && r.shadow.as_ref().map(|o| o.kill_switch).unwrap_or(false)).unwrap_or(false);
        if leftover_kill_switch {
            if resume_shadow_ks {
                // Keep blocking until the tunnel is back.
                self.0.mark_kill_switch_on();
            } else {
                self.0.log("removing kill switch left by a previous run");
                self.0.disable_guards();
            }
        }
        let Some(rec) = rec else { return };
        if !resume {
            state::clear_session(&self.0.paths.session_file);
            return;
        }
        let same_boot = rec.boot_epoch.abs_diff(boot_now) < 120;
        self.0.log(format!("resuming {:?} session after {}", rec.mode, if same_boot { "service restart" } else { "reboot" }));
        let res = match (rec.mode, rec.lotus, rec.profile, rec.shadow) {
            (Mode::Lotus, Some(o), _, _) => self.connect_lotus(o, true).await,
            (Mode::Shadow, _, Some(p), Some(o)) => self.connect_shadow(p, o, true).await,
            _ => Ok(()),
        };
        if let Err(e) = res {
            self.0.log(format!("resume failed: {}", e.message));
        }
    }

    pub async fn shutdown(&self) {
        self.stop_session(true).await;
    }

    pub async fn test_profile(&self, profile: ServerProfile) -> Result<TestResult, ErrorInfo> {
        validate::validate_profile(&profile).map_err(|e| fail("invalid_profile", e.to_string()))?;
        let _slot = self.0.test_slots.acquire().await.map_err(|_| fail("busy", "Busy"))?;
        let mut result = TestResult::default();
        if !profile.protocol.is_udp_native() {
            result.tcp_ms = crate::probe::tcp_ping(profile.server(), profile.port(), Duration::from_secs(4)).await.ok();
        }
        let (port, secret) = engine::new_controller_endpoint().map_err(|e| fail("internal", e.to_string()))?;
        let ctx = singbox::EngineContext { clash_port: port, clash_secret: secret.clone(), tracker_ruleset: None };
        let cfg = singbox::probe_config(&profile, &ctx).map_err(|e| fail("invalid_profile", e))?;
        let mut eng = self.0.spawn_engine(&cfg, port, secret, Some(&profile)).await?;
        if let Err(e) = eng.wait_ready(Duration::from_secs(8)).await {
            eng.stop().await;
            return Err(fail("engine", e.to_string()));
        }
        match eng.clash.delay(singbox::PROXY_TAG, crate::probe::DELAY_URL, 8000).await {
            Ok(ms) => {
                result.ok = true;
                result.latency_ms = Some(ms);
                result.message = "Server reachable. The secure handshake succeeded.".into();
            }
            Err(e) => {
                tokio::time::sleep(Duration::from_millis(150)).await;
                result.message = crate::explain::outbound_failure(&e.to_string(), &eng.recent_logs());
            }
        }
        eng.stop().await;
        Ok(result)
    }
}

impl Inner {
    pub fn log(&self, msg: impl Into<String>) {
        let m = msg.into();
        tracing::info!("{m}");
        self.logs.push(format!("{} service: {m}", engine::timestamp()));
    }

    pub fn update(&self, f: impl FnOnce(&mut Status)) {
        self.status.send_modify(|s| {
            f(s);
            s.seq += 1;
        });
    }

    pub fn set_connecting(&self, mode: Mode, profile: Option<&ServerProfile>, reconnect: bool) {
        self.update(|s| {
            s.state = if reconnect { ConnState::Reconnecting } else { ConnState::Connecting };
            s.mode = Some(mode);
            s.error = None;
            if !reconnect {
                s.connected_since = None;
                s.exit = None;
                s.latency_ms = None;
                s.traffic = Traffic::default();
                s.protections.clear();
            }
            s.profile_id = profile.map(|p| p.id.clone());
            s.profile_name = profile.map(|p| p.name.clone());
            s.protocol_label = profile.map(|p| p.protocol.label());
        });
    }

    pub async fn spawn_engine(&self, config: &Value, port: u16, secret: String, profile: Option<&ServerProfile>) -> Result<Engine, ErrorInfo> {
        Engine::spawn(Spawn {
            paths: &self.paths,
            #[cfg(windows)]
            job: &self.job,
            config,
            clash_port: port,
            clash_secret: secret,
            logs: self.logs.clone(),
            redactor: Arc::new(Redactor::new(profile)),
        })
        .await
        .map_err(|e| fail("engine", e.to_string()))
    }

    // ----- firewall ------------------------------------------------------

    pub fn enable_dns_guard(&self) -> Result<(), ErrorInfo> {
        #[cfg(windows)]
        {
            let mut g = self.dns_guard.lock().unwrap();
            if g.is_none() {
                let engine = self.paths.engine.to_string_lossy().into_owned();
                *g = Some(wfp::DnsGuard::enable(&engine).map_err(|e| fail("firewall", format!("Couldn't enable DNS leak protection: {e}")))?);
            }
        }
        Ok(())
    }

    /// Arm (or atomically re-arm) the persistent kill switch.
    pub fn enable_kill_switch(&self, allow_lan: bool) -> Result<(), ErrorInfo> {
        #[cfg(windows)]
        {
            let spec = wfp::KillSwitchSpec {
                engine_path: self.paths.engine.to_string_lossy().into_owned(),
                tun_v4: ("172.19.0.0".parse().unwrap(), 30),
                tun_v6: ("fdfe:dcba:9876::".parse().unwrap(), 126),
                allow_lan,
            };
            wfp::kill_switch_enable(&spec).map_err(|e| fail("firewall", format!("Couldn't enable the kill switch: {e}")))?;
            self.kill_switch_on.store(true, Ordering::SeqCst);
            self.update(|s| s.kill_switch_engaged = true);
        }
        Ok(())
    }

    pub fn kill_switch_active(&self) -> bool {
        self.kill_switch_on.load(Ordering::SeqCst)
    }

    pub fn mark_kill_switch_on(&self) {
        self.kill_switch_on.store(true, Ordering::SeqCst);
        self.update(|s| s.kill_switch_engaged = true);
    }

    pub fn disable_guards(&self) {
        #[cfg(windows)]
        {
            // Dropping the guard closes its dynamic WFP session, removing its filters.
            self.dns_guard.lock().unwrap().take();
            if let Err(e) = wfp::kill_switch_disable() {
                tracing::error!("kill switch removal failed: {e}");
                self.log(format!("kill switch removal failed: {e}"));
                return;
            }
        }
        self.kill_switch_on.store(false, Ordering::SeqCst);
        self.update(|s| s.kill_switch_engaged = false);
    }

    pub fn cleanup_system(&self) {
        self.disable_guards();
        self.restore_dns();
    }

    // ----- system DNS (LOTUS mode) ----------------------------------------

    /// Point every active adapter at the local encrypted resolver, saving the
    /// original configuration first. Returns true when anything changed.
    pub fn apply_dns(&self, with_v6: bool) -> Result<bool, ErrorInfo> {
        #[cfg(windows)]
        {
            let list = adapters::list().map_err(|e| fail("dns", e.to_string()))?;
            let mut st = self.state.lock().unwrap();
            let mut changed = false;
            for a in list.iter().filter(|a| a.up && !a.is_loopback() && a.friendly_name != singbox::TUN_NAME) {
                for v6 in [false, true] {
                    if v6 && (!with_v6 || a.ipv6_if_index == 0) {
                        continue;
                    }
                    let ours = if v6 { "::1" } else { "127.0.0.1" };
                    let current = match adapters::get_static_dns(a.guid, v6) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    if current.trim() == ours {
                        continue;
                    }
                    let entry = st.data.dns_backup.entry(a.guid_str.clone()).or_insert_with(|| state::AdapterBackup { name: a.friendly_name.clone(), ..Default::default() });
                    // The user (or DHCP tooling) changed it while we were active: remember the new value.
                    if v6 { entry.v6 = Some(current) } else { entry.v4 = Some(current) }
                    st.save().map_err(|e| fail("dns", format!("Couldn't save original DNS settings: {e}")))?;
                    adapters::set_static_dns(a.guid, v6, ours).map_err(|e| fail("dns", format!("Couldn't configure DNS on \"{}\": {e}", a.friendly_name)))?;
                    changed = true;
                }
            }
            if changed {
                adapters::flush_dns_cache();
            }
            return Ok(changed);
        }
        #[allow(unreachable_code)]
        Ok(false)
    }

    pub fn restore_dns(&self) {
        #[cfg(windows)]
        {
            let mut st = self.state.lock().unwrap();
            if st.data.dns_backup.is_empty() {
                return;
            }
            let entries = std::mem::take(&mut st.data.dns_backup);
            let mut keep = std::collections::BTreeMap::new();
            for (guid_str, b) in entries {
                let Some(guid) = adapters::parse_guid(&guid_str) else { continue };
                let mut failed = b.clone();
                let mut any_fail = false;
                for (v6, orig) in [(false, &b.v4), (true, &b.v6)] {
                    if let Some(orig) = orig {
                        match adapters::set_static_dns(guid, v6, orig) {
                            Ok(()) => {
                                if v6 { failed.v6 = None } else { failed.v4 = None }
                            }
                            Err(e) => {
                                // Adapter removed: nothing to restore.
                                let msg = e.to_string();
                                if msg.contains("0x00000002") || msg.contains("0x00000490") || msg.contains("0x00000057") {
                                    if v6 { failed.v6 = None } else { failed.v4 = None }
                                } else {
                                    any_fail = true;
                                    tracing::error!("restore DNS on {}: {e}", b.name);
                                }
                            }
                        }
                    }
                }
                if any_fail {
                    keep.insert(guid_str, failed);
                }
            }
            st.data.dns_backup = keep;
            let _ = st.save();
            adapters::flush_dns_cache();
        }
    }

    // ----- session record --------------------------------------------------

    pub fn save_record(&self, rec: SessionRecord) {
        if let Err(e) = state::save_session(&self.paths.session_file, &rec) {
            tracing::warn!("couldn't save session record: {e}");
        }
    }

    pub fn boot_epoch(&self) -> u64 {
        #[cfg(windows)]
        {
            return crate::win::boot_epoch_secs();
        }
        #[allow(unreachable_code)]
        0
    }

    pub fn server_is_local(profile: &ServerProfile) -> bool {
        match profile.server().trim_matches(|c| c == '[' || c == ']').parse::<IpAddr>() {
            Ok(IpAddr::V4(v4)) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
            Ok(IpAddr::V6(v6)) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80,
            Err(_) => false,
        }
    }
}
