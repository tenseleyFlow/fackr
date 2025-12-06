use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::path::PathBuf;
use std::time::Duration;

use crate::buffer::Buffer;
use crate::input::{Key, Modifiers};
use crate::render::Screen;

use super::{Cursor, History, Operation, Position};

/// Main editor state
pub struct Editor {
    buffer: Buffer,
    cursor: Cursor,
    viewport_line: usize,
    screen: Screen,
    filename: Option<PathBuf>,
    running: bool,
    history: History,
    clipboard: String,
    /// Message to display in status bar
    message: Option<String>,
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
            history: History::new(),
            clipboard: String::new(),
            message: None,
        })
    }

    pub fn open(&mut self, path: &str) -> Result<()> {
        self.buffer = Buffer::load(path)?;
        self.filename = Some(PathBuf::from(path));
        self.cursor = Cursor::new();
        self.viewport_line = 0;
        self.history = History::new();
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        while self.running {
            self.screen.refresh_size()?;
            self.render()?;

            // Process all available events before rendering again
            // Use a short timeout to remain responsive
            if event::poll(Duration::from_millis(16))? {
                // Process all queued events
                loop {
                    if let Event::Key(key_event) = event::read()? {
                        self.handle_key(key_event)?;
                    }
                    // Check if more events are immediately available
                    if !event::poll(Duration::from_millis(0))? {
                        break;
                    }
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
            self.message.as_deref(),
        )
    }

    fn handle_key(&mut self, key_event: KeyEvent) -> Result<()> {
        let (key, mods) = Key::from_crossterm(key_event);

        // Clear message on any key
        self.message = None;

        match (&key, &mods) {
            // === System ===
            // Quit: Ctrl+Q
            (Key::Char('q'), Modifiers { ctrl: true, .. }) => {
                self.running = false;
            }
            // Save: Ctrl+S
            (Key::Char('s'), Modifiers { ctrl: true, .. }) => {
                self.save()?;
            }
            // Escape: clear selection
            (Key::Escape, _) => {
                self.cursor.clear_selection();
            }

            // === Undo/Redo ===
            (Key::Char('z'), Modifiers { ctrl: true, shift: false, .. }) => {
                self.undo();
            }
            (Key::Char('z'), Modifiers { ctrl: true, shift: true, .. })
            | (Key::Char(']'), Modifiers { ctrl: true, .. }) => {
                self.redo();
            }

            // === Clipboard ===
            (Key::Char('c'), Modifiers { ctrl: true, .. }) => {
                self.copy();
            }
            (Key::Char('x'), Modifiers { ctrl: true, .. }) => {
                self.cut();
            }
            (Key::Char('v'), Modifiers { ctrl: true, .. }) => {
                self.paste();
            }

            // === Line operations (must come before movement to capture Alt+arrows) ===
            // Move line up/down: Alt+Up/Down
            (Key::Up, Modifiers { alt: true, shift: false, .. }) => self.move_line_up(),
            (Key::Down, Modifiers { alt: true, shift: false, .. }) => self.move_line_down(),
            // Duplicate line: Alt+Shift+Up/Down
            (Key::Up, Modifiers { alt: true, shift: true, .. }) => self.duplicate_line_up(),
            (Key::Down, Modifiers { alt: true, shift: true, .. }) => self.duplicate_line_down(),

            // Word movement: Alt+Left/Right
            (Key::Left, Modifiers { alt: true, shift, .. }) => self.move_word_left(*shift),
            (Key::Right, Modifiers { alt: true, shift, .. }) => self.move_word_right(*shift),
            // Unix-style word movement: Alt+B (back), Alt+F (forward)
            (Key::Char('b'), Modifiers { alt: true, .. }) => self.move_word_left(false),
            (Key::Char('f'), Modifiers { alt: true, .. }) => self.move_word_right(false),

            // === Movement with selection ===
            (Key::Up, Modifiers { shift, .. }) => self.move_up(*shift),
            (Key::Down, Modifiers { shift, .. }) => self.move_down(*shift),
            (Key::Left, Modifiers { shift, .. }) => self.move_left(*shift),
            (Key::Right, Modifiers { shift, .. }) => self.move_right(*shift),

            // Home/End
            (Key::Home, Modifiers { shift, .. }) => self.move_home(*shift),
            (Key::End, Modifiers { shift, .. }) => self.move_end(*shift),
            (Key::Char('a'), Modifiers { ctrl: true, shift, .. }) => self.smart_home(*shift),
            (Key::Char('e'), Modifiers { ctrl: true, shift, .. }) => self.move_end(*shift),

            // Page movement
            (Key::PageUp, Modifiers { shift, .. }) => self.page_up(*shift),
            (Key::PageDown, Modifiers { shift, .. }) => self.page_down(*shift),

            // Join lines: Ctrl+J
            (Key::Char('j'), Modifiers { ctrl: true, .. }) => self.join_lines(),

            // === Editing ===
            (Key::Char(c), Modifiers { ctrl: false, alt: false, .. }) => {
                self.insert_char(*c);
            }
            (Key::Enter, _) => self.insert_newline(),
            (Key::Backspace, Modifiers { alt: true, .. }) => self.delete_word_backward(),
            (Key::Backspace, _) | (Key::Char('h'), Modifiers { ctrl: true, .. }) => {
                self.delete_backward();
            }
            (Key::Delete, _) => self.delete_forward(),
            (Key::Tab, Modifiers { shift: false, .. }) => self.insert_tab(),
            (Key::Tab, Modifiers { shift: true, .. }) => self.dedent(),

            // Delete word backward: Ctrl+W
            (Key::Char('w'), Modifiers { ctrl: true, .. }) => self.delete_word_backward(),
            // Delete word forward: Alt+D
            (Key::Char('d'), Modifiers { alt: true, .. }) => self.delete_word_forward(),

            _ => {}
        }

        self.scroll_to_cursor();
        Ok(())
    }

    // === Movement ===

    fn move_up(&mut self, extend_selection: bool) {
        if self.cursor.line > 0 {
            let new_line = self.cursor.line - 1;
            let line_len = self.buffer.line_len(new_line);
            let new_col = self.cursor.desired_col.min(line_len);
            self.cursor.move_to(new_line, new_col, extend_selection);
        }
    }

    fn move_down(&mut self, extend_selection: bool) {
        if self.cursor.line + 1 < self.buffer.line_count() {
            let new_line = self.cursor.line + 1;
            let line_len = self.buffer.line_len(new_line);
            let new_col = self.cursor.desired_col.min(line_len);
            self.cursor.move_to(new_line, new_col, extend_selection);
        }
    }

    fn move_left(&mut self, extend_selection: bool) {
        if self.cursor.col > 0 {
            self.cursor.move_to(self.cursor.line, self.cursor.col - 1, extend_selection);
            self.cursor.desired_col = self.cursor.col;
        } else if self.cursor.line > 0 {
            let new_line = self.cursor.line - 1;
            let new_col = self.buffer.line_len(new_line);
            self.cursor.move_to(new_line, new_col, extend_selection);
            self.cursor.desired_col = self.cursor.col;
        }
    }

    fn move_right(&mut self, extend_selection: bool) {
        let line_len = self.buffer.line_len(self.cursor.line);
        if self.cursor.col < line_len {
            self.cursor.move_to(self.cursor.line, self.cursor.col + 1, extend_selection);
            self.cursor.desired_col = self.cursor.col;
        } else if self.cursor.line + 1 < self.buffer.line_count() {
            self.cursor.move_to(self.cursor.line + 1, 0, extend_selection);
            self.cursor.desired_col = 0;
        }
    }

    fn move_word_left(&mut self, extend_selection: bool) {
        let (mut line, mut col) = (self.cursor.line, self.cursor.col);

        // If at start of line, go to end of previous line
        if col == 0 && line > 0 {
            line -= 1;
            col = self.buffer.line_len(line);
        }

        if let Some(line_str) = self.buffer.line_str(line) {
            let chars: Vec<char> = line_str.chars().collect();
            if col > 0 {
                col = col.min(chars.len());
                // Skip whitespace
                while col > 0 && chars.get(col - 1).map_or(false, |c| c.is_whitespace()) {
                    col -= 1;
                }
                // Skip word characters
                while col > 0 && chars.get(col - 1).map_or(false, |c| c.is_alphanumeric() || *c == '_') {
                    col -= 1;
                }
            }
        }

        self.cursor.move_to(line, col, extend_selection);
        self.cursor.desired_col = col;
    }

    fn move_word_right(&mut self, extend_selection: bool) {
        let (mut line, mut col) = (self.cursor.line, self.cursor.col);
        let line_len = self.buffer.line_len(line);

        // If at end of line, go to start of next line
        if col >= line_len && line + 1 < self.buffer.line_count() {
            line += 1;
            col = 0;
        }

        if let Some(line_str) = self.buffer.line_str(line) {
            let chars: Vec<char> = line_str.chars().collect();
            // Skip word characters
            while col < chars.len() && chars.get(col).map_or(false, |c| c.is_alphanumeric() || *c == '_') {
                col += 1;
            }
            // Skip whitespace
            while col < chars.len() && chars.get(col).map_or(false, |c| c.is_whitespace()) {
                col += 1;
            }
        }

        self.cursor.move_to(line, col, extend_selection);
        self.cursor.desired_col = col;
    }

    fn move_home(&mut self, extend_selection: bool) {
        self.cursor.move_to(self.cursor.line, 0, extend_selection);
        self.cursor.desired_col = 0;
    }

    fn smart_home(&mut self, extend_selection: bool) {
        // Toggle between column 0 and first non-whitespace
        if let Some(line_str) = self.buffer.line_str(self.cursor.line) {
            let first_non_ws = line_str.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
            let new_col = if self.cursor.col == first_non_ws || self.cursor.col == 0 {
                if self.cursor.col == 0 { first_non_ws } else { 0 }
            } else {
                first_non_ws
            };
            self.cursor.move_to(self.cursor.line, new_col, extend_selection);
            self.cursor.desired_col = new_col;
        }
    }

    fn move_end(&mut self, extend_selection: bool) {
        let line_len = self.buffer.line_len(self.cursor.line);
        self.cursor.move_to(self.cursor.line, line_len, extend_selection);
        self.cursor.desired_col = line_len;
    }

    fn page_up(&mut self, extend_selection: bool) {
        let page = self.screen.rows.saturating_sub(2) as usize;
        let new_line = self.cursor.line.saturating_sub(page);
        let line_len = self.buffer.line_len(new_line);
        let new_col = self.cursor.desired_col.min(line_len);
        self.cursor.move_to(new_line, new_col, extend_selection);
    }

    fn page_down(&mut self, extend_selection: bool) {
        let page = self.screen.rows.saturating_sub(2) as usize;
        let new_line = (self.cursor.line + page).min(self.buffer.line_count().saturating_sub(1));
        let line_len = self.buffer.line_len(new_line);
        let new_col = self.cursor.desired_col.min(line_len);
        self.cursor.move_to(new_line, new_col, extend_selection);
    }

    // === Editing ===

    fn cursor_pos(&self) -> Position {
        Position::new(self.cursor.line, self.cursor.col)
    }

    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.cursor.selection_bounds() {
            let start_idx = self.buffer.line_col_to_char(start.line, start.col);
            let end_idx = self.buffer.line_col_to_char(end.line, end.col);

            // Record for undo
            let deleted_text: String = self.buffer.slice(start_idx, end_idx).chars().collect();
            let cursor_before = self.cursor_pos();

            self.buffer.delete(start_idx, end_idx);

            self.cursor.line = start.line;
            self.cursor.col = start.col;
            self.cursor.desired_col = start.col;
            self.cursor.clear_selection();

            let cursor_after = self.cursor_pos();
            self.history.record_delete(start_idx, deleted_text, cursor_before, cursor_after);
            self.history.maybe_break_group();

            true
        } else {
            false
        }
    }

    fn insert_text(&mut self, text: &str) {
        self.delete_selection();

        let cursor_before = self.cursor_pos();
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);

        self.buffer.insert(idx, text);
        self.history.record_insert(idx, text.to_string(), cursor_before, Position::new(0, 0));

        // Update cursor position
        for c in text.chars() {
            if c == '\n' {
                self.cursor.line += 1;
                self.cursor.col = 0;
            } else {
                self.cursor.col += 1;
            }
        }
        self.cursor.desired_col = self.cursor.col;

        // Update the cursor_after in history
        let cursor_after = self.cursor_pos();
        if let Some(op) = self.history.undo_stack_last_mut() {
            if let Operation::Insert { cursor_after: ref mut ca, .. } = op {
                *ca = cursor_after;
            }
        }
    }

    fn insert_char(&mut self, c: char) {
        self.insert_text(&c.to_string());
    }

    fn insert_newline(&mut self) {
        self.history.maybe_break_group();
        self.insert_text("\n");
        self.history.maybe_break_group();
    }

    fn insert_tab(&mut self) {
        self.insert_text("    ");
    }

    fn delete_backward(&mut self) {
        if self.delete_selection() {
            return;
        }

        if self.cursor.col > 0 {
            let cursor_before = self.cursor_pos();
            let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
            let deleted = self.buffer.char_at(idx - 1).map(|c| c.to_string()).unwrap_or_default();

            self.buffer.delete(idx - 1, idx);
            self.cursor.col -= 1;
            self.cursor.desired_col = self.cursor.col;

            let cursor_after = self.cursor_pos();
            self.history.record_delete(idx - 1, deleted, cursor_before, cursor_after);
        } else if self.cursor.line > 0 {
            let cursor_before = self.cursor_pos();
            let prev_line_len = self.buffer.line_len(self.cursor.line - 1);
            let idx = self.buffer.line_col_to_char(self.cursor.line, 0);

            self.buffer.delete(idx - 1, idx);
            self.cursor.line -= 1;
            self.cursor.col = prev_line_len;
            self.cursor.desired_col = self.cursor.col;

            let cursor_after = self.cursor_pos();
            self.history.record_delete(idx - 1, "\n".to_string(), cursor_before, cursor_after);
            self.history.maybe_break_group();
        }
    }

    fn delete_forward(&mut self) {
        if self.delete_selection() {
            return;
        }

        let line_len = self.buffer.line_len(self.cursor.line);
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);

        if self.cursor.col < line_len {
            let cursor_before = self.cursor_pos();
            let deleted = self.buffer.char_at(idx).map(|c| c.to_string()).unwrap_or_default();
            self.buffer.delete(idx, idx + 1);
            let cursor_after = self.cursor_pos();
            self.history.record_delete(idx, deleted, cursor_before, cursor_after);
        } else if self.cursor.line + 1 < self.buffer.line_count() {
            let cursor_before = self.cursor_pos();
            self.buffer.delete(idx, idx + 1);
            let cursor_after = self.cursor_pos();
            self.history.record_delete(idx, "\n".to_string(), cursor_before, cursor_after);
            self.history.maybe_break_group();
        }
    }

    fn delete_word_backward(&mut self) {
        if self.delete_selection() {
            return;
        }

        let start_col = self.cursor.col;
        self.move_word_left(false);

        if self.cursor.line == self.cursor.line && self.cursor.col < start_col {
            let cursor_before = Position::new(self.cursor.line, start_col);
            let start_idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
            let end_idx = self.buffer.line_col_to_char(self.cursor.line, start_col);
            let deleted: String = self.buffer.slice(start_idx, end_idx).chars().collect();

            self.buffer.delete(start_idx, end_idx);
            let cursor_after = self.cursor_pos();
            self.history.record_delete(start_idx, deleted, cursor_before, cursor_after);
            self.history.maybe_break_group();
        }
    }

    fn delete_word_forward(&mut self) {
        if self.delete_selection() {
            return;
        }

        let start_line = self.cursor.line;
        let start_col = self.cursor.col;
        self.move_word_right(false);

        let cursor_before = Position::new(start_line, start_col);
        let start_idx = self.buffer.line_col_to_char(start_line, start_col);
        let end_idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);

        if end_idx > start_idx {
            let deleted: String = self.buffer.slice(start_idx, end_idx).chars().collect();
            self.buffer.delete(start_idx, end_idx);
            self.cursor.line = start_line;
            self.cursor.col = start_col;
            let cursor_after = self.cursor_pos();
            self.history.record_delete(start_idx, deleted, cursor_before, cursor_after);
            self.history.maybe_break_group();
        }
    }

    fn dedent(&mut self) {
        if let Some(line_str) = self.buffer.line_str(self.cursor.line) {
            let spaces_to_remove = line_str.chars().take(4).take_while(|c| *c == ' ').count();
            if spaces_to_remove > 0 {
                let cursor_before = self.cursor_pos();
                let line_start = self.buffer.line_col_to_char(self.cursor.line, 0);
                let deleted: String = " ".repeat(spaces_to_remove);

                self.buffer.delete(line_start, line_start + spaces_to_remove);
                self.cursor.col = self.cursor.col.saturating_sub(spaces_to_remove);
                self.cursor.desired_col = self.cursor.col;

                let cursor_after = self.cursor_pos();
                self.history.record_delete(line_start, deleted, cursor_before, cursor_after);
                self.history.maybe_break_group();
            }
        }
    }

    // === Line operations ===

    fn move_line_up(&mut self) {
        if self.cursor.line > 0 {
            self.history.begin_group();

            let curr_line = self.cursor.line;
            let prev_line = curr_line - 1;

            let curr_content = self.buffer.line_str(curr_line).unwrap_or_default();
            let _prev_content = self.buffer.line_str(prev_line).unwrap_or_default();

            // Delete current line (including newline)
            let curr_start = self.buffer.line_col_to_char(curr_line, 0);
            let curr_end = curr_start + curr_content.len() + 1; // +1 for newline
            self.buffer.delete(curr_start.saturating_sub(1), curr_end.saturating_sub(1));

            // Insert current line before previous line
            let prev_start = self.buffer.line_col_to_char(prev_line, 0);
            self.buffer.insert(prev_start, &format!("{}\n", curr_content));

            self.cursor.line = prev_line;
            self.history.end_group();
        }
    }

    fn move_line_down(&mut self) {
        if self.cursor.line + 1 < self.buffer.line_count() {
            self.history.begin_group();

            let curr_line = self.cursor.line;
            let next_line = curr_line + 1;

            let curr_content = self.buffer.line_str(curr_line).unwrap_or_default();
            let _next_content = self.buffer.line_str(next_line).unwrap_or_default();

            // Delete current line (including newline before next)
            let curr_start = self.buffer.line_col_to_char(curr_line, 0);
            let next_start = self.buffer.line_col_to_char(next_line, 0);
            self.buffer.delete(curr_start, next_start);

            // Insert current line after next line
            let new_next_end = self.buffer.line_col_to_char(curr_line, self.buffer.line_len(curr_line));
            self.buffer.insert(new_next_end, &format!("\n{}", curr_content));

            self.cursor.line = next_line;
            self.history.end_group();
        }
    }

    fn duplicate_line_up(&mut self) {
        self.history.begin_group();
        let content = self.buffer.line_str(self.cursor.line).unwrap_or_default();
        let line_start = self.buffer.line_col_to_char(self.cursor.line, 0);
        self.buffer.insert(line_start, &format!("{}\n", content));
        self.history.end_group();
    }

    fn duplicate_line_down(&mut self) {
        self.history.begin_group();
        let content = self.buffer.line_str(self.cursor.line).unwrap_or_default();
        let line_end = self.buffer.line_col_to_char(self.cursor.line, self.buffer.line_len(self.cursor.line));
        self.buffer.insert(line_end, &format!("\n{}", content));
        self.cursor.line += 1;
        self.history.end_group();
    }

    fn join_lines(&mut self) {
        if self.cursor.line + 1 < self.buffer.line_count() {
            self.history.begin_group();

            let line_len = self.buffer.line_len(self.cursor.line);
            let idx = self.buffer.line_col_to_char(self.cursor.line, line_len);

            // Delete newline
            self.buffer.delete(idx, idx + 1);

            // Move cursor to join point
            self.cursor.col = line_len;
            self.cursor.desired_col = self.cursor.col;

            self.history.end_group();
        }
    }

    // === Clipboard ===

    fn get_selection_text(&self) -> Option<String> {
        self.cursor.selection_bounds().map(|(start, end)| {
            let start_idx = self.buffer.line_col_to_char(start.line, start.col);
            let end_idx = self.buffer.line_col_to_char(end.line, end.col);
            self.buffer.slice(start_idx, end_idx).chars().collect()
        })
    }

    fn copy(&mut self) {
        if let Some(text) = self.get_selection_text() {
            self.clipboard = text;
            self.message = Some("Copied".to_string());
        } else {
            // Copy current line
            if let Some(line) = self.buffer.line_str(self.cursor.line) {
                self.clipboard = format!("{}\n", line);
                self.message = Some("Copied line".to_string());
            }
        }
    }

    fn cut(&mut self) {
        if let Some(text) = self.get_selection_text() {
            self.clipboard = text;
            self.delete_selection();
            self.message = Some("Cut".to_string());
        } else {
            // Cut current line
            if let Some(line) = self.buffer.line_str(self.cursor.line) {
                self.clipboard = format!("{}\n", line);

                let line_start = self.buffer.line_col_to_char(self.cursor.line, 0);
                let line_end = line_start + line.len() + 1; // +1 for newline

                if self.cursor.line + 1 < self.buffer.line_count() {
                    self.buffer.delete(line_start, line_end);
                } else if self.cursor.line > 0 {
                    // Last line - delete newline before it too
                    self.buffer.delete(line_start.saturating_sub(1), line_start + line.len());
                    self.cursor.line -= 1;
                } else {
                    // Only line - just clear it
                    self.buffer.delete(line_start, line_start + line.len());
                }

                self.cursor.col = 0;
                self.cursor.desired_col = 0;
                self.message = Some("Cut line".to_string());
            }
        }
        self.history.maybe_break_group();
    }

    fn paste(&mut self) {
        if !self.clipboard.is_empty() {
            self.insert_text(&self.clipboard.clone());
            self.message = Some("Pasted".to_string());
            self.history.maybe_break_group();
        }
    }

    // === Undo/Redo ===

    fn undo(&mut self) {
        if let Some((ops, cursor_pos)) = self.history.undo() {
            // Apply operations in reverse
            for op in ops.into_iter().rev() {
                match op {
                    Operation::Insert { pos, text, .. } => {
                        self.buffer.delete(pos, pos + text.chars().count());
                    }
                    Operation::Delete { pos, text, .. } => {
                        self.buffer.insert(pos, &text);
                    }
                }
            }
            self.cursor.line = cursor_pos.line;
            self.cursor.col = cursor_pos.col;
            self.cursor.desired_col = cursor_pos.col;
            self.cursor.clear_selection();
            self.message = Some("Undo".to_string());
        }
    }

    fn redo(&mut self) {
        if let Some((ops, cursor_pos)) = self.history.redo() {
            // Apply operations forward
            for op in ops {
                match op {
                    Operation::Insert { pos, text, .. } => {
                        self.buffer.insert(pos, &text);
                    }
                    Operation::Delete { pos, text, .. } => {
                        self.buffer.delete(pos, pos + text.chars().count());
                    }
                }
            }
            self.cursor.line = cursor_pos.line;
            self.cursor.col = cursor_pos.col;
            self.cursor.desired_col = cursor_pos.col;
            self.cursor.clear_selection();
            self.message = Some("Redo".to_string());
        }
    }

    // === Viewport ===

    fn scroll_to_cursor(&mut self) {
        let visible_rows = self.screen.rows.saturating_sub(1) as usize;

        if self.cursor.line < self.viewport_line {
            self.viewport_line = self.cursor.line;
        }

        if self.cursor.line >= self.viewport_line + visible_rows {
            self.viewport_line = self.cursor.line - visible_rows + 1;
        }
    }

    // === File operations ===

    fn save(&mut self) -> Result<()> {
        if let Some(ref path) = self.filename {
            self.buffer.save(path)?;
            self.message = Some("Saved".to_string());
        }
        Ok(())
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.screen.leave_raw_mode();
    }
}
