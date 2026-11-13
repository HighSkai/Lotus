//! Named-pipe endpoints for the app ⇄ service protocol.

use super::acl::{SecurityDescriptor, PIPE_CLIENT_ACCESS};
use super::wide;
use lotus_core::ipc::PIPE_NAME;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use tokio::net::windows::named_pipe::{NamedPipeClient, NamedPipeServer, ServerOptions};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING};
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;

const SECURITY_SQOS_PRESENT: u32 = 0x0010_0000;
const SECURITY_IDENTIFICATION: u32 = 0x0001_0000;

pub fn create_server(first: bool, sd: &SecurityDescriptor) -> io::Result<NamedPipeServer> {
    let mut sa = sd.attributes();
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .in_buffer_size(64 * 1024)
            .out_buffer_size(64 * 1024)
            .create_with_security_attributes_raw(PIPE_NAME, &mut sa as *mut _ as *mut core::ffi::c_void)
    }
}

pub fn client_pid(server: &NamedPipeServer) -> Option<u32> {
    let mut pid = 0u32;
    let ok = unsafe { GetNamedPipeClientProcessId(server.as_raw_handle() as _, &mut pid) };
    (ok != 0).then_some(pid)
}

/// Open the pipe with exactly the rights the service grants (read + write data),
/// limiting the server to identification-level impersonation.
pub fn open_client() -> io::Result<NamedPipeClient> {
    let w = wide(PIPE_NAME);
    let h = unsafe {
        CreateFileW(
            w.as_ptr(),
            PIPE_CLIENT_ACCESS,
            0,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    unsafe { NamedPipeClient::from_raw_handle(h as _) }
}
