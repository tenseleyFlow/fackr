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
use crate::fuss::VisibleItem;

// Editor color scheme (256-color palette)
const BG_COLOR: Color = Color::AnsiValue(234);           // Off-black editor background
const CURRENT_LINE_BG: Color = Color::AnsiValue(236);    // Slightly lighter for current line
const LINE_NUM_COLOR: Color = Color::AnsiValue(243);     // Gray for line numbers
const CURRENT_LINE_NUM_COLOR: Color = Color::Yellow;     // Yellow for active line number
const BRACKET_MATCH_BG: Color = Color::AnsiValue(240);   // Highlight for matching brackets
// Secondary cursors use Color::Magenta for visibility

// Tab bar colors
const TAB_BAR_BG: Color = Color::AnsiValue(235);         // Slightly lighter than editor bg
const TAB_ACTIVE_BG: Color = Color::AnsiValue(238);      // Active tab background
const TAB_INACTIVE_FG: Color = Color::AnsiValue(245);    // Inactive tab text
const TAB_ACTIVE_FG: Color = Color::White;               // Active tab text
const TAB_MODIFIED_FG: Color = Color::Yellow;            // Modified indicator

/// Tab information for rendering
pub struct TabInfo {
    pub name: String,
    pub is_active: bool,
    pub is_modified: bool,
    pub index: usize,
}

/// Pane information for rendering
pub struct PaneInfo<'a> {
    pub buffer: &'a Buffer,
    pub cursors: &'a Cursors,
    pub viewport_line: usize,
    pub bounds: PaneBounds,
    pub is_active: bool,
    pub bracket_match: Option<(usize, usize)>,
    pub is_modified: bool,
}

/// Normalized pane bounds (0.0 to 1.0)
#[derive(Debug, Clone)]
pub struct PaneBounds {
    pub x_start: f32,
    pub y_start: f32,
    pub x_end: f32,
    pub y_end: f32,
}

// Pane colors
const PANE_SEPARATOR_FG: Color = Color::AnsiValue(240);
const PANE_ACTIVE_SEPARATOR_FG: Color = Color::AnsiValue(250);
// Inactive pane uses darker colors
const INACTIVE_BG_COLOR: Color = Color::AnsiValue(233);        // Darker than active
const INACTIVE_CURRENT_LINE_BG: Color = Color::AnsiValue(234); // Dimmed current line
const INACTIVE_LINE_NUM_COLOR: Color = Color::AnsiValue(240);  // Dimmed line numbers
const INACTIVE_TEXT_COLOR: Color = Color::AnsiValue(245);      // Dimmed text

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

    /// Render the tab bar
    /// Returns the height of the tab bar (1 if rendered, 0 if only one tab)
    pub fn render_tab_bar(&mut self, tabs: &[TabInfo], left_offset: u16) -> Result<u16> {
        // Only show tab bar if there's more than one tab
        if tabs.len() <= 1 {
            return Ok(0);
        }

        execute!(self.stdout, MoveTo(left_offset, 0))?;

        // Fill the tab bar background
        let available_width = self.cols.saturating_sub(left_offset) as usize;
        execute!(
            self.stdout,
            SetBackgroundColor(TAB_BAR_BG),
            SetForegroundColor(TAB_INACTIVE_FG),
        )?;

        // Calculate max width per tab
        let tab_count = tabs.len();
        let separators = tab_count.saturating_sub(1);
        let available_for_tabs = available_width.saturating_sub(separators);
        let max_tab_width = (available_for_tabs / tab_count).max(3); // At least 3 chars per tab

        let mut current_col = left_offset as usize;

        for (i, tab) in tabs.iter().enumerate() {
            // Build tab label: [index] name [*]
            let index_str = if tab.index < 9 {
                format!("{}", tab.index + 1)
            } else {
                String::new()
            };

            let modified_str = if tab.is_modified { "*" } else { "" };

            // Calculate available space for name
            let prefix_len = if index_str.is_empty() { 0 } else { index_str.len() + 1 }; // "1 "
            let suffix_len = modified_str.len();
            let name_max = max_tab_width.saturating_sub(prefix_len + suffix_len);

            // Truncate name if needed
            let display_name: String = if tab.name.len() > name_max {
                tab.name.chars().take(name_max.saturating_sub(1)).collect::<String>() + "…"
            } else {
                tab.name.clone()
            };

            // Set colors based on active state
            let (bg, fg) = if tab.is_active {
                (TAB_ACTIVE_BG, TAB_ACTIVE_FG)
            } else {
                (TAB_BAR_BG, TAB_INACTIVE_FG)
            };

            execute!(
                self.stdout,
                MoveTo(current_col as u16, 0),
                SetBackgroundColor(bg),
            )?;

            // Print index number (for Alt+N shortcut hint)
            if !index_str.is_empty() {
                execute!(
                    self.stdout,
                    SetForegroundColor(LINE_NUM_COLOR),
                    Print(&index_str),
                    Print(" "),
                )?;
            }

            // Print tab name
            execute!(
                self.stdout,
                SetForegroundColor(fg),
                Print(&display_name),
            )?;

            // Print modified indicator
            if tab.is_modified {
                execute!(
                    self.stdout,
                    SetForegroundColor(TAB_MODIFIED_FG),
                    Print(modified_str),
                )?;
            }

            current_col += prefix_len + display_name.len() + suffix_len;

            // Add separator between tabs
            if i + 1 < tab_count {
                execute!(
                    self.stdout,
                    SetBackgroundColor(TAB_BAR_BG),
                    SetForegroundColor(LINE_NUM_COLOR),
                    Print("│"),
                )?;
                current_col += 1;
            }
        }

        // Fill the rest of the line
        execute!(
            self.stdout,
            SetBackgroundColor(TAB_BAR_BG),
            Clear(ClearType::UntilNewLine),
            ResetColor,
        )?;

        Ok(1)
    }

    /// Render multiple panes with their separators
    /// Returns the position of the hardware cursor (for the active pane)
    pub fn render_panes(
        &mut self,
        panes: &[PaneInfo],
        filename: Option<&str>,
        message: Option<&str>,
        left_offset: u16,
        top_offset: u16,
    ) -> Result<()> {
        execute!(self.stdout, Hide)?;

        // Calculate available screen area
        let available_width = self.cols.saturating_sub(left_offset) as f32;
        let available_height = self.rows.saturating_sub(1 + top_offset) as f32; // -1 for status bar

        // Track where to place the hardware cursor (active pane's primary cursor)
        let mut cursor_screen_pos: Option<(u16, u16)> = None;

        for pane in panes {
            // Convert normalized bounds to screen coordinates
            let pane_x = left_offset + (pane.bounds.x_start * available_width) as u16;
            let pane_y = top_offset + (pane.bounds.y_start * available_height) as u16;
            let pane_width = ((pane.bounds.x_end - pane.bounds.x_start) * available_width) as u16;
            let pane_height = ((pane.bounds.y_end - pane.bounds.y_start) * available_height) as u16;

            // Render this pane
            let cursor_pos = self.render_single_pane(
                pane,
                pane_x,
                pane_y,
                pane_width,
                pane_height,
            )?;

            // Track active pane's cursor position
            if pane.is_active {
                cursor_screen_pos = cursor_pos;
            }

            // Draw separator on the left edge if not at left boundary
            if pane.bounds.x_start > 0.01 {
                let sep_x = pane_x.saturating_sub(1);
                let sep_color = if pane.is_active { PANE_ACTIVE_SEPARATOR_FG } else { PANE_SEPARATOR_FG };
                for row in 0..pane_height {
                    execute!(
                        self.stdout,
                        MoveTo(sep_x, pane_y + row),
                        SetBackgroundColor(BG_COLOR),
                        SetForegroundColor(sep_color),
                        Print("│"),
                    )?;
                }
            }

            // Draw separator on the top edge if not at top boundary
            if pane.bounds.y_start > 0.01 {
                let sep_y = pane_y.saturating_sub(1);
                let sep_color = if pane.is_active { PANE_ACTIVE_SEPARATOR_FG } else { PANE_SEPARATOR_FG };
                for col in 0..pane_width {
                    execute!(
                        self.stdout,
                        MoveTo(pane_x + col, sep_y),
                        SetBackgroundColor(BG_COLOR),
                        SetForegroundColor(sep_color),
                        Print("─"),
                    )?;
                }
            }
        }

        // Render status bar (use active pane's info)
        if let Some(active_pane) = panes.iter().find(|p| p.is_active) {
            self.render_status_bar_with_offset(
                active_pane.cursors,
                filename,
                message,
                left_offset,
                active_pane.is_modified,
            )?;
        }

        // Position hardware cursor
        if let Some((col, row)) = cursor_screen_pos {
            execute!(self.stdout, MoveTo(col, row), Show)?;
        }

        self.stdout.flush()?;
        Ok(())
    }

    /// Render a single pane within its screen bounds
    /// Returns the screen position of the primary cursor if this is the active pane
    fn render_single_pane(
        &mut self,
        pane: &PaneInfo,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<Option<(u16, u16)>> {
        let buffer = pane.buffer;
        let cursors = pane.cursors;
        let is_active = pane.is_active;

        // Choose colors based on active state
        let bg_color = if is_active { BG_COLOR } else { INACTIVE_BG_COLOR };
        let current_line_bg = if is_active { CURRENT_LINE_BG } else { INACTIVE_CURRENT_LINE_BG };
        let line_num_color = if is_active { LINE_NUM_COLOR } else { INACTIVE_LINE_NUM_COLOR };
        let current_line_num_color = if is_active { CURRENT_LINE_NUM_COLOR } else { INACTIVE_LINE_NUM_COLOR };
        let text_color = if is_active { Color::Reset } else { INACTIVE_TEXT_COLOR };

        let line_num_width = self.line_number_width(buffer.line_count());
        let text_cols = (width as usize).saturating_sub(line_num_width + 1);

        let primary = cursors.primary();

        // Collect selections and cursor positions (only show in active pane)
        let selections: Vec<(Position, Position)> = if is_active {
            cursors.all()
                .iter()
                .filter_map(|c| c.selection_bounds())
                .collect()
        } else {
            Vec::new()
        };

        let primary_idx = cursors.primary_index();
        let cursor_positions: Vec<(usize, usize, bool)> = if is_active {
            cursors.all()
                .iter()
                .enumerate()
                .map(|(i, c)| (c.line, c.col, i == primary_idx))
                .collect()
        } else {
            Vec::new()
        };

        // Draw text area
        for row in 0..height as usize {
            let line_idx = pane.viewport_line + row;
            let is_current_line = line_idx == primary.line;
            execute!(self.stdout, MoveTo(x, y + row as u16))?;

            if line_idx < buffer.line_count() {
                let line_num_fg = if is_current_line {
                    current_line_num_color
                } else {
                    line_num_color
                };
                let line_bg = if is_current_line { current_line_bg } else { bg_color };

                execute!(
                    self.stdout,
                    SetBackgroundColor(line_bg),
                    SetForegroundColor(line_num_fg),
                    Print(format!("{:>width$} ", line_idx + 1, width = line_num_width)),
                )?;

                if let Some(line) = buffer.line_str(line_idx) {
                    if is_active {
                        // Active pane: full highlighting
                        let bracket_col = pane.bracket_match
                            .filter(|(bl, _)| *bl == line_idx)
                            .map(|(_, bc)| bc);

                        let secondary_cursors: Vec<usize> = cursor_positions.iter()
                            .filter(|(l, _, is_primary)| *l == line_idx && !*is_primary)
                            .map(|(_, c, _)| *c)
                            .collect();

                        self.render_line_with_cursors_bounded(
                            &line,
                            line_idx,
                            text_cols,
                            &selections,
                            is_current_line,
                            bracket_col,
                            &secondary_cursors,
                        )?;
                    } else {
                        // Inactive pane: simple dimmed text
                        let chars: String = line.chars().take(text_cols).collect();
                        execute!(
                            self.stdout,
                            SetBackgroundColor(line_bg),
                            SetForegroundColor(text_color),
                            Print(&chars),
                        )?;
                    }
                }

                // Fill rest of pane width
                execute!(
                    self.stdout,
                    SetBackgroundColor(line_bg),
                )?;
                let line_len = buffer.line_str(line_idx).map(|l| l.len()).unwrap_or(0);
                let current_col = x + line_num_width as u16 + 1 + text_cols.min(line_len) as u16;
                let remaining = (x + width).saturating_sub(current_col);
                if remaining > 0 {
                    execute!(self.stdout, Print(" ".repeat(remaining as usize)))?;
                }
                execute!(self.stdout, ResetColor)?;
            } else {
                execute!(
                    self.stdout,
                    SetBackgroundColor(bg_color),
                    SetForegroundColor(if is_active { Color::DarkBlue } else { INACTIVE_LINE_NUM_COLOR }),
                    Print(format!("{:>width$} ", "~", width = line_num_width)),
                )?;
                // Fill rest of line within pane bounds
                let remaining = width.saturating_sub(line_num_width as u16 + 1);
                execute!(self.stdout, Print(" ".repeat(remaining as usize)), ResetColor)?;
            }
        }

        // Return cursor position if this is the active pane
        if pane.is_active {
            let cursor_row = primary.line.saturating_sub(pane.viewport_line);
            if cursor_row < height as usize {
                let cursor_screen_row = y + cursor_row as u16;
                let cursor_screen_col = x + line_num_width as u16 + 1 + primary.col as u16;
                return Ok(Some((cursor_screen_col, cursor_screen_row)));
            }
        }

        Ok(None)
    }

    /// Render line with cursors, bounded to a specific width
    fn render_line_with_cursors_bounded(
        &mut self,
        line: &str,
        line_idx: usize,
        max_cols: usize,
        selections: &[(Position, Position)],
        is_current_line: bool,
        bracket_col: Option<usize>,
        secondary_cursors: &[usize],
    ) -> Result<()> {
        // Delegate to existing method - it already handles max_cols
        self.render_line_with_cursors(
            line,
            line_idx,
            max_cols,
            selections,
            is_current_line,
            bracket_col,
            secondary_cursors,
        )
    }

    /// Render the editor view (without offsets - use render_with_offset instead)
    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    /// Render the fuss mode sidebar
    pub fn render_fuss(
        &mut self,
        items: &[VisibleItem],
        selected: usize,
        scroll: usize,
        width: u16,
        hints_expanded: bool,
        repo_name: &str,
        branch: Option<&str>,
    ) -> Result<()> {
        let width = width as usize;
        let text_rows = self.rows.saturating_sub(1) as usize;
        let hint_rows = if hints_expanded { 4 } else { 1 };
        let header_rows = 2; // Header line + separator
        let tree_rows = text_rows.saturating_sub(hint_rows + header_rows);

        // Draw header: repo_name:branch
        execute!(self.stdout, MoveTo(0, 0))?;
        let header_text = if let Some(b) = branch {
            format!("{}:{}", repo_name, b)
        } else {
            repo_name.to_string()
        };
        let truncated: String = header_text.chars().take(width.saturating_sub(1)).collect();
        let padded = format!("{:<width$}", truncated, width = width);

        // Render header with cyan repo name, yellow branch
        execute!(
            self.stdout,
            SetBackgroundColor(BG_COLOR),
            SetForegroundColor(Color::Cyan),
        )?;
        if let Some(b) = branch {
            let repo_display: String = repo_name.chars().take(width.saturating_sub(1)).collect();
            execute!(self.stdout, Print(&repo_display))?;
            execute!(
                self.stdout,
                SetForegroundColor(Color::DarkGrey),
                Print(":"),
                SetForegroundColor(Color::Yellow),
            )?;
            let remaining = width.saturating_sub(repo_display.len() + 1);
            let branch_display: String = b.chars().take(remaining).collect();
            let branch_padded = format!("{:<width$}", branch_display, width = remaining);
            execute!(self.stdout, Print(&branch_padded))?;
        } else {
            execute!(self.stdout, Print(&padded))?;
        }
        execute!(self.stdout, ResetColor)?;

        // Draw separator
        execute!(self.stdout, MoveTo(0, 1))?;
        let separator = "─".repeat(width);
        execute!(
            self.stdout,
            SetBackgroundColor(BG_COLOR),
            SetForegroundColor(Color::DarkGrey),
            Print(&separator),
            ResetColor,
        )?;

        // Draw file tree (starting after header)
        for row in 0..tree_rows {
            let screen_row = (row + header_rows) as u16;
            execute!(self.stdout, MoveTo(0, screen_row))?;

            let item_idx = scroll + row;
            if item_idx < items.len() {
                let item = &items[item_idx];
                let is_selected = item_idx == selected;

                // Build git status indicator
                let git_indicator = if item.git_status.staged {
                    " \x1b[32m↑\x1b[0m" // Green up arrow
                } else if item.git_status.unstaged {
                    " \x1b[31m✗\x1b[0m" // Red X
                } else if item.git_status.untracked {
                    " \x1b[90m?\x1b[0m" // Gray question mark
                } else if item.git_status.incoming {
                    " \x1b[34m↓\x1b[0m" // Blue down arrow
                } else {
                    ""
                };

                // Build display line
                let indent = "  ".repeat(item.depth.saturating_sub(1));
                let icon = if item.is_dir {
                    if item.expanded { "- " } else { "+ " }
                } else {
                    "  "
                };
                let suffix = if item.is_dir { "/" } else { "" };

                // Calculate space for name (leave room for git indicator)
                let prefix_len = indent.len() + icon.len();
                let indicator_display_len = if git_indicator.is_empty() { 0 } else { 2 }; // " X"
                let name_max = width.saturating_sub(prefix_len + suffix.len() + indicator_display_len);
                let name_truncated: String = item.name.chars().take(name_max).collect();

                let display_base = format!("{}{}{}{}", indent, icon, name_truncated, suffix);

                if is_selected {
                    // Highlight selected - need to handle git indicator specially
                    let padded_len = width.saturating_sub(indicator_display_len);
                    let padded = format!("{:<width$}", display_base, width = padded_len);
                    execute!(
                        self.stdout,
                        SetBackgroundColor(Color::DarkGrey),
                        SetForegroundColor(Color::White),
                        Print(&padded),
                    )?;
                    if !git_indicator.is_empty() {
                        // Git indicator with selection background
                        if item.git_status.staged {
                            execute!(self.stdout, SetForegroundColor(Color::Green), Print(" ↑"))?;
                        } else if item.git_status.unstaged {
                            execute!(self.stdout, SetForegroundColor(Color::Red), Print(" ✗"))?;
                        } else if item.git_status.untracked {
                            execute!(self.stdout, SetForegroundColor(Color::DarkGrey), Print(" ?"))?;
                        } else if item.git_status.incoming {
                            execute!(self.stdout, SetForegroundColor(Color::Blue), Print(" ↓"))?;
                        }
                    }
                    execute!(self.stdout, ResetColor)?;
                } else if item.is_dir {
                    // Directories in blue
                    let padded_len = width.saturating_sub(indicator_display_len);
                    let padded = format!("{:<width$}", display_base, width = padded_len);
                    execute!(
                        self.stdout,
                        SetBackgroundColor(BG_COLOR),
                        SetForegroundColor(Color::Blue),
                        Print(&padded),
                        ResetColor
                    )?;
                } else if item.git_status.gitignored {
                    // Gitignored files in dark gray
                    let padded = format!("{:<width$}", display_base, width = width);
                    execute!(
                        self.stdout,
                        SetBackgroundColor(BG_COLOR),
                        SetForegroundColor(Color::DarkGrey),
                        Print(&padded),
                        ResetColor
                    )?;
                } else {
                    // Files in default color with git status
                    let padded_len = width.saturating_sub(indicator_display_len);
                    let padded = format!("{:<width$}", display_base, width = padded_len);
                    execute!(
                        self.stdout,
                        SetBackgroundColor(BG_COLOR),
                        SetForegroundColor(Color::Reset),
                        Print(&padded),
                    )?;
                    // Add git status indicator
                    if item.git_status.staged {
                        execute!(self.stdout, SetForegroundColor(Color::Green), Print(" ↑"))?;
                    } else if item.git_status.unstaged {
                        execute!(self.stdout, SetForegroundColor(Color::Red), Print(" ✗"))?;
                    } else if item.git_status.untracked {
                        execute!(self.stdout, SetForegroundColor(Color::DarkGrey), Print(" ?"))?;
                    } else if item.git_status.incoming {
                        execute!(self.stdout, SetForegroundColor(Color::Blue), Print(" ↓"))?;
                    }
                    execute!(self.stdout, ResetColor)?;
                }
            } else {
                // Empty row
                let empty = " ".repeat(width);
                execute!(
                    self.stdout,
                    SetBackgroundColor(BG_COLOR),
                    Print(&empty),
                    ResetColor
                )?;
            }
        }

        // Draw hints at bottom (after header + tree)
        let hint_start = header_rows + tree_rows;
        if hints_expanded {
            let hints = [
                "j/k:nav spc:toggle o:open .:hidden",
                "a:stage u:unstage d:diff m:commit",
                "p:push l:pull f:fetch t:tag",
                "ctrl-b:close ctrl-/:hints",
            ];
            for (i, hint) in hints.iter().enumerate() {
                if hint_start + i < text_rows {
                    execute!(self.stdout, MoveTo(0, (hint_start + i) as u16))?;
                    let padded = format!("{:<width$}", hint, width = width);
                    execute!(
                        self.stdout,
                        SetBackgroundColor(BG_COLOR),
                        SetForegroundColor(Color::DarkGrey),
                        Print(&padded),
                        ResetColor
                    )?;
                }
            }
        } else {
            if hint_start < text_rows {
                execute!(self.stdout, MoveTo(0, hint_start as u16))?;
                let hint = "ctrl-/:hints";
                let padded = format!("{:<width$}", hint, width = width);
                execute!(
                    self.stdout,
                    SetBackgroundColor(BG_COLOR),
                    SetForegroundColor(Color::DarkGrey),
                    Print(&padded),
                    ResetColor
                )?;
            }
        }

        Ok(())
    }

    /// Render the editor view with horizontal and vertical offsets (for fuss mode and tab bar)
    pub fn render_with_offset(
        &mut self,
        buffer: &Buffer,
        cursors: &Cursors,
        viewport_line: usize,
        filename: Option<&str>,
        message: Option<&str>,
        bracket_match: Option<(usize, usize)>,
        left_offset: u16,
        top_offset: u16,
        is_modified: bool,
    ) -> Result<()> {
        // Hide cursor during render to prevent flicker
        execute!(self.stdout, Hide)?;

        let available_cols = self.cols.saturating_sub(left_offset) as usize;
        let line_num_width = self.line_number_width(buffer.line_count());
        let text_cols = available_cols.saturating_sub(line_num_width + 1);

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
            .map(|(i, c)| (c.line, c.col, i == primary_idx))
            .collect();

        // Reserve 1 row for status bar, accounting for top offset
        let text_rows = self.rows.saturating_sub(1 + top_offset) as usize;

        // Draw text area
        for row in 0..text_rows {
            let line_idx = viewport_line + row;
            let is_current_line = line_idx == primary.line;
            execute!(self.stdout, MoveTo(left_offset, (row as u16) + top_offset))?;

            if line_idx < buffer.line_count() {
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

                if let Some(line) = buffer.line_str(line_idx) {
                    let bracket_col = bracket_match
                        .filter(|(bl, _)| *bl == line_idx)
                        .map(|(_, bc)| bc);

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

                execute!(
                    self.stdout,
                    SetBackgroundColor(line_bg),
                    Clear(ClearType::UntilNewLine),
                    ResetColor
                )?;
            } else {
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
        self.render_status_bar_with_offset(cursors, filename, message, left_offset, is_modified)?;

        // Position hardware cursor at primary cursor
        let cursor_row = (primary.line.saturating_sub(viewport_line) as u16) + top_offset;
        let cursor_col = left_offset as usize + line_num_width + 1 + primary.col;
        execute!(
            self.stdout,
            MoveTo(cursor_col as u16, cursor_row),
            Show
        )?;

        self.stdout.flush()?;
        Ok(())
    }

    fn render_status_bar_with_offset(
        &mut self,
        cursors: &Cursors,
        filename: Option<&str>,
        message: Option<&str>,
        offset: u16,
        is_modified: bool,
    ) -> Result<()> {
        let status_row = self.rows.saturating_sub(1);
        let available_cols = self.cols.saturating_sub(offset) as usize;
        execute!(self.stdout, MoveTo(offset, status_row))?;

        execute!(
            self.stdout,
            SetBackgroundColor(Color::DarkGrey),
            SetForegroundColor(Color::White)
        )?;

        let name = filename.unwrap_or("[No Name]");
        let modified = if is_modified { " [+]" } else { "" };
        let cursor_count = if cursors.len() > 1 {
            format!(" ({} cursors)", cursors.len())
        } else {
            String::new()
        };
        let left = format!(" {}{}{}", name, modified, cursor_count);

        let primary = cursors.primary();
        let pos = format!("Ln {}, Col {}", primary.line + 1, primary.col + 1);
        let right = if let Some(msg) = message {
            format!(" {} | {} ", msg, pos)
        } else {
            format!(" {} ", pos)
        };

        let padding = available_cols.saturating_sub(left.len() + right.len());
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

    /// Render the welcome menu
    pub fn render_welcome(
        &mut self,
        items: &[(String, String, bool, bool)], // (label, path, is_selected, is_current_dir)
        scroll: usize,
    ) -> Result<()> {
        execute!(self.stdout, Hide)?;

        let cols = self.cols as usize;
        let rows = self.rows as usize;

        // Fill background
        for row in 0..rows {
            execute!(
                self.stdout,
                MoveTo(0, row as u16),
                SetBackgroundColor(BG_COLOR),
                Clear(ClearType::UntilNewLine),
            )?;
        }

        // Calculate box dimensions
        let box_width = cols.min(60).max(40);
        let box_height = rows.saturating_sub(4).min(items.len() + 6).max(10);
        let box_x = (cols.saturating_sub(box_width)) / 2;
        let box_y = (rows.saturating_sub(box_height)) / 2;

        // Draw box border
        let top_border = format!("╭{}╮", "─".repeat(box_width.saturating_sub(2)));
        let bottom_border = format!("╰{}╯", "─".repeat(box_width.saturating_sub(2)));

        execute!(
            self.stdout,
            MoveTo(box_x as u16, box_y as u16),
            SetBackgroundColor(BG_COLOR),
            SetForegroundColor(Color::DarkGrey),
            Print(&top_border),
        )?;

        // Title
        let title = "Welcome to fackr";
        let title_row = box_y + 1;
        let title_x = box_x + (box_width.saturating_sub(title.len())) / 2;
        execute!(
            self.stdout,
            MoveTo(box_x as u16, title_row as u16),
            SetForegroundColor(Color::DarkGrey),
            Print("│"),
            SetForegroundColor(Color::White),
        )?;
        let padding_left = title_x.saturating_sub(box_x + 1);
        let padding_right = box_width.saturating_sub(2).saturating_sub(padding_left + title.len());
        execute!(
            self.stdout,
            Print(&" ".repeat(padding_left)),
            Print(title),
            Print(&" ".repeat(padding_right)),
            SetForegroundColor(Color::DarkGrey),
            Print("│"),
        )?;

        // Subtitle
        let subtitle = "Select a workspace:";
        let subtitle_row = box_y + 2;
        execute!(
            self.stdout,
            MoveTo(box_x as u16, subtitle_row as u16),
            SetForegroundColor(Color::DarkGrey),
            Print("│"),
            SetForegroundColor(Color::AnsiValue(245)),
        )?;
        let padding_left = (box_width.saturating_sub(2).saturating_sub(subtitle.len())) / 2;
        let padding_right = box_width.saturating_sub(2).saturating_sub(padding_left + subtitle.len());
        execute!(
            self.stdout,
            Print(&" ".repeat(padding_left)),
            Print(subtitle),
            Print(&" ".repeat(padding_right)),
            SetForegroundColor(Color::DarkGrey),
            Print("│"),
        )?;

        // Separator
        let separator_row = box_y + 3;
        execute!(
            self.stdout,
            MoveTo(box_x as u16, separator_row as u16),
            SetForegroundColor(Color::DarkGrey),
            Print("├"),
            Print(&"─".repeat(box_width.saturating_sub(2))),
            Print("┤"),
        )?;

        // Item list area
        let list_start_row = box_y + 4;
        let list_height = box_height.saturating_sub(6);
        let inner_width = box_width.saturating_sub(4);

        for i in 0..list_height {
            let row = list_start_row + i;
            let item_idx = scroll + i;

            execute!(
                self.stdout,
                MoveTo(box_x as u16, row as u16),
                SetForegroundColor(Color::DarkGrey),
                Print("│ "),
            )?;

            if item_idx < items.len() {
                let (label, _path, is_selected, is_current_dir) = &items[item_idx];

                // Truncate label to fit
                let display_label: String = label.chars().take(inner_width).collect();
                let padded = format!("{:<width$}", display_label, width = inner_width);

                if *is_selected {
                    execute!(
                        self.stdout,
                        SetBackgroundColor(Color::DarkGrey),
                        SetForegroundColor(Color::White),
                        Print(&padded),
                        SetBackgroundColor(BG_COLOR),
                    )?;
                } else if *is_current_dir {
                    execute!(
                        self.stdout,
                        SetForegroundColor(Color::Cyan),
                        Print(&padded),
                    )?;
                } else {
                    execute!(
                        self.stdout,
                        SetForegroundColor(Color::Reset),
                        Print(&padded),
                    )?;
                }

                // Show path hint for selected item
                if *is_selected && inner_width > 30 {
                    // Clear and show path below
                }
            } else {
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::Reset),
                    Print(&" ".repeat(inner_width)),
                )?;
            }

            execute!(
                self.stdout,
                SetForegroundColor(Color::DarkGrey),
                Print(" │"),
            )?;
        }

        // Path display row (show selected path)
        let path_row = list_start_row + list_height;
        execute!(
            self.stdout,
            MoveTo(box_x as u16, path_row as u16),
            SetForegroundColor(Color::DarkGrey),
            Print("├"),
            Print(&"─".repeat(box_width.saturating_sub(2))),
            Print("┤"),
        )?;

        // Show selected path
        let selected_item = items.iter().find(|(_, _, sel, _)| *sel);
        let path_display_row = path_row + 1;
        execute!(
            self.stdout,
            MoveTo(box_x as u16, path_display_row as u16),
            SetForegroundColor(Color::DarkGrey),
            Print("│ "),
        )?;
        if let Some((_, path, _, _)) = selected_item {
            let truncated_path: String = path.chars().take(inner_width).collect();
            let padded_path = format!("{:<width$}", truncated_path, width = inner_width);
            execute!(
                self.stdout,
                SetForegroundColor(Color::AnsiValue(240)),
                Print(&padded_path),
            )?;
        } else {
            execute!(
                self.stdout,
                Print(&" ".repeat(inner_width)),
            )?;
        }
        execute!(
            self.stdout,
            SetForegroundColor(Color::DarkGrey),
            Print(" │"),
        )?;

        // Bottom border
        let bottom_row = path_display_row + 1;
        execute!(
            self.stdout,
            MoveTo(box_x as u16, bottom_row as u16),
            SetForegroundColor(Color::DarkGrey),
            Print(&bottom_border),
        )?;

        // Hints at bottom
        let hint_row = bottom_row + 1;
        let hints = "↑/↓: navigate  Enter: select  ESC: quit";
        let hints_x = (cols.saturating_sub(hints.len())) / 2;
        execute!(
            self.stdout,
            MoveTo(hints_x as u16, hint_row as u16),
            SetForegroundColor(Color::AnsiValue(240)),
            Print(hints),
            ResetColor,
        )?;

        self.stdout.flush()?;
        Ok(())
    }
}
