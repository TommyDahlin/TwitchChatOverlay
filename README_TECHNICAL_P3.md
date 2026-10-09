# ChatReal - Technical Documentation (Part 3 - Complete Reference)

## Color & Badge Handling

### Color System

#### Primary Color Source
- **Source**: Twitch IRC `color` tag in message metadata
- **Format**: Hex string, e.g., `#FF69B4`
- **Parsing**: 
  1. Extract hex value from tag
  2. Parse to RGB: `#RRGGBB` → `(R, G, B)` as decimal values
  3. Example: `#FF69B4` → `(255, 105, 180)` (hot pink)

#### Fallback Color Palette
- **When Used**: If `color` tag missing, malformed, or parsing fails
- **Algorithm**: Deterministic hash based on username
  ```
  hash = username.chars().map(|c| c as usize).sum::<usize>()
  color_index = hash % 4
  ```
- **Palette** (4-color rotation):
  1. Pink: `#FF69B4` (255, 105, 180)
  2. Blue: `#0094D3` (0, 148, 211)
  3. Green: `#4CAF50` (76, 175, 80)
  4. Gold: `#FFC107` (255, 193, 7)

#### Rendering
- **Usernames**: Rendered in parsed or fallback color
- **Badges**: Rendered in color (inherits from username color)
- **Message Text**: Always rendered in white (255, 255, 255)

### Badge System

#### Raw Badge String Format (from Twitch IRC)
```
broadcaster/1,moderator/1,subscriber/24,vip/1
```
- Format: `badge_id/version` pairs, comma-separated
- Version number: days subscribed, tier, etc. (mostly decorative)
- Multiple badges per user possible

#### Parsed Badge String Format (displayed)
```
MOD VIP SUB
```
- Space-separated uppercase labels
- Order: Badges displayed left-to-right in message
- One line per badge type

#### Badge Mapping Reference
| Twitch Badge ID | Display Label | Rendering | Condition |
|---|---|---|---|
| `broadcaster` | `MOD` | Text | Channel owner |
| `moderator` | `MOD` | Text | User is moderator |
| `subscriber` | `SUB` | Text | User subscribed |
| `vip` | `VIP` | Text | User is VIP |
| `premium` | `PRIME` | Text | Prime Gaming member |
| Other badges | Ignored | None | Not displayed |

#### Current Limitation
- **Text-only rendering**: Badge IDs converted to text labels only
- **Image rendering**: Not implemented (Twitch provides badge images, but app displays text only)
- **Badges Display Location**: Before username in message format: `[MOD] Username: message text`

---

## Variable Reference Guide

### App-Level Variables

#### Message Queue Management
```rust
messages: VecDeque<TwitchMessage>  // Circular buffer of recent messages
scroll_offset: usize               // Messages scrolled off top (currently unused)
```
- **Purpose**: Stores up to 15 most recent chat messages
- **Max Size**: 15 messages (configured in code)
- **Eviction Policy**: FIFO (first-in-first-out) when exceeding 15
- **Lifecycle**: Messages added when received from worker thread, removed when old

#### Channel Communication
```rust
cmd_tx: Sender<TwitchCommand>      // Sends commands to Twitch worker thread
msg_rx: Receiver<TwitchMessage>    // Receives parsed messages from worker thread
```
- **Type**: Crossbeam MPSC (Multi-Producer, Single-Consumer) channels
- **cmd_tx**: UI thread → Worker thread (non-blocking send)
- **msg_rx**: Worker thread → UI thread (non-blocking receive via try_recv)

#### State Variables
```rust
current_channel: String             // Currently connected Twitch channel
input_buffer: String                // Text being typed in config panel
```
- **current_channel**: Updated when user changes channel via Enter key
- **input_buffer**: Accumulates typed characters; cleared when Enter pressed

#### Rendering Resources
```rust
config_window: WindowSurface        // Config panel window resources
chat_window: WindowSurface          // Chat overlay window resources
text_renderer: TextRenderer         // Text rendering engine (fonts, glyphs)
```

#### Persistent State
```rust
settings: Settings                  // User preferences and window dimensions
```
- **Auto-saved**: After window moves, resizes, font changes, channel changes
- **Auto-loaded**: On application startup

### Local Variables in Event Handlers

#### Window Event Handler
```rust
PhysicalSize { width, height }     // New window dimensions in pixels
PhysicalPosition { x, y }          // New window position on screen
```

#### Rendering Functions
```rust
y_offset: u32                       // Current vertical position for text blitting
x: f32                              // Horizontal position for character rendering
line_height: u32                    // Height of single line of text
```

#### Message Rendering Loop
```rust
i: usize                            // Index in messages deque
msg: &TwitchMessage                 // Current message being rendered
y_pos: u32                          // Vertical position in window
```

### Worker Thread Variables

#### WebSocket Connection
```rust
websocket: WebSocket<MaybeTlsStream<TcpStream>>  // Tungstenite WebSocket handle
```
- **Type**: TLS-encrypted WebSocket connection
- **URL**: `wss://irc-ws.chat.twitch.tv:443`
- **Non-blocking**: Explicitly set to non-blocking mode
- **Authentication**: Uses anonymous `justinfan1234` account (no OAuth required)

#### Message Parsing
```rust
raw_data: String                    // Raw IRC message received from server
lines: Vec<&str>                    // Split raw data by newlines
tags: HashMap<String, String>       // Parsed IRC tags from message
```

#### Connection State
```rust
is_connected: bool                  // Current WebSocket connection state
current_channel_name: String        // Channel currently joined in IRC
```

---

## Rendering Implementation Details

### Pixel Buffer Format
- **Color Depth**: 32-bit ARGB (8 bits per channel)
- **Byte Order**: 0xAARRGGBB (Alpha, Red, Green, Blue)
- **Layout**: Row-major (Y-major), pixels left-to-right, top-to-bottom
- **Buffer Access**: `buffer[y * width + x] = 0xAARRGGBB`

### Transparent Background Rendering
- **Transparency Color**: `0x00000000` (fully transparent black)
- **Purpose**: Allows game/desktop to show through overlay window
- **Windows Handling**: Windows compositor honors alpha channel; fully transparent pixels become invisible

### Text Blitting Algorithm
```rust
fn blit_text_to_buffer(
    renderer: &TextRenderer,
    buffer: &mut [u32],
    width: u32,
    height: u32,
    text: &str,
    x: u32,
    y: u32,
    color: (u8, u8, u8),
) -> (u32, u32)
```

**Steps**:
1. Create `Layout` from text using cosmic-text
2. Shape glyphs for specified font size
3. For each glyph:
   - Get rasterized bitmap (grayscale 0-255)
   - For each pixel in bitmap:
     - Scale grayscale value to alpha channel
     - Create ARGB pixel: `0xFF + (R<<16) + (G<<8) + B`
     - Blend into buffer with alpha composition
4. Return new X,Y cursor position

**Word Wrapping**:
- `cosmic_text::Wrap::Word` enabled
- Text automatically breaks at word boundaries
- Prevents mid-word splitting

### Font Rendering
- **Library**: `cosmic-text` (pure Rust text layout)
- **Font Loading**: System fonts searched via fontdb
- **Glyph Caching**: SwashCache stores rasterized glyphs
- **Font Size**: Configurable per font size (username_font_size, message_font_size)

---

## Performance Optimization Details

### Memory Management
```rust
// Bounded message buffer
const MAX_MESSAGES: usize = 15;
if messages.len() >= MAX_MESSAGES {
    messages.pop_front();  // FIFO eviction
}
```
- **Reason**: Prevents unbounded memory growth in long chat sessions
- **Trade-off**: Oldest messages disappear when buffer full
- **Justification**: Overlay use case - streamers only need recent messages

### CPU Efficiency
```rust
// Worker thread
thread::sleep(Duration::from_millis(10));  // 10ms sleep per loop iteration
```
- **Reason**: Prevents busy-waiting and 100% CPU spin
- **Effect**: ~10ms message latency (acceptable for streaming)
- **Alternative**: Could use event-driven model (more complex)

### Rendering Frequency
```rust
ControlFlow::Poll  // Event loop polls ~60 FPS (platform-dependent)
```
- **Frequency**: Window renders on every event loop iteration
- **Vertical Sync**: Handled by OS/GPU compositor
- **Result**: Smooth message scrolling/animation

### Channel Performance
- **MPSC Channels**: Lock-free data structure for thread communication
- **try_recv()**: Non-blocking - returns immediately if queue empty
- **Throughput**: Millions of messages per second (far exceeds Twitch rate)

---

## Known Limitations & Design Quirks

### Font Size Inconsistency
```rust
// In Settings struct
username_font_size: f32,   // Tracked but not used separately
message_font_size: f32,    // Used for both username and message
```
- **Issue**: Two font sizes stored but only one applied
- **Reason**: Architectural leftover from development iteration
- **Impact**: Usernames and messages always render at same size
- **Fix**: Would require refactoring TextRenderer to accept per-text font sizes

### Badge Image Support
- **Current**: Text-only badge labels (`MOD`, `SUB`, `VIP`)
- **Twitch Provides**: Badge PNG images (3 sizes: 18px, 36px, 72px)
- **Limitation**: App doesn't download/render badge images
- **Reason**: Simplifies implementation; text labels sufficient for MVP
- **Potential Enhancement**: Fetch images from Twitch CDN and render as sprites

### Anonymous Authentication
- **Account**: `justinfan1234` (public, well-known anonymous account)
- **Authentication**: `PASS justinfan1234` (no OAuth token)
- **Limitation**: Cannot authenticate as specific user
- **Effect**: Cannot see mod-only channels or get personalized features
- **Reason**: Simplifies deployment (no user auth flow needed)

### Non-Blocking Socket Tradeoff
- **Benefit**: UI never freezes while waiting for Twitch response
- **Drawback**: Must handle `WouldBlock` errors and retry
- **Latency**: ~10ms message display delay due to thread sleep

### Platform Specificity
- **Windows Only**: Uses Windows API (`SetWindowPos`) for always-on-top
- **Rendering**: CPU-based softbuffer (works everywhere but slower than GPU)
- **Future**: Could add macOS/Linux support but requires platform-specific window managers

---

## Building & Deployment

### Dependencies
```toml
[dependencies]
winit = "0.30"          # Window management and events
softbuffer = "0.4"      # CPU-based 2D rendering
cosmic-text = "0.12"    # Text layout and rendering
tungstenite = "0.24"    # WebSocket client
crossbeam = "0.8"       # Thread-safe channels
serde = "1.0"           # Serialization (for future use)
```

### Build Process
- **Debug Build**: `cargo build` (unoptimized, faster compile)
- **Release Build**: `cargo build --release` (optimized, ~3-5MB executable)
- **MSI Installer**: Uses `cargo-wix` to package release binary into Windows installer

### Deployment
- **Standalone Executable**: Direct Windows binary (requires no installation)
- **MSI Installer**: Windows installer package (registers in Programs & Features)
- **Portable**: Can run from USB drive or cloud storage

---

## Implementation Highlights

### Always-On-Top Window Behavior
- **Mechanism**: Windows API `SetWindowPos()` with `HWND_TOPMOST` flag
- **Effect**: Window stays above all other windows (including fullscreen games)
- **Refresh**: Re-applied on certain focus events to maintain top-most status
- **Requirement**: Critical for streamer overlay use case

### Non-Blocking Networking
- **Mechanism**: `websocket.set_nonblocking(true)` on Tungstenite socket
- **Effect**: `read_message()` returns immediately with error if no data ready
- **Benefit**: Worker thread never blocks on network I/O
- **Drawback**: Must explicitly loop and sleep to avoid CPU spinning

### Continuous Rendering While Unfocused
- **Mechanism**: `ControlFlow::Poll` in winit event loop
- **Effect**: Window continues rendering even without focus
- **Requirement**: Winit event loop calls render code every frame regardless of focus
- **Benefit**: Chat updates visible even when game is focused

### Message FIFO Scrolling
- **Mechanism**: Render messages in reverse order (newest at bottom)
- **Effect**: New messages appear at bottom, older messages scroll up
- **Rendering Order**: Last message → First message (reverse iteration of deque)
- **Visual Result**: Natural reading flow for chat overlay

---

## Testing Recommendations

### Unit Tests to Implement
1. **Color Parsing**: Test hex → RGB conversion with various inputs
2. **Badge Formatting**: Test badge string transformation (broadcaster → MOD, etc.)
3. **IRC Parsing**: Test message extraction from raw IRC format
4. **Settings I/O**: Test save/load roundtrip for all settings

### Integration Tests
1. **Channel Connection**: Verify WebSocket connects to real Twitch channel
2. **Message Reception**: Verify messages received and displayed
3. **Window Management**: Test window positioning, sizing, always-on-top persistence
4. **Threading**: Verify no deadlocks or channel drops under sustained load

### Manual Testing Checklist
- [ ] Launch app and verify both windows appear
- [ ] Type channel name and press Enter (should connect)
- [ ] Send test message via Twitch web chat (should appear in overlay)
- [ ] Move windows and restart app (positions should persist)
- [ ] Test font size adjustments and verify persistence
- [ ] Verify always-on-top works with fullscreen game
- [ ] Test with high message volume (~50+ messages/second)

---

## Future Enhancement Ideas

1. **Animated Messages**: Fade-in effect when new messages arrive
2. **Message Timeouts**: Auto-remove messages after 30 seconds on screen
3. **Emote Rendering**: Parse and render Twitch emotes (BTTV, FFZ, 7TV)
4. **Multi-Channel**: Monitor multiple channels simultaneously
5. **Filtering**: Filter messages by badge type (mods only, subs only, etc.)
6. **Custom Fonts**: User-selectable fonts instead of system default
7. **Message Highlighting**: Highlight specific usernames or keywords
8. **Statistics**: Track message volume, user activity, etc.
9. **Oauth Login**: Support authenticated connections for personalized experience
10. **Cross-Platform**: Port to macOS and Linux

---

## Summary

ChatReal implements a production-grade Twitch chat overlay specifically optimized for streamers playing fullscreen games. The architecture emphasizes:

- **Non-blocking I/O**: Worker thread prevents UI thread from ever freezing
- **Memory Efficiency**: Bounded message buffer with FIFO eviction
- **Simplicity**: No external authentication required (anonymous mode)
- **Reliability**: Thread-safe channels for all communication
- **Always-On-Top**: Windows API integration ensures overlay stays visible
- **Persistence**: Automatic save/load of user settings

The implementation demonstrates solid Rust practices including thread spawning, MPSC channels, error handling, and cross-platform UI development with winit. The codebase is well-commented and documented for future maintenance and enhancement.

