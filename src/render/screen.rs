use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Stdout, Write};

use crate::buffer::Buffer;
use crate::editor::{Cursor, Position};

/// Terminal screen renderer
pub struct Screen {
    stdout: Stdout,
    pub rows: u16,
    pub cols: u16,
}

impl Screen {
    pub fn new() -> Result<Self> {
        let (cols, rows) = terminal::size()?;
        Ok(Self {
            stdout: stdout(),
            rows,
            cols,
        })
    }

    pub fn enter_raw_mode(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen, Hide)?;
        Ok(())
    }

    pub fn leave_raw_mode(&mut self) -> Result<()> {
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
            execute!(self.stdout, MoveTo(0, row as u16))?;

            if line_idx < buffer.line_count() {
                // Line number
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::DarkGrey),
                    Print(format!("{:>width$} ", line_idx + 1, width = line_num_width)),
                    ResetColor
                )?;

                // Line content with selection highlighting
                if let Some(line) = buffer.line_str(line_idx) {
                    self.render_line_with_selection(
                        &line,
                        line_idx,
                        text_cols,
                        selection.as_ref(),
                    )?;
                }
            } else {
                // Empty line indicator
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::DarkBlue),
                    Print(format!("{:>width$} ", "~", width = line_num_width)),
                    ResetColor
                )?;
            }

            // Clear rest of line
            execute!(self.stdout, Clear(ClearType::UntilNewLine))?;
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
    ) -> Result<()> {
        let chars: Vec<char> = line.chars().take(max_cols).collect();

        if let Some((start, end)) = selection {
            // Check if this line has any selection
            let line_has_selection = line_idx >= start.line && line_idx <= end.line;

            if line_has_selection {
                let sel_start_col = if line_idx == start.line { start.col } else { 0 };
                let sel_end_col = if line_idx == end.line { end.col } else { chars.len() };

                // Render in three parts: before selection, selection, after selection
                // Before selection
                if sel_start_col > 0 {
                    let before: String = chars[..sel_start_col.min(chars.len())].iter().collect();
                    execute!(self.stdout, Print(&before))?;
                }

                // Selection (highlighted)
                if sel_start_col < chars.len() && sel_end_col > sel_start_col {
                    let selected: String = chars[sel_start_col..sel_end_col.min(chars.len())].iter().collect();
                    execute!(
                        self.stdout,
                        SetBackgroundColor(Color::Blue),
                        SetForegroundColor(Color::White),
                        Print(&selected),
                        ResetColor
                    )?;
                }

                // After selection
                if sel_end_col < chars.len() {
                    let after: String = chars[sel_end_col..].iter().collect();
                    execute!(self.stdout, Print(&after))?;
                }
            } else {
                // No selection on this line
                let visible: String = chars.iter().collect();
                execute!(self.stdout, Print(&visible))?;
            }
        } else {
            // No selection at all
            let visible: String = chars.iter().collect();
            execute!(self.stdout, Print(&visible))?;
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
