use crate::frecency::FrecencyStore;
use crate::modes::{Action, Item, Mode};
use crate::search::FuzzyMatcher;
use crate::ui::Popup;
use anyhow::Result;
use gartk_core::{InputEvent, Key};
use gartk_x11::{Connection, EventLoop, EventLoopConfig, Window, WindowConfig};
use std::path::PathBuf;

/// Application state
pub struct App {
    /// The popup window
    popup: Popup,
    /// Current mode
    mode: Box<dyn Mode>,
    /// Input text
    input: String,
    /// Cursor position in input
    cursor: usize,
    /// All items from the mode
    all_items: Vec<Item>,
    /// Filtered items
    filtered_items: Vec<Item>,
    /// Selected item index
    selected: usize,
    /// Scroll offset
    scroll_offset: usize,
    /// Maximum visible items
    max_visible: usize,
    /// Fuzzy matcher
    matcher: FuzzyMatcher,
    /// Result action (set when user selects an item)
    result: Option<Action>,
    /// Whether the app should quit
    should_quit: bool,
    /// Whether we have received focus (prevents early FocusOut quit)
    has_focus: bool,
}

impl App {
    /// Create a new app with the given mode
    pub fn new(mode_name: &str, prompt: Option<&str>, source: Option<PathBuf>) -> Result<Self> {
        // Connect to X11
        let conn = Connection::connect(None)?;

        // Detect monitor of active window (falls back to pointer position)
        let monitor = gartk_x11::monitor_of_active_window(&conn)?;

        // Calculate popup size and position
        let width = 600;
        let height = 400;
        let x = monitor.rect.x + (monitor.rect.width as i32 - width as i32) / 2;
        let y = monitor.rect.y + (monitor.rect.height as i32 - height as i32) / 3; // Upper third

        // Create window
        let window = Window::create(
            conn.clone(),
            WindowConfig::popup()
                .title("garlaunch")
                .class("garlaunch")
                .position(x, y)
                .size(width, height)
                .transparent(true),
        )?;

        // Grab keyboard exclusively so we receive all input and don't lose focus.
        // Retry needed because the WM may still hold a grab from the keybinding that launched us.
        window.grab_keyboard_with_retry(5, 50)?;

        // Create popup UI
        let popup = Popup::new(window, prompt)?;

        // Create mode (script mode needs source path)
        let mut mode: Box<dyn Mode> = if mode_name == "script" {
            let source = source.ok_or_else(|| anyhow::anyhow!("Script mode requires source"))?;
            crate::modes::create_script_mode(source)
        } else {
            crate::modes::create_mode(mode_name)?
        };
        mode.load()?;
        let all_items = mode.items().to_vec();

        let max_visible = 10;

        // Load frecency store for this mode
        let frecency = FrecencyStore::load(mode_name).unwrap_or_else(|e| {
            tracing::warn!("Failed to load frecency cache: {}", e);
            FrecencyStore::new(mode_name)
        });

        let mut matcher = FuzzyMatcher::new().with_frecency(frecency);

        // Apply initial frecency sorting
        let filtered_items = matcher.filter(&all_items, "");

        Ok(Self {
            popup,
            mode,
            input: String::new(),
            cursor: 0,
            filtered_items,
            all_items,
            selected: 0,
            scroll_offset: 0,
            max_visible,
            matcher,
            result: None,
            should_quit: false,
            has_focus: false,
        })
    }

    /// Run the application event loop
    pub fn run(&mut self) -> Result<()> {
        let window = self.popup.window();
        let mut event_loop = EventLoop::new(window, EventLoopConfig::default())?;

        // Initial render
        self.render()?;

        event_loop.run(|ev, event| {
            match event {
                InputEvent::Key(key_event) if key_event.pressed => {
                    self.handle_key(&key_event.key);
                    ev.request_redraw();
                }
                InputEvent::Expose => {
                    ev.request_redraw();
                }
                InputEvent::CloseRequested => {
                    self.should_quit = true;
                }
                InputEvent::FocusIn => {
                    // Mark that we have focus - prevents premature FocusOut quit
                    self.has_focus = true;
                }
                InputEvent::FocusOut => {
                    // Only close on FocusOut if we previously had focus
                    // This prevents closing when opened over empty desktop
                    if self.has_focus {
                        self.should_quit = true;
                    }
                }
                _ => {}
            }

            if ev.needs_redraw() {
                let _ = self.render();
                ev.redraw_done();
            }

            Ok(!self.should_quit)
        })?;

        Ok(())
    }

    /// Handle a key press
    fn handle_key(&mut self, key: &Key) {
        match key {
            Key::Escape => {
                self.should_quit = true;
            }
            Key::Return => {
                self.select_current();
            }
            Key::Tab => {
                // Could switch modes
            }
            Key::Up => {
                self.select_prev();
            }
            Key::Down => {
                self.select_next();
            }
            Key::PageUp => {
                for _ in 0..self.max_visible {
                    self.select_prev();
                }
            }
            Key::PageDown => {
                for _ in 0..self.max_visible {
                    self.select_next();
                }
            }
            Key::Home => {
                self.cursor = 0;
            }
            Key::End => {
                self.cursor = self.input.len();
            }
            Key::Left => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
            }
            Key::Right => {
                if self.cursor < self.input.len() {
                    self.cursor += 1;
                }
            }
            Key::Backspace => {
                if self.cursor > 0 {
                    self.input.remove(self.cursor - 1);
                    self.cursor -= 1;
                    self.filter_items();
                }
            }
            Key::Delete => {
                if self.cursor < self.input.len() {
                    self.input.remove(self.cursor);
                    self.filter_items();
                }
            }
            Key::Char(c) => {
                self.input.insert(self.cursor, *c);
                self.cursor += 1;
                self.filter_items();
            }
            Key::Space => {
                self.input.insert(self.cursor, ' ');
                self.cursor += 1;
                self.filter_items();
            }
            _ => {}
        }
    }

    /// Filter items based on current input
    fn filter_items(&mut self) {
        if self.input.is_empty() {
            self.filtered_items = self.all_items.clone();
        } else {
            self.filtered_items = self.matcher.filter(&self.all_items, &self.input);
        }

        // Reset selection
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Select the previous item
    fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    /// Select the next item
    fn select_next(&mut self) {
        if self.selected + 1 < self.filtered_items.len() {
            self.selected += 1;
            if self.selected >= self.scroll_offset + self.max_visible {
                self.scroll_offset = self.selected - self.max_visible + 1;
            }
        }
    }

    /// Select the current item
    fn select_current(&mut self) {
        if let Some(item) = self.filtered_items.get(self.selected).cloned() {
            // Record the selection for frecency
            if let Some(frecency) = self.matcher.frecency_mut() {
                frecency.record(&item.id);
                if let Err(e) = frecency.save() {
                    tracing::warn!("Failed to save frecency cache: {}", e);
                }
            }

            if let Ok(action) = self.mode.activate(&item) {
                match action {
                    Action::Close => {
                        self.should_quit = true;
                    }
                    Action::Launch(_) => {
                        self.result = Some(action);
                        self.should_quit = true;
                    }
                    Action::SwitchMode(mode_name) => {
                        // Switch to a different mode
                        if let Ok(mut new_mode) = crate::modes::create_mode(&mode_name) {
                            if new_mode.load().is_ok() {
                                self.all_items = new_mode.items().to_vec();
                                self.mode = new_mode;
                                self.input.clear();
                                self.cursor = 0;
                                self.filter_items();
                            }
                        }
                    }
                    Action::Custom(_) => {
                        self.result = Some(action);
                        self.should_quit = true;
                    }
                }
            }
        }
    }

    /// Render the popup
    fn render(&mut self) -> Result<()> {
        let visible_items: Vec<&Item> = self
            .filtered_items
            .iter()
            .skip(self.scroll_offset)
            .take(self.max_visible)
            .collect();

        let selected_visible = self.selected.saturating_sub(self.scroll_offset);

        self.popup.render(
            &self.input,
            self.cursor,
            &visible_items,
            selected_visible,
            self.filtered_items.len(),
        )?;

        Ok(())
    }

    /// Take the result action
    pub fn take_result(&mut self) -> Option<Action> {
        self.result.take()
    }
}
