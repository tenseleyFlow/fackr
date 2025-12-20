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

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let params: Vec<u16> = params.iter().map(|p| p.first().copied().unwrap_or(0) as u16).collect();

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
                    1 => {} // Clear from start to cursor (TODO)
                    2 | 3 => self.clear_screen(),
                    _ => {}
                }
            }
            // Erase in Line
            'K' => {
                let mode = params.first().copied().unwrap_or(0);
                match mode {
                    0 => self.clear_to_eol(),
                    1 => {} // Clear from start of line to cursor (TODO)
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

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}
