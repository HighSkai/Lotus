//! Security descriptors for the service's data directory and IPC pipe.

use super::{wide, win_error};
use anyhow::Result;
use std::path::Path;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{LocalFree, BOOL};
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::*;

/// SYSTEM and Administrators only, not inherited from %ProgramData%.
pub const DATA_DIR_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";

/// Pipe access: SYSTEM and Administrators full control; interactive users may
/// read and write data only — deliberately *not* FILE_CREATE_PIPE_INSTANCE, so
/// no other process can create a competing instance of the pipe.
/// 0x0012008B = FILE_GENERIC_READ | FILE_WRITE_DATA.
pub const PIPE_SDDL: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12008B;;;IU)";
pub const PIPE_CLIENT_ACCESS: u32 = 0x0012_008B;

pub struct SecurityDescriptor(pub PSECURITY_DESCRIPTOR);

unsafe impl Send for SecurityDescriptor {}
unsafe impl Sync for SecurityDescriptor {}

impl SecurityDescriptor {
    pub fn from_sddl(sddl: &str) -> Result<Self> {
        let w = wide(sddl);
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(w.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) };
        if ok == 0 {
            return Err(win_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32, "ConvertStringSecurityDescriptorToSecurityDescriptorW"));
        }
        Ok(Self(sd))
    }

    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: self.0, bInheritHandle: 0 }
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0 as _) };
    }
}

/// Replace the DACL of `path` with a protected (non-inherited) one.
pub fn protect_path(path: &Path, sddl: &str) -> Result<()> {
    let sd = SecurityDescriptor::from_sddl(sddl)?;
    let mut present: BOOL = 0;
    let mut defaulted: BOOL = 0;
    let mut dacl: *mut ACL = null_mut();
    let ok = unsafe { GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted) };
    if ok == 0 || present == 0 {
        anyhow::bail!("security descriptor has no DACL");
    }
    let w = wide(&path.to_string_lossy());
    let rc = unsafe {
        SetNamedSecurityInfoW(
            w.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            std::ptr::null(),
        )
    };
    if rc != 0 {
        return Err(win_error(rc, "SetNamedSecurityInfoW"));
    }
    Ok(())
}
