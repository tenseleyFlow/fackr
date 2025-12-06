use anyhow::Result;
use arboard::Clipboard;
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
    clipboard: Option<Clipboard>,
    /// Fallback internal clipboard if system clipboard unavailable
    internal_clipboard: String,
    /// Message to display in status bar
    message: Option<String>,
    /// Escape key timeout in milliseconds (for Alt key detection)
    escape_time: u64,
}

impl Editor {
    pub fn new() -> Result<Self> {
        let mut screen = Screen::new()?;
        screen.enter_raw_mode()?;

        // Read escape timeout from environment, default to 5ms
        // Similar to vim's ttimeoutlen or tmux's escape-time
        let escape_time = std::env::var("FAC_ESCAPE_TIME")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        // Try to initialize system clipboard, fall back to internal if unavailable
        let clipboard = Clipboard::new().ok();

        Ok(Self {
            buffer: Buffer::new(),
            cursor: Cursor::new(),
            viewport_line: 0,
            screen,
            filename: None,
            running: true,
            history: History::new(),
            clipboard,
            internal_clipboard: String::new(),
            message: None,
            escape_time,
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
        // Initial render
        self.screen.refresh_size()?;
        self.render()?;

        while self.running {
            // Block until an event is available (no busy polling)
            match event::read()? {
                Event::Key(key_event) => self.process_key(key_event)?,
                Event::Resize(cols, rows) => {
                    self.screen.cols = cols;
                    self.screen.rows = rows;
                }
                _ => {}
            }

            // Process any additional queued events before rendering
            while event::poll(Duration::from_millis(0))? {
                match event::read()? {
                    Event::Key(key_event) => self.process_key(key_event)?,
                    Event::Resize(cols, rows) => {
                        self.screen.cols = cols;
                        self.screen.rows = rows;
                    }
                    _ => {}
                }
            }

            // Only render after processing events
            self.screen.refresh_size()?;
            self.render()?;
        }

        self.screen.leave_raw_mode()?;
        Ok(())
    }

    /// Process a key event, handling ESC as potential Alt prefix
    fn process_key(&mut self, key_event: KeyEvent) -> Result<()> {
        use crossterm::event::KeyCode;

        // Check if this is a bare Escape key (potential Alt prefix)
        if key_event.code == KeyCode::Esc && key_event.modifiers.is_empty() {
            // Check if more data is available within escape_time
            // Escape sequences from terminals arrive together, so short timeouts work
            let timeout = Duration::from_millis(self.escape_time);

            if event::poll(timeout)? {
                if let Event::Key(next_event) = event::read()? {
                    // Check for CSI sequences (ESC [ ...) which are arrow keys etc.
                    if next_event.code == KeyCode::Char('[') {
                        // CSI sequence - read the rest
                        if event::poll(timeout)? {
                            if let Event::Key(csi_event) = event::read()? {
                                let mods = Modifiers { alt: true, ..Default::default() };
                                return match csi_event.code {
                                    KeyCode::Char('A') => self.handle_key_with_mods(Key::Up, mods),
                                    KeyCode::Char('B') => self.handle_key_with_mods(Key::Down, mods),
                                    KeyCode::Char('C') => self.handle_key_with_mods(Key::Right, mods),
                                    KeyCode::Char('D') => self.handle_key_with_mods(Key::Left, mods),
                                    _ => Ok(()), // Unknown CSI sequence
                                };
                            }
                        }
                        return Ok(()); // Incomplete CSI
                    }

                    // Regular Alt+key (ESC followed by a normal key)
                    let (key, mut mods) = Key::from_crossterm(next_event);
                    mods.alt = true;
                    return self.handle_key_with_mods(key, mods);
                }
            }
            // No key followed - it's a real Escape
            return self.handle_key_with_mods(Key::Escape, Modifiers::default());
        }

        // Normal key processing
        let (key, mods) = Key::from_crossterm(key_event);
        self.handle_key_with_mods(key, mods)
    }

    fn render(&mut self) -> Result<()> {
        // Find matching bracket for cursor position
        let bracket_match = self.buffer.find_matching_bracket(self.cursor.line, self.cursor.col);

        self.screen.render(
            &self.buffer,
            &self.cursor,
            self.viewport_line,
            self.filename.as_ref().and_then(|p| p.to_str()),
            self.message.as_deref(),
            bracket_match,
        )
    }

    fn handle_key_with_mods(&mut self, key: Key, mods: Modifiers) -> Result<()> {
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

            // Select line: Ctrl+L
            (Key::Char('l'), Modifiers { ctrl: true, .. }) => self.select_line(),
            // Select word: Ctrl+D (select word at cursor, or next occurrence if already selected)
            (Key::Char('d'), Modifiers { ctrl: true, .. }) => self.select_word(),

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
            (Key::Tab, _) => self.insert_tab(),
            (Key::BackTab, _) => self.dedent(),

            // Delete word backward: Ctrl+W
            (Key::Char('w'), Modifiers { ctrl: true, .. }) => self.delete_word_backward(),
            // Delete word forward: Alt+D
            (Key::Char('d'), Modifiers { alt: true, .. }) => self.delete_word_forward(),

            // Character transpose: Ctrl+T
            (Key::Char('t'), Modifiers { ctrl: true, .. }) => self.transpose_chars(),

            // === Bracket/Quote operations ===
            // Jump to matching bracket: Alt+[ or Alt+]
            (Key::Char('['), Modifiers { alt: true, .. }) |
            (Key::Char(']'), Modifiers { alt: true, .. }) => self.jump_to_matching_bracket(),
            // Cycle quotes: Alt+' (cycles " -> ' -> ` -> ")
            (Key::Char('\''), Modifiers { alt: true, shift: false, .. }) => self.cycle_quotes(),
            // Remove surrounding quotes/brackets: Alt+Shift+' (Alt+")
            (Key::Char('"'), Modifiers { alt: true, .. }) => self.remove_surrounding(),
            // Cycle bracket type: Alt+Shift+9 (cycles ( -> { -> [ -> ()
            (Key::Char('('), Modifiers { alt: true, .. }) => self.cycle_brackets(),
            // Remove surrounding brackets: Alt+Shift+0
            (Key::Char(')'), Modifiers { alt: true, .. }) => self.remove_surrounding_brackets(),

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
                // Determine what kind of characters to skip based on char before cursor
                if col > 0 {
                    let prev_char = chars[col - 1];
                    if is_word_char(prev_char) {
                        // Skip word characters
                        while col > 0 && chars.get(col - 1).map_or(false, |c| is_word_char(*c)) {
                            col -= 1;
                        }
                    } else {
                        // Skip punctuation/symbols
                        while col > 0 && chars.get(col - 1).map_or(false, |c| !is_word_char(*c) && !c.is_whitespace()) {
                            col -= 1;
                        }
                    }
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
            if col < chars.len() {
                let curr_char = chars[col];
                if is_word_char(curr_char) {
                    // Skip word characters
                    while col < chars.len() && chars.get(col).map_or(false, |c| is_word_char(*c)) {
                        col += 1;
                    }
                } else if !curr_char.is_whitespace() {
                    // Skip punctuation/symbols
                    while col < chars.len() && chars.get(col).map_or(false, |c| !is_word_char(*c) && !c.is_whitespace()) {
                        col += 1;
                    }
                }
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

    // === Selection ===

    fn select_line(&mut self) {
        // Select the entire current line (including newline if not last line)
        let line_len = self.buffer.line_len(self.cursor.line);
        self.cursor.anchor_line = self.cursor.line;
        self.cursor.anchor_col = 0;
        self.cursor.col = line_len;
        self.cursor.desired_col = line_len;
        self.cursor.selecting = true;
    }

    fn select_word(&mut self) {
        // If no selection, select word at cursor
        // If already have selection, this could expand to next occurrence (future enhancement)
        if let Some(line_str) = self.buffer.line_str(self.cursor.line) {
            let chars: Vec<char> = line_str.chars().collect();
            let col = self.cursor.col.min(chars.len());

            // Find word boundaries
            let mut start = col;
            let mut end = col;

            // If cursor is on a word char, expand to word boundaries
            if col < chars.len() && is_word_char(chars[col]) {
                // Expand left
                while start > 0 && is_word_char(chars[start - 1]) {
                    start -= 1;
                }
                // Expand right
                while end < chars.len() && is_word_char(chars[end]) {
                    end += 1;
                }
            } else if col > 0 && is_word_char(chars[col - 1]) {
                // Cursor is just after a word
                end = col;
                start = col - 1;
                while start > 0 && is_word_char(chars[start - 1]) {
                    start -= 1;
                }
            }

            if start < end {
                self.cursor.anchor_line = self.cursor.line;
                self.cursor.anchor_col = start;
                self.cursor.col = end;
                self.cursor.desired_col = end;
                self.cursor.selecting = true;
            }
        }
    }

    // === Bracket/Quote Operations ===

    fn jump_to_matching_bracket(&mut self) {
        // First check if cursor is on a bracket
        if let Some((line, col)) = self.buffer.find_matching_bracket(self.cursor.line, self.cursor.col) {
            self.cursor.clear_selection();
            self.cursor.line = line;
            self.cursor.col = col;
            self.cursor.desired_col = col;
            return;
        }

        // If not on a bracket, find surrounding brackets and jump to opening
        if let Some((open_idx, close_idx, _, _)) = self.buffer.find_surrounding_brackets(self.cursor.line, self.cursor.col) {
            let cursor_idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
            // Jump to whichever bracket we're not at
            let (target_line, target_col) = if cursor_idx == open_idx + 1 {
                self.buffer.char_to_line_col(close_idx)
            } else {
                self.buffer.char_to_line_col(open_idx)
            };
            self.cursor.clear_selection();
            self.cursor.line = target_line;
            self.cursor.col = target_col;
            self.cursor.desired_col = target_col;
        }
    }

    fn cycle_quotes(&mut self) {
        // Find surrounding quotes (across lines) and cycle: " -> ' -> ` -> "
        if let Some((open_idx, close_idx, quote_char)) = self.buffer.find_surrounding_quotes(self.cursor.line, self.cursor.col) {
            let new_quote = match quote_char {
                '"' => '\'',
                '\'' => '`',
                '`' => '"',
                _ => return,
            };

            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            // Replace closing quote first (to maintain positions)
            self.buffer.delete(close_idx, close_idx + 1);
            self.buffer.insert(close_idx, &new_quote.to_string());
            self.history.record_delete(close_idx, quote_char.to_string(), cursor_before, cursor_before);
            self.history.record_insert(close_idx, new_quote.to_string(), cursor_before, cursor_before);

            // Replace opening quote
            self.buffer.delete(open_idx, open_idx + 1);
            self.buffer.insert(open_idx, &new_quote.to_string());
            self.history.record_delete(open_idx, quote_char.to_string(), cursor_before, cursor_before);
            self.history.record_insert(open_idx, new_quote.to_string(), cursor_before, cursor_before);

            self.history.end_group();
        }
    }

    fn cycle_brackets(&mut self) {
        // Find surrounding brackets (across lines) and cycle: ( -> { -> [ -> (
        if let Some((open_idx, close_idx, open, close)) = self.buffer.find_surrounding_brackets(self.cursor.line, self.cursor.col) {
            let (new_open, new_close) = match open {
                '(' => ('{', '}'),
                '{' => ('[', ']'),
                '[' => ('(', ')'),
                _ => return,
            };

            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            // Replace closing bracket first
            self.buffer.delete(close_idx, close_idx + 1);
            self.buffer.insert(close_idx, &new_close.to_string());
            self.history.record_delete(close_idx, close.to_string(), cursor_before, cursor_before);
            self.history.record_insert(close_idx, new_close.to_string(), cursor_before, cursor_before);

            // Replace opening bracket
            self.buffer.delete(open_idx, open_idx + 1);
            self.buffer.insert(open_idx, &new_open.to_string());
            self.history.record_delete(open_idx, open.to_string(), cursor_before, cursor_before);
            self.history.record_insert(open_idx, new_open.to_string(), cursor_before, cursor_before);

            self.history.end_group();
        }
    }

    fn remove_surrounding(&mut self) {
        // Remove surrounding quotes OR brackets (whichever is innermost/closest)
        let cursor_idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);

        // Find both surrounding quotes and brackets
        let quotes = self.buffer.find_surrounding_quotes(self.cursor.line, self.cursor.col);
        let brackets = self.buffer.find_surrounding_brackets(self.cursor.line, self.cursor.col);

        // Pick whichever has the closer opening (innermost)
        let (open_idx, close_idx, open_char, close_char) = match (quotes, brackets) {
            (Some((qo, qc, qch)), Some((bo, bc, bop, bcl))) => {
                if qo > bo { (qo, qc, qch, qch) } else { (bo, bc, bop, bcl) }
            }
            (Some((qo, qc, qch)), None) => (qo, qc, qch, qch),
            (None, Some((bo, bc, bop, bcl))) => (bo, bc, bop, bcl),
            (None, None) => return,
        };

        let cursor_before = self.cursor_pos();
        self.history.begin_group();

        // Delete closing first (to maintain open position)
        self.buffer.delete(close_idx, close_idx + 1);
        self.history.record_delete(close_idx, close_char.to_string(), cursor_before, cursor_before);

        // Delete opening
        self.buffer.delete(open_idx, open_idx + 1);
        self.history.record_delete(open_idx, open_char.to_string(), cursor_before, cursor_before);

        // Adjust cursor position
        if cursor_idx > open_idx {
            self.cursor.col = self.cursor.col.saturating_sub(1);
        }
        // Recalculate line/col after deletions
        let new_cursor_idx = if cursor_idx > close_idx {
            cursor_idx - 2
        } else if cursor_idx > open_idx {
            cursor_idx - 1
        } else {
            cursor_idx
        };
        let (new_line, new_col) = self.buffer.char_to_line_col(new_cursor_idx.min(self.buffer.len_chars().saturating_sub(1)));
        self.cursor.line = new_line;
        self.cursor.col = new_col;
        self.cursor.desired_col = new_col;

        self.history.end_group();
    }

    fn remove_surrounding_brackets(&mut self) {
        // Remove only surrounding brackets (not quotes)
        if let Some((open_idx, close_idx, open, close)) = self.buffer.find_surrounding_brackets(self.cursor.line, self.cursor.col) {
            let cursor_idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            // Delete closing first
            self.buffer.delete(close_idx, close_idx + 1);
            self.history.record_delete(close_idx, close.to_string(), cursor_before, cursor_before);

            // Delete opening
            self.buffer.delete(open_idx, open_idx + 1);
            self.history.record_delete(open_idx, open.to_string(), cursor_before, cursor_before);

            // Recalculate cursor position after deletions
            let new_cursor_idx = if cursor_idx > close_idx {
                cursor_idx - 2
            } else if cursor_idx > open_idx {
                cursor_idx - 1
            } else {
                cursor_idx
            };
            let (new_line, new_col) = self.buffer.char_to_line_col(new_cursor_idx.min(self.buffer.len_chars().saturating_sub(1)));
            self.cursor.line = new_line;
            self.cursor.col = new_col;
            self.cursor.desired_col = new_col;

            self.history.end_group();
        }
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
        // Check for auto-pair closing: if typing a closing bracket/quote
        // and the next char is the same, just move cursor right
        if let Some(next_char) = self.char_at_cursor() {
            if c == next_char && (c == ')' || c == ']' || c == '}' || c == '"' || c == '\'' || c == '`') {
                self.cursor.col += 1;
                self.cursor.desired_col = self.cursor.col;
                return;
            }
        }

        // Check for auto-pair opening: insert pair and place cursor between
        let pair = match c {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        };

        if let Some(close) = pair {
            // For quotes, only auto-pair if not inside a word
            let should_pair = if c == '"' || c == '\'' || c == '`' {
                // Don't auto-pair if previous char is alphanumeric (e.g., typing apostrophe in "don't")
                let prev_char = if self.cursor.col > 0 {
                    let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
                    self.buffer.char_at(idx.saturating_sub(1))
                } else {
                    None
                };
                !prev_char.map_or(false, |ch| ch.is_alphanumeric())
            } else {
                true
            };

            if should_pair {
                self.delete_selection();
                let cursor_before = self.cursor_pos();
                let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
                let pair_str = format!("{}{}", c, close);

                self.buffer.insert(idx, &pair_str);
                self.cursor.col += 1; // Position cursor between the pair
                self.cursor.desired_col = self.cursor.col;

                let cursor_after = self.cursor_pos();
                self.history.record_insert(idx, pair_str, cursor_before, cursor_after);
                return;
            }
        }

        self.insert_text(&c.to_string());
    }

    /// Get character at cursor position (if any)
    fn char_at_cursor(&self) -> Option<char> {
        let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
        self.buffer.char_at(idx)
    }

    fn insert_newline(&mut self) {
        self.history.maybe_break_group();
        self.insert_text("\n");
        self.history.maybe_break_group();
    }

    fn insert_tab(&mut self) {
        if self.cursor.has_selection() {
            self.indent_selection();
        } else {
            self.insert_text("    ");
        }
    }

    /// Indent all lines in selection
    fn indent_selection(&mut self) {
        if let Some((start, end)) = self.cursor.selection_bounds() {
            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            // Indent each line from start to end (inclusive)
            for line_idx in start.line..=end.line {
                let line_start = self.buffer.line_col_to_char(line_idx, 0);
                let indent = "    ";
                self.buffer.insert(line_start, indent);
                self.history.record_insert(line_start, indent.to_string(), cursor_before, cursor_before);
            }

            // Adjust selection to cover the indented text
            self.cursor.anchor_col += 4;
            self.cursor.col += 4;
            self.cursor.desired_col = self.cursor.col;

            self.history.end_group();
        }
    }

    fn delete_backward(&mut self) {
        if self.delete_selection() {
            return;
        }

        if self.cursor.col > 0 {
            let cursor_before = self.cursor_pos();
            let idx = self.buffer.line_col_to_char(self.cursor.line, self.cursor.col);
            let prev_char = self.buffer.char_at(idx - 1);
            let next_char = self.buffer.char_at(idx);

            // Check for auto-pair deletion: if deleting opening bracket/quote
            // and next char is the matching close, delete both
            let is_pair = match (prev_char, next_char) {
                (Some('('), Some(')')) => true,
                (Some('['), Some(']')) => true,
                (Some('{'), Some('}')) => true,
                (Some('"'), Some('"')) => true,
                (Some('\''), Some('\'')) => true,
                (Some('`'), Some('`')) => true,
                _ => false,
            };

            if is_pair {
                // Delete both characters
                let deleted = format!("{}{}", prev_char.unwrap(), next_char.unwrap());
                self.buffer.delete(idx - 1, idx + 1);
                self.cursor.col -= 1;
                self.cursor.desired_col = self.cursor.col;

                let cursor_after = self.cursor_pos();
                self.history.record_delete(idx - 1, deleted, cursor_before, cursor_after);
            } else {
                let deleted = prev_char.map(|c| c.to_string()).unwrap_or_default();

                self.buffer.delete(idx - 1, idx);
                self.cursor.col -= 1;
                self.cursor.desired_col = self.cursor.col;

                let cursor_after = self.cursor_pos();
                self.history.record_delete(idx - 1, deleted, cursor_before, cursor_after);
            }
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

    fn transpose_chars(&mut self) {
        // Transpose the two characters around the cursor
        // If at end of line, swap the two chars before cursor
        // If at start of line, do nothing
        let line_len = self.buffer.line_len(self.cursor.line);
        if line_len < 2 {
            return;
        }

        let (swap_pos, move_cursor) = if self.cursor.col == 0 {
            // At start of line - nothing to transpose
            return;
        } else if self.cursor.col >= line_len {
            // At or past end of line - swap last two chars
            (self.cursor.col - 2, false)
        } else {
            // In middle - swap char before cursor with char at cursor
            (self.cursor.col - 1, true)
        };

        let idx = self.buffer.line_col_to_char(self.cursor.line, swap_pos);
        let char1 = self.buffer.char_at(idx);
        let char2 = self.buffer.char_at(idx + 1);

        if let (Some(c1), Some(c2)) = (char1, char2) {
            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            // Delete both chars
            let deleted = format!("{}{}", c1, c2);
            self.buffer.delete(idx, idx + 2);
            self.history.record_delete(idx, deleted, cursor_before, cursor_before);

            // Insert in swapped order
            let swapped = format!("{}{}", c2, c1);
            self.buffer.insert(idx, &swapped);

            if move_cursor {
                self.cursor.col += 1;
                self.cursor.desired_col = self.cursor.col;
            }

            let cursor_after = self.cursor_pos();
            self.history.record_insert(idx, swapped, cursor_before, cursor_after);
            self.history.end_group();
        }
    }

    fn dedent(&mut self) {
        if self.cursor.has_selection() {
            self.dedent_selection();
        } else {
            self.dedent_line(self.cursor.line);
            self.history.maybe_break_group();
        }
    }

    /// Dedent a single line, returns number of spaces removed
    fn dedent_line(&mut self, line_idx: usize) -> usize {
        if let Some(line_str) = self.buffer.line_str(line_idx) {
            let spaces_to_remove = line_str.chars().take(4).take_while(|c| *c == ' ').count();
            if spaces_to_remove > 0 {
                let cursor_before = self.cursor_pos();
                let line_start = self.buffer.line_col_to_char(line_idx, 0);
                let deleted: String = " ".repeat(spaces_to_remove);

                self.buffer.delete(line_start, line_start + spaces_to_remove);

                // Only adjust cursor if this is the cursor's line
                if line_idx == self.cursor.line {
                    self.cursor.col = self.cursor.col.saturating_sub(spaces_to_remove);
                    self.cursor.desired_col = self.cursor.col;
                }

                let cursor_after = self.cursor_pos();
                self.history.record_delete(line_start, deleted, cursor_before, cursor_after);
                return spaces_to_remove;
            }
        }
        0
    }

    /// Dedent all lines in selection
    fn dedent_selection(&mut self) {
        if let Some((start, end)) = self.cursor.selection_bounds() {
            self.history.begin_group();

            let mut total_removed_anchor_line = 0;
            let mut total_removed_cursor_line = 0;

            // Dedent each line from start to end (inclusive)
            // We need to track adjustments carefully since positions shift
            for line_idx in start.line..=end.line {
                let removed = self.dedent_line(line_idx);
                if line_idx == self.cursor.anchor_line {
                    total_removed_anchor_line = removed;
                }
                if line_idx == self.cursor.line {
                    total_removed_cursor_line = removed;
                }
            }

            // Adjust selection columns
            self.cursor.anchor_col = self.cursor.anchor_col.saturating_sub(total_removed_anchor_line);
            self.cursor.col = self.cursor.col.saturating_sub(total_removed_cursor_line);
            self.cursor.desired_col = self.cursor.col;

            self.history.end_group();
        }
    }

    // === Line operations ===

    fn move_line_up(&mut self) {
        if self.cursor.line > 0 {
            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            let curr_line = self.cursor.line;
            let prev_line = curr_line - 1;

            let curr_content = self.buffer.line_str(curr_line).unwrap_or_default();

            // Delete current line (including its newline)
            let curr_start = self.buffer.line_col_to_char(curr_line, 0);
            let delete_start = curr_start.saturating_sub(1); // Include newline before
            let delete_end = curr_start + curr_content.len();
            let deleted: String = self.buffer.slice(delete_start, delete_end).chars().collect();
            self.buffer.delete(delete_start, delete_end);
            self.history.record_delete(delete_start, deleted, cursor_before, cursor_before);

            // Insert current line before previous line
            let prev_start = self.buffer.line_col_to_char(prev_line, 0);
            let insert_text = format!("{}\n", curr_content);
            self.buffer.insert(prev_start, &insert_text);
            self.history.record_insert(prev_start, insert_text, cursor_before, Position::new(prev_line, self.cursor.col));

            self.cursor.line = prev_line;
            self.history.end_group();
        }
    }

    fn move_line_down(&mut self) {
        if self.cursor.line + 1 < self.buffer.line_count() {
            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            let curr_line = self.cursor.line;
            let next_line = curr_line + 1;

            let curr_content = self.buffer.line_str(curr_line).unwrap_or_default();

            // Delete current line (including newline after)
            let curr_start = self.buffer.line_col_to_char(curr_line, 0);
            let next_start = self.buffer.line_col_to_char(next_line, 0);
            let deleted: String = self.buffer.slice(curr_start, next_start).chars().collect();
            self.buffer.delete(curr_start, next_start);
            self.history.record_delete(curr_start, deleted, cursor_before, cursor_before);

            // Insert current line after what was the next line (now at curr_line)
            let new_line_end = self.buffer.line_col_to_char(curr_line, self.buffer.line_len(curr_line));
            let insert_text = format!("\n{}", curr_content);
            self.buffer.insert(new_line_end, &insert_text);
            self.history.record_insert(new_line_end, insert_text, cursor_before, Position::new(next_line, self.cursor.col));

            self.cursor.line = next_line;
            self.history.end_group();
        }
    }

    fn duplicate_line_up(&mut self) {
        let cursor_before = self.cursor_pos();
        self.history.begin_group();
        let content = self.buffer.line_str(self.cursor.line).unwrap_or_default();
        let line_start = self.buffer.line_col_to_char(self.cursor.line, 0);
        let insert_text = format!("{}\n", content);
        self.buffer.insert(line_start, &insert_text);
        // Cursor stays on same logical line (now shifted down by 1)
        self.cursor.line += 1;
        let cursor_after = self.cursor_pos();
        self.history.record_insert(line_start, insert_text, cursor_before, cursor_after);
        self.history.end_group();
    }

    fn duplicate_line_down(&mut self) {
        let cursor_before = self.cursor_pos();
        self.history.begin_group();
        let content = self.buffer.line_str(self.cursor.line).unwrap_or_default();
        let line_end = self.buffer.line_col_to_char(self.cursor.line, self.buffer.line_len(self.cursor.line));
        let insert_text = format!("\n{}", content);
        self.buffer.insert(line_end, &insert_text);
        self.cursor.line += 1;
        let cursor_after = self.cursor_pos();
        self.history.record_insert(line_end, insert_text, cursor_before, cursor_after);
        self.history.end_group();
    }

    fn join_lines(&mut self) {
        if self.cursor.line + 1 < self.buffer.line_count() {
            let cursor_before = self.cursor_pos();
            self.history.begin_group();

            let line_len = self.buffer.line_len(self.cursor.line);
            let idx = self.buffer.line_col_to_char(self.cursor.line, line_len);

            // Delete newline
            self.buffer.delete(idx, idx + 1);

            // Move cursor to join point
            self.cursor.col = line_len;
            self.cursor.desired_col = self.cursor.col;

            let cursor_after = self.cursor_pos();
            self.history.record_delete(idx, "\n".to_string(), cursor_before, cursor_after);
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

    /// Set clipboard text (system if available, internal fallback)
    fn set_clipboard(&mut self, text: String) {
        if let Some(ref mut cb) = self.clipboard {
            let _ = cb.set_text(&text);
        }
        self.internal_clipboard = text;
    }

    /// Get clipboard text (system if available, internal fallback)
    fn get_clipboard(&mut self) -> String {
        if let Some(ref mut cb) = self.clipboard {
            if let Ok(text) = cb.get_text() {
                return text;
            }
        }
        self.internal_clipboard.clone()
    }

    fn copy(&mut self) {
        if let Some(text) = self.get_selection_text() {
            self.set_clipboard(text);
            self.message = Some("Copied".to_string());
        } else {
            // Copy current line
            if let Some(line) = self.buffer.line_str(self.cursor.line) {
                self.set_clipboard(format!("{}\n", line));
                self.message = Some("Copied line".to_string());
            }
        }
    }

    fn cut(&mut self) {
        if let Some(text) = self.get_selection_text() {
            self.set_clipboard(text);
            self.delete_selection();
            self.message = Some("Cut".to_string());
        } else {
            // Cut current line
            if let Some(line) = self.buffer.line_str(self.cursor.line) {
                self.set_clipboard(format!("{}\n", line));
                let cursor_before = self.cursor_pos();

                let line_start = self.buffer.line_col_to_char(self.cursor.line, 0);

                if self.cursor.line + 1 < self.buffer.line_count() {
                    // Not the last line - delete line including its newline
                    let line_end = line_start + line.len() + 1;
                    let deleted: String = self.buffer.slice(line_start, line_end).chars().collect();
                    self.buffer.delete(line_start, line_end);
                    self.cursor.col = 0;
                    self.cursor.desired_col = 0;
                    let cursor_after = self.cursor_pos();
                    self.history.record_delete(line_start, deleted, cursor_before, cursor_after);
                } else if self.cursor.line > 0 {
                    // Last line with content - delete newline before it and the line
                    let delete_start = line_start.saturating_sub(1);
                    let delete_end = line_start + line.len();
                    let deleted: String = self.buffer.slice(delete_start, delete_end).chars().collect();
                    self.buffer.delete(delete_start, delete_end);
                    self.cursor.line -= 1;
                    self.cursor.col = 0;
                    self.cursor.desired_col = 0;
                    let cursor_after = self.cursor_pos();
                    self.history.record_delete(delete_start, deleted, cursor_before, cursor_after);
                } else {
                    // Only line - just clear it
                    if !line.is_empty() {
                        self.buffer.delete(line_start, line_start + line.len());
                        self.cursor.col = 0;
                        self.cursor.desired_col = 0;
                        let cursor_after = self.cursor_pos();
                        self.history.record_delete(line_start, line.clone(), cursor_before, cursor_after);
                    }
                }

                self.message = Some("Cut line".to_string());
            }
        }
        self.history.maybe_break_group();
    }

    fn paste(&mut self) {
        let text = self.get_clipboard();
        if !text.is_empty() {
            self.insert_text(&text);
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

/// Check if a character is a "word" character (alphanumeric or underscore)
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
