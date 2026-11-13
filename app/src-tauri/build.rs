fn main() {
    println!("cargo:rerun-if-changed=../../server/lotus-server-install.sh");
    tauri_build::build()
}
