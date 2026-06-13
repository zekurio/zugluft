//! Named-pipe transport.
//!
//! The server side needs raw Win32 (std has no named-pipe listener); accepted
//! connections and the client side are plain `std::fs::File`s — Windows pipe
//! handles speak ReadFile/WriteFile.

use std::fs::{File, OpenOptions};
use std::io;
use std::iter::once;
use std::mem::size_of;
use std::os::windows::io::FromRawHandle;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_FIRST_PIPE_INSTANCE;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
const SDDL_REVISION_1: u32 = 1;
const BUFFER_SIZE: u32 = 64 * 1024;

/// Who may talk to the service: full control for SYSTEM and Administrators,
/// connect (generic read/write) for interactive — i.e. logged-on desktop —
/// users. Network logons and other service accounts get nothing.
const PIPE_SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)";

/// Accepts client connections on one pipe name, one instance per client.
pub struct PipeServer {
    pipe_name: &'static str,
    first_instance: bool,
}

impl PipeServer {
    pub fn new(pipe_name: &'static str) -> Self {
        Self {
            pipe_name,
            first_instance: true,
        }
    }

    /// Creates the next pipe instance and blocks until a client connects.
    pub fn accept(&mut self) -> io::Result<File> {
        let name: Vec<u16> = self.pipe_name.encode_utf16().chain(once(0)).collect();
        let sddl: Vec<u16> = PIPE_SDDL.encode_utf16().chain(once(0)).collect();

        let mut descriptor = null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };

        let mut open_mode = PIPE_ACCESS_DUPLEX;
        if self.first_instance {
            // Fail if something else already owns the pipe name.
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_SIZE,
                BUFFER_SIZE,
                0,
                &attributes,
            )
        };
        unsafe { LocalFree(descriptor) };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        self.first_instance = false;

        let ok = unsafe { ConnectNamedPipe(handle, null_mut()) };
        // The client may have connected between create and this call.
        if ok == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
            let error = io::Error::last_os_error();
            unsafe { CloseHandle(handle) };
            return Err(error);
        }

        Ok(unsafe { File::from_raw_handle(handle as _) })
    }
}

/// Connects to the service's event stream (read side). Fails immediately
/// when the service isn't running; callers are expected to retry.
pub fn connect_events() -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(crate::EVENTS_PIPE)
}

/// Connects to the service's control stream (write side).
pub fn connect_control() -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(crate::CONTROL_PIPE)
}
