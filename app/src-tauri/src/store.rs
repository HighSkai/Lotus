//! Per-user storage: settings (plain JSON) and server profiles (encrypted with
//! DPAPI for the current Windows user — never written in plaintext).

use lotus_core::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    #[default]
    System,
    Reduce,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub mode: Mode,
    pub selected_profile: Option<String>,
    pub lotus: LotusOptions,
    pub shadow: ShadowOptions,
    pub launch_at_startup: bool,
    pub connect_on_launch: bool,
    pub close_to_tray: bool,
    pub disconnect_on_quit: bool,
    pub motion: Motion,
    /// Check for updates in the background and download them when found.
    pub auto_update: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Lotus,
            selected_profile: None,
            lotus: LotusOptions::default(),
            shadow: ShadowOptions::default(),
            launch_at_startup: false,
            connect_on_launch: false,
            close_to_tray: true,
            disconnect_on_quit: false,
            motion: Motion::System,
            auto_update: true,
        }
    }
}

pub struct Store {
    dir: PathBuf,
    pub settings: AppSettings,
    pub profiles: Vec<ServerProfile>,
    pub load_error: Option<String>,
}

impl Store {
    pub fn load(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let settings = std::fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut load_error = None;
        let profiles = match std::fs::read(dir.join("profiles.bin")) {
            Ok(sealed) => match open_profiles(&sealed) {
                Ok(p) => p,
                Err(e) => {
                    load_error = Some(format!("Saved servers couldn't be decrypted ({e}). They were kept on disk as profiles.bin.bak."));
                    let _ = std::fs::copy(dir.join("profiles.bin"), dir.join("profiles.bin.bak"));
                    Vec::new()
                }
            },
            Err(_) => Vec::new(),
        };
        Self { dir, settings, profiles, load_error }
    }

    fn write(&self, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let path = self.dir.join(name);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn save_settings(&self) -> anyhow::Result<()> {
        self.write("settings.json", &serde_json::to_vec_pretty(&self.settings)?)
    }

    pub fn save_profiles(&self) -> anyhow::Result<()> {
        let plain = serde_json::to_vec(&self.profiles)?;
        #[cfg(windows)]
        let sealed = crate::win::protect(&plain)?;
        #[cfg(not(windows))]
        let sealed: Vec<u8> = anyhow::bail!("secure profile storage is implemented for Windows only so far");
        self.write("profiles.bin", &sealed)
    }

    pub fn selected(&self) -> Option<&ServerProfile> {
        let id = self.settings.selected_profile.as_ref()?;
        self.profiles.iter().find(|p| &p.id == id)
    }
}

fn open_profiles(sealed: &[u8]) -> anyhow::Result<Vec<ServerProfile>> {
    #[cfg(windows)]
    let plain = crate::win::unprotect(sealed)?;
    #[cfg(not(windows))]
    let plain = sealed.to_vec();
    Ok(serde_json::from_slice(&plain)?)
}
