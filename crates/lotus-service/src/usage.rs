//! Local usage history: time connected and traffic per day, mode and server.
//!
//! Only totals are kept (no destinations), in `%ProgramData%\LOTUS\usage.json`,
//! which only SYSTEM and administrators can read; the app asks for it over IPC.
//! The recorder samples the status the service already publishes (traffic
//! counters change every second in SHADOW), so it adds no extra work.

use crate::controller::Controller;
use lotus_core::ipc::*;
use lotus_core::*;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// About 13 months, enough for a year view.
const KEEP_DAYS: usize = 400;
const SAVE_EVERY: Duration = Duration::from_secs(60);

pub struct UsageStore {
    path: PathBuf,
    days: Vec<UsageDay>,
    dirty: bool,
    saved: Instant,
}

impl UsageStore {
    pub fn load(path: PathBuf) -> Self {
        let days = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<UsageReport>(&b).ok()).map(|r| r.days).unwrap_or_default();
        Self { path, days, dirty: false, saved: Instant::now() }
    }

    pub fn report(&self) -> UsageReport {
        UsageReport { days: self.days.clone() }
    }

    pub fn clear(&mut self) {
        self.days.clear();
        self.dirty = true;
        self.save();
    }

    fn add(&mut self, date: String, mode: Mode, server_id: Option<String>, server: Option<String>, seconds: u64, up: u64, down: u64) {
        if seconds == 0 && up == 0 && down == 0 {
            return;
        }
        match self.days.iter_mut().find(|d| d.date == date && d.mode == mode && d.server_id == server_id) {
            Some(d) => {
                d.seconds += seconds;
                d.up += up;
                d.down += down;
                if server.is_some() {
                    d.server = server;
                }
            }
            None => self.days.push(UsageDay { date, mode, server_id, server, seconds, up, down }),
        }
        let mut dates: Vec<&str> = self.days.iter().map(|d| d.date.as_str()).collect();
        dates.sort_unstable();
        dates.dedup();
        if dates.len() > KEEP_DAYS {
            let cutoff = dates[dates.len() - KEEP_DAYS].to_string();
            self.days.retain(|d| d.date >= cutoff);
        }
        self.dirty = true;
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let tmp = self.path.with_extension("tmp");
        let ok = serde_json::to_vec(&self.report()).ok().and_then(|b| std::fs::write(&tmp, b).ok()).and_then(|_| std::fs::rename(&tmp, &self.path).ok()).is_some();
        if ok {
            self.dirty = false;
            self.saved = Instant::now();
        }
    }
}

/// Today's local date as `YYYY-MM-DD`.
fn local_date() -> String {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut t = unsafe { std::mem::zeroed() };
        unsafe { GetLocalTime(&mut t) };
        format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
    }
    #[cfg(not(windows))]
    {
        let days = now_ms() / 86_400_000;
        format!("day-{days}")
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Sample {
    at: Instant,
    mode: Mode,
    profile_id: Option<String>,
    profile_name: Option<String>,
    up: u64,
    down: u64,
}

/// Time and traffic to add for one interval.
#[derive(Debug, PartialEq)]
struct Entry {
    mode: Mode,
    profile_id: Option<String>,
    profile_name: Option<String>,
    seconds: u64,
    up: u64,
    down: u64,
}

/// Advance from the previous sample to the status seen at `now`. Returns the next
/// sample, what to record, and whether a session just ended.
fn step(prev: Option<Sample>, st: &Status, now: Instant) -> (Option<Sample>, Option<Entry>, bool) {
    let live = matches!(st.state, ConnState::Connected | ConnState::Reconnecting);
    let fresh = || st.mode.filter(|_| live).map(|mode| Sample { at: now, mode, profile_id: st.profile_id.clone(), profile_name: st.profile_name.clone(), up: st.traffic.up_total, down: st.traffic.down_total });
    let Some(prev) = prev else { return (fresh(), None, false) };
    let same = live && st.mode == Some(prev.mode) && st.profile_id == prev.profile_id;
    // Whole seconds only; the remainder carries into the next sample.
    let seconds = now.saturating_duration_since(prev.at).as_secs();
    let (up, down) = if same {
        // Counters are per session; a drop means a new session started.
        (st.traffic.up_total.checked_sub(prev.up).unwrap_or(st.traffic.up_total), st.traffic.down_total.checked_sub(prev.down).unwrap_or(st.traffic.down_total))
    } else {
        (0, 0)
    };
    let entry = Entry { mode: prev.mode, profile_id: prev.profile_id.clone(), profile_name: prev.profile_name.clone(), seconds, up, down };
    let next = if same { Some(Sample { at: prev.at + Duration::from_secs(seconds), up: st.traffic.up_total, down: st.traffic.down_total, ..prev }) } else { fresh() };
    (next, Some(entry), !same)
}

/// Follow the published status and add each connected interval to the history.
pub async fn run(ctrl: Controller) {
    let mut rx = ctrl.subscribe();
    let mut last: Option<Sample> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            r = rx.changed() => if r.is_err() { break },
            _ = tick.tick() => {}
        }
        let st = rx.borrow_and_update().clone();
        let (next, entry, ended) = step(last.take(), &st, Instant::now());
        last = next;
        let mut store = ctrl.inner().usage.lock().unwrap();
        if let Some(e) = entry {
            if ctrl.inner().usage_on.load(Ordering::SeqCst) {
                store.add(local_date(), e.mode, e.profile_id, e.profile_name, e.seconds, e.up, e.down);
            }
        }
        if ended || store.saved.elapsed() >= SAVE_EVERY {
            store.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: ConnState, mode: Mode, profile: Option<&str>, up: u64, down: u64) -> Status {
        Status { state, mode: Some(mode), profile_id: profile.map(Into::into), profile_name: profile.map(Into::into), traffic: Traffic { up_total: up, down_total: down, ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn counts_time_and_bytes_of_a_session() {
        let t0 = Instant::now();
        let (s, e, _) = step(None, &status(ConnState::Connected, Mode::Shadow, Some("p1"), 0, 0), t0);
        assert!(e.is_none());
        let (s, e, ended) = step(s, &status(ConnState::Connected, Mode::Shadow, Some("p1"), 100, 1000), t0 + Duration::from_millis(1500));
        let e = e.unwrap();
        assert_eq!((e.seconds, e.up, e.down, ended), (1, 100, 1000, false));
        // The half second left over is counted in the next interval.
        let (s, e, _) = step(s, &status(ConnState::Connected, Mode::Shadow, Some("p1"), 150, 1500), t0 + Duration::from_millis(3000));
        let e = e.unwrap();
        assert_eq!((e.seconds, e.up, e.down), (2, 50, 500));
        // Disconnect: time up to now is kept, the session ends.
        let (s, e, ended) = step(s, &status(ConnState::Disconnected, Mode::Shadow, None, 0, 0), t0 + Duration::from_millis(4000));
        assert_eq!(e.unwrap().seconds, 1);
        assert!(ended);
        assert!(s.is_none());
    }

    #[test]
    fn new_session_counters_start_from_zero() {
        let t0 = Instant::now();
        let (s, _, _) = step(None, &status(ConnState::Connected, Mode::Shadow, Some("p1"), 5000, 5000), t0);
        // Counters fell: a new session behind the same profile (reconnect after restart).
        let (_, e, _) = step(s, &status(ConnState::Connected, Mode::Shadow, Some("p1"), 10, 20), t0 + Duration::from_secs(2));
        let e = e.unwrap();
        assert_eq!((e.up, e.down), (10, 20));
    }

    #[test]
    fn switching_server_closes_the_old_one() {
        let t0 = Instant::now();
        let (s, _, _) = step(None, &status(ConnState::Connected, Mode::Shadow, Some("a"), 0, 0), t0);
        let (s, e, ended) = step(s, &status(ConnState::Connected, Mode::Shadow, Some("b"), 0, 0), t0 + Duration::from_secs(3));
        assert_eq!(e.unwrap().profile_id.as_deref(), Some("a"));
        assert!(ended);
        assert_eq!(s.unwrap().profile_id.as_deref(), Some("b"));
    }

    #[test]
    fn store_merges_days_and_prunes() {
        let dir = std::env::temp_dir().join(format!("lotus-usage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut st = UsageStore::load(dir.join("usage.json"));
        st.add("2026-11-01".into(), Mode::Shadow, Some("p".into()), Some("VPS".into()), 10, 1, 2);
        st.add("2026-11-01".into(), Mode::Shadow, Some("p".into()), Some("VPS".into()), 5, 1, 2);
        st.add("2026-11-01".into(), Mode::Lotus, None, None, 7, 0, 0);
        assert_eq!(st.days.len(), 2);
        assert_eq!(st.days[0].seconds, 15);
        for d in 0..(KEEP_DAYS + 5) {
            st.add(format!("2027-{:04}", d), Mode::Lotus, None, None, 1, 0, 0);
        }
        let mut dates: Vec<_> = st.days.iter().map(|d| d.date.clone()).collect();
        dates.dedup();
        assert_eq!(dates.len(), KEEP_DAYS);
        st.save();
        assert_eq!(UsageStore::load(dir.join("usage.json")).days.len(), st.days.len());
        let _ = std::fs::remove_dir_all(dir);
    }
}
