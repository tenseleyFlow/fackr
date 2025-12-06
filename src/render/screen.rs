use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Stdout, Write};

use crate::buffer::Buffer;
use crate::editor::{Cursor, Position};

// Editor color scheme (256-color palette)
const BG_COLOR: Color = Color::AnsiValue(234);           // Off-black editor background
const CURRENT_LINE_BG: Color = Color::AnsiValue(236);    // Slightly lighter for current line
const LINE_NUM_COLOR: Color = Color::AnsiValue(243);     // Gray for line numbers
const CURRENT_LINE_NUM_COLOR: Color = Color::Yellow;     // Yellow for active line number
const BRACKET_MATCH_BG: Color = Color::AnsiValue(240);   // Highlight for matching brackets

/// Terminal screen renderer
pub struct Screen {
    stdout: Stdout,
    pub rows: u16,
    pub cols: u16,
    keyboard_enhanced: bool,
}

impl Screen {
    pub fn new() -> Result<Self> {
        let (cols, rows) = terminal::size()?;
        Ok(Self {
            stdout: stdout(),
            rows,
            cols,
            keyboard_enhanced: false,
        })
    }

    pub fn enter_raw_mode(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen, Hide)?;

        // Try to enable keyboard enhancement for better modifier key detection
        // This enables the kitty keyboard protocol on supporting terminals
        if execute!(
            self.stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        )
        .is_ok()
        {
            self.keyboard_enhanced = true;
        }

        Ok(())
    }

    pub fn leave_raw_mode(&mut self) -> Result<()> {
        if self.keyboard_enhanced {
            let _ = execute!(self.stdout, PopKeyboardEnhancementFlags);
        }
        execute!(self.stdout, Show, LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    pub fn refresh_size(&mut self) -> Result<()> {
        let (cols, rows) = terminal::size()?;
        self.cols = cols;
        self.rows = rows;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) -> Result<()> {
        execute!(self.stdout, Clear(ClearType::All))?;
        Ok(())
    }

    /// Render the editor view
    pub fn render(
        &mut self,
        buffer: &Buffer,
        cursor: &Cursor,
        viewport_line: usize,
        filename: Option<&str>,
        message: Option<&str>,
        bracket_match: Option<(usize, usize)>,
    ) -> Result<()> {
        // Hide cursor during render to prevent flicker
        execute!(self.stdout, Hide)?;

        let line_num_width = self.line_number_width(buffer.line_count());
        let text_cols = self.cols as usize - line_num_width - 1;

        // Get selection bounds if any
        let selection = cursor.selection_bounds();

        // Reserve 1 row for status bar
        let text_rows = self.rows.saturating_sub(1) as usize;

        // Draw text area
        for row in 0..text_rows {
            let line_idx = viewport_line + row;
            let is_current_line = line_idx == cursor.line;
            execute!(self.stdout, MoveTo(0, row as u16))?;

            if line_idx < buffer.line_count() {
                // Line number with appropriate color
                let line_num_fg = if is_current_line {
                    CURRENT_LINE_NUM_COLOR
                } else {
                    LINE_NUM_COLOR
                };
                let line_bg = if is_current_line { CURRENT_LINE_BG } else { BG_COLOR };

                execute!(
                    self.stdout,
                    SetBackgroundColor(line_bg),
                    SetForegroundColor(line_num_fg),
                    Print(format!("{:>width$} ", line_idx + 1, width = line_num_width)),
                )?;

                // Line content with selection highlighting
                if let Some(line) = buffer.line_str(line_idx) {
                    // Check if bracket match is on this line
                    let bracket_col = bracket_match
                        .filter(|(bl, _)| *bl == line_idx)
                        .map(|(_, bc)| bc);
                    self.render_line_with_selection(
                        &line,
                        line_idx,
                        text_cols,
                        selection.as_ref(),
                        is_current_line,
                        bracket_col,
                    )?;
                }

                // Fill rest of line with background color
                execute!(
                    self.stdout,
                    SetBackgroundColor(line_bg),
                    Clear(ClearType::UntilNewLine),
                    ResetColor
                )?;
            } else {
                // Empty line indicator
                execute!(
                    self.stdout,
                    SetBackgroundColor(BG_COLOR),
                    SetForegroundColor(Color::DarkBlue),
                    Print(format!("{:>width$} ", "~", width = line_num_width)),
                    Clear(ClearType::UntilNewLine),
                    ResetColor
                )?;
            }
        }

        // Status bar
        self.render_status_bar(buffer, cursor, filename, message)?;

        // Position cursor
        let cursor_row = cursor.line.saturating_sub(viewport_line);
        let cursor_col = line_num_width + 1 + cursor.col;
        execute!(
            self.stdout,
            MoveTo(cursor_col as u16, cursor_row as u16),
            Show
        )?;

        self.stdout.flush()?;
        Ok(())
    }

    fn render_line_with_selection(
        &mut self,
        line: &str,
        line_idx: usize,
        max_cols: usize,
        selection: Option<&(Position, Position)>,
        is_current_line: bool,
        bracket_col: Option<usize>,
    ) -> Result<()> {
        let chars: Vec<char> = line.chars().take(max_cols).collect();
        let line_bg = if is_current_line { CURRENT_LINE_BG } else { BG_COLOR };

        // Determine selection range for this line
        let (sel_start, sel_end) = if let Some((start, end)) = selection {
            let line_has_selection = line_idx >= start.line && line_idx <= end.line;
            if line_has_selection {
                let s = if line_idx == start.line { start.col } else { 0 };
                let e = if line_idx == end.line { end.col } else { chars.len() };
                (Some(s), Some(e))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // Render character by character for precise highlighting
        for (col, ch) in chars.iter().enumerate() {
            let in_selection = sel_start.map_or(false, |s| col >= s)
                && sel_end.map_or(false, |e| col < e);
            let is_bracket_match = bracket_col == Some(col);

            let bg = if in_selection {
                Color::Blue
            } else if is_bracket_match {
                BRACKET_MATCH_BG
            } else {
                line_bg
            };

            let fg = if in_selection {
                Some(Color::White)
            } else {
                None
            };

            execute!(self.stdout, SetBackgroundColor(bg))?;
            if let Some(fg_color) = fg {
                execute!(self.stdout, SetForegroundColor(fg_color))?;
            }
            execute!(self.stdout, Print(ch))?;
            if fg.is_some() {
                execute!(self.stdout, ResetColor)?;
            }
        }

        Ok(())
    }

    fn render_status_bar(
        &mut self,
        buffer: &Buffer,
        cursor: &Cursor,
        filename: Option<&str>,
        message: Option<&str>,
    ) -> Result<()> {
        let status_row = self.rows.saturating_sub(1);
        execute!(self.stdout, MoveTo(0, status_row))?;

        // Status bar background
        execute!(
            self.stdout,
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::White)
        )?;

        // Left side: filename + modified indicator
        let name = filename.unwrap_or("[No Name]");
        let modified = if buffer.modified { " [+]" } else { "" };
        let left = format!(" {}{}", name, modified);

        // Right side: position (and message if any)
        let pos = format!("Ln {}, Col {}", cursor.line + 1, cursor.col + 1);
        let right = if let Some(msg) = message {
            format!(" {} | {} ", msg, pos)
        } else {
            format!(" {} ", pos)
        };

        // Pad middle
        let padding = (self.cols as usize).saturating_sub(left.len() + right.len());
        let middle = " ".repeat(padding);

        execute!(
            self.stdout,
            Print(&left),
            Print(&middle),
            Print(&right),
            ResetColor
        )?;

        Ok(())
    }

    fn line_number_width(&self, line_count: usize) -> usize {
        let digits = if line_count == 0 {
            1
        } else {
            (line_count as f64).log10().floor() as usize + 1
        };
        digits.max(3) // Minimum 3 characters
    }
}
