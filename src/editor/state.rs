use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{self, Event, KeyEvent, MouseEvent};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::input::{Key, Modifiers, Mouse, Button};
use crate::lsp::{CompletionItem, Diagnostic, HoverInfo, Location, ServerManagerPanel};
use crate::render::{PaneBounds as RenderPaneBounds, PaneInfo, Screen, TabInfo};
use crate::terminal::TerminalPanel;
use crate::workspace::{PaneDirection, Tab, Workspace};

use super::{Cursor, Cursors, History, Operation, Position};

/// How long to wait after last edit before writing idle backup (seconds)
const BACKUP_IDLE_SECS: u64 = 30;

/// Which input field is active in find/replace
#[derive(Debug, Clone, Copy, PartialEq)]
enum FindReplaceField {
    Find,
    Replace,
}

/// Entry in the fortress file explorer
#[derive(Debug, Clone, PartialEq)]
struct FortressEntry {
    /// File/directory name
    name: String,
    /// Full path
    path: PathBuf,
    /// Is this a directory?
    is_dir: bool,
}

/// A command in the command palette
#[derive(Debug, Clone, PartialEq)]
struct PaletteCommand {
    /// Display name (e.g., "Save File")
    name: &'static str,
    /// Keyboard shortcut (e.g., "Ctrl+S")
    shortcut: &'static str,
    /// Category for grouping (e.g., "File", "Edit")
    category: &'static str,
    /// Unique command identifier
    id: &'static str,
    /// Fuzzy match score (computed during filtering)
    score: i32,
}

impl PaletteCommand {
    const fn new(name: &'static str, shortcut: &'static str, category: &'static str, id: &'static str) -> Self {
        Self { name, shortcut, category, id, score: 0 }
    }
}

/// All available commands for the command palette
const ALL_COMMANDS: &[PaletteCommand] = &[
    // File operations
    PaletteCommand::new("Save File", "Ctrl+S", "File", "save"),
    PaletteCommand::new("Save All", "", "File", "save-all"),
    PaletteCommand::new("Open File Browser", "Ctrl+O", "File", "open"),
    PaletteCommand::new("New Tab", "Alt+T", "File", "new-tab"),
    PaletteCommand::new("Close Tab", "Alt+Q", "File", "close-tab"),
    PaletteCommand::new("Next Tab", "Alt+.", "File", "next-tab"),
    PaletteCommand::new("Previous Tab", "Alt+,", "File", "prev-tab"),
    PaletteCommand::new("Quit", "Ctrl+Q", "File", "quit"),

    // Edit operations
    PaletteCommand::new("Undo", "Ctrl+Z", "Edit", "undo"),
    PaletteCommand::new("Redo", "Ctrl+]", "Edit", "redo"),
    PaletteCommand::new("Cut", "Ctrl+X", "Edit", "cut"),
    PaletteCommand::new("Copy", "Ctrl+C", "Edit", "copy"),
    PaletteCommand::new("Paste", "Ctrl+V", "Edit", "paste"),
    PaletteCommand::new("Select All", "Ctrl+A", "Edit", "select-all"),
    PaletteCommand::new("Select Line", "Ctrl+L", "Edit", "select-line"),
    PaletteCommand::new("Select Word", "Ctrl+D", "Edit", "select-word"),
    PaletteCommand::new("Toggle Line Comment", "Ctrl+/", "Edit", "toggle-comment"),
    PaletteCommand::new("Join Lines", "Ctrl+J", "Edit", "join-lines"),
    PaletteCommand::new("Duplicate Line", "Alt+Shift+Down", "Edit", "duplicate-line"),
    PaletteCommand::new("Move Line Up", "Alt+Up", "Edit", "move-line-up"),
    PaletteCommand::new("Move Line Down", "Alt+Down", "Edit", "move-line-down"),
    PaletteCommand::new("Delete Line", "", "Edit", "delete-line"),
    PaletteCommand::new("Indent", "Tab", "Edit", "indent"),
    PaletteCommand::new("Outdent", "Shift+Tab", "Edit", "outdent"),
    PaletteCommand::new("Transpose Characters", "Ctrl+T", "Edit", "transpose"),

    // Search operations
    PaletteCommand::new("Find", "Ctrl+F", "Search", "find"),
    PaletteCommand::new("Find and Replace", "Ctrl+R", "Search", "replace"),
    PaletteCommand::new("Find Next", "F3", "Search", "find-next"),
    PaletteCommand::new("Find Previous", "Shift+F3", "Search", "find-prev"),
    PaletteCommand::new("Search in Files", "F4", "Search", "search-files"),

    // Navigation
    PaletteCommand::new("Go to Line", "Ctrl+G", "Navigation", "goto-line"),
    PaletteCommand::new("Go to Beginning of File", "Ctrl+Home", "Navigation", "goto-start"),
    PaletteCommand::new("Go to End of File", "Ctrl+End", "Navigation", "goto-end"),
    PaletteCommand::new("Go to Matching Bracket", "Ctrl+M", "Navigation", "goto-bracket"),
    PaletteCommand::new("Page Up", "PageUp", "Navigation", "page-up"),
    PaletteCommand::new("Page Down", "PageDown", "Navigation", "page-down"),

    // Selection
    PaletteCommand::new("Expand Selection to Brackets", "", "Selection", "select-brackets"),
    PaletteCommand::new("Add Cursor Above", "Ctrl+Alt+Up", "Selection", "cursor-above"),
    PaletteCommand::new("Add Cursor Below", "Ctrl+Alt+Down", "Selection", "cursor-below"),

    // View / Panes
    PaletteCommand::new("Split Pane Vertical", "Alt+V", "View", "split-vertical"),
    PaletteCommand::new("Split Pane Horizontal", "Alt+S", "View", "split-horizontal"),
    PaletteCommand::new("Close Pane", "Alt+Q", "View", "close-pane"),
    PaletteCommand::new("Focus Next Pane", "Alt+N", "View", "next-pane"),
    PaletteCommand::new("Focus Previous Pane", "Alt+P", "View", "prev-pane"),
    PaletteCommand::new("Toggle File Explorer", "Ctrl+B", "View", "toggle-explorer"),

    // LSP / Code Intelligence
    PaletteCommand::new("Go to Definition", "F12", "LSP", "goto-definition"),
    PaletteCommand::new("Find References", "Shift+F12", "LSP", "find-references"),
    PaletteCommand::new("Rename Symbol", "F2", "LSP", "rename"),
    PaletteCommand::new("Show Hover Info", "Ctrl+K Ctrl+I", "LSP", "hover"),
    PaletteCommand::new("Trigger Completion", "Ctrl+Space", "LSP", "completion"),
    PaletteCommand::new("LSP Server Manager", "Alt+M", "LSP", "server-manager"),

    // Bracket/Quote operations
    PaletteCommand::new("Jump to Bracket", "Alt+]", "Brackets", "jump-bracket"),
    PaletteCommand::new("Cycle Bracket Type", "Alt+[", "Brackets", "cycle-brackets"),
    PaletteCommand::new("Remove Surrounding", "Alt+Backspace", "Brackets", "remove-surrounding"),

    // Help
    PaletteCommand::new("Command Palette", "Ctrl+P", "Help", "command-palette"),
    PaletteCommand::new("Help / Keybindings", "Shift+F1", "Help", "help"),
];

/// A keybinding entry for the help menu
#[derive(Debug, Clone, PartialEq)]
struct HelpKeybind {
    /// Keyboard shortcut (e.g., "Ctrl+S")
    shortcut: &'static str,
    /// Alternative shortcut (shown when "/" is held)
    alt_shortcut: &'static str,
    /// Description of what the keybind does
    description: &'static str,
    /// Category for grouping
    category: &'static str,
}

impl HelpKeybind {
    const fn new(shortcut: &'static str, description: &'static str, category: &'static str) -> Self {
        Self { shortcut, alt_shortcut: "", description, category }
    }

    const fn with_alt(shortcut: &'static str, alt_shortcut: &'static str, description: &'static str, category: &'static str) -> Self {
        Self { shortcut, alt_shortcut, description, category }
    }
}

/// All keybindings for the help menu - comprehensive list
const ALL_KEYBINDS: &[HelpKeybind] = &[
    // File Operations
    HelpKeybind::new("Ctrl+S", "Save file", "File"),
    HelpKeybind::new("Ctrl+O", "Open file browser (Fortress)", "File"),
    HelpKeybind::new("Ctrl+Q", "Quit editor", "File"),
    HelpKeybind::with_alt("Ctrl+B", "F3", "Toggle file explorer", "File"),

    // Tabs
    HelpKeybind::new("Alt+T", "New tab", "Tabs"),
    HelpKeybind::new("Alt+Q", "Close tab/pane", "Tabs"),
    HelpKeybind::new("Alt+.", "Next tab", "Tabs"),
    HelpKeybind::new("Alt+,", "Previous tab", "Tabs"),
    HelpKeybind::new("Alt+1-9", "Switch to tab 1-9", "Tabs"),

    // Panes
    HelpKeybind::new("Alt+V", "Split vertical", "Panes"),
    HelpKeybind::new("Alt+S", "Split horizontal", "Panes"),
    HelpKeybind::new("Alt+H/J/K/L", "Navigate panes (vim-style)", "Panes"),
    HelpKeybind::new("Alt+N", "Next pane", "Panes"),
    HelpKeybind::new("Alt+P", "Previous pane", "Panes"),

    // Editing
    HelpKeybind::new("Ctrl+Z", "Undo", "Edit"),
    HelpKeybind::with_alt("Ctrl+]", "Ctrl+Shift+Z", "Redo", "Edit"),
    HelpKeybind::new("Ctrl+C", "Copy", "Edit"),
    HelpKeybind::new("Ctrl+X", "Cut", "Edit"),
    HelpKeybind::new("Ctrl+V", "Paste", "Edit"),
    HelpKeybind::new("Ctrl+J", "Join lines", "Edit"),
    HelpKeybind::new("Ctrl+/", "Toggle line comment", "Edit"),
    HelpKeybind::new("Ctrl+T", "Transpose characters", "Edit"),
    HelpKeybind::new("Tab", "Indent", "Edit"),
    HelpKeybind::new("Shift+Tab", "Outdent", "Edit"),
    HelpKeybind::new("Backspace", "Delete backward", "Edit"),
    HelpKeybind::new("Delete", "Delete forward", "Edit"),
    HelpKeybind::new("Ctrl+W", "Delete word backward", "Edit"),
    HelpKeybind::new("Alt+D", "Delete word forward", "Edit"),
    HelpKeybind::new("Alt+Backspace", "Delete word backward", "Edit"),
    HelpKeybind::new("Ctrl+K", "Kill to end of line", "Edit"),
    HelpKeybind::new("Ctrl+U", "Kill to start of line", "Edit"),
    HelpKeybind::new("Ctrl+Y", "Yank (paste from kill ring)", "Edit"),
    HelpKeybind::new("Alt+Y", "Cycle yank stack", "Edit"),

    // Line Operations
    HelpKeybind::new("Alt+Up", "Move line up", "Lines"),
    HelpKeybind::new("Alt+Down", "Move line down", "Lines"),
    HelpKeybind::new("Alt+Shift+Up", "Duplicate line up", "Lines"),
    HelpKeybind::new("Alt+Shift+Down", "Duplicate line down", "Lines"),

    // Movement
    HelpKeybind::new("Arrow keys", "Move cursor", "Movement"),
    HelpKeybind::with_alt("Home", "Ctrl+A", "Go to line start (smart)", "Movement"),
    HelpKeybind::with_alt("End", "Ctrl+E", "Go to line end", "Movement"),
    HelpKeybind::with_alt("Alt+Left", "Alt+B", "Move word left", "Movement"),
    HelpKeybind::with_alt("Alt+Right", "Alt+F", "Move word right", "Movement"),
    HelpKeybind::new("PageUp", "Page up", "Movement"),
    HelpKeybind::new("PageDown", "Page down", "Movement"),
    HelpKeybind::with_alt("Ctrl+G", "F5", "Go to line", "Movement"),

    // Selection
    HelpKeybind::new("Shift+Arrow", "Extend selection", "Selection"),
    HelpKeybind::new("Ctrl+L", "Select line", "Selection"),
    HelpKeybind::new("Ctrl+D", "Select word / next occurrence", "Selection"),
    HelpKeybind::new("Escape", "Clear selection / collapse cursors", "Selection"),
    HelpKeybind::new("Ctrl+Alt+Up", "Add cursor above", "Selection"),
    HelpKeybind::new("Ctrl+Alt+Down", "Add cursor below", "Selection"),

    // Search
    HelpKeybind::new("Ctrl+F", "Find", "Search"),
    HelpKeybind::new("Ctrl+R", "Find and replace", "Search"),
    HelpKeybind::new("F3", "Find next", "Search"),
    HelpKeybind::new("Shift+F3", "Find previous", "Search"),
    HelpKeybind::new("F4", "Search in files", "Search"),
    HelpKeybind::new("Alt+I", "Toggle case sensitivity (in find)", "Search"),
    HelpKeybind::new("Alt+X", "Toggle regex mode (in find)", "Search"),
    HelpKeybind::new("Alt+Enter", "Replace all (in find)", "Search"),

    // Brackets & Quotes
    HelpKeybind::with_alt("Alt+[", "Alt+]", "Jump to matching bracket", "Brackets"),
    HelpKeybind::new("Alt+'", "Cycle quote type (\"/'/`)", "Brackets"),
    HelpKeybind::new("Alt+\"", "Remove surrounding quotes", "Brackets"),
    HelpKeybind::new("Alt+(", "Cycle bracket type (/{/[)", "Brackets"),
    HelpKeybind::new("Alt+)", "Remove surrounding brackets", "Brackets"),

    // LSP / Code Intelligence
    HelpKeybind::new("F1", "Show hover info", "LSP"),
    HelpKeybind::new("F2", "Rename symbol", "LSP"),
    HelpKeybind::new("F12", "Go to definition", "LSP"),
    HelpKeybind::new("Shift+F12", "Find references", "LSP"),
    HelpKeybind::new("Ctrl+N", "Trigger completion", "LSP"),
    HelpKeybind::new("Alt+M", "LSP server manager", "LSP"),

    // Help & Commands
    HelpKeybind::new("Ctrl+P", "Command palette", "Help"),
    HelpKeybind::new("Shift+F1", "Help / keybindings", "Help"),

    // File Explorer (Fortress/Fuss mode)
    HelpKeybind::new("Up/Down", "Navigate files", "Explorer"),
    HelpKeybind::new("Enter", "Open file/directory", "Explorer"),
    HelpKeybind::new("Right", "Expand directory", "Explorer"),
    HelpKeybind::new("Left", "Collapse / go to parent", "Explorer"),
    HelpKeybind::new("Space", "Toggle selection", "Explorer"),
    HelpKeybind::new("a", "Add file", "Explorer"),
    HelpKeybind::new("d", "Delete selected", "Explorer"),
    HelpKeybind::new("m", "Move/rename selected", "Explorer"),
    HelpKeybind::new("p", "Paste", "Explorer"),
    HelpKeybind::new("u", "Undo last action", "Explorer"),
    HelpKeybind::new("f", "Create folder", "Explorer"),
    HelpKeybind::new("t", "Open in new tab", "Explorer"),
    HelpKeybind::new("l", "Open in vertical split", "Explorer"),
    HelpKeybind::new("Alt+G", "Git status", "Explorer"),
    HelpKeybind::new("Alt+.", "Toggle hidden files", "Explorer"),
];

/// Prompt state for quit confirmation
#[derive(Debug, Clone, PartialEq)]
enum PromptState {
    /// No prompt active
    None,
    /// Quit prompt: Save/Discard/Cancel
    QuitConfirm,
    /// Close buffer prompt: Save/Discard/Cancel
    CloseBufferConfirm,
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
    /// Find/Replace dialog in status bar
    FindReplace {
        /// Search query
        find_query: String,
        /// Replacement text
        replace_text: String,
        /// Which field is active
        active_field: FindReplaceField,
        /// Case insensitive search
        case_insensitive: bool,
        /// Regex mode
        regex_mode: bool,
    },
    /// Fortress mode - file explorer modal
    Fortress {
        /// Current directory being browsed
        current_path: PathBuf,
        /// Directory entries (directories first, then files)
        entries: Vec<FortressEntry>,
        /// Currently selected index
        selected_index: usize,
        /// Filter/search query
        filter: String,
        /// Scroll offset for long lists
        scroll_offset: usize,
    },
    /// Multi-file search modal (F4)
    FileSearch {
        /// Search query
        query: String,
        /// Search results: (file_path, line_number, line_content)
        results: Vec<FileSearchResult>,
        /// Currently selected index
        selected_index: usize,
        /// Scroll offset for long lists
        scroll_offset: usize,
        /// Whether search is in progress
        searching: bool,
    },
    /// Command palette (Ctrl+P)
    CommandPalette {
        /// Search/filter query (with > prefix)
        query: String,
        /// Filtered commands matching query
        filtered: Vec<PaletteCommand>,
        /// Currently selected index
        selected_index: usize,
        /// Scroll offset for long lists
        scroll_offset: usize,
    },
    /// Help menu (Shift+F1)
    HelpMenu {
        /// Search/filter query
        query: String,
        /// Filtered keybinds matching query
        filtered: Vec<HelpKeybind>,
        /// Currently selected index
        selected_index: usize,
        /// Scroll offset for long lists
        scroll_offset: usize,
        /// Show alternative keybindings (toggled with "/")
        show_alt: bool,
    },
}

/// Which UI component currently has keyboard focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Main editor panes
    Editor,
    /// Integrated terminal panel
    Terminal,
    /// Fuss mode file tree sidebar
    FussMode,
    /// LSP server manager panel
    ServerManager,
    /// Active prompt/modal (prompts are exclusive by nature)
    Prompt,
}

/// Hit test result for determining which region was clicked
#[derive(Debug, Clone, Copy)]
enum HitRegion {
    /// Editor pane (with index for split panes)
    Editor { pane_index: usize },
    /// Terminal panel
    Terminal,
    /// Fuss mode sidebar
    FussMode,
    /// Server manager panel
    ServerManager,
    /// Prompt/modal area
    Prompt,
    /// Outside any interactive region
    None,
}

/// A single result from multi-file search
#[derive(Debug, Clone, PartialEq)]
struct FileSearchResult {
    /// Relative path to file
    path: PathBuf,
    /// Line number (1-indexed for display)
    line_num: usize,
    /// The matching line content (trimmed)
    line_content: String,
}

/// Action to perform when text input is complete
#[derive(Debug, Clone, PartialEq)]
enum TextInputAction {
    /// Commit with the entered message
    GitCommit,
    /// Create a git tag
    GitTag,
    /// Go to line (and optionally column)
    GotoLine,
}

/// LSP UI state
#[derive(Debug, Default)]
struct LspState {
    /// Current hover information to display
    hover: Option<HoverInfo>,
    /// Whether hover popup is visible
    hover_visible: bool,
    /// Original unfiltered completion list from LSP
    completions_original: Vec<CompletionItem>,
    /// Current filtered completion list
    completions: Vec<CompletionItem>,
    /// Selected completion index
    completion_index: usize,
    /// Whether completion popup is visible
    completion_visible: bool,
    /// Filter text typed while completion is open
    completion_filter: String,
    /// Cursor column when completion was opened (to track popup position)
    completion_start_col: usize,
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

/// A search match position
#[derive(Debug, Clone, PartialEq)]
struct SearchMatch {
    line: usize,
    start_col: usize,
    end_col: usize,
}

/// Search state for find/replace
#[derive(Debug, Default)]
struct SearchState {
    /// All matches in the current buffer
    matches: Vec<SearchMatch>,
    /// Current match index (which one is "active")
    current_match: usize,
    /// Last search query (to detect changes)
    last_query: String,
    /// Last search settings
    last_case_insensitive: bool,
    last_regex: bool,
}

/// Cached bracket match result
#[derive(Debug, Default)]
struct BracketMatchCache {
    /// Cursor position when cache was computed (line, col)
    cursor_pos: Option<(usize, usize)>,
    /// The cached match result
    result: Option<(usize, usize)>,
    /// Whether the cache is valid (invalidated on buffer changes)
    valid: bool,
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
    /// Time of last edit (for idle backup timing), None if no pending backup
    last_edit_time: Option<Instant>,
    /// LSP-related UI state
    lsp_state: LspState,
    /// LSP server manager panel
    server_manager: ServerManagerPanel,
    /// Search state for find/replace
    search_state: SearchState,
    /// Cached bracket match for rendering
    bracket_cache: BracketMatchCache,
    /// Yank stack (kill ring) - separate from system clipboard
    yank_stack: Vec<String>,
    /// Current index in yank stack when cycling with Alt+Y
    yank_index: Option<usize>,
    /// Length of last yank (for replacing when cycling)
    last_yank_len: usize,
    /// Integrated terminal panel
    terminal: TerminalPanel,
    /// Terminal resize: dragging in progress
    terminal_resize_dragging: bool,
    /// Terminal resize: starting Y position of drag
    terminal_resize_start_y: u16,
    /// Terminal resize: starting height when drag began
    terminal_resize_start_height: u16,
    /// Current keyboard focus target
    focus: Focus,
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

        // Create terminal panel with screen dimensions
        let terminal = TerminalPanel::new(screen.cols, screen.rows);

        let mut editor = Self {
            workspace,
            screen,
            running: true,
            clipboard,
            internal_clipboard: String::new(),
            message: None,
            escape_time,
            prompt: PromptState::None,
            last_edit_time: None, // No pending backup initially
            lsp_state: LspState::default(),
            server_manager: ServerManagerPanel::new(),
            search_state: SearchState::default(),
            bracket_cache: BracketMatchCache::default(),
            yank_stack: Vec::with_capacity(32),
            yank_index: None,
            last_yank_len: 0,
            terminal,
            terminal_resize_dragging: false,
            terminal_resize_start_y: 0,
            terminal_resize_start_height: 0,
            focus: Focus::Editor,
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

    /// Invalidate syntax highlight cache from a given line onward.
    /// Call this when buffer content changes at or after the specified line.
    #[inline]
    fn invalidate_highlight_cache(&mut self, from_line: usize) {
        let tab = self.workspace.active_tab_mut();
        let pane_idx = tab.active_pane;
        let buffer_idx = tab.panes[pane_idx].buffer_idx;
        tab.buffers[buffer_idx].highlighter.invalidate_cache(from_line);
    }

    /// Invalidate the bracket match cache (call on buffer changes)
    #[inline]
    fn invalidate_bracket_cache(&mut self) {
        self.bracket_cache.valid = false;
    }

    /// Get cached bracket match for current cursor position.
    /// Computes and caches the result if needed.
    fn get_bracket_match(&mut self) -> Option<(usize, usize)> {
        let cursor = self.cursor();
        let cursor_pos = (cursor.line, cursor.col);

        // Check if cache is valid and for the same position
        if self.bracket_cache.valid {
            if let Some(cached_pos) = self.bracket_cache.cursor_pos {
                if cached_pos == cursor_pos {
                    return self.bracket_cache.result;
                }
            }
        }

        // Cache miss - compute the bracket match
        let result = self.buffer().find_matching_bracket(cursor_pos.0, cursor_pos.1);

        // Update cache
        self.bracket_cache.cursor_pos = Some(cursor_pos);
        self.bracket_cache.result = result;
        self.bracket_cache.valid = true;

        result
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

    /// Get current buffer entry (immutable)
    #[inline]
    fn buffer_entry(&self) -> &crate::workspace::BufferEntry {
        let tab = self.workspace.active_tab();
        let pane_idx = tab.active_pane;
        let buffer_idx = tab.panes[pane_idx].buffer_idx;
        &tab.buffers[buffer_idx]
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
                        self.terminal.update_screen_size(cols, rows);
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
                            self.terminal.update_screen_size(cols, rows);
                        }
                        _ => {}
                    }
                }
            }

            // Poll terminal for output (only render if data received)
            if self.terminal.visible && self.terminal.poll() {
                needs_render = true;
            }

            // Process LSP messages from language servers
            if self.process_lsp_messages() {
                needs_render = true;
            }

            // Poll for completed server installations
            if self.server_manager.poll_installs() {
                needs_render = true;
            }

            // Check if it's time for idle backup
            self.maybe_idle_backup();

            // Only render if something changed
            if needs_render {
                self.screen.refresh_size()?;
                self.render()?;
            }
        }

        self.screen.leave_raw_mode()?;
        Ok(())
    }

    /// Write idle backups if enough time has passed since last edit
    fn maybe_idle_backup(&mut self) {
        if let Some(last_edit) = self.last_edit_time {
            if last_edit.elapsed() >= Duration::from_secs(BACKUP_IDLE_SECS) {
                if self.workspace.has_unsaved_changes() {
                    let _ = self.workspace.backup_all_modified();
                    // Mark all modified buffers as backed up
                    for tab in &mut self.workspace.tabs {
                        for buffer_entry in &mut tab.buffers {
                            if buffer_entry.is_modified() {
                                buffer_entry.backed_up = true;
                            }
                        }
                    }
                }
                self.last_edit_time = None; // Reset until next edit
            }
        }
    }

    /// Called after key handling - triggers backup if buffer was modified
    fn on_buffer_edit(&mut self) {
        // Check buffer state
        let (is_modified, needs_first_backup) = {
            let buffer_entry = self.buffer_entry_mut();
            (buffer_entry.is_modified(), !buffer_entry.backed_up && buffer_entry.is_modified())
        };

        // Update edit time if buffer has unsaved changes (for idle backup)
        if is_modified {
            self.last_edit_time = Some(Instant::now());
        }

        // First edit since save/load - backup immediately
        if needs_first_backup {
            let backup_info: Option<(PathBuf, String)> = {
                let buffer_entry = self.buffer_entry();
                buffer_entry.path.as_ref().map(|path| {
                    let full_path = if buffer_entry.is_orphan {
                        path.clone()
                    } else {
                        self.workspace.root.join(path)
                    };
                    let content = buffer_entry.buffer.contents();
                    (full_path, content)
                })
            };

            if let Some((full_path, content)) = backup_info {
                let _ = self.workspace.write_backup(&full_path, &content);
                self.buffer_entry_mut().backed_up = true;
            }
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
                        self.lsp_state.completions_original = items.clone();
                        self.lsp_state.completions = items;
                        self.lsp_state.completion_index = 0;
                        self.lsp_state.completion_visible = !self.lsp_state.completions.is_empty();
                        self.lsp_state.completion_filter.clear();
                        self.lsp_state.completion_start_col = self.cursor().col;
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

        // Update diagnostics for current file (need full path to match LSP URIs)
        if let Some(path) = self.current_file_path() {
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
        let current_hash = self.buffer_mut().content_hash();

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
            self.return_focus();
        } else {
            self.server_manager.show();
            self.focus = Focus::ServerManager;
        }
    }

    /// Handle key input when server manager panel is visible
    fn handle_server_manager_key(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        let max_visible = 10; // Should match screen.rs

        // Alt+M toggles the panel closed
        if key == Key::Char('m') && mods.alt {
            self.server_manager.hide();
            self.return_focus();
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
        self.dismiss_completion();
    }

    /// Dismiss the completion popup
    fn dismiss_completion(&mut self) {
        self.lsp_state.completion_visible = false;
        self.lsp_state.completions.clear();
        self.lsp_state.completions_original.clear();
        self.lsp_state.completion_index = 0;
        self.lsp_state.completion_filter.clear();
    }

    /// Filter completions based on typed text
    fn filter_completions(&mut self) {
        let filter = self.lsp_state.completion_filter.to_lowercase();
        if filter.is_empty() {
            self.lsp_state.completions = self.lsp_state.completions_original.clone();
        } else {
            self.lsp_state.completions = self.lsp_state.completions_original
                .iter()
                .filter(|item| item.label.to_lowercase().contains(&filter))
                .cloned()
                .collect();
        }
        // Reset selection to first item
        self.lsp_state.completion_index = 0;
    }

    /// Process a key event, handling ESC as potential Alt prefix
    fn process_key(&mut self, key_event: KeyEvent) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Ctrl+` or Ctrl+j toggles terminal (works in both editor and terminal mode)
        // Ctrl+j is alternate since Ctrl+` can conflict with tmux
        if (key_event.code == KeyCode::Char('`') || key_event.code == KeyCode::Char('j'))
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            let _ = self.terminal.toggle();
            self.terminal_resize_dragging = false;
            // Set focus when opening, return focus when closing
            if self.terminal.visible {
                self.focus = Focus::Terminal;
            } else {
                self.return_focus();
            }
            return Ok(());
        }

        // Focus-based routing for terminal
        if self.focus == Focus::Terminal && self.terminal.visible {
            // ESC hides terminal and returns focus
            if key_event.code == KeyCode::Esc {
                self.terminal.hide();
                self.terminal_resize_dragging = false;
                self.return_focus();
                return Ok(());
            }

            // Terminal tab management keybindings (Alt + key)
            if key_event.modifiers.contains(KeyModifiers::ALT) {
                match key_event.code {
                    // Alt+T: New terminal tab
                    KeyCode::Char('t') => {
                        let _ = self.terminal.new_session();
                        return Ok(());
                    }
                    // Alt+Q: Close current tab
                    KeyCode::Char('q') => {
                        if self.terminal.close_active_session() {
                            self.terminal.hide();
                            self.terminal_resize_dragging = false;
                            self.return_focus();
                        }
                        return Ok(());
                    }
                    // Alt+.: Next tab
                    KeyCode::Char('.') => {
                        self.terminal.next_session();
                        return Ok(());
                    }
                    // Alt+,: Previous tab
                    KeyCode::Char(',') => {
                        self.terminal.prev_session();
                        return Ok(());
                    }
                    // Alt+1-9: Switch to specific tab
                    KeyCode::Char(c @ '1'..='9') => {
                        let idx = (c as usize) - ('1' as usize);
                        self.terminal.switch_session(idx);
                        return Ok(());
                    }
                    _ => {}
                }
            }

            // F3 or Ctrl+B toggles fuss mode (works over terminal)
            if key_event.code == KeyCode::F(3)
                || (key_event.code == KeyCode::Char('b')
                    && key_event.modifiers.contains(KeyModifiers::CONTROL))
            {
                self.toggle_fuss_mode();
                return Ok(());
            }

            // Send all other keys to terminal
            let _ = self.terminal.send_key(&key_event);
            return Ok(());
        }

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

    /// Hit test to determine which UI region contains a screen coordinate
    fn hit_test(&self, col: u16, row: u16) -> HitRegion {
        // Check prompt/modal first (overlays everything)
        // Prompts take up various areas but for click purposes we consider them focused
        if self.prompt != PromptState::None {
            return HitRegion::Prompt;
        }

        // Check server manager panel (right side overlay)
        if self.server_manager.visible {
            let panel_width = 50.min(self.screen.cols / 2);
            let panel_start_col = self.screen.cols.saturating_sub(panel_width);
            if col >= panel_start_col {
                return HitRegion::ServerManager;
            }
        }

        // Check terminal (bottom region)
        if self.terminal.visible {
            let terminal_start_row = self.terminal.render_start_row(self.screen.rows);
            if row >= terminal_start_row {
                // Terminal shrinks when fuss mode is active
                let fuss_width = if self.workspace.fuss.active {
                    self.workspace.fuss.width(self.screen.cols)
                } else {
                    0
                };
                if col >= fuss_width {
                    return HitRegion::Terminal;
                }
            }
        }

        // Check fuss sidebar (left side)
        if self.workspace.fuss.active {
            let fuss_width = self.workspace.fuss.width(self.screen.cols);
            if col < fuss_width {
                return HitRegion::FussMode;
            }
        }

        // Otherwise it's the editor - determine which pane
        let pane_index = self.workspace.pane_at_position(col, row, self.screen.cols, self.screen.rows);
        HitRegion::Editor { pane_index }
    }

    /// Return focus to a sensible default after closing a component
    fn return_focus(&mut self) {
        // Return focus to the most recently visible component, defaulting to editor
        self.focus = Focus::Editor;
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

        // Click-to-focus: determine which region was clicked and set focus
        if let Mouse::Click { col, row, .. } = mouse {
            let region = self.hit_test(col, row);
            match region {
                HitRegion::Terminal => {
                    self.focus = Focus::Terminal;
                }
                HitRegion::FussMode => {
                    self.focus = Focus::FussMode;
                }
                HitRegion::Editor { pane_index } => {
                    self.focus = Focus::Editor;
                    // Also set the active pane when clicking in editor area
                    self.workspace.tabs[self.workspace.active_tab].active_pane = pane_index;
                }
                HitRegion::ServerManager => {
                    self.focus = Focus::ServerManager;
                }
                HitRegion::Prompt => {
                    self.focus = Focus::Prompt;
                }
                HitRegion::None => {}
            }
        }

        // Handle terminal resize dragging
        if self.terminal.visible {
            let title_row = self.screen.rows.saturating_sub(self.terminal.height);

            match mouse {
                Mouse::Click { button: Button::Left, row, .. } if row == title_row => {
                    // Start dragging on title bar
                    self.terminal_resize_dragging = true;
                    self.terminal_resize_start_y = row;
                    self.terminal_resize_start_height = self.terminal.height;
                    return Ok(());
                }
                Mouse::Drag { button: Button::Left, row, .. } if self.terminal_resize_dragging => {
                    // Resize while dragging
                    let delta = self.terminal_resize_start_y as i32 - row as i32;
                    let new_height = (self.terminal_resize_start_height as i32 + delta).max(3) as u16;
                    self.terminal.resize_height(new_height);
                    return Ok(());
                }
                Mouse::Up { button: Button::Left, .. } if self.terminal_resize_dragging => {
                    // Stop dragging
                    self.terminal_resize_dragging = false;
                    return Ok(());
                }
                _ => {}
            }
        }

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

        // Update fuss mode viewport (actual rendering happens after terminal)
        if self.workspace.fuss.active {
            let visible_rows = self.screen.rows.saturating_sub(2) as usize;
            self.workspace.fuss.update_viewport(visible_rows);
        }

        // Build tab info for tab bar
        let tabs: Vec<TabInfo> = self.workspace.tabs.iter_mut().enumerate().map(|(i, tab)| {
            TabInfo {
                name: tab.display_name(),
                is_active: i == self.workspace.active_tab,
                is_modified: tab.is_modified(),
                index: i,
            }
        }).collect();

        // Render tab bar (returns height: 1 if multiple tabs, 0 if single tab)
        let top_offset = self.screen.render_tab_bar(&tabs, fuss_width)?;

        // Get pane count and filename before potentially getting mutable reference
        let pane_count = {
            let tab = self.workspace.active_tab();
            tab.panes.len()
        };
        let filename = {
            let tab = self.workspace.active_tab();
            let pane = &tab.panes[tab.active_pane];
            tab.buffers[pane.buffer_idx].path.as_ref().and_then(|p| p.to_str()).map(|s| s.to_string())
        };
        let filename_ref = filename.as_deref();

        // Use multi-pane rendering if we have more than one pane
        if pane_count > 1 {
            // Pre-compute is_modified for each buffer (needs mutable access)
            let buffer_modified: Vec<bool> = {
                let tab = self.workspace.active_tab_mut();
                tab.buffers.iter_mut().map(|be| be.is_modified()).collect()
            };

            let tab = self.workspace.active_tab();
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
                    is_modified: buffer_modified[pane.buffer_idx],
                }
            }).collect();

            self.screen.render_panes(
                &pane_infos,
                filename_ref,
                self.message.as_deref(),
                fuss_width,
                top_offset,
            )
        } else {
            // Single pane - use simpler render path with syntax highlighting
            // Get cached bracket match (this may compute it if not cached)
            let bracket_match = self.get_bracket_match();

            // Get is_modified first (needs mutable access for hash caching)
            let is_modified = {
                let tab = self.workspace.active_tab_mut();
                let pane = &tab.panes[tab.active_pane];
                tab.buffers[pane.buffer_idx].is_modified()
            };

            // Get values we need before mutable borrow for highlighter
            let (viewport_line, viewport_col, cursors, line_count) = {
                let tab = self.workspace.active_tab();
                let pane = &tab.panes[tab.active_pane];
                let buffer_entry = &tab.buffers[pane.buffer_idx];
                let buffer = &buffer_entry.buffer;
                let cursors = pane.cursors.clone();
                (pane.viewport_line, pane.viewport_col, cursors, buffer.line_count())
            };

            // Now get mutable access to highlighter and buffer for rendering
            {
                let tab = self.workspace.active_tab_mut();
                let buffer_idx = tab.panes[tab.active_pane].buffer_idx;
                let buffer_entry = &mut tab.buffers[buffer_idx];
                let buffer = &buffer_entry.buffer;

                self.screen.render_with_syntax(
                    buffer,
                    &cursors,
                    viewport_line,
                    viewport_col,
                    filename_ref,
                    self.message.as_deref(),
                    bracket_match,
                    fuss_width,
                    top_offset,
                    is_modified,
                    &mut buffer_entry.highlighter,
                )?;
            }

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
                let line_num_width = self.screen.line_number_width(line_count) as u16;
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
                    let line_num_width = self.screen.line_number_width(line_count) as u16;
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

            // Render terminal panel if visible (overlays editor content)
            if self.terminal.visible {
                self.screen.render_terminal(&self.terminal, fuss_width)?;
            }

            // Render fuss mode sidebar if active (after terminal so it paints on top)
            if self.workspace.fuss.active {
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
                        self.workspace.fuss.git_mode,
                    )?;
                }
            }

            // Render rename modal if active
            if let PromptState::RenameModal { ref original_name, ref new_name, .. } = self.prompt {
                self.screen.render_rename_modal(original_name, new_name)?;
            }

            // Render references panel if active
            if let PromptState::ReferencesPanel { ref locations, selected_index, ref query } = self.prompt {
                self.screen.render_references_panel(locations, selected_index, query, &self.workspace.root)?;
            }

            // Render fortress modal if active
            if let PromptState::Fortress {
                ref current_path,
                ref entries,
                selected_index,
                ref filter,
                scroll_offset,
            } = self.prompt {
                // Convert entries to tuple format for render function
                let entries_tuples: Vec<(String, PathBuf, bool)> = entries
                    .iter()
                    .map(|e| (e.name.clone(), e.path.clone(), e.is_dir))
                    .collect();
                self.screen.render_fortress_modal(
                    current_path,
                    &entries_tuples,
                    selected_index,
                    filter,
                    scroll_offset,
                )?;
                return Ok(()); // Modal handles cursor
            }

            // Render file search modal if active
            if let PromptState::FileSearch {
                ref query,
                ref results,
                selected_index,
                scroll_offset,
                searching,
            } = self.prompt {
                // Convert results to tuple format for render function
                let results_tuples: Vec<(PathBuf, usize, String)> = results
                    .iter()
                    .map(|r| (r.path.clone(), r.line_num, r.line_content.clone()))
                    .collect();
                self.screen.render_file_search_modal(
                    query,
                    &results_tuples,
                    selected_index,
                    scroll_offset,
                    searching,
                )?;
                return Ok(()); // Modal handles cursor
            }

            // Render command palette if active
            if let PromptState::CommandPalette {
                ref query,
                ref filtered,
                selected_index,
                scroll_offset,
            } = self.prompt {
                // Convert commands to tuple format for render function
                let commands_tuples: Vec<(String, String, String, String)> = filtered
                    .iter()
                    .map(|c| (c.name.to_string(), c.shortcut.to_string(), c.category.to_string(), c.id.to_string()))
                    .collect();
                self.screen.render_command_palette(
                    query,
                    &commands_tuples,
                    selected_index,
                    scroll_offset,
                )?;
                return Ok(()); // Modal handles cursor
            }

            // Render help menu if active
            if let PromptState::HelpMenu {
                ref query,
                ref filtered,
                selected_index,
                scroll_offset,
                show_alt,
            } = self.prompt {
                // Convert keybinds to tuple format for render function
                // Use alt_shortcut when show_alt is true (for entries that have one)
                let keybinds_tuples: Vec<(String, String, String)> = filtered
                    .iter()
                    .map(|kb| {
                        let shortcut = if show_alt && !kb.alt_shortcut.is_empty() {
                            kb.alt_shortcut.to_string()
                        } else {
                            kb.shortcut.to_string()
                        };
                        (shortcut, kb.description.to_string(), kb.category.to_string())
                    })
                    .collect();
                self.screen.render_help_menu(
                    query,
                    &keybinds_tuples,
                    selected_index,
                    scroll_offset,
                    show_alt,
                )?;
                return Ok(()); // Modal handles cursor
            }

            // Render find/replace bar if active (replaces status bar)
            if let PromptState::FindReplace {
                ref find_query,
                ref replace_text,
                active_field,
                case_insensitive,
                regex_mode,
            } = self.prompt {
                let is_find_active = active_field == FindReplaceField::Find;
                self.screen.render_find_replace_bar(
                    find_query,
                    replace_text,
                    is_find_active,
                    case_insensitive,
                    regex_mode,
                    self.search_state.matches.len(),
                    self.search_state.current_match,
                    fuss_width,
                )?;
                return Ok(()); // Skip cursor repositioning, bar handles it
            }

            // After all overlays are rendered, reposition cursor to the correct location
            // (overlays may have moved the terminal cursor position)
            let cursor = cursors.primary();
            let cursor_row = (cursor.line.saturating_sub(viewport_line)) as u16 + top_offset;
            let line_num_width = self.screen.line_number_width(line_count) as u16;
            // Account for horizontal scroll offset
            let cursor_screen_col = fuss_width + line_num_width + 1 + (cursor.col.saturating_sub(viewport_col)) as u16;
            self.screen.show_cursor_at(cursor_screen_col, cursor_row)?;

            Ok(())
        }
    }

    fn handle_key_with_mods(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        // Handle Ctrl+F/Ctrl+R specially - they can toggle/switch even when in FindReplace prompt
        if let PromptState::FindReplace { .. } = &self.prompt {
            match (&key, &mods) {
                (Key::Char('f'), Modifiers { ctrl: true, .. }) => {
                    self.open_find();
                    return Ok(());
                }
                (Key::Char('r'), Modifiers { ctrl: true, .. }) => {
                    self.open_replace();
                    return Ok(());
                }
                // Alt+I: toggle case insensitivity
                (Key::Char('i'), Modifiers { alt: true, .. }) => {
                    self.toggle_case_sensitivity();
                    return Ok(());
                }
                // Alt+X: toggle regex mode
                (Key::Char('x'), Modifiers { alt: true, .. }) => {
                    self.toggle_regex_mode();
                    return Ok(());
                }
                // Alt+Enter or Ctrl+Shift+Enter: replace all
                (Key::Enter, Modifiers { alt: true, .. }) |
                (Key::Enter, Modifiers { ctrl: true, shift: true, .. }) => {
                    self.replace_all();
                    return Ok(());
                }
                _ => {}
            }
        }

        // Handle active prompts first
        if self.prompt != PromptState::None {
            return self.handle_prompt_key(key);
        }

        // Focus-based routing for server manager
        if self.focus == Focus::ServerManager && self.server_manager.visible {
            return self.handle_server_manager_key(key, mods);
        }

        // Clear message on any key
        self.message = None;

        // Toggle fuss mode: Ctrl+B or F3 (global shortcut that sets focus)
        if matches!((&key, &mods), (Key::Char('b'), Modifiers { ctrl: true, .. }) | (Key::F(3), _)) {
            self.toggle_fuss_mode();
            return Ok(());
        }

        // Focus-based routing for fuss mode
        if self.focus == Focus::FussMode && self.workspace.fuss.active {
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
                    self.dismiss_completion();
                    return Ok(());
                }
                // Backspace: remove from filter (if any) and continue
                (Key::Backspace, _) => {
                    if !self.lsp_state.completion_filter.is_empty() {
                        self.lsp_state.completion_filter.pop();
                        self.filter_completions();
                    } else {
                        // No filter left, dismiss popup
                        self.dismiss_completion();
                    }
                    // Continue to normal backspace handling
                }
                // Character input: add to filter and continue typing
                (Key::Char(c), Modifiers { ctrl: false, alt: false, .. }) => {
                    self.lsp_state.completion_filter.push(*c);
                    self.filter_completions();
                    // If no matches left, dismiss
                    if self.lsp_state.completions.is_empty() {
                        self.dismiss_completion();
                    }
                    // Continue to normal character handling
                }
                // Any other key dismisses popup
                _ => {
                    self.dismiss_completion();
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
            | (Key::Char(']'), Modifiers { ctrl: true, .. })
            | (Key::Char('5'), Modifiers { ctrl: true, .. })  // Ctrl+] reports as Ctrl+5 on some terminals
            | (Key::Char('\x1d'), _) => {  // 0x1D = Ctrl+] as raw control character
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

            // Toggle line comment: Ctrl+/
            // Different terminals send: Ctrl+/, Ctrl+_, \x1f (ASCII 31), or Ctrl+7
            (Key::Char('/'), Modifiers { ctrl: true, .. })
            | (Key::Char('_'), Modifiers { ctrl: true, .. })
            | (Key::Char('\x1f'), _)
            | (Key::Char('7'), Modifiers { ctrl: true, .. }) => self.toggle_line_comment(),

            // Select line: Ctrl+L
            (Key::Char('l'), Modifiers { ctrl: true, .. }) => self.select_line(),
            // Select word: Ctrl+D (select word at cursor, or next occurrence if already selected)
            (Key::Char('d'), Modifiers { ctrl: true, .. }) => self.select_word(),

            // === Find/Replace ===
            // Find: Ctrl+F
            (Key::Char('f'), Modifiers { ctrl: true, .. }) => self.open_find(),
            // Replace: Ctrl+R (or Ctrl+H for compatibility)
            (Key::Char('r'), Modifiers { ctrl: true, .. }) |
            (Key::Char('h'), Modifiers { ctrl: true, alt: true, .. }) => self.open_replace(),
            // Find next: F3
            (Key::F(3), Modifiers { shift: false, .. }) => self.find_next(),
            // Find previous: Shift+F3
            (Key::F(3), Modifiers { shift: true, .. }) => self.find_prev(),

            // === File operations ===
            // Open file browser (Fortress mode): Ctrl+O
            (Key::Char('o'), Modifiers { ctrl: true, .. }) => self.open_fortress(),
            // Go to line: Ctrl+G or F5
            (Key::Char('g'), Modifiers { ctrl: true, .. }) |
            (Key::F(5), _) => self.open_goto_line(),
            // Multi-file search: F4
            (Key::F(4), _) => self.open_file_search(),
            // Command palette: Ctrl+P
            (Key::Char('p'), Modifiers { ctrl: true, .. }) => self.open_command_palette(),

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

            // Unix-style kill commands
            // Kill to end of line: Ctrl+K
            (Key::Char('k'), Modifiers { ctrl: true, .. }) => self.kill_to_end_of_line(),
            // Kill to start of line: Ctrl+U
            (Key::Char('u'), Modifiers { ctrl: true, .. }) => self.kill_to_start_of_line(),
            // Yank from kill ring: Ctrl+Y
            (Key::Char('y'), Modifiers { ctrl: true, .. }) => self.yank(),
            // Cycle yank stack: Alt+Y
            (Key::Char('y'), Modifiers { alt: true, .. }) => self.yank_cycle(),

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
            (Key::F(1), Modifiers { shift: false, .. }) => self.lsp_hover(),
            // Code completion: Ctrl+N (vim-style)
            (Key::Char('n'), Modifiers { ctrl: true, .. }) => self.lsp_complete(),
            // Rename: F2
            (Key::F(2), _) => self.lsp_rename(),
            // Server manager: Alt+M
            (Key::Char('m'), Modifiers { alt: true, .. }) => self.toggle_server_manager(),

            // === Help ===
            // Help / keybindings: Shift+F1
            (Key::F(1), Modifiers { shift: true, .. }) => self.open_help_menu(),

            _ => {}
        }

        // Check if buffer was edited and needs backup
        self.on_buffer_edit();

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
            } else {
                // On first line, move to start of line
                cursor.move_to(0, 0, extend_selection);
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
            } else {
                // On last line, move to end of line
                let line_len = line_lens.get(cursor.line).copied().unwrap_or(0);
                cursor.move_to(cursor.line, line_len, extend_selection);
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

            // Invalidate caches
            self.invalidate_highlight_cache(start.line);
            self.invalidate_bracket_cache();

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

        // Multi-cursor: compute absolute character indices FIRST from a frozen view of the buffer.
        // Then sort by ASCENDING char index, apply edits from start to end,
        // and track cumulative offset to adjust subsequent positions.

        // Step 1: Compute char indices for all cursors from current buffer state
        let mut cursor_char_indices: Vec<(usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let char_idx = self.buffer().line_col_to_char(c.line, c.col);
                (i, char_idx)
            })
            .collect();

        // Step 2: Sort by ASCENDING char index (process from start of document)
        cursor_char_indices.sort_by(|a, b| a.1.cmp(&b.1));

        // Invalidate caches from the earliest cursor's line
        if let Some(&(first_cursor_idx, _)) = cursor_char_indices.first() {
            let first_line = self.cursors().all()[first_cursor_idx].line;
            self.invalidate_highlight_cache(first_line);
        }
        self.invalidate_bracket_cache();

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        let text_char_count = text.chars().count();
        let cursor_before = self.cursor_pos();

        // Step 3: Apply inserts from start to end, tracking cumulative offset
        let mut cumulative_offset: usize = 0;
        let mut new_positions: Vec<(usize, usize, usize)> = Vec::new(); // (cursor_idx, line, col)

        for (cursor_idx, original_char_idx) in cursor_char_indices {
            // Adjust position by cumulative offset from previous inserts
            let adjusted_char_idx = original_char_idx + cumulative_offset;

            self.buffer_mut().insert(adjusted_char_idx, text);
            self.history_mut().record_insert(adjusted_char_idx, text.to_string(), cursor_before, cursor_before);

            // New cursor position is right after the inserted text
            let new_char_idx = adjusted_char_idx + text_char_count;
            let (new_line, new_col) = self.buffer().char_to_line_col(new_char_idx);
            new_positions.push((cursor_idx, new_line, new_col));

            // Update cumulative offset for next cursor
            cumulative_offset += text_char_count;
        }

        // Step 4: Update all cursor positions at once
        for (cursor_idx, new_line, new_col) in new_positions {
            let cursor = &mut self.cursors_mut().all_mut()[cursor_idx];
            cursor.line = new_line;
            cursor.col = new_col;
            cursor.desired_col = new_col;
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
        let edit_line = self.cursor().line;
        let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);

        self.buffer_mut().insert(idx, text);
        self.invalidate_highlight_cache(edit_line);
        self.invalidate_bracket_cache();
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
        // Multi-cursor: compute absolute character indices FIRST from a frozen view of the buffer.
        // Sort by ASCENDING, process start to end, track cumulative offset.

        // Step 1: Compute char indices for all cursors from current buffer state
        let mut cursor_char_indices: Vec<(usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let char_idx = self.buffer().line_col_to_char(c.line, c.col);
                (i, char_idx)
            })
            .collect();

        // Step 2: Sort by ASCENDING char index (process from start of document)
        cursor_char_indices.sort_by(|a, b| a.1.cmp(&b.1));

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        let cursor_before = self.cursor_pos();

        // Step 3: Apply deletes from start to end, tracking cumulative offset
        let mut cumulative_offset: isize = 0;
        let mut new_positions: Vec<(usize, usize, usize)> = Vec::new();

        for (cursor_idx, original_char_idx) in cursor_char_indices {
            if original_char_idx == 0 {
                // Can't delete backward from position 0, keep cursor where it is
                let cursor = &self.cursors().all()[cursor_idx];
                new_positions.push((cursor_idx, cursor.line, cursor.col));
                continue;
            }

            // Adjust position by cumulative offset from previous deletes
            let adjusted_char_idx = (original_char_idx as isize + cumulative_offset) as usize;

            if adjusted_char_idx > 0 {
                let deleted = self.buffer().char_at(adjusted_char_idx - 1).map(|c| c.to_string()).unwrap_or_default();
                self.buffer_mut().delete(adjusted_char_idx - 1, adjusted_char_idx);
                self.history_mut().record_delete(adjusted_char_idx - 1, deleted, cursor_before, cursor_before);

                // New cursor position is at the delete point
                let new_char_idx = adjusted_char_idx - 1;
                let (new_line, new_col) = self.buffer().char_to_line_col(new_char_idx);
                new_positions.push((cursor_idx, new_line, new_col));

                // Update cumulative offset (we deleted 1 char, so offset decreases by 1)
                cumulative_offset -= 1;
            }
        }

        // Step 4: Update all cursor positions at once
        for (cursor_idx, new_line, new_col) in new_positions {
            let cursor = &mut self.cursors_mut().all_mut()[cursor_idx];
            cursor.line = new_line;
            cursor.col = new_col;
            cursor.desired_col = new_col;
        }

        // Record all cursor positions after the operation
        let cursors_after = self.all_cursor_positions();
        self.history_mut().set_cursors_after(cursors_after);
        self.history_mut().end_group();
        self.cursors_mut().merge_overlapping();
    }

    /// Delete forward at all cursor positions (multi-cursor)
    fn delete_forward_multi(&mut self) {
        // Multi-cursor: compute absolute character indices FIRST from a frozen view of the buffer.
        // Sort by ASCENDING, process start to end, track cumulative offset.

        let total_chars = self.buffer().char_count();

        // Step 1: Compute char indices for all cursors from current buffer state
        let mut cursor_char_indices: Vec<(usize, usize)> = self.cursors().all()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let char_idx = self.buffer().line_col_to_char(c.line, c.col);
                (i, char_idx)
            })
            .collect();

        // Step 2: Sort by ASCENDING char index (process from start of document)
        cursor_char_indices.sort_by(|a, b| a.1.cmp(&b.1));

        // Record all cursor positions before the operation
        let cursors_before = self.all_cursor_positions();
        self.history_mut().begin_group();
        self.history_mut().set_cursors_before(cursors_before);

        let cursor_before = self.cursor_pos();

        // Step 3: Apply deletes from start to end, tracking cumulative offset
        let mut cumulative_offset: isize = 0;
        let mut new_positions: Vec<(usize, usize, usize)> = Vec::new();

        for (cursor_idx, original_char_idx) in cursor_char_indices {
            // Adjust position by cumulative offset from previous deletes
            let adjusted_char_idx = (original_char_idx as isize + cumulative_offset) as usize;
            let current_total = (total_chars as isize + cumulative_offset) as usize;

            if adjusted_char_idx < current_total {
                let deleted = self.buffer().char_at(adjusted_char_idx).map(|c| c.to_string()).unwrap_or_default();
                // Don't delete newlines in multi-cursor mode for simplicity
                if deleted != "\n" {
                    self.buffer_mut().delete(adjusted_char_idx, adjusted_char_idx + 1);
                    self.history_mut().record_delete(adjusted_char_idx, deleted, cursor_before, cursor_before);
                    cumulative_offset -= 1;
                }
            }

            // Cursor position: convert from adjusted char index (cursor doesn't move for delete forward)
            let (new_line, new_col) = self.buffer().char_to_line_col(adjusted_char_idx.min(self.buffer().char_count()));
            new_positions.push((cursor_idx, new_line, new_col));
        }

        // Step 4: Update all cursor positions at once
        for (cursor_idx, new_line, new_col) in new_positions {
            let cursor = &mut self.cursors_mut().all_mut()[cursor_idx];
            cursor.line = new_line;
            cursor.col = new_col;
            cursor.desired_col = new_col;
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

        // Invalidate caches from cursor line (or previous line if merging)
        let invalidate_line = if self.cursor().col == 0 && self.cursor().line > 0 {
            self.cursor().line - 1
        } else {
            self.cursor().line
        };
        self.invalidate_highlight_cache(invalidate_line);
        self.invalidate_bracket_cache();

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

        // Invalidate caches from cursor line
        self.invalidate_highlight_cache(self.cursor().line);
        self.invalidate_bracket_cache();

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
            self.yank_push(deleted.clone());
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
            self.yank_push(deleted.clone());
            self.cursor_mut().line = start_line;
            self.cursor_mut().col = start_col;
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(start_idx, deleted, cursor_before, cursor_after);
            self.history_mut().maybe_break_group();
        }
    }

    /// Push text onto the yank stack (kill ring)
    fn yank_push(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        // Limit stack size to 32 entries
        const MAX_YANK_STACK: usize = 32;
        if self.yank_stack.len() >= MAX_YANK_STACK {
            self.yank_stack.remove(0);
        }
        self.yank_stack.push(text);
        // Reset cycling state
        self.yank_index = None;
    }

    /// Yank (paste) from yank stack (Ctrl+Y)
    fn yank(&mut self) {
        if self.yank_stack.is_empty() {
            self.message = Some("Yank stack empty".to_string());
            return;
        }

        // Delete selection first if any
        self.delete_selection();

        // Get the most recent entry
        let text = self.yank_stack.last().unwrap().clone();
        let cursor_before = self.cursor_pos();

        // Insert the text
        let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
        self.buffer_mut().insert(idx, &text);

        // Move cursor to end of inserted text
        let text_len = text.chars().count();
        self.last_yank_len = text_len;

        // Update cursor position
        for ch in text.chars() {
            if ch == '\n' {
                self.cursor_mut().line += 1;
                self.cursor_mut().col = 0;
            } else {
                self.cursor_mut().col += 1;
            }
        }
        self.cursor_mut().desired_col = self.cursor().col;

        let cursor_after = self.cursor_pos();
        self.history_mut().record_insert(idx, text, cursor_before, cursor_after);

        // Set yank index for cycling
        self.yank_index = Some(self.yank_stack.len() - 1);
    }

    /// Cycle through yank stack (Alt+Y) - must be used after Ctrl+Y
    fn yank_cycle(&mut self) {
        // Only works if we just yanked
        let current_idx = match self.yank_index {
            Some(idx) => idx,
            None => {
                self.message = Some("No active yank to cycle".to_string());
                return;
            }
        };

        if self.yank_stack.len() <= 1 {
            self.message = Some("Only one item in yank stack".to_string());
            return;
        }

        // Calculate previous index (cycle backwards through stack)
        let new_idx = if current_idx == 0 {
            self.yank_stack.len() - 1
        } else {
            current_idx - 1
        };

        // Delete the previously yanked text
        let cursor_before = self.cursor_pos();
        let end_idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
        let start_idx = end_idx.saturating_sub(self.last_yank_len);

        if start_idx < end_idx {
            let deleted: String = self.buffer().slice(start_idx, end_idx).chars().collect();
            self.buffer_mut().delete(start_idx, end_idx);

            // Move cursor back
            let (line, col) = self.buffer().char_to_line_col(start_idx);
            self.cursor_mut().line = line;
            self.cursor_mut().col = col;
            self.cursor_mut().desired_col = self.cursor().col;
        }

        // Insert the new text from yank stack
        let text = self.yank_stack[new_idx].clone();
        let idx = self.buffer().line_col_to_char(self.cursor().line, self.cursor().col);
        self.buffer_mut().insert(idx, &text);

        // Move cursor to end of inserted text
        let text_len = text.chars().count();
        self.last_yank_len = text_len;

        for ch in text.chars() {
            if ch == '\n' {
                self.cursor_mut().line += 1;
                self.cursor_mut().col = 0;
            } else {
                self.cursor_mut().col += 1;
            }
        }
        self.cursor_mut().desired_col = self.cursor().col;

        let cursor_after = self.cursor_pos();
        self.history_mut().record_insert(idx, text, cursor_before, cursor_after);

        // Update yank index
        self.yank_index = Some(new_idx);
    }

    /// Kill (delete) from cursor to end of line (Ctrl+K)
    fn kill_to_end_of_line(&mut self) {
        if self.delete_selection() {
            return;
        }

        let line = self.cursor().line;
        let col = self.cursor().col;
        let line_len = self.buffer().line_len(line);

        if col >= line_len {
            // At end of line - delete the newline to join with next line
            if line < self.buffer().line_count().saturating_sub(1) {
                let cursor_before = self.cursor_pos();
                let idx = self.buffer().line_col_to_char(line, col);
                self.buffer_mut().delete(idx, idx + 1);
                self.yank_push("\n".to_string());
                let cursor_after = self.cursor_pos();
                self.history_mut().record_delete(idx, "\n".to_string(), cursor_before, cursor_after);
            }
        } else {
            // Delete from cursor to end of line
            let cursor_before = self.cursor_pos();
            let start_idx = self.buffer().line_col_to_char(line, col);
            let end_idx = self.buffer().line_col_to_char(line, line_len);
            let deleted: String = self.buffer().slice(start_idx, end_idx).chars().collect();
            self.buffer_mut().delete(start_idx, end_idx);
            self.yank_push(deleted.clone());
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(start_idx, deleted, cursor_before, cursor_after);
        }
        self.history_mut().maybe_break_group();
    }

    /// Kill (delete) from cursor to start of line (Ctrl+U)
    fn kill_to_start_of_line(&mut self) {
        if self.delete_selection() {
            return;
        }

        let line = self.cursor().line;
        let col = self.cursor().col;

        if col == 0 {
            // At start of line - delete the newline before to join with previous line
            if line > 0 {
                let cursor_before = self.cursor_pos();
                let idx = self.buffer().line_col_to_char(line, 0);
                self.buffer_mut().delete(idx - 1, idx);
                self.yank_push("\n".to_string());
                // Move cursor to end of previous line
                self.cursor_mut().line = line - 1;
                self.cursor_mut().col = self.buffer().line_len(line - 1);
                self.cursor_mut().desired_col = self.cursor().col;
                let cursor_after = self.cursor_pos();
                self.history_mut().record_delete(idx - 1, "\n".to_string(), cursor_before, cursor_after);
            }
        } else {
            // Delete from start of line to cursor
            let cursor_before = self.cursor_pos();
            let start_idx = self.buffer().line_col_to_char(line, 0);
            let end_idx = self.buffer().line_col_to_char(line, col);
            let deleted: String = self.buffer().slice(start_idx, end_idx).chars().collect();
            self.buffer_mut().delete(start_idx, end_idx);
            self.yank_push(deleted.clone());
            self.cursor_mut().col = 0;
            self.cursor_mut().desired_col = 0;
            let cursor_after = self.cursor_pos();
            self.history_mut().record_delete(start_idx, deleted, cursor_before, cursor_after);
        }
        self.history_mut().maybe_break_group();
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

    /// Toggle line comment on current line or all lines in selection
    /// Works like VSCode: if all lines are commented, uncomment them; otherwise comment them all
    fn toggle_line_comment(&mut self) {
        // Get the comment prefix for current language
        let comment_prefix = match self.buffer_entry().highlighter.line_comment() {
            Some(prefix) => prefix,
            None => {
                self.message = Some("No line comment syntax for this file type".to_string());
                return;
            }
        };

        // Determine line range to operate on
        let (start_line, end_line) = if let Some((start, end)) = self.cursor().selection_bounds() {
            // Selection: operate on all lines in selection
            (start.line, end.line)
        } else {
            // No selection: operate on current line only
            let line = self.cursor().line;
            (line, line)
        };

        // Check if all lines in range are commented
        let all_commented = (start_line..=end_line).all(|line_idx| {
            if let Some(line) = self.buffer().line_str(line_idx) {
                let trimmed = line.trim_start();
                trimmed.starts_with(comment_prefix)
            } else {
                false
            }
        });

        self.history_mut().begin_group();

        if all_commented {
            // Uncomment all lines
            for line_idx in start_line..=end_line {
                self.uncomment_line(line_idx, comment_prefix);
            }
        } else {
            // Comment all lines - find minimum indentation first for alignment
            let min_indent = (start_line..=end_line)
                .filter_map(|line_idx| {
                    self.buffer().line_str(line_idx).map(|line| {
                        if line.trim().is_empty() {
                            usize::MAX // Don't count empty lines
                        } else {
                            line.chars().take_while(|c| c.is_whitespace()).count()
                        }
                    })
                })
                .min()
                .unwrap_or(0);

            for line_idx in start_line..=end_line {
                self.comment_line(line_idx, comment_prefix, min_indent);
            }
        }

        self.history_mut().end_group();

        // Invalidate highlight cache for affected lines
        self.invalidate_highlight_cache(start_line);
    }

    /// Add a comment prefix to a line at the specified indentation level
    fn comment_line(&mut self, line_idx: usize, prefix: &str, indent: usize) {
        let Some(line) = self.buffer().line_str(line_idx) else {
            return;
        };

        // Skip completely empty lines
        if line.is_empty() {
            return;
        }

        let cursor_before = self.cursor_pos();

        // Insert comment prefix after the minimum indentation
        let insert_pos = self.buffer().line_col_to_char(line_idx, indent);
        let insert_text = format!("{} ", prefix);
        self.buffer_mut().insert(insert_pos, &insert_text);

        let cursor_after = self.cursor_pos();
        self.history_mut().record_insert(insert_pos, insert_text, cursor_before, cursor_after);

        // Adjust cursor if on this line and after the insert point
        if self.cursor().line == line_idx && self.cursor().col >= indent {
            let prefix_len = prefix.len() + 1; // +1 for space
            self.cursor_mut().col += prefix_len;
            self.cursor_mut().desired_col = self.cursor().col;
        }
    }

    /// Remove a comment prefix from a line
    fn uncomment_line(&mut self, line_idx: usize, prefix: &str) {
        let Some(line) = self.buffer().line_str(line_idx) else {
            return;
        };

        // Find where the comment prefix starts
        let trimmed = line.trim_start();
        if !trimmed.starts_with(prefix) {
            return;
        }

        let cursor_before = self.cursor_pos();

        // Calculate the position of the comment prefix
        let leading_spaces = line.len() - trimmed.len();
        let delete_start = self.buffer().line_col_to_char(line_idx, leading_spaces);

        // Calculate how much to delete (prefix + optional space after)
        let delete_len = if trimmed.len() > prefix.len() && trimmed.chars().nth(prefix.len()) == Some(' ') {
            prefix.len() + 1
        } else {
            prefix.len()
        };

        let delete_end = delete_start + delete_len;
        let deleted_text: String = self.buffer().slice(delete_start, delete_end).chars().collect();
        self.buffer_mut().delete(delete_start, delete_end);

        let cursor_after = self.cursor_pos();
        self.history_mut().record_delete(delete_start, deleted_text, cursor_before, cursor_after);

        // Adjust cursor if on this line and after the deleted region
        if self.cursor().line == line_idx && self.cursor().col > leading_spaces {
            let new_col = if self.cursor().col >= leading_spaces + delete_len {
                self.cursor().col - delete_len
            } else {
                leading_spaces
            };
            self.cursor_mut().col = new_col;
            self.cursor_mut().desired_col = self.cursor().col;
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

        // In multi-cursor mode, scroll to the LAST cursor (most recently added)
        // This ensures Ctrl+D shows the newly found occurrence
        let cursors = self.cursors();
        let target_cursor = if cursors.len() > 1 {
            cursors.all().last().unwrap()
        } else {
            cursors.primary()
        };
        let cursor_line = target_cursor.line;
        let cursor_col = target_cursor.col;

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
            // Delete backup after successful save (use full path to match backup hash)
            let full_path = if self.buffer_entry().is_orphan {
                p.clone()
            } else {
                self.workspace.root.join(p)
            };
            let _ = self.workspace.delete_backup(&full_path);
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
        // Check if current buffer has unsaved changes
        if self.buffer_entry_mut().is_modified() {
            self.prompt = PromptState::CloseBufferConfirm;
            self.message = Some("Unsaved changes. [S]ave / [D]iscard / [C]ancel".to_string());
            return;
        }
        self.close_pane_force();
    }

    /// Close pane without checking for unsaved changes (used after save/discard)
    fn close_pane_force(&mut self) {
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
            self.focus = Focus::FussMode;
        } else {
            self.workspace.fuss.deactivate();
            self.return_focus();
        }
    }

    fn handle_fuss_key(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        // Handle git mode separately
        if self.workspace.fuss.git_mode {
            return self.handle_fuss_git_key(key, mods);
        }

        match (&key, &mods) {
            // Quit: Ctrl+Q (still works in fuss mode)
            (Key::Char('q'), Modifiers { ctrl: true, .. }) => {
                self.try_quit();
            }

            // Exit fuss mode (Escape or F3)
            (Key::Escape, _) | (Key::F(3), _) => {
                self.workspace.fuss.filter_clear();
                self.workspace.fuss.deactivate();
                self.return_focus();
            }

            // Navigation
            (Key::Up, _) => {
                self.workspace.fuss.filter_clear();
                self.workspace.fuss.move_up();
            }
            (Key::Down, _) => {
                self.workspace.fuss.filter_clear();
                self.workspace.fuss.move_down();
            }

            // Toggle expand/collapse directory, or collapse parent if on a file/collapsed dir
            (Key::Char(' '), _) => {
                self.workspace.fuss.filter_clear();
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
                self.workspace.fuss.filter_clear();
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
                self.workspace.fuss.filter_clear();
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
            (Key::Enter, _) => {
                self.workspace.fuss.filter_clear();
                if self.workspace.fuss.is_dir_selected() {
                    self.workspace.fuss.toggle_expand();
                } else if let Some(path) = self.workspace.fuss.selected_file() {
                    self.open_file(&path)?;
                    self.workspace.fuss.deactivate();
                }
            }

            // Toggle hidden files: Alt+.
            (Key::Char('.'), Modifiers { alt: true, .. }) => {
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

            // Open file in vertical split: Ctrl+V
            (Key::Char('v'), Modifiers { ctrl: true, .. }) => {
                if !self.workspace.fuss.is_dir_selected() {
                    if let Some(path) = self.workspace.fuss.selected_file() {
                        self.open_file_in_vsplit(&path)?;
                        self.workspace.fuss.deactivate();
                    }
                }
            }

            // Open file in horizontal split: Ctrl+S
            (Key::Char('s'), Modifiers { ctrl: true, .. }) => {
                if !self.workspace.fuss.is_dir_selected() {
                    if let Some(path) = self.workspace.fuss.selected_file() {
                        self.open_file_in_hsplit(&path)?;
                        self.workspace.fuss.deactivate();
                    }
                }
            }

            // Enter git mode: Alt+G
            (Key::Char('g'), Modifiers { alt: true, .. }) => {
                self.workspace.fuss.enter_git_mode();
                self.message = Some("Git: [a]dd [u]nstage [d]iff [m]sg [p]ush pu[l]l [f]etch [t]ag".to_string());
            }

            // Backspace: remove last filter character
            (Key::Backspace, _) => {
                self.workspace.fuss.filter_pop();
            }

            // Regular characters: add to filter for fuzzy jump
            (Key::Char(c), Modifiers { ctrl: false, alt: false, .. }) => {
                self.workspace.fuss.filter_push(*c);
            }

            _ => {}
        }
        Ok(())
    }

    /// Handle keys when in git sub-mode within fuss
    fn handle_fuss_git_key(&mut self, key: Key, mods: Modifiers) -> Result<()> {
        // Any key exits git mode (after potentially doing an action)
        self.workspace.fuss.exit_git_mode();
        self.message = None;

        match (&key, &mods) {
            // Git: Stage file (a)
            (Key::Char('a'), _) => {
                if self.workspace.fuss.stage_selected() {
                    self.message = Some("Staged".to_string());
                } else {
                    self.message = Some("Failed to stage".to_string());
                }
            }

            // Git: Unstage file (u)
            (Key::Char('u'), _) => {
                if self.workspace.fuss.unstage_selected() {
                    self.message = Some("Unstaged".to_string());
                } else {
                    self.message = Some("Failed to unstage".to_string());
                }
            }

            // Git: Show diff (d)
            (Key::Char('d'), _) => {
                if let Some((filename, diff)) = self.workspace.fuss.get_diff_for_selected() {
                    let display_name = format!("[diff] {}", filename);
                    self.workspace.open_content_tab(&diff, &display_name);
                    self.workspace.fuss.deactivate();
                } else {
                    self.message = Some("No diff available".to_string());
                }
            }

            // Git: Commit (m) - opens prompt for commit message
            (Key::Char('m'), _) => {
                self.prompt = PromptState::TextInput {
                    label: "Commit message: ".to_string(),
                    buffer: String::new(),
                    action: TextInputAction::GitCommit,
                };
                self.message = Some("Enter commit message (Enter to commit, Esc to cancel)".to_string());
            }

            // Git: Push (p)
            (Key::Char('p'), _) => {
                let (_, msg) = self.workspace.fuss.git_push();
                self.message = Some(msg);
            }

            // Git: Pull (l)
            (Key::Char('l'), _) => {
                let (_, msg) = self.workspace.fuss.git_pull();
                self.message = Some(msg);
            }

            // Git: Fetch (f)
            (Key::Char('f'), _) => {
                let (_, msg) = self.workspace.fuss.git_fetch();
                self.message = Some(msg);
            }

            // Git: Tag (t) - opens prompt for tag name
            (Key::Char('t'), _) => {
                self.prompt = PromptState::TextInput {
                    label: "Tag name: ".to_string(),
                    buffer: String::new(),
                    action: TextInputAction::GitTag,
                };
                self.message = Some("Enter tag name (Enter to create, Esc to cancel)".to_string());
            }

            // Escape or any other key just cancels git mode
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
            PromptState::CloseBufferConfirm => {
                match key {
                    Key::Char('s') | Key::Char('S') => {
                        // Save and close
                        if let Err(e) = self.save() {
                            self.message = Some(format!("Save failed: {}", e));
                        } else {
                            self.prompt = PromptState::None;
                            self.close_pane_force();
                        }
                    }
                    Key::Char('d') | Key::Char('D') => {
                        // Discard changes - delete backup for this buffer and close
                        if let Some(path) = self.buffer_entry().path.clone() {
                            let full_path = if self.buffer_entry().is_orphan {
                                path.clone()
                            } else {
                                self.workspace.root.join(&path)
                            };
                            let _ = self.workspace.delete_backup(&full_path);
                        }
                        self.prompt = PromptState::None;
                        self.close_pane_force();
                    }
                    Key::Char('c') | Key::Char('C') | Key::Escape => {
                        // Cancel - return to editing
                        self.prompt = PromptState::None;
                        self.message = None;
                    }
                    _ => {
                        // Repeat the prompt
                        self.message = Some("Unsaved changes. [S]ave / [D]iscard / [C]ancel".to_string());
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
            PromptState::FindReplace {
                ref mut find_query,
                ref mut replace_text,
                ref mut active_field,
                case_insensitive: _,
                regex_mode: _,
            } => {
                match key {
                    Key::Escape => {
                        self.prompt = PromptState::None;
                        self.search_state.matches.clear();
                        self.message = None;
                    }
                    Key::Enter => {
                        if *active_field == FindReplaceField::Find {
                            // Find next
                            self.find_next();
                        } else {
                            // Replace current and find next
                            self.replace_current();
                        }
                    }
                    Key::Tab => {
                        // Switch between find and replace fields
                        *active_field = if *active_field == FindReplaceField::Find {
                            FindReplaceField::Replace
                        } else {
                            FindReplaceField::Find
                        };
                    }
                    Key::BackTab => {
                        // Switch in reverse
                        *active_field = if *active_field == FindReplaceField::Replace {
                            FindReplaceField::Find
                        } else {
                            FindReplaceField::Replace
                        };
                    }
                    Key::Up => {
                        // Find previous
                        self.find_prev();
                    }
                    Key::Down => {
                        // Find next
                        self.find_next();
                    }
                    Key::Backspace => {
                        if *active_field == FindReplaceField::Find {
                            find_query.pop();
                            self.search_state.last_query.clear(); // Force re-search
                            self.update_search_matches();
                        } else {
                            replace_text.pop();
                        }
                    }
                    Key::Char(c) => {
                        if *active_field == FindReplaceField::Find {
                            find_query.push(c);
                            self.search_state.last_query.clear(); // Force re-search
                            self.update_search_matches();
                        } else {
                            replace_text.push(c);
                        }
                    }
                    _ => {}
                }
            }
            PromptState::Fortress {
                ref current_path,
                ref entries,
                ref mut selected_index,
                ref mut filter,
                ref mut scroll_offset,
            } => {
                // Filter entries based on query
                let filtered: Vec<(usize, &FortressEntry)> = if filter.is_empty() {
                    entries.iter().enumerate().collect()
                } else {
                    let f = filter.to_lowercase();
                    entries.iter().enumerate()
                        .filter(|(_, e)| e.name.to_lowercase().contains(&f))
                        .collect()
                };

                match key {
                    Key::Enter => {
                        // Open selected entry
                        if let Some((orig_idx, _entry)) = filtered.get(*selected_index) {
                            let entry = entries[*orig_idx].clone();
                            if entry.is_dir {
                                // Navigate into directory
                                self.fortress_navigate_to(&entry.path);
                            } else {
                                // Open the file
                                self.prompt = PromptState::None;
                                self.fortress_open_file(&entry.path);
                            }
                        }
                    }
                    Key::Escape => {
                        self.prompt = PromptState::None;
                        self.message = None;
                    }
                    Key::Backspace if filter.is_empty() => {
                        // Go up one directory when filter is empty
                        if let Some(parent) = current_path.parent() {
                            let parent = parent.to_path_buf();
                            self.fortress_navigate_to(&parent);
                        }
                    }
                    Key::Backspace => {
                        filter.pop();
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    Key::Up => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                            // Adjust scroll
                            if *selected_index < *scroll_offset {
                                *scroll_offset = *selected_index;
                            }
                        }
                    }
                    Key::Down => {
                        if *selected_index + 1 < filtered.len() {
                            *selected_index += 1;
                        }
                    }
                    Key::Left => {
                        // Go up one directory
                        if let Some(parent) = current_path.parent() {
                            let parent = parent.to_path_buf();
                            self.fortress_navigate_to(&parent);
                        }
                    }
                    Key::Right => {
                        // Enter selected directory (same as Enter for dirs)
                        if let Some((orig_idx, _)) = filtered.get(*selected_index) {
                            let entry = entries[*orig_idx].clone();
                            if entry.is_dir {
                                self.fortress_navigate_to(&entry.path);
                            }
                        }
                    }
                    Key::PageUp => {
                        *selected_index = selected_index.saturating_sub(10);
                        *scroll_offset = scroll_offset.saturating_sub(10);
                    }
                    Key::PageDown => {
                        let max = filtered.len().saturating_sub(1);
                        *selected_index = (*selected_index + 10).min(max);
                    }
                    Key::Home => {
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    Key::End => {
                        if !filtered.is_empty() {
                            *selected_index = filtered.len() - 1;
                        }
                    }
                    Key::Char(c) => {
                        filter.push(c);
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    _ => {}
                }
            }
            PromptState::FileSearch {
                ref mut query,
                ref mut results,
                ref mut selected_index,
                ref mut scroll_offset,
                searching: _,
            } => {
                match key {
                    Key::Enter => {
                        if !query.is_empty() && results.is_empty() {
                            // Trigger search - clone query first to avoid borrow conflict
                            let query_str = query.clone();
                            let new_results = self.search_files(&query_str);
                            // Re-borrow after search
                            if let PromptState::FileSearch { results, selected_index, scroll_offset, .. } = &mut self.prompt {
                                *results = new_results;
                                *selected_index = 0;
                                *scroll_offset = 0;
                            }
                        } else if !results.is_empty() {
                            // Open selected result
                            let result = results[*selected_index].clone();
                            self.prompt = PromptState::None;
                            self.file_search_open_result(&result);
                        }
                    }
                    Key::Escape => {
                        self.prompt = PromptState::None;
                        self.message = None;
                    }
                    Key::Backspace => {
                        if !query.is_empty() {
                            query.pop();
                            // Clear results when query changes
                            results.clear();
                            *selected_index = 0;
                            *scroll_offset = 0;
                        }
                    }
                    Key::Up => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                            if *selected_index < *scroll_offset {
                                *scroll_offset = *selected_index;
                            }
                        }
                    }
                    Key::Down => {
                        if *selected_index + 1 < results.len() {
                            *selected_index += 1;
                        }
                    }
                    Key::PageUp => {
                        *selected_index = selected_index.saturating_sub(10);
                        *scroll_offset = scroll_offset.saturating_sub(10);
                    }
                    Key::PageDown => {
                        let max = results.len().saturating_sub(1);
                        *selected_index = (*selected_index + 10).min(max);
                    }
                    Key::Home => {
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    Key::End => {
                        if !results.is_empty() {
                            *selected_index = results.len() - 1;
                        }
                    }
                    Key::Char(c) => {
                        query.push(c);
                        // Clear results when query changes
                        results.clear();
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    _ => {}
                }
            }
            PromptState::CommandPalette {
                ref mut query,
                ref mut filtered,
                ref mut selected_index,
                ref mut scroll_offset,
            } => {
                match key {
                    Key::Escape => {
                        self.prompt = PromptState::None;
                    }
                    Key::Enter => {
                        // Execute selected command
                        if let Some(cmd) = filtered.get(*selected_index) {
                            let cmd_id = cmd.id.to_string();
                            self.prompt = PromptState::None;
                            self.execute_command(&cmd_id);
                            self.scroll_to_cursor(); // Ensure viewport follows cursor after command
                        } else {
                            self.prompt = PromptState::None;
                        }
                    }
                    Key::Up => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                            if *selected_index < *scroll_offset {
                                *scroll_offset = *selected_index;
                            }
                        }
                    }
                    Key::Down => {
                        if *selected_index + 1 < filtered.len() {
                            *selected_index += 1;
                            // Keep selected item visible (assume ~15 visible rows)
                            let visible_rows = 15;
                            if *selected_index >= *scroll_offset + visible_rows {
                                *scroll_offset = selected_index.saturating_sub(visible_rows - 1);
                            }
                        }
                    }
                    Key::PageUp => {
                        *selected_index = selected_index.saturating_sub(10);
                        if *selected_index < *scroll_offset {
                            *scroll_offset = *selected_index;
                        }
                    }
                    Key::PageDown => {
                        *selected_index = (*selected_index + 10).min(filtered.len().saturating_sub(1));
                        let visible_rows = 15;
                        if *selected_index >= *scroll_offset + visible_rows {
                            *scroll_offset = selected_index.saturating_sub(visible_rows - 1);
                        }
                    }
                    Key::Backspace => {
                        if !query.is_empty() {
                            query.pop();
                            *filtered = filter_commands(query);
                            *selected_index = 0;
                            *scroll_offset = 0;
                        }
                    }
                    Key::Char(c) => {
                        query.push(c);
                        *filtered = filter_commands(query);
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    _ => {}
                }
            }
            PromptState::HelpMenu {
                ref mut query,
                ref mut filtered,
                ref mut selected_index,
                ref mut scroll_offset,
                ref mut show_alt,
            } => {
                match key {
                    Key::Escape | Key::Enter => {
                        self.prompt = PromptState::None;
                    }
                    Key::Up => {
                        if *selected_index > 0 {
                            *selected_index -= 1;
                            if *selected_index < *scroll_offset {
                                *scroll_offset = *selected_index;
                            }
                        }
                    }
                    Key::Down => {
                        if *selected_index + 1 < filtered.len() {
                            *selected_index += 1;
                            let visible_rows = 18;
                            if *selected_index >= *scroll_offset + visible_rows {
                                *scroll_offset = selected_index.saturating_sub(visible_rows - 1);
                            }
                        }
                    }
                    Key::PageUp => {
                        *selected_index = selected_index.saturating_sub(10);
                        if *selected_index < *scroll_offset {
                            *scroll_offset = *selected_index;
                        }
                    }
                    Key::PageDown => {
                        *selected_index = (*selected_index + 10).min(filtered.len().saturating_sub(1));
                        let visible_rows = 18;
                        if *selected_index >= *scroll_offset + visible_rows {
                            *scroll_offset = selected_index.saturating_sub(visible_rows - 1);
                        }
                    }
                    Key::Home => {
                        *selected_index = 0;
                        *scroll_offset = 0;
                    }
                    Key::End => {
                        *selected_index = filtered.len().saturating_sub(1);
                        let visible_rows = 18;
                        *scroll_offset = selected_index.saturating_sub(visible_rows - 1);
                    }
                    Key::Backspace => {
                        if !query.is_empty() {
                            query.pop();
                            *filtered = filter_keybinds(query);
                            *selected_index = 0;
                            *scroll_offset = 0;
                        }
                    }
                    // Toggle alternate keybindings view
                    Key::Char('/') => {
                        *show_alt = !*show_alt;
                    }
                    Key::Char(c) => {
                        query.push(c);
                        *filtered = filter_keybinds(query);
                        *selected_index = 0;
                        *scroll_offset = 0;
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
            TextInputAction::GotoLine => {
                self.goto_line_col(buffer);
            }
        }
    }

    /// Open the goto line prompt
    fn open_goto_line(&mut self) {
        self.prompt = PromptState::TextInput {
            label: "Go to line: ".to_string(),
            buffer: String::new(),
            action: TextInputAction::GotoLine,
        };
        self.message = Some("Go to line: ".to_string());
    }

    /// Parse line:col input and jump to position
    fn goto_line_col(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }

        // Parse formats: "line", "line:", "line:col"
        let (line_str, col_str) = if let Some(colon_pos) = input.find(':') {
            (&input[..colon_pos], &input[colon_pos + 1..])
        } else {
            (input, "")
        };

        let line: usize = match line_str.parse::<usize>() {
            Ok(n) if n > 0 => n - 1, // Convert to 0-indexed
            Ok(_) => {
                self.message = Some("Invalid line number".to_string());
                return;
            }
            Err(_) => {
                self.message = Some("Invalid line number".to_string());
                return;
            }
        };

        let col: usize = if col_str.is_empty() {
            0
        } else {
            match col_str.parse::<usize>() {
                Ok(n) if n > 0 => n - 1, // Convert to 0-indexed
                Ok(_) => 0,
                Err(_) => 0,
            }
        };

        // Clamp to buffer bounds
        let line_count = self.buffer().line_count();
        let line = line.min(line_count.saturating_sub(1));
        let line_len = self.buffer().line_len(line);
        let col = col.min(line_len);

        // Move cursor
        self.cursor_mut().line = line;
        self.cursor_mut().col = col;
        self.cursor_mut().desired_col = col;
        self.cursor_mut().clear_selection();

        // Center the view on the target line
        self.scroll_to_cursor();

        self.message = Some(format!("Line {}, Column {}", line + 1, col + 1));
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

    // ========== Find/Replace Methods ==========

    /// Open the find dialog (or toggle if already open on find field)
    fn open_find(&mut self) {
        match &self.prompt {
            PromptState::FindReplace { active_field: FindReplaceField::Find, .. } => {
                // Already in find mode with find field active - close
                self.prompt = PromptState::None;
                self.search_state.matches.clear();
            }
            PromptState::FindReplace { find_query, replace_text, case_insensitive, regex_mode, .. } => {
                // In find/replace but on replace field - switch to find
                self.prompt = PromptState::FindReplace {
                    find_query: find_query.clone(),
                    replace_text: replace_text.clone(),
                    active_field: FindReplaceField::Find,
                    case_insensitive: *case_insensitive,
                    regex_mode: *regex_mode,
                };
            }
            _ => {
                // Open fresh find dialog, possibly with selected text
                let initial_query = self.get_selection_text().unwrap_or_default();
                self.prompt = PromptState::FindReplace {
                    find_query: initial_query,
                    replace_text: String::new(),
                    active_field: FindReplaceField::Find,
                    case_insensitive: false,
                    regex_mode: false,
                };
                self.update_search_matches();
            }
        }
    }

    /// Open the replace dialog (or switch to replace field, or close if already on replace)
    fn open_replace(&mut self) {
        match &self.prompt {
            PromptState::FindReplace { active_field: FindReplaceField::Replace, .. } => {
                // Already in replace mode with replace field active - close
                self.prompt = PromptState::None;
                self.search_state.matches.clear();
            }
            PromptState::FindReplace { find_query, replace_text, case_insensitive, regex_mode, .. } => {
                // In find/replace but on find field - switch to replace
                self.prompt = PromptState::FindReplace {
                    find_query: find_query.clone(),
                    replace_text: replace_text.clone(),
                    active_field: FindReplaceField::Replace,
                    case_insensitive: *case_insensitive,
                    regex_mode: *regex_mode,
                };
            }
            _ => {
                // Open find/replace with replace field active
                let initial_query = self.get_selection_text().unwrap_or_default();
                self.prompt = PromptState::FindReplace {
                    find_query: initial_query,
                    replace_text: String::new(),
                    active_field: FindReplaceField::Replace,
                    case_insensitive: false,
                    regex_mode: false,
                };
                self.update_search_matches();
            }
        }
    }

    /// Update search matches based on current query
    fn update_search_matches(&mut self) {
        let (query, case_insensitive, regex_mode) = match &self.prompt {
            PromptState::FindReplace { find_query, case_insensitive, regex_mode, .. } => {
                (find_query.clone(), *case_insensitive, *regex_mode)
            }
            _ => return,
        };

        // Check if we need to update (query or settings changed)
        if query == self.search_state.last_query
            && case_insensitive == self.search_state.last_case_insensitive
            && regex_mode == self.search_state.last_regex
        {
            return;
        }

        self.search_state.last_query = query.clone();
        self.search_state.last_case_insensitive = case_insensitive;
        self.search_state.last_regex = regex_mode;
        self.search_state.matches.clear();
        self.search_state.current_match = 0;

        if query.is_empty() {
            return;
        }

        // Collect all lines from buffer first to avoid borrow issues
        let buffer = self.buffer();
        let line_count = buffer.line_count();
        let lines: Vec<String> = (0..line_count)
            .filter_map(|i| buffer.line_str(i))
            .collect();

        // Now search through the collected lines
        let mut matches = Vec::new();

        if regex_mode {
            // Regex search
            let pattern = if case_insensitive {
                format!("(?i){}", query)
            } else {
                query.clone()
            };

            if let Ok(re) = regex::Regex::new(&pattern) {
                for (line_idx, line) in lines.iter().enumerate() {
                    for mat in re.find_iter(line) {
                        // Convert byte positions to char positions for proper cursor placement
                        let start_col = line[..mat.start()].chars().count();
                        let match_char_len = line[mat.start()..mat.end()].chars().count();
                        matches.push(SearchMatch {
                            line: line_idx,
                            start_col,
                            end_col: start_col + match_char_len,
                        });
                    }
                }
            }
        } else {
            // Plain text search - optimized version using str::find()
            let search_query = if case_insensitive {
                query.to_lowercase()
            } else {
                query.clone()
            };
            let query_char_len = query.chars().count();

            // Reusable buffer for case-insensitive search to avoid allocations
            let mut lowered_line = String::new();

            for (line_idx, line) in lines.iter().enumerate() {
                // Get the search line (reuse buffer for case-insensitive)
                let search_line: &str = if case_insensitive {
                    lowered_line.clear();
                    for c in line.chars() {
                        for lc in c.to_lowercase() {
                            lowered_line.push(lc);
                        }
                    }
                    &lowered_line
                } else {
                    line
                };

                // Use str::find() which is SIMD-optimized for byte search
                // Then convert byte positions to char positions
                let mut byte_offset = 0;
                while let Some(byte_pos) = search_line[byte_offset..].find(&search_query) {
                    let abs_byte_pos = byte_offset + byte_pos;

                    // Convert byte position to char position
                    let start_col = search_line[..abs_byte_pos].chars().count();

                    matches.push(SearchMatch {
                        line: line_idx,
                        start_col,
                        end_col: start_col + query_char_len,
                    });

                    // Move past this match (by at least one byte, or query length)
                    byte_offset = abs_byte_pos + search_query.len().max(1);
                    if byte_offset >= search_line.len() {
                        break;
                    }
                }
            }
        }

        self.search_state.matches = matches;

        // Find the match closest to current cursor position
        if !self.search_state.matches.is_empty() {
            let cursor = self.cursors().primary();
            let cursor_pos = (cursor.line, cursor.col);

            // Find first match at or after cursor
            let mut best_idx = 0;
            for (i, m) in self.search_state.matches.iter().enumerate() {
                if (m.line, m.start_col) >= cursor_pos {
                    best_idx = i;
                    break;
                }
                best_idx = i;
            }
            self.search_state.current_match = best_idx;
        }
    }

    /// Find and jump to next match
    fn find_next(&mut self) {
        self.update_search_matches();

        if self.search_state.matches.is_empty() {
            self.message = Some("No matches found".to_string());
            return;
        }

        // Move to next match (wrap around)
        self.search_state.current_match =
            (self.search_state.current_match + 1) % self.search_state.matches.len();

        self.jump_to_current_match();
    }

    /// Find and jump to previous match
    fn find_prev(&mut self) {
        self.update_search_matches();

        if self.search_state.matches.is_empty() {
            self.message = Some("No matches found".to_string());
            return;
        }

        // Move to previous match (wrap around)
        if self.search_state.current_match == 0 {
            self.search_state.current_match = self.search_state.matches.len() - 1;
        } else {
            self.search_state.current_match -= 1;
        }

        self.jump_to_current_match();
    }

    /// Jump cursor to the current match and select it
    fn jump_to_current_match(&mut self) {
        if let Some(m) = self.search_state.matches.get(self.search_state.current_match).cloned() {
            // Collapse to primary cursor and move it to the match
            self.cursors_mut().collapse_to_primary();
            let cursor = self.cursors_mut().primary_mut();

            // Move cursor to start of match with selection extending to end
            cursor.line = m.start_col.min(m.end_col); // Cursor at start
            cursor.col = m.start_col;
            cursor.line = m.line;

            // Set up selection from end to start (so cursor is at start, anchor at end)
            cursor.anchor_line = m.line;
            cursor.anchor_col = m.end_col;
            cursor.selecting = true;

            // Scroll to make match visible
            self.scroll_to_cursor();

            // Update message with match count
            let total = self.search_state.matches.len();
            let current = self.search_state.current_match + 1;
            self.message = Some(format!("{}/{} matches", current, total));
        }
    }

    /// Replace current match and find next
    fn replace_current(&mut self) {
        let replace_text = match &self.prompt {
            PromptState::FindReplace { replace_text, .. } => replace_text.clone(),
            _ => return,
        };

        if self.search_state.matches.is_empty() {
            self.message = Some("No matches to replace".to_string());
            return;
        }

        // Get current match
        let current_idx = self.search_state.current_match;
        if let Some(m) = self.search_state.matches.get(current_idx).cloned() {
            // Delete the matched text and insert replacement
            let buffer = self.buffer_mut();
            let start_char = buffer.line_col_to_char(m.line, m.start_col);
            let end_char = buffer.line_col_to_char(m.line, m.end_col);
            buffer.delete(start_char, end_char);
            buffer.insert(start_char, &replace_text);

            // Re-run search to update matches
            self.search_state.last_query.clear(); // Force re-search
            self.update_search_matches();

            // Jump to next (or stay at same index if there are still matches)
            if !self.search_state.matches.is_empty() {
                // Keep index in bounds
                if self.search_state.current_match >= self.search_state.matches.len() {
                    self.search_state.current_match = 0;
                }
                self.jump_to_current_match();
            } else {
                self.message = Some("All matches replaced".to_string());
            }
        }
    }

    /// Replace all matches
    fn replace_all(&mut self) {
        let replace_text = match &self.prompt {
            PromptState::FindReplace { replace_text, .. } => replace_text.clone(),
            _ => return,
        };

        self.update_search_matches();

        if self.search_state.matches.is_empty() {
            self.message = Some("No matches to replace".to_string());
            return;
        }

        let count = self.search_state.matches.len();

        // Replace from end to start to preserve positions
        let matches: Vec<_> = self.search_state.matches.iter().cloned().collect();
        for m in matches.into_iter().rev() {
            let buffer = self.buffer_mut();
            let start_char = buffer.line_col_to_char(m.line, m.start_col);
            let end_char = buffer.line_col_to_char(m.line, m.end_col);
            buffer.delete(start_char, end_char);
            buffer.insert(start_char, &replace_text);
        }

        self.search_state.matches.clear();
        self.search_state.last_query.clear();
        self.message = Some(format!("Replaced {} occurrences", count));
    }

    /// Toggle case sensitivity
    fn toggle_case_sensitivity(&mut self) {
        if let PromptState::FindReplace { find_query, replace_text, active_field, case_insensitive, regex_mode } = &self.prompt {
            self.prompt = PromptState::FindReplace {
                find_query: find_query.clone(),
                replace_text: replace_text.clone(),
                active_field: *active_field,
                case_insensitive: !*case_insensitive,
                regex_mode: *regex_mode,
            };
            self.search_state.last_query.clear(); // Force re-search
            self.update_search_matches();
        }
    }

    /// Toggle regex mode
    fn toggle_regex_mode(&mut self) {
        if let PromptState::FindReplace { find_query, replace_text, active_field, case_insensitive, regex_mode } = &self.prompt {
            self.prompt = PromptState::FindReplace {
                find_query: find_query.clone(),
                replace_text: replace_text.clone(),
                active_field: *active_field,
                case_insensitive: *case_insensitive,
                regex_mode: !*regex_mode,
            };
            self.search_state.last_query.clear(); // Force re-search
            self.update_search_matches();
        }
    }

    // === Fortress mode (file browser) ===

    /// Open fortress mode file browser
    fn open_fortress(&mut self) {
        // Start at current file's directory, or workspace root
        let start_path = if let Some(path) = self.current_file_path() {
            if let Some(parent) = path.parent() {
                parent.to_path_buf()
            } else {
                self.workspace.root.clone()
            }
        } else {
            self.workspace.root.clone()
        };

        let entries = self.read_directory(&start_path);
        self.prompt = PromptState::Fortress {
            current_path: start_path,
            entries,
            selected_index: 0,
            filter: String::new(),
            scroll_offset: 0,
        };
    }

    /// Read directory contents and return sorted entries (dirs first, then files)
    fn read_directory(&self, path: &Path) -> Vec<FortressEntry> {
        let mut entries = Vec::new();

        // Add parent directory entry if not at root
        if path.parent().is_some() {
            entries.push(FortressEntry {
                name: "..".to_string(),
                path: path.parent().unwrap().to_path_buf(),
                is_dir: true,
            });
        }

        if let Ok(read_dir) = std::fs::read_dir(path) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in read_dir.flatten() {
                let entry_path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files (starting with .)
                if name.starts_with('.') {
                    continue;
                }

                let is_dir = entry_path.is_dir();
                let fortress_entry = FortressEntry {
                    name,
                    path: entry_path,
                    is_dir,
                };

                if is_dir {
                    dirs.push(fortress_entry);
                } else {
                    files.push(fortress_entry);
                }
            }

            // Sort alphabetically (case-insensitive)
            dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

            // Directories first, then files
            entries.extend(dirs);
            entries.extend(files);
        }

        entries
    }

    /// Navigate to a new directory in fortress mode
    fn fortress_navigate_to(&mut self, path: &Path) {
        let entries = self.read_directory(path);
        self.prompt = PromptState::Fortress {
            current_path: path.to_path_buf(),
            entries,
            selected_index: 0,
            filter: String::new(),
            scroll_offset: 0,
        };
    }

    /// Open a file from fortress mode
    fn fortress_open_file(&mut self, path: &Path) {
        // Open file in current pane by reusing workspace method
        if let Err(e) = self.workspace.open_file(path) {
            self.message = Some(format!("Failed to open file: {}", e));
        } else {
            // Sync with LSP
            self.sync_document_to_lsp();
        }
    }

    /// Open multi-file search modal (F4)
    fn open_file_search(&mut self) {
        self.prompt = PromptState::FileSearch {
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            searching: false,
        };
    }

    /// Search files in workspace for query string (grep-like)
    /// Uses streaming file reading to avoid loading entire files into memory
    fn search_files(&self, query: &str) -> Vec<FileSearchResult> {
        use std::io::{BufRead, BufReader};
        use std::fs::File;

        if query.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();
        let root = &self.workspace.root;
        let query_lower = query.to_lowercase();

        // Walk directory tree
        fn walk_dir(
            dir: &Path,
            query_lower: &str,
            results: &mut Vec<FileSearchResult>,
            root: &Path,
        ) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };

            for entry in entries.flatten() {
                // Check result limit early to avoid unnecessary work
                if results.len() >= 500 {
                    return;
                }

                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Skip hidden files/dirs
                if name.starts_with('.') {
                    continue;
                }

                // Skip common non-text directories
                if path.is_dir() {
                    if matches!(name, "target" | "node_modules" | "build" | "dist" | "__pycache__") {
                        continue;
                    }
                    walk_dir(&path, query_lower, results, root);
                } else if path.is_file() {
                    // Skip binary/large files by extension
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "ico" | "woff" | "woff2" | "ttf" | "eot" | "pdf" | "zip" | "tar" | "gz" | "exe" | "dll" | "so" | "dylib" | "o" | "a" | "rlib") {
                        continue;
                    }

                    // Stream file line-by-line instead of loading entire file
                    let Ok(file) = File::open(&path) else {
                        continue;
                    };
                    let reader = BufReader::new(file);
                    let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

                    // Reusable buffer for lowercasing
                    let mut line_lower = String::new();

                    for (line_idx, line_result) in reader.lines().enumerate() {
                        // Check result limit
                        if results.len() >= 500 {
                            return;
                        }

                        let Ok(line) = line_result else {
                            // Non-UTF8 content - likely binary, skip file
                            break;
                        };

                        // Reuse buffer for lowercase conversion
                        line_lower.clear();
                        for c in line.chars() {
                            for lc in c.to_lowercase() {
                                line_lower.push(lc);
                            }
                        }

                        if line_lower.contains(query_lower) {
                            results.push(FileSearchResult {
                                path: rel_path.clone(),
                                line_num: line_idx + 1,
                                line_content: line.trim().to_string(),
                            });
                        }
                    }
                }
            }
        }

        walk_dir(root, &query_lower, &mut results, root);
        results
    }

    /// Open file at the location from a file search result
    fn file_search_open_result(&mut self, result: &FileSearchResult) {
        let full_path = self.workspace.root.join(&result.path);

        if let Err(e) = self.workspace.open_file(&full_path) {
            self.message = Some(format!("Failed to open file: {}", e));
            return;
        }

        // Sync with LSP
        self.sync_document_to_lsp();

        // Go to line
        let line = result.line_num.saturating_sub(1); // Convert to 0-indexed
        let tab = self.workspace.active_tab_mut();
        let max_line = tab.active_buffer().buffer.line_count().saturating_sub(1);
        let target_line = line.min(max_line);

        let pane = tab.active_pane_mut();
        pane.cursors.primary_mut().line = target_line;
        pane.cursors.primary_mut().col = 0;

        // Center the line in viewport
        let viewport_height = self.screen.rows.saturating_sub(2) as usize;
        pane.viewport_line = target_line.saturating_sub(viewport_height / 2);
    }

    // === Command Palette ===

    /// Open the command palette
    fn open_command_palette(&mut self) {
        let filtered = filter_commands("");
        self.prompt = PromptState::CommandPalette {
            query: String::new(),
            filtered,
            selected_index: 0,
            scroll_offset: 0,
        };
    }

    /// Execute a command by its ID
    fn execute_command(&mut self, command_id: &str) {
        match command_id {
            // File operations
            "save" => { let _ = self.save(); }
            "save-all" => { let _ = self.workspace.save_all(); }
            "open" => self.open_fortress(),
            "new-tab" => self.workspace.new_tab(),
            "close-tab" => self.close_pane(), // Close current pane/tab
            "next-tab" => self.workspace.next_tab(),
            "prev-tab" => self.workspace.prev_tab(),
            "quit" => self.try_quit(),

            // Edit operations
            "undo" => self.undo(),
            "redo" => self.redo(),
            "cut" => self.cut(),
            "copy" => self.copy(),
            "paste" => self.paste(),
            "select-all" => {
                // Select all text in current buffer
                let line_count = self.buffer().line_count();
                let last_line = line_count.saturating_sub(1);
                let last_col = self.buffer().line_len(last_line);
                self.cursor_mut().anchor_line = 0;
                self.cursor_mut().anchor_col = 0;
                self.cursor_mut().line = last_line;
                self.cursor_mut().col = last_col;
                self.cursor_mut().selecting = true;
            }
            "select-line" => self.select_line(),
            "select-word" => self.select_word(),
            "toggle-comment" => self.toggle_line_comment(),
            "join-lines" => self.join_lines(),
            "duplicate-line" => self.duplicate_line_down(),
            "move-line-up" => self.move_line_up(),
            "move-line-down" => self.move_line_down(),
            "delete-line" => {
                // Delete the current line
                let line = self.cursor().line;
                let line_count = self.buffer().line_count();
                let line_start = self.buffer().line_col_to_char(line, 0);
                let line_end = if line + 1 < line_count {
                    self.buffer().line_col_to_char(line + 1, 0)
                } else {
                    self.buffer().len_chars()
                };
                if line_start < line_end {
                    self.buffer_mut().delete(line_start, line_end);
                    self.cursor_mut().col = 0;
                    self.cursor_mut().desired_col = 0;
                    // Clamp line if we deleted the last line
                    let new_line_count = self.buffer().line_count();
                    if self.cursor().line >= new_line_count {
                        self.cursor_mut().line = new_line_count.saturating_sub(1);
                    }
                }
            }
            "indent" => self.insert_tab(),
            "outdent" => self.dedent(),
            "transpose" => self.transpose_chars(),

            // Search operations
            "find" => self.open_find(),
            "replace" => self.open_replace(),
            "find-next" => self.find_next(),
            "find-prev" => self.find_prev(),
            "search-files" => self.open_file_search(),

            // Navigation
            "goto-line" => self.open_goto_line(),
            "goto-start" => {
                self.cursor_mut().line = 0;
                self.cursor_mut().col = 0;
                self.cursor_mut().desired_col = 0;
                self.cursor_mut().clear_selection();
            }
            "goto-end" => {
                let last_line = self.buffer().line_count().saturating_sub(1);
                let last_col = self.buffer().line_len(last_line);
                self.cursor_mut().line = last_line;
                self.cursor_mut().col = last_col;
                self.cursor_mut().desired_col = last_col;
                self.cursor_mut().clear_selection();
            }
            "goto-bracket" => self.jump_to_matching_bracket(),
            "page-up" => self.page_up(false),
            "page-down" => self.page_down(false),

            // Selection
            "select-brackets" => self.jump_to_matching_bracket(), // TODO: implement select inside brackets
            "cursor-above" => self.add_cursor_above(),
            "cursor-below" => self.add_cursor_below(),

            // View / Panes
            "split-vertical" => self.split_vertical(),
            "split-horizontal" => self.split_horizontal(),
            "close-pane" => self.close_pane(),
            "next-pane" => self.tab_mut().navigate_pane(PaneDirection::Right),
            "prev-pane" => self.tab_mut().navigate_pane(PaneDirection::Left),
            "toggle-explorer" => self.workspace.fuss.toggle(),

            // LSP operations
            "goto-definition" => self.lsp_goto_definition(),
            "find-references" => self.lsp_find_references(),
            "rename" => self.lsp_rename(),
            "hover" => self.lsp_hover(),
            "completion" => self.filter_completions(),
            "server-manager" => self.toggle_server_manager(),

            // Bracket/Quote operations
            "jump-bracket" => self.jump_to_matching_bracket(),
            "cycle-brackets" => self.cycle_brackets(),
            "remove-surrounding" => self.remove_surrounding(),

            // Help
            "command-palette" => {} // Already open
            "help" => self.open_help_menu(),

            _ => {
                self.message = Some(format!("Unknown command: {}", command_id));
            }
        }
    }

    // === Help Menu ===

    /// Open the help menu with keybindings
    fn open_help_menu(&mut self) {
        let filtered = filter_keybinds("");
        self.prompt = PromptState::HelpMenu {
            query: String::new(),
            filtered,
            selected_index: 0,
            scroll_offset: 0,
            show_alt: false,
        };
    }
}

/// Fuzzy match scoring for command palette
fn fuzzy_match_score(text: &str, pattern: &str) -> i32 {
    if pattern.is_empty() {
        return 100; // Empty pattern matches everything with base score
    }

    let text_lower = text.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    let mut score = 0i32;
    let mut pattern_idx = 0;
    let mut consecutive = 0;
    let pattern_chars: Vec<char> = pattern_lower.chars().collect();
    let text_chars: Vec<char> = text_lower.chars().collect();

    if pattern_chars.is_empty() {
        return 100;
    }

    for (i, &tc) in text_chars.iter().enumerate() {
        if pattern_idx >= pattern_chars.len() {
            break;
        }

        if tc == pattern_chars[pattern_idx] {
            score += 10;
            consecutive += 1;

            // Bonus for consecutive matches
            if consecutive > 1 {
                score += 5;
            }

            // Bonus for matching at start or after space/separator
            if i == 0 || matches!(text_chars.get(i.wrapping_sub(1)), Some(' ' | ':' | '-' | '_' | '/')) {
                score += 15;
            }

            pattern_idx += 1;
        } else {
            consecutive = 0;
        }
    }

    // Only return positive score if all pattern characters matched
    if pattern_idx == pattern_chars.len() {
        score
    } else {
        0
    }
}

/// Filter and sort commands by fuzzy match score
fn filter_commands(query: &str) -> Vec<PaletteCommand> {
    let mut filtered: Vec<PaletteCommand> = ALL_COMMANDS
        .iter()
        .filter_map(|cmd| {
            // Match against name, category, or command ID
            let name_score = fuzzy_match_score(cmd.name, query);
            let category_score = fuzzy_match_score(cmd.category, query) / 2; // Category match worth less
            let id_score = fuzzy_match_score(cmd.id, query) / 2;

            let score = name_score.max(category_score).max(id_score);
            if score > 0 {
                let mut cmd = cmd.clone();
                cmd.score = score;
                Some(cmd)
            } else {
                None
            }
        })
        .collect();

    // Sort by score descending
    filtered.sort_by(|a, b| b.score.cmp(&a.score));
    filtered
}

/// Filter keybinds by fuzzy match (for help menu)
fn filter_keybinds(query: &str) -> Vec<HelpKeybind> {
    if query.is_empty() {
        // Return all keybinds in original order (grouped by category)
        return ALL_KEYBINDS.to_vec();
    }

    let mut filtered: Vec<(HelpKeybind, i32)> = ALL_KEYBINDS
        .iter()
        .filter_map(|kb| {
            // Match against shortcut, description, or category
            let shortcut_score = fuzzy_match_score(kb.shortcut, query);
            let desc_score = fuzzy_match_score(kb.description, query);
            let category_score = fuzzy_match_score(kb.category, query) / 2;

            let score = shortcut_score.max(desc_score).max(category_score);
            if score > 0 {
                Some((kb.clone(), score))
            } else {
                None
            }
        })
        .collect();

    // Sort by score descending
    filtered.sort_by(|a, b| b.1.cmp(&a.1));
    filtered.into_iter().map(|(kb, _)| kb).collect()
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
