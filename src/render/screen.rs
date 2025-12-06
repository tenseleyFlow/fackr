use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        DisableMouseCapture, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Stdout, Write};

use crate::buffer::Buffer;
use crate::editor::{Cursors, Position};

// Editor color scheme (256-color palette)
const BG_COLOR: Color = Color::AnsiValue(234);           // Off-black editor background
const CURRENT_LINE_BG: Color = Color::AnsiValue(236);    // Slightly lighter for current line
const LINE_NUM_COLOR: Color = Color::AnsiValue(243);     // Gray for line numbers
const CURRENT_LINE_NUM_COLOR: Color = Color::Yellow;     // Yellow for active line number
const BRACKET_MATCH_BG: Color = Color::AnsiValue(240);   // Highlight for matching brackets
// Secondary cursors use Color::Magenta for visibility

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
        execute!(self.stdout, EnterAlternateScreen, Hide, EnableMouseCapture)?;

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
        execute!(self.stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
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
        cursors: &Cursors,
        viewport_line: usize,
        filename: Option<&str>,
        message: Option<&str>,
        bracket_match: Option<(usize, usize)>,
    ) -> Result<()> {
        // Hide cursor during render to prevent flicker
        execute!(self.stdout, Hide)?;

        let line_num_width = self.line_number_width(buffer.line_count());
        let text_cols = self.cols as usize - line_num_width - 1;

        // Get primary cursor for current line highlighting
        let primary = cursors.primary();

        // Collect all selections from all cursors
        let selections: Vec<(Position, Position)> = cursors.all()
            .iter()
            .filter_map(|c| c.selection_bounds())
            .collect();

        // Collect all cursor positions for rendering
        let primary_idx = cursors.primary_index();
        let cursor_positions: Vec<(usize, usize, bool)> = cursors.all()
            .iter()
            .enumerate()
            .map(|(i, c)| (c.line, c.col, i == primary_idx)) // (line, col, is_primary)
            .collect();

        // Reserve 1 row for status bar
        let text_rows = self.rows.saturating_sub(1) as usize;

        // Draw text area
        for row in 0..text_rows {
            let line_idx = viewport_line + row;
            let is_current_line = line_idx == primary.line;
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

                // Line content with selection and cursor highlighting
                if let Some(line) = buffer.line_str(line_idx) {
                    // Check if bracket match is on this line
                    let bracket_col = bracket_match
                        .filter(|(bl, _)| *bl == line_idx)
                        .map(|(_, bc)| bc);

                    // Get cursors on this line (excluding primary which uses hardware cursor)
                    let secondary_cursors: Vec<usize> = cursor_positions.iter()
                        .filter(|(l, _, is_primary)| *l == line_idx && !*is_primary)
                        .map(|(_, c, _)| *c)
                        .collect();

                    self.render_line_with_cursors(
                        &line,
                        line_idx,
                        text_cols,
                        &selections,
                        is_current_line,
                        bracket_col,
                        &secondary_cursors,
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
        self.render_status_bar(buffer, cursors, filename, message)?;

        // Position hardware cursor at primary cursor
        let cursor_row = primary.line.saturating_sub(viewport_line);
        let cursor_col = line_num_width + 1 + primary.col;
        execute!(
            self.stdout,
            MoveTo(cursor_col as u16, cursor_row as u16),
            Show
        )?;

        self.stdout.flush()?;
        Ok(())
    }

    fn render_line_with_cursors(
        &mut self,
        line: &str,
        line_idx: usize,
        max_cols: usize,
        selections: &[(Position, Position)],
        is_current_line: bool,
        bracket_col: Option<usize>,
        secondary_cursors: &[usize],
    ) -> Result<()> {
        let chars: Vec<char> = line.chars().take(max_cols).collect();
        let line_bg = if is_current_line { CURRENT_LINE_BG } else { BG_COLOR };
        let default_fg = Color::Reset; // Default terminal foreground

        // Build selection ranges for this line from all selections
        let mut sel_ranges: Vec<(usize, usize)> = Vec::new();
        for (start, end) in selections {
            if line_idx >= start.line && line_idx <= end.line {
                let s = if line_idx == start.line { start.col } else { 0 };
                let e = if line_idx == end.line { end.col } else { chars.len() };
                if s < e {
                    sel_ranges.push((s, e));
                }
            }
        }

        // Render character by character for precise highlighting
        for (col, ch) in chars.iter().enumerate() {
            let in_selection = sel_ranges.iter().any(|(s, e)| col >= *s && col < *e);
            let is_bracket_match = bracket_col == Some(col);
            let is_secondary_cursor = secondary_cursors.contains(&col);

            // Determine background color
            let bg = if in_selection {
                Color::Blue
            } else if is_secondary_cursor {
                Color::Magenta  // Use magenta for better visibility
            } else if is_bracket_match {
                BRACKET_MATCH_BG
            } else {
                line_bg
            };

            // Determine foreground color
            let fg = if in_selection {
                Color::White
            } else if is_secondary_cursor {
                Color::White  // White text on magenta bg
            } else {
                default_fg
            };

            execute!(
                self.stdout,
                SetBackgroundColor(bg),
                SetForegroundColor(fg),
                Print(ch)
            )?;
        }

        // Reset to line background for rest of line
        execute!(self.stdout, SetBackgroundColor(line_bg), SetForegroundColor(default_fg))?;

        // Handle secondary cursors at end of line (past text content)
        // Find the rightmost secondary cursor past text
        let max_cursor_past_text = secondary_cursors.iter()
            .filter(|&&c| c >= chars.len())
            .max()
            .copied();

        if let Some(max_cursor) = max_cursor_past_text {
            if max_cursor < max_cols {
                // Fill spaces up to and including the cursor positions
                for col in chars.len()..=max_cursor {
                    if secondary_cursors.contains(&col) {
                        execute!(
                            self.stdout,
                            SetBackgroundColor(Color::Magenta),
                            SetForegroundColor(Color::White),
                            Print(" ")
                        )?;
                    } else {
                        execute!(
                            self.stdout,
                            SetBackgroundColor(line_bg),
                            Print(" ")
                        )?;
                    }
                }
                // Reset for the rest of the line
                execute!(self.stdout, SetBackgroundColor(line_bg), SetForegroundColor(default_fg))?;
            }
        }

        Ok(())
    }

    fn render_status_bar(
        &mut self,
        buffer: &Buffer,
        cursors: &Cursors,
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

        // Left side: filename + modified indicator + cursor count
        let name = filename.unwrap_or("[No Name]");
        let modified = if buffer.modified { " [+]" } else { "" };
        let cursor_count = if cursors.len() > 1 {
            format!(" ({} cursors)", cursors.len())
        } else {
            String::new()
        };
        let left = format!(" {}{}{}", name, modified, cursor_count);

        // Right side: position (and message if any)
        let primary = cursors.primary();
        let pos = format!("Ln {}, Col {}", primary.line + 1, primary.col + 1);
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
