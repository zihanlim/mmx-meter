# MiniMax Meter

A Windows desktop application that monitors MiniMax API usage in real-time and displays it in the taskbar.

## Features

- **Taskbar Widget** - Displays usage in a compact widget embedded in the Windows taskbar
- **System Tray Icon** - Shows MiniMax logo with usage tooltip
- **5-Hour Interval Tracking** - Rolling window usage with progress bars
- **Weekly Usage Tracking** - Weekly total usage monitoring
- **Start with Windows** - Option to auto-start on login
- **Right-Click Menu** - Quick access to refresh, show/hide widget, and exit

## Requirements

- Windows 10/11
- MiniMax API key

## Setup

### 1. Get your MiniMax API Key

The app does **not** store your API key. You must provide it at runtime. The app checks for your API key in this order of priority:

1. **Command-line argument** (not yet implemented)
2. **Environment variable** `MINIMAX_API_KEY`
3. **Claude credentials file** `~/.claude/.credentials.json`
4. **App config file** `%APPDATA%\mmx-meter\config.json`

**Option A: Set environment variable (recommended for testing)**
```powershell
$env:MINIMAX_API_KEY = "your-api-key-here"
```

**Option B: Use Claude credentials file**
If you use Claude Code or the MiniMax Agent, your API key may already be in:
```
~/.claude/.credentials.json
```

**Option C: Create a config file**
Create `%APPDATA%\mmx-meter\config.json`:
```json
{
  "api_key": "your-api-key-here"
}
```

### 2. Build

```bash
# Debug build
cargo build

# Release build (recommended)
cargo build --release
```

### 3. Run

```bash
# Debug
cargo run

# Release
./target/release/mmx-meter.exe
```

The executable is at `target/release/mmx-meter.exe`.

## Usage

### Tray Icon
- **Right-click** the tray icon to access the menu
- **Left-click** the tray icon to toggle widget visibility

### Menu Options
- **Start with Windows** - Enable/disable auto-start on login
- **Refresh Now** - Manually refresh usage data
- **Show Widget** - Show/hide the taskbar widget
- **Exit** - Close the application

### Widget Display
The widget appears embedded in your taskbar:
- MiniMax logo on the left
- 5h interval usage bars (top row)
- Weekly usage bars (bottom row)
- Percentage and reset time

![MiniMax Meter Widget](https://minimax-algeng-chat-tts-us.oss-us-east-1.aliyuncs.com/ccv2%2F2026-05-17%2FMiniMax-M2.7%2F2024092508790731553%2Fd200b6acee6c598d563821fda93702bacffb1aa79c5eaff2ae01c9e137582fcd..png?Expires=1779094449&OSSAccessKeyId=LTAI5tCpJNKCf5EkQHSuL9xg&Signature=pCk3ABBQJuK%2F9rGRJpsvWmxwHyc%3D)

## Configuration

Settings are stored in `%APPDATA%\mmx-meter\settings.json`:

```json
{
  "poll_interval_minutes": 10,
  "start_with_windows": false,
  "hardware_ble_enabled": false,
  "region": "auto"
}
```

## Troubleshooting

**Widget not visible?**
The widget appears inside the taskbar. If not showing, other taskbar items may be pushing it out of view. Try right-clicking tray → Show Widget.

**Tray icon missing?**
Look in the system tray (bottom-right near the clock). The icon may be in the overflow area - click the arrow to see all tray icons.

**API errors?**
Ensure your API key is available via one of these methods:
- Set `MINIMAX_API_KEY` environment variable
- Ensure `~/.claude/.credentials.json` exists with your key
- Create `%APPDATA%\mmx-meter\config.json` with `{"api_key": "your-key"}`

## Security

Your API key is **never** stored in the executable or committed to git. It is only read at runtime from:
- Environment variable
- Claude's existing credentials file
- Your local config file (which is gitignored)

## License

MIT