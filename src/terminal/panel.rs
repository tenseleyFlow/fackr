//! Terminal panel
//!
//! The main interface for the integrated terminal.

use anyhow::Result;

use super::pty::Pty;
use super::screen::{Cell, Color, TerminalScreen};

/// Default terminal height as percentage of screen
const DEFAULT_HEIGHT_PERCENT: u16 = 30;
/// Maximum terminal height as percentage of screen
const MAX_HEIGHT_PERCENT: u16 = 80;
/// Minimum terminal height in rows
const MIN_HEIGHT_ROWS: u16 = 3;

/// Integrated terminal panel
pub struct TerminalPanel {
    /// PTY connection to shell
    pty: Option<Pty>,
    /// Terminal screen buffer
    screen: TerminalScreen,
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
        // Content area is height - 1 (title bar takes one row)
        let content_height = height.saturating_sub(1).max(1);
        Self {
            pty: None,
            screen: TerminalScreen::new(screen_width, content_height),
            visible: false,
            height,
            screen_height,
            screen_width,
        }
    }

    /// Toggle terminal visibility
    pub fn toggle(&mut self) -> Result<()> {
        self.visible = !self.visible;

        // Spawn PTY on first show
        if self.visible && self.pty.is_none() {
            self.spawn()?;
        }

        Ok(())
    }

    /// Spawn the PTY process
    fn spawn(&mut self) -> Result<()> {
        // PTY gets content height (excluding title bar)
        let content_height = self.height.saturating_sub(1).max(1);
        let pty = Pty::spawn(self.screen_width, content_height)?;
        self.pty = Some(pty);
        Ok(())
    }

    /// Hide the terminal (ESC pressed)
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Send input to the terminal
    pub fn send_input(&mut self, data: &[u8]) -> Result<()> {
        if let Some(ref mut pty) = self.pty {
            pty.write(data)?;
        }
        Ok(())
    }

    /// Send a key to the terminal
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

    /// Poll for and process PTY output. Returns true if data was received.
    pub fn poll(&mut self) -> bool {
        let mut had_data = false;

        if let Some(ref mut pty) = self.pty {
            if let Some(data) = pty.read() {
                self.screen.process(&data);
                had_data = true;
            }
        }

        // Send any queued responses (e.g., device status reports) back to PTY
        let responses = self.screen.drain_responses();
        for response in responses {
            let _ = self.send_input(&response);
        }

        had_data
    }

    /// Get the terminal screen for rendering
    pub fn screen(&self) -> &TerminalScreen {
        &self.screen
    }

    /// Get a cell from the terminal screen
    pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.screen.cells().get(row).and_then(|r| r.get(col))
    }

    /// Get cursor position
    pub fn cursor_pos(&self) -> (u16, u16) {
        (self.screen.cursor_row, self.screen.cursor_col)
    }

    /// Update screen dimensions
    pub fn update_screen_size(&mut self, width: u16, height: u16) {
        self.screen_width = width;
        self.screen_height = height;

        // Recalculate terminal height (maintain percentage)
        let max_height = height * MAX_HEIGHT_PERCENT / 100;
        self.height = self.height.min(max_height).max(MIN_HEIGHT_ROWS);

        // Content height excludes title bar
        let content_height = self.height.saturating_sub(1).max(1);

        // Resize terminal screen
        self.screen.resize(width, content_height);

        // Resize PTY
        if let Some(ref pty) = self.pty {
            let _ = pty.resize(width, content_height);
        }
    }

    /// Resize terminal height
    pub fn resize_height(&mut self, new_height: u16) {
        let max_height = self.screen_height * MAX_HEIGHT_PERCENT / 100;
        self.height = new_height.min(max_height).max(MIN_HEIGHT_ROWS);

        // Content height excludes title bar
        let content_height = self.height.saturating_sub(1).max(1);

        self.screen.resize(self.screen_width, content_height);

        if let Some(ref pty) = self.pty {
            let _ = pty.resize(self.screen_width, content_height);
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
