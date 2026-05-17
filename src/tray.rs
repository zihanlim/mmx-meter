//! System tray icon and menu for MiniMax Meter

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::settings;

use windows::{
    core::{HSTRING, PCWSTR},
    Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Win32::Graphics::Gdi::{BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DeleteDC, GetDC, ReleaseDC, CreateSolidBrush, FillRect, CreateCompatibleDC, SelectObject, DIB_RGB_COLORS, HBITMAP},
    Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    },
    Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
        GetCursorPos, PostQuitMessage, RegisterClassExW, SetForegroundWindow, TrackPopupMenu,
        WM_COMMAND, WM_DESTROY, WS_EX_TOOLWINDOW, WS_POPUP, MF_STRING, MF_CHECKED, TPM_LEFTALIGN, TPM_RIGHTBUTTON, TPM_RETURNCMD,
    },
};

const WM_TRAYICON: u32 = 0x8000;
const ID_EXIT: usize = 100;
const ID_REFRESH: usize = 101;
const ID_SHOW_WIDGET: usize = 102;
const ID_STARTUP: usize = 103;

static TRAY_HWND: AtomicU64 = AtomicU64::new(0);

// Embed SVG at compile time
const LOGO_SVG: &[u8] = include_bytes!("../minimax-color.svg");

fn create_icon_from_svg() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    let svg_data = LOGO_SVG;

    let opt = resvg::usvg::Options::default();
    let tree = match resvg::usvg::Tree::from_data(&svg_data, &opt) {
        Ok(t) => t,
        Err(_) => return None,
    };

    let size = 16u32;
    let mut pixmap = match resvg::tiny_skia::Pixmap::new(size, size) {
        Some(p) => p,
        None => return None,
    };

    let svg_w = tree.size().width() as f32;
    let svg_h = tree.size().height() as f32;
    let scale = size as f32 / svg_w.max(svg_h);
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(&tree, transform, &mut pixmap_mut);

    let rgba = pixmap.data();

    let color_dc = unsafe { GetDC(None) };
    let compat_dc = unsafe { CreateCompatibleDC(color_dc) };

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut color_bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let color_bmp = unsafe {
        CreateDIBSection(
            color_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut color_bits,
            None,
            0,
        )
    }.ok();

    if color_bmp.is_none() || color_bits.is_null() {
        unsafe { DeleteDC(compat_dc); ReleaseDC(None, color_dc); }
        return None;
    }

    // Copy pixel data (convert RGBA to BGRA for Windows)
    unsafe {
        let slice = std::slice::from_raw_parts_mut(color_bits.cast::<u8>(), (size * size * 4) as usize);
        for i in 0..(size * size) as usize {
            let r = rgba[i * 4 + 0];
            let g = rgba[i * 4 + 1];
            let b = rgba[i * 4 + 2];
            let a = rgba[i * 4 + 3];
            slice[i * 4 + 0] = b;     // B
            slice[i * 4 + 1] = g;     // G
            slice[i * 4 + 2] = r;     // R
            slice[i * 4 + 3] = a;     // A
        }
    }

    // Select color bitmap into DC
    unsafe {
        SelectObject(compat_dc, color_bmp.unwrap());
    }

    // Create mask - 1-bit packed format
    let mut mask_bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let mask_bmp = unsafe {
        CreateDIBSection(
            color_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut mask_bits,
            None,
            0,
        )
    }.ok();

    if mask_bmp.is_none() || mask_bits.is_null() {
        unsafe { DeleteDC(compat_dc); ReleaseDC(None, color_dc); }
        return None;
    }

    // Fill mask with 0 (all visible - AND mask)
    unsafe {
        let mask_bytes = (size * size) / 8;
        let slice = std::slice::from_raw_parts_mut(mask_bits.cast::<u8>(), mask_bytes as usize);
        slice.fill(0);
        SelectObject(compat_dc, mask_bmp.unwrap());
    }

    unsafe { DeleteDC(compat_dc); ReleaseDC(None, color_dc); };

    let icon_info = windows::Win32::UI::WindowsAndMessaging::ICONINFO {
        fIcon: windows::Win32::Foundation::BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bmp.unwrap(),
        hbmColor: color_bmp.unwrap(),
    };

    let result = unsafe { CreateIconIndirect(&icon_info) };
    eprintln!("DEBUG: CreateIconIndirect result = {:?}", result);
    Some(result.ok().unwrap_or_default())
}

pub fn init_tray() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let class = HSTRING::from("MiniMaxMeterTray");
    let inst = unsafe { GetModuleHandleW(None) }.expect("Failed to get module handle");

    let wc = windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW {
        cbSize: std::mem::size_of::<windows::Win32::UI::WindowsAndMessaging::WNDCLASSEXW>() as u32,
        hInstance: inst.into(),
        lpfnWndProc: Some(tray_window_proc),
        lpszClassName: PCWSTR::from_raw(class.as_ptr()),
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassExW(&wc);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR::from_raw(class.as_ptr()),
            PCWSTR::from_raw(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            inst,
            None,
        )
    }
    .expect("Failed to create tray window");

    TRAY_HWND.store(hwnd.0 as u64, Ordering::SeqCst);

    unsafe {
        add_tray_icon(hwnd)?;
    }

    Ok(())
}

unsafe fn add_tray_icon(hwnd: HWND) -> Result<()> {
    let hicon = create_icon_from_svg();
    eprintln!("DEBUG: tray icon = {:?}", hicon);
    let hicon = hicon.unwrap_or_default();

    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAYICON;
    nid.hIcon = hicon;

    let tip = "MiniMax Meter";
    let tip_chars: Vec<u16> = tip
        .encode_utf16()
        .chain(std::iter::once(0))
        .take(128)
        .collect();
    for (i, &c) in tip_chars.iter().enumerate() {
        nid.szTip[i] = c;
    }

    Shell_NotifyIconW(NIM_ADD, &nid);
    Ok(())
}

pub unsafe fn update_tray_icon(hwnd: HWND, pct: u32, weekly_pct: u32) {
    let tip = format!("MiniMax Meter\nInterval: {}%\nWeekly: {}%", pct, weekly_pct);
    let tip_chars: Vec<u16> = tip
        .encode_utf16()
        .chain(std::iter::once(0))
        .take(128)
        .collect();

    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_TIP;
    for (i, &c) in tip_chars.iter().enumerate() {
        nid.szTip[i] = c;
    }

    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

unsafe extern "system" fn tray_window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_TRAYICON => {
            let lp = l.0 as u32;
            if lp == 0x204 {
                // WM_RBUTTONDOWN
                show_tray_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = w.0 as usize;
            if id == ID_EXIT {
                PostQuitMessage(0);
            } else if id == ID_REFRESH {
                // Trigger refresh
            } else if id == ID_STARTUP {
                // Toggle startup setting
                if let Ok(mut s) = settings::load() {
                    s.start_with_windows = !s.start_with_windows;
                    let _ = settings::save(&s);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let menu = CreatePopupMenu().expect("Failed to create popup menu");

    let startup_enabled = settings::load().map(|s| s.start_with_windows).unwrap_or(false);
    let startup_flags = if startup_enabled {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };
    AppendMenuW(
        menu,
        startup_flags,
        ID_STARTUP,
        windows::core::PCWSTR::from_raw(HSTRING::from("Start with Windows").as_ptr()),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        ID_REFRESH,
        windows::core::PCWSTR::from_raw(HSTRING::from("Refresh Now").as_ptr()),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        ID_SHOW_WIDGET,
        windows::core::PCWSTR::from_raw(HSTRING::from("Show Widget").as_ptr()),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        ID_EXIT,
        windows::core::PCWSTR::from_raw(HSTRING::from("Exit").as_ptr()),
    );

    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );
    DestroyMenu(menu);

    if cmd.0 != 0 {
        match cmd.0 as usize {
            ID_EXIT => PostQuitMessage(0),
            ID_REFRESH => {}
            ID_SHOW_WIDGET => {}
            _ => {}
        }
    }
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    Shell_NotifyIconW(NIM_DELETE, &nid);
}
