use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub exe_dir: PathBuf,
    pub engine: PathBuf,
    pub tracker_rules: Option<PathBuf>,
    pub data_dir: PathBuf,
    pub work_dir: PathBuf,
    pub state_file: PathBuf,
    pub session_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let exe = std::env::current_exe().context("locate service executable")?;
        let exe_dir = exe.parent().context("service executable has no directory")?.to_path_buf();
        let engine = exe_dir.join("sing-box.exe");
        let tracker_rules = ["rules", "resources"]
            .iter()
            .map(|d| exe_dir.join(d).join("block-trackers.srs"))
            .find(|p| p.exists());
        let program_data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        let data_dir = program_data.join("LOTUS");
        Ok(Self {
            engine,
            tracker_rules,
            work_dir: data_dir.join("engine"),
            state_file: data_dir.join("state.json"),
            session_file: data_dir.join("session.bin"),
            data_dir,
            exe_dir,
        })
    }

    /// Create the data directory and lock it down to SYSTEM and Administrators.
    pub fn prepare(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir).with_context(|| format!("create {}", self.data_dir.display()))?;
        #[cfg(windows)]
        crate::win::acl::protect_path(&self.data_dir, crate::win::acl::DATA_DIR_SDDL).context("protect data directory")?;
        std::fs::create_dir_all(&self.work_dir)?;
        Ok(())
    }

    pub fn tracker_rules_str(&self) -> Option<String> {
        self.tracker_rules.as_ref().map(|p| p.to_string_lossy().into_owned())
    }
}
