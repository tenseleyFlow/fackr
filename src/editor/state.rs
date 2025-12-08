use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyEvent, MouseEvent};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::input::{Key, Modifiers, Mouse, Button};
use crate::lsp::{CompletionItem, Diagnostic, HoverInfo, Location, ServerManagerPanel};
use crate::render::{PaneBounds as RenderPaneBounds, PaneInfo, Screen, TabInfo};
use crate::workspace::{PaneDirection, Tab, Workspace};

use super::{Cursor, Cursors, History, Operation, Position};

/// How often to write backups (debounce interval)
const BACKUP_INTERVAL_SECS: u64 = 5;

/// Prompt state for quit confirmation
#[derive(Debug, Clone, PartialEq)]
enum PromptState {
    /// No prompt active
    None,
    /// Quit prompt: Save/Discard/Cancel
    QuitConfirm,
    /// Restore prompt: Restore/Discard
    RestoreBackup,
    /// Text input prompt (label, current input buffer)
    TextInput { label: String, buffer: String, action: TextInputAction },
    /// LSP rename modal with original name shown
    RenameModal {
        original_name: String,
        new_name: String,
        path: String,
        line: u32,
        col: u32,
    },
    /// LSP references panel
    ReferencesPanel {
        locations: Vec<Location>,
        selected_index: usize,
        /// Search query being typed (for filtering)
        query: String,
    },
}

/// Action to perform when text input is complete
#[derive(Debug, Clone, PartialEq)]
enum TextInputAction {
    /// Commit with the entered message
    GitCommit,
    /// Create a git tag
    GitTag,
}

/// LSP UI state
#[derive(Debug, Default)]
struct LspState {
    /// Current hover information to display
    hover: Option<HoverInfo>,
    /// Whether hover popup is visible
    hover_visible: bool,
    /// Current completion list
    completions: Vec<CompletionItem>,
    /// Selected completion index
    completion_index: usize,
    /// Whether completion popup is visible
    completion_visible: bool,
    /// Current diagnostics for the active file
    diagnostics: Vec<Diagnostic>,
    /// Go-to-definition results (for multi-result navigation)
    definition_locations: Vec<Location>,
    /// Pending request IDs (to match responses)
    pending_hover: Option<i64>,
    pending_completion: Option<i64>,
    pending_definition: Option<i64>,
    pending_references: Option<i64>,
    /// Last known buffer hash (to detect changes)
    last_buffer_hash: Option<u64>,
    /// Last file path that was synced to LSP
    last_synced_path: Option<PathBuf>,
}

/// Main editor state
pub struct Editor {
    /// The workspace (owns tabs, panes, fuss mode, and config)
    workspace: Workspace,
    /// Terminal screen
    screen: Screen,
    /// Is the editor running?
    running: bool,
    /// System clipboard (if available)
    clipboard: Option<Clipboard>,
    /// Fallback internal clipboard if system clipboard unavailable
    internal_clipboard: String,
    /// Message to display in status bar
    message: Option<String>,
    /// Escape key timeout in milliseconds (for Alt key detection)
    escape_time: u64,
    /// Current prompt state
    prompt: PromptState,
    /// Last time we wrote backups
    last_backup: Instant,
    /// LSP-related UI state
    lsp_state: LspState,
    /// LSP server manager panel
    server_manager: ServerManagerPanel,
}

impl Editor {
    pub fn new() -> Result<Self> {
        // Default workspace is current directory
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::new_with_workspace(workspace_root)
    }

    pub fn new_with_workspace(workspace_root: PathBuf) -> Result<Self> {
        let mut screen = Screen::new()?;
        screen.enter_raw_mode()?;
        Self::new_with_screen_and_workspace(screen, workspace_root)
    }

    pub fn new_with_screen_and_workspace(screen: Screen, workspace_root: PathBuf) -> Result<Self> {
        // Read escape timeout from environment, default to 5ms
        // Similar to vim's ttimeoutlen or tmux's escape-time
        let escape_time = std::env::var("FAC_ESCAPE_TIME")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);

        // Try to initialize system clipboard, fall back to internal if unavailable
        let clipboard = Clipboard::new().ok();

        let workspace = Workspace::open(workspace_root)?;

        // Check if there are backups to restore
        let has_backups = workspace.has_backups();

        let mut editor = Self {
            workspace,
            screen,
            running: true,
            clipboard,
            internal_clipboard: String::new(),
            message: None,
            escape_time,
            prompt: PromptState::None,
            last_backup: Instant::now(),
            lsp_state: LspState::default(),
            server_manager: ServerManagerPanel::new(),
        };

        // If there are backups, show restore prompt
        if has_backups {
            editor.prompt = PromptState::RestoreBackup;
            editor.message = Some("Recovered unsaved changes. [R]estore / [D]iscard / [Esc]".to_string());
        }

        Ok(editor)
    }

    pub fn open(&mut self, path: &str) -> Result<()> {
        let file_path = PathBuf::from(path);

        // If this is the initial open (empty default tab), use workspace detection
        let is_initial = self.workspace.tabs.len() == 1
            && !self.workspace.tabs[0].is_modified()
            && self.workspace.tabs[0].path().is_none();

        if is_initial {
            // Replace workspace with one detected from the file path
            // This finds existing .fackr/ in parent dirs or uses file's parent
            self.workspace = Workspace::open_with_file(&file_path)?;
        } else {
            // Just open the file in the current workspace
            self.workspace.open_file(&file_path)?;
        }

        Ok(())
    }

    // ============================================================
    // ACCESSOR METHODS - These provide access to current tab/pane/buffer
    // ============================================================

    /// Get the workspace root path
    pub fn workspace_root(&self) -> PathBuf {
        self.workspace.root.clone()
    }

    /// Get the current tab mutably
    #[inline]
    fn tab_mut(&mut self) -> &mut Tab {
        self.workspace.active_tab_mut()
    }

    /// Get current buffer (read-only)
    #[inline]
    fn buffer(&self) -> &Buffer {
        let tab = self.workspace.active_tab();
        let pane = &tab.panes[tab.active_pane];
        &tab.buffers[pane.buffer_idx].buffer
    }

    /// Get current buffer (mutable)
    #[inline]
    fn buffer_mut(&mut self) -> &mut Buffer {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        let buffer_idx = tab.panes[pane_idx].buffer_idx;
        &mut tab.buffers[buffer_idx].buffer
    }

    /// Get current cursors (read-only)
    #[inline]
    fn cursors(&self) -> &Cursors {
        let tab = self.workspace.active_tab();
        &tab.panes[tab.active_pane].cursors
    }

    /// Get current cursors (mutable)
    #[inline]
    fn cursors_mut(&mut self) -> &mut Cursors {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        &mut tab.panes[pane_idx].cursors
    }

    /// Get current history (mutable)
    #[inline]
    fn history_mut(&mut self) -> &mut History {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        let buffer_idx = tab.panes[pane_idx].buffer_idx;
        &mut tab.buffers[buffer_idx].history
    }

    /// Get current buffer entry (mutable)
    #[inline]
    fn buffer_entry_mut(&mut self) -> &mut crate::workspace::BufferEntry {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        let buffer_idx = tab.panes[pane_idx].buffer_idx;
        &mut tab.buffers[buffer_idx]
    }

    /// Get current viewport line
    #[inline]
    fn viewport_line(&self) -> usize {
        let tab = self.workspace.active_tab();
        tab.panes[tab.active_pane].viewport_line
    }

    /// Set current viewport line
    #[inline]
    fn set_viewport_line(&mut self, line: usize) {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        tab.panes[pane_idx].viewport_line = line;
    }

    /// Get current viewport column (horizontal scroll offset)
    #[inline]
    fn viewport_col(&self) -> usize {
        let tab = self.workspace.active_tab();
        tab.panes[tab.active_pane].viewport_col
    }

    /// Set current viewport column (horizontal scroll offset)
    #[inline]
    fn set_viewport_col(&mut self, col: usize) {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        tab.panes[pane_idx].viewport_col = col;
    }

    /// Get current filename
    #[inline]
    fn filename(&self) -> Option<PathBuf> {
        let tab = self.workspace.active_tab();
        let pane = &tab.panes[tab.active_pane];
        tab.buffers[pane.buffer_idx].path.clone()
    }

    pub fn run(&mut self) -> Result<()> {
        // Initial render
        self.screen.refresh_size()?;
        self.render()?;

        while self.running {
            // Track whether we need to re-render
            let mut needs_render = false;

            // Poll with a short timeout to allow LSP processing
            // This balances responsiveness with CPU usage
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key_event) => self.process_key(key_event)?,
                    Event::Mouse(mouse_event) => self.process_mouse(mouse_event)?,
                    Event::Resize(cols, rows) => {
                        self.screen.cols = cols;
                        self.screen.rows = rows;
                    }
                    _ => {}
                }
                needs_render = true;

                // Process any additional queued events before rendering
                while event::poll(Duration::from_millis(0))? {
                    match event::read()? {
                        Event::Key(key_event) => self.process_key(key_event)?,
                        Event::Mouse(mouse_event) => self.process_mouse(mouse_event)?,
                        Event::Resize(cols, rows) => {
                            self.screen.cols = cols;
                            self.screen.rows = rows;
                        }
                        _ => {}
                    }
                }
            }

            // Process LSP messages from language servers
            if self.process_lsp_messages() {
                needs_render = true;
            }

            // Poll for completed server installations
            if self.server_manager.poll_installs() {
                needs_render = true;
            }

            // Check if it's time to backup modified buffers
            self.maybe_backup();

            // Only render if something changed
            if needs_render {
                self.screen.refresh_size()?;
                self.render()?;
            }
        }

        self.screen.leave_raw_mode()?;
        Ok(())
    }

    /// Write backups if enough time has passed and there are unsaved changes
    fn maybe_backup(&mut self) {
        if self.last_backup.elapsed() >= Duration::from_secs(BACKUP_INTERVAL_SECS) {
            if self.workspace.has_unsaved_changes() {
                let _ = self.workspace.backup_all_modified();
            }
            self.last_backup = Instant::now();
        }
    }

    /// Process LSP messages. Returns true if any messages were processed.
    fn process_lsp_messages(&mut self) -> bool {
        use crate::lsp::LspResponse;

        // Process pending messages from language servers
        self.workspace.lsp.process_messages();

        let mut had_response = false;

        // Handle any responses that came in
        while let Some(response) = self.workspace.lsp.poll_response() {
            had_response = true;
            match response {
                LspResponse::Completions(id, items) => {
                    if self.lsp_state.pending_completion == Some(id) {
                        self.lsp_state.completions = items;
                        self.lsp_state.completion_index = 0;
                        self.lsp_state.completion_visible = !self.lsp_state.completions.is_empty();
                        self.lsp_state.pending_completion = None;
                    }
                }
                LspResponse::Hover(id, info) => {
                    if self.lsp_state.pending_hover == Some(id) {
                        self.lsp_state.hover = info;
                        self.lsp_state.hover_visible = self.lsp_state.hover.is_some();
                        self.lsp_state.pending_hover = None;
                        if self.lsp_state.hover.is_none() {
                            self.message = Some("No hover info available".to_string());
                        }
                    }
                }
                LspResponse::Definition(id, locations) => {
                    if self.lsp_state.pending_definition == Some(id) {
                        self.lsp_state.definition_locations = locations.clone();
                        self.lsp_state.pending_definition = None;
                        // Jump to first definition
                        if let Some(loc) = locations.first() {
                            self.goto_location(loc);
                        } else {
                            self.message = Some("No definition found".to_string());
                        }
                    }
                }
                LspResponse::References(id, locations) => {
                    if self.lsp_state.pending_references == Some(id) {
                        self.lsp_state.pending_references = None;
                        if locations.is_empty() {
                            self.message = Some("No references found".to_string());
                        } else if locations.len() == 1 {
                            // Single reference - just go there
                            self.goto_location(&locations[0]);
                        } else {
                            // Multiple references - show the references panel
                            self.prompt = PromptState::ReferencesPanel {
                                locations,
                                selected_index: 0,
                                query: String::new(),
                            };
                            self.message = None;
                        }
                    }
                }
                LspResponse::Symbols(id, symbols) => {
                    // TODO: Show symbols panel
                    let _ = (id, symbols);
                }
                LspResponse::Formatting(id, edits) => {
                    // Apply formatting edits
                    let _ = (id, edits);
                    // TODO: Apply text edits to buffer
                }
                LspResponse::Rename(_id, workspace_edit) => {
                    // Apply rename edits across all affected files
                    let mut total_edits = 0;
                    let mut files_changed = 0;

                    for (uri, edits) in &workspace_edit.changes {
                        if let Some(path_str) = crate::lsp::uri_to_path(uri) {
                            // Check if we have this file open
                            let path = std::path::PathBuf::from(&path_str);
                            if let Some(tab_idx) = self.workspace.find_tab_by_path(&path) {
                                // Apply edits to the open buffer (in reverse order to preserve positions)
                                let mut sorted_edits = edits.clone();
                                sorted_edits.sort_by(|a, b| {
                                    // Sort by start position, descending
                                    b.range.start.line.cmp(&a.range.start.line)
                                        .then(b.range.start.character.cmp(&a.range.start.character))
                                });

                                for edit in sorted_edits {
                                    self.workspace.apply_text_edit(tab_idx, &edit);
                                    total_edits += 1;
                                }
                                files_changed += 1;
                            } else {
                                // File not open - would need to open, edit, and save
                                self.message = Some(format!("Note: {} not open, skipped", path_str));
                            }
                        }
                    }

                    if total_edits > 0 {
                        self.message = Some(format!("Renamed: {} edits in {} file(s)", total_edits, files_changed));
                    } else {
                        self.message = Some("No rename edits to apply".to_string());
                    }
                }
                LspResponse::CodeActions(id, actions) => {
                    // TODO: Show code actions menu
                    let _ = (id, actions);
                }
                LspResponse::Error(id, message) => {
                    // Clear any pending state for this request
                    if self.lsp_state.pending_completion == Some(id) {
                        self.lsp_state.pending_completion = None;
                    }
                    if self.lsp_state.pending_hover == Some(id) {
                        self.lsp_state.pending_hover = None;
                    }
                    if self.lsp_state.pending_definition == Some(id) {
                        self.lsp_state.pending_definition = None;
                    }
                    if self.lsp_state.pending_references == Some(id) {
                        self.lsp_state.pending_references = None;
                    }
                    // Optionally show error
                    if !message.is_empty() {
                        self.message = Some(format!("LSP: {}", message));
                    }
                }
            }
        }

        // Update diagnostics for current file
        if let Some(path) = self.filename() {
            let path_str = path.to_string_lossy();
            self.lsp_state.diagnostics = self.workspace.lsp.get_diagnostics(&path_str);
        }

        // Sync document changes to LSP if buffer has changed
        self.sync_document_to_lsp();

        had_response
    }

    /// Sync document changes to LSP server
    fn sync_document_to_lsp(&mut self) {
        let current_path = self.filename();
        let current_hash = self.buffer().content_hash();

        // Check if we switched files
        let file_changed = current_path != self.lsp_state.last_synced_path;

        // Check if buffer content changed
        let content_changed = self.lsp_state.last_buffer_hash != Some(current_hash);

        if file_changed {
            // Close the old document if we had one open
            if let Some(ref old_path) = self.lsp_state.last_synced_path {
                let old_path_str = old_path.to_string_lossy();
                let _ = self.workspace.lsp.close_document(&old_path_str);
            }

            // Open the new document
            if let Some(ref path) = current_path {
                let tab = self.workspace.active_tab();
                let pane = &tab.panes[tab.active_pane];
                let buffer_entry = &tab.buffers[pane.buffer_idx];

                let full_path = if buffer_entry.is_orphan {
                    path.clone()
                } else {
                    self.workspace.root.join(path)
                };
                let path_str = full_path.to_string_lossy();
                let content = self.buffer().contents();
                let _ = self.workspace.lsp.open_document(&path_str, &content);
            }

            self.lsp_state.last_synced_path = current_path;
            self.lsp_state.last_buffer_hash = Some(current_hash);
        } else if content_changed {
            // Content changed - send didChange notification
            if let Some(ref path) = current_path {
                let tab = self.workspace.active_tab();
                let pane = &tab.panes[tab.active_pane];
                let buffer_entry = &tab.buffers[pane.buffer_idx];

                let full_path = if buffer_entry.is_orphan {
                    path.clone()
                } else {
                    self.workspace.root.join(path)
                };
                let path_str = full_path.to_string_lossy();
                let content = self.buffer().contents();
                let _ = self.workspace.lsp.document_changed(&path_str, &content);
            }

            self.lsp_state.last_buffer_hash = Some(current_hash);
        }
    }

    /// Navigate to an LSP location
    fn goto_location(&mut self, location: &Location) {
        use crate::lsp::uri_to_path;

        if let Some(path) = uri_to_path(&location.uri) {
            let path_buf = PathBuf::from(&path);
            // Open the file if not already open
            if let Err(e) = self.workspace.open_file(&path_buf) {
                self.message = Some(format!("Failed to open {}: {}", path, e));
                return;
            }

            // Move cursor to the location
            let line = location.range.start.line as usize;
            let col = location.range.start.character as usize;

            self.cursors_mut().collapse_to_primary();
            self.cursor_mut().line = line.min(self.buffer().line_count().saturating_sub(1));
            self.cursor_mut().col = col.min(self.buffer().line_len(self.cursor().line));
            self.cursor_mut().desired_col = self.cursor().col;
            self.cursor_mut().clear_selection();
            self.scroll_to_cursor();
        }
    }

    /// Get the full path to the current file
    fn current_file_path(&self) -> Option<PathBuf> {
        let tab = self.workspace.active_tab();
        let pane = &tab.panes[tab.active_pane];
        let buffer_entry = &tab.buffers[pane.buffer_idx];

        buffer_entry.path.as_ref().map(|p| {
            if buffer_entry.is_orphan {
                p.clone()
            } else {
                self.workspace.root.join(p)
            }
        })
    }

    /// LSP: Go to definition
    fn lsp_goto_definition(&mut self) {
        if let Some(path) = self.current_file_path() {
            let path_str = path.to_string_lossy().to_string();
            let line = self.cursor().line as u32;
            let col = self.cursor().col as u32;

            match self.workspace.lsp.request_definition(&path_str, line, col) {
                Ok(id) => {
                    self.lsp_state.pending_definition = Some(id);
                    self.message = Some("Finding definition...".to_string());
                }
                Err(e) => {
                    self.message = Some(format!("LSP error: {}", e));
                }
            }
        } else {
            self.message = Some("No file open".to_string());
        }
    }

    /// LSP: Find references
    fn lsp_find_references(&mut self) {
        if let Some(path) = self.current_file_path() {
            let path_str = path.to_string_lossy().to_string();
            let line = self.cursor().line as u32;
            let col = self.cursor().col as u32;

            match self.workspace.lsp.request_references(&path_str, line, col, true) {
                Ok(id) => {
                    self.lsp_state.pending_references = Some(id);
                    self.message = Some("Finding references...".to_string());
                }
                Err(e) => {
                    self.message = Some(format!("LSP error: {}", e));
                }
            }
        } else {
            self.message = Some("No file open".to_string());
        }
    }

    /// LSP: Show hover information
    fn lsp_hover(&mut self) {
        if let Some(path) = self.current_file_path() {
            let path_str = path.to_string_lossy().to_string();
            let line = self.cursor().line as u32;
            let col = self.cursor().col as u32;

            match self.workspace.lsp.request_hover(&path_str, line, col) {
                Ok(id) => {
                    self.lsp_state.pending_hover = Some(id);
                    self.message = Some("Loading hover info...".to_string());
                }
                Err(e) => {
                    self.message = Some(format!("LSP error: {}", e));
                }
            }
        } else {
            self.message = Some("No file open".to_string());
        }
    }

    /// LSP: Trigger completion
    fn lsp_complete(&mut self) {
        if let Some(path) = self.current_file_path() {
            let path_str = path.to_string_lossy().to_string();
            let line = self.cursor().line as u32;
            let col = self.cursor().col as u32;

            match self.workspace.lsp.request_completions(&path_str, line, col) {
                Ok(id) => {
                    self.lsp_state.pending_completion = Some(id);
                    self.message = Some("Loading completions...".to_string());
                }
                Err(e) => {
                    self.message = Some(format!("LSP error: {}", e));
                }
            }
        } else {
            self.message = Some("No file open".to_string());
        }
    }

    /// Toggle the LSP server manager panel
    fn toggle_server_manager(&mut self) {
        if self.server_manager.visible {
            self.server_manager.hide();
        } else {
            self.server_manager.show();
        }
    }

    /// Handle key input when server manager panel is visible
    fn handle_server_manager_key(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        let max_visible = 10; // Should match screen.rs

        // Alt+M toggles the panel closed
        if key == Key::Char('m') && mods.alt {
            self.server_manager.hide();
            return Ok(());
        }

        // Handle confirm mode
        if self.server_manager.confirm_mode {
            match key {
                Key::Char('y') | Key::Char('Y') => {
                    // Start install in background thread (non-blocking)
                    self.server_manager.start_install();
                }
                Key::Char('n') | Key::Char('N') | Key::Escape => {
                    self.server_manager.cancel_confirm();
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle manual info mode
        if self.server_manager.manual_info_mode {
            match key {
                Key::Char('c') | Key::Char('C') => {
                    // Copy install instructions to clipboard
                    if let Some(text) = self.server_manager.get_manual_install_text() {
                        if let Some(ref mut clip) = self.clipboard {
                            if clip.set_text(&text).is_ok() {
                                self.server_manager.mark_copied();
                            } else {
                                self.server_manager.status_message = Some("Failed to copy".to_string());
                            }
                        } else {
                            // Fall back to internal clipboard
                            self.internal_clipboard = text;
                            self.server_manager.mark_copied();
                        }
                    }
                }
                Key::Escape | Key::Char('q') => {
                    self.server_manager.cancel_confirm();
                }
                _ => {}
            }
            return Ok(());
        }

        // Normal panel navigation
        match key {
            Key::Up | Key::Char('k') => {
                self.server_manager.move_up();
            }
            Key::Down | Key::Char('j') => {
                self.server_manager.move_down(max_visible);
            }
            Key::Enter => {
                self.server_manager.enter_confirm_mode();
            }
            Key::Char('r') | Key::Char('R') => {
                self.server_manager.refresh();
            }
            Key::Escape | Key::Char('q') => {
                self.server_manager.hide();
            }
            _ => {}
        }

        Ok(())
    }

    /// LSP: Rename symbol - opens prompt for new name
    fn lsp_rename(&mut self) {
        if let Some(path) = self.current_file_path() {
            let path_str = path.to_string_lossy().to_string();
            let line = self.cursor().line as u32;
            let col = self.cursor().col as u32;

            // Get the word under cursor to show in prompt
            let buffer = self.buffer();
            let cursor = self.cursor();
            let current_word = if let Some(line_slice) = buffer.line(cursor.line) {
                let line_text: String = line_slice.chars().collect();
                let mut start = cursor.col;
                let mut end = cursor.col;

                // Find word boundaries
                while start > 0 {
                    let ch = line_text.chars().nth(start - 1).unwrap_or(' ');
                    if ch.is_alphanumeric() || ch == '_' {
                        start -= 1;
                    } else {
                        break;
                    }
                }
                while end < line_text.len() {
                    let ch = line_text.chars().nth(end).unwrap_or(' ');
                    if ch.is_alphanumeric() || ch == '_' {
                        end += 1;
                    } else {
                        break;
                    }
                }
                line_text[start..end].to_string()
            } else {
                String::new()
            };

            if current_word.is_empty() {
                self.message = Some("No symbol under cursor".to_string());
                return;
            }

            self.prompt = PromptState::RenameModal {
                original_name: current_word,
                new_name: String::new(),
                path: path_str,
                line,
                col,
            };
        } else {
            self.message = Some("No file open".to_string());
        }
    }

    /// Accept the currently selected completion and insert it
    fn accept_completion(&mut self) {
        if self.lsp_state.completions.is_empty() {
            return;
        }

        let completion = self.lsp_state.completions[self.lsp_state.completion_index].clone();

        // Determine the text to insert
        let insert_text = if let Some(ref text_edit) = completion.text_edit {
            // Use text edit if provided (includes range to replace)
            // For now, just use the new text - proper range replacement would be more complex
            text_edit.new_text.clone()
        } else if let Some(ref insert) = completion.insert_text {
            insert.clone()
        } else {
            completion.label.clone()
        };

        // Find the start of the word being completed (walk back from cursor)
        let buffer = self.buffer();
        let cursor = self.cursor();
        let line_idx = cursor.line;
        let cursor_col = cursor.col;
        let mut word_start = cursor_col;

        // Walk back to find word start (alphanumeric or underscore)
        if let Some(line_slice) = buffer.line(line_idx) {
            let line_text: String = line_slice.chars().collect();
            while word_start > 0 {
                let prev_char = line_text.chars().nth(word_start - 1).unwrap_or(' ');
                if prev_char.is_alphanumeric() || prev_char == '_' {
                    word_start -= 1;
                } else {
                    break;
                }
            }
        }

        // Delete the partial word and insert completion
        if word_start < cursor_col {
            // Select from word start to cursor
            let cursor = self.cursor_mut();
            cursor.anchor_line = cursor.line;
            cursor.anchor_col = word_start;
            cursor.selecting = true;
        }

        // Insert the completion text (this will replace selection if any)
        for ch in insert_text.chars() {
            self.insert_char(ch);
        }

        // Clear completion state
        self.lsp_state.completion_visible = false;
        self.lsp_state.completions.clear();
        self.lsp_state.completion_index = 0;
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

    /// Process a mouse event
    fn process_mouse(&mut self, mouse_event: MouseEvent) -> Result<()> {
        if let Some(mouse) = Mouse::from_crossterm(mouse_event) {
            self.handle_mouse(mouse)?;
        }
        Ok(())
    }

    /// Handle mouse input
    fn handle_mouse(&mut self, mouse: Mouse) -> Result<()> {
        // Calculate offsets for fuss mode and tab bar
        let left_offset = if self.workspace.fuss.active {
            self.workspace.fuss.width(self.screen.cols) as usize
        } else {
            0
        };
        let top_offset = if self.workspace.tabs.len() > 1 { 1 } else { 0 };

        // Calculate line number column width (same as in screen.rs)
        let line_num_width = {
            let line_count = self.buffer().line_count();
            let digits = if line_count == 0 { 1 } else { (line_count as f64).log10().floor() as usize + 1 };
            digits.max(3)
        };
        let text_start_col = left_offset + line_num_width + 1;

        match mouse {
            Mouse::Click { button: Button::Left, col, row, modifiers } => {
                // Convert screen coordinates to buffer coordinates
                let screen_row = row as usize;
                let screen_col = col as usize;

                // Check if click is in the text area (not line numbers, not status bar, not fuss pane)
                let status_row = self.screen.rows.saturating_sub(1) as usize;
                if screen_row >= top_offset && screen_row < status_row && screen_col >= text_start_col {
                    // Calculate buffer position (accounting for top_offset)
                    let buffer_line = self.viewport_line() + (screen_row - top_offset);
                    let buffer_col = screen_col - text_start_col;

                    // Clamp to valid positions
                    if buffer_line < self.buffer().line_count() {
                        let line_len = self.buffer().line_len(buffer_line);
                        let clamped_col = buffer_col.min(line_len);

                        if modifiers.ctrl {
                            // Ctrl+click: add or remove cursor at position
                            self.toggle_cursor_at(buffer_line, clamped_col);
                        } else {
                            // Normal click: move cursor to clicked position
                            self.cursors_mut().collapse_to_primary();
                            self.cursor_mut().line = buffer_line;
                            self.cursor_mut().col = clamped_col;
                            self.cursor_mut().desired_col = clamped_col;
                            self.cursor_mut().clear_selection();
                        }
                    }
                }
            }
            Mouse::Drag { button: Button::Left, col, row, .. } => {
                // Extend selection while dragging
                let screen_row = row as usize;
                let screen_col = col as usize;

                let status_row = self.screen.rows.saturating_sub(1) as usize;
                if screen_row >= top_offset && screen_row < status_row && screen_col >= text_start_col {
                    let buffer_line = self.viewport_line() + (screen_row - top_offset);
                    let buffer_col = screen_col - text_start_col;

                    if buffer_line < self.buffer().line_count() {
                        let line_len = self.buffer().line_len(buffer_line);
                        let clamped_col = buffer_col.min(line_len);

                        // Start selection if not already selecting
                        if !self.cursor().selecting {
                            self.cursor_mut().start_selection();
                        }

                        // Move cursor (extends selection)
                        self.cursor_mut().line = buffer_line;
                        self.cursor_mut().col = clamped_col;
                        self.cursor_mut().desired_col = clamped_col;
                    }
                }
            }
            Mouse::ScrollUp { .. } => {
                // Scroll up 3 lines
                let new_line = self.viewport_line().saturating_sub(3);
                self.set_viewport_line(new_line);
            }
            Mouse::ScrollDown { .. } => {
                // Scroll down 3 lines
                // Calculate visible rows (accounting for tab bar, gap, and status bar)
                let top_offset = if self.workspace.tabs.len() > 1 { 1 } else { 0 };
                let visible_rows = (self.screen.rows as usize).saturating_sub(2 + top_offset);
                // Max viewport is when the last line is at the bottom of visible area
                let max_viewport = self.buffer().line_count().saturating_sub(visible_rows).max(0);
                let new_line = (self.viewport_line() + 3).min(max_viewport);
                self.set_viewport_line(new_line);
            }
            _ => {}
        }

        Ok(())
    }

    fn render(&mut self) -> Result<()> {
        // Calculate fuss pane width if active
        let fuss_width = if self.workspace.fuss.active {
            self.workspace.fuss.width(self.screen.cols)
        } else {
            0
        };

        // Render fuss mode sidebar if active
        if self.workspace.fuss.active {
            let visible_rows = self.screen.rows.saturating_sub(2) as usize;
            self.workspace.fuss.update_viewport(visible_rows);

            if let Some(ref tree) = self.workspace.fuss.tree {
                let repo_name = self.workspace.repo_name();
                let branch = self.workspace.git_branch();
                self.screen.render_fuss(
                    tree.visible_items(),
                    self.workspace.fuss.selected,
                    self.workspace.fuss.scroll,
                    fuss_width,
                    self.workspace.fuss.hints_expanded,
                    &repo_name,
                    branch.as_deref(),
                )?;
            }
        }

        // Build tab info for tab bar
        let tabs: Vec<TabInfo> = self.workspace.tabs.iter().enumerate().map(|(i, tab)| {
            TabInfo {
                name: tab.display_name(),
                is_active: i == self.workspace.active_tab,
                is_modified: tab.is_modified(),
                index: i,
            }
        }).collect();

        // Render tab bar (returns height: 1 if multiple tabs, 0 if single tab)
        let top_offset = self.screen.render_tab_bar(&tabs, fuss_width)?;

        let tab = self.workspace.active_tab();
        let pane = &tab.panes[tab.active_pane];
        let filename = tab.buffers[pane.buffer_idx].path.as_ref().and_then(|p| p.to_str());

        // Use multi-pane rendering if we have more than one pane
        if tab.panes.len() > 1 {
            // Build PaneInfo for each pane
            let pane_infos: Vec<PaneInfo> = tab.panes.iter().enumerate().map(|(i, pane)| {
                let buffer_entry = &tab.buffers[pane.buffer_idx];
                let buffer = &buffer_entry.buffer;
                let cursor = pane.cursors.primary();
                let bracket_match = buffer.find_matching_bracket(cursor.line, cursor.col);

                PaneInfo {
                    buffer,
                    cursors: &pane.cursors,
                    viewport_line: pane.viewport_line,
                    bounds: RenderPaneBounds {
                        x_start: pane.bounds.x_start,
                        y_start: pane.bounds.y_start,
                        x_end: pane.bounds.x_end,
                        y_end: pane.bounds.y_end,
                    },
                    is_active: i == tab.active_pane,
                    bracket_match,
                    is_modified: buffer_entry.is_modified(),
                }
            }).collect();

            self.screen.render_panes(
                &pane_infos,
                filename,
                self.message.as_deref(),
                fuss_width,
                top_offset,
            )
        } else {
            // Single pane - use simpler render path with syntax highlighting
            let pane = &tab.panes[tab.active_pane];
            let buffer_entry = &tab.buffers[pane.buffer_idx];
            let buffer = &buffer_entry.buffer;
            let cursors = &pane.cursors;
            let viewport_line = pane.viewport_line;
            let viewport_col = pane.viewport_col;
            let is_modified = buffer_entry.is_modified();
            let highlighter = &buffer_entry.highlighter;

            // Find matching bracket for primary cursor
            let cursor = cursors.primary();
            let bracket_match = buffer.find_matching_bracket(cursor.line, cursor.col);

            self.screen.render_with_syntax(
                buffer,
                cursors,
                viewport_line,
                viewport_col,
                filename,
                self.message.as_deref(),
                bracket_match,
                fuss_width,
                top_offset,
                is_modified,
                highlighter,
            )?;

            // Render diagnostics markers in gutter
            if !self.lsp_state.diagnostics.is_empty() {
                self.screen.render_diagnostics_gutter(
                    &self.lsp_state.diagnostics,
                    viewport_line,
                    fuss_width,
                    top_offset,
                )?;
            }

            // Render completion popup if visible
            if self.lsp_state.completion_visible && !self.lsp_state.completions.is_empty() {
                let cursor = cursors.primary();
                // Calculate cursor screen position
                let cursor_row = (cursor.line.saturating_sub(viewport_line)) as u16 + top_offset;
                let line_num_width = self.screen.line_number_width(buffer.line_count()) as u16;
                let cursor_col = cursor.col as u16 + line_num_width + 1;

                self.screen.render_completion_popup(
                    &self.lsp_state.completions,
                    self.lsp_state.completion_index,
                    cursor_row,
                    cursor_col,
                    fuss_width,
                )?;
            }

            // Render hover popup if visible
            if self.lsp_state.hover_visible {
                if let Some(ref hover) = self.lsp_state.hover {
                    let cursor = cursors.primary();
                    let cursor_row = (cursor.line.saturating_sub(viewport_line)) as u16 + top_offset;
                    let line_num_width = self.screen.line_number_width(buffer.line_count()) as u16;
                    let cursor_col = cursor.col as u16 + line_num_width + 1;

                    self.screen.render_hover_popup(
                        hover,
                        cursor_row,
                        cursor_col,
                        fuss_width,
                    )?;
                }
            }

            // Render server manager panel if visible (on top of everything)
            if self.server_manager.visible {
                self.screen.render_server_manager_panel(&self.server_manager)?;
            }

            // Render rename modal if active
            if let PromptState::RenameModal { ref original_name, ref new_name, .. } = self.prompt {
                self.screen.render_rename_modal(original_name, new_name)?;
            }

            // Render references panel if active
            if let PromptState::ReferencesPanel { ref locations, selected_index, ref query } = self.prompt {
                self.screen.render_references_panel(locations, selected_index, query, &self.workspace.root)?;
            }

            // After all overlays are rendered, reposition cursor to the correct location
            // (overlays may have moved the terminal cursor position)
            let cursor = cursors.primary();
            let cursor_row = (cursor.line.saturating_sub(viewport_line)) as u16 + top_offset;
            let line_num_width = self.screen.line_number_width(buffer.line_count()) as u16;
            // Account for horizontal scroll offset
            let cursor_screen_col = fuss_width + line_num_width + 1 + (cursor.col.saturating_sub(viewport_col)) as u16;
            self.screen.show_cursor_at(cursor_screen_col, cursor_row)?;

            Ok(())
        }
    }

    fn handle_key_with_mods(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        // Handle active prompts first
        if self.prompt != PromptState::None {
            return self.handle_prompt_key(key);
        }

        // Handle server manager panel when visible
        if self.server_manager.visible {
            return self.handle_server_manager_key(key, mods);
        }

        // Clear message on any key
        self.message = None;

        // Toggle fuss mode: Ctrl+B or F3 (works in both modes)
        if matches!((&key, &mods), (Key::Char('b'), Modifiers { ctrl: true, .. }) | (Key::F(3), _)) {
            self.toggle_fuss_mode();
            return Ok(());
        }

        // Route to fuss mode handler if active
        if self.workspace.fuss.active {
            return self.handle_fuss_key(key, mods);
        }

        // Handle completion popup navigation when visible
        if self.lsp_state.completion_visible {
            match (&key, &mods) {
                // Navigate up in completion list
                (Key::Up, _) => {
                    if self.lsp_state.completion_index > 0 {
                        self.lsp_state.completion_index -= 1;
                    } else {
                        // Wrap to bottom
                        self.lsp_state.completion_index = self.lsp_state.completions.len().saturating_sub(1);
                    }
                    return Ok(());
                }
                // Navigate down in completion list
                (Key::Down, _) => {
                    if self.lsp_state.completion_index < self.lsp_state.completions.len().saturating_sub(1) {
                        self.lsp_state.completion_index += 1;
                    } else {
                        // Wrap to top
                        self.lsp_state.completion_index = 0;
                    }
                    return Ok(());
                }
                // Select completion with Enter or Tab
                (Key::Enter, _) | (Key::Tab, _) => {
                    self.accept_completion();
                    return Ok(());
                }
                // Dismiss completion popup with Escape
                (Key::Escape, _) => {
                    self.lsp_state.completion_visible = false;
                    self.lsp_state.completions.clear();
                    return Ok(());
                }
                // Any other key dismisses popup and continues normally
                _ => {
                    self.lsp_state.completion_visible = false;
                    self.lsp_state.completions.clear();
                }
            }
        }

        // Dismiss hover popup on any key press
        if self.lsp_state.hover_visible {
            self.lsp_state.hover_visible = false;
            self.lsp_state.hover = None;
            // Let Escape just dismiss the popup without doing anything else
            if matches!(key, Key::Escape) {
                return Ok(());
            }
        }

        // Break undo group on any non-character key (movement, commands, etc.)
        // This ensures each "typing session" is its own undo unit
        let is_typing = matches!(
            (&key, &mods),
            (Key::Char(_), Modifiers { ctrl: false, alt: false, .. })
        );
        if !is_typing {
            self.history_mut().maybe_break_group();
        }

        match (&key, &mods) {
            // === System ===
            // Quit: Ctrl+Q
            (Key::Char('q'), Modifiers { ctrl: true, .. }) => {
                self.try_quit();
            }
            // Save: Ctrl+S
            (Key::Char('s'), Modifiers { ctrl: true, .. }) => {
                self.save()?;
            }
            // Escape: clear selection and collapse to single cursor
            (Key::Escape, _) => {
                if self.cursors().len() > 1 {
                    self.cursors_mut().collapse_to_primary();
                } else {
                    self.cursors_mut().primary_mut().clear_selection();
                }
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

            // === Multi-cursor operations (must come before other movement to capture Ctrl+Alt) ===
            // Add cursor above: Ctrl+Alt+Up
            (Key::Up, Modifiers { ctrl: true, alt: true, .. }) => self.add_cursor_above(),
            // Add cursor below: Ctrl+Alt+Down
            (Key::Down, Modifiers { ctrl: true, alt: true, .. }) => self.add_cursor_below(),

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

            // === Pane operations ===
            // Split vertical: Alt+V
            (Key::Char('v'), Modifiers { alt: true, .. }) => {
                self.split_vertical();
            }
            // Split horizontal: Alt+S
            (Key::Char('s'), Modifiers { alt: true, .. }) => {
                self.split_horizontal();
            }
            // Close pane/tab: Alt+Q
            (Key::Char('q'), Modifiers { alt: true, .. }) => {
                self.close_pane();
            }
            // Navigate panes: Alt+H/J/K/L (vim-style)
            (Key::Char('h'), Modifiers { alt: true, .. }) => {
                self.navigate_pane_left();
            }
            (Key::Char('j'), Modifiers { alt: true, .. }) => {
                self.navigate_pane_down();
            }
            (Key::Char('k'), Modifiers { alt: true, .. }) => {
                self.navigate_pane_up();
            }
            (Key::Char('l'), Modifiers { alt: true, .. }) => {
                self.navigate_pane_right();
            }
            // Next/Prev pane: Alt+N / Alt+P
            (Key::Char('n'), Modifiers { alt: true, .. }) => {
                self.next_pane();
            }
            (Key::Char('p'), Modifiers { alt: true, .. }) => {
                self.prev_pane();
            }

            // === Tab operations ===
            // Switch to tab by number: Alt+1-9
            (Key::Char('1'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(0),
            (Key::Char('2'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(1),
            (Key::Char('3'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(2),
            (Key::Char('4'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(3),
            (Key::Char('5'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(4),
            (Key::Char('6'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(5),
            (Key::Char('7'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(6),
            (Key::Char('8'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(7),
            (Key::Char('9'), Modifiers { alt: true, .. }) => self.workspace.switch_to_tab(8),
            // Next/Prev tab: Alt+. / Alt+,
            (Key::Char('.'), Modifiers { alt: true, .. }) => self.workspace.next_tab(),
            (Key::Char(','), Modifiers { alt: true, .. }) => self.workspace.prev_tab(),
            // New tab: Alt+T
            (Key::Char('t'), Modifiers { alt: true, .. }) => self.workspace.new_tab(),

            // === LSP operations ===
            // Go to definition: F12
            (Key::F(12), Modifiers { shift: false, .. }) => self.lsp_goto_definition(),
            // Find references: Shift+F12
            (Key::F(12), Modifiers { shift: true, .. }) => self.lsp_find_references(),
            // Hover info: F1
            (Key::F(1), _) => self.lsp_hover(),
            // Code completion: Ctrl+Space
            (Key::Char(' '), Modifiers { ctrl: true, .. }) => self.lsp_complete(),
            // Rename: F2
            (Key::F(2), _) => self.lsp_rename(),
            // Server manager: Alt+M
            (Key::Char('m'), Modifiers { alt: true, .. }) => self.toggle_server_manager(),

            _ => {}
        }

        self.scroll_to_cursor();
        Ok(())
    }

    // === Cursor helpers ===

    /// Get reference to primary cursor
    fn cursor(&self) -> &Cursor {
        self.cursors().primary()
    }

    /// Get mutable reference to primary cursor
    fn cursor_mut(&mut self) -> &mut Cursor {
        self.cursors_mut().primary_mut()
    }

    // === Multi-cursor operations ===

    /// Add a cursor on the line above the topmost cursor
    fn add_cursor_above(&mut self) {
        // Find the topmost cursor
        let topmost = self.cursors().all().iter().map(|c| c.line).min().unwrap_or(0);
        let col = self.cursors().primary().col;

        if topmost > 0 {
            let new_line = topmost - 1;
            let line_len = self.buffer().line_len(new_line);
            let new_col = col.min(line_len);
            self.cursors_mut().add(new_line, new_col);
        }
    }

    /// Add a cursor on the line below the bottommost cursor
    fn add_cursor_below(&mut self) {
        // Find the bottommost cursor
        let bottommost = self.cursors().all().iter().map(|c| c.line).max().unwrap_or(0);
        let col = self.cursors().primary().col;
        let line_count = self.buffer().line_count();

        if bottommost + 1 < line_count {
            let new_line = bottommost + 1;
            let line_len = self.buffer().line_len(new_line);
            let new_col = col.min(line_len);
            self.cursors_mut().add(new_line, new_col);
        }
    }

    /// Toggle cursor at position (for Ctrl+click)
    /// Returns true if cursor was added, false if removed
    fn toggle_cursor_at(&mut self, line: usize, col: usize) -> bool {
        self.cursors_mut().toggle_at(line, col)
    }

    // === Movement ===

    fn move_up(&mut self, extend_selection: bool) {
        // Get line lengths we need before borrowing cursors mutably
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        // Apply to all cursors
        for cursor in self.cursors_mut().all_mut() {
            if cursor.line > 0 {
                let new_line = cursor.line - 1;
                let line_len = line_lens.get(new_line).copied().unwrap_or(0);
                let new_col = cursor.desired_col.min(line_len);
                cursor.move_to(new_line, new_col, extend_selection);
            }
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_down(&mut self, extend_selection: bool) {
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        for cursor in self.cursors_mut().all_mut() {
            if cursor.line + 1 < line_count {
                let new_line = cursor.line + 1;
                let line_len = line_lens.get(new_line).copied().unwrap_or(0);
                let new_col = cursor.desired_col.min(line_len);
                cursor.move_to(new_line, new_col, extend_selection);
            }
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_left(&mut self, extend_selection: bool) {
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        for cursor in self.cursors_mut().all_mut() {
            if cursor.col > 0 {
                cursor.move_to(cursor.line, cursor.col - 1, extend_selection);
                cursor.desired_col = cursor.col;
            } else if cursor.line > 0 {
                let new_line = cursor.line - 1;
                let new_col = line_lens.get(new_line).copied().unwrap_or(0);
                cursor.move_to(new_line, new_col, extend_selection);
                cursor.desired_col = cursor.col;
            }
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_right(&mut self, extend_selection: bool) {
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        for cursor in self.cursors_mut().all_mut() {
            let line_len = line_lens.get(cursor.line).copied().unwrap_or(0);
            if cursor.col < line_len {
                cursor.move_to(cursor.line, cursor.col + 1, extend_selection);
                cursor.desired_col = cursor.col;
            } else if cursor.line + 1 < line_count {
                cursor.move_to(cursor.line + 1, 0, extend_selection);
                cursor.desired_col = 0;
            }
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_word_left(&mut self, extend_selection: bool) {
        // Collect line data before borrowing cursors mutably
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();
        let line_strs: Vec<String> = (0..line_count)
            .map(|l| self.buffer().line_str(l).unwrap_or_default())
            .collect();

        for cursor in self.cursors_mut().all_mut() {
            let (mut line, mut col) = (cursor.line, cursor.col);

            // If at start of line, go to end of previous line
            if col == 0 && line > 0 {
                line -= 1;
                col = line_lens.get(line).copied().unwrap_or(0);
            }

            if let Some(line_str) = line_strs.get(line) {
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

            cursor.move_to(line, col, extend_selection);
            cursor.desired_col = col;
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_word_right(&mut self, extend_selection: bool) {
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();
        let line_strs: Vec<String> = (0..line_count)
            .map(|l| self.buffer().line_str(l).unwrap_or_default())
            .collect();

        for cursor in self.cursors_mut().all_mut() {
            let (mut line, mut col) = (cursor.line, cursor.col);
            let line_len = line_lens.get(line).copied().unwrap_or(0);

            // If at end of line, go to start of next line
            if col >= line_len && line + 1 < line_count {
                line += 1;
                col = 0;
            }

            if let Some(line_str) = line_strs.get(line) {
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

            cursor.move_to(line, col, extend_selection);
            cursor.desired_col = col;
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_home(&mut self, extend_selection: bool) {
        for cursor in self.cursors_mut().all_mut() {
            let line = cursor.line;
            cursor.move_to(line, 0, extend_selection);
            cursor.desired_col = 0;
        }
        self.cursors_mut().merge_overlapping();
    }

    fn smart_home(&mut self, extend_selection: bool) {
        // Toggle between column 0 and first non-whitespace
        let line_count = self.buffer().line_count();
        let line_strs: Vec<String> = (0..line_count)
            .map(|l| self.buffer().line_str(l).unwrap_or_default())
            .collect();

        for cursor in self.cursors_mut().all_mut() {
            let line = cursor.line;
            let col = cursor.col;
            if let Some(line_str) = line_strs.get(line) {
                let first_non_ws = line_str.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
                let new_col = if col == first_non_ws || col == 0 {
                    if col == 0 { first_non_ws } else { 0 }
                } else {
                    first_non_ws
                };
                cursor.move_to(line, new_col, extend_selection);
                cursor.desired_col = new_col;
            }
        }
        self.cursors_mut().merge_overlapping();
    }

    fn move_end(&mut self, extend_selection: bool) {
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        for cursor in self.cursors_mut().all_mut() {
            let line = cursor.line;
            let line_len = line_lens.get(line).copied().unwrap_or(0);
            cursor.move_to(line, line_len, extend_selection);
            cursor.desired_col = line_len;
        }
        self.cursors_mut().merge_overlapping();
    }

    fn page_up(&mut self, extend_selection: bool) {
        let page = self.screen.rows.saturating_sub(2) as usize;
        let line_count = self.buffer().line_count();
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        for cursor in self.cursors_mut().all_mut() {
            let new_line = cursor.line.saturating_sub(page);
            let line_len = line_lens.get(new_line).copied().unwrap_or(0);
            let new_col = cursor.desired_col.min(line_len);
            cursor.move_to(new_line, new_col, extend_selection);
        }
        self.cursors_mut().merge_overlapping();
    }

    fn page_down(&mut self, extend_selection: bool) {
        let page = self.screen.rows.saturating_sub(2) as usize;
        let line_count = self.buffer().line_count();
        let max_line = line_count.saturating_sub(1);
        let line_lens: Vec<usize> = (0..line_count).map(|l| self.buffer().line_len(l)).collect();

        for cursor in self.cursors_mut().all_mut() {
            let new_line = (cursor.line + page).min(max_line);
            let line_len = line_lens.get(new_line).copied().unwrap_or(0);
            let new_col = cursor.desired_col.min(line_len);
            cursor.move_to(new_line, new_col, extend_selection);
        }
        self.cursors_mut().merge_overlapping();
    }

    // === Selection ===

    fn select_line(&mut self) {
        // Select the entire current line (including newline if not last line)
        let line_len = self.buffer().line_len(self.cursor().line);
        self.cursor_mut().anchor_line = self.cursor().line;
        self.cursor_mut().anchor_col = 0;
        self.cursor_mut().col = line_len;
        self.cursor_mut().desired_col = line_len;
        self.cursor_mut().selecting = true;
    }

    fn select_word(&mut self) {
        // If primary cursor has a selection, find next occurrence and add cursor there
        if self.cursor().has_selection() {
            self.select_next_occurrence();
            return;
        }

        // No selection - select word at cursor
        if let Some(line_str) = self.buffer().line_str(self.cursor().line) {
            let chars: Vec<char> = line_str.chars().collect();
            let col = self.cursor().col.min(chars.len());

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
                self.cursor_mut().anchor_line = self.cursor().line;
                self.cursor_mut().anchor_col = start;
                self.cursor_mut().col = end;
                self.cursor_mut().desired_col = end;
                self.cursor_mut().selecting = true;
            }
        }
    }

    /// Find the next occurrence of the selected text and add a cursor there
    fn select_next_occurrence(&mut self) {
        // Get the selected text from primary cursor
        let selected_text = {
            let cursor = self.cursor();
            if !cursor.has_selection() {
                return;
            }
            let (start, end) = cursor.selection().ordered();
            let buffer = self.buffer();

            // Extract selected text
            let mut text = String::new();
            for line_idx in start.line..=end.line {
                if let Some(line) = buffer.line_str(line_idx) {
                    let line_start = if line_idx == start.line { start.col } else { 0 };
                    let line_end = if line_idx == end.line { end.col } else { line.len() };
                    if line_start < line_end && line_end <= line.len() {
                        text.push_str(&line[line_start..line_end]);
                    }
                    if line_idx < end.line {
                        text.push('\n');
                    }
                }
            }
            text
        };

        if selected_text.is_empty() {
            return;
        }

        // Find the position to start searching from (after the last cursor with this selection)
        let search_start = {
            let cursors = self.cursors();
            let mut max_pos = (0usize, 0usize);
            for cursor in cursors.all() {
                if cursor.has_selection() {
                    let (_, end) = cursor.selection().ordered();
                    if (end.line, end.col) > max_pos {
                        max_pos = (end.line, end.col);
                    }
                }
            }
            max_pos
        };

        // Search for next occurrence
        let buffer = self.buffer();
        let line_count = buffer.line_count();
        let search_text = &selected_text;

        // Start searching from the line after the last selection end
        for line_idx in search_start.0..line_count {
            if let Some(line) = buffer.line_str(line_idx) {
                let start_col = if line_idx == search_start.0 { search_start.1 } else { 0 };

                // Search for the text in this line (only works for single-line selections for now)
                if !search_text.contains('\n') {
                    if let Some(found_col) = line[start_col..].find(search_text) {
                        let match_start = start_col + found_col;
                        let match_end = match_start + search_text.len();

                        // Add a new cursor with selection at this location
                        self.cursors_mut().add_with_selection(
                            line_idx,
                            match_end,
                            line_idx,
                            match_start,
                        );
                        return;
                    }
                }
            }
        }

        // Wrap around to beginning if not found
        for line_idx in 0..=search_start.0 {
            if let Some(line) = buffer.line_str(line_idx) {
                let end_col = if line_idx == search_start.0 {
                    // Don't search past where we started
                    search_start.1.saturating_sub(search_text.len())
                } else {
                    line.len()
                };

                if !search_text.contains('\n') {
                    if let Some(found_col) = line[..end_col].find(search_text) {
                        let match_start = found_col;
                        let match_end = match_start + search_text.len();

                        // Check if this position already has a cursor
                        let already_has_cursor = self.cursors().all().iter().any(|c| {
                            c.line == line_idx && c.col == match_end
                        });

                        if !already_has_cursor {
                            self.cursors_mut().add_with_selection(
                                line_idx,
                                match_end,
                                line_idx,
                                match_start,
                            );
                            return;
                        }
                    }
                }
            }
        }

        // No more occurrences found
        self.message = Some("No more occurrences".to_string());
    }

    // === Bracket/Quote Operations ===

    fn jump_to_matching_bracket(&mut self) {
        // First check if cursor is on a bracket
        if let Some((line, col)) = self.buffer().find_matching_bracket(self.cursor().line, self.cursor().col) {
            self.cursor_mut().clear_selection();
            self.cursor_mut().line = line;
            self.cursor_mut().col = col;
            self.cursor_mut().desired_col = col;
            return;
        }

        // If not on a bracket, find surrounding brackets and jump to opening
        if let Some((open_idx, close_idx, _, _)) = self.buffer().find_surrounding_brackets(self.cursor().line, self.cursor().col) {
            let cursor_idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
            // Jump to whichever bracket we're not at
            let (target_line, target_col) = if cursor_idx == open_idx + 1 {
                self.buffer().char_to_line_col(close_idx)
            } else {
                self.buffer().char_to_line_col(open_idx)
            };
            self.cursor_mut().clear_selection();
            self.cursor_mut().line = target_line;
            self.cursor_mut().col = target_col;
            self.cursor_mut().desired_col = target_col;
        }
    }

    fn cycle_quotes(&mut self) {
        // Find surrounding quotes (across lines) and cycle: " -> ' -> ` -> "
        if let Some((open_idx, close_idx, quote_char)) = self.buffer().find_surrounding_quotes(self.cursor().line, self.cursor().col) {
            let new_quote = match quote_char {
                '"' => '\'',
                '\'' => '`',
                '`' => '"',
                _ => return,
            };

            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            // Replace closing quote first (to maintain positions)
            self.buffer_mut().delete(close_idx, close_idx + 1);
            self.buffer_mut().insert(close_idx, &new_quote.to_string());
            self.history_mut().record_delete(close_idx, quote_char.to_string(), cursor_before, cursor_before);
            self.history_mut().record_insert(close_idx, new_quote.to_string(), cursor_before, cursor_before);

            // Replace opening quote
            self.buffer_mut().delete(open_idx, open_idx + 1);
            self.buffer_mut().insert(open_idx, &new_quote.to_string());
            self.history_mut().record_delete(open_idx, quote_char.to_string(), cursor_before, cursor_before);
            self.history_mut().record_insert(open_idx, new_quote.to_string(), cursor_before, cursor_before);

            self.history_mut().end_group();
        }
    }

    fn cycle_brackets(&mut self) {
        // Find surrounding brackets (across lines) and cycle: ( -> { -> [ -> (
        if let Some((open_idx, close_idx, open, close)) = self.buffer().find_surrounding_brackets(self.cursor().line, self.cursor().col) {
            let (new_open, new_close) = match open {
                '(' => ('{', '}'),
                '{' => ('[', ']'),
                '[' => ('(', ')'),
                _ => return,
            };

            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            // Replace closing bracket first
            self.buffer_mut().delete(close_idx, close_idx + 1);
            self.buffer_mut().insert(close_idx, &new_close.to_string());
            self.history_mut().record_delete(close_idx, close.to_string(), cursor_before, cursor_before);
            self.history_mut().record_insert(close_idx, new_close.to_string(), cursor_before, cursor_before);

            // Replace opening bracket
            self.buffer_mut().delete(open_idx, open_idx + 1);
            self.buffer_mut().insert(open_idx, &new_open.to_string());
            self.history_mut().record_delete(open_idx, open.to_string(), cursor_before, cursor_before);
            self.history_mut().record_insert(open_idx, new_open.to_string(), cursor_before, cursor_before);

            self.history_mut().end_group();
        }
    }

    fn remove_surrounding(&mut self) {
        // Remove surrounding quotes OR brackets (whichever is innermost/closest)
        let cursor_idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);

        // Find both surrounding quotes and brackets
        let quotes = self.buffer().find_surrounding_quotes(self.cursor().line, self.cursor().col);
        let brackets = self.buffer().find_surrounding_brackets(self.cursor().line, self.cursor().col);

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
        self.history_mut().begin_group();

        // Delete closing first (to maintain open position)
        self.buffer_mut().delete(close_idx, close_idx + 1);
        self.history_mut().record_delete(close_idx, close_char.to_string(), cursor_before, cursor_before);

        // Delete opening
        self.buffer_mut().delete(open_idx, open_idx + 1);
        self.history_mut().record_delete(open_idx, open_char.to_string(), cursor_before, cursor_before);

        // Adjust cursor position
        if cursor_idx > open_idx {
            self.cursor_mut().col = self.cursor().col.saturating_sub(1);
        }
        // Recalculate line/col after deletions
        let new_cursor_idx = if cursor_idx > close_idx {
            cursor_idx - 2
        } else if cursor_idx > open_idx {
            cursor_idx - 1
        } else {
            cursor_idx
        };
        let (new_line, new_col) = self.buffer().char_to_line_col(new_cursor_idx.min(self.buffer().len_chars().saturating_sub(1)));
        self.cursor_mut().line = new_line;
        self.cursor_mut().col = new_col;
        self.cursor_mut().desired_col = new_col;

        self.history_mut().end_group();
    }

    fn remove_surrounding_brackets(&mut self) {
        // Remove only surrounding brackets (not quotes)
        if let Some((open_idx, close_idx, open, close)) = self.buffer().find_surrounding_brackets(self.cursor().line, self.cursor().col) {
            let cursor_idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            // Delete closing first
            self.buffer_mut().delete(close_idx, close_idx + 1);
            self.history_mut().record_delete(close_idx, close.to_string(), cursor_before, cursor_before);

            // Delete opening
            self.buffer_mut().delete(open_idx, open_idx + 1);
            self.history_mut().record_delete(open_idx, open.to_string(), cursor_before, cursor_before);

            // Recalculate cursor position after deletions
            let new_cursor_idx = if cursor_idx > close_idx {
                cursor_idx - 2
            } else if cursor_idx > open_idx {
                cursor_idx - 1
            } else {
                cursor_idx
            };
            let (new_line, new_col) = self.buffer().char_to_line_col(new_cursor_idx.min(self.buffer().len_chars().saturating_sub(1)));
            self.cursor_mut().line = new_line;
            self.cursor_mut().col = new_col;
            self.cursor_mut().desired_col = new_col;

            self.history_mut().end_group();
        }
    }

    // === Editing ===

    fn cursor_pos(&self) -> Position {
        Position::new(self.cursor().line, self.cursor().col)
    }

    /// Get all cursor positions (for multi-cursor undo/redo)
    fn all_cursor_positions(&self) -> Vec<Position> {
        self.cursors().all().iter().map(|c| Position::new(c.line, c.col)).collect()
    }

    fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.cursor().selection_bounds() {
            let start_idx = self.buffer().line_col_to_char(start.line, start.col);
            let end_idx = self.buffer().line_col_to_char(end.line, end.col);

            // Record for undo
            let deleted_text: String = self.buffer().slice(start_idx, end_idx).chars().collect();
            let cursor_before = self.cursor_pos();

            self.buffer_mut().delete(start_idx, end_idx);

            self.cursor_mut().line = start.line;
            self.cursor_mut().col = start.col;
            self.cursor_mut().desired_col = start.col;
            self.cursor_mut().clear_selection();

            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(start_idx, deleted_text, cursor_before, cursor_after);
            self.history_mut().maybe_break_group();

            true
        } else {
            false
        }
    }

    /// Insert text at all cursor positions (for multi-cursor support)
    fn insert_text_multi(&mut self, text: &str) {
        if self.cursors().len() == 1 {
            // Single cursor - use simple path
            self.insert_text_single(text);
            return;
        }

        // Multi-cursor: process from bottom-right to top-left to maintain correct positions.
        // This ordering ensures that when we insert text, we don't affect the character indices
        // of cursors we haven't processed yet (they're all earlier in the document).
        //
        // Collect original cursor positions with indices
        let mut positions: Vec<(usize, usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.line, c.col))
            .collect();

        // Sort by position, bottom-right first (highest line, then highest col)
        positions.sort_by(|a, b| {
            match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2),
                ord => ord,
            }
        });

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        // Count newlines and chars for position updates
        let newlines = text.chars().filter(|&c| c == '\n').count();
        let text_char_count = text.chars().count();
        let chars_after_last_newline = if let Some(pos) = text.rfind('\n') {
            text[pos + 1..].chars().count()
        } else {
            text_char_count
        };

        let cursor_before = self.cursor_pos();

        // Process each cursor using the ORIGINAL positions we captured.
        // Since we go bottom-right to top-left, insertions don't affect positions we'll use later.
        for (cursor_idx, orig_line, orig_col) in positions.iter().copied() {
            let idx = self.buffer().line_col_to_char(orig_line, orig_col);
            self.buffer_mut().insert(idx, text);
            self.history_mut().record_insert(idx, text.to_string(), cursor_before, cursor_before);

            // Update this cursor's final position
            let cursor = &mut self.cursors_mut().all_mut()[cursor_idx];
            if newlines > 0 {
                cursor.line = orig_line + newlines;
                cursor.col = chars_after_last_newline;
            } else {
                cursor.line = orig_line;
                cursor.col = orig_col + text_char_count;
            }
            cursor.desired_col = cursor.col;
        }

        // Record all cursor positions after the operation
        let cursors_after = self.all_cursor_positions();
        self.history_mut().set_cursors_after(cursors_after);
        self.history_mut().end_group();
        self.cursors_mut().merge_overlapping();
    }

    /// Insert text at single (primary) cursor position
    fn insert_text_single(&mut self, text: &str) {
        self.delete_selection();

        let cursor_before = self.cursor_pos();
        let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);

        self.buffer_mut().insert(idx, text);
        self.history_mut().record_insert(idx, text.to_string(), cursor_before, Position::new(0, 0));

        // Update cursor position
        for c in text.chars() {
            if c == '\n' {
                self.cursor_mut().line += 1;
                self.cursor_mut().col = 0;
            } else {
                self.cursor_mut().col += 1;
            }
        }
        self.cursor_mut().desired_col = self.cursor().col;

        // Update the cursor_after in history
        let cursor_after = self.cursor_pos();
        if let Some(op) = self.history_mut().undo_stack_last_mut() {
            if let Operation::Insert { cursor_after: ref mut ca, .. } = op {
                *ca = cursor_after;
            }
        }
    }

    fn insert_text(&mut self, text: &str) {
        self.insert_text_multi(text);
    }

    fn insert_char(&mut self, c: char) {
        // For multi-cursor, use simple insert (skip auto-pair complexity for now)
        if self.cursors().len() > 1 {
            self.insert_text_multi(&c.to_string());
            return;
        }

        // Single cursor: handle auto-pair
        // Check for auto-pair closing: if typing a closing bracket/quote
        // and the next char is the same, just move cursor right
        if let Some(next_char) = self.char_at_cursor() {
            if c == next_char && (c == ')' || c == ']' || c == '}' || c == '"' || c == '\'' || c == '`') {
                self.cursor_mut().col += 1;
                self.cursor_mut().desired_col = self.cursor().col;
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
                let prev_char = if self.cursor().col > 0 {
                    let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
                    self.buffer().char_at(idx.saturating_sub(1))
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
                let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
                let pair_str = format!("{}{}", c, close);

                self.buffer_mut().insert(idx, &pair_str);
                self.cursor_mut().col += 1; // Position cursor between the pair
                self.cursor_mut().desired_col = self.cursor().col;

                let cursor_after = self.cursor_pos();
                self.history_mut().record_insert(idx, pair_str, cursor_before, cursor_after);
                return;
            }
        }

        self.insert_text(&c.to_string());
    }

    /// Get character at cursor position (if any)
    fn char_at_cursor(&self) -> Option<char> {
        let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
        self.buffer().char_at(idx)
    }

    fn insert_newline(&mut self) {
        self.history_mut().maybe_break_group();
        self.insert_text("\n");
        self.history_mut().maybe_break_group();
    }

    fn insert_tab(&mut self) {
        if self.cursor().has_selection() {
            self.indent_selection();
        } else {
            self.insert_text("    ");
        }
    }

    /// Indent all lines in selection
    fn indent_selection(&mut self) {
        if let Some((start, end)) = self.cursor().selection_bounds() {
            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            // Indent each line from start to end (inclusive)
            for line_idx in start.line..=end.line {
                let line_start = self.buffer().line_col_to_char(line_idx, 0);
                let indent = "    ";
                self.buffer_mut().insert(line_start, indent);
                self.history_mut().record_insert(line_start, indent.to_string(), cursor_before, cursor_before);
            }

            // Adjust selection to cover the indented text
            self.cursor_mut().anchor_col += 4;
            self.cursor_mut().col += 4;
            self.cursor_mut().desired_col = self.cursor().col;

            self.history_mut().end_group();
        }
    }

    /// Delete backward at all cursor positions (multi-cursor)
    fn delete_backward_multi(&mut self) {
        // Collect cursor positions, process from bottom to top
        let mut positions: Vec<(usize, usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.line, c.col))
            .collect();

        // Sort by position, bottom-right first
        positions.sort_by(|a, b| {
            match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2),
                ord => ord,
            }
        });

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        let cursor_before = self.cursor_pos();
        for (cursor_idx, line, col) in positions {
            if col > 0 {
                let idx = self.buffer().line_col_to_char(line, col);
                let deleted = self.buffer().char_at(idx - 1).map(|c| c.to_string()).unwrap_or_default();
                self.buffer_mut().delete(idx - 1, idx);
                self.history_mut().record_delete(idx - 1, deleted, cursor_before, cursor_before);

                // Update cursor position
                let cursor = &mut self.cursors_mut().all_mut()[cursor_idx];
                cursor.col -= 1;
                cursor.desired_col = cursor.col;
            }
            // Note: For simplicity, we don't handle joining lines in multi-cursor mode
        }

        // Record all cursor positions after the operation
        let cursors_after = self.all_cursor_positions();
        self.history_mut().set_cursors_after(cursors_after);
        self.history_mut().end_group();
        self.cursors_mut().merge_overlapping();
    }

    /// Delete forward at all cursor positions (multi-cursor)
    fn delete_forward_multi(&mut self) {
        // Collect cursor positions, process from bottom to top
        let mut positions: Vec<(usize, usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.line, c.col))
            .collect();

        // Sort by position, bottom-right first
        positions.sort_by(|a, b| {
            match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2),
                ord => ord,
            }
        });

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        let cursor_before = self.cursor_pos();
        for (_cursor_idx, line, col) in positions {
            let line_len = self.buffer().line_len(line);
            if col < line_len {
                let idx = self.buffer().line_col_to_char(line, col);
                let deleted = self.buffer().char_at(idx).map(|c| c.to_string()).unwrap_or_default();
                self.buffer_mut().delete(idx, idx + 1);
                self.history_mut().record_delete(idx, deleted, cursor_before, cursor_before);
                // Cursor position doesn't change for delete forward
            }
            // Note: For simplicity, we don't handle joining lines in multi-cursor mode
        }

        // Record all cursor positions after the operation
        let cursors_after = self.all_cursor_positions();
        self.history_mut().set_cursors_after(cursors_after);
        self.history_mut().end_group();
        self.cursors_mut().merge_overlapping();
    }

    fn delete_backward(&mut self) {
        // For multi-cursor, use simplified delete
        if self.cursors().len() > 1 {
            self.delete_backward_multi();
            return;
        }

        if self.delete_selection() {
            return;
        }

        if self.cursor().col > 0 {
            let cursor_before = self.cursor_pos();
            let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
            let prev_char = self.buffer().char_at(idx - 1);
            let next_char = self.buffer().char_at(idx);

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
                self.buffer_mut().delete(idx - 1, idx + 1);
                self.cursor_mut().col -= 1;
                self.cursor_mut().desired_col = self.cursor().col;

                let cursor_after = self.cursor_pos();
                self.history_mut().record_delete(idx - 1, deleted, cursor_before, cursor_after);
            } else {
                let deleted = prev_char.map(|c| c.to_string()).unwrap_or_default();

                self.buffer_mut().delete(idx - 1, idx);
                self.cursor_mut().col -= 1;
                self.cursor_mut().desired_col = self.cursor().col;

                let cursor_after = self.cursor_pos();
                self.history_mut().record_delete(idx - 1, deleted, cursor_before, cursor_after);
            }
        } else if self.cursor().line > 0 {
            let cursor_before = self.cursor_pos();
            let prev_line_len = self.buffer().line_len(self.cursor().line - 1);
            let idx = self.buffer().line_col_to_char(self.cursor().line, 0);

            self.buffer_mut().delete(idx - 1, idx);
            self.cursor_mut().line -= 1;
            self.cursor_mut().col = prev_line_len;
            self.cursor_mut().desired_col = self.cursor().col;

            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(idx - 1, "\n".to_string(), cursor_before, cursor_after);
            self.history_mut().maybe_break_group();
        }
    }

    fn delete_forward(&mut self) {
        // For multi-cursor, use simplified delete
        if self.cursors().len() > 1 {
            self.delete_forward_multi();
            return;
        }

        if self.delete_selection() {
            return;
        }

        let line_len = self.buffer().line_len(self.cursor().line);
        let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);

        if self.cursor().col < line_len {
            let cursor_before = self.cursor_pos();
            let deleted = self.buffer().char_at(idx).map(|c| c.to_string()).unwrap_or_default();
            self.buffer_mut().delete(idx, idx + 1);
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(idx, deleted, cursor_before, cursor_after);
        } else if self.cursor().line + 1 < self.buffer().line_count() {
            let cursor_before = self.cursor_pos();
            self.buffer_mut().delete(idx, idx + 1);
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(idx, "\n".to_string(), cursor_before, cursor_after);
            self.history_mut().maybe_break_group();
        }
    }

    fn delete_word_backward(&mut self) {
        // For multi-cursor, use multi version
        if self.cursors().len() > 1 {
            self.delete_word_backward_multi();
            return;
        }

        if self.delete_selection() {
            return;
        }

        let start_col = self.cursor().col;
        self.move_word_left(false);

        if self.cursor_mut().line == self.cursor().line && self.cursor().col < start_col {
            let cursor_before = Position::new(self.cursor().line, start_col);
            let start_idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
            let end_idx = self.buffer().line_col_to_char(self.cursor().line, start_col);
            let deleted: String = self.buffer().slice(start_idx, end_idx).chars().collect();

            self.buffer_mut().delete(start_idx, end_idx);
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(start_idx, deleted, cursor_before, cursor_after);
            self.history_mut().maybe_break_group();
        }
    }

    fn delete_word_backward_multi(&mut self) {
        // Collect cursor positions, process from bottom to top
        let mut cursor_data: Vec<(usize, usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.line, c.col))
            .collect();

        // Sort by position, bottom-right first
        cursor_data.sort_by(|a, b| {
            match b.1.cmp(&a.1) {
                std::cmp::Ordering::Equal => b.2.cmp(&a.2),
                ord => ord,
            }
        });

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        for (cursor_idx, line, col) in cursor_data {
            if col == 0 {
                continue; // Can't delete word at start of line in multi-cursor mode
            }

            // Find word start (same logic as move_word_left)
            let line_str = self.buffer().line_str(line).unwrap_or_default();
            let chars: Vec<char> = line_str.chars().collect();
            let mut new_col = col;

            // Skip whitespace backward
            while new_col > 0 && chars.get(new_col - 1).map(|c| c.is_whitespace()).unwrap_or(false) {
                new_col -= 1;
            }

            // Skip word characters backward
            if new_col > 0 {
                let is_word = chars.get(new_col - 1).map(|c| is_word_char(*c)).unwrap_or(false);
                if is_word {
                    while new_col > 0 && chars.get(new_col - 1).map(|c| is_word_char(*c)).unwrap_or(false) {
                        new_col -= 1;
                    }
                } else {
                    // Skip punctuation
                    while new_col > 0 && chars.get(new_col - 1).map(|c| !c.is_whitespace() && !is_word_char(*c)).unwrap_or(false) {
                        new_col -= 1;
                    }
                }
            }

            if new_col < col {
                let cursor_before = Position::new(line, col);
                let start_idx = self.buffer().line_col_to_char(line, new_col);
                let end_idx = self.buffer().line_col_to_char(line, col);
                let deleted: String = self.buffer().slice(start_idx, end_idx).chars().collect();

                self.buffer_mut().delete(start_idx, end_idx);
                self.history_mut().record_delete(start_idx, deleted, cursor_before, cursor_before);

                // Update cursor position
                let cursor = &mut self.cursors_mut().all_mut()[cursor_idx];
                cursor.col = new_col;
                cursor.desired_col = new_col;
            }
        }

        // Record all cursor positions after the operation
        let cursors_after = self.all_cursor_positions();
        self.history_mut().set_cursors_after(cursors_after);
        self.history_mut().end_group();
        self.cursors_mut().merge_overlapping();
    }

    fn delete_word_forward(&mut self) {
        if self.delete_selection() {
            return;
        }

        let start_line = self.cursor().line;
        let start_col = self.cursor().col;
        self.move_word_right(false);

        let cursor_before = Position::new(start_line, start_col);
        let start_idx = self.buffer().line_col_to_char(start_line, start_col);
        let end_idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);

        if end_idx > start_idx {
            let deleted: String = self.buffer().slice(start_idx, end_idx).chars().collect();
            self.buffer_mut().delete(start_idx, end_idx);
            self.cursor_mut().line = start_line;
            self.cursor_mut().col = start_col;
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(start_idx, deleted, cursor_before, cursor_after);
            self.history_mut().maybe_break_group();
        }
    }

    fn transpose_chars(&mut self) {
        // Transpose the two characters around the cursor
        // If at end of line, swap the two chars before cursor
        // If at start of line, do nothing
        let line_len = self.buffer().line_len(self.cursor().line);
        if line_len < 2 {
            return;
        }

        let (swap_pos, move_cursor) = if self.cursor_mut().col == 0 {
            // At start of line - nothing to transpose
            return;
        } else if self.cursor().col >= line_len {
            // At or past end of line - swap last two chars
            (self.cursor().col - 2, false)
        } else {
            // In middle - swap char before cursor with char at cursor
            (self.cursor().col - 1, true)
        };

        let idx = self.buffer().line_col_to_char(self.cursor().line, swap_pos);
        let char1 = self.buffer().char_at(idx);
        let char2 = self.buffer().char_at(idx + 1);

        if let (Some(c1), Some(c2)) = (char1, char2) {
            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            // Delete both chars
            let deleted = format!("{}{}", c1, c2);
            self.buffer_mut().delete(idx, idx + 2);
            self.history_mut().record_delete(idx, deleted, cursor_before, cursor_before);

            // Insert in swapped order
            let swapped = format!("{}{}", c2, c1);
            self.buffer_mut().insert(idx, &swapped);

            if move_cursor {
                self.cursor_mut().col += 1;
                self.cursor_mut().desired_col = self.cursor().col;
            }

            let cursor_after = self.cursor_pos();
            self.history_mut().record_insert(idx, swapped, cursor_before, cursor_after);
            self.history_mut().end_group();
        }
    }

    fn dedent(&mut self) {
        if self.cursor().has_selection() {
            self.dedent_selection();
        } else {
            self.dedent_line(self.cursor().line);
            self.history_mut().maybe_break_group();
        }
    }

    /// Dedent a single line, returns number of spaces removed
    fn dedent_line(&mut self, line_idx: usize) -> usize {
        if let Some(line_str) = self.buffer().line_str(line_idx) {
            let spaces_to_remove = line_str.chars().take(4).take_while(|c| *c == ' ').count();
            if spaces_to_remove > 0 {
                let cursor_before = self.cursor_pos();
                let line_start = self.buffer().line_col_to_char(line_idx, 0);
                let deleted: String = " ".repeat(spaces_to_remove);

                self.buffer_mut().delete(line_start, line_start + spaces_to_remove);

                // Only adjust cursor if this is the cursor's line
                if line_idx == self.cursor().line {
                    self.cursor_mut().col = self.cursor().col.saturating_sub(spaces_to_remove);
                    self.cursor_mut().desired_col = self.cursor().col;
                }

                let cursor_after = self.cursor_pos();
                self.history_mut().record_delete(line_start, deleted, cursor_before, cursor_after);
                return spaces_to_remove;
            }
        }
        0
    }

    /// Dedent all lines in selection
    fn dedent_selection(&mut self) {
        if let Some((start, end)) = self.cursor().selection_bounds() {
            self.history_mut().begin_group();

            let mut total_removed_anchor_line = 0;
            let mut total_removed_cursor_line = 0;

            // Dedent each line from start to end (inclusive)
            // We need to track adjustments carefully since positions shift
            for line_idx in start.line..=end.line {
                let removed = self.dedent_line(line_idx);
                if line_idx == self.cursor().anchor_line {
                    total_removed_anchor_line = removed;
                }
                if line_idx == self.cursor().line {
                    total_removed_cursor_line = removed;
                }
            }

            // Adjust selection columns
            self.cursor_mut().anchor_col = self.cursor().anchor_col.saturating_sub(total_removed_anchor_line);
            self.cursor_mut().col = self.cursor().col.saturating_sub(total_removed_cursor_line);
            self.cursor_mut().desired_col = self.cursor().col;

            self.history_mut().end_group();
        }
    }

    // === Line operations ===

    fn move_line_up(&mut self) {
        if self.cursor().line > 0 {
            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            let curr_line = self.cursor().line;
            let prev_line = curr_line - 1;

            let curr_content = self.buffer().line_str(curr_line).unwrap_or_default();

            // Delete current line (including its newline)
            let curr_start = self.buffer().line_col_to_char(curr_line, 0);
            let delete_start = curr_start.saturating_sub(1); // Include newline before
            let delete_end = curr_start + curr_content.len();
            let deleted: String = self.buffer().slice(delete_start, delete_end).chars().collect();
            self.buffer_mut().delete(delete_start, delete_end);
            self.history_mut().record_delete(delete_start, deleted, cursor_before, cursor_before);

            // Insert current line before previous line
            let prev_start = self.buffer().line_col_to_char(prev_line, 0);
            let insert_text = format!("{}\n", curr_content);
            let cursor_col = self.cursor().col;
            self.buffer_mut().insert(prev_start, &insert_text);
            self.history_mut().record_insert(prev_start, insert_text, cursor_before, Position::new(prev_line, cursor_col));

            self.cursor_mut().line = prev_line;
            self.history_mut().end_group();
        }
    }

    fn move_line_down(&mut self) {
        if self.cursor().line + 1 < self.buffer().line_count() {
            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            let curr_line = self.cursor().line;
            let next_line = curr_line + 1;

            let curr_content = self.buffer().line_str(curr_line).unwrap_or_default();

            // Delete current line (including newline after)
            let curr_start = self.buffer().line_col_to_char(curr_line, 0);
            let next_start = self.buffer().line_col_to_char(next_line, 0);
            let deleted: String = self.buffer().slice(curr_start, next_start).chars().collect();
            self.buffer_mut().delete(curr_start, next_start);
            self.history_mut().record_delete(curr_start, deleted, cursor_before, cursor_before);

            // Insert current line after what was the next line (now at curr_line)
            let new_line_end = self.buffer().line_col_to_char(curr_line, self.buffer().line_len(curr_line));
            let insert_text = format!("\n{}", curr_content);
            let cursor_col = self.cursor().col;
            self.buffer_mut().insert(new_line_end, &insert_text);
            self.history_mut().record_insert(new_line_end, insert_text, cursor_before, Position::new(next_line, cursor_col));

            self.cursor_mut().line = next_line;
            self.history_mut().end_group();
        }
    }

    fn duplicate_line_up(&mut self) {
        let cursor_before = self.cursor_pos();
        self.history_mut().begin_group();
        let content = self.buffer().line_str(self.cursor().line).unwrap_or_default();
        let line_start = self.buffer().line_col_to_char(self.cursor().line, 0);
        let insert_text = format!("{}\n", content);
        self.buffer_mut().insert(line_start, &insert_text);
        // Cursor stays on same logical line (now shifted down by 1)
        self.cursor_mut().line += 1;
        let cursor_after = self.cursor_pos();
        self.history_mut().record_insert(line_start, insert_text, cursor_before, cursor_after);
        self.history_mut().end_group();
    }

    fn duplicate_line_down(&mut self) {
        let cursor_before = self.cursor_pos();
        self.history_mut().begin_group();
        let content = self.buffer().line_str(self.cursor().line).unwrap_or_default();
        let line_end = self.buffer().line_col_to_char(self.cursor().line, self.buffer().line_len(self.cursor().line));
        let insert_text = format!("\n{}", content);
        self.buffer_mut().insert(line_end, &insert_text);
        self.cursor_mut().line += 1;
        let cursor_after = self.cursor_pos();
        self.history_mut().record_insert(line_end, insert_text, cursor_before, cursor_after);
        self.history_mut().end_group();
    }

    fn join_lines(&mut self) {
        if self.cursor().line + 1 < self.buffer().line_count() {
            let cursor_before = self.cursor_pos();
            self.history_mut().begin_group();

            let line_len = self.buffer().line_len(self.cursor().line);
            let idx = self.buffer().line_col_to_char(self.cursor().line, line_len);

            // Delete newline
            self.buffer_mut().delete(idx, idx + 1);

            // Move cursor to join point
            self.cursor_mut().col = line_len;
            self.cursor_mut().desired_col = self.cursor().col;

            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(idx, "\n".to_string(), cursor_before, cursor_after);
            self.history_mut().end_group();
        }
    }

    // === Clipboard ===

    fn get_selection_text(&self) -> Option<String> {
        self.cursor().selection_bounds().map(|(start, end)| {
            let start_idx = self.buffer().line_col_to_char(start.line, start.col);
            let end_idx = self.buffer().line_col_to_char(end.line, end.col);
            self.buffer().slice(start_idx, end_idx).chars().collect()
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
            if let Some(line) = self.buffer().line_str(self.cursor().line) {
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
            if let Some(line) = self.buffer().line_str(self.cursor().line) {
                self.set_clipboard(format!("{}\n", line));
                let cursor_before = self.cursor_pos();

                let line_start = self.buffer().line_col_to_char(self.cursor().line, 0);

                if self.cursor().line + 1 < self.buffer().line_count() {
                    // Not the last line - delete line including its newline
                    let line_end = line_start + line.len() + 1;
                    let deleted: String = self.buffer().slice(line_start, line_end).chars().collect();
                    self.buffer_mut().delete(line_start, line_end);
                    self.cursor_mut().col = 0;
                    self.cursor_mut().desired_col = 0;
                    let cursor_after = self.cursor_pos();
                    self.history_mut().record_delete(line_start, deleted, cursor_before, cursor_after);
                } else if self.cursor().line > 0 {
                    // Last line with content - delete newline before it and the line
                    let delete_start = line_start.saturating_sub(1);
                    let delete_end = line_start + line.len();
                    let deleted: String = self.buffer().slice(delete_start, delete_end).chars().collect();
                    self.buffer_mut().delete(delete_start, delete_end);
                    self.cursor_mut().line -= 1;
                    self.cursor_mut().col = 0;
                    self.cursor_mut().desired_col = 0;
                    let cursor_after = self.cursor_pos();
                    self.history_mut().record_delete(delete_start, deleted, cursor_before, cursor_after);
                } else {
                    // Only line - just clear it
                    if !line.is_empty() {
                        self.buffer_mut().delete(line_start, line_start + line.len());
                        self.cursor_mut().col = 0;
                        self.cursor_mut().desired_col = 0;
                        let cursor_after = self.cursor_pos();
                        self.history_mut().record_delete(line_start, line.clone(), cursor_before, cursor_after);
                    }
                }

                self.message = Some("Cut line".to_string());
            }
        }
        self.history_mut().maybe_break_group();
    }

    fn paste(&mut self) {
        let text = self.get_clipboard();
        if !text.is_empty() {
            self.insert_text(&text);
            self.message = Some("Pasted".to_string());
            self.history_mut().maybe_break_group();
        }
    }

    // === Undo/Redo ===

    fn undo(&mut self) {
        if let Some((ops, cursor_positions)) = self.history_mut().undo() {
            // Apply operations in reverse
            for op in ops.into_iter().rev() {
                match op {
                    Operation::Insert { pos, text, .. } => {
                        self.buffer_mut().delete(pos, pos + text.chars().count());
                    }
                    Operation::Delete { pos, text, .. } => {
                        self.buffer_mut().insert(pos, &text);
                    }
                }
            }
            // Restore cursor positions from before the operation
            self.cursors_mut().set_from_positions(&cursor_positions);
            self.cursors_mut().clear_selections();
            self.message = Some("Undo".to_string());
        }
    }

    fn redo(&mut self) {
        if let Some((ops, cursor_positions)) = self.history_mut().redo() {
            // Apply operations forward
            for op in ops {
                match op {
                    Operation::Insert { pos, text, .. } => {
                        self.buffer_mut().insert(pos, &text);
                    }
                    Operation::Delete { pos, text, .. } => {
                        self.buffer_mut().delete(pos, pos + text.chars().count());
                    }
                }
            }
            // Restore cursor positions from after the operation
            self.cursors_mut().set_from_positions(&cursor_positions);
            self.cursors_mut().clear_selections();
            self.message = Some("Redo".to_string());
        }
    }

    // === Viewport ===

    fn scroll_to_cursor(&mut self) {
        // Calculate top offset (tab bar takes 1 row if multiple tabs)
        let top_offset = if self.workspace.tabs.len() > 1 { 1 } else { 0 };
        // Vertical scrolling (2 rows reserved: gap + status bar, plus top_offset for tab bar)
        let visible_rows = (self.screen.rows as usize).saturating_sub(2 + top_offset);
        let cursor_line = self.cursor().line;
        let viewport_line = self.viewport_line();

        if cursor_line < viewport_line {
            self.set_viewport_line(cursor_line);
        }

        if cursor_line >= viewport_line + visible_rows {
            self.set_viewport_line(cursor_line - visible_rows + 1);
        }

        // Horizontal scrolling
        let line_num_width = self.screen.line_number_width(self.buffer().line_count());
        let fuss_width = if self.workspace.fuss.active {
            self.workspace.fuss.width(self.screen.cols)
        } else {
            0
        };
        // Available text columns = screen width - fuss sidebar - line numbers - 1 (separator)
        let visible_cols = (self.screen.cols as usize)
            .saturating_sub(fuss_width as usize)
            .saturating_sub(line_num_width + 1);

        let cursor_col = self.cursor().col;
        let viewport_col = self.viewport_col();

        // Keep some margin (3 chars) so cursor isn't right at the edge
        let margin = 3;

        if cursor_col < viewport_col {
            // Cursor is left of viewport - scroll left
            self.set_viewport_col(cursor_col.saturating_sub(margin));
        }

        if cursor_col >= viewport_col + visible_cols.saturating_sub(margin) {
            // Cursor is right of viewport - scroll right
            self.set_viewport_col(cursor_col.saturating_sub(visible_cols.saturating_sub(margin + 1)));
        }
    }

    // === File operations ===

    fn save(&mut self) -> Result<()> {
        let path = self.filename();
        if let Some(ref p) = path {
            self.buffer_mut().save(p)?;
            self.buffer_entry_mut().mark_saved();
            self.message = Some("Saved".to_string());
        }
        Ok(())
    }

    // === Pane operations ===

    fn split_vertical(&mut self) {
        self.tab_mut().split_vertical();
        self.message = Some("Split vertical".to_string());
    }

    fn split_horizontal(&mut self) {
        self.tab_mut().split_horizontal();
        self.message = Some("Split horizontal".to_string());
    }

    fn close_pane(&mut self) {
        if self.workspace.active_tab_mut().close_active_pane() {
            // Last pane was closed - close the tab
            if self.workspace.close_active_tab() {
                // Last tab - quit the editor
                self.running = false;
            } else {
                self.message = Some("Tab closed".to_string());
            }
        } else {
            self.message = Some("Pane closed".to_string());
        }
    }

    fn next_pane(&mut self) {
        self.tab_mut().next_pane();
    }

    fn prev_pane(&mut self) {
        self.tab_mut().prev_pane();
    }

    fn navigate_pane_left(&mut self) {
        self.tab_mut().navigate_pane(PaneDirection::Left);
    }

    fn navigate_pane_right(&mut self) {
        self.tab_mut().navigate_pane(PaneDirection::Right);
    }

    fn navigate_pane_up(&mut self) {
        self.tab_mut().navigate_pane(PaneDirection::Up);
    }

    fn navigate_pane_down(&mut self) {
        self.tab_mut().navigate_pane(PaneDirection::Down);
    }

    // === Fuss mode (file tree) ===

    fn toggle_fuss_mode(&mut self) {
        if !self.workspace.fuss.active {
            self.workspace.fuss.activate(&self.workspace.root);
        } else {
            self.workspace.fuss.deactivate();
        }
    }

    fn handle_fuss_key(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        match (&key, &mods) {
            // Quit: Ctrl+Q (still works in fuss mode)
            (Key::Char('q'), Modifiers { ctrl: true, .. }) => {
                self.try_quit();
            }

            // Exit fuss mode (Escape or F3)
            (Key::Escape, _) | (Key::F(3), _) => {
                self.workspace.fuss.deactivate();
            }

            // Navigation
            (Key::Up, _) | (Key::Char('k'), Modifiers { ctrl: false, alt: false, .. }) => {
                self.workspace.fuss.move_up();
            }
            (Key::Down, _) | (Key::Char('j'), Modifiers { ctrl: false, alt: false, .. }) => {
                self.workspace.fuss.move_down();
            }

            // Toggle expand/collapse directory, or collapse parent if on a file/collapsed dir
            (Key::Char(' '), _) => {
                if self.workspace.fuss.is_dir_selected() {
                    // If on a directory, toggle its expand state
                    self.workspace.fuss.toggle_expand();
                } else {
                    // If on a file, collapse parent directory
                    self.workspace.fuss.collapse_parent();
                }
            }

            // Expand directory (right arrow)
            (Key::Right, _) => {
                if self.workspace.fuss.is_dir_selected() {
                    // Only expand if not already expanded
                    if let Some(ref tree) = self.workspace.fuss.tree {
                        let items = tree.visible_items();
                        if let Some(item) = items.get(self.workspace.fuss.selected) {
                            if item.is_dir && !item.expanded {
                                self.workspace.fuss.toggle_expand();
                            }
                        }
                    }
                }
            }

            // Collapse directory or go to parent (left arrow)
            (Key::Left, _) => {
                let mut collapsed = false;
                if self.workspace.fuss.is_dir_selected() {
                    // If on an expanded directory, collapse it
                    if let Some(ref tree) = self.workspace.fuss.tree {
                        let items = tree.visible_items();
                        if let Some(item) = items.get(self.workspace.fuss.selected) {
                            if item.is_dir && item.expanded {
                                self.workspace.fuss.toggle_expand();
                                collapsed = true;
                            }
                        }
                    }
                }
                // If not collapsed (either a file or already-collapsed dir), go to parent
                if !collapsed {
                    self.workspace.fuss.collapse_parent();
                }
            }

            // Open file or toggle directory
            (Key::Enter, _) | (Key::Char('o'), Modifiers { ctrl: false, alt: false, .. }) => {
                if self.workspace.fuss.is_dir_selected() {
                    self.workspace.fuss.toggle_expand();
                } else if let Some(path) = self.workspace.fuss.selected_file() {
                    self.open_file(&path)?;
                    self.workspace.fuss.deactivate();
                }
            }

            // Toggle hidden files
            (Key::Char('.'), _) => {
                self.workspace.fuss.toggle_hidden();
            }

            // Toggle hints (Ctrl+/ may send different codes depending on terminal)
            // Different terminals send: Ctrl+/, Ctrl+_, \x1f (ASCII 31), or Ctrl+7
            (Key::Char('/'), Modifiers { ctrl: true, .. })
            | (Key::Char('_'), Modifiers { ctrl: true, .. })
            | (Key::Char('\x1f'), _)  // ASCII 31 = Ctrl+/
            | (Key::Char('7'), Modifiers { ctrl: true, .. }) => {
                self.workspace.fuss.toggle_hints();
            }

            // Also allow 'h' for hints toggle as fallback
            (Key::Char('h'), Modifiers { ctrl: false, alt: false, .. }) => {
                self.workspace.fuss.toggle_hints();
            }

            // Open file in vertical split (v)
            (Key::Char('v'), Modifiers { ctrl: false, alt: false, .. }) => {
                if !self.workspace.fuss.is_dir_selected() {
                    if let Some(path) = self.workspace.fuss.selected_file() {
                        self.open_file_in_vsplit(&path)?;
                        self.workspace.fuss.deactivate();
                    }
                }
            }

            // Open file in horizontal split (s)
            (Key::Char('s'), Modifiers { ctrl: false, alt: false, .. }) => {
                if !self.workspace.fuss.is_dir_selected() {
                    if let Some(path) = self.workspace.fuss.selected_file() {
                        self.open_file_in_hsplit(&path)?;
                        self.workspace.fuss.deactivate();
                    }
                }
            }

            // Git: Stage file (a)
            (Key::Char('a'), Modifiers { ctrl: false, alt: false, .. }) => {
                if self.workspace.fuss.stage_selected() {
                    self.message = Some("Staged".to_string());
                } else {
                    self.message = Some("Failed to stage".to_string());
                }
            }

            // Git: Unstage file (u)
            (Key::Char('u'), Modifiers { ctrl: false, alt: false, .. }) => {
                if self.workspace.fuss.unstage_selected() {
                    self.message = Some("Unstaged".to_string());
                } else {
                    self.message = Some("Failed to unstage".to_string());
                }
            }

            // Git: Show diff (d)
            (Key::Char('d'), Modifiers { ctrl: false, alt: false, .. }) => {
                if let Some((filename, diff)) = self.workspace.fuss.get_diff_for_selected() {
                    let display_name = format!("[diff] {}", filename);
                    self.workspace.open_content_tab(&diff, &display_name);
                    self.workspace.fuss.deactivate();
                } else {
                    self.message = Some("No diff available".to_string());
                }
            }

            // Git: Commit (m) - opens prompt for commit message
            (Key::Char('m'), Modifiers { ctrl: false, alt: false, .. }) => {
                self.prompt = PromptState::TextInput {
                    label: "Commit message: ".to_string(),
                    buffer: String::new(),
                    action: TextInputAction::GitCommit,
                };
                self.message = Some("Enter commit message (Enter to commit, Esc to cancel)".to_string());
            }

            // Git: Push (p)
            (Key::Char('p'), Modifiers { ctrl: false, alt: false, .. }) => {
                let (_, msg) = self.workspace.fuss.git_push();
                self.message = Some(msg);
            }

            // Git: Pull (l)
            (Key::Char('l'), Modifiers { ctrl: false, alt: false, .. }) => {
                let (_, msg) = self.workspace.fuss.git_pull();
                self.message = Some(msg);
            }

            // Git: Fetch (f)
            (Key::Char('f'), Modifiers { ctrl: false, alt: false, .. }) => {
                let (_, msg) = self.workspace.fuss.git_fetch();
                self.message = Some(msg);
            }

            // Git: Tag (t) - opens prompt for tag name
            (Key::Char('t'), Modifiers { ctrl: false, alt: false, .. }) => {
                self.prompt = PromptState::TextInput {
                    label: "Tag name: ".to_string(),
                    buffer: String::new(),
                    action: TextInputAction::GitTag,
                };
                self.message = Some("Enter tag name (Enter to create, Esc to cancel)".to_string());
            }

            _ => {}
        }
        Ok(())
    }

    fn open_file(&mut self, path: &Path) -> Result<()> {
        self.workspace.open_file(path)
    }

    fn open_file_in_vsplit(&mut self, path: &Path) -> Result<()> {
        self.workspace.open_file_in_vsplit(path)?;
        self.message = Some("Opened in vertical split".to_string());
        Ok(())
    }

    fn open_file_in_hsplit(&mut self, path: &Path) -> Result<()> {
        self.workspace.open_file_in_hsplit(path)?;
        self.message = Some("Opened in horizontal split".to_string());
        Ok(())
    }

    // === Quit and prompt handling ===

    fn try_quit(&mut self) {
        if self.workspace.has_unsaved_changes() {
            // Show quit confirmation prompt
            self.prompt = PromptState::QuitConfirm;
            self.message = Some("Unsaved changes. [S]ave all / [D]iscard / [C]ancel".to_string());
        } else {
            // No unsaved changes, quit immediately
            self.running = false;
        }
    }

    fn handle_prompt_key(&mut self, key: Key) -> Result<()> {
        match self.prompt {
            PromptState::QuitConfirm => {
                match key {
                    Key::Char('s') | Key::Char('S') => {
                        // Save all and quit
                        if let Err(e) = self.workspace.save_all() {
                            self.message = Some(format!("Save failed: {}", e));
                        } else {
                            self.running = false;
                        }
                        self.prompt = PromptState::None;
                    }
                    Key::Char('d') | Key::Char('D') => {
                        // Discard changes and quit - delete backups
                        let _ = self.workspace.delete_all_backups();
                        self.running = false;
                        self.prompt = PromptState::None;
                    }
                    Key::Char('c') | Key::Char('C') | Key::Escape => {
                        // Cancel - return to editing
                        self.prompt = PromptState::None;
                        self.message = None;
                    }
                    _ => {
                        // Repeat the prompt
                        self.message = Some("Unsaved changes. [S]ave all / [D]iscard / [C]ancel".to_string());
                    }
                }
            }
            PromptState::RestoreBackup => {
                match key {
                    Key::Char('r') | Key::Char('R') => {
                        // Restore backups
                        if let Err(e) = self.restore_backups() {
                            self.message = Some(format!("Restore failed: {}", e));
                        } else {
                            self.message = Some("Restored unsaved changes".to_string());
                        }
                        self.prompt = PromptState::None;
                    }
                    Key::Char('d') | Key::Char('D') | Key::Escape => {
                        // Discard backups (Escape = discard)
                        let _ = self.workspace.delete_all_backups();
                        self.message = Some("Discarded recovered changes".to_string());
                        self.prompt = PromptState::None;
                    }
                    _ => {
                        // Repeat the prompt
                        self.message = Some("Recovered unsaved changes. [R]estore / [D]iscard / [Esc]".to_string());
                    }
                }
            }
            PromptState::TextInput { ref label, ref mut buffer, ref action } => {
                match key {
                    Key::Enter => {
                        // Execute the action
                        let action = action.clone();
                        let buffer = buffer.clone();
                        self.prompt = PromptState::None;
                        self.execute_text_input_action(action, &buffer);
                    }
                    Key::Escape => {
                        // Cancel
                        self.prompt = PromptState::None;
                        self.message = Some("Cancelled".to_string());
                    }
                    Key::Backspace => {
                        // Delete last character
                        buffer.pop();
                        self.message = Some(format!("{}{}", label, buffer));
                    }
                    Key::Char(c) => {
                        // Add character to buffer
                        buffer.push(c);
                        self.message = Some(format!("{}{}", label, buffer));
                    }
                    _ => {
                        // Update display
                        self.message = Some(format!("{}{}", label, buffer));
                    }
                }
            }
            PromptState::RenameModal { ref original_name, ref mut new_name, ref path, line, col } => {
                match key {
                    Key::Enter => {
                        // Clone values before modifying self.prompt
                        let original = original_name.clone();
                        let new = new_name.clone();
                        let path = path.clone();

                        // Execute rename
                        if new.is_empty() {
                            self.prompt = PromptState::None;
                            self.message = Some("Rename cancelled: empty name".to_string());
                        } else if new == original {
                            self.prompt = PromptState::None;
                            self.message = Some("Rename cancelled: name unchanged".to_string());
                        } else {
                            self.prompt = PromptState::None;
                            match self.workspace.lsp.request_rename(&path, line, col, &new) {
                                Ok(_id) => {
                                    self.message = Some(format!("Renaming '{}' to '{}'...", original, new));
                                }
                                Err(e) => {
                                    self.message = Some(format!("Rename failed: {}", e));
                                }
                            }
                        }
                    }
                    Key::Escape => {
                        self.prompt = PromptState::None;
                        self.message = Some("Rename cancelled".to_string());
                    }
                    Key::Backspace => {
                        new_name.pop();
                    }
                    Key::Char(c) => {
                        new_name.push(c);
                    }
                    _ => {}
                }
            }
            PromptState::ReferencesPanel { ref locations, ref mut selected_index, ref mut query } => {
                // Filter locations based on query
                let filtered: Vec<(usize, &Location)> = if query.is_empty() {
                    locations.iter().enumerate().collect()
                } else {
                    let q = query.to_lowercase();
                    locations.iter().enumerate()
                        .filter(|(_, loc)| {
                            loc.uri.to_lowercase().contains(&q)
                        })
                        .collect()
                };

                match key {
                    Key::Enter => {
                        // Jump to selected reference
                        if let Some((orig_idx, _)) = filtered.get(*selected_index) {
                            let loc = locations[*orig_idx].clone();
                            self.prompt = PromptState::None;
                            self.goto_location(&loc);
                        }
                    }
                    Key::Escape => {
                        self.prompt = PromptState::None;
                        self.message = None;
                    }
                    Key::Up => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                        }
                    }
                    Key::Down => {
                        if *selected_index + 1 < filtered.len() {
                            *selected_index += 1;
                        }
                    }
                    Key::PageUp => {
                        *selected_index = selected_index.saturating_sub(10);
                    }
                    Key::PageDown => {
                        *selected_index = (*selected_index + 10).min(filtered.len().saturating_sub(1));
                    }
                    Key::Home => {
                        *selected_index = 0;
                    }
                    Key::End => {
                        if !filtered.is_empty() {
                            *selected_index = filtered.len() - 1;
                        }
                    }
                    Key::Backspace => {
                        query.pop();
                        // Reset selection when filter changes
                        *selected_index = 0;
                    }
                    Key::Char(c) => {
                        query.push(c);
                        // Reset selection when filter changes
                        *selected_index = 0;
                    }
                    _ => {}
                }
            }
            PromptState::None => {}
        }
        Ok(())
    }

    fn execute_text_input_action(&mut self, action: TextInputAction, buffer: &str) {
        match action {
            TextInputAction::GitCommit => {
                let (_, msg) = self.workspace.fuss.git_commit(buffer);
                self.message = Some(msg);
            }
            TextInputAction::GitTag => {
                let (_, msg) = self.workspace.fuss.git_tag(buffer);
                self.message = Some(msg);
            }
        }
    }

    fn restore_backups(&mut self) -> Result<()> {
        let backups = self.workspace.list_backups();

        for (original_path, backup_path) in backups {
            let (_, content) = self.workspace.read_backup(&backup_path)?;

            // Try to find an open buffer with this path
            let mut found = false;
            for tab in &mut self.workspace.tabs {
                for buffer_entry in &mut tab.buffers {
                    if let Some(ref buf_path) = buffer_entry.path {
                        let full_path = if buffer_entry.is_orphan {
                            buf_path.clone()
                        } else {
                            self.workspace.root.join(buf_path)
                        };
                        if full_path == original_path {
                            buffer_entry.buffer.set_contents(&content);
                            found = true;
                            break;
                        }
                    }
                }
                if found {
                    break;
                }
            }

            // If not found as open buffer, open the file first then restore
            if !found {
                // Open the file
                self.workspace.open_file(&original_path)?;
                // Now restore content to the newly opened buffer
                if let Some(tab) = self.workspace.tabs.last_mut() {
                    if let Some(buffer_entry) = tab.buffers.last_mut() {
                        buffer_entry.buffer.set_contents(&content);
                    }
                }
            }

            // Delete the backup after successful restore
            std::fs::remove_file(&backup_path)?;
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
