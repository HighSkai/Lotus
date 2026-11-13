//! Windows-specific system integration. Every unsafe FFI call in the service
//! lives below this module so the rest of the code stays platform-neutral.

pub mod acl;
pub mod adapters;
pub mod dpapi;
pub mod job;
pub mod netmon;
pub mod pipe;
pub mod svc;
pub mod wfp;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// # Safety
/// `p` must be null or point to a NUL-terminated UTF-16 string.
pub unsafe fn from_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

pub fn win_error(code: u32, what: &str) -> anyhow::Error {
    anyhow::anyhow!("{what} failed (Windows error 0x{code:08X}: {})", std::io::Error::from_raw_os_error(code as i32))
}

/// System boot time in Unix seconds, used to tell a service crash apart from a reboot.
pub fn boot_epoch_secs() -> u64 {
    let up_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    now.saturating_sub(up_ms / 1000)
}
