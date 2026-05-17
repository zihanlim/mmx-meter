//! Floating widget window embedded in taskbar
//! Follows Claude-Code-Usage-Monitor pattern: WS_CHILD of taskbar, layered window

use image::GenericImageView;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use windows::{
    core::{HSTRING, PCWSTR},
    Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, TRUE, WPARAM},
    Win32::Graphics::Gdi::{
        BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush,
        DeleteDC, DeleteObject, EndPaint, FillRect, GetDC, InvalidateRect, ReleaseDC, SelectObject,
        SetBkMode, SetTextColor, StretchBlt, TextOutW, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS,
        FF_DONTCARE, FW_MEDIUM, OUT_TT_PRECIS, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
    },
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    Win32::UI::Shell::{SHAppBarMessage, ABM_GETTASKBARPOS, APPBARDATA},
    Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetSystemMetrics,
        GetWindowLongW, KillTimer, PostQuitMessage, RegisterClassExW, SetParent, SetTimer,
        SetWindowLongW, SetWindowPos, ShowWindow, UpdateLayeredWindow, GWL_EXSTYLE, GWL_STYLE, MSG,
        SM_CXSCREEN, SM_CYSCREEN, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW,
        SW_HIDE, SW_SHOW, ULW_ALPHA, WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_CHILD,
        WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    },
};

const WIDGET_WIDTH: i32 = 280; // Wider to fit logo
const WIDGET_HEIGHT: i32 = 44;
const TIMER_ID: usize = 1;

const COLOR_BG: u32 = 0xFF333333; // Dark charcoal, matches taskbar
const COLOR_TEXT: u32 = 0x00CCCCCC; // Light gray text
const COLOR_ACCENT: u32 = 0x00E8A87F; // Warm orange accent (like reference)
const COLOR_TRACK: u32 = 0x00555555; // Darker track blocks
const COLOR_BLOCK_EMPTY: u32 = 0x00444444; // Empty block color

static WIDGET_HWND: AtomicU64 = AtomicU64::new(0);
static WIDGET_VISIBLE: AtomicBool = AtomicBool::new(false);
static USAGE_PCT: AtomicU32 = AtomicU32::new(0);
static WEEKLY_PCT: AtomicU32 = AtomicU32::new(0);
static INTERVAL_RESET_MINS: AtomicU32 = AtomicU32::new(0);

fn get_taskbar_hwnd() -> Option<HWND> {
    unsafe {
        let class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
        windows::Win32::UI::WindowsAndMessaging::FindWindowW(
            PCWSTR::from_raw(class.as_ptr()),
            PCWSTR::null(),
        )
        .ok()
    }
}

fn get_taskbar_rect(taskbar: HWND) -> Option<RECT> {
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: taskbar,
            ..Default::default()
        };
        let result = SHAppBarMessage(ABM_GETTASKBARPOS, &mut abd);
        if result == 0 {
            None
        } else {
            Some(abd.rc)
        }
    }
}

pub fn create_widget() -> windows::core::Result<HWND> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let class = HSTRING::from("MiniMaxMeterWidget");
    let inst = unsafe { GetModuleHandleW(None) }?;

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        hInstance: inst.into(),
        lpfnWndProc: Some(widget_proc),
        lpszClassName: PCWSTR::from_raw(class.as_ptr()),
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassExW(&wc);
    }

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            PCWSTR::from_raw(class.as_ptr()),
            PCWSTR::from_raw(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            WIDGET_WIDTH,
            WIDGET_HEIGHT,
            None,
            None,
            inst,
            None,
        )
    }?;

    eprintln!("DEBUG create_widget: hwnd={}", hwnd.0 as u64);

    unsafe {
        ShowWindow(hwnd, SW_SHOW);
    }

    WIDGET_HWND.store(hwnd.0 as u64, Ordering::SeqCst);
    Ok(hwnd)
}

pub fn embed_in_taskbar(hwnd: HWND) {
    if let Some(taskbar) = get_taskbar_hwnd() {
        unsafe {
            // Keep WS_EX_LAYERED - it's needed for UpdateLayeredWindow
            // Add tool window + no activate extended styles
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let _ = SetWindowLongW(
                hwnd,
                GWL_EXSTYLE,
                ex_style | WS_EX_TOOLWINDOW.0 as i32 | WS_EX_NOACTIVATE.0 as i32,
            );

            // Convert from popup to child
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let new_style = (style & !WS_POPUP.0) | WS_CHILD.0 | WS_CLIPSIBLINGS.0;
            let _ = SetWindowLongW(hwnd, GWL_STYLE, new_style as i32);

            // Reparent into taskbar
            let _ = SetParent(hwnd, taskbar);
        }
    }
}

pub fn position_widget(hwnd: HWND) {
    // Use taskbar rect to position widget INSIDE the taskbar (child window)
    if let Some(taskbar) = get_taskbar_hwnd() {
        if let Some(rect) = get_taskbar_rect(taskbar) {
            let taskbar_h = rect.bottom - rect.top;
            let x = 10;
            let y = (taskbar_h - WIDGET_HEIGHT) / 2; // vertically centered in taskbar
            let result = unsafe {
                SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    WIDGET_WIDTH,
                    WIDGET_HEIGHT,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            };
            eprintln!(
                "DEBUG position: widget in taskbar pos={},{} taskbar_h={} result={:?}",
                x, y, taskbar_h, result
            );
            WIDGET_VISIBLE.store(true, Ordering::SeqCst);
            return;
        }
    }

    // Fallback
    let cx = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let x = 10;
    let y = 50;
    let result = unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            WIDGET_WIDTH,
            WIDGET_HEIGHT,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    };
    eprintln!(
        "DEBUG position: FALLBACK pos={},{} result={:?}",
        x, y, result
    );
    WIDGET_VISIBLE.store(true, Ordering::SeqCst);
}

pub fn hide_widget(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOZORDER | SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_HIDEWINDOW,
        );
    }
    WIDGET_VISIBLE.store(false, Ordering::SeqCst);
}

pub fn toggle_widget() {
    let h = WIDGET_HWND.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    let hwnd = HWND(h as *mut _);
    if WIDGET_VISIBLE.load(Ordering::SeqCst) {
        hide_widget(hwnd);
    } else {
        position_widget(hwnd);
    }
}

pub fn update_usage(interval_pct: u32, weekly_pct: u32, reset_mins: u32) {
    USAGE_PCT.store(interval_pct, Ordering::SeqCst);
    WEEKLY_PCT.store(weekly_pct, Ordering::SeqCst);
    INTERVAL_RESET_MINS.store(reset_mins, Ordering::SeqCst);

    let h = WIDGET_HWND.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    let hwnd = HWND(h as *mut _);
    unsafe {
        render_widget(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}

pub unsafe fn render_widget(hwnd: HWND) {
    let pct = USAGE_PCT.load(Ordering::SeqCst);
    let weekly = WEEKLY_PCT.load(Ordering::SeqCst);
    let _reset = INTERVAL_RESET_MINS.load(Ordering::SeqCst);

    let sdc = GetDC(hwnd);
    if sdc.is_invalid() {
        return;
    }
    let mdc = CreateCompatibleDC(sdc);
    if mdc.is_invalid() {
        ReleaseDC(hwnd, sdc);
        return;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: WIDGET_WIDTH,
            biHeight: -WIDGET_HEIGHT,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib = match CreateDIBSection(mdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        Ok(d) => d,
        Err(_) => {
            DeleteDC(mdc);
            ReleaseDC(hwnd, sdc);
            return;
        }
    };

    let old = SelectObject(mdc, dib);

    // Background
    let bg = CreateSolidBrush(COLORREF(COLOR_BG));
    FillRect(
        mdc,
        &RECT {
            left: 0,
            top: 0,
            right: WIDGET_WIDTH,
            bottom: WIDGET_HEIGHT,
        },
        bg,
    );
    let _ = DeleteObject(bg);

    // Logo on the left side (~40x44 area)
    'logo_block: {
        // Use embedded SVG
        const LOGO_SVG: &[u8] = include_bytes!("../minimax-color.svg");
        let svg_data = LOGO_SVG;
        eprintln!("DEBUG: using embedded SVG ({} bytes)", svg_data.len());

        // Target draw size
        let draw_w: u32 = 28;
        let draw_h: u32 = 22;
        let draw_x = 1i32;
        let draw_y = (WIDGET_HEIGHT - draw_h as i32) / 2;
        let opt = resvg::usvg::Options::default();
        let tree = match resvg::usvg::Tree::from_data(&svg_data, &opt) {
            Ok(t) => t,
            Err(e) => { eprintln!("DEBUG: failed to parse svg: {}", e); break 'logo_block; }
        };

        let svg_w = tree.size().width() as u32;
        let svg_h = tree.size().height() as u32;
        // Render at 4x native (48x48) for quality, then Lanczos down to target
        let scale = 4;
        let render_w = svg_w * scale;
        let render_h = svg_h * scale;
        let mut svg_pixmap = match resvg::tiny_skia::Pixmap::new(render_w, render_h) {
            Some(p) => p,
            None => break 'logo_block,
        };
        let transform = resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32);
        let mut svg_pixmap_mut = svg_pixmap.as_mut();
        resvg::render(&tree, transform, &mut svg_pixmap_mut);
        eprintln!("DEBUG: rendered at {}x{}", render_w, render_h);
        let svg_rgba = svg_pixmap.data();

        // Convert to image::RgbaImage for high-quality resize
        let src_img = image::RgbaImage::from_raw(render_w, render_h, svg_rgba.to_vec());
        if src_img.is_none() {
            eprintln!("DEBUG: RgbaImage::from_raw failed");
            break 'logo_block;
        }
        let src_img = src_img.unwrap();

        // Scale to target with Lanczos3 (high quality)
        let scaled = image::imageops::resize(&src_img, draw_w, draw_h, image::imageops::FilterType::Lanczos3);
        let rgba = scaled.as_raw();
        let non_zero = rgba.chunks(4).filter(|chunk| chunk[3] > 0).count();
        eprintln!("DEBUG: scaled {}x{}, non-zero pixels: {}", draw_w, draw_h, non_zero);

        let dest_start_x = draw_x as usize;
        let dest_start_y = draw_y as usize;
        let mut written = 0;
        for y in 0..draw_h as usize {
            for x in 0..draw_w as usize {
                let idx = (y * draw_w as usize + x) * 4;
                if rgba[idx + 3] > 0 {
                    let offset = ((dest_start_y + y) * WIDGET_WIDTH as usize + dest_start_x + x) * 4;
                    let slice = std::slice::from_raw_parts_mut(bits.cast::<u8>().add(offset), 4);
                    slice[0] = rgba[idx + 2];     // B
                    slice[1] = rgba[idx + 1];     // G
                    slice[2] = rgba[idx];         // R
                    slice[3] = rgba[idx + 3];     // A
                    written += 1;
                }
            }
        }
        eprintln!("DEBUG: wrote {} pixels to DIB", written);
    }

    // Block-based progress bars (10 blocks per row)
    const BLOCK_SIZE: i32 = 7;
    const BLOCK_GAP: i32 = 2;
    const BLOCKS_COUNT: i32 = 10;
    const BLOCK_ROW_Y1: i32 = 6;
    const BLOCK_ROW_Y2: i32 = 26;
    const BLOCKS_START: i32 = 65; // After logo (x=1-28) + gap

    let draw_blocks = |pct_val: u32, y: i32| {
        let filled_blocks = ((pct_val as f64 / 100.0) * BLOCKS_COUNT as f64).ceil() as i32;
        for i in 0..BLOCKS_COUNT {
            let bx = BLOCKS_START + i * (BLOCK_SIZE + BLOCK_GAP);
            let color = if i < filled_blocks {
                COLOR_ACCENT
            } else {
                COLOR_BLOCK_EMPTY
            };
            let block = CreateSolidBrush(COLORREF(color));
            FillRect(
                mdc,
                &RECT {
                    left: bx,
                    top: y,
                    right: bx + BLOCK_SIZE,
                    bottom: y + 10,
                },
                block,
            );
            let _ = DeleteObject(block);
        }
    };

    draw_blocks(pct, BLOCK_ROW_Y1);
    draw_blocks(weekly, BLOCK_ROW_Y2);

    // Text
    SetBkMode(mdc, TRANSPARENT);
    SetTextColor(mdc, COLORREF(COLOR_TEXT));

    let font = CreateFontW(
        -12,
        0,
        0,
        0,
        FW_MEDIUM.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_TT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        PCWSTR::from_raw(HSTRING::from("Segoe UI").as_ptr()),
    );
    let oldf = SelectObject(mdc, font);

    // Labels
    let l5h: Vec<u16> = "5h".encode_utf16().collect();
    let l7d: Vec<u16> = "7d".encode_utf16().collect();
    TextOutW(mdc, 40, 6, &l5h);
    TextOutW(mdc, 40, 22, &l7d);

    // Usage text with reset time
    let reset_mins = INTERVAL_RESET_MINS.load(Ordering::SeqCst);
    let reset_str = if reset_mins >= 60 {
        let hrs = reset_mins / 60;
        let mins = reset_mins % 60;
        format!("{}h{}m", hrs, mins)
    } else {
        format!("{}m", reset_mins)
    };
    let usage_txt = format!("{}% {}", pct, reset_str);
    let usage_wide: Vec<u16> = usage_txt.encode_utf16().collect();
    TextOutW(mdc, 160, 6, &usage_wide);

    let weekly_txt = format!("{}%", weekly);
    let weekly_wide: Vec<u16> = weekly_txt.encode_utf16().collect();
    TextOutW(mdc, 160, 22, &weekly_wide);

    SelectObject(mdc, oldf);
    let _ = DeleteObject(font);

    // Update layered window - use constant alpha for full opacity
    let pt = POINT { x: 0, y: 0 };
    let sz = SIZE {
        cx: WIDGET_WIDTH,
        cy: WIDGET_HEIGHT,
    };
    let blend = BLENDFUNCTION {
        BlendOp: 0, // AC_SRC_OVER
        BlendFlags: 0,
        SourceConstantAlpha: 255, // Full opacity for entire window
        AlphaFormat: 1,           // Use per-pixel alpha from logo
    };

    let result = UpdateLayeredWindow(
        hwnd,
        sdc,
        None,
        Some(&sz),
        mdc,
        Some(&pt),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
    if result.is_err() {
        // Fallback to plain BitBlt if UpdateLayeredWindow fails
        let _ = BitBlt(sdc, 0, 0, WIDGET_WIDTH, WIDGET_HEIGHT, mdc, 0, 0, SRCCOPY);
    }
    eprintln!(
        "DEBUG render_widget: UpdateLayeredWindow result={:?}",
        result
    );

    SelectObject(mdc, old);
    let _ = DeleteObject(dib);
    let _ = DeleteDC(mdc);
    let _ = ReleaseDC(hwnd, sdc);
}

pub unsafe fn paint_widget(hwnd: HWND, hdc: &windows::Win32::Graphics::Gdi::HDC) {
    // Alternative paint for child windows - not used currently
}

pub fn start_timer(hwnd: HWND, interval_ms: u32) {
    unsafe {
        let _ = SetTimer(hwnd, TIMER_ID, interval_ms, None);
    }
}

pub fn stop_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, TIMER_ID);
    }
}

unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            stop_timer(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_TIMER => {
            if w.0 == TIMER_ID as usize {}
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
