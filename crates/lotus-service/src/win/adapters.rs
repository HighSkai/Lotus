//! Network adapter enumeration and per-adapter DNS configuration
//! (`GetAdaptersAddresses`, `Get/SetInterfaceDnsSettings`).

use super::{from_wide, wide, win_error};
use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ptr::null;
use windows_sys::core::GUID;
use windows_sys::Win32::NetworkManagement::IpHelper::*;
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::*;

#[derive(Clone)]
pub struct Adapter {
    pub guid: GUID,
    pub guid_str: String,
    pub friendly_name: String,
    pub if_index: u32,
    pub ipv6_if_index: u32,
    pub if_type: u32,
    pub up: bool,
    pub dns_servers: Vec<IpAddr>,
}

impl Adapter {
    pub fn is_loopback(&self) -> bool {
        self.if_type == IF_TYPE_SOFTWARE_LOOPBACK
    }
}

pub fn parse_guid(s: &str) -> Option<GUID> {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let d1 = u32::from_str_radix(parts[0], 16).ok()?;
    let d2 = u16::from_str_radix(parts[1], 16).ok()?;
    let d3 = u16::from_str_radix(parts[2], 16).ok()?;
    let tail = format!("{}{}", parts[3], parts[4]);
    if tail.len() != 16 {
        return None;
    }
    let mut d4 = [0u8; 8];
    for (i, b) in d4.iter_mut().enumerate() {
        *b = u8::from_str_radix(&tail[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(GUID { data1: d1, data2: d2, data3: d3, data4: d4 })
}

/// # Safety
/// `sa` must be null or point to a valid SOCKADDR of the family it declares.
unsafe fn sockaddr_ip(sa: *const SOCKADDR) -> Option<IpAddr> {
    if sa.is_null() {
        return None;
    }
    match (*sa).sa_family {
        AF_INET => {
            let s = &*(sa as *const SOCKADDR_IN);
            Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(s.sin_addr.S_un.S_addr))))
        }
        AF_INET6 => {
            let s = &*(sa as *const SOCKADDR_IN6);
            Some(IpAddr::V6(Ipv6Addr::from(s.sin6_addr.u.Byte)))
        }
        _ => None,
    }
}

pub fn list() -> Result<Vec<Adapter>> {
    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 32 * 1024;
    let mut buf: Vec<u64>;
    loop {
        buf = vec![0u64; (size as usize / 8) + 1];
        let rc = unsafe { GetAdaptersAddresses(AF_UNSPEC as u32, flags, null(), buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH, &mut size) };
        match rc {
            0 => break,
            111 /* ERROR_BUFFER_OVERFLOW */ => continue,
            232 /* ERROR_NO_DATA */ => return Ok(Vec::new()),
            e => return Err(win_error(e, "GetAdaptersAddresses")),
        }
    }
    let mut out = Vec::new();
    let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !p.is_null() {
        let a = unsafe { &*p };
        let name = unsafe { std::ffi::CStr::from_ptr(a.AdapterName as *const i8) }.to_string_lossy().into_owned();
        if let Some(guid) = parse_guid(&name) {
            let mut dns = Vec::new();
            let mut d = a.FirstDnsServerAddress;
            while !d.is_null() {
                let dd = unsafe { &*d };
                if let Some(ip) = unsafe { sockaddr_ip(dd.Address.lpSockaddr) } {
                    dns.push(ip);
                }
                d = dd.Next;
            }
            out.push(Adapter {
                guid,
                guid_str: name,
                friendly_name: unsafe { from_wide(a.FriendlyName) },
                if_index: unsafe { a.Anonymous1.Anonymous.IfIndex },
                ipv6_if_index: a.Ipv6IfIndex,
                if_type: a.IfType,
                up: a.OperStatus == IfOperStatusUp,
                dns_servers: dns,
            });
        }
        p = a.Next;
    }
    Ok(out)
}

/// Statically configured DNS servers of an adapter for one family
/// (empty string = obtained automatically via DHCP/RA).
pub fn get_static_dns(guid: GUID, ipv6: bool) -> Result<String> {
    let mut s: DNS_INTERFACE_SETTINGS = unsafe { std::mem::zeroed() };
    s.Version = DNS_INTERFACE_SETTINGS_VERSION1;
    if ipv6 {
        s.Flags = DNS_SETTING_IPV6 as u64;
    }
    let rc = unsafe { GetInterfaceDnsSettings(guid, &mut s) };
    if rc != 0 {
        return Err(win_error(rc, "GetInterfaceDnsSettings"));
    }
    let v = unsafe { from_wide(s.NameServer) };
    unsafe { FreeInterfaceDnsSettings(&mut s) };
    Ok(v)
}

pub fn set_static_dns(guid: GUID, ipv6: bool, servers: &str) -> Result<()> {
    let mut w = wide(servers);
    let mut s: DNS_INTERFACE_SETTINGS = unsafe { std::mem::zeroed() };
    s.Version = DNS_INTERFACE_SETTINGS_VERSION1;
    s.Flags = DNS_SETTING_NAMESERVER as u64 | if ipv6 { DNS_SETTING_IPV6 as u64 } else { 0 };
    s.NameServer = w.as_mut_ptr();
    let rc = unsafe { SetInterfaceDnsSettings(guid, &s) };
    if rc != 0 {
        return Err(win_error(rc, "SetInterfaceDnsSettings"));
    }
    Ok(())
}

#[link(name = "dnsapi")]
extern "system" {
    fn DnsFlushResolverCache() -> i32;
}

pub fn flush_dns_cache() {
    unsafe {
        DnsFlushResolverCache();
    }
}

/// Interface index Windows would use to reach `dest`.
pub fn best_interface(dest: IpAddr) -> Option<u32> {
    let mut idx = 0u32;
    let rc = unsafe {
        match dest {
            IpAddr::V4(v4) => {
                let mut sa: SOCKADDR_IN = std::mem::zeroed();
                sa.sin_family = AF_INET;
                sa.sin_addr.S_un.S_addr = u32::from(v4).to_be();
                GetBestInterfaceEx(&sa as *const _ as *const SOCKADDR, &mut idx)
            }
            IpAddr::V6(v6) => {
                let mut sa: SOCKADDR_IN6 = std::mem::zeroed();
                sa.sin6_family = AF_INET6;
                sa.sin6_addr.u.Byte = v6.octets();
                GetBestInterfaceEx(&sa as *const _ as *const SOCKADDR, &mut idx)
            }
        }
    };
    (rc == 0).then_some(idx)
}
