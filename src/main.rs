//! MiniMax Meter - Usage monitor for Windows taskbar
//! Software-first implementation, expandable to hardware (Waveshare ESP32 display via BLE)

#![windows_subsystem = "windows"]

mod api;
mod credentials;
mod native;
mod quota;
mod settings;
mod tray;
mod widget;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use windows::{
    core::{HSTRING, PCWSTR},
    Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
    Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PostQuitMessage, TranslateMessage,
    },
};

use crate::api::fetch_blocking;
use crate::credentials::load as load_credentials;
use crate::settings::Settings;
use crate::tray::init_tray;
use crate::widget::{create_widget, embed_in_taskbar, position_widget, update_usage, start_timer};

struct AppState {
    settings: Settings,
    api_key: String,
}

static APP_STATE: Mutex<Option<AppState>> = Mutex::new(None);
static WIDGET_HWND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SHOULD_RUN: AtomicBool = AtomicBool::new(true);

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    if let Err(e) = run() {
        eprintln!("Error: {}", e);
    }
}

fn run() -> windows::core::Result<()> {
    // Load credentials
    let creds = match load_credentials(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load credentials: {}", e);
            eprintln!("Set MINIMAX_API_KEY environment variable or ensure ~/.claude/.credentials.json exists");
            return Ok(());
        }
    };

    // Load settings
    let app_settings = settings::load().unwrap_or_default();

    // Apply settings (including startup registration if enabled)
    let _ = settings::save(&app_settings);

    // Store app state
    {
        let mut state = APP_STATE.lock().unwrap();
        *state = Some(AppState {
            settings: app_settings.clone(),
            api_key: creds.api_key.clone(),
        });
    }

    // Create widget window
    let widget_hwnd = create_widget()?;
    WIDGET_HWND.store(widget_hwnd.0 as u64, Ordering::SeqCst);

    // Embed widget into taskbar as child window
    embed_in_taskbar(widget_hwnd);

    // Position widget inside taskbar
    position_widget(widget_hwnd);

    // Initialize system tray icon
    match init_tray() {
        Ok(_) => eprintln!("DEBUG: tray initialized"),
        Err(e) => eprintln!("Failed to init tray: {}", e),
    }

    // Initial poll (this will also render the widget)
    do_poll();

    // Start timer for periodic polling
    let poll_ms = app_settings.poll_interval_minutes * 60 * 1000;
    start_timer(widget_hwnd, poll_ms);

    // Message loop
    let mut msg = MSG::default();
    while SHOULD_RUN.load(Ordering::SeqCst) && unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.as_bool() {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

fn do_poll() {
    let api_key = {
        let state = APP_STATE.lock().unwrap();
        match state.as_ref() {
            Some(s) => s.api_key.clone(),
            None => return,
        }
    };

    match fetch_blocking(&api_key) {
        Ok(data) => {
            println!("Poll: interval {}% ({}m reset), weekly {}%",
                data.interval_percentage(),
                data.interval_reset_mins(),
                data.weekly_percentage()
            );
            update_usage(
                data.interval_percentage(),
                data.weekly_percentage(),
                data.interval_reset_mins(),
            );
        }
        Err(e) => {
            eprintln!("Poll error: {}", e);
        }
    }
}