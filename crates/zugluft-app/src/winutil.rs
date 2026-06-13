//! Small Win32 helpers for the GUI process.
//!
//! gpui 0.2.2's Windows backend can't drive these itself:
//!
//! - `WindowControlArea::Drag` never produces a working HTCAPTION move and
//!   `start_window_move()` is a no-op, so the painted titlebar hands its
//!   mouse-down to the native caption-drag syscommand instead. The native
//!   move loop is what makes Aero Snap (top-edge maximize, side tiling)
//!   and drag-restore from a maximized window work; gpui supports the
//!   modal loop explicitly (WM_ENTERSIZEMOVE starts a timer that keeps the
//!   app running).
//! - `zoom()` only ever maximizes (never restores), so the caption button
//!   and titlebar double-click toggle through here.
//!
//! Everything here must act asynchronously (PostMessage/ShowWindowAsync):
//! these helpers run inside gpui event callbacks, where the window state
//! is already mutably borrowed — a synchronous resize delivers WM_SIZE
//! re-entrantly, gpui's handler can't take its borrow, and the renderer
//! is left painting a stale frame at the old size.

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, ReleaseCapture};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IsIconic, IsWindow, IsZoomed, PostMessageW, SC_MOVE, SW_HIDE, SW_MAXIMIZE, SW_RESTORE, SW_SHOW,
    SetForegroundWindow, ShowWindowAsync, WM_CLOSE, WM_SYSCOMMAND,
};

static MAIN_WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// `SC_MOVE | HTCAPTION`: the universal "start a caption drag with the
/// button already held" syscommand that DefWindowProc turns into the
/// native move loop.
const SC_DRAGMOVE: usize = SC_MOVE as usize | 0x0002;

pub fn remember_main_window(hwnd: isize) {
    MAIN_WINDOW.store(hwnd, std::sync::atomic::Ordering::Relaxed);
}

fn main_window() -> Option<windows_sys::Win32::Foundation::HWND> {
    let hwnd = MAIN_WINDOW.load(std::sync::atomic::Ordering::Relaxed)
        as windows_sys::Win32::Foundation::HWND;
    unsafe { (!hwnd.is_null() && IsWindow(hwnd) != 0).then_some(hwnd) }
}

pub fn show_main_window() {
    let Some(hwnd) = main_window() else { return };
    unsafe {
        ShowWindowAsync(
            hwnd,
            if IsIconic(hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(hwnd);
    }
}

pub fn hide_main_window() {
    let Some(hwnd) = main_window() else { return };
    unsafe {
        ShowWindowAsync(hwnd, SW_HIDE);
    }
}

pub fn close_main_window() {
    let Some(hwnd) = main_window() else { return };
    unsafe {
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

/// Hands the held titlebar press to the native window move loop. Posted,
/// not sent, so the loop starts from the top of the message pump instead
/// of re-entrantly inside the gpui callback this is called from.
pub fn begin_titlebar_drag() {
    unsafe {
        let hwnd = GetActiveWindow();
        if hwnd.is_null() {
            return;
        }
        ReleaseCapture();
        PostMessageW(hwnd, WM_SYSCOMMAND, SC_DRAGMOVE, 0);
    }
}

/// Maximizes or restores the active window.
pub fn toggle_maximize() {
    unsafe {
        let hwnd = GetActiveWindow();
        if hwnd.is_null() {
            return;
        }
        ShowWindowAsync(
            hwnd,
            if IsZoomed(hwnd) != 0 {
                SW_RESTORE
            } else {
                SW_MAXIMIZE
            },
        );
    }
}
