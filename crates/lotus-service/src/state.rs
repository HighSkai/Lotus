//! State that must survive a crash or reboot:
//! * the original DNS servers of every adapter LOTUS changed (plain JSON, no secrets);
//! * the active session, so the service can resume after a crash (DPAPI-encrypted,
//!   decryptable only by SYSTEM on this machine).

use anyhow::{Context, Result};
use lotus_core::{LotusOptions, Mode, ServerProfile, ShadowOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterBackup {
    pub name: String,
    /// Original static IPv4 servers ("" = automatic). `None` = not modified.
    pub v4: Option<String>,
    pub v6: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersistentState {
    pub dns_backup: BTreeMap<String, AdapterBackup>,
}

pub struct StateStore {
    path: PathBuf,
    pub data: PersistentState,
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

impl StateStore {
    pub fn load(path: PathBuf) -> Self {
        let data = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { path, data }
    }

    pub fn save(&self) -> Result<()> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(&self.data)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub mode: Mode,
    pub lotus: Option<LotusOptions>,
    pub profile: Option<ServerProfile>,
    pub shadow: Option<ShadowOptions>,
    pub boot_epoch: u64,
    pub resume_on_boot: bool,
}

pub fn save_session(path: &Path, rec: &SessionRecord) -> Result<()> {
    let plain = serde_json::to_vec(rec)?;
    #[cfg(windows)]
    let sealed = crate::win::dpapi::protect(&plain)?;
    #[cfg(not(windows))]
    let sealed = plain;
    atomic_write(path, &sealed)
}

pub fn load_session(path: &Path) -> Option<SessionRecord> {
    let sealed = std::fs::read(path).ok()?;
    #[cfg(windows)]
    let plain = crate::win::dpapi::unprotect(&sealed).ok()?;
    #[cfg(not(windows))]
    let plain = sealed;
    serde_json::from_slice(&plain).ok()
}

pub fn clear_session(path: &Path) {
    let _ = std::fs::remove_file(path);
}
