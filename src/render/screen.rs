use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        DisableMouseCapture, EnableMouseCapture,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Stdout, Write};
use unicode_width::UnicodeWidthStr;

use crate::buffer::Buffer;
use crate::editor::{Cursors, Position};
use crate::fuss::VisibleItem;
use crate::lsp::{CompletionItem, Diagnostic, DiagnosticSeverity, HoverInfo, Location, ServerManagerPanel};
use crate::syntax::{Highlighter, Token};
use crate::terminal::TerminalPanel;

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

/// Extract the last component of a path for display
fn extract_dirname(path: &str) -> String {
    // Handle home directory
    if path == "/" {
        return "/".to_string();
    }

    // Get the last path component
    path.rsplit('/')
        .find(|s| !s.is_empty())
        .map(|s| {
            // If it starts with ~, keep it
            if path.starts_with('~') || path == "/" {
                s.to_string()
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| path.to_string())
}

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
        // This enables the kitty keyboard protocol on supporting terminals.
        // We use REPORT_ALTERNATE_KEYS so crossterm receives the shifted character
        // (e.g., 'A' instead of 'a' with shift modifier) for consistent behavior.
        // See: https://github.com/helix-editor/helix/pull/4939
        if execute!(
            self.stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
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

    /// Position and show the hardware cursor at the given screen coordinates
    pub fn show_cursor_at(&mut self, col: u16, row: u16) -> Result<()> {
        execute!(self.stdout, MoveTo(col, row), Show)?;
        self.stdout.flush()?;
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

        // Reserve 2 rows: 1 for gap above status bar, 1 for status bar itself
        let text_rows = self.rows.saturating_sub(2) as usize;

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

        // Render the gap row (empty line between text and status bar)
        let gap_row = text_rows as u16;
        execute!(
            self.stdout,
            MoveTo(0, gap_row),
            SetBackgroundColor(BG_COLOR),
            Clear(ClearType::UntilNewLine),
            ResetColor
        )?;

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
        // Call the syntax-aware version with no tokens
        self.render_line_with_syntax(
            line,
            line_idx,
            max_cols,
            selections,
            is_current_line,
            bracket_col,
            secondary_cursors,
            &[],
        )
    }

    fn render_line_with_syntax(
        &mut self,
        line: &str,
        line_idx: usize,
        max_cols: usize,
        selections: &[(Position, Position)],
        is_current_line: bool,
        bracket_col: Option<usize>,
        secondary_cursors: &[usize],
        tokens: &[Token],
    ) -> Result<()> {
        let line_bg = if is_current_line { CURRENT_LINE_BG } else { BG_COLOR };
        let default_fg = Color::Reset; // Default terminal foreground

        // Pre-compute selection ranges for this line (small fixed array to avoid allocation)
        // Most users have at most a few cursors with selections
        let mut sel_start: [usize; 8] = [0; 8];
        let mut sel_end: [usize; 8] = [0; 8];
        let mut sel_count = 0;
        for (start, end) in selections {
            if line_idx >= start.line && line_idx <= end.line && sel_count < 8 {
                sel_start[sel_count] = if line_idx == start.line { start.col } else { 0 };
                sel_end[sel_count] = if line_idx == end.line { end.col } else { usize::MAX };
                if sel_start[sel_count] < sel_end[sel_count] {
                    sel_count += 1;
                }
            }
        }

        // Track current token index for efficient lookup (tokens are sorted by position)
        let mut current_token_idx = 0;

        // Count characters rendered for end-of-line cursor handling
        let mut char_count = 0;

        // Render character by character for precise highlighting
        for (col, ch) in line.chars().enumerate() {
            if col >= max_cols {
                break;
            }
            char_count = col + 1;

            // Check selection (inline check against fixed array)
            let in_selection = (0..sel_count).any(|i| col >= sel_start[i] && col < sel_end[i]);
            let is_bracket_match = bracket_col == Some(col);
            let is_secondary_cursor = secondary_cursors.contains(&col);

            // Advance token index if needed (tokens are sorted by start position)
            while current_token_idx < tokens.len() && tokens[current_token_idx].end <= col {
                current_token_idx += 1;
            }

            // Get current token if any
            let current_token = if current_token_idx < tokens.len() {
                let t = &tokens[current_token_idx];
                if col >= t.start && col < t.end {
                    Some(t)
                } else {
                    None
                }
            } else {
                None
            };

            // Determine background color (priority: selection > cursor > bracket > syntax/line)
            let bg = if in_selection {
                Color::Blue
            } else if is_secondary_cursor {
                Color::Magenta
            } else if is_bracket_match {
                BRACKET_MATCH_BG
            } else {
                line_bg
            };

            // Determine foreground color and boldness
            let (fg, bold) = if in_selection {
                (Color::White, false)
            } else if is_secondary_cursor {
                (Color::White, false)
            } else if let Some(token) = current_token {
                (token.token_type.color(), token.token_type.bold())
            } else {
                (default_fg, false)
            };

            // Apply styling
            if bold {
                execute!(
                    self.stdout,
                    SetBackgroundColor(bg),
                    SetForegroundColor(fg),
                    SetAttribute(Attribute::Bold),
                    Print(ch),
                    SetAttribute(Attribute::NoBold),
                )?;
            } else {
                execute!(
                    self.stdout,
                    SetBackgroundColor(bg),
                    SetForegroundColor(fg),
                    Print(ch)
                )?;
            }
        }

        // Reset to line background for rest of line
        execute!(self.stdout, SetBackgroundColor(line_bg), SetForegroundColor(default_fg))?;

        // Handle secondary cursors at end of line (past text content)
        let max_cursor_past_text = secondary_cursors.iter()
            .filter(|&&c| c >= char_count)
            .max()
            .copied();

        if let Some(max_cursor) = max_cursor_past_text {
            if max_cursor < max_cols {
                for col in char_count..=max_cursor {
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

        // Right side: help hint, position, and message if any
        let primary = cursors.primary();
        let pos = format!("Ln {}, Col {}", primary.line + 1, primary.col + 1);
        let right = if let Some(msg) = message {
            format!(" {} | Shift+F1: Help | {} ", msg, pos)
        } else {
            format!(" Shift+F1: Help | {} ", pos)
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

    pub fn line_number_width(&self, line_count: usize) -> usize {
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
        git_mode: bool,
    ) -> Result<()> {
        let width = width as usize;
        let text_rows = self.rows.saturating_sub(1) as usize;
        let hint_rows = if hints_expanded { 4 } else { 1 };
        // Header line + separator + optional git mode line
        let header_rows = if git_mode { 3 } else { 2 };
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

        // Draw git mode indicator line
        if git_mode {
            let git_row = 2u16;
            execute!(self.stdout, MoveTo(0, git_row))?;
            let git_hint = "Git: a/u/d/m/p/l/f/t";
            let padded = format!("{:<width$}", git_hint, width = width);
            execute!(
                self.stdout,
                SetBackgroundColor(Color::AnsiValue(235)),
                SetForegroundColor(Color::Yellow),
                Print(&padded),
                ResetColor,
            )?;
        }

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
                "type:jump  spc:toggle  enter:open",
                "alt-.:hidden  alt-g:git  ctrl-v/s:split",
                "ctrl-b:close  ctrl-/:hints",
                "",
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
    #[allow(dead_code)]
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

        // Reserve 2 rows: 1 for gap above status bar, 1 for status bar itself
        let text_rows = self.rows.saturating_sub(2 + top_offset) as usize;

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

        // Render the gap row (empty line between text and status bar)
        let gap_row = text_rows as u16 + top_offset;
        execute!(
            self.stdout,
            MoveTo(left_offset, gap_row),
            SetBackgroundColor(BG_COLOR),
            Clear(ClearType::UntilNewLine),
            ResetColor
        )?;

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

    /// Render the editor view with syntax highlighting
    pub fn render_with_syntax(
        &mut self,
        buffer: &Buffer,
        cursors: &Cursors,
        viewport_line: usize,
        viewport_col: usize,
        filename: Option<&str>,
        message: Option<&str>,
        bracket_match: Option<(usize, usize)>,
        left_offset: u16,
        top_offset: u16,
        is_modified: bool,
        highlighter: &mut Highlighter,
    ) -> Result<()> {
        execute!(self.stdout, Hide)?;

        let available_cols = self.cols.saturating_sub(left_offset) as usize;
        let line_num_width = self.line_number_width(buffer.line_count());
        let text_cols = available_cols.saturating_sub(line_num_width + 1);

        let primary = cursors.primary();

        // Adjust selections for horizontal scroll
        let selections: Vec<(Position, Position)> = cursors.all()
            .iter()
            .filter_map(|c| c.selection_bounds())
            .map(|(start, end)| {
                (
                    Position { line: start.line, col: start.col.saturating_sub(viewport_col) },
                    Position { line: end.line, col: end.col.saturating_sub(viewport_col) },
                )
            })
            .collect();

        let primary_idx = cursors.primary_index();
        // Adjust cursor positions for horizontal scroll
        let cursor_positions: Vec<(usize, usize, bool)> = cursors.all()
            .iter()
            .enumerate()
            .map(|(i, c)| (c.line, c.col.saturating_sub(viewport_col), i == primary_idx))
            .collect();

        // Reserve 2 rows: 1 for gap above status bar, 1 for status bar itself
        let text_rows = self.rows.saturating_sub(2 + top_offset) as usize;

        // Get the starting highlight state for the viewport using the cache.
        // Only tokenize lines from the last cached point if needed.
        let cache_valid = highlighter.cache_valid_from();
        let start_line = cache_valid.min(viewport_line);
        let mut highlight_state = highlighter.get_state_for_line(start_line);

        // Build cache from last valid point up to viewport (only if needed)
        for line_idx in start_line..viewport_line {
            if let Some(line) = buffer.line_str(line_idx) {
                let _ = highlighter.tokenize_line(&line, &mut highlight_state);
                highlighter.update_cache(line_idx, &highlight_state);
            }
        }

        // Draw text area with syntax highlighting
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
                    // Tokenize this line and update cache
                    let tokens = highlighter.tokenize_line(&line, &mut highlight_state);
                    highlighter.update_cache(line_idx, &highlight_state);

                    // Apply horizontal scroll to bracket match column
                    // Only show if the bracket is in the visible area
                    let bracket_col = bracket_match
                        .filter(|(bl, bc)| *bl == line_idx && *bc >= viewport_col)
                        .map(|(_, bc)| bc - viewport_col);

                    let secondary_cursors: Vec<usize> = cursor_positions.iter()
                        .filter(|(l, _, is_primary)| *l == line_idx && !*is_primary)
                        .map(|(_, c, _)| *c)
                        .collect();

                    // Skip characters before viewport_col
                    let display_line: String = line.chars().skip(viewport_col).collect();

                    // Adjust tokens for horizontal scroll
                    let adjusted_tokens: Vec<Token> = tokens.iter()
                        .filter_map(|t| {
                            let new_start = t.start.saturating_sub(viewport_col);
                            let new_end = t.end.saturating_sub(viewport_col);
                            if t.end <= viewport_col {
                                None // Token is entirely before viewport
                            } else {
                                Some(Token {
                                    start: new_start,
                                    end: new_end,
                                    token_type: t.token_type,
                                })
                            }
                        })
                        .collect();

                    self.render_line_with_syntax(
                        &display_line,
                        line_idx,
                        text_cols,
                        &selections,
                        is_current_line,
                        bracket_col,
                        &secondary_cursors,
                        &adjusted_tokens,
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

        // Render the gap row (empty line between text and status bar)
        let gap_row = text_rows as u16 + top_offset;
        execute!(
            self.stdout,
            MoveTo(left_offset, gap_row),
            SetBackgroundColor(BG_COLOR),
            Clear(ClearType::UntilNewLine),
            ResetColor
        )?;

        // Status bar
        self.render_status_bar_with_offset(cursors, filename, message, left_offset, is_modified)?;

        // Position hardware cursor (adjusted for horizontal scroll)
        let cursor_row = (primary.line.saturating_sub(viewport_line) as u16) + top_offset;
        let cursor_col = left_offset as usize + line_num_width + 1 + primary.col.saturating_sub(viewport_col);
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
            format!(" {} | Shift+F1: Help | {} ", msg, pos)
        } else {
            format!(" Shift+F1: Help | {} ", pos)
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

    /// Render a completion popup at the given screen position
    pub fn render_completion_popup(
        &mut self,
        completions: &[CompletionItem],
        selected_index: usize,
        cursor_row: u16,
        cursor_col: u16,
        left_offset: u16,
    ) -> Result<()> {
        if completions.is_empty() {
            return Ok(());
        }

        // Popup settings
        let max_items = 10.min(completions.len());
        let popup_width = 40;
        let popup_bg = Color::AnsiValue(237);
        let selected_bg = Color::AnsiValue(24);
        let item_fg = Color::AnsiValue(252);
        let detail_fg = Color::AnsiValue(244);

        // Position popup below cursor, or above if not enough space
        let popup_row = if cursor_row + (max_items as u16) + 2 < self.rows {
            cursor_row + 1
        } else {
            cursor_row.saturating_sub(max_items as u16 + 1)
        };

        let popup_col = (cursor_col + left_offset).min(self.cols.saturating_sub(popup_width as u16));

        // Calculate scroll offset to keep selection visible
        let scroll_offset = if selected_index >= max_items {
            selected_index - max_items + 1
        } else {
            0
        };

        // Draw border and items
        for (i, item) in completions.iter().skip(scroll_offset).take(max_items).enumerate() {
            let row = popup_row + i as u16;
            let is_selected = i + scroll_offset == selected_index;
            let bg = if is_selected { selected_bg } else { popup_bg };

            execute!(
                self.stdout,
                MoveTo(popup_col, row),
                SetBackgroundColor(bg),
                SetForegroundColor(item_fg),
            )?;

            // Format: [icon] label   detail
            let icon = item.kind.map(|k| k.icon()).unwrap_or(" ");
            let label = &item.label;
            let detail = item.detail.as_deref().unwrap_or("");

            let label_width = popup_width - 4;
            let truncated_label: String = if label.len() > label_width - 2 {
                format!("{}...", &label[..label_width - 5])
            } else {
                label.clone()
            };

            write!(self.stdout, " {} ", icon)?;
            write!(self.stdout, "{:<width$}", truncated_label, width = label_width - detail.len().min(15))?;

            if !detail.is_empty() {
                execute!(self.stdout, SetForegroundColor(detail_fg))?;
                let truncated_detail: String = if detail.len() > 12 {
                    format!("{}...", &detail[..9])
                } else {
                    detail.to_string()
                };
                write!(self.stdout, "{}", truncated_detail)?;
            }

            // Clear to popup width
            execute!(self.stdout, ResetColor)?;
        }

        // Show scroll indicator if needed
        if completions.len() > max_items {
            let indicator_row = popup_row + max_items as u16;
            execute!(
                self.stdout,
                MoveTo(popup_col, indicator_row),
                SetBackgroundColor(popup_bg),
                SetForegroundColor(detail_fg),
                Print(format!(" {}/{} items ", selected_index + 1, completions.len())),
                ResetColor,
            )?;
        }

        Ok(())
    }

    /// Render diagnostics in the gutter or inline
    pub fn render_diagnostics_gutter(
        &mut self,
        diagnostics: &[Diagnostic],
        viewport_line: usize,
        left_offset: u16,
        top_offset: u16,
    ) -> Result<()> {
        // Match text_rows calculation from render functions
        let text_rows = self.rows.saturating_sub(2 + top_offset) as usize;

        for diagnostic in diagnostics {
            let line = diagnostic.range.start.line as usize;

            // Only render if in visible viewport
            if line >= viewport_line && line < viewport_line + text_rows {
                let row = (line - viewport_line) as u16 + top_offset;

                // Determine color based on severity
                let color = match diagnostic.severity {
                    Some(DiagnosticSeverity::Error) => Color::Red,
                    Some(DiagnosticSeverity::Warning) => Color::Yellow,
                    Some(DiagnosticSeverity::Information) => Color::Blue,
                    Some(DiagnosticSeverity::Hint) => Color::Cyan,
                    None => Color::Yellow,
                };

                // Draw indicator at the start of the line (before line number)
                execute!(
                    self.stdout,
                    MoveTo(left_offset, row),
                    SetForegroundColor(color),
                    Print("●"),
                    ResetColor,
                )?;
            }
        }

        Ok(())
    }

    /// Render a hover info popup at the given screen position
    pub fn render_hover_popup(
        &mut self,
        hover: &HoverInfo,
        cursor_row: u16,
        cursor_col: u16,
        left_offset: u16,
    ) -> Result<()> {
        let (width, height) = (self.cols, self.rows);

        // Split content into lines
        let lines: Vec<&str> = hover.contents.lines().collect();
        if lines.is_empty() {
            return Ok(());
        }

        // Calculate popup dimensions
        let max_popup_width = (width as usize).saturating_sub(left_offset as usize + 4).min(80);
        let popup_width = lines
            .iter()
            .map(|l| l.len().min(max_popup_width))
            .max()
            .unwrap_or(20)
            .max(20);
        let max_popup_height = (height as usize).saturating_sub(4).min(15);
        let popup_height = lines.len().min(max_popup_height);

        // Determine position - prefer above cursor, but go below if needed
        let (popup_row, show_above) = if cursor_row as usize >= popup_height + 2 {
            (cursor_row.saturating_sub(popup_height as u16 + 1), true)
        } else {
            (cursor_row + 1, false)
        };

        let popup_col = cursor_col.max(left_offset);

        // Ensure popup fits on screen
        let popup_col = if popup_col as usize + popup_width + 2 > width as usize {
            (width as usize).saturating_sub(popup_width + 3) as u16
        } else {
            popup_col
        };

        // Draw popup border and content
        for (i, line) in lines.iter().take(popup_height).enumerate() {
            let row = popup_row + i as u16;

            // Background and border
            execute!(
                self.stdout,
                MoveTo(popup_col, row),
                SetBackgroundColor(Color::AnsiValue(238)),
                SetForegroundColor(Color::White),
            )?;

            // Truncate line if needed
            let display_line: String = if line.len() > popup_width {
                format!(" {}... ", &line[..popup_width.saturating_sub(4)])
            } else {
                format!(" {:width$} ", line, width = popup_width)
            };

            execute!(self.stdout, Print(&display_line), ResetColor)?;
        }

        // Show indicator if content is truncated
        if lines.len() > popup_height {
            let row = popup_row + popup_height as u16;
            execute!(
                self.stdout,
                MoveTo(popup_col, row),
                SetBackgroundColor(Color::AnsiValue(238)),
                SetForegroundColor(Color::DarkGrey),
                Print(format!(" [{} more lines] ", lines.len() - popup_height)),
                ResetColor
            )?;
        }

        // Hide cursor position indicator
        let _ = show_above; // suppress unused warning

        Ok(())
    }

    /// Render a centered rename modal dialog
    pub fn render_rename_modal(&mut self, original_name: &str, new_name: &str) -> Result<()> {
        let (width, height) = (self.cols, self.rows);

        // Calculate modal dimensions
        let title = "Rename Symbol";
        let from_label = "From: ";
        let to_label = "To:   ";
        let content_width = original_name.len().max(new_name.len()).max(20).max(title.len());
        let modal_width = content_width + 8; // padding + border
        let modal_height = 6; // title + from + to + bottom border + padding

        // Center the modal
        let start_col = ((width as usize).saturating_sub(modal_width)) / 2;
        let start_row = ((height as usize).saturating_sub(modal_height)) / 2;

        let bg = Color::AnsiValue(236);
        let border_color = Color::AnsiValue(244);
        let label_color = Color::AnsiValue(248);
        let value_color = Color::White;
        let input_bg = Color::AnsiValue(238);

        // Draw top border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("┌{:─<width$}┐", "", width = modal_width - 2)),
        )?;

        // Draw title row
        let title_padding = (modal_width - 2 - title.len()) / 2;
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16 + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│"),
            SetForegroundColor(Color::Cyan),
            Print(format!("{:>pad$}{}{:<rpad$}", "", title, "", pad = title_padding, rpad = modal_width - 2 - title_padding - title.len())),
            SetForegroundColor(border_color),
            Print("│"),
        )?;

        // Draw separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16 + 2),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("├{:─<width$}┤", "", width = modal_width - 2)),
        )?;

        // Draw "From:" row
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16 + 3),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetForegroundColor(label_color),
            Print(from_label),
            SetForegroundColor(value_color),
            Print(format!("{:<width$}", original_name, width = modal_width - 4 - from_label.len())),
            SetForegroundColor(border_color),
            Print(" │"),
        )?;

        // Draw "To:" row with input field
        let input_width = modal_width - 4 - to_label.len();
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16 + 4),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetForegroundColor(label_color),
            Print(to_label),
            SetBackgroundColor(input_bg),
            SetForegroundColor(Color::White),
            Print(format!("{:<width$}", new_name, width = input_width)),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(" │"),
        )?;

        // Draw bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16 + 5),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("└{:─<width$}┘", "", width = modal_width - 2)),
            ResetColor,
        )?;

        // Position cursor in the input field
        let cursor_col = start_col + 2 + to_label.len() + new_name.len();
        execute!(
            self.stdout,
            MoveTo(cursor_col as u16, start_row as u16 + 4),
            SetBackgroundColor(input_bg),
            crossterm::cursor::Show,
        )?;

        Ok(())
    }

    /// Render the find/replace bar in the status area
    pub fn render_find_replace_bar(
        &mut self,
        find_query: &str,
        replace_text: &str,
        active_field: bool, // true = find, false = replace
        case_insensitive: bool,
        regex_mode: bool,
        match_count: usize,
        current_match: usize,
        left_offset: u16,
    ) -> Result<()> {
        let status_row = self.rows.saturating_sub(1);
        let available_cols = (self.cols.saturating_sub(left_offset)) as usize;

        execute!(self.stdout, MoveTo(left_offset, status_row))?;

        // Colors
        let bg = Color::DarkGrey;
        let active_bg = Color::AnsiValue(238);
        let inactive_bg = Color::AnsiValue(236);
        let label_color = Color::AnsiValue(250);
        let active_label = Color::White;
        let toggle_on = Color::Yellow;
        let toggle_off = Color::AnsiValue(243);

        // Calculate widths
        // Layout: Find: [____] Replace: [____] [.*] [Aa] | N/M matches
        let find_label = "Find: ";
        let replace_label = " Replace: ";
        let suffix_len = 25; // toggles + match count
        let input_width = (available_cols.saturating_sub(find_label.len() + replace_label.len() + suffix_len)) / 2;
        let input_width = input_width.max(10).min(40);

        // Start with background
        execute!(self.stdout, SetBackgroundColor(bg))?;

        // Find label and input
        let find_bg = if active_field { active_bg } else { inactive_bg };
        let find_label_color = if active_field { active_label } else { label_color };

        execute!(
            self.stdout,
            SetForegroundColor(find_label_color),
            Print(find_label),
            SetBackgroundColor(find_bg),
            SetForegroundColor(Color::White),
        )?;

        // Truncate or pad find query
        let find_display: String = if find_query.len() > input_width {
            find_query.chars().skip(find_query.len() - input_width).collect()
        } else {
            format!("{:<width$}", find_query, width = input_width)
        };
        execute!(self.stdout, Print(&find_display))?;

        // Replace label and input
        let replace_bg = if !active_field { active_bg } else { inactive_bg };
        let replace_label_color = if !active_field { active_label } else { label_color };

        execute!(
            self.stdout,
            SetBackgroundColor(bg),
            SetForegroundColor(replace_label_color),
            Print(replace_label),
            SetBackgroundColor(replace_bg),
            SetForegroundColor(Color::White),
        )?;

        // Truncate or pad replace text
        let replace_display: String = if replace_text.len() > input_width {
            replace_text.chars().skip(replace_text.len() - input_width).collect()
        } else {
            format!("{:<width$}", replace_text, width = input_width)
        };
        execute!(self.stdout, Print(&replace_display))?;

        // Toggle buttons
        execute!(self.stdout, SetBackgroundColor(bg))?;

        // Regex toggle [.*]
        let regex_color = if regex_mode { toggle_on } else { toggle_off };
        execute!(
            self.stdout,
            Print(" "),
            SetForegroundColor(regex_color),
            Print("[.*]"),
        )?;

        // Case sensitivity toggle [Aa]
        let case_color = if case_insensitive { toggle_on } else { toggle_off };
        execute!(
            self.stdout,
            Print(" "),
            SetForegroundColor(case_color),
            Print("[Aa]"),
        )?;

        // Match count
        execute!(self.stdout, SetForegroundColor(label_color))?;
        if match_count > 0 {
            execute!(
                self.stdout,
                Print(format!(" {}/{}", current_match + 1, match_count)),
            )?;
        } else if !find_query.is_empty() {
            execute!(self.stdout, Print(" No matches"))?;
        }

        // Fill remaining space
        let used = find_label.len() + input_width + replace_label.len() + input_width + 5 + 5 +
            if match_count > 0 { format!(" {}/{}", current_match + 1, match_count).len() }
            else if !find_query.is_empty() { 11 }
            else { 0 };
        let remaining = available_cols.saturating_sub(used);
        execute!(
            self.stdout,
            Print(" ".repeat(remaining)),
            ResetColor,
        )?;

        // Position cursor in active field
        let cursor_col = if active_field {
            left_offset as usize + find_label.len() + find_query.len().min(input_width)
        } else {
            left_offset as usize + find_label.len() + input_width + replace_label.len() + replace_text.len().min(input_width)
        };
        execute!(
            self.stdout,
            MoveTo(cursor_col as u16, status_row),
            crossterm::cursor::Show,
        )?;

        Ok(())
    }

    /// Render the Fortress file browser modal
    pub fn render_fortress_modal(
        &mut self,
        current_path: &std::path::Path,
        entries: &[(String, std::path::PathBuf, bool)], // (name, path, is_dir)
        selected_index: usize,
        filter: &str,
        scroll_offset: usize,
    ) -> Result<()> {
        let (width, height) = (self.cols as usize, self.rows as usize);

        // Modal dimensions - centered
        let modal_width = 60.min(width - 4);
        let modal_height = 20.min(height - 4);
        let start_col = (width.saturating_sub(modal_width)) / 2;
        let start_row = (height.saturating_sub(modal_height)) / 2;

        // Filter entries based on query
        let filtered: Vec<(usize, &(String, std::path::PathBuf, bool))> = if filter.is_empty() {
            entries.iter().enumerate().collect()
        } else {
            let f = filter.to_lowercase();
            entries.iter().enumerate()
                .filter(|(_, (name, _, _))| name.to_lowercase().contains(&f))
                .collect()
        };

        // Colors
        let bg = Color::AnsiValue(235);
        let border_color = Color::AnsiValue(244);
        let header_color = Color::Cyan;
        let dir_color = Color::Blue;
        let file_color = Color::AnsiValue(252);
        let selected_bg = Color::AnsiValue(240);
        let input_bg = Color::AnsiValue(238);

        // Draw top border with title
        let path_str = current_path.to_string_lossy();
        let max_path_len = modal_width - 6;
        let display_path = if path_str.len() > max_path_len {
            format!("...{}", &path_str[path_str.len().saturating_sub(max_path_len - 3)..])
        } else {
            path_str.to_string()
        };
        let title = format!(" {} ", display_path);
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("┌"),
            SetForegroundColor(header_color),
            Print(&title),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}┐", "", width = modal_width.saturating_sub(title.len() + 2))),
            ResetColor,
        )?;

        // Draw filter input row
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 1) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetForegroundColor(Color::AnsiValue(248)),
            Print("Filter: "),
            SetBackgroundColor(input_bg),
            SetForegroundColor(Color::White),
            Print(format!("{:<width$}", filter, width = modal_width.saturating_sub(12))),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│"),
            ResetColor,
        )?;

        // Draw separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 2) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("├{:─<width$}┤", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Calculate visible range
        let visible_rows = modal_height.saturating_sub(5); // Account for borders, title, filter, help

        // Adjust scroll offset so selected item is visible
        let scroll = if selected_index < scroll_offset {
            selected_index
        } else if selected_index >= scroll_offset + visible_rows {
            selected_index - visible_rows + 1
        } else {
            scroll_offset
        };

        // Draw file/directory entries
        for (display_idx, (_orig_idx, (name, _, is_dir))) in filtered.iter().enumerate().skip(scroll).take(visible_rows) {
            let row = (start_row + 3 + display_idx - scroll) as u16;
            let is_selected = display_idx == selected_index;

            let item_bg = if is_selected { selected_bg } else { bg };
            let name_color = if *is_dir { dir_color } else { file_color };
            let icon = if *is_dir { "[d] " } else { "    " };

            // Truncate name if needed
            let max_name_len = modal_width.saturating_sub(6);
            let display_name = if name.len() > max_name_len {
                format!("{}...", &name[..max_name_len - 3])
            } else {
                name.clone()
            };

            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(item_bg),
                SetForegroundColor(border_color),
                Print("│ "),
                Print(icon),
                SetForegroundColor(name_color),
                Print(format!("{:<width$}", display_name, width = modal_width.saturating_sub(6))),
                SetForegroundColor(border_color),
                Print("│"),
                ResetColor,
            )?;
        }

        // Fill remaining rows with empty space
        let items_drawn = filtered.len().saturating_sub(scroll).min(visible_rows);
        for i in items_drawn..visible_rows {
            let row = (start_row + 3 + i) as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(bg),
                SetForegroundColor(border_color),
                Print(format!("│{:width$}│", "", width = modal_width.saturating_sub(2))),
                ResetColor,
            )?;
        }

        // Draw help text row
        let help_row = (start_row + 3 + visible_rows) as u16;
        let help_text = "←:up  →/Enter:open  ↑↓:nav  Esc:close";
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("├"),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!(" {:<width$}", help_text, width = modal_width.saturating_sub(3))),
            SetForegroundColor(border_color),
            Print("┤"),
            ResetColor,
        )?;

        // Draw bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("└{:─<width$}┘", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Hide cursor when in fortress modal
        execute!(self.stdout, Hide)?;

        self.stdout.flush()?;
        Ok(())
    }

    /// Render the multi-file search modal (F4)
    pub fn render_file_search_modal(
        &mut self,
        query: &str,
        results: &[(std::path::PathBuf, usize, String)], // (path, line_num, line_content)
        selected_index: usize,
        scroll_offset: usize,
        searching: bool,
    ) -> Result<()> {
        let (width, height) = (self.cols as usize, self.rows as usize);

        // Modal dimensions - centered, wider than fortress
        let modal_width = 80.min(width - 4);
        let modal_height = 25.min(height - 4);
        let start_col = (width.saturating_sub(modal_width)) / 2;
        let start_row = (height.saturating_sub(modal_height)) / 2;

        // Colors
        let bg = Color::AnsiValue(235);
        let border_color = Color::AnsiValue(244);
        let header_color = Color::Cyan;
        let path_color = Color::Blue;
        let line_num_color = Color::Yellow;
        let content_color = Color::AnsiValue(252);
        let selected_bg = Color::AnsiValue(240);
        let input_bg = Color::AnsiValue(238);

        // Draw top border with title
        let title = " Search in Files (F4) ";
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("┌"),
            SetForegroundColor(header_color),
            Print(title),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}┐", "", width = modal_width.saturating_sub(title.len() + 2))),
            ResetColor,
        )?;

        // Draw search input row
        let status = if searching {
            "Searching..."
        } else if results.is_empty() && !query.is_empty() {
            "No results"
        } else if !results.is_empty() {
            ""
        } else {
            "Type query, press Enter"
        };
        let input_width = modal_width.saturating_sub(14 + status.len());
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 1) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetForegroundColor(Color::AnsiValue(248)),
            Print("Search: "),
            SetBackgroundColor(input_bg),
            SetForegroundColor(Color::White),
            Print(format!("{:<width$}", query, width = input_width)),
            SetBackgroundColor(bg),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!(" {}", status)),
            SetForegroundColor(border_color),
            Print(" │"),
            ResetColor,
        )?;

        // Draw separator with result count
        let count_str = if results.is_empty() {
            String::new()
        } else {
            format!(" {} results ", results.len())
        };
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 2) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("├"),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(&count_str),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}┤", "", width = modal_width.saturating_sub(2 + count_str.len()))),
            ResetColor,
        )?;

        // Calculate visible range
        let visible_rows = modal_height.saturating_sub(5); // Account for borders, title, input, help

        // Adjust scroll offset so selected item is visible
        let scroll = if selected_index < scroll_offset {
            selected_index
        } else if selected_index >= scroll_offset + visible_rows {
            selected_index - visible_rows + 1
        } else {
            scroll_offset
        };

        // Draw results
        for (display_idx, (path, line_num, content)) in results.iter().enumerate().skip(scroll).take(visible_rows) {
            let row = (start_row + 3 + display_idx - scroll) as u16;
            let is_selected = display_idx == selected_index;

            let item_bg = if is_selected { selected_bg } else { bg };

            // Format: path:line: content
            let path_str = path.to_string_lossy();
            let line_str = format!("{}", line_num);

            // Calculate available width for content
            let prefix_len = path_str.len().min(30) + 1 + line_str.len() + 2; // path:line:
            let content_width = modal_width.saturating_sub(prefix_len + 4);

            // Truncate path if needed
            let display_path = if path_str.len() > 30 {
                format!("...{}", &path_str[path_str.len().saturating_sub(27)..])
            } else {
                path_str.to_string()
            };

            // Truncate content if needed
            let display_content = if content.len() > content_width {
                format!("{}...", &content[..content_width.saturating_sub(3)])
            } else {
                content.clone()
            };

            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(item_bg),
                SetForegroundColor(border_color),
                Print("│ "),
                SetForegroundColor(path_color),
                Print(&display_path),
                SetForegroundColor(Color::AnsiValue(243)),
                Print(":"),
                SetForegroundColor(line_num_color),
                Print(&line_str),
                SetForegroundColor(Color::AnsiValue(243)),
                Print(": "),
                SetForegroundColor(content_color),
            )?;

            // Calculate remaining width and print content with padding
            let used = display_path.len() + 1 + line_str.len() + 2 + 2;
            let remaining = modal_width.saturating_sub(used + 2);
            execute!(
                self.stdout,
                Print(format!("{:<width$}", display_content, width = remaining)),
                SetForegroundColor(border_color),
                Print("│"),
                ResetColor,
            )?;
        }

        // Fill remaining rows with empty space
        let items_drawn = results.len().saturating_sub(scroll).min(visible_rows);
        for i in items_drawn..visible_rows {
            let row = (start_row + 3 + i) as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(bg),
                SetForegroundColor(border_color),
                Print(format!("│{:width$}│", "", width = modal_width.saturating_sub(2))),
                ResetColor,
            )?;
        }

        // Draw help text row
        let help_row = (start_row + 3 + visible_rows) as u16;
        let help_text = "Enter:search/open  ↑↓:nav  PgUp/Dn:scroll  Esc:close";
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("├"),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!(" {:<width$}", help_text, width = modal_width.saturating_sub(3))),
            SetForegroundColor(border_color),
            Print("┤"),
            ResetColor,
        )?;

        // Draw bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("└{:─<width$}┘", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Hide cursor when in modal
        execute!(self.stdout, Hide)?;

        self.stdout.flush()?;
        Ok(())
    }

    /// Render the command palette modal (Ctrl+P)
    pub fn render_command_palette(
        &mut self,
        query: &str,
        commands: &[(String, String, String, String)], // (name, shortcut, category, id)
        selected_index: usize,
        scroll_offset: usize,
    ) -> Result<()> {
        let (width, height) = (self.cols as usize, self.rows as usize);

        // Modal dimensions - centered at top like VSCode
        let modal_width = 60.min(width - 4);
        let modal_height = 20.min(height - 4);
        let start_col = (width.saturating_sub(modal_width)) / 2;
        let start_row = 2; // Near top of screen

        // Colors - sleek dark theme
        let bg = Color::AnsiValue(236);
        let border_color = Color::AnsiValue(240);
        let _header_color = Color::Cyan; // reserved for future header styling
        let category_color = Color::AnsiValue(243);
        let name_color = Color::White;
        let shortcut_color = Color::AnsiValue(245);
        let selected_bg = Color::AnsiValue(24); // Blue highlight
        let selected_name = Color::White;
        let input_bg = Color::AnsiValue(238);
        let prompt_color = Color::Yellow;

        // Draw top border with subtle styling
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("╭{:─<width$}╮", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Draw search input row with > prefix
        let display_query = if query.is_empty() { "" } else { query };
        let input_display_width = modal_width.saturating_sub(6);
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 1) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetForegroundColor(prompt_color),
            SetAttribute(crossterm::style::Attribute::Bold),
            Print(">"),
            SetAttribute(crossterm::style::Attribute::Reset),
            SetBackgroundColor(input_bg),
            SetForegroundColor(Color::White),
            Print(format!(" {:<width$}", display_query, width = input_display_width - 1)),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(" │"),
            ResetColor,
        )?;

        // Draw separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 2) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("├{:─<width$}┤", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Calculate visible range
        let visible_rows = modal_height.saturating_sub(5);

        // Adjust scroll offset for visibility
        let scroll = if selected_index < scroll_offset {
            selected_index
        } else if selected_index >= scroll_offset + visible_rows {
            selected_index - visible_rows + 1
        } else {
            scroll_offset
        };

        // Draw commands
        for (display_idx, (name, shortcut, category, _id)) in commands.iter().enumerate().skip(scroll).take(visible_rows) {
            let row = (start_row + 3 + display_idx - scroll) as u16;
            let is_selected = display_idx == selected_index;

            let item_bg = if is_selected { selected_bg } else { bg };
            let item_name_color = if is_selected { selected_name } else { name_color };

            // Format: [Category] Name          Shortcut
            let category_prefix = if category.is_empty() {
                String::new()
            } else {
                format!("[{}] ", category)
            };

            let shortcut_display = shortcut.as_str();
            let name_width = modal_width.saturating_sub(4 + category_prefix.len() + shortcut_display.len() + 2);

            // Truncate name if needed
            let display_name = if name.len() > name_width {
                format!("{}…", &name[..name_width.saturating_sub(1)])
            } else {
                name.clone()
            };

            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(item_bg),
                SetForegroundColor(border_color),
                Print("│ "),
                SetForegroundColor(category_color),
                Print(&category_prefix),
                SetForegroundColor(item_name_color),
            )?;

            // Print name with padding
            let name_padding = name_width.saturating_sub(display_name.len());
            execute!(
                self.stdout,
                Print(&display_name),
                Print(format!("{:width$}", "", width = name_padding)),
                SetForegroundColor(shortcut_color),
                Print(format!(" {}", shortcut_display)),
                SetForegroundColor(border_color),
                Print(" │"),
                ResetColor,
            )?;
        }

        // Fill remaining rows
        let items_drawn = commands.len().saturating_sub(scroll).min(visible_rows);
        for i in items_drawn..visible_rows {
            let row = (start_row + 3 + i) as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(bg),
                SetForegroundColor(border_color),
                Print(format!("│{:width$}│", "", width = modal_width.saturating_sub(2))),
                ResetColor,
            )?;
        }

        // Draw help text row
        let help_row = (start_row + 3 + visible_rows) as u16;
        let help_text = "↑↓:select  Enter:run  Esc:close";
        let result_count = if commands.is_empty() {
            "No matches".to_string()
        } else {
            format!("{} commands", commands.len())
        };
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("├"),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!(" {} ", result_count)),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}", "", width = modal_width.saturating_sub(result_count.len() + 4))),
            Print("┤"),
            ResetColor,
        )?;

        // Draw bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("╰{:─<width$}╯", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Show help in lighter text at bottom
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row + 2),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!("{:^width$}", help_text, width = modal_width)),
            ResetColor,
        )?;

        // Hide cursor when in modal
        execute!(self.stdout, Hide)?;

        self.stdout.flush()?;
        Ok(())
    }

    /// Render the help menu modal (Shift+F1)
    pub fn render_help_menu(
        &mut self,
        query: &str,
        keybinds: &[(String, String, String)], // (shortcut, description, category)
        selected_index: usize,
        scroll_offset: usize,
        show_alt: bool,
    ) -> Result<()> {
        let (width, height) = (self.cols as usize, self.rows as usize);

        // Modal dimensions - larger to show keybindings comfortably
        let modal_width = 70.min(width - 4);
        let modal_height = 24.min(height - 4);
        let start_col = (width.saturating_sub(modal_width)) / 2;
        let start_row = 1; // Near top of screen

        // Colors - sleek dark theme matching command palette
        let bg = Color::AnsiValue(236);
        let border_color = Color::AnsiValue(240);
        let title_color = Color::Cyan;
        let category_color = Color::AnsiValue(243);
        let shortcut_color = if show_alt { Color::Magenta } else { Color::Yellow };
        let desc_color = Color::White;
        let selected_bg = Color::AnsiValue(24); // Blue highlight
        let input_bg = Color::AnsiValue(238);

        // Draw top border with title (show indicator when viewing alternates)
        let title = if show_alt { " Keybindings [/] " } else { " Keybindings " };
        let title_padding = (modal_width.saturating_sub(title.len() + 2)) / 2;
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("╭"),
            Print(format!("{:─<width$}", "", width = title_padding)),
            SetForegroundColor(title_color),
            SetAttribute(crossterm::style::Attribute::Bold),
            Print(title),
            SetAttribute(crossterm::style::Attribute::Reset),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}", "", width = modal_width.saturating_sub(title_padding + title.len() + 2))),
            Print("╮"),
            ResetColor,
        )?;

        // Draw search input row: "│ " + " {query}" + " │" = 2 + 1 + width + 2 = modal_width
        let display_query = if query.is_empty() { "Type to filter..." } else { query };
        let input_display_width = modal_width.saturating_sub(5);
        let placeholder_color = if query.is_empty() { Color::AnsiValue(243) } else { Color::White };
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 1) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetBackgroundColor(input_bg),
            SetForegroundColor(placeholder_color),
            Print(format!(" {:<width$}", display_query, width = input_display_width)),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(" │"),
            ResetColor,
        )?;

        // Draw separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, (start_row + 2) as u16),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("├{:─<width$}┤", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Calculate visible range
        let visible_rows = modal_height.saturating_sub(5);

        // Adjust scroll offset for visibility
        let scroll = if selected_index < scroll_offset {
            selected_index
        } else if selected_index >= scroll_offset + visible_rows {
            selected_index - visible_rows + 1
        } else {
            scroll_offset
        };

        // Draw keybindings
        let mut row_offset = 0;
        for (idx, (shortcut, description, category)) in keybinds.iter().enumerate().skip(scroll) {
            if row_offset >= visible_rows {
                break;
            }

            let row = (start_row + 3 + row_offset) as u16;
            let is_selected = idx == selected_index;
            let item_bg = if is_selected { selected_bg } else { bg };

            // Format: "│ " + shortcut + " " + description + " " + category + " │"
            // Widths: 2 + 16 + 1 + desc + 1 + 10 + 2 = 32 + desc = modal_width
            let shortcut_width = 16;
            let category_width = 10;
            let desc_width = modal_width.saturating_sub(shortcut_width + category_width + 6);

            // Truncate description if needed
            let display_desc = if description.len() > desc_width {
                format!("{}…", &description[..desc_width.saturating_sub(1)])
            } else {
                description.clone()
            };

            // Truncate shortcut if needed
            let display_shortcut = if shortcut.len() > shortcut_width {
                format!("{}…", &shortcut[..shortcut_width.saturating_sub(1)])
            } else {
                shortcut.clone()
            };

            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(item_bg),
                SetForegroundColor(border_color),
                Print("│ "),
                SetForegroundColor(shortcut_color),
                SetAttribute(crossterm::style::Attribute::Bold),
                Print(format!("{:<width$}", display_shortcut, width = shortcut_width)),
                SetAttribute(crossterm::style::Attribute::Reset),
                SetBackgroundColor(item_bg),
                SetForegroundColor(desc_color),
                Print(format!(" {:<width$}", display_desc, width = desc_width)),
                SetForegroundColor(category_color),
                Print(format!(" {:>width$}", category, width = category_width)),
                SetForegroundColor(border_color),
                Print(" │"),
                ResetColor,
            )?;

            row_offset += 1;
        }

        // Fill remaining rows
        for i in row_offset..visible_rows {
            let row = (start_row + 3 + i) as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(bg),
                SetForegroundColor(border_color),
                Print(format!("│{:width$}│", "", width = modal_width.saturating_sub(2))),
                ResetColor,
            )?;
        }

        // Draw info row
        let info_row = (start_row + 3 + visible_rows) as u16;
        let result_count = if keybinds.is_empty() {
            "No matches".to_string()
        } else {
            format!("{} keybinds", keybinds.len())
        };
        execute!(
            self.stdout,
            MoveTo(start_col as u16, info_row),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("├"),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!(" {} ", result_count)),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}", "", width = modal_width.saturating_sub(result_count.len() + 4))),
            Print("┤"),
            ResetColor,
        )?;

        // Draw bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, info_row + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("╰{:─<width$}╯", "", width = modal_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Show help text below
        let help_text = if show_alt {
            "↑↓:scroll  PgUp/PgDn:page  Home/End:jump  /:alt binds  Esc:close"
        } else {
            "↑↓:scroll  PgUp/PgDn:page  Home/End:jump  /:alt binds  Esc:close"
        };
        execute!(
            self.stdout,
            MoveTo(start_col as u16, info_row + 2),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!("{:^width$}", help_text, width = modal_width)),
            ResetColor,
        )?;

        // Hide cursor when in modal
        execute!(self.stdout, Hide)?;

        self.stdout.flush()?;
        Ok(())
    }

    /// Render the LSP references panel (sidebar style)
    pub fn render_references_panel(
        &mut self,
        locations: &[Location],
        selected_index: usize,
        query: &str,
        workspace_root: &std::path::Path,
    ) -> Result<()> {
        let (width, height) = (self.cols as usize, self.rows as usize);

        // Panel dimensions - sidebar style on the right
        let panel_width = 50.min(width / 2);
        let panel_height = height.saturating_sub(3); // Leave room for tab bar and status bar
        let start_col = width.saturating_sub(panel_width);
        let start_row = 1u16; // Below tab bar

        // Filter locations based on query
        let filtered: Vec<(usize, &Location)> = if query.is_empty() {
            locations.iter().enumerate().collect()
        } else {
            let q = query.to_lowercase();
            locations.iter().enumerate()
                .filter(|(_, loc)| loc.uri.to_lowercase().contains(&q))
                .collect()
        };

        // Colors
        let bg = Color::AnsiValue(235);
        let border_color = Color::AnsiValue(244);
        let header_color = Color::Cyan;
        let file_color = Color::AnsiValue(252);
        let line_num_color = Color::AnsiValue(243);
        let selected_bg = Color::AnsiValue(240);
        let input_bg = Color::AnsiValue(238);

        // Draw top border with title
        let title = format!(" References ({}) ", filtered.len());
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("┌"),
            SetForegroundColor(header_color),
            Print(&title),
            SetForegroundColor(border_color),
            Print(format!("{:─<width$}┐", "", width = panel_width.saturating_sub(title.len() + 2))),
            ResetColor,
        )?;

        // Draw filter input row
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│ "),
            SetForegroundColor(Color::AnsiValue(248)),
            Print("Filter: "),
            SetBackgroundColor(input_bg),
            SetForegroundColor(Color::White),
            Print(format!("{:<width$}", query, width = panel_width.saturating_sub(12))),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("│"),
            ResetColor,
        )?;

        // Draw separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 2),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("├{:─<width$}┤", "", width = panel_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Calculate visible range with scrolling
        let visible_rows = panel_height.saturating_sub(5); // Account for borders, title, filter, help
        let scroll_offset = if selected_index >= visible_rows {
            selected_index - visible_rows + 1
        } else {
            0
        };

        // Draw reference items
        for (display_idx, (_orig_idx, loc)) in filtered.iter().enumerate().skip(scroll_offset).take(visible_rows) {
            let row = start_row + 3 + (display_idx - scroll_offset) as u16;
            let is_selected = display_idx == selected_index;

            // Extract relative path and line number
            let path_str = if loc.uri.starts_with("file://") {
                &loc.uri[7..]
            } else {
                &loc.uri
            };

            // Make path relative to workspace if possible
            let display_path = if let Ok(rel_path) = std::path::Path::new(path_str).strip_prefix(workspace_root) {
                rel_path.to_string_lossy().to_string()
            } else {
                // Just show filename if we can't make it relative
                std::path::Path::new(path_str)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.to_string())
            };

            let line_info = format!(":{}", loc.range.start.line + 1);
            let max_path_width = panel_width.saturating_sub(line_info.len() + 4);
            let truncated_path = if display_path.len() > max_path_width {
                format!("...{}", &display_path[display_path.len().saturating_sub(max_path_width - 3)..])
            } else {
                display_path
            };

            let item_bg = if is_selected { selected_bg } else { bg };

            // Build a fixed-width line: "│ " + path (padded to max_path_width) + line_info + " │"
            // Total: 2 + max_path_width + line_info.len() + 2 = panel_width
            // So we need: max_path_width = panel_width - line_info.len() - 4
            // The remaining padding goes after line_info
            let remaining = panel_width.saturating_sub(max_path_width + line_info.len() + 4);

            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(item_bg),
                SetForegroundColor(border_color),
                Print("│ "),
                SetForegroundColor(file_color),
                Print(format!("{:<width$}", truncated_path, width = max_path_width)),
                SetForegroundColor(line_num_color),
                Print(&line_info),
                Print(format!("{:width$}", "", width = remaining)),
                SetForegroundColor(border_color),
                Print(" │"),
                ResetColor,
            )?;
        }

        // Fill remaining rows with empty space
        let items_drawn = filtered.len().saturating_sub(scroll_offset).min(visible_rows);
        for i in items_drawn..visible_rows {
            let row = start_row + 3 + i as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetBackgroundColor(bg),
                SetForegroundColor(border_color),
                Print(format!("│{:width$}│", "", width = panel_width.saturating_sub(2))),
                ResetColor,
            )?;
        }

        // Draw help text row
        let help_row = start_row + 3 + visible_rows as u16;
        let help_text = "↑↓:nav  Enter:go  Esc:close";
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print("├"),
            SetForegroundColor(Color::AnsiValue(243)),
            Print(format!(" {:<width$}", help_text, width = panel_width.saturating_sub(3))),
            SetForegroundColor(border_color),
            Print("┤"),
            ResetColor,
        )?;

        // Draw bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, help_row + 1),
            SetBackgroundColor(bg),
            SetForegroundColor(border_color),
            Print(format!("└{:─<width$}┘", "", width = panel_width.saturating_sub(2))),
            ResetColor,
        )?;

        // Hide cursor when in references panel
        execute!(self.stdout, Hide)?;

        self.stdout.flush()?;
        Ok(())
    }

    /// Render the LSP server manager panel
    pub fn render_server_manager_panel(&mut self, panel: &ServerManagerPanel) -> Result<()> {
        if !panel.visible {
            return Ok(());
        }

        let (width, height) = (self.cols, self.rows);
        let panel_width = 64.min(width as usize - 4);
        let max_visible = 10.min(height as usize - 8);

        // Center the panel
        let start_col = ((width as usize).saturating_sub(panel_width)) / 2;
        let start_row = 2u16;

        // Draw confirm dialog if in confirm mode
        if panel.confirm_mode {
            self.render_server_install_confirm(panel, start_col, start_row + 4)?;
            return Ok(());
        }

        // Draw manual install info dialog
        if panel.manual_info_mode {
            self.render_manual_install_info(panel, start_col, start_row + 4)?;
            return Ok(());
        }

        // Top border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row),
            SetForegroundColor(Color::Cyan),
            Print("┌"),
            Print("─".repeat(panel_width - 2)),
            Print("┐"),
            ResetColor
        )?;

        // Header
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 1),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            SetForegroundColor(Color::Cyan),
            SetAttribute(Attribute::Bold),
            Print(" Language Server Manager"),
            SetAttribute(Attribute::Reset),
            SetForegroundColor(Color::DarkGrey),
        )?;
        let header_len = 25;
        let padding = panel_width - header_len - 7;
        execute!(
            self.stdout,
            Print(" ".repeat(padding)),
            Print("Alt+M"),
            SetForegroundColor(Color::Cyan),
            Print(" │"),
            ResetColor
        )?;

        // Header separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 2),
            SetForegroundColor(Color::Cyan),
            Print("├"),
            Print("─".repeat(panel_width - 2)),
            Print("┤"),
            ResetColor
        )?;

        // Server list
        let visible_end = (panel.scroll_offset + max_visible).min(panel.servers.len());
        for (i, idx) in (panel.scroll_offset..visible_end).enumerate() {
            let server = &panel.servers[idx];
            let row = start_row + 3 + i as u16;
            let is_selected = idx == panel.selected_index;

            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetForegroundColor(Color::Cyan),
                Print("│"),
            )?;

            // Highlight selected row
            if is_selected {
                execute!(self.stdout, SetAttribute(Attribute::Reverse))?;
            }

            // Status icon
            execute!(self.stdout, Print(" "))?;
            if server.is_installed {
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::Green),
                    Print("✓"),
                )?;
            } else {
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::Red),
                    Print("✗"),
                )?;
            }

            // Server name and language (or "Installing..." if being installed)
            let is_installing = panel.is_installing(idx);
            let name_lang = if is_installing {
                " Installing...".to_string()
            } else {
                format!(" {} ({})", server.name, server.language)
            };
            let name_len = name_lang.len().min(panel_width - 20);
            execute!(
                self.stdout,
                SetForegroundColor(if is_installing { Color::Yellow } else { Color::White }),
                Print(&name_lang[..name_len]),
            )?;

            // Status text
            let status = if is_installing {
                ""
            } else if server.is_installed {
                "installed"
            } else if server.install_cmd.starts_with('#') {
                "manual"
            } else {
                "Enter to install"
            };
            // Content width is panel_width - 2 (for the two │ borders)
            // We've printed: 1 space + 1 icon + name_len chars
            // We need to print: status + 1 trailing space before │
            let used = 1 + 1 + name_len + status.len() + 1;
            let content_width = panel_width - 2;
            let status_padding = content_width.saturating_sub(used);
            execute!(self.stdout, Print(" ".repeat(status_padding)))?;

            if server.is_installed {
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::DarkGrey),
                    Print(status),
                )?;
            } else {
                execute!(
                    self.stdout,
                    SetForegroundColor(Color::Yellow),
                    Print(status),
                )?;
            }

            if is_selected {
                execute!(self.stdout, SetAttribute(Attribute::Reset))?;
            }

            execute!(
                self.stdout,
                Print(" "),
                SetForegroundColor(Color::Cyan),
                Print("│"),
                ResetColor
            )?;
        }

        // Fill remaining rows
        for i in (visible_end - panel.scroll_offset)..max_visible {
            let row = start_row + 3 + i as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetForegroundColor(Color::Cyan),
                Print("│"),
                Print(" ".repeat(panel_width - 2)),
                Print("│"),
                ResetColor
            )?;
        }

        // Footer separator
        let footer_row = start_row + 3 + max_visible as u16;
        execute!(
            self.stdout,
            MoveTo(start_col as u16, footer_row),
            SetForegroundColor(Color::Cyan),
            Print("├"),
            Print("─".repeat(panel_width - 2)),
            Print("┤"),
            ResetColor
        )?;

        // Status or help
        execute!(
            self.stdout,
            MoveTo(start_col as u16, footer_row + 1),
            SetForegroundColor(Color::Cyan),
            Print("│"),
        )?;

        if let Some(ref msg) = panel.status_message {
            let content_width = panel_width - 2;
            let msg_width = msg.width();
            // Truncate if needed (simple truncation, could be smarter)
            let msg_display = if msg_width > content_width - 2 {
                // Find a safe truncation point
                let mut truncated = String::new();
                let mut w = 0;
                for c in msg.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if w + cw > content_width - 5 {
                        break;
                    }
                    truncated.push(c);
                    w += cw;
                }
                truncated.push_str("...");
                truncated
            } else {
                msg.clone()
            };
            let display_width = msg_display.width();
            execute!(
                self.stdout,
                SetForegroundColor(Color::Yellow),
                Print(format!(" {}", msg_display)),
            )?;
            // We printed 1 space + msg_display, need to fill to content_width
            let pad = content_width.saturating_sub(1 + display_width);
            execute!(self.stdout, Print(" ".repeat(pad)))?;
        } else {
            let help_text = " ↑↓ Navigate  Enter Install  r Refresh  Esc Close ";
            let help_width = help_text.width();
            execute!(
                self.stdout,
                SetForegroundColor(Color::DarkGrey),
                Print(help_text),
            )?;
            // Content width is panel_width - 2 (for borders)
            let content_width = panel_width - 2;
            let pad = content_width.saturating_sub(help_width);
            execute!(self.stdout, Print(" ".repeat(pad)))?;
        }

        execute!(
            self.stdout,
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, footer_row + 2),
            SetForegroundColor(Color::Cyan),
            Print("└"),
            Print("─".repeat(panel_width - 2)),
            Print("┘"),
            ResetColor
        )?;

        Ok(())
    }

    /// Render the install confirmation dialog
    fn render_server_install_confirm(
        &mut self,
        panel: &ServerManagerPanel,
        start_col: usize,
        start_row: u16,
    ) -> Result<()> {
        let panel_width = 60;

        let server = match panel.confirm_server() {
            Some(s) => s,
            None => return Ok(()),
        };

        // Top border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row),
            SetForegroundColor(Color::Cyan),
            Print("┌"),
            Print("─".repeat(panel_width - 2)),
            Print("┐"),
            ResetColor
        )?;

        // Title
        let title = format!(" Install {}? ", server.name);
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 1),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            SetAttribute(Attribute::Bold),
            Print(&title),
            SetAttribute(Attribute::Reset),
        )?;
        let pad = panel_width - 2 - title.len();
        execute!(
            self.stdout,
            Print(" ".repeat(pad)),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Blank line
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 2),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            Print(" ".repeat(panel_width - 2)),
            Print("│"),
            ResetColor
        )?;

        // Command
        let cmd_display = if server.install_cmd.len() > panel_width - 14 {
            format!("{}...", &server.install_cmd[..panel_width - 17])
        } else {
            server.install_cmd.to_string()
        };
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 3),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            SetForegroundColor(Color::White),
            Print(" Command: "),
            SetForegroundColor(Color::Yellow),
            Print(&cmd_display),
        )?;
        let pad = panel_width - 12 - cmd_display.len();
        execute!(
            self.stdout,
            Print(" ".repeat(pad)),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Blank line
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 4),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            Print(" ".repeat(panel_width - 2)),
            Print("│"),
            ResetColor
        )?;

        // Buttons
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 5),
            SetForegroundColor(Color::Cyan),
            Print("│"),
        )?;
        let button_text = "[Y]es    [N]o";
        let button_pad = (panel_width - 2 - button_text.len()) / 2;
        execute!(
            self.stdout,
            Print(" ".repeat(button_pad)),
            Print("["),
            SetForegroundColor(Color::Green),
            Print("Y"),
            SetForegroundColor(Color::White),
            Print("]es    ["),
            SetForegroundColor(Color::Red),
            Print("N"),
            SetForegroundColor(Color::White),
            Print("]o"),
            Print(" ".repeat(panel_width - 2 - button_pad - button_text.len())),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 6),
            SetForegroundColor(Color::Cyan),
            Print("└"),
            Print("─".repeat(panel_width - 2)),
            Print("┘"),
            ResetColor
        )?;

        Ok(())
    }

    /// Render the manual install info dialog
    fn render_manual_install_info(
        &mut self,
        panel: &ServerManagerPanel,
        start_col: usize,
        start_row: u16,
    ) -> Result<()> {
        let panel_width = 60;

        let server = match panel.manual_info_server() {
            Some(s) => s,
            None => return Ok(()),
        };

        // Parse the install instructions (remove leading #)
        let instructions = server.install_cmd.trim_start_matches('#').trim();

        // Top border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row),
            SetForegroundColor(Color::Cyan),
            Print("┌"),
            Print("─".repeat(panel_width - 2)),
            Print("┐"),
            ResetColor
        )?;

        // Title
        let title = format!(" {} - Manual Installation ", server.name);
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 1),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            SetAttribute(Attribute::Bold),
            SetForegroundColor(Color::Yellow),
            Print(&title),
            SetAttribute(Attribute::Reset),
        )?;
        let pad = panel_width - 2 - title.len();
        execute!(
            self.stdout,
            Print(" ".repeat(pad)),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Separator
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 2),
            SetForegroundColor(Color::Cyan),
            Print("├"),
            Print("─".repeat(panel_width - 2)),
            Print("┤"),
            ResetColor
        )?;

        // Language
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 3),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            SetForegroundColor(Color::White),
            Print(" Language: "),
            SetForegroundColor(Color::Green),
            Print(server.language),
        )?;
        let lang_pad = panel_width - 13 - server.language.len();
        execute!(
            self.stdout,
            Print(" ".repeat(lang_pad)),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Blank line
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 4),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            Print(" ".repeat(panel_width - 2)),
            Print("│"),
            ResetColor
        )?;

        // Instructions label
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 5),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            SetForegroundColor(Color::White),
            Print(" Installation:"),
        )?;
        execute!(
            self.stdout,
            Print(" ".repeat(panel_width - 16)),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Instructions text (may be multi-line, show up to 3 lines)
        let instr_lines: Vec<&str> = instructions.lines().collect();
        for (i, line) in instr_lines.iter().take(3).enumerate() {
            let row = start_row + 6 + i as u16;
            let display_line = if line.len() > panel_width - 6 {
                format!("{}...", &line[..panel_width - 9])
            } else {
                line.to_string()
            };
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetForegroundColor(Color::Cyan),
                Print("│"),
                SetForegroundColor(Color::Yellow),
                Print(format!("   {}", display_line)),
            )?;
            let line_pad = panel_width - 5 - display_line.len();
            execute!(
                self.stdout,
                Print(" ".repeat(line_pad)),
                SetForegroundColor(Color::Cyan),
                Print("│"),
                ResetColor
            )?;
        }

        // Fill remaining instruction lines if less than 3
        for i in instr_lines.len()..3 {
            let row = start_row + 6 + i as u16;
            execute!(
                self.stdout,
                MoveTo(start_col as u16, row),
                SetForegroundColor(Color::Cyan),
                Print("│"),
                Print(" ".repeat(panel_width - 2)),
                Print("│"),
                ResetColor
            )?;
        }

        // Blank line
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 9),
            SetForegroundColor(Color::Cyan),
            Print("│"),
            Print(" ".repeat(panel_width - 2)),
            Print("│"),
            ResetColor
        )?;

        // Status or help line
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 10),
            SetForegroundColor(Color::Cyan),
            Print("│"),
        )?;

        if panel.copied_to_clipboard {
            execute!(
                self.stdout,
                SetForegroundColor(Color::Green),
                Print(" ✓ Copied to clipboard!"),
            )?;
            execute!(self.stdout, Print(" ".repeat(panel_width - 26)))?;
        } else {
            execute!(
                self.stdout,
                SetForegroundColor(Color::DarkGrey),
                Print(" [C] Copy to clipboard  [Esc] Close"),
            )?;
            execute!(self.stdout, Print(" ".repeat(panel_width - 38)))?;
        }

        execute!(
            self.stdout,
            SetForegroundColor(Color::Cyan),
            Print("│"),
            ResetColor
        )?;

        // Bottom border
        execute!(
            self.stdout,
            MoveTo(start_col as u16, start_row + 11),
            SetForegroundColor(Color::Cyan),
            Print("└"),
            Print("─".repeat(panel_width - 2)),
            Print("┘"),
            ResetColor
        )?;

        Ok(())
    }

    /// Render the integrated terminal panel
    pub fn render_terminal(&mut self, terminal: &TerminalPanel, left_offset: u16) -> Result<()> {
        // Hide cursor during render to prevent flicker
        execute!(self.stdout, Hide)?;

        let start_row = terminal.render_start_row(self.rows);
        let height = terminal.height;
        let terminal_width = self.cols.saturating_sub(left_offset) as usize;

        // Draw terminal border (top line with title)
        execute!(
            self.stdout,
            MoveTo(left_offset, start_row),
            SetBackgroundColor(Color::AnsiValue(237)),
            SetForegroundColor(Color::White),
        )?;

        // Terminal title bar with tabs
        let session_count = terminal.session_count();
        let active_idx = terminal.active_session_index();

        if session_count <= 1 {
            // Single session: show CWD or "Terminal" centered
            let name = terminal.active_cwd()
                .map(|p| extract_dirname(p))
                .unwrap_or_else(|| "Terminal".to_string());
            let title = format!(" {} ", name);
            let separator = "─".repeat(terminal_width.saturating_sub(title.len() + 2) / 2);
            execute!(
                self.stdout,
                Print(&separator),
                SetAttribute(Attribute::Bold),
                Print(&title),
                SetAttribute(Attribute::Reset),
                SetBackgroundColor(Color::AnsiValue(237)),
                SetForegroundColor(Color::White),
                Print(&separator),
            )?;

            // Pad to end of line
            let printed = separator.chars().count() * 2 + title.len();
            if printed < terminal_width {
                execute!(self.stdout, Print(" ".repeat(terminal_width - printed)))?;
            }
        } else {
            // Multiple sessions: render tab bar
            let sessions = terminal.sessions();
            let available_width = terminal_width;
            let tab_width = (available_width / session_count).max(8).min(25);

            let mut printed = 0;
            for (i, session) in sessions.iter().enumerate() {
                let is_active = i == active_idx;
                let name = session.cwd()
                    .map(|p| extract_dirname(p))
                    .unwrap_or_else(|| format!("Term {}", i + 1));

                // Format: "[n] name" with truncation
                let prefix = format!("{} ", i + 1);
                let max_name_len = tab_width.saturating_sub(prefix.len() + 1);
                let display_name = if name.len() > max_name_len {
                    format!("{}…", &name[..max_name_len.saturating_sub(1)])
                } else {
                    name
                };
                let tab_content = format!("{}{}", prefix, display_name);

                // Set colors based on active state
                if is_active {
                    execute!(
                        self.stdout,
                        SetBackgroundColor(Color::AnsiValue(238)),
                        SetForegroundColor(Color::White),
                        SetAttribute(Attribute::Bold),
                    )?;
                } else {
                    execute!(
                        self.stdout,
                        SetBackgroundColor(Color::AnsiValue(235)),
                        SetForegroundColor(Color::AnsiValue(245)),
                        SetAttribute(Attribute::Reset),
                    )?;
                }

                // Print tab with padding
                let padding = tab_width.saturating_sub(tab_content.len());
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                execute!(
                    self.stdout,
                    Print(" ".repeat(left_pad)),
                    Print(&tab_content),
                    Print(" ".repeat(right_pad)),
                )?;
                printed += tab_width;

                // Separator between tabs
                if i < session_count - 1 {
                    execute!(
                        self.stdout,
                        SetBackgroundColor(Color::AnsiValue(237)),
                        SetForegroundColor(Color::AnsiValue(240)),
                        SetAttribute(Attribute::Reset),
                        Print("│"),
                    )?;
                    printed += 1;
                }
            }

            // Fill remaining space
            if printed < available_width {
                execute!(
                    self.stdout,
                    SetBackgroundColor(Color::AnsiValue(237)),
                    SetForegroundColor(Color::White),
                    SetAttribute(Attribute::Reset),
                    Print(" ".repeat(available_width - printed)),
                )?;
            }
        }

        // Terminal content area - use batched rendering to reduce flicker
        let (cursor_row, cursor_col) = terminal.cursor_pos();
        let default_bg = Color::AnsiValue(232);
        let default_fg = Color::White;

        // Track current colors to avoid redundant escape sequences
        let mut current_fg = default_fg;
        let mut current_bg = default_bg;
        let mut current_bold = false;
        let mut current_underline = false;

        // Set initial colors
        execute!(
            self.stdout,
            SetBackgroundColor(default_bg),
            SetForegroundColor(default_fg)
        )?;

        for row in 0..(height - 1) {
            execute!(self.stdout, MoveTo(left_offset, start_row + 1 + row))?;

            // Build a string of characters with same attributes to batch print
            let mut batch = String::new();
            let mut batch_fg = current_fg;
            let mut batch_bg = current_bg;
            let mut batch_bold = current_bold;
            let mut batch_underline = current_underline;

            for col in 0..terminal_width {
                let (c, fg, bg, bold, underline) = if let Some(cell) = terminal.get_cell(row as usize, col) {
                    let (fg, bg) = if cell.inverse {
                        let fg = TerminalPanel::to_crossterm_color(&cell.bg);
                        let bg = TerminalPanel::to_crossterm_color(&cell.fg);
                        (
                            if fg == Color::Reset { default_bg } else { fg },
                            if bg == Color::Reset { default_fg } else { bg },
                        )
                    } else {
                        let fg = TerminalPanel::to_crossterm_color(&cell.fg);
                        let bg = TerminalPanel::to_crossterm_color(&cell.bg);
                        (
                            if fg == Color::Reset { default_fg } else { fg },
                            if bg == Color::Reset { default_bg } else { bg },
                        )
                    };
                    (cell.c, fg, bg, cell.bold, cell.underline)
                } else {
                    (' ', default_fg, default_bg, false, false)
                };

                // Check if attributes changed
                if fg != batch_fg || bg != batch_bg || bold != batch_bold || underline != batch_underline {
                    // Flush current batch
                    if !batch.is_empty() {
                        // Apply batch attributes if different from current
                        if batch_fg != current_fg {
                            execute!(self.stdout, SetForegroundColor(batch_fg))?;
                            current_fg = batch_fg;
                        }
                        if batch_bg != current_bg {
                            execute!(self.stdout, SetBackgroundColor(batch_bg))?;
                            current_bg = batch_bg;
                        }
                        if batch_bold != current_bold {
                            if batch_bold {
                                execute!(self.stdout, SetAttribute(Attribute::Bold))?;
                            } else {
                                execute!(self.stdout, SetAttribute(Attribute::NoBold))?;
                            }
                            current_bold = batch_bold;
                        }
                        if batch_underline != current_underline {
                            if batch_underline {
                                execute!(self.stdout, SetAttribute(Attribute::Underlined))?;
                            } else {
                                execute!(self.stdout, SetAttribute(Attribute::NoUnderline))?;
                            }
                            current_underline = batch_underline;
                        }
                        execute!(self.stdout, Print(&batch))?;
                        batch.clear();
                    }
                    batch_fg = fg;
                    batch_bg = bg;
                    batch_bold = bold;
                    batch_underline = underline;
                }
                batch.push(c);
            }

            // Flush remaining batch for this row
            if !batch.is_empty() {
                if batch_fg != current_fg {
                    execute!(self.stdout, SetForegroundColor(batch_fg))?;
                    current_fg = batch_fg;
                }
                if batch_bg != current_bg {
                    execute!(self.stdout, SetBackgroundColor(batch_bg))?;
                    current_bg = batch_bg;
                }
                if batch_bold != current_bold {
                    if batch_bold {
                        execute!(self.stdout, SetAttribute(Attribute::Bold))?;
                    } else {
                        execute!(self.stdout, SetAttribute(Attribute::NoBold))?;
                    }
                    current_bold = batch_bold;
                }
                if batch_underline != current_underline {
                    if batch_underline {
                        execute!(self.stdout, SetAttribute(Attribute::Underlined))?;
                    } else {
                        execute!(self.stdout, SetAttribute(Attribute::NoUnderline))?;
                    }
                    current_underline = batch_underline;
                }
                execute!(self.stdout, Print(&batch))?;
            }
        }

        // Position cursor in terminal (offset by left_offset)
        execute!(
            self.stdout,
            MoveTo(left_offset + cursor_col, start_row + 1 + cursor_row),
            Show,
            ResetColor
        )?;

        Ok(())
    }
}
