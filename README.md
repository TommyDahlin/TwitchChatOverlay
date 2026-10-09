# Twitch Chat Overlay

A lightweight, always-on-top chat overlay for Twitch streamers to display live chat while playing fullscreen games.

## Features

✨ **Always-on-Top Overlay** - Stays above fullscreen games without stealing focus
- 🎮 Fullscreen game compatible (doesn't interrupt gameplay)
- 💬 Real-time message updates (even when unfocused)
- 🏷️ Mod/Sub/VIP badges with custom colors
- 👤 Username colors from Twitch IRC
- ⌨️ Adjustable font sizes (Press `+` and `-` to adjust)
- ⚙️ Config window for easy channel switching
- 💾 Auto-saves all settings

## Installation

### Option 1: Windows Installer (Recommended)
1. Download `TwitchChatOverlay_Setup_1.0.0.exe`
2. Run the installer
3. Click through the setup wizard
4. Launch from Start Menu or Desktop shortcut

### Option 2: Portable Version
1. Download `ChatReal.zip`
2. Extract to any folder
3. Run `ChatReal.exe`
4. No installation required, no registry changes

## Quick Start

1. **Launch the application**
   - The **Config Window** appears on startup (white background)
   - The **Chat Overlay** is hidden until you connect to a channel

2. **Connect to Twitch**
   - In the Config Window, type a channel name (e.g., `#criken`)
   - Press `Enter`
   - The bot connects as `justinfan1234` (anonymous read-only bot)
   - Chat messages appear in the overlay

3. **Adjust Font Sizes**
   - **Press `+` key**: Increase message font size (max 32px)
   - **Press `-` key**: Decrease message font size (min 8px)
   - Changes save automatically to `settings.ini`

4. **Toggle Overlay Visibility**
   - **Press `F1`** to hide/show the chat overlay
   - Config window always stays visible

## Controls

| Key | Action |
|-----|--------|
| `Enter` | Connect to channel (type channel name first) |
| `+` | Increase message font size (+2px) |
| `-` | Decrease message font size (-2px) |
| `F1` | Toggle chat overlay visibility |
| `Backspace` | Delete character in config window |

## Settings

Settings are saved to `settings.ini` in the same folder as the executable:

```ini
[window]
config_x=100
config_y=100
config_width=400
config_height=300

[chat]
chat_x=100
chat_y=400
chat_width=500
chat_height=300
message_font_size=14
username_font_size=16

[chat_connection]
default_channel=criken
```

You can edit this file directly to customize:
- Window positions and sizes
- Font sizes (message and username)
- Default channel on startup

Changes to font sizes via keyboard (`+`/`-` keys) save automatically.

## Features in Detail

### Badges
- **MOD** (gold) - Channel moderator
- **SUB** (gold) - Channel subscriber
- **VIP** (gold) - Channel VIP
- Badges appear before usernames in the chat overlay

### Colors
- Username colors extracted from Twitch IRC color tags
- Messages use default white text on transparent background
- Overlay window is transparent with black text for readability

### Always-On-Top Behavior
- Overlay stays above fullscreen games
- Does NOT steal focus when messages arrive
- Continues rendering even when unfocused
- Perfect for streaming or gaming

## Troubleshooting

**"White screen on startup"**
- Config window should show text field for channel name
- If no text appears, try resizing the window
- Try adjusting font sizes with `+`/`-` keys

**"Messages not appearing"**
- Verify channel name is correct (e.g., `criken` not `#criken`)
- Check internet connection
- Try another popular channel (e.g., `twitch`)

**"Overlay doesn't stay on top"**
- This is an OS-level limitation with fullscreen exclusive mode
- Try borderless fullscreen instead of exclusive fullscreen
- Check that `F1` didn't hide the overlay

**"Settings not saving"**
- Ensure you have write permissions to the application folder
- Try running as Administrator
- Check that `settings.ini` exists and is writable

## Advanced: Building from Source

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (latest stable)
- [NSIS](https://nsis.sourceforge.io/) (for building installer)

### Build Release Version
```bash
cd ChatReal\src
cargo build --release
```

Executable: `ChatReal\src\target\release\src.exe`

### Create Installer
1. Install NSIS
2. Open `ChatReal\installer.nsi` in NSIS Editor
3. Click "Compile NSI Scripts"
4. Output: `TwitchChatOverlay_Setup_1.0.0.exe`

## Technical Details

- **Language**: Rust
- **Windowing**: winit
- **Rendering**: cosmic_text
- **Chat Protocol**: Twitch IRC
- **Bot Account**: Anonymous (`justinfan1234`)
- **Platform**: Windows only (uses Windows API for always-on-top behavior)

## License

[Your License Here]

## Support

For bugs, features, or questions, please [open an issue](https://github.com/yourusername/TwitchChatOverlay/issues) or contact the developer.

---

**Happy Streaming! 🎮💬**
