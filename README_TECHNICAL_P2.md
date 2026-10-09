# ChatReal - Technical Documentation (Part 2)

## Twitch IRC Protocol Implementation

### Connection Establishment

#### `spawn_twitch_worker(cmd_rx, msg_tx) -> thread::JoinHandle`
**Purpose**: Spawns a separate OS thread that manages Twitch WebSocket connection and IRC protocol.
**Parameters**:
- `cmd_rx: Receiver<TwitchCommand>` - Receives commands from UI thread
- `msg_tx: Sender<TwitchMessage>` - Sends parsed messages to UI thread

**Thread Lifecycle**:
1. Enters infinite loop polling for commands and incoming data
2. When `ChangeChannel` command received:
   - Closes existing WebSocket connection (if any)
   - Creates new WebSocket connection to Twitch IRC
   - Sends IRC authentication handshake
   - Begins parsing incoming messages
3. Reads incoming IRC data in non-blocking mode
4. Parses PRIVMSG lines and extracts message data
5. Sends parsed messages to UI thread via `msg_tx`
6. Responds to PING commands to maintain connection
7. Sleeps 10ms each loop iteration to prevent CPU spinning

### IRC Protocol Details

#### WebSocket Connection
```
URL: wss://irc-ws.chat.twitch.tv:443
Protocol: IRC (Internet Relay Chat) over WebSocket Secure (WSS)
Authentication: OAuth token (or PASS justinfan1234 for anonymous)
```

#### Handshake Sequence
```
Client sends:
  PASS justinfan1234
  NICK justinfan1234
  JOIN #channel_name

Server responds with:
  001 (Welcome)
  002 (Host info)
  003-004 (Server info)
  375-376 (MOTD)
  353 (User list)
  366 (End of names list)
```

#### Message Parsing

**PRIVMSG Format (IRC)**:
```
@badges=broadcaster/1;color=#FF69B4;display-name=UserName;user-id=12345 :username!username@username.tmi.twitch.tv PRIVMSG #channel :Hello world
```

**Parsing Steps**:
1. Extract tags (everything after `@` and before first space)
   - `badges=` - User badges (broadcaster, subscriber, moderator, etc.)
   - `color=` - User's chat color in hex format
   - `display-name=` - User's display name (used instead of username for consistency)
2. Extract message sender and content
   - Split on `:` to get username
   - Split on `PRIVMSG` to get channel and message content
3. Format badges into readable string (convert IDs to labels)
4. Parse hex color to RGB tuple
5. Create TwitchMessage struct and send via channel

### Helper Functions

#### `format_badges(raw_badges: &str) -> String`
**Purpose**: Converts raw badge string from IRC into human-readable format.
**Input Format**: `broadcaster/1,moderator/1,subscriber/12`
**Output Format**: `MOD SUB` (space-separated, uppercase)
**Badge Mappings**:
- `broadcaster` → `MOD` (broadcaster has mod powers)
- `moderator` → `MOD`
- `subscriber` → `SUB`
- `vip` → `VIP`
- `premium` → `PRIME`
- Other badges → ignored for display

#### `parse_irc_tags(tags: &str) -> HashMap<String, String>`
**Purpose**: Parses IRC tag string into key-value pairs.
**Input**: `badges=broadcaster/1;color=#FF69B4;display-name=UserName`
**Output**: HashMap with keys: `badges`, `color`, `display-name`, etc.
**Logic**: Splits on `;`, then on `=` for each tag

#### `parse_hex_color(hex_str: &str) -> (u8, u8, u8)`
**Purpose**: Converts hex color string to RGB tuple.
**Input Format**: `#FF69B4` (hex) or empty string
**Output Format**: `(255, 105, 180)` (RGB decimal)
**Fallback**: If parsing fails, returns color from fallback palette based on username hash
**Fallback Palette** (4 colors):
- Index 0: Pink `(255, 105, 180)`
- Index 1: Blue `(0, 148, 211)`
- Index 2: Green `(76, 175, 80)`
- Index 3: Gold `(255, 193, 7)`

**Hash Function**: 
```rust
color_index = username.chars().map(|c| c as usize).sum::<usize>() % 4
```

#### `validate_text_within_bounds(renderer, text, x, y, width, height) -> bool`
**Purpose**: Determines if text will fit within specified rectangular bounds.
**Returns**: `true` if text fits without overflow, `false` if it would exceed bounds
**Used for**: Layout calculations to prevent text rendering outside window boundaries

---

## Message Flow Diagram

### Complete Message Lifecycle

```
┌──────────────────────────────────────────────────────────────────┐
│ User Types "mychannel" in Config Panel & Presses Enter           │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ UI Thread: Detects KeyboardInput Event (Enter Key)               │
│ - Validates channel name                                         │
│ - Sends TwitchCommand::ChangeChannel("mychannel") via cmd_tx     │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Twitch Worker Thread Receives Command                             │
│ - Closes existing WebSocket (if connected)                       │
│ - Creates new WebSocket to wss://irc-ws.chat.twitch.tv:443       │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Worker: Sends IRC Handshake                                      │
│ - PASS justinfan1234                                             │
│ - NICK justinfan1234                                             │
│ - JOIN #mychannel                                                │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Twitch Server Responds                                           │
│ - 001 Welcome response                                           │
│ - Channel user list (353, 366)                                   │
│ - Ready for messages                                             │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Streamer (or other user) Types Message in Twitch Chat            │
│ Example: "hello world"                                           │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Twitch Server Broadcasts PRIVMSG to Connected Clients            │
│ @badges=broadcaster/1;color=#FF69B4;display-name=Streamer       │
│ :streamer!streamer@streamer.tmi.twitch.tv PRIVMSG #mychannel    │
│ :hello world                                                     │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Worker Thread Reads WebSocket Data (Non-blocking)                │
│ - Detects PRIVMSG in raw IRC data                                │
│ - Calls parse_irc_tags() to extract tags                         │
│ - Calls parse_hex_color() for user color                         │
│ - Calls format_badges() for badge string                         │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Worker Creates TwitchMessage:                                    │
│ TwitchMessage {                                                  │
│     user: "Streamer",                                            │
│     message: "hello world",                                      │
│     color: (255, 105, 180),  // From #FF69B4                    │
│     badges: "MOD"            // From broadcaster/1               │
│ }                                                                │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Worker Thread Sends Message via msg_tx Channel                   │
│ (Non-blocking - message queued in channel)                       │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ UI Thread Event Loop: about_to_wait Event                        │
│ - Calls msg_rx.try_recv() to check for new messages              │
│ - Receives TwitchMessage from channel                            │
│ - Adds to App::messages VecDeque                                 │
│ - Keeps only last 15 messages (FIFO eviction)                    │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ UI Thread: Render Chat Window                                    │
│ - Clear buffer to transparent (0x00000000)                       │
│ - Iterate messages in REVERSE (newest at bottom)                 │
│ - For each message:                                              │
│   * blit_text_to_buffer() for badges "[MOD]" in color           │
│   * blit_text_to_buffer() for "Streamer" in color (255,105,180)│
│   * blit_text_to_buffer() for "hello world" in white            │
│ - Word-wrap text to fit window width                             │
│ - Flush pixel buffer to window via softbuffer                    │
└──────────────────────────┬───────────────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────────────┐
│ Message Visible in Chat Overlay Window!                          │
│ (Displayed to streamer while playing fullscreen game)            │
└──────────────────────────────────────────────────────────────────┘
```

---

## Settings & Configuration

### Settings File Format

**File**: `settings.ini` (located in application directory)

**Format**: Simple key=value pairs, one per line

**Example Content**:
```ini
default_channel=criken
config_width=800
config_height=600
config_x=100
config_y=100
chat_width=400
chat_height=600
chat_x=500
chat_y=100
username_font_size=18
message_font_size=18
```

### Settings Persistence Strategy

**Auto-save Triggers**:
- Window move/resize events (position/dimension changes)
- Font size adjustment in config panel
- Channel change (when pressing Enter with new channel)
- Application shutdown (implicit via Settings::drop)

**Load Order**:
1. Application startup loads `settings.ini`
2. If file missing, creates default Settings struct
3. If keys missing from file, uses hardcoded defaults
4. Default values ensure app runs with reasonable settings even if config corrupted

**Default Values** (if settings.ini missing/incomplete):
```rust
Settings {
    default_channel: String::new(),
    config_width: 800,
    config_height: 600,
    config_x: 100,
    config_y: 100,
    chat_width: 400,
    chat_height: 600,
    chat_x: 500,
    chat_y: 100,
    username_font_size: 18.0,
    message_font_size: 18.0,
}
```

### Window Dimension Variables

**Config Window** (Settings/Input Panel):
- `config_width`: Horizontal extent in pixels (default 800)
- `config_height`: Vertical extent in pixels (default 600)
- `config_x`: Left edge position in screen coordinates (default 100)
- `config_y`: Top edge position in screen coordinates (default 100)

**Chat Overlay Window** (Always-on-Top):
- `chat_width`: Horizontal extent in pixels (default 400)
- `chat_height`: Vertical extent in pixels (default 600)
- `chat_x`: Left edge position in screen coordinates (default 500)
- `chat_y`: Top edge position in screen coordinates (default 100)

---

## Thread Management

### Thread Architecture

```
┌─────────────────────────────┐
│  Main Thread (UI)           │
│                             │
│ - Winit Event Loop          │
│ - Window rendering          │
│ - User input handling       │
│ - Message queue management  │
│ - ControlFlow::Poll         │
└────────────┬────────────────┘
             │
    ┌────────┴────────┐
    │                 │
    ▼                 ▼
┌─────────────┐  ┌──────────────┐
│ cmd_tx/rx   │  │ msg_tx/rx    │
│ (mpsc)      │  │ (mpsc)       │
└──────┬──────┘  └──────┬───────┘
       │                │
       │                │
       ▼                │
┌──────────────────────▼────────┐
│  Worker Thread (Twitch)        │
│                                │
│ - WebSocket connection         │
│ - IRC message parsing          │
│ - Badge/color extraction       │
│ - Non-blocking socket reads    │
│ - 10ms sleep per iteration     │
└────────────────────────────────┘
```

### Channel Communication

**Command Channel** (`cmd_tx` / `cmd_rx`):
- **Sender**: UI thread
- **Receiver**: Twitch worker thread
- **Message Type**: `TwitchCommand` enum
- **Purpose**: Send commands from UI to worker (e.g., change channel)
- **Behavior**: Unbounded MPSC channel - fast, non-blocking sends

**Message Channel** (`msg_tx` / `msg_rx`):
- **Sender**: Twitch worker thread
- **Receiver**: UI thread
- **Message Type**: `TwitchMessage` struct
- **Purpose**: Send parsed chat messages from worker to UI
- **Behavior**: Unbounded MPSC channel - messages queued in UI thread
- **Reading**: UI thread calls `try_recv()` during `about_to_wait` event (non-blocking)

### Worker Thread Loop Pattern

```rust
loop {
    // Check for commands from UI
    if let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
            TwitchCommand::ChangeChannel(channel) => {
                // Close old connection, establish new one
            }
        }
    }
    
    // Read from WebSocket (non-blocking)
    match websocket.read_message() {
        Ok(msg) => {
            // Parse IRC message
            if msg.contains("PRIVMSG") {
                // Extract message data
                // Create TwitchMessage
                // Send via msg_tx
            }
            if msg.contains("PING") {
                // Respond with PONG
            }
        }
        Err(TungsteniteError::AlreadyClosed) => {
            // Connection closed, wait for new command
        }
        Err(TungsteniteError::NonBlocking) => {
            // No data ready, continue
        }
        Err(e) => {
            // Handle other errors
        }
    }
    
    // Prevent CPU spinning
    thread::sleep(Duration::from_millis(10));
}
```

### Thread Safety Guarantees

- **Channels**: Use Rust MPSC channels (safe, one sender/one receiver per direction)
- **Message Struct**: All fields are owned (String, tuple) - safe to send between threads
- **Command Enum**: Simple variants - safe to send
- **Window Handles**: Wrapped in `Arc<Window>` for thread-safe reference counting (read-only from worker)
- **No Shared Mutable State**: Each thread owns its data; communication only through channels

