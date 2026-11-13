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

/// Program file names of apps with a visible window, for the split-tunnel
/// picker (sorted, without duplicates, LOTUS itself or shell components).
pub fn running_apps() -> Vec<String> {
    use std::collections::BTreeMap;
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible};

    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let pids = &mut *(lparam as *mut Vec<u32>);
        if IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != 0 {
                pids.push(pid);
            }
        }
        1
    }
    const SHELL: &[&str] = &["explorer.exe", "applicationframehost.exe", "textinputhost.exe", "shellexperiencehost.exe", "searchhost.exe", "startmenuexperiencehost.exe", "systemsettings.exe", "lockapp.exe"];
    let mut pids: Vec<u32> = Vec::new();
    unsafe { EnumWindows(Some(each), &mut pids as *mut _ as LPARAM) };
    pids.sort_unstable();
    pids.dedup();
    let me = std::process::id();
    // Keyed by lower case so "Discord.exe" and "discord.exe" appear once.
    let mut names = BTreeMap::new();
    for pid in pids.into_iter().filter(|p| *p != me) {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                continue;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
            CloseHandle(h);
            if ok {
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                if let Some(name) = path.rsplit('\\').next().filter(|n| !n.is_empty()) {
                    let key = name.to_ascii_lowercase();
                    if !SHELL.contains(&key.as_str()) && key != "lotus.exe" {
                        names.entry(key).or_insert_with(|| name.to_string());
                    }
                }
            }
        }
    }
    names.into_values().collect()
}