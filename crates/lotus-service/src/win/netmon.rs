//! Network change notifications (interfaces and routes), forwarded to the
//! controller as coarse "something changed" events.

use super::win_error;
use anyhow::Result;
use std::ptr::null_mut;
use std::sync::OnceLock;
use tokio::sync::mpsc::UnboundedSender;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;

static SINK: OnceLock<UnboundedSender<()>> = OnceLock::new();

unsafe extern "system" fn on_interface(_ctx: *const core::ffi::c_void, _row: *const MIB_IPINTERFACE_ROW, _t: MIB_NOTIFICATION_TYPE) {
    if let Some(tx) = SINK.get() {
        let _ = tx.send(());
    }
}

unsafe extern "system" fn on_route(_ctx: *const core::ffi::c_void, _row: *const MIB_IPFORWARD_ROW2, _t: MIB_NOTIFICATION_TYPE) {
    if let Some(tx) = SINK.get() {
        let _ = tx.send(());
    }
}

pub struct NetMonitor {
    handles: Vec<HANDLE>,
}

unsafe impl Send for NetMonitor {}

impl NetMonitor {
    pub fn start(tx: UnboundedSender<()>) -> Result<Self> {
        let _ = SINK.set(tx);
        let mut h1: HANDLE = null_mut();
        let rc = unsafe { NotifyIpInterfaceChange(AF_UNSPEC, Some(on_interface), std::ptr::null(), 0, &mut h1) };
        if rc != 0 {
            return Err(win_error(rc, "NotifyIpInterfaceChange"));
        }
        let mut h2: HANDLE = null_mut();
        let rc = unsafe { NotifyRouteChange2(AF_UNSPEC, Some(on_route), std::ptr::null(), 0, &mut h2) };
        if rc != 0 {
            unsafe { CancelMibChangeNotify2(h1) };
            return Err(win_error(rc, "NotifyRouteChange2"));
        }
        Ok(Self { handles: vec![h1, h2] })
    }
}

impl Drop for NetMonitor {
    fn drop(&mut self) {
        for h in &self.handles {
            unsafe { CancelMibChangeNotify2(*h) };
        }
    }
}
