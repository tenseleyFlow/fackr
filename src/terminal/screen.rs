//! Terminal screen buffer
//!
//! Manages the grid of cells that make up the terminal display.
//! Uses VTE for parsing escape sequences.

use vte::{Params, Parser, Perform};

/// A single cell in the terminal grid
#[derive(Clone, Debug)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            underline: false,
            inverse: false,
        }
    }
}

/// Terminal colors
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Terminal screen state
pub struct TerminalScreen {
    /// Grid of cells (row-major)
    cells: Vec<Vec<Cell>>,
    /// Number of columns
    pub cols: u16,
    /// Number of rows
    pub rows: u16,
    /// Cursor position (0-indexed)
    pub cursor_row: u16,
    pub cursor_col: u16,
    /// Current text attributes
    current_fg: Color,
    current_bg: Color,
    current_bold: bool,
    current_underline: bool,
    current_inverse: bool,
    /// VTE parser
    parser: Parser,
    /// Scrollback buffer
    scrollback: Vec<Vec<Cell>>,
    /// Max scrollback lines
    max_scrollback: usize,
    /// Scroll offset (0 = at bottom)
    pub scroll_offset: usize,
    /// DEC private modes
    pub cursor_visible: bool,
    autowrap: bool,
    application_cursor_keys: bool,
    bracketed_paste: bool,
    /// Alternate screen buffer
    alt_cells: Option<Vec<Vec<Cell>>>,
    alt_cursor_row: u16,
    alt_cursor_col: u16,
    using_alt_screen: bool,
    /// Saved cursor position (for ESC 7/8 and CSI s/u)
    saved_cursor_row: u16,
    saved_cursor_col: u16,
    /// Scroll region (top, bottom) - 0-indexed, inclusive
    scroll_top: u16,
    scroll_bottom: u16,
    /// Response queue for device status reports
    response_queue: Vec<Vec<u8>>,
    /// Current working directory (from OSC 7)
    pub cwd: Option<String>,
}

impl TerminalScreen {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cells = vec![vec![Cell::default(); cols as usize]; rows as usize];
        Self {
            cells,
            cols,
            rows,
            cursor_row: 0,
            cursor_col: 0,
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_bold: false,
            current_underline: false,
            current_inverse: false,
            parser: Parser::new(),
            scrollback: Vec::new(),
            max_scrollback: 10000,
            scroll_offset: 0,
            // DEC private modes
            cursor_visible: true,
            autowrap: true,
            application_cursor_keys: false,
            bracketed_paste: false,
            // Alternate screen buffer
            alt_cells: None,
            alt_cursor_row: 0,
            alt_cursor_col: 0,
            using_alt_screen: false,
            // Saved cursor
            saved_cursor_row: 0,
            saved_cursor_col: 0,
            // Scroll region (full screen by default)
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            // Response queue
            response_queue: Vec::new(),
            // Current working directory
            cwd: None,
        }
    }

    /// Process raw bytes from the PTY
    pub fn process(&mut self, data: &[u8]) {
        // Take parser out temporarily to avoid borrow conflict
        let mut parser = std::mem::take(&mut self.parser);
        for byte in data {
            parser.advance(self, *byte);
        }
        self.parser = parser;
    }

    /// Get a reference to the cell grid
    pub fn cells(&self) -> &Vec<Vec<Cell>> {
        &self.cells
    }

    /// Get a row from scrollback or current screen
    pub fn get_row(&self, row: usize) -> Option<&Vec<Cell>> {
        if self.scroll_offset > 0 {
            let scrollback_row = self.scrollback.len().saturating_sub(self.scroll_offset) + row;
            if scrollback_row < self.scrollback.len() {
                self.scrollback.get(scrollback_row)
            } else {
                let screen_row = scrollback_row - self.scrollback.len();
                self.cells.get(screen_row)
            }
        } else {
            self.cells.get(row)
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) {
        // Create new cell grid
        let mut new_cells = vec![vec![Cell::default(); cols as usize]; rows as usize];

        // Copy existing content
        for (r, row) in self.cells.iter().enumerate() {
            if r >= rows as usize {
                break;
            }
            for (c, cell) in row.iter().enumerate() {
                if c >= cols as usize {
                    break;
                }
                new_cells[r][c] = cell.clone();
            }
        }

        self.cells = new_cells;
        self.cols = cols;
        self.rows = rows;

        // Ensure cursor is within bounds
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));

        // Update scroll region to new size
        self.scroll_bottom = rows.saturating_sub(1);
        if self.scroll_top > self.scroll_bottom {
            self.scroll_top = 0;
        }
    }

    /// Drain response queue (for device status reports)
    pub fn drain_responses(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.response_queue)
    }

    /// Enter alternate screen buffer
    fn enter_alt_screen(&mut self) {
        if !self.using_alt_screen {
            // Save primary screen and cursor
            self.alt_cells = Some(std::mem::take(&mut self.cells));
            self.alt_cursor_row = self.cursor_row;
            self.alt_cursor_col = self.cursor_col;
            // Create fresh alt screen
            self.cells = vec![vec![Cell::default(); self.cols as usize]; self.rows as usize];
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.using_alt_screen = true;
        }
    }

    /// Leave alternate screen buffer
    fn leave_alt_screen(&mut self) {
        if self.using_alt_screen {
            if let Some(primary) = self.alt_cells.take() {
                self.cells = primary;
                self.cursor_row = self.alt_cursor_row;
                self.cursor_col = self.alt_cursor_col;
            }
            self.using_alt_screen = false;
        }
    }

    /// Save cursor position
    fn save_cursor(&mut self) {
        self.saved_cursor_row = self.cursor_row;
        self.saved_cursor_col = self.cursor_col;
    }

    /// Restore cursor position
    fn restore_cursor(&mut self) {
        self.cursor_row = self.saved_cursor_row.min(self.rows.saturating_sub(1));
        self.cursor_col = self.saved_cursor_col.min(self.cols.saturating_sub(1));
    }

    /// Handle DEC private mode set/reset
    fn handle_dec_private_mode(&mut self, params: &[u16], set: bool) {
        for &param in params {
            match param {
                1 => self.application_cursor_keys = set,     // DECCKM
                7 => self.autowrap = set,                     // DECAWM
                25 => self.cursor_visible = set,              // DECTCEM
                1049 => {
                    // Alternate screen buffer
                    if set {
                        self.enter_alt_screen();
                    } else {
                        self.leave_alt_screen();
                    }
                }
                2004 => self.bracketed_paste = set,           // Bracketed paste
                _ => {} // Ignore unknown modes
            }
        }
    }

    /// Reverse index - move cursor up, scroll down if at top
    fn reverse_index(&mut self) {
        if self.cursor_row == self.scroll_top {
            self.scroll_down_region(1);
        } else {
            self.cursor_row = self.cursor_row.saturating_sub(1);
        }
    }

    /// Index - move cursor down, scroll up if at bottom
    fn index(&mut self) {
        if self.cursor_row == self.scroll_bottom {
            self.scroll_up_region(1);
        } else if self.cursor_row < self.rows - 1 {
            self.cursor_row += 1;
        }
    }

    /// Next line - move to start of next line, scroll if needed
    fn next_line(&mut self) {
        self.cursor_col = 0;
        self.index();
    }

    /// Scroll up within scroll region
    fn scroll_up_region(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;

        for _ in 0..n {
            if top < self.cells.len() && bottom < self.cells.len() && top <= bottom {
                // Move top row to scrollback (only if scroll region is full screen)
                if self.scroll_top == 0 && self.scroll_bottom == self.rows - 1 {
                    let top_row = self.cells.remove(top);
                    self.scrollback.push(top_row);
                    if self.scrollback.len() > self.max_scrollback {
                        self.scrollback.remove(0);
                    }
                } else {
                    self.cells.remove(top);
                }
                // Insert new row at bottom of scroll region
                self.cells.insert(bottom, vec![Cell::default(); self.cols as usize]);
            }
        }
    }

    /// Scroll down within scroll region
    fn scroll_down_region(&mut self, n: u16) {
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;

        for _ in 0..n {
            if top < self.cells.len() && bottom < self.cells.len() && top <= bottom {
                // Remove bottom row
                self.cells.remove(bottom);
                // Insert new row at top of scroll region
                self.cells.insert(top, vec![Cell::default(); self.cols as usize]);
            }
        }
    }

    /// Insert n lines at cursor position
    fn insert_lines(&mut self, n: u16) {
        let row = self.cursor_row as usize;
        let bottom = self.scroll_bottom as usize;

        for _ in 0..n {
            if row <= bottom && bottom < self.cells.len() {
                self.cells.remove(bottom);
                self.cells.insert(row, vec![Cell::default(); self.cols as usize]);
            }
        }
    }

    /// Delete n lines at cursor position
    fn delete_lines(&mut self, n: u16) {
        let row = self.cursor_row as usize;
        let bottom = self.scroll_bottom as usize;

        for _ in 0..n {
            if row <= bottom && row < self.cells.len() {
                self.cells.remove(row);
                self.cells.insert(bottom, vec![Cell::default(); self.cols as usize]);
            }
        }
    }

    /// Insert n blank characters at cursor position
    fn insert_chars(&mut self, n: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let col = self.cursor_col as usize;
            for _ in 0..n {
                if col < row.len() {
                    row.pop(); // Remove from end
                    row.insert(col, Cell::default()); // Insert at cursor
                }
            }
        }
    }

    /// Delete n characters at cursor position
    fn delete_chars(&mut self, n: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let col = self.cursor_col as usize;
            for _ in 0..n {
                if col < row.len() {
                    row.remove(col);
                    row.push(Cell::default()); // Add blank at end
                }
            }
        }
    }

    /// Clear from start of screen to cursor
    fn clear_from_start(&mut self) {
        // Clear all rows before cursor row
        for row in self.cells.iter_mut().take(self.cursor_row as usize) {
            for cell in row.iter_mut() {
                *cell = Cell::default();
            }
        }
        // Clear current row from start to cursor
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for cell in row.iter_mut().take(self.cursor_col as usize + 1) {
                *cell = Cell::default();
            }
        }
    }

    /// Clear from start of line to cursor
    fn clear_line_from_start(&mut self) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for cell in row.iter_mut().take(self.cursor_col as usize + 1) {
                *cell = Cell::default();
            }
        }
    }

    /// Scroll the screen up by one line
    fn scroll_up(&mut self) {
        if !self.cells.is_empty() {
            // Move top row to scrollback
            let top_row = self.cells.remove(0);
            self.scrollback.push(top_row);

            // Trim scrollback if too large
            if self.scrollback.len() > self.max_scrollback {
                self.scrollback.remove(0);
            }

            // Add new empty row at bottom
            self.cells.push(vec![Cell::default(); self.cols as usize]);
        }
    }

    /// Clear the screen
    fn clear_screen(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                *cell = Cell::default();
            }
        }
    }

    /// Clear from cursor to end of line
    fn clear_to_eol(&mut self) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for cell in row.iter_mut().skip(self.cursor_col as usize) {
                *cell = Cell::default();
            }
        }
    }

    /// Clear from cursor to end of screen
    fn clear_to_eos(&mut self) {
        self.clear_to_eol();
        for row in self.cells.iter_mut().skip(self.cursor_row as usize + 1) {
            for cell in row {
                *cell = Cell::default();
            }
        }
    }

    /// Put a character at the cursor position
    fn put_char(&mut self, c: char) {
        if self.cursor_row < self.rows && self.cursor_col < self.cols {
            if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
                if let Some(cell) = row.get_mut(self.cursor_col as usize) {
                    cell.c = c;
                    cell.fg = self.current_fg;
                    cell.bg = self.current_bg;
                    cell.bold = self.current_bold;
                    cell.underline = self.current_underline;
                    cell.inverse = self.current_inverse;
                }
            }
        }
    }
}

/// VTE Perform implementation for processing escape sequences
impl Perform for TerminalScreen {
    fn print(&mut self, c: char) {
        self.put_char(c);
        self.cursor_col += 1;

        // Handle line wrap
        if self.cursor_col >= self.cols {
            self.cursor_col = 0;
            self.cursor_row += 1;
            if self.cursor_row >= self.rows {
                self.scroll_up();
                self.cursor_row = self.rows - 1;
            }
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // Backspace
            0x08 => {
                self.cursor_col = self.cursor_col.saturating_sub(1);
            }
            // Tab
            0x09 => {
                self.cursor_col = ((self.cursor_col / 8) + 1) * 8;
                if self.cursor_col >= self.cols {
                    self.cursor_col = self.cols - 1;
                }
            }
            // Line feed
            0x0A => {
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    self.scroll_up();
                    self.cursor_row = self.rows - 1;
                }
            }
            // Carriage return
            0x0D => {
                self.cursor_col = 0;
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC 7: Set working directory
        // Format: OSC 7 ; file://hostname/path ST
        if !params.is_empty() {
            if let Ok(cmd) = std::str::from_utf8(params[0]) {
                if cmd == "7" && params.len() >= 2 {
                    if let Ok(url) = std::str::from_utf8(params[1]) {
                        // Parse file://hostname/path format
                        if let Some(path) = url.strip_prefix("file://") {
                            // Find the first slash after hostname
                            if let Some(slash_idx) = path.find('/') {
                                let dir = &path[slash_idx..];
                                self.cwd = Some(dir.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let params: Vec<u16> = params.iter().map(|p| p.first().copied().unwrap_or(0) as u16).collect();

        // Check for DEC private mode sequences (CSI ? ...)
        let is_private = intermediates.contains(&b'?');

        if is_private {
            match action {
                'h' => self.handle_dec_private_mode(&params, true),  // Set mode
                'l' => self.handle_dec_private_mode(&params, false), // Reset mode
                _ => {}
            }
            return;
        }

        match action {
            // Cursor Up
            'A' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            // Cursor Down
            'B' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
            }
            // Cursor Forward
            'C' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = (self.cursor_col + n).min(self.cols - 1);
            }
            // Cursor Back
            'D' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            // Cursor Next Line
            'E' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = 0;
                self.cursor_row = (self.cursor_row + n).min(self.rows - 1);
            }
            // Cursor Previous Line
            'F' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = 0;
                self.cursor_row = self.cursor_row.saturating_sub(n);
            }
            // Cursor Horizontal Absolute
            'G' => {
                let col = params.first().copied().unwrap_or(1).max(1) - 1;
                self.cursor_col = col.min(self.cols - 1);
            }
            // Cursor Position (CUP)
            'H' | 'f' => {
                let row = params.first().copied().unwrap_or(1).max(1) - 1;
                let col = params.get(1).copied().unwrap_or(1).max(1) - 1;
                self.cursor_row = row.min(self.rows - 1);
                self.cursor_col = col.min(self.cols - 1);
            }
            // Erase in Display
            'J' => {
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.clear_to_eos(),
                    1 => self.clear_from_start(),
                    2 | 3 => self.clear_screen(),
                    _ => {}
                }
            }
            // Erase in Line
            'K' => {
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.clear_to_eol(),
                    1 => self.clear_line_from_start(),
                    2 => {
                        // Clear entire line
                        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
                            for cell in row {
                                *cell = Cell::default();
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Insert Lines
            'L' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.insert_lines(n);
            }
            // Delete Lines
            'M' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.delete_lines(n);
            }
            // Delete Characters
            'P' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.delete_chars(n);
            }
            // Scroll Up
            'S' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.scroll_up_region(n);
            }
            // Scroll Down
            'T' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.scroll_down_region(n);
            }
            // Erase Characters
            'X' => {
                let n = params.first().copied().unwrap_or(1).max(1) as usize;
                if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
                    for i in 0..n {
                        let col = self.cursor_col as usize + i;
                        if col < row.len() {
                            row[col] = Cell::default();
                        }
                    }
                }
            }
            // Insert Characters
            '@' => {
                let n = params.first().copied().unwrap_or(1).max(1);
                self.insert_chars(n);
            }
            // Cursor Vertical Absolute
            'd' => {
                let row = params.first().copied().unwrap_or(1).max(1) - 1;
                self.cursor_row = row.min(self.rows - 1);
            }
            // Device Status Report
            'n' => {
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    5 => {
                        // Status report - respond "OK"
                        self.response_queue.push(b"\x1b[0n".to_vec());
                    }
                    6 => {
                        // Cursor position report
                        let response = format!("\x1b[{};{}R", self.cursor_row + 1, self.cursor_col + 1);
                        self.response_queue.push(response.into_bytes());
                    }
                    _ => {}
                }
            }
            // Set Scroll Region (DECSTBM)
            'r' => {
                let top = params.first().copied().unwrap_or(1).max(1) - 1;
                let bottom = params.get(1).copied().unwrap_or(self.rows).max(1) - 1;
                if top < bottom && bottom < self.rows {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                    // Move cursor to home position
                    self.cursor_row = 0;
                    self.cursor_col = 0;
                }
            }
            // Save Cursor Position
            's' => {
                self.save_cursor();
            }
            // Restore Cursor Position
            'u' => {
                self.restore_cursor();
            }
            // Select Graphic Rendition (SGR) - colors and attributes
            'm' => {
                if params.is_empty() {
                    // Reset all attributes
                    self.current_fg = Color::Default;
                    self.current_bg = Color::Default;
                    self.current_bold = false;
                    self.current_underline = false;
                    self.current_inverse = false;
                    return;
                }

                let mut iter = params.iter().peekable();
                while let Some(&param) = iter.next() {
                    match param {
                        0 => {
                            self.current_fg = Color::Default;
                            self.current_bg = Color::Default;
                            self.current_bold = false;
                            self.current_underline = false;
                            self.current_inverse = false;
                        }
                        1 => self.current_bold = true,
                        4 => self.current_underline = true,
                        7 => self.current_inverse = true,
                        22 => self.current_bold = false,
                        24 => self.current_underline = false,
                        27 => self.current_inverse = false,
                        // Foreground colors
                        30 => self.current_fg = Color::Black,
                        31 => self.current_fg = Color::Red,
                        32 => self.current_fg = Color::Green,
                        33 => self.current_fg = Color::Yellow,
                        34 => self.current_fg = Color::Blue,
                        35 => self.current_fg = Color::Magenta,
                        36 => self.current_fg = Color::Cyan,
                        37 => self.current_fg = Color::White,
                        38 => {
                            // Extended foreground color
                            if let Some(&mode) = iter.next() {
                                match mode {
                                    5 => {
                                        // 256-color mode
                                        if let Some(&idx) = iter.next() {
                                            self.current_fg = Color::Indexed(idx as u8);
                                        }
                                    }
                                    2 => {
                                        // RGB mode
                                        let r = iter.next().copied().unwrap_or(0) as u8;
                                        let g = iter.next().copied().unwrap_or(0) as u8;
                                        let b = iter.next().copied().unwrap_or(0) as u8;
                                        self.current_fg = Color::Rgb(r, g, b);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        39 => self.current_fg = Color::Default,
                        // Background colors
                        40 => self.current_bg = Color::Black,
                        41 => self.current_bg = Color::Red,
                        42 => self.current_bg = Color::Green,
                        43 => self.current_bg = Color::Yellow,
                        44 => self.current_bg = Color::Blue,
                        45 => self.current_bg = Color::Magenta,
                        46 => self.current_bg = Color::Cyan,
                        47 => self.current_bg = Color::White,
                        48 => {
                            // Extended background color
                            if let Some(&mode) = iter.next() {
                                match mode {
                                    5 => {
                                        if let Some(&idx) = iter.next() {
                                            self.current_bg = Color::Indexed(idx as u8);
                                        }
                                    }
                                    2 => {
                                        let r = iter.next().copied().unwrap_or(0) as u8;
                                        let g = iter.next().copied().unwrap_or(0) as u8;
                                        let b = iter.next().copied().unwrap_or(0) as u8;
                                        self.current_bg = Color::Rgb(r, g, b);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        49 => self.current_bg = Color::Default,
                        // Bright foreground colors
                        90 => self.current_fg = Color::BrightBlack,
                        91 => self.current_fg = Color::BrightRed,
                        92 => self.current_fg = Color::BrightGreen,
                        93 => self.current_fg = Color::BrightYellow,
                        94 => self.current_fg = Color::BrightBlue,
                        95 => self.current_fg = Color::BrightMagenta,
                        96 => self.current_fg = Color::BrightCyan,
                        97 => self.current_fg = Color::BrightWhite,
                        // Bright background colors
                        100 => self.current_bg = Color::BrightBlack,
                        101 => self.current_bg = Color::BrightRed,
                        102 => self.current_bg = Color::BrightGreen,
                        103 => self.current_bg = Color::BrightYellow,
                        104 => self.current_bg = Color::BrightBlue,
                        105 => self.current_bg = Color::BrightMagenta,
                        106 => self.current_bg = Color::BrightCyan,
                        107 => self.current_bg = Color::BrightWhite,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            // Save cursor position (DECSC)
            ([], b'7') => self.save_cursor(),
            // Restore cursor position (DECRC)
            ([], b'8') => self.restore_cursor(),
            // Reverse Index - move cursor up, scroll down if at top
            ([], b'M') => self.reverse_index(),
            // Index - move cursor down, scroll up if at bottom
            ([], b'D') => self.index(),
            // Next Line - move to start of next line
            ([], b'E') => self.next_line(),
            // Reset to Initial State (RIS)
            ([], b'c') => {
                // Full reset
                self.clear_screen();
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.current_fg = Color::Default;
                self.current_bg = Color::Default;
                self.current_bold = false;
                self.current_underline = false;
                self.current_inverse = false;
                self.scroll_top = 0;
                self.scroll_bottom = self.rows.saturating_sub(1);
            }
            _ => {}
        }
    }
}
