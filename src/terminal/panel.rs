//! Terminal panel
//!
//! The main interface for the integrated terminal with multi-session support.

use anyhow::Result;

use super::pty::Pty;
use super::screen::{Cell, Color, TerminalScreen};

/// Default terminal height as percentage of screen
const DEFAULT_HEIGHT_PERCENT: u16 = 30;
/// Maximum terminal height as percentage of screen
const MAX_HEIGHT_PERCENT: u16 = 80;
/// Minimum terminal height in rows
const MIN_HEIGHT_ROWS: u16 = 3;

/// A single terminal session (PTY + screen buffer)
pub struct TerminalSession {
    /// PTY connection to shell
    pty: Option<Pty>,
    /// Terminal screen buffer
    screen: TerminalScreen,
}

impl TerminalSession {
    /// Create a new terminal session
    fn new(width: u16, height: u16) -> Self {
        Self {
            pty: None,
            screen: TerminalScreen::new(width, height),
        }
    }

    /// Spawn the PTY for this session
    fn spawn(&mut self, width: u16, height: u16) -> Result<()> {
        let pty = Pty::spawn(width, height)?;
        self.pty = Some(pty);
        Ok(())
    }

    /// Check if the session's shell is still alive
    fn is_alive(&self) -> bool {
        self.pty.as_ref().map(|p| p.is_alive()).unwrap_or(false)
    }

    /// Poll for output from this session
    fn poll(&mut self) -> bool {
        let mut had_data = false;

        if let Some(ref mut pty) = self.pty {
            if let Some(data) = pty.read() {
                self.screen.process(&data);
                had_data = true;
            }
        }

        // Send any queued responses back to PTY
        let responses = self.screen.drain_responses();
        for response in responses {
            if let Some(ref mut pty) = self.pty {
                let _ = pty.write(&response);
            }
        }

        had_data
    }

    /// Send input to this session
    fn send_input(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref mut pty) = self.pty {
            pty.write(data)?;
        }
        Ok(())
    }

    /// Resize this session
    fn resize(&mut self, width: u16, height: u16) {
        self.screen.resize(width, height);
        if let Some(ref pty) = self.pty {
            let _ = pty.resize(width, height);
        }
    }

    /// Get the current working directory (from OSC 7)
    pub fn cwd(&self) -> Option<&str> {
        self.screen.cwd.as_deref()
    }

    /// Get the screen buffer
    pub fn screen(&self) -> &TerminalScreen {
        &self.screen
    }
}

/// Integrated terminal panel with multi-session support
pub struct TerminalPanel {
    /// All terminal sessions
    sessions: Vec<TerminalSession>,
    /// Active session index
    active_session: usize,
    /// Whether the terminal is visible
    pub visible: bool,
    /// Terminal height in rows
    pub height: u16,
    /// Total screen height (for percentage calculations)
    screen_height: u16,
    /// Total screen width
    screen_width: u16,
}

impl TerminalPanel {
    /// Create a new terminal panel (not yet spawned)
    pub fn new(screen_width: u16, screen_height: u16) -> Self {
        let height = (screen_height * DEFAULT_HEIGHT_PERCENT / 100).max(MIN_HEIGHT_ROWS);
        Self {
            sessions: Vec::new(),
            active_session: 0,
            visible: false,
            height,
            screen_height,
            screen_width,
        }
    }

    /// Get the content height (excluding title bar)
    fn content_height(&self) -> u16 {
        self.height.saturating_sub(1).max(1)
    }

    /// Toggle terminal visibility
    pub fn toggle(&mut self) -> Result<()> {
        self.visible = !self.visible;

        // Spawn first session on first show
        if self.visible && self.sessions.is_empty() {
            self.new_session()?;
        }

        Ok(())
    }

    /// Create a new terminal session
    pub fn new_session(&mut self) -> Result<()> {
        let content_height = self.content_height();
        let mut session = TerminalSession::new(self.screen_width, content_height);
        session.spawn(self.screen_width, content_height)?;
        self.sessions.push(session);
        self.active_session = self.sessions.len() - 1;
        Ok(())
    }

    /// Close the active session. Returns true if the terminal should be hidden.
    pub fn close_active_session(&mut self) -> bool {
        if self.sessions.is_empty() {
            return true;
        }

        self.sessions.remove(self.active_session);

        if self.sessions.is_empty() {
            return true;
        }

        // Adjust active_session if needed
        if self.active_session >= self.sessions.len() {
            self.active_session = self.sessions.len() - 1;
        }

        false
    }

    /// Switch to a specific session by index
    pub fn switch_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active_session = index;
        }
    }

    /// Switch to the next session
    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_session = (self.active_session + 1) % self.sessions.len();
        }
    }

    /// Switch to the previous session
    pub fn prev_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_session = if self.active_session == 0 {
                self.sessions.len() - 1
            } else {
                self.active_session - 1
            };
        }
    }

    /// Get the number of sessions
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get the active session index
    pub fn active_session_index(&self) -> usize {
        self.active_session
    }

    /// Get a reference to all sessions (for rendering tabs)
    pub fn sessions(&self) -> &[TerminalSession] {
        &self.sessions
    }

    /// Get the CWD of the active session
    pub fn active_cwd(&self) -> Option<&str> {
        self.sessions.get(self.active_session).and_then(|s| s.cwd())
    }

    /// Hide the terminal (ESC pressed)
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Send input to the active terminal
    pub fn send_input(&mut self, data: &[u8]) -> Result<()> {
        if let Some(session) = self.sessions.get_mut(self.active_session) {
            session.send_input(data)?;
        }
        Ok(())
    }

    /// Send a key to the active terminal
    pub fn send_key(&mut self, key: &crossterm::event::KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        let data: Vec<u8> = match key.code {
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Convert to control character
                    let ctrl_char = (c.to_ascii_lowercase() as u8).wrapping_sub(b'a').wrapping_add(1);
                    vec![ctrl_char]
                } else if key.modifiers.contains(KeyModifiers::ALT) {
                    // Alt sends ESC prefix
                    vec![0x1b, c as u8]
                } else {
                    c.to_string().into_bytes()
                }
            }
            KeyCode::Enter => vec![b'\r'],
            KeyCode::Backspace => vec![0x7f],
            KeyCode::Tab => vec![b'\t'],
            KeyCode::Up => vec![0x1b, b'[', b'A'],
            KeyCode::Down => vec![0x1b, b'[', b'B'],
            KeyCode::Right => vec![0x1b, b'[', b'C'],
            KeyCode::Left => vec![0x1b, b'[', b'D'],
            KeyCode::Home => vec![0x1b, b'[', b'H'],
            KeyCode::End => vec![0x1b, b'[', b'F'],
            KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
            KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],
            KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
            KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
            KeyCode::F(n) => {
                // F1-F12 escape sequences
                match n {
                    1 => vec![0x1b, b'O', b'P'],
                    2 => vec![0x1b, b'O', b'Q'],
                    3 => vec![0x1b, b'O', b'R'],
                    4 => vec![0x1b, b'O', b'S'],
                    5 => vec![0x1b, b'[', b'1', b'5', b'~'],
                    6 => vec![0x1b, b'[', b'1', b'7', b'~'],
                    7 => vec![0x1b, b'[', b'1', b'8', b'~'],
                    8 => vec![0x1b, b'[', b'1', b'9', b'~'],
                    9 => vec![0x1b, b'[', b'2', b'0', b'~'],
                    10 => vec![0x1b, b'[', b'2', b'1', b'~'],
                    11 => vec![0x1b, b'[', b'2', b'3', b'~'],
                    12 => vec![0x1b, b'[', b'2', b'4', b'~'],
                    _ => vec![],
                }
            }
            _ => vec![],
        };

        if !data.is_empty() {
            self.send_input(&data)?;
        }
        Ok(())
    }

    /// Poll for and process PTY output. Returns true if data was received or terminal state changed.
    pub fn poll(&mut self) -> bool {
        let mut had_activity = false;

        // Poll all sessions (to keep them responsive)
        for session in &mut self.sessions {
            if session.poll() {
                had_activity = true;
            }
        }

        // Remove dead sessions
        let active_before = self.active_session;
        self.sessions.retain(|s| s.is_alive());

        if self.sessions.is_empty() {
            self.visible = false;
            return true;
        }

        // Adjust active_session if sessions were removed
        if self.active_session >= self.sessions.len() {
            self.active_session = self.sessions.len() - 1;
            had_activity = true;
        } else if active_before != self.active_session {
            had_activity = true;
        }

        had_activity
    }

    /// Get the active terminal screen for rendering
    pub fn screen(&self) -> Option<&TerminalScreen> {
        self.sessions.get(self.active_session).map(|s| s.screen())
    }

    /// Get a cell from the active terminal screen
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.screen()?.cells().get(row).and_then(|r| r.get(col))
    }

    /// Get cursor position from the active session
    pub fn cursor_pos(&self) -> (u16, u16) {
        self.sessions
            .get(self.active_session)
            .map(|s| (s.screen.cursor_row, s.screen.cursor_col))
            .unwrap_or((0, 0))
    }

    /// Update screen dimensions
    pub fn update_screen_size(&mut self, width: u16, height: u16) {
        self.screen_width = width;
        self.screen_height = height;

        // Recalculate terminal height (maintain percentage)
        let max_height = height * MAX_HEIGHT_PERCENT / 100;
        self.height = self.height.min(max_height).max(MIN_HEIGHT_ROWS);

        let content_height = self.content_height();

        // Resize all sessions
        for session in &mut self.sessions {
            session.resize(width, content_height);
        }
    }

    /// Resize terminal height
    pub fn resize_height(&mut self, new_height: u16) {
        let max_height = self.screen_height * MAX_HEIGHT_PERCENT / 100;
        self.height = new_height.min(max_height).max(MIN_HEIGHT_ROWS);

        let content_height = self.content_height();

        // Resize all sessions
        for session in &mut self.sessions {
            session.resize(self.screen_width, content_height);
        }
    }

    /// Get the starting row for rendering (from bottom of screen)
    pub fn render_start_row(&self, total_rows: u16) -> u16 {
        total_rows.saturating_sub(self.height)
    }

    /// Convert terminal Color to crossterm Color
    pub fn to_crossterm_color(color: &Color) -> crossterm::style::Color {
        use crossterm::style::Color as CtColor;
        match color {
            Color::Default => CtColor::Reset,
            Color::Black => CtColor::Black,
            Color::Red => CtColor::DarkRed,
            Color::Green => CtColor::DarkGreen,
            Color::Yellow => CtColor::DarkYellow,
            Color::Blue => CtColor::DarkBlue,
            Color::Magenta => CtColor::DarkMagenta,
            Color::Cyan => CtColor::DarkCyan,
            Color::White => CtColor::Grey,
            Color::BrightBlack => CtColor::DarkGrey,
            Color::BrightRed => CtColor::Red,
            Color::BrightGreen => CtColor::Green,
            Color::BrightYellow => CtColor::Yellow,
            Color::BrightBlue => CtColor::Blue,
            Color::BrightMagenta => CtColor::Magenta,
            Color::BrightCyan => CtColor::Cyan,
            Color::BrightWhite => CtColor::White,
            Color::Indexed(idx) => CtColor::AnsiValue(*idx),
            Color::Rgb(r, g, b) => CtColor::Rgb { r: *r, g: *g, b: *b },
        }
    }
}
