//! Windows Filtering Platform guards.
//!
//! * **DNS guard** (LOTUS mode) lives in a *dynamic* WFP session: it blocks
//!   plaintext DNS (53) and DoT (853) to anything but loopback, except for the
//!   engine itself. If the service dies the filters vanish with it, which is
//!   the right failure mode for a privacy (not anonymity) feature.
//!
//! * **Kill switch** (SHADOW mode) uses *persistent* filters with fixed keys.
//!   They survive a crash of the service (and a reboot) so there is no window
//!   in which traffic can leak while the service restarts. They are replaced
//!   atomically when re-armed and removed explicitly on disconnect, on service
//!   start when no session is being resumed, by `restore-network` and on uninstall.
//!   Allowed while armed: loopback, the engine binary, the tunnel's own
//!   addresses, DHCP, and (optionally) the local network.

use super::{wide, win_error};
use anyhow::Result;
use std::mem::zeroed;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::ptr::{null, null_mut};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{FWP_E_ALREADY_EXISTS, FWP_E_FILTER_NOT_FOUND, FWP_E_SUBLAYER_NOT_FOUND, HANDLE};
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::*;

const DNS_SUBLAYER: GUID = GUID::from_u128(0x4c4f5455_535f_5746_5000_000000000001);
const KS_SUBLAYER: GUID = GUID::from_u128(0x4c4f5455_535f_5746_5000_000000000002);
/// Kill-switch filter keys are KS_FILTER_BASE + n, n < KS_MAX_FILTERS.
const KS_FILTER_BASE: u128 = 0x4c4f5455_535f_4b53_0000_000000000000;
const KS_MAX_FILTERS: u128 = 64;
const RPC_C_AUTHN_WINNT: u32 = 10;
const IPPROTO_UDP: u8 = 17;

pub struct KillSwitchSpec {
    pub engine_path: String,
    pub tun_v4: (Ipv4Addr, u8),
    pub tun_v6: (Ipv6Addr, u8),
    pub allow_lan: bool,
}

enum Cond {
    Loopback,
    AppId(*mut FWP_BYTE_BLOB),
    RemotePort(u16),
    LocalPort(u16),
    Protocol(u8),
    LocalV4(Ipv4Addr, u8),
    RemoteV4(Ipv4Addr, u8),
    LocalV6(Ipv6Addr, u8),
    RemoteV6(Ipv6Addr, u8),
}

fn v4_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix as u32)
    }
}

struct Session(HANDLE);

impl Session {
    fn open(dynamic: bool) -> Result<Self> {
        let name = wide("LOTUS");
        let mut s: FWPM_SESSION0 = unsafe { zeroed() };
        if dynamic {
            s.flags = FWPM_SESSION_FLAG_DYNAMIC;
        }
        s.displayData.name = name.as_ptr() as *mut u16;
        s.txnWaitTimeoutInMSec = 5000;
        let mut h: HANDLE = null_mut();
        let rc = unsafe { FwpmEngineOpen0(null(), RPC_C_AUTHN_WINNT, null(), &s, &mut h) };
        if rc != 0 {
            return Err(win_error(rc, "FwpmEngineOpen0"));
        }
        Ok(Self(h))
    }

    fn add_sublayer(&self, key: GUID, persistent: bool) -> Result<()> {
        let name = wide("LOTUS guard");
        let mut sl: FWPM_SUBLAYER0 = unsafe { zeroed() };
        sl.subLayerKey = key;
        sl.displayData.name = name.as_ptr() as *mut u16;
        sl.weight = 0xFFFF;
        if persistent {
            sl.flags = FWPM_SUBLAYER_FLAG_PERSISTENT;
        }
        let rc = unsafe { FwpmSubLayerAdd0(self.0, &sl, null_mut()) };
        if rc != 0 && rc != FWP_E_ALREADY_EXISTS as u32 {
            return Err(win_error(rc, "FwpmSubLayerAdd0"));
        }
        Ok(())
    }

    fn transaction<R>(&self, f: impl FnOnce(&Self) -> Result<R>) -> Result<R> {
        let rc = unsafe { FwpmTransactionBegin0(self.0, 0) };
        if rc != 0 {
            return Err(win_error(rc, "FwpmTransactionBegin0"));
        }
        match f(self) {
            Ok(r) => {
                let rc = unsafe { FwpmTransactionCommit0(self.0) };
                if rc != 0 {
                    unsafe { FwpmTransactionAbort0(self.0) };
                    return Err(win_error(rc, "FwpmTransactionCommit0"));
                }
                Ok(r)
            }
            Err(e) => {
                unsafe { FwpmTransactionAbort0(self.0) };
                Err(e)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add(&self, key: Option<GUID>, sublayer: GUID, name: &str, layer: GUID, permit: bool, weight: u8, conds: &[Cond]) -> Result<u64> {
        let mut v4s: Vec<Box<FWP_V4_ADDR_AND_MASK>> = Vec::new();
        let mut v6s: Vec<Box<FWP_V6_ADDR_AND_MASK>> = Vec::new();
        let mut fc: Vec<FWPM_FILTER_CONDITION0> = Vec::with_capacity(conds.len());
        for c in conds {
            let mut x: FWPM_FILTER_CONDITION0 = unsafe { zeroed() };
            x.matchType = FWP_MATCH_EQUAL;
            match c {
                Cond::Loopback => {
                    x.fieldKey = FWPM_CONDITION_FLAGS;
                    x.matchType = FWP_MATCH_FLAGS_ALL_SET;
                    x.conditionValue.r#type = FWP_UINT32;
                    x.conditionValue.Anonymous.uint32 = FWP_CONDITION_FLAG_IS_LOOPBACK;
                }
                Cond::AppId(blob) => {
                    x.fieldKey = FWPM_CONDITION_ALE_APP_ID;
                    x.conditionValue.r#type = FWP_BYTE_BLOB_TYPE;
                    x.conditionValue.Anonymous.byteBlob = *blob;
                }
                Cond::RemotePort(p) => {
                    x.fieldKey = FWPM_CONDITION_IP_REMOTE_PORT;
                    x.conditionValue.r#type = FWP_UINT16;
                    x.conditionValue.Anonymous.uint16 = *p;
                }
                Cond::LocalPort(p) => {
                    x.fieldKey = FWPM_CONDITION_IP_LOCAL_PORT;
                    x.conditionValue.r#type = FWP_UINT16;
                    x.conditionValue.Anonymous.uint16 = *p;
                }
                Cond::Protocol(p) => {
                    x.fieldKey = FWPM_CONDITION_IP_PROTOCOL;
                    x.conditionValue.r#type = FWP_UINT8;
                    x.conditionValue.Anonymous.uint8 = *p;
                }
                Cond::LocalV4(a, pfx) | Cond::RemoteV4(a, pfx) => {
                    x.fieldKey = if matches!(c, Cond::LocalV4(..)) { FWPM_CONDITION_IP_LOCAL_ADDRESS } else { FWPM_CONDITION_IP_REMOTE_ADDRESS };
                    let mask = v4_mask(*pfx);
                    let mut b = Box::new(FWP_V4_ADDR_AND_MASK { addr: u32::from(*a) & mask, mask });
                    x.conditionValue.r#type = FWP_V4_ADDR_MASK;
                    x.conditionValue.Anonymous.v4AddrMask = &mut *b;
                    v4s.push(b);
                }
                Cond::LocalV6(a, pfx) | Cond::RemoteV6(a, pfx) => {
                    x.fieldKey = if matches!(c, Cond::LocalV6(..)) { FWPM_CONDITION_IP_LOCAL_ADDRESS } else { FWPM_CONDITION_IP_REMOTE_ADDRESS };
                    let mut b = Box::new(FWP_V6_ADDR_AND_MASK { addr: a.octets(), prefixLength: *pfx });
                    x.conditionValue.r#type = FWP_V6_ADDR_MASK;
                    x.conditionValue.Anonymous.v6AddrMask = &mut *b;
                    v6s.push(b);
                }
            }
            fc.push(x);
        }
        let wname = wide(name);
        let mut f: FWPM_FILTER0 = unsafe { zeroed() };
        if let Some(k) = key {
            f.filterKey = k;
            f.flags = FWPM_FILTER_FLAG_PERSISTENT;
        }
        f.displayData.name = wname.as_ptr() as *mut u16;
        f.layerKey = layer;
        f.subLayerKey = sublayer;
        f.weight.r#type = FWP_UINT8;
        f.weight.Anonymous.uint8 = weight;
        f.numFilterConditions = fc.len() as u32;
        f.filterCondition = if fc.is_empty() { null_mut() } else { fc.as_mut_ptr() };
        f.action.r#type = if permit { FWP_ACTION_PERMIT } else { FWP_ACTION_BLOCK };
        let mut id = 0u64;
        let rc = unsafe { FwpmFilterAdd0(self.0, &f, null_mut(), &mut id) };
        drop((v4s, v6s));
        if rc != 0 {
            return Err(win_error(rc, &format!("FwpmFilterAdd0({name})")));
        }
        Ok(id)
    }

    fn purge_kill_switch(&self) -> Result<()> {
        for n in 0..KS_MAX_FILTERS {
            let key = GUID::from_u128(KS_FILTER_BASE + n);
            let rc = unsafe { FwpmFilterDeleteByKey0(self.0, &key) };
            if rc != 0 && rc != FWP_E_FILTER_NOT_FOUND as u32 {
                return Err(win_error(rc, "FwpmFilterDeleteByKey0"));
            }
        }
        let rc = unsafe { FwpmSubLayerDeleteByKey0(self.0, &KS_SUBLAYER) };
        if rc != 0 && rc != FWP_E_SUBLAYER_NOT_FOUND as u32 {
            return Err(win_error(rc, "FwpmSubLayerDeleteByKey0"));
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { FwpmEngineClose0(self.0) };
    }
}

fn app_id(path: &str) -> Result<*mut FWP_BYTE_BLOB> {
    let w = wide(path);
    let mut blob: *mut FWP_BYTE_BLOB = null_mut();
    let rc = unsafe { FwpmGetAppIdFromFileName0(w.as_ptr(), &mut blob) };
    if rc != 0 {
        return Err(win_error(rc, "FwpmGetAppIdFromFileName0"));
    }
    Ok(blob)
}

fn free_app_id(blob: *mut FWP_BYTE_BLOB) {
    let mut p = blob as *mut core::ffi::c_void;
    unsafe { FwpmFreeMemory0(&mut p) };
}

// ---------------------------------------------------------------------------
// DNS guard (dynamic)
// ---------------------------------------------------------------------------

pub struct DnsGuard {
    session: Session,
}

unsafe impl Send for DnsGuard {}

impl DnsGuard {
    pub fn enable(engine_path: &str) -> Result<Self> {
        let session = Session::open(true)?;
        session.add_sublayer(DNS_SUBLAYER, false)?;
        let app = app_id(engine_path)?;
        let res = session.transaction(|s| {
            for layer in [FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_CONNECT_V6] {
                s.add(None, DNS_SUBLAYER, "LOTUS DNS: loopback", layer, true, 15, &[Cond::Loopback])?;
                s.add(None, DNS_SUBLAYER, "LOTUS DNS: engine", layer, true, 14, &[Cond::AppId(app)])?;
                s.add(None, DNS_SUBLAYER, "LOTUS DNS: block plaintext DNS", layer, false, 1, &[Cond::RemotePort(53)])?;
                s.add(None, DNS_SUBLAYER, "LOTUS DNS: block DNS-over-TLS", layer, false, 1, &[Cond::RemotePort(853)])?;
            }
            Ok(())
        });
        free_app_id(app);
        res?;
        Ok(Self { session })
    }
}

impl Drop for DnsGuard {
    fn drop(&mut self) {
        // Closing the dynamic session removes every filter it created.
        let _ = &self.session;
    }
}

// ---------------------------------------------------------------------------
// Kill switch (persistent)
// ---------------------------------------------------------------------------

/// Arm (or re-arm) the kill switch. Existing kill-switch filters are replaced
/// in the same transaction, so there is never a moment without protection.
pub fn kill_switch_enable(spec: &KillSwitchSpec) -> Result<()> {
    let session = Session::open(false)?;
    let app = app_id(&spec.engine_path)?;
    let res = session.transaction(|s| {
        s.purge_kill_switch()?;
        s.add_sublayer(KS_SUBLAYER, true)?;
        let mut n: u128 = 0;
        let mut add = |name: &str, layer: GUID, permit: bool, weight: u8, conds: &[Cond]| -> Result<()> {
            assert!(n < KS_MAX_FILTERS);
            s.add(Some(GUID::from_u128(KS_FILTER_BASE + n)), KS_SUBLAYER, name, layer, permit, weight, conds)?;
            n += 1;
            Ok(())
        };
        for layer in [FWPM_LAYER_ALE_AUTH_CONNECT_V4, FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4] {
            add("LOTUS kill switch: loopback", layer, true, 15, &[Cond::Loopback])?;
            add("LOTUS kill switch: engine", layer, true, 14, &[Cond::AppId(app)])?;
            add("LOTUS kill switch: tunnel", layer, true, 13, &[Cond::LocalV4(spec.tun_v4.0, spec.tun_v4.1)])?;
            add("LOTUS kill switch: DHCP", layer, true, 12, &[Cond::Protocol(IPPROTO_UDP), Cond::LocalPort(68)])?;
            add("LOTUS kill switch: DHCP", layer, true, 12, &[Cond::Protocol(IPPROTO_UDP), Cond::RemotePort(67)])?;
            if spec.allow_lan {
                for (net, pfx) in [
                    (Ipv4Addr::new(10, 0, 0, 0), 8),
                    (Ipv4Addr::new(172, 16, 0, 0), 12),
                    (Ipv4Addr::new(192, 168, 0, 0), 16),
                    (Ipv4Addr::new(169, 254, 0, 0), 16),
                    (Ipv4Addr::new(224, 0, 0, 0), 4),
                    (Ipv4Addr::new(255, 255, 255, 255), 32),
                ] {
                    add("LOTUS kill switch: LAN", layer, true, 11, &[Cond::RemoteV4(net, pfx)])?;
                }
            }
            add("LOTUS kill switch: block", layer, false, 0, &[])?;
        }
        for layer in [FWPM_LAYER_ALE_AUTH_CONNECT_V6, FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6] {
            add("LOTUS kill switch: loopback", layer, true, 15, &[Cond::Loopback])?;
            add("LOTUS kill switch: engine", layer, true, 14, &[Cond::AppId(app)])?;
            add("LOTUS kill switch: tunnel", layer, true, 13, &[Cond::LocalV6(spec.tun_v6.0, spec.tun_v6.1)])?;
            add("LOTUS kill switch: DHCPv6", layer, true, 12, &[Cond::Protocol(IPPROTO_UDP), Cond::LocalPort(546)])?;
            add("LOTUS kill switch: DHCPv6", layer, true, 12, &[Cond::Protocol(IPPROTO_UDP), Cond::RemotePort(547)])?;
            if spec.allow_lan {
                for (net, pfx) in [("fe80::".parse::<Ipv6Addr>().unwrap(), 10), ("fc00::".parse().unwrap(), 7), ("ff00::".parse().unwrap(), 8)] {
                    add("LOTUS kill switch: LAN", layer, true, 11, &[Cond::RemoteV6(net, pfx)])?;
                }
            }
            add("LOTUS kill switch: block", layer, false, 0, &[])?;
        }
        Ok(())
    });
    free_app_id(app);
    res
}

/// Remove every kill-switch filter (no-op when none exist).
pub fn kill_switch_disable() -> Result<()> {
    let session = Session::open(false)?;
    session.transaction(|s| s.purge_kill_switch())
}

/// True when kill-switch filters are installed (e.g. left by a crashed service).
pub fn kill_switch_present() -> bool {
    let Ok(session) = Session::open(false) else { return false };
    let mut out: *mut FWPM_SUBLAYER0 = null_mut();
    let rc = unsafe { FwpmSubLayerGetByKey0(session.0, &KS_SUBLAYER, &mut out) };
    if rc == 0 && !out.is_null() {
        let mut p = out as *mut core::ffi::c_void;
        unsafe { FwpmFreeMemory0(&mut p) };
        return true;
    }
    false
}
