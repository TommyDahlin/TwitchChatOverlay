# ChatReal - Technical Documentation

## Table of Contents
1. [Architecture Overview](#architecture-overview)
2. [Core Structs & Data Structures](#core-structs--data-structures)
3. [Main Functions & Modules](#main-functions--modules)
4. [Twitch IRC Protocol Implementation](#twitch-irc-protocol-implementation)
5. [Rendering Pipeline](#rendering-pipeline)
6. [Message Flow Diagram](#message-flow-diagram)
7. [Settings & Configuration](#settings--configuration)
8. [Thread Management](#thread-management)
9. [Color & Badge Handling](#color--badge-handling)
10. [Variable Reference Guide](#variable-reference-guide)

---

## Architecture Overview

ChatReal is a Twitch chat overlay application designed for streamers playing fullscreen games. The application consists of two main windows:

1. **Config Panel**: A configuration window where users can set the Twitch channel, adjust font sizes, and see help text
2. **Chat Overlay**: A transparent, always-on-top window that displays incoming chat messages in real-time

### Design Principles

- **Non-blocking UI**: Uses `ControlFlow::Poll` with winit event loop to ensure UI remains responsive even when unfocused
- **Separate Threading**: Twitch WebSocket connection runs on a dedicated background thread to prevent blocking the UI thread
- **Persistent State**: Settings automatically save to `settings.ini` and load on application startup
- **Message Buffer**: Maintains only the 15 most recent messages in memory to prevent unbounded memory growth
- **Always-on-Top**: Uses Windows API `SetWindowPos` with `HWND_TOPMOST` flag to stay above fullscreen applications

### Multi-Window Architecture

```
┌─────────────────────────┐
│   Winit EventLoop       │
│  (ControlFlow::Poll)    │
└────────┬────────────────┘
         │
    ┌────┴─────────────────────────────────────┐
    │                                            │
┌───▼────────────────┐              ┌──────────▼──────────┐
│ Config Window      │              │ Chat Overlay Window │
│ (Settings UI)      │              │ (Always-on-Top)     │
│                    │              │                      │
│ - Input buffer     │              │ - Message list       │
│ - Font controls    │              │ - Text rendering     │
│ - Help text        │              │ - Transparent bg     │
└────────────────────┘              └──────────────────────┘
         │                                   │
         └─────────────┬─────────────────────┘
                       │
              ┌────────▼──────────┐
              │  Channels (mpsc)  │
              │                   │
              │ - cmd_tx/rx       │
              │ - msg_tx/rx       │
              └─────────┬──────────┘
                        │
              ┌─────────▼──────────────┐
              │  Twitch Worker Thread  │
              │                        │
              │ - WebSocket (WSS)      │
              │ - IRC handshake        │
              │ - Message parsing      │
              │ - Badge extraction     │
              └────────────────────────┘
```

---

## Core Structs & Data Structures

### 1. Settings Struct
```rust
struct Settings {
    default_channel: String,        // Default Twitch channel to join on startup
    config_width: u32,              // Width of config window in pixels
    config_height: u32,             // Height of config window in pixels
    config_x: i32,                  // X position of config window on screen
    config_y: i32,                  // Y position of config window on screen
    chat_width: u32,                // Width of chat overlay window in pixels
    chat_height: u32,               // Height of chat overlay window in pixels
    chat_x: i32,                    // X position of chat window on screen
    chat_y: i32,                    // Y position of chat window on screen
    username_font_size: f32,        // Font size for usernames (tracked but not separately rendered)
    message_font_size: f32,         // Font size for message text (currently used for both username and message)
}
```
**Purpose**: Persistent configuration storage. Automatically serialized/deserialized from `settings.ini` file.

### 2. TwitchMessage Struct
```rust
struct TwitchMessage {
    user: String,           // Username of the chat message sender
    message: String,        // Content of the chat message
    color: (u8, u8, u8),    // RGB color tuple parsed from IRC color tag (or fallback palette)
    badges: String,         // Formatted badge string (e.g., "MOD SUB VIP")
}
```
**Purpose**: Represents a single chat message received from Twitch IRC. Used to pass data between worker thread and UI thread via message channel.

### 3. TextRenderer Struct
```rust
struct TextRenderer {
    font_system: FontSystem,    // cosmic-text FontSystem: manages font loading and glyph caching
    swash_cache: SwashCache,    // Caches rendered glyph bitmap data for performance
}
```
**Purpose**: Handles all text rendering operations. Wraps cosmic-text library to provide word-wrapped text layout and glyph rasterization.

### 4. WindowSurface Struct
```rust
struct WindowSurface {
    window: Arc<Window>,        // Winit window handle (Arc for thread-safe reference counting)
    context: softbuffer::Context<Arc<Window>>,  // GPU context from softbuffer
    surface: softbuffer::Surface<Arc<Window>>,  // Drawing surface for pixel-by-pixel rendering
}
```
**Purpose**: Encapsulates all rendering resources needed to draw to a window. Combines winit window, softbuffer context, and surface for CPU-based 2D rendering.

### 5. App Struct
```rust
struct App {
    config_window: WindowSurface,           // Config panel window
    chat_window: WindowSurface,             // Chat overlay window
    input_buffer: String,                   // Current text being typed in config panel
    text_renderer: TextRenderer,            // Text rendering engine
    messages: VecDeque<TwitchMessage>,      // Queue of recent messages (max 15)
    cmd_tx: Sender<TwitchCommand>,          // Send commands to Twitch worker thread
    msg_rx: Receiver<TwitchMessage>,        // Receive messages from Twitch worker thread
    current_channel: String,                // Currently connected Twitch channel
    settings: Settings,                     // Persistent settings
    scroll_offset: usize,                   // Number of messages scrolled off the top
}
```
**Purpose**: Main application state. Manages both windows, message queue, text rendering, and communication with Twitch worker thread.

### 6. TwitchCommand Enum
```rust
enum TwitchCommand {
    ChangeChannel(String),  // Command to connect to a new Twitch channel (parameter: channel name)
}
```
**Purpose**: Commands sent from UI thread to Twitch worker thread. Extensible design for future command types.

---

## Main Functions & Modules

### Settings Functions

#### `load_settings() -> Settings`
**Purpose**: Loads application settings from `settings.ini` file or creates defaults if file doesn't exist.
**Returns**: `Settings` struct with loaded or default values.
**Key Logic**:
- Reads `settings.ini` from current directory
- Parses INI key-value pairs
- Provides sensible defaults if keys are missing:
  - Window dimensions: 800x600 for config, 400x600 for chat
  - Positions: (100, 100) for config, (500, 100) for chat
  - Font sizes: 18.0 for both username and message
  - Default channel: empty string (user must set)

#### `save_settings(settings: &Settings)`
**Purpose**: Persists current settings to `settings.ini` file.
**When called**:
- After window resize or move events
- When font size values change
- When channel is changed
- On application shutdown (implicit via drops)

---

### Window Management Functions

#### `set_window_always_on_top(window: &Window, window_id: u64)`
**Purpose**: Sets a window to always-on-top state using Windows API.
**Implementation Details**:
- Uses FFI binding to Windows `SetWindowPos` API
- Sets `HWND_TOPMOST` flag to position window above all other windows
- Critical for overlay functionality to work with fullscreen games
- Called during window creation and after certain focus events

**Windows API Call**:
```rust
SetWindowPos(
    hwnd,                   // Window handle (HWND)
    HWND_TOPMOST,           // Position after HWND_TOPMOST (always on top)
    0, 0, 0, 0,            // Position/size parameters (unused with SWP_NOMOVE | SWP_NOSIZE)
    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
)
```

### Main Event Handler

#### `window_event(&mut app, window_id, event) -> ControlFlow`
**Purpose**: Central event dispatcher for all window events.
**Handles**:
- **Resize**: Updates window dimensions in settings, resizes rendering surfaces
- **Move**: Updates window position in settings, triggers save
- **Focus/Unfocus**: Ensures chat window stays on top when refocusing
- **Key input**: Processes keyboard input for config panel
- **Close**: Saves settings and exits application

---

## Rendering Pipeline

### Config Panel Rendering
**Location**: Main event loop `about_to_wait` event
**Steps**:
1. Clear buffer to dark gray (#222222)
2. Render input buffer text with current font size
3. Render help text: "Type channel name and press Enter"
4. Render font size controls and current values
5. Flush buffer to window via softbuffer

### Chat Overlay Rendering
**Location**: Main event loop `about_to_wait` event
**Steps**:
1. Clear buffer to transparent (0x00000000) - allows desktop/game to show through
2. Get last 15 messages from queue (or fewer if not enough messages)
3. Iterate through messages in REVERSE ORDER (newest at bottom)
4. For each message:
   - Call `blit_text_to_buffer()` for badges (if any)
   - Call `blit_text_to_buffer()` for username with parsed color
   - Call `blit_text_to_buffer()` for message text in white
   - Word-wrap messages to window width using cosmic-text
   - Calculate line height and advance Y position
5. Flush buffer to window via softbuffer

### Text Blitting Function
#### `blit_text_to_buffer(renderer, buffer, width, height, text, x, y, color) -> (u32, u32)`
**Purpose**: Renders a single line of text to a pixel buffer with specified color.
**Returns**: `(new_x, new_y)` - updated cursor position after rendering
**Implementation**:
- Uses cosmic-text to layout and rasterize glyphs
- Converts glyph bitmaps (grayscale) to ARGB with specified color
- Blends with alpha channel for anti-aliasing
- Supports word wrapping within buffer boundaries

**Pixel Blending Formula**:
```
For each pixel in glyph bitmap (grayscale value 0-255):
  ARGB_output = 0xFF000000 | (color_r << 16) | (color_g << 8) | color_b
  With alpha channel consideration for anti-aliasing
```

