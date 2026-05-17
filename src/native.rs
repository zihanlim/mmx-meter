//! Native Windows interop

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{SHAppBarMessage, ABM_GETTASKBARPOS, APPBARDATA};

/// Get the taskbar window handle
pub fn find_taskbar() -> Option<HWND> {
    unsafe {
        let class = wide_str("Shell_TrayWnd");
        windows::Win32::UI::WindowsAndMessaging::FindWindowW(
            PCWSTR::from_raw(class.as_ptr()),
            PCWSTR::null(),
        ).ok()
    }
}

/// Get taskbar position
pub fn get_taskbar_rect(taskbar: HWND) -> Option<windows::Win32::Foundation::RECT> {
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: taskbar,
            ..Default::default()
        };
        let result = SHAppBarMessage(ABM_GETTASKBARPOS, &mut abd);
        if result == 0 { None } else { Some(abd.rc) }
    }
}

/// Wide string helper
pub fn wide_str(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Wrapper to make HWND sendable
#[derive(Clone, Copy)]
pub struct SendHWND(isize);

unsafe impl Send for SendHWND {}

impl SendHWND {
    pub fn from_hwnd(hwnd: HWND) -> Self { Self(hwnd.0 as isize) }
    pub fn to_hwnd(self) -> HWND { HWND(self.0 as *mut std::ffi::c_void) }
}