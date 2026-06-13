//! Windows notification-area icon for the GUI.
//!
//! The service keeps controlling fans after the GUI exits, so the close button
//! still quits the process. The tray icon is an explicit convenience: restore
//! the window, hide it on purpose, or quit from the notification area.

#[cfg(windows)]
mod imp {
    use std::ptr;
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_SETVERSION,
        NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, GetMessageW, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
        LoadImageW, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
        SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
        WM_APP, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
        WM_RBUTTONUP, WNDCLASSW,
    };

    const TRAY_UID: u32 = 1;
    const TRAY_CALLBACK: u32 = WM_APP + 0x441;
    const CMD_OPEN: usize = 1001;
    const CMD_HIDE: usize = 1002;
    const CMD_QUIT: usize = 1003;
    const ICON_RESOURCE_ID: usize = 1;

    pub struct TrayIcon {
        hwnd: isize,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for TrayIcon {
        fn drop(&mut self) {
            if self.hwnd != 0 {
                unsafe {
                    PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0);
                }
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub fn spawn() -> Option<TrayIcon> {
        let (tx, rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("zugluft-tray".into())
            .spawn(move || unsafe {
                let hwnd = create_window();
                let _ = tx.send(hwnd as isize);
                if hwnd.is_null() {
                    return;
                }

                add_icon(hwnd);

                let mut msg = MSG::default();
                while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            })
            .ok()?;

        let hwnd = rx.recv().ok()?;
        if hwnd == 0 {
            let _ = thread.join();
            None
        } else {
            Some(TrayIcon {
                hwnd,
                thread: Some(thread),
            })
        }
    }

    fn create_window() -> HWND {
        let instance = unsafe { GetModuleHandleW(ptr::null()) };
        let class_name = wide_null("zugluft-tray-window");
        let class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&class);
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                wide_null("zugluft tray").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            )
        }
    }

    fn add_icon(hwnd: HWND) {
        let mut data = icon_data(hwnd);
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &data);
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        unsafe {
            Shell_NotifyIconW(NIM_SETVERSION, &data);
        }
    }

    fn remove_icon(hwnd: HWND) {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
        }
    }

    fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: TRAY_UID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_CALLBACK,
            hIcon: load_app_icon(),
            ..Default::default()
        };
        fill_wide(&mut data.szTip, "zugluft");
        data
    }

    fn load_app_icon() -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
        unsafe {
            LoadImageW(
                GetModuleHandleW(ptr::null()),
                ICON_RESOURCE_ID as _,
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE | LR_SHARED,
            ) as _
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            TRAY_CALLBACK => {
                let event = (lparam & 0xffff) as u32;
                match event {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => crate::winutil::show_main_window(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                    _ => {}
                }
                0
            }
            WM_COMMAND => {
                match wparam & 0xffff {
                    CMD_OPEN => crate::winutil::show_main_window(),
                    CMD_HIDE => crate::winutil::hide_main_window(),
                    CMD_QUIT => crate::winutil::close_main_window(),
                    _ => {}
                }
                0
            }
            WM_CLOSE => {
                remove_icon(hwnd);
                unsafe {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_DESTROY => {
                unsafe {
                    PostQuitMessage(0);
                }
                0
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    fn show_menu(hwnd: HWND) {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }

        let open = wide_null("Open zugluft");
        let hide = wide_null("Hide to tray");
        let quit = wide_null("Quit zugluft");
        unsafe {
            AppendMenuW(menu, MF_STRING, CMD_OPEN, open.as_ptr());
            AppendMenuW(menu, MF_STRING, CMD_HIDE, hide.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, ptr::null());
            AppendMenuW(menu, MF_STRING, CMD_QUIT, quit.as_ptr());
        }

        let mut point = POINT::default();
        let command = unsafe {
            GetCursorPos(&mut point);
            SetForegroundWindow(hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_RETURNCMD,
                point.x,
                point.y,
                0,
                hwnd,
                ptr::null(),
            ) as usize;
            DestroyMenu(menu);
            command
        };

        match command {
            CMD_OPEN => crate::winutil::show_main_window(),
            CMD_HIDE => crate::winutil::hide_main_window(),
            CMD_QUIT => crate::winutil::close_main_window(),
            _ => {}
        }
    }

    fn fill_wide<const N: usize>(target: &mut [u16; N], text: &str) {
        for (slot, unit) in target
            .iter_mut()
            .zip(text.encode_utf16().chain(std::iter::once(0)))
        {
            *slot = unit;
        }
    }

    fn wide_null(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct TrayIcon;

    pub fn spawn() -> Option<TrayIcon> {
        None
    }
}

pub use imp::*;
