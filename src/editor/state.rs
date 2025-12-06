use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::path::PathBuf;
use std::time::Duration;

use crate::buffer::Buffer;
use crate::input::{Key, Modifiers};
use crate::render::Screen;

use super::Cursor;

/// Main editor state
pub struct Editor {
    buffer: Buffer,
    cursor: Cursor,
    viewport_line: usize,
    screen: Screen,
    filename: Option<PathBuf>,
    running: bool,
}

impl Editor {
    pub fn new() -> Result<Self> {
        let mut screen = Screen::new()?;
        screen.enter_raw_mode()?;

        Ok(Self {
            buffer: Buffer::new(),
            cursor: Cursor::new(),
            viewport_line: 0,
            screen,
            filename: None,
            running: true,
        })
    }

    pub fn open(&mut self, path: &str) -> Result<()> {
        self.buffer = Buffer::load(path)?;
        self.filename = Some(PathBuf::from(path));
        self.cursor = Cursor::new();
        self.viewport_line = 0;
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        while self.running {
            self.screen.refresh_size()?;
            self.render()?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    self.handle_key(key_event)?;
                }
            }
        }

        self.screen.leave_raw_mode()?;
        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        self.screen.render(
            &self.buffer,
            &self.cursor,
            self.viewport_line,
            self.filename.as_ref().and_then(|p| p.to_str()),
        )
    }

    fn handle_key(&mut self, key_event: KeyEvent) -> Result<()> {
        let (key, mods) = Key::from_crossterm(key_event);

        match (&key, &mods) {
            // Quit: Ctrl+Q
            (Key::Char('q'), Modifiers { ctrl: true, .. }) => {
                self.running = false;
            }

            // Save: Ctrl+S
            (Key::Char('s'), Modifiers { ctrl: true, .. }) => {
                self.save()?;
            }

            // Movement
            (Key::Up, _) => self.move_up(),
            (Key::Down, _) => self.move_down(),
            (Key::Left, _) => self.move_left(),
            (Key::Right, _) => self.move_right(),
            (Key::Home, _) | (Key::Char('a'), Modifiers { ctrl: true, .. }) => self.move_home(),
            (Key::End, _) | (Key::Char('e'), Modifiers { ctrl: true, .. }) => self.move_end(),
            (Key::PageUp, _) => self.page_up(),
            (Key::PageDown, _) => self.page_down(),

            // Editing
            (Key::Char(c), Modifiers { ctrl: false, alt: false, .. }) => {
                self.insert_char(*c);
            }
            (Key::Enter, _) => self.insert_newline(),
            (Key::Backspace, _) | (Key::Char('h'), Modifiers { ctrl: true, .. }) => {
                self.delete_backward();
            }
            (Key::Delete, _) => self.delete_forward(),
            (Key::Tab, Modifiers { shift: false, .. }) => self.insert_tab(),

            _ => {}
        }

        self.scroll_to_cursor();
        Ok(())
    }

    // === Movement ===

    fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let line_len = self.buffer.line_len(self.cursor.line);
            self.cursor.col = self.cursor.desired_col.min(line_len);
        }
    }

    fn move_down(&mut self) {
        if self.cursor.line + 1 < self.buffer.line_count() {
            self.cursor.line += 1;
            let line_len = self.buffer.line_len(self.cursor.line);
            self.cursor.col = self.cursor.desired_col.min(line_len);
        }
    }

    fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
            self.cursor.desired_col = self.cursor.col;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.buffer.line_len(self.cursor.line);
            self.cursor.desired_col = self.cursor.col;
        }
    }

    fn move_right(&mut self) {
        let line_len = self.buffer.line_len(self.cursor.line);
        if self.cursor.col < line_len {
            self.cursor.col += 1;
            self.cursor.desired_col = self.cursor.col;
        } else if self.cursor.line + 1 < self.buffer.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
            self.cursor.desired_col = 0;
        }
    }

    fn move_home(&mut self) {
        self.cursor.col = 0;
        self.cursor.desired_col = 0;
    }

    fn move_end(&mut self) {
        self.cursor.col = self.buffer.line_len(self.cursor.line);
        self.cursor.desired_col = self.cursor.col;
    }

    fn page_up(&mut self) {
        let page = self.screen.rows.saturating_sub(2) as usize;
        self.cursor.line = self.cursor.line.saturating_sub(page);
        let line_len = self.buffer.line_len(self.cursor.line);
        self.cursor.col = self.cursor.desired_col.min(line_len);
    }

    fn page_down(&mut self) {
        let page = self.screen.rows.saturating_sub(2) as usize;
        self.cursor.line = (self.cursor.line + page).min(self.buffer.line_count().saturating_sub(1));
        let line_len = self.buffer.line_len(self.cursor.line);
        self.cursor.col = self.cursor.desired_col.min(line_len);
    }

    // === Editing ===

    fn insert_char(&mut self, c: char) {
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
        self.buffer.insert(idx, &c.to_string());
        self.cursor.col += 1;
        self.cursor.desired_col = self.cursor.col;
    }

    fn insert_newline(&mut self) {
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
        self.buffer.insert(idx, "\n");
        self.cursor.line += 1;
        self.cursor.col = 0;
        self.cursor.desired_col = 0;
    }

    fn insert_tab(&mut self) {
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
        self.buffer.insert(idx, "    ");
        self.cursor.col += 4;
        self.cursor.desired_col = self.cursor.col;
    }

    fn delete_backward(&mut self) {
        if self.cursor.col > 0 {
            let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
            self.buffer.delete(idx - 1, idx);
            self.cursor.col -= 1;
            self.cursor.desired_col = self.cursor.col;
        } else if self.cursor.line > 0 {
            // Join with previous line
            let prev_line_len = self.buffer.line_len(self.cursor.line - 1);
            let idx = self.buffer.line_col_to_char(self.cursor.line, 0);
            self.buffer.delete(idx - 1, idx); // Delete the newline
            self.cursor.line -= 1;
            self.cursor.col = prev_line_len;
            self.cursor.desired_col = self.cursor.col;
        }
    }

    fn delete_forward(&mut self) {
        let line_len = self.buffer.line_len(self.cursor.line);
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);

        if self.cursor.col < line_len {
            self.buffer.delete(idx, idx + 1);
        } else if self.cursor.line + 1 < self.buffer.line_count() {
            // Delete newline, joining with next line
            self.buffer.delete(idx, idx + 1);
        }
    }

    // === Viewport ===

    fn scroll_to_cursor(&mut self) {
        let visible_rows = self.screen.rows.saturating_sub(1) as usize;

        // Scroll up if cursor above viewport
        if self.cursor.line < self.viewport_line {
            self.viewport_line = self.cursor.line;
        }

        // Scroll down if cursor below viewport
        if self.cursor.line >= self.viewport_line + visible_rows {
            self.viewport_line = self.cursor.line - visible_rows + 1;
        }
    }

    // === File operations ===

    fn save(&mut self) -> Result<()> {
        if let Some(ref path) = self.filename {
            self.buffer.save(path)?;
        }
        Ok(())
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.screen.leave_raw_mode();
    }
}
