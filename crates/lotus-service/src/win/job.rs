//! A job object that kills engine processes if the service itself exits,
//! so an orphaned engine can never keep routing traffic.

use super::win_error;
use anyhow::Result;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::*;

pub struct Job(HANDLE);

unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub fn new() -> Result<Self> {
        let h = unsafe { CreateJobObjectW(null(), null()) };
        if h.is_null() {
            return Err(win_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32, "CreateJobObjectW"));
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            unsafe { CloseHandle(h) };
            return Err(win_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32, "SetInformationJobObject"));
        }
        Ok(Self(h))
    }

    pub fn assign(&self, process: HANDLE) -> Result<()> {
        let ok = unsafe { AssignProcessToJobObject(self.0, process) };
        if ok == 0 {
            return Err(win_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32, "AssignProcessToJobObject"));
        }
        Ok(())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = null_mut::<u8>();
        unsafe { CloseHandle(self.0) };
    }
}
