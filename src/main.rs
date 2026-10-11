use std::num::NonZeroU32;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::fs;
use std::path::Path;
use tungstenite::{connect, Message as WsMessage};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent, MouseButton};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::dpi::{PhysicalSize, PhysicalPosition};
use winit::window::{Window, WindowId};
use raw_window_handle::HasWindowHandle;
use image::RgbaImage;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE};

enum TwitchCommand {
    ChangeChannel(String),
}

#[cfg(target_os = "windows")]
fn set_window_always_on_top(window: &Window) -> bool {
    use raw_window_handle::RawWindowHandle;
    
    if let Ok(handle) = window.window_handle() {
        if let RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
            let hwnd = HWND(win32_handle.hwnd.get() as isize);
            unsafe {
                match SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                ) {
                    Ok(_) => {
                        println!("[DEBUG] SetWindowPos succeeded for window: {:?}", hwnd);
                        return true;
                    }
                    Err(e) => {
                        println!("[DEBUG] SetWindowPos FAILED for window: {:?}, error: {:?}", hwnd, e);
                        return false;
                    }
                }
            }
        } else {
            println!("[DEBUG] Failed to get Win32 window handle");
        }
    } else {
        println!("[DEBUG] Failed to access window handle");
    }
    false
}

// Settings struct for persistence
#[derive(Clone, Debug)]
struct Settings {
    default_channel: String,
    config_width: u32,
    config_height: u32,
    config_x: i32,
    config_y: i32,
    chat_width: u32,
    chat_height: u32,
    chat_x: i32,
    chat_y: i32,
    username_font_size: u16,
    message_font_size: u16,
    channel_valid: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_channel: "criken".to_string(),
            config_width: 400,
            config_height: 300,
            config_x: 100,
            config_y: 100,
            chat_width: 400,
            chat_height: 300,
            chat_x: 500,
            chat_y: 100,
            username_font_size: 16,
            message_font_size: 14,
            channel_valid: true,
        }
    }
}

fn load_settings() -> Settings {
    let settings_path = Path::new("settings.ini");
    if !settings_path.exists() {
        return Settings::default();
    }
    
    let content = match fs::read_to_string(settings_path) {
        Ok(c) => c,
        Err(_) => return Settings::default(),
    };

    let mut settings = Settings::default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "default_channel" => settings.default_channel = value.to_string(),
                "config_width" => settings.config_width = value.parse().unwrap_or(400),
                "config_height" => settings.config_height = value.parse().unwrap_or(300),
                "config_x" => settings.config_x = value.parse().unwrap_or(100),
                "config_y" => settings.config_y = value.parse().unwrap_or(100),
                "chat_width" => settings.chat_width = value.parse().unwrap_or(400),
                "chat_height" => settings.chat_height = value.parse().unwrap_or(300),
                "chat_x" => settings.chat_x = value.parse().unwrap_or(500),
                "chat_y" => settings.chat_y = value.parse().unwrap_or(100),
                "username_font_size" => settings.username_font_size = value.parse().unwrap_or(16),
                "message_font_size" => settings.message_font_size = value.parse().unwrap_or(14),
                "channel_valid" => settings.channel_valid = value.parse().unwrap_or(true),
                _ => {}
            }
        }
    }
    settings
}

fn save_settings(settings: &Settings) {
    let content = format!(
        "# Twitch Chat Overlay Settings\n# Last generated automatically\n\ndefault_channel={}\nconfig_width={}\nconfig_height={}\nconfig_x={}\nconfig_y={}\nchat_width={}\nchat_height={}\nchat_x={}\nchat_y={}\nusername_font_size={}\nmessage_font_size={}\nchannel_valid={}\n",
        settings.default_channel,
        settings.config_width,
        settings.config_height,
        settings.config_x,
        settings.config_y,
        settings.chat_width,
        settings.chat_height,
        settings.chat_x,
        settings.chat_y,
        settings.username_font_size,
        settings.message_font_size,
        settings.channel_valid,
    );
    let _ = fs::write("settings.ini", content);
}

fn build_slider_display(font_size: i32) -> (usize, i32) {
    let min_size = 8i32;
    let max_size = 32i32;
    let bar_width = 30usize;
    
    let clamped_size = font_size.max(min_size).min(max_size);
    let percentage = ((clamped_size - min_size) as f32) / ((max_size - min_size) as f32);
    let active_count = (percentage * (bar_width as f32)).round() as usize;
    
    // Return active_count and total bar_width; rendering will apply opacity to each portion
    (active_count, clamped_size)
}

// Struct to store a cleanly parsed Twitch Chat message
struct TwitchMessage {
    user: String,
    message: String,
    color: cosmic_text::Color,
    badges: String, // e.g., "moderator/1,subscriber/12"
}

struct TextRenderer {
    font_system: cosmic_text::FontSystem,
    swash_cache: cosmic_text::SwashCache,
}

struct WindowSurface {
    window: std::sync::Arc<Window>,
    _context: softbuffer::Context<std::sync::Arc<Window>>,
    surface: softbuffer::Surface<std::sync::Arc<Window>, std::sync::Arc<Window>>,
}

struct App {
    text_renderer: Option<TextRenderer>,
    config_surface: Option<WindowSurface>,
    chat_surface: Option<WindowSurface>,

    // Shared State Layers
    chat_decorations: bool,
    chat_messages: Vec<TwitchMessage>,

    input_buffer: String,    // Stores what you are currently typing
    current_channel: String, // The active connected channel
    msg_rx: Option<Receiver<TwitchMessage>>,
    cmd_tx: Option<Sender<TwitchCommand>>, // Channel to send commands to background thread
    settings: Settings,
    last_cursor_pos: (i32, i32), // Track cursor position for slider interaction
}

impl Default for App {
    fn default() -> Self {
        let settings = load_settings();
        Self {
            text_renderer: None,
            config_surface: None,
            chat_surface: None,
            chat_decorations: false,
            chat_messages: vec![
                TwitchMessage {
                    user: "[System]".to_string(),
                    message: format!(" Skriv ett kanalnamn i config-fönstret och tryck Enter! (nuvarande: {})", settings.default_channel),
                    color: cosmic_text::Color::rgb(255, 100, 100),
                    badges: String::new(),
                }
            ],
            input_buffer: String::new(),
            current_channel: "Ingen".to_string(),
            msg_rx: None,
            cmd_tx: None,
            settings,
            last_cursor_pos: (0, 0),
        }
    }
}

impl App {
    fn update_settings_from_windows(&mut self) {
        // Capture current window positions and sizes before saving
        if let Some(config) = &self.config_surface {
            if let Some(pos) = config.window.outer_position().ok() {
                self.settings.config_x = pos.x;
                self.settings.config_y = pos.y;
            }
            let size = config.window.inner_size();
            self.settings.config_width = size.width;
            self.settings.config_height = size.height;
        }
        if let Some(chat) = &self.chat_surface {
            if let Some(pos) = chat.window.outer_position().ok() {
                self.settings.chat_x = pos.x;
                self.settings.chat_y = pos.y;
            }
            let size = chat.window.inner_size();
            self.settings.chat_width = size.width;
            self.settings.chat_height = size.height;
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let font_system = cosmic_text::FontSystem::new();
        let swash_cache = cosmic_text::SwashCache::new();
        self.text_renderer = Some(TextRenderer {
            font_system,
            swash_cache,
        });

        // 1. Spawning Borderless Overlay chat panel FIRST
        let chat_win = std::sync::Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Twitch Chat Overlays")
                        .with_transparent(true)
                        .with_decorations(self.chat_decorations)
                        .with_inner_size(PhysicalSize::new(self.settings.chat_width, self.settings.chat_height))
                        .with_position(PhysicalPosition::new(self.settings.chat_x, self.settings.chat_y)),
                )
                .unwrap(),
        );
        set_window_always_on_top(&chat_win);
        let chat_ctx = softbuffer::Context::new(chat_win.clone()).unwrap();
        let chat_surf = softbuffer::Surface::new(&chat_ctx, chat_win.clone()).unwrap();
        self.chat_surface = Some(WindowSurface {
            window: chat_win.clone(),
            _context: chat_ctx,
            surface: chat_surf,
        });
        eprintln!("[DEBUG] Chat window created with ID: {:?}", chat_win.id());

        // 2. Spawning Config control panel SECOND
        let config_win = std::sync::Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("ChatReal - Config")
                        .with_visible(true)
                        .with_inner_size(PhysicalSize::new(self.settings.config_width, self.settings.config_height))
                        .with_position(PhysicalPosition::new(self.settings.config_x, self.settings.config_y)),
                )
                .unwrap(),
        );
        // Config window doesn't need to be always-on-top; only the chat overlay does
        let config_ctx = softbuffer::Context::new(config_win.clone()).unwrap();
        let config_surf = softbuffer::Surface::new(&config_ctx, config_win.clone()).unwrap();
        self.config_surface = Some(WindowSurface {
            window: config_win.clone(),
            _context: config_ctx,
            surface: config_surf,
        });
        eprintln!("[DEBUG] Config window created with ID: {:?}", config_win.id());

        // Auto-connect to default channel if configured
        let trimmed = self.settings.default_channel.trim().to_lowercase();
        if !trimmed.is_empty() {
            self.current_channel = trimmed.clone();
            self.chat_messages.clear();
            if let Some(tx) = &self.cmd_tx {
                let _ = tx.send(TwitchCommand::ChangeChannel(self.current_channel.clone()));
            }
            self.update_settings_from_windows();
            save_settings(&self.settings);
        }
    }

    // Use AboutToWait to handle continuous rendering
    // Polls continuously to ensure chat updates even when unfocused
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Check if there are new chat messages to render
        if let Some(msg_rx) = &self.msg_rx {
            while let Ok(msg) = msg_rx.try_recv() {
                self.chat_messages.push(msg);
                
                // Keep only 15 most recent messages
                while self.chat_messages.len() > 15 {
                    self.chat_messages.remove(0);
                }
                
                // Request redraw for both windows on message arrival
                if let Some(chat) = &self.chat_surface {
                    chat.window.request_redraw();
                }
                if let Some(config) = &self.config_surface {
                    config.window.request_redraw();
                }
            }
        }
        
        // Note: event_loop.set_control_flow(ControlFlow::Poll) is set once in main()
        // Setting it here on every poll tick is redundant and wastes CPU.
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let config_id_opt = self.config_surface.as_ref().map(|s| s.window.id());
        let chat_id_opt = self.chat_surface.as_ref().map(|s| s.window.id());
        let is_config = config_id_opt.map_or(false, |id| id == window_id);
        let is_chat = chat_id_opt.map_or(false, |id| id == window_id);
        
        if matches!(event, WindowEvent::RedrawRequested) {
            eprintln!(
                "[DEBUG] RedrawRequested: window_id={:?}, config_id={:?}, chat_id={:?}, is_config={}, is_chat={}",
                window_id, config_id_opt, chat_id_opt, is_config, is_chat
            );
        }

                match event {
            WindowEvent::CloseRequested => {
                if is_config {
                    // Save settings before closing
                    self.update_settings_from_windows();
                    save_settings(&self.settings);
                    event_loop.exit();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if is_config {
                    if let Some(config) = &self.config_surface {
                        let cursor_pos = self.last_cursor_pos;
                        // Slider visual position: all 30 "=" chars with opacity, hitbox adjusted UP+LEFT 10px
                        let slider_y_min = 285;
                        let slider_y_max = 315;
                        let slider_x_min = 10;
                        let slider_x_max = 350;
                        
                        if cursor_pos.1 >= slider_y_min && cursor_pos.1 <= slider_y_max 
                            && cursor_pos.0 >= slider_x_min && cursor_pos.0 <= slider_x_max {
                            // Click on slider: map x position to font size (8-32)
                            let slider_width = (slider_x_max - slider_x_min) as f32;
                            let click_x = (cursor_pos.0 - slider_x_min) as f32;
                            let percentage = (click_x / slider_width).clamp(0.0, 1.0);
                            let new_font_size = (8.0_f32 + (percentage * 24.0)).round() as u16;
                            
                            // Capture window references before mutable borrow
                            let config_window = config.window.clone();
                            let has_chat = self.chat_surface.is_some();
                            
                            self.settings.message_font_size = new_font_size;
                            self.update_settings_from_windows();
                            save_settings(&self.settings);
                            eprintln!("[DEBUG] Font size set to: {}", self.settings.message_font_size);
                            config_window.request_redraw();
                            if has_chat {
                                if let Some(chat) = &self.chat_surface {
                                    chat.window.request_redraw();
                                }
                            }
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_pos = (position.x as i32, position.y as i32);
            }
            // Behåll ENDAST denna KeyboardInput-arm och ta bort eventuella andra kopior:
            WindowEvent::KeyboardInput { event: KeyEvent { state: ElementState::Pressed, logical_key, text, .. }, .. } => {
                if is_config {
                    match logical_key {
                        Key::Named(NamedKey::F1) => {
                            self.chat_decorations = !self.chat_decorations;
                            if let Some(chat) = &self.chat_surface {
                                chat.window.set_decorations(self.chat_decorations);
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            let trimmed = self.input_buffer.trim().to_lowercase();
                            if !trimmed.is_empty() {
                                self.current_channel = trimmed.clone();
                                self.settings.default_channel = self.current_channel.clone();
                                self.chat_messages.clear();
                                if let Some(tx) = &self.cmd_tx {
                                    let _ = tx.send(TwitchCommand::ChangeChannel(self.current_channel.clone()));
                                }
                                // Save settings after channel change
                                self.update_settings_from_windows();
                                save_settings(&self.settings);
                                self.input_buffer.clear();
                            }
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.input_buffer.pop();
                        }
                        _ => {
                            if let Some(txt) = text {
                                if txt.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                    self.input_buffer.push_str(&txt);
                                }
                            }
                        }
                    }
                    // Request redraw when config state changes
                    if let Some(config) = &self.config_surface {
                        config.window.request_redraw();
                    }
                    if let Some(chat) = &self.chat_surface {
                        chat.window.request_redraw();
                    }
                }
            }
            
            WindowEvent::Resized(physical_size) => {
                if let (Some(w), Some(h)) = (
                    NonZeroU32::new(physical_size.width),
                    NonZeroU32::new(physical_size.height),
                ) {
                    if is_config {
                        if let Some(ref mut surface_context) = self.config_surface {
                            let _ = surface_context.surface.resize(w, h);
                            surface_context.window.request_redraw();
                        }
                    }
                    if is_chat {
                        if let Some(ref mut surface_context) = self.chat_surface {
                            let _ = surface_context.surface.resize(w, h);
                            surface_context.window.request_redraw();
                        }
                    }
                    // Auto-save window position and size
                    self.update_settings_from_windows();
                    save_settings(&self.settings);
                }
            }
            WindowEvent::Moved(_) => {
                // Auto-save window position when moved
                self.update_settings_from_windows();
                save_settings(&self.settings);
            }
                        WindowEvent::RedrawRequested => {
                // Messages are now handled in about_to_wait() for immediate updates
                // This handler just renders what's already in the message buffer
                
                let tr = match &mut self.text_renderer {
                    // ... resten av din ritkod för config och chat ...

                    Some(t) => t,
                    None => return,
                };

                // --- DRAWING CONFIGURATION PANEL ---
                if is_config {
                    let surf_state = self.config_surface.as_mut().unwrap();
                    let size = surf_state.window.inner_size();
                    
                    eprintln!("[DEBUG] Config window render: size={}x{}", size.width, size.height);
                    
                    let mut buffer = surf_state.surface.buffer_mut().unwrap();
                    buffer.fill(0xFF2A2A30); // Opaque dark gray background

                    // Use fixed font size for config window (16px - static, doesn't change with chat font)
                    let font_size = 16.0_f32;
                    let line_height = (font_size * 1.3).ceil();
                    
                    let mut text_buffer = cosmic_text::Buffer::new(
                        &mut tr.font_system,
                        cosmic_text::Metrics::new(font_size, line_height),
                    );
                    text_buffer.set_size(Some(size.width as f32), Some(size.height as f32));

                    // Validate text bounds for config window
                    let valid_bounds = validate_text_within_bounds(
                        size.width as i32,
                        size.height as i32,
                        10,
                        10,
                    );

                    // Validate and wrap the input buffer to fit window width
                    let wrapped_input = validate_and_wrap_text(
                        &self.input_buffer,
                        (size.width as i32 - 20).max(50),
                        (size.height as i32 / 3).max(30),
                        18,
                        24,
                    );
                    let wrapped_input_display = if valid_bounds {
                        wrapped_input.join("\n")
                    } else {
                        String::new()
                    };

                    let (active_count, display_size) = build_slider_display(self.settings.message_font_size as i32);
                    let bar_width = 30usize;
                    let inactive_count = bar_width - active_count;
                    
                    // Build config_text as Vec<(&str, Attrs)> for set_rich_text compatibility
                    let mut config_text: Vec<(&str, cosmic_text::Attrs)> = vec![
                        (
                            "TWITCH CONTROL CONSOLE\n\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(255, 255, 255)),
                        ),
                        (
                            Box::leak(format!("Active Channel: #{}\n\n", self.current_channel).into_boxed_str()),
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(150, 100, 255)),
                        ),
                        (
                            "----------------------------------------\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(100, 100, 100)),
                        ),
                        (
                            "Enter Target Channel Name:\n > ",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(200, 200, 200)),
                        ),
                        (&wrapped_input_display, cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(255, 255, 100))),
                        ("_\n", cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(255, 255, 100))),
                        (
                            "\n----------------------------------------\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(100, 100, 100)),
                        ),
                        (
                            "Message Font Size - Click the slider below:\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(200, 200, 200)),
                        ),
                        (
                            "[F1] -> Toggle Chat Window Frame/Borders\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(150, 150, 150)),
                        ),
                        (
                            "[Enter] -> Confirm & Connect to Channel\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(150, 150, 150)),
                        ),
                        (
                            "Click slider to adjust font size (8-32px)\n",
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(150, 150, 150)),
                        ),
                    ];
                    
                    // Build slider bar with opacity: active portion bright, inactive portion dimmed
                    config_text.push(("[", cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(150, 200, 255))));
                    
                    if active_count > 0 {
                        config_text.push((
                            Box::leak(std::iter::repeat('=').take(active_count).collect::<String>().into_boxed_str()),
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgba(150, 200, 255, 255)),
                        ));
                    }
                    
                    if inactive_count > 0 {
                        config_text.push((
                            Box::leak(std::iter::repeat('=').take(inactive_count).collect::<String>().into_boxed_str()),
                            cosmic_text::Attrs::new().color(cosmic_text::Color::rgba(150, 200, 255, 77)),
                        ));
                    }
                    
                    config_text.push((
                        Box::leak(format!("] {}px", display_size).into_boxed_str()),
                        cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(150, 150, 150)),
                    ));

                    text_buffer.set_rich_text(
                        config_text.iter().cloned(),
                        &cosmic_text::Attrs::new(),
                        cosmic_text::Shaping::Advanced,
                        None,
                    );
                    text_buffer.shape_until_scroll(&mut tr.font_system, false);
                    let _msg_y_positions = blit_text_to_buffer(
                        &mut buffer,
                        &mut text_buffer,
                        tr,
                        size.width as i32,
                        size.height as i32,
                        10, // Default left padding for config window (no badges)
                        &[], // Empty slice—config window has no chat messages
                    );
                    buffer.present().unwrap();
                }

                // --- DRAWING CHAT OVERLAY PANEL ---
                if is_chat {
                    let surf_state = self.chat_surface.as_mut().unwrap();
                    let size = surf_state.window.inner_size();
                    
                    eprintln!("[DEBUG] Chat overlay render: size={}x{}, messages={}", 
                             size.width, size.height, self.chat_messages.len());
                    
                    let mut buffer = surf_state.surface.buffer_mut().unwrap();
                    buffer.fill(0x00000000); // Transparent - invisible overlay


                    // Use message font size for the base metrics (for messages and separators)
                    let msg_font_size = self.settings.message_font_size as f32;
                    let msg_line_height = (msg_font_size * 1.3).ceil();
                    
                    let mut text_buffer = cosmic_text::Buffer::new(
                        &mut tr.font_system,
                        cosmic_text::Metrics::new(msg_font_size, msg_line_height),
                    );

                    // Set boundaries to match the window's physical layout dimensions
                    text_buffer.set_size(Some(size.width as f32), Some(size.height as f32));

                    // ENABLE WORD WRAPPING HERE
                    // This forces cosmic-text to compute line-breaks at whitespace boundaries
                    text_buffer.set_wrap(cosmic_text::Wrap::Word);

                    // Track badge information: (badge_type, message_index, line_y_position)
                    // Will be populated during text rendering
                    let mut badges_to_render: Vec<(String, usize)> = Vec::new();

                    let text_spans: Vec<(String, cosmic_text::Attrs)> = {
                        let mut spans = Vec::new();
                        for (msg_idx, msg) in self.chat_messages.iter().rev().enumerate() {
                            // Collect badges for this message (without rendering as text)
                            if !msg.badges.is_empty() {
                                let badge_types = format_badges(&msg.badges);
                                for badge_type in badge_types {
                                    badges_to_render.push((badge_type.clone(), msg_idx));
                                }
                            }
                            
                            // Render message text without badge placeholders
                            spans.push((
                                msg.user.clone(),
                                cosmic_text::Attrs::new().color(msg.color),
                            ));
                            spans.push((
                                ": ".to_string(),
                                cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(200, 200, 200)),
                            ));
                            spans.push((
                                msg.message.clone(),
                                cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(255, 255, 255)),
                            ));
                            spans.push((
                                "\n".to_string(),
                                cosmic_text::Attrs::new().color(cosmic_text::Color::rgb(0, 0, 0)),
                            ));
                        }
                        spans
                    };
                    
                    let total_chars: usize = text_spans.iter().map(|(s, _)| s.len()).sum();
                    eprintln!("[DEBUG] Chat text spans: {} spans built, {} total chars", text_spans.len(), total_chars);
                    
                    for (i, (s, _)) in text_spans.iter().enumerate() {
                        if i < 10 {  // Log first 10 spans for debugging
                            eprintln!("[DEBUG]   Span {}: {:?}", i, s);
                        }
                    }

                    // Pre-compute y-position for each message independently
                    let badge_size = (msg_font_size as f32 * 1.2).ceil() as u32;
                    let mut max_badge_width = 0;
                    
                    for msg in self.chat_messages.iter() {
                        if !msg.badges.is_empty() {
                            let badge_types = format_badges(&msg.badges);
                            let msg_badge_width = (badge_size as i32 * badge_types.len() as i32) + (2 * (badge_types.len() as i32 - 1));
                            max_badge_width = max_badge_width.max(msg_badge_width);
                        }
                    }
                    
                    let text_x_offset = if max_badge_width > 0 { max_badge_width + 5 } else { 10 };
                    
                    // Define attrs for measurement phase
                    let name_attrs = cosmic_text::Attrs::new();
                    let msg_attrs = cosmic_text::Attrs::new();
                    
                    // Measure each message independently to capture exact y-position
                    let mut msg_y_positions = vec![10i32; self.chat_messages.len()];
                    for (msg_idx, msg) in self.chat_messages.iter().enumerate() {
                        let username_text = &msg.user;
                        let message_text = &msg.message;
                        
                        // Build text for this message only
                        let msg_spans = vec![
                            (username_text.clone(), name_attrs.clone()),
                            (": ".to_string(), msg_attrs.clone()),
                            (message_text.clone(), msg_attrs.clone()),
                        ];
                        
                        let attrs_spans: Vec<(&str, cosmic_text::Attrs)> = msg_spans
                            .iter()
                            .map(|(s, a)| (s.as_str(), a.clone()))
                            .collect();
                        
                        // Measure in isolation
                        let metrics = cosmic_text::Metrics::new(size.width as f32, size.height as f32);
                        let mut measure_buffer = cosmic_text::Buffer::new(&mut tr.font_system, metrics);
                        measure_buffer.set_rich_text(
                            attrs_spans,
                            &cosmic_text::Attrs::new(),
                            cosmic_text::Shaping::Advanced,
                            None,
                        );
                        measure_buffer.shape_until_scroll(&mut tr.font_system, false);
                        
                        // Capture y from first glyph
                        let mut first_y_captured = false;
                        measure_buffer.draw(
                            &mut tr.font_system,
                            &mut tr.swash_cache,
                            cosmic_text::Color::rgb(255, 255, 255),
                            |_x, y, _w, _h, _color| {
                                if !first_y_captured {
                                    msg_y_positions[msg_idx] = y + 10; // base_offset_y = 10
                                    first_y_captured = true;
                                }
                            },
                        );
                    }
                    
                    // Now build composite text with all messages
                    let attrs_spans: Vec<(&str, cosmic_text::Attrs)> = text_spans
                        .iter()
                        .map(|(s, a)| (s.as_str(), a.clone()))
                        .collect();

                    text_buffer.set_rich_text(
                        attrs_spans,
                        &cosmic_text::Attrs::new(),
                        cosmic_text::Shaping::Advanced,
                        None,
                    );
                    text_buffer.shape_until_scroll(&mut tr.font_system, false);
                    
                    // Render composite text to buffer
                    blit_text_to_buffer(
                        &mut buffer,
                        &mut text_buffer,
                        tr,
                        size.width as i32,
                        size.height as i32,
                        text_x_offset,
                        &msg_y_positions,
                    );

                    // Calculate line heights and render badges at the start of each message line
                    let badge_size = (msg_font_size as f32 * 1.2).ceil() as u32;
                    let badge_y_offset = ((msg_line_height - badge_size as f32) / 2.0).ceil() as i32; // Center vertically in line
                    
                    // Render badges at the start of each message
                    for (idx, msg) in self.chat_messages.iter().rev().enumerate() {
                        if !msg.badges.is_empty() {
                            let msg_idx = self.chat_messages.len() - 1 - idx; // Convert rev index back to forward index
                            let badge_line_y = msg_y_positions.get(msg_idx).copied()
                                .unwrap_or(10 + (msg_idx as i32 * msg_line_height.ceil() as i32));
                            
                            let badge_types = format_badges(&msg.badges);
                            let mut x_pos = 10;
                            for badge_type in badge_types {
                                if let Some(badge_img) = load_badge_image(&badge_type) {
                                    composite_badge_image(
                                        &mut buffer,
                                        &badge_img,
                                        x_pos,
                                        badge_line_y + badge_y_offset,
                                        badge_size,
                                        size.width as i32,
                                        size.height as i32,
                                    );
                                    x_pos += badge_size as i32 + 2; // Next badge with 2px spacing
                                }
                            }
                        }
                    }

                    buffer.present().unwrap();
                }
            }
            // FIXED: Added wildcard back to catch remaining Winit variants safely
            _ => (),
        }
    }
}

fn validate_text_within_bounds(width: i32, height: i32, offset_x: i32, offset_y: i32) -> bool {
    // Check if the text rendering area (after accounting for offsets) has meaningful space
    let available_width = width - offset_x;
    let available_height = height - offset_y;
    let has_adequate_space = available_width > 100 && available_height > 30;
    
    if !has_adequate_space {
        eprintln!("[WARN] Window size ({}, {}) too small for text rendering after offset ({}, {}). Available: ({}, {})",
                  width, height, offset_x, offset_y, available_width, available_height);
    }
    has_adequate_space
}

fn blit_text_to_buffer<'a>(
    buffer: &mut softbuffer::Buffer<'_, std::sync::Arc<Window>, std::sync::Arc<Window>>,
    text_buffer: &mut cosmic_text::Buffer,
    tr: &mut TextRenderer,
    width: i32,
    height: i32,
    text_offset_x: i32,
    msg_y_positions: &'a [i32],
) -> &'a [i32] {
    // Use configurable offset for text (after badges)
    let base_offset_y = 10;
    
    // Validate text will fit in bounds
    validate_text_within_bounds(width, height, text_offset_x, base_offset_y);
    
    let mut pixel_count = 0;
    text_buffer.draw(
        &mut tr.font_system,
        &mut tr.swash_cache,
        cosmic_text::Color::rgb(255, 255, 255),
        |x, y, w, h, color| {
            let offset_y = y + base_offset_y;
            
            let offset_x = x + text_offset_x;
            for row in 0..h as i32 {
                for col in 0..w as i32 {
                    let pixel_x = offset_x + col;
                    let pixel_y = offset_y + row;
                    if pixel_x >= 0 && pixel_x < width && pixel_y >= 0 && pixel_y < height {
                        let index = (pixel_y * width + pixel_x) as usize;
                        if color.a() > 0 {
                            buffer[index] = ((color.a() as u32) << 24)
                                | ((color.r() as u32) << 16)
                                | ((color.g() as u32) << 8)
                                | (color.b() as u32);
                            pixel_count += 1;
                        }
                    }
                }
            }
        },
    );
    
    if pixel_count == 0 {
        eprintln!("[WARN] blit_text_to_buffer: No pixels rendered! (buffer size: {}x{})", width, height);
    } else {
        eprintln!("[DEBUG] blit_text_to_buffer: {} pixels rendered", pixel_count);
    }
    
    msg_y_positions
}

/// Validates and truncates text to fit within window bounds
/// Returns a vector of text lines that fit within the available space
fn validate_and_wrap_text(text: &str, max_width: i32, max_height: i32, font_size: u16, line_height: i32) -> Vec<String> {
    if max_width <= 0 || max_height <= 0 {
        return vec![];
    }
    
    let chars_per_line = (max_width / (font_size as i32 / 2)).max(1) as usize;
    let max_lines = (max_height / line_height).max(1) as usize;
    
    let mut lines: Vec<String> = Vec::new();
    
    for word in text.split_whitespace() {
        if let Some(last_line) = lines.last_mut() {
            if last_line.len() + word.len() + 1 <= chars_per_line {
                last_line.push(' ');
                last_line.push_str(word);
                continue;
            }
        }
        
        if lines.len() < max_lines {
            if word.len() > chars_per_line {
                // Truncate very long words
                lines.push(word[..chars_per_line].to_string() + "…");
            } else {
                lines.push(word.to_string());
            }
        } else {
            // Out of space - truncate last line with ellipsis
            if let Some(last) = lines.last_mut() {
                if last.len() > 3 {
                    last.truncate(last.len() - 3);
                    last.push_str("…");
                }
            }
            break;
        }
    }
    
    lines
}

// Helper function to extract only MOD and SUB badges
fn format_badges(badges_str: &str) -> Vec<String> {
    if badges_str.is_empty() {
        return Vec::new();
    }

    let mut badge_list = Vec::new();
    
    // Split badges by comma (e.g., "moderator/1,subscriber/12" -> ["moderator/1", "subscriber/12"])
    for badge_pair in badges_str.split(',') {
        if let Some(slash_pos) = badge_pair.find('/') {
            let badge_type = &badge_pair[..slash_pos];
            match badge_type {
                "moderator" => badge_list.push("MOD".to_string()),
                "subscriber" => badge_list.push("SUB".to_string()),
                _ => {} // Ignore all other badge types
            };
        }
    }
    
    badge_list
}

// Load a PNG badge image from the img directory (relative to executable)
fn load_badge_image(badge_type: &str) -> Option<RgbaImage> {
    // Try multiple paths to find the badge image:
    // 1. img/ (if running from project root)
    // 2. src/img/ (if running from source)
    // 3. ../img/ (if running from target/release)
    let possible_paths = vec![
        format!("img/{}.png", badge_type),
        format!("src/img/{}.png", badge_type),
        format!("../img/{}.png", badge_type),
        format!("../../img/{}.png", badge_type),
    ];
    
    for img_path in possible_paths {
        if let Ok(img) = image::open(&img_path) {
            return Some(img.to_rgba8());
        }
    }
    
    eprintln!("[ERROR] Failed to load badge image {}.png from any search path", badge_type);
    None
}

// Composite a badge image onto the framebuffer with alpha blending
fn composite_badge_image(
    buffer: &mut [u32],
    badge_image: &RgbaImage,
    target_x: i32,
    target_y: i32,
    target_height: u32,
    buffer_width: i32,
    buffer_height: i32,
) {
    // Calculate the scale factor to match badge height to target height
    let img_height = badge_image.height();
    let scale = if img_height > 0 {
        target_height as f32 / img_height as f32
    } else {
        1.0
    };
    
    let _scaled_width = ((badge_image.width() as f32) * scale).ceil() as u32;
    let _scaled_height = target_height;
    
    // Iterate through the badge image pixels and composite them
    for img_y in 0..badge_image.height() {
        let scaled_y = ((img_y as f32) * scale) as i32;
        
        for img_x in 0..badge_image.width() {
            let scaled_x = ((img_x as f32) * scale) as i32;
            
            // Calculate target position in framebuffer
            let dst_x = target_x + scaled_x;
            let dst_y = target_y + scaled_y;
            
            // Skip if outside framebuffer bounds
            if dst_x < 0 || dst_x >= buffer_width || dst_y < 0 || dst_y >= buffer_height {
                continue;
            }
            
            let pixel = badge_image.get_pixel(img_x, img_y);
            let alpha = pixel[3] as f32 / 255.0;
            
            // Skip fully transparent pixels
            if alpha < 0.01 {
                continue;
            }
            
            let idx = (dst_y * buffer_width + dst_x) as usize;
            if idx >= buffer.len() {
                continue;
            }
            
            // Get existing pixel
            let existing = buffer[idx];
            let existing_a = ((existing >> 24) & 0xFF) as f32 / 255.0;
            let existing_r = ((existing >> 16) & 0xFF) as f32 / 255.0;
            let existing_g = ((existing >> 8) & 0xFF) as f32 / 255.0;
            let existing_b = (existing & 0xFF) as f32 / 255.0;
            
            // Alpha blend: new_color = src * src_alpha + dst * (1 - src_alpha)
            let src_r = pixel[0] as f32 / 255.0;
            let src_g = pixel[1] as f32 / 255.0;
            let src_b = pixel[2] as f32 / 255.0;
            
            let blended_r = (src_r * alpha + existing_r * (1.0 - alpha)) * 255.0;
            let blended_g = (src_g * alpha + existing_g * (1.0 - alpha)) * 255.0;
            let blended_b = (src_b * alpha + existing_b * (1.0 - alpha)) * 255.0;
            let blended_a = (alpha + existing_a * (1.0 - alpha)) * 255.0;
            
            buffer[idx] = ((blended_a as u32) << 24)
                | ((blended_r as u32) << 16)
                | ((blended_g as u32) << 8)
                | (blended_b as u32);
        }
    }
}

// Helper function to parse IRC tags and extract badges and color
fn parse_irc_tags(raw_irc: &str) -> (Option<String>, Option<String>) {
    if !raw_irc.starts_with('@') {
        return (None, None);
    }

    let tag_section_end = match raw_irc.find(' ') {
        Some(pos) => pos,
        None => return (None, None),
    };

    let tag_section = &raw_irc[1..tag_section_end];
    let mut badges = None;
    let mut color = None;

    for tag in tag_section.split(';') {
        if let Some(eq_pos) = tag.find('=') {
            let key = &tag[..eq_pos];
            let value = &tag[eq_pos + 1..];

            if key == "badges" && !value.is_empty() {
                badges = Some(value.to_string());
            } else if key == "color" && !value.is_empty() {
                color = Some(value.to_string());
            }
        }
    }

    (badges, color)
}

// Helper function to parse hex color string to cosmic_text::Color
fn parse_hex_color(hex_str: &str) -> Option<cosmic_text::Color> {
    let hex = if hex_str.starts_with('#') {
        &hex_str[1..]
    } else {
        hex_str
    };

    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(cosmic_text::Color::rgb(r, g, b))
}

fn spawn_twitch_worker(tx: std::sync::mpsc::Sender<TwitchMessage>, cmd_rx: std::sync::mpsc::Receiver<TwitchCommand>) {
    thread::spawn(move || {
        let url = "wss://irc-ws.chat.twitch.tv:443";
        let mut active_channel;
        let mut socket_opt = None;

        loop {
            // 1. Kolla efter kommandon om att byta kanal från UI-tråden
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    TwitchCommand::ChangeChannel(new_chan) => {
                        println!("[DEBUG] Tog emot kommando att byta till kanal: #{}", new_chan);
                        active_channel = new_chan;
                        socket_opt = None; // Stänger gammal anslutning
                        
                        println!("[DEBUG] Försöker ansluta till Twitch WebSocket URL: {}...", url);
                        match connect(url) {
                            Ok((mut new_socket, response)) => {
                                println!("[DEBUG] WebSocket-anslutning etablerad! HTTP Status: {}", response.status());
                                
                                // Konfigurera anslutningen till att vara icke-blockerande
                                let stream = new_socket.get_ref();
                                let tcp_stream = match stream {
                                    tungstenite::stream::MaybeTlsStream::Plain(s) => Some(s),
                                    tungstenite::stream::MaybeTlsStream::Rustls(s) => Some(s.get_ref()),
                                    _ => {
                                        println!("[DEBUG] Okänd strömtyp (inte Plain eller Rustls)");
                                        None
                                    }
                                };
                                
                                if let Some(s) = tcp_stream {
                                    if let Err(e) = s.set_nonblocking(true) {
                                        println!("[DEBUG] Misslyckades med att sätta nonblocking: {:?}", e);
                                    } else {
                                        println!("[DEBUG] Strömmen är nu satt till non-blocking.");
                                    }
                                }
                                
                                println!("[DEBUG] Skickar Twitch IRC-handskakning (PASS, NICK, JOIN)...");
                                let _ = new_socket.send(WsMessage::Text("CAP REQ :twitch.tv/tags".to_string().into()));
                                let _ = new_socket.send(WsMessage::Text("PASS JUSTINFAN1234".to_string().into()));
                                let _ = new_socket.send(WsMessage::Text("NICK justinfan1234".to_string().into()));
                                let _ = new_socket.send(WsMessage::Text(format!("JOIN #{}", active_channel.to_lowercase()).into()));
                                println!("[DEBUG] Handskakning skickad till #{}", active_channel);
                                
                                let _ = tx.send(TwitchMessage {
                                    user: "[System]".to_string(),
                                    message: " Connected successfully!".to_string(),
                                    color: cosmic_text::Color::rgb(100, 255, 100),
                                    badges: String::new(),
                                });
                                socket_opt = Some(new_socket);
                            }
                            Err(e) => {
                                println!("[DEBUG] Misslyckades med att ansluta till Twitch: {:?}", e);
                                let _ = tx.send(TwitchMessage {
                                    user: "[System]".to_string(),
                                    message: " Connection failed. Press Enter to retry.".to_string(),
                                    color: cosmic_text::Color::rgb(255, 100, 100),
                                    badges: String::new(),
                                });
                            }
                        }
                    }
                }
            }

            // 2. Läs meddelanden om det finns en aktiv anslutning
            if let Some(ref mut current_socket) = socket_opt {


                match current_socket.read() {
                    Ok(WsMessage::Text(raw_irc)) => {
                        println!("[DEBUG] Rådata mottagen: {}", raw_irc.trim());

                        // Svara på Twitch PING-anrop så vi inte blir utkastade
                        if raw_irc.starts_with("PING") {
                            let _ = current_socket.send(WsMessage::Text("PONG :tmi.twitch.tv".to_string().into()));
                            continue;
                        }

                        // Enkel parser för PRIVMSG (chattmeddelanden)
                        if raw_irc.contains("PRIVMSG") {
                            // Extract IRC tags (badges, color) from the beginning of the line
                            let (extracted_badges, extracted_color) = parse_irc_tags(raw_irc.as_ref());
                            
                            // Skip the @tags section (ends at first space), then find the username
                            let search_start = if raw_irc.starts_with('@') {
                                raw_irc.find(' ').unwrap_or(0) + 1
                            } else {
                                0
                            };
                            
                            if let Some(user_start_offset) = raw_irc[search_start..].find(':') {
                                let user_start = search_start + user_start_offset;
                                if let Some(user_end_offset) = raw_irc[user_start..].find('!') {
                                    let user_end = user_start + user_end_offset;
                                    let username = raw_irc[user_start + 1..user_end].to_string();
                                    
                                    // DEBUG: Print extracted username before any processing
                                    println!("[DEBUG] Extracted username: '{}'", username);
                                    println!("[DEBUG] search_start={}, user_start={}, user_end={}", search_start, user_start, user_end);

                                        if let Some(msg_split) = raw_irc[user_end..].find("PRIVMSG #") {
                                            let content_block = &raw_irc[user_end + msg_split..];
                                            if let Some(msg_start) = content_block.find(':') {
                                                let chat_text = content_block[msg_start + 1..].trim().to_string();

                                                println!("Ping! Nytt meddelande från: {}", username);
                                                
                                                // Debug: print extracted badges and color
                                                if let Some(ref badges) = extracted_badges {
                                                    println!("[DEBUG] Badges: {}", badges);
                                                }
                                                if let Some(ref color_str) = extracted_color {
                                                    println!("[DEBUG] Color from tags: {}", color_str);
                                                }

                                                // Determine final color: use extracted color if available, otherwise fall back to hash-based palette
                                                let color = extracted_color
                                                    .as_ref()
                                                    .and_then(|c| parse_hex_color(c))
                                                    .unwrap_or_else(|| {
                                                        // Fallback to hash-based 4-color palette
                                                        let hash = username
                                                            .bytes()
                                                            .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
                                                        match hash % 4 {
                                                            0 => cosmic_text::Color::rgb(255, 105, 180), // Rosa
                                                            1 => cosmic_text::Color::rgb(30, 144, 255),  // Blå
                                                            2 => cosmic_text::Color::rgb(50, 205, 50),   // Grön
                                                            _ => cosmic_text::Color::rgb(255, 215, 0),    // Guld
                                                        }
                                                    });

                                                let _ = tx.send(TwitchMessage {
                                                    user: username,
                                                    message: chat_text,
                                                    color,
                                                    badges: extracted_badges.unwrap_or_default(),
                                                });
                                            }
                                        }
                                }
                            }
                        }
                    }
                    Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Helt normalt i non-blocking läge (inga nya meddelanden just nu)
                    }
                    Err(e) => {
                        println!("[DEBUG] Anslutningen stängdes eller fick ett riktigt fel: {:?}", e);
                        socket_opt = None;
                    }
                    _ => {}
                }
            }

            // 3. Sov 10ms för att förhindra att tråden drar 100% CPU när den loopar
            thread::sleep(std::time::Duration::from_millis(10));
        }
    });
}

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let event_loop = EventLoop::new().unwrap();
    // Changed from ControlFlow::Wait to Poll for continuous rendering even when not in focus
    event_loop.set_control_flow(ControlFlow::Poll);

    let (msg_tx, msg_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();
    
    spawn_twitch_worker(msg_tx, cmd_rx);

    let mut app = App::default();
    app.msg_rx = Some(msg_rx);
    app.cmd_tx = Some(cmd_tx);
    
    event_loop.run_app(&mut app).unwrap();
}
