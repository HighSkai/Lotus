//! Windows helpers for the app: DPAPI (current user) and elevated service repair.

use anyhow::{bail, Result};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::*;

const ENTROPY: &[u8] = b"LOTUS-app-profiles-v1";

fn blob(d: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: d.len() as u32, pbData: d.as_ptr() as *mut u8 }
}

/// Encrypt for the current Windows user (only this user on this machine can decrypt).
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    let (i, e) = (blob(data), blob(ENTROPY));
    let mut o = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    if unsafe { CryptProtectData(&i, null(), &e, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut o) } == 0 {
        bail!("CryptProtectData failed: {}", std::io::Error::last_os_error());
    }
    let v = unsafe { std::slice::from_raw_parts(o.pbData, o.cbData as usize) }.to_vec();
    unsafe { LocalFree(o.pbData as _) };
    Ok(v)
}

pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    let (i, e) = (blob(data), blob(ENTROPY));
    let mut o = CRYPT_INTEGER_BLOB { cbData: 0, pbData: null_mut() };
    if unsafe { CryptUnprotectData(&i, null_mut(), &e, null(), null(), CRYPTPROTECT_UI_FORBIDDEN, &mut o) } == 0 {
        bail!("CryptUnprotectData failed: {}", std::io::Error::last_os_error());
    }
    let v = unsafe { std::slice::from_raw_parts(o.pbData, o.cbData as usize) }.to_vec();
    unsafe { LocalFree(o.pbData as _) };
    Ok(v)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Run `lotus-service.exe install` with a UAC prompt. The path is fixed to the
/// service binary next to this executable; no caller-supplied input reaches it.
pub fn repair_service() -> Result<()> {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
    let exe = std::env::current_exe()?.parent().map(|p| p.join("lotus-service.exe")).ok_or_else(|| anyhow::anyhow!("no install dir"))?;
    if !exe.exists() {
        bail!("lotus-service.exe is missing; reinstall LOTUS");
    }
    let file = wide(&exe.to_string_lossy());
    let verb = wide("runas");
    let params = wide("install");
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        bail!("Repair was cancelled");
    }
    if !info.hProcess.is_null() {
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(info.hProcess, 60_000);
            windows_sys::Win32::Foundation::CloseHandle(info.hProcess);
        }
    }
    Ok(())
}
