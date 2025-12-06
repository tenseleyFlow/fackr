use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Stdout, Write};

use crate::buffer::Buffer;
use crate::editor::Cursor;

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
    ) -> Result<()> {
        let line_num_width = self.line_number_width(buffer.line_count());
        let text_cols = self.cols as usize - line_num_width - 1; // -1 for separator

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

                // Line content
                if let Some(line) = buffer.line_str(line_idx) {
                    let visible: String = line.chars().take(text_cols).collect();
                    execute!(self.stdout, Print(&visible))?;
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
        self.render_status_bar(buffer, cursor, filename)?;

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

    fn render_status_bar(
        &mut self,
        buffer: &Buffer,
        cursor: &Cursor,
        filename: Option<&str>,
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

        // Right side: position
        let right = format!(" Ln {}, Col {} ", cursor.line + 1, cursor.col + 1);

        // Pad middle
        let padding = self.cols as usize - left.len() - right.len();
        let middle = " ".repeat(padding.max(0));

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
