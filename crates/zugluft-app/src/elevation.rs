//! One-shot elevated process launch (UAC prompt), used to install or manage
//! the zugluft service. The GUI itself always runs unelevated.

use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// Runs `exe args...` with the "runas" verb. Returns true when the elevated
/// process was started (i.e. the user accepted the UAC prompt).
pub fn run_elevated(exe: &Path, args: &str) -> bool {
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain(once(0)).collect();
    let args: Vec<u16> = args.encode_utf16().chain(once(0)).collect();
    let verb: Vec<u16> = "runas".encode_utf16().chain(once(0)).collect();
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            exe.as_ptr(),
            args.as_ptr(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    result as usize > 32
}
