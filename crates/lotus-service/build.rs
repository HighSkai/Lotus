// Embeds the pinned engine digest from vendor/LOCK so the service can verify
// the sing-box binary before every launch.
use std::path::PathBuf;

fn main() {
    let lock = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/LOCK");
    println!("cargo:rerun-if-changed={}", lock.display());
    let text = std::fs::read_to_string(&lock).unwrap_or_default();
    let get = |key: &str| {
        text.lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    };
    println!("cargo:rustc-env=LOTUS_ENGINE_SHA256={}", get("engine.exe.sha256"));
    println!("cargo:rustc-env=LOTUS_ENGINE_VERSION={}", get("engine.version"));
}
