//! Application settings stored in %APPDATA%\mmx-meter\settings.json

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Poll interval in minutes (default: 10)
    pub poll_interval_minutes: u32,
    /// Start app when Windows starts
    pub start_with_windows: bool,
    /// Enable BLE hardware display (future feature)
    pub hardware_ble_enabled: bool,
    /// Region: "auto", "global", or "cn"
    pub region: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval_minutes: 10,
            start_with_windows: false,
            hardware_ble_enabled: false,
            region: "auto".to_string(),
        }
    }
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mmx-meter")
        .join("settings.json")
}

pub fn load() -> Result<Settings> {
    let path = settings_path();
    if !path.exists() {
        return Ok(Settings::default());
    }
    let content = std::fs::read_to_string(&path)
        .context("Failed to read settings file")?;
    serde_json::from_str(&content)
        .context("Failed to parse settings JSON")
        .map(|mut s: Settings| {
            // Ensure defaults for missing fields
            if s.poll_interval_minutes == 0 {
                s.poll_interval_minutes = 10;
            }
            s
        })
}

pub fn save(settings: &Settings) -> Result<()> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context("Failed to create config directory")?;
    }

    let content = serde_json::to_string_pretty(settings)
        .context("Failed to serialize settings")?;
    std::fs::write(&path, content)
        .context("Failed to write settings file")?;

    // Always update startup setting to match current settings
    set_startup_enabled(settings.start_with_windows)?;
    Ok(())
}

pub fn set_startup_enabled(enabled: bool) -> Result<()> {
    let exe_path = std::env::current_exe()
        .context("Failed to get current executable path")?;

    eprintln!("DEBUG set_startup_enabled: enabled={}, exe_path={}", enabled, exe_path.display());

    let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let value_name = "MiniMaxMeter";

    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(key_path, KEY_WRITE)
        .context("Failed to open registry key")?;

    eprintln!("DEBUG: opened registry key");

    if enabled {
        eprintln!("DEBUG: setting value to {}", exe_path.to_string_lossy());
        run_key.set_value(value_name, &exe_path.to_string_lossy().to_string())
            .context("Failed to set registry value")?;
        eprintln!("DEBUG: value set successfully");
    } else {
        eprintln!("DEBUG: deleting value");
        let _ = run_key.delete_value(value_name);
    }

    Ok(())
}