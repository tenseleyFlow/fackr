//! Workspace state management
//!
//! The Workspace is the defining unit of fackr. Every editing session
//! operates within a workspace context.

#![allow(dead_code)]

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::buffer::Buffer;
use crate::editor::{Cursors, History};

/// Normalized pane bounds (0.0 to 1.0)
/// Converted to screen coordinates at render time
#[derive(Debug, Clone)]
pub struct PaneBounds {
    pub x_start: f32,
    pub y_start: f32,
    pub x_end: f32,
    pub y_end: f32,
}

impl Default for PaneBounds {
    fn default() -> Self {
        Self {
            x_start: 0.0,
            y_start: 0.0,
            x_end: 1.0,
            y_end: 1.0,
        }
    }
}

/// A pane is a view into a buffer with its own cursor and viewport
#[derive(Debug)]
pub struct Pane {
    /// Cursor positions within this pane
    pub cursors: Cursors,
    /// First visible line
    pub viewport_line: usize,
    /// First visible column (for horizontal scrolling)
    pub viewport_col: usize,
    /// Normalized bounds within the tab area
    pub bounds: PaneBounds,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            cursors: Cursors::new(),
            viewport_line: 0,
            viewport_col: 0,
            bounds: PaneBounds::default(),
        }
    }
}

impl Pane {
    pub fn new() -> Self {
        Self::default()
    }
}

/// A tab represents an open file (or unsaved buffer)
#[derive(Debug)]
pub struct Tab {
    /// File path (relative to workspace for workspace files, absolute for orphans)
    /// None means unsaved new file
    pub path: Option<PathBuf>,
    /// The text buffer
    pub buffer: Buffer,
    /// Undo/redo history for this tab
    pub history: History,
    /// Views into this buffer
    pub panes: Vec<Pane>,
    /// Which pane is active (index into panes)
    pub active_pane: usize,
    /// File is outside workspace directory
    pub is_orphan: bool,
}

impl Tab {
    /// Create a new empty tab
    pub fn new() -> Self {
        Self {
            path: None,
            buffer: Buffer::new(),
            history: History::new(),
            panes: vec![Pane::new()],
            active_pane: 0,
            is_orphan: false,
        }
    }

    /// Create a tab from a file
    pub fn from_file(path: &Path, workspace_root: &Path) -> Result<Self> {
        let buffer = Buffer::load(path)?;
        let is_orphan = !path.starts_with(workspace_root);

        // Store relative path for workspace files, absolute for orphans
        let stored_path = if is_orphan {
            path.to_path_buf()
        } else {
            path.strip_prefix(workspace_root)
                .unwrap_or(path)
                .to_path_buf()
        };

        Ok(Self {
            path: Some(stored_path),
            buffer,
            history: History::new(),
            panes: vec![Pane::new()],
            active_pane: 0,
            is_orphan,
        })
    }

    /// Get the display name for the tab bar
    pub fn display_name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("[unknown]")
                .to_string(),
            None => "[new]".to_string(),
        }
    }

    /// Check if buffer has been modified
    pub fn is_modified(&self) -> bool {
        self.buffer.modified
    }

    /// Get the active pane
    pub fn active_pane(&self) -> &Pane {
        &self.panes[self.active_pane]
    }

    /// Get mutable reference to active pane
    pub fn active_pane_mut(&mut self) -> &mut Pane {
        &mut self.panes[self.active_pane]
    }
}

impl Default for Tab {
    fn default() -> Self {
        Self::new()
    }
}

/// Fuss mode state (file tree sidebar)
#[derive(Debug, Default)]
pub struct FussState {
    /// Whether fuss mode is active
    pub active: bool,
    /// Width as percentage of screen (default 30%)
    pub width_percent: u8,
    /// Currently selected item index
    pub selected: usize,
    /// Scroll offset in the tree
    pub scroll: usize,
    /// Expanded directories (set of paths)
    pub expanded: std::collections::HashSet<PathBuf>,
}

impl FussState {
    pub fn new() -> Self {
        Self {
            active: false,
            width_percent: 30,
            selected: 0,
            scroll: 0,
            expanded: std::collections::HashSet::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
    }
}

/// Workspace configuration
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    /// Tab width in spaces
    pub tab_width: usize,
    /// Use spaces instead of tabs
    pub use_spaces: bool,
    // Add more config options as needed
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            use_spaces: true,
        }
    }
}

/// The Workspace - defining unit of fackr
///
/// Every editing session operates within a workspace context.
/// A workspace is tied to a directory and persists state in .fackr/
#[derive(Debug)]
pub struct Workspace {
    /// Root directory of the workspace
    pub root: PathBuf,
    /// All open tabs
    pub tabs: Vec<Tab>,
    /// Currently active tab index
    pub active_tab: usize,
    /// Fuss mode (file tree) state
    pub fuss: FussState,
    /// Workspace configuration
    pub config: WorkspaceConfig,
}

impl Workspace {
    /// Create a new workspace for a directory
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            tabs: vec![Tab::new()],
            active_tab: 0,
            fuss: FussState::new(),
            config: WorkspaceConfig::default(),
        }
    }

    /// Initialize workspace directory structure (.fackr/)
    pub fn init(&self) -> Result<()> {
        let fackr_dir = self.root.join(".fackr");
        if !fackr_dir.exists() {
            std::fs::create_dir_all(&fackr_dir)?;
            std::fs::create_dir_all(fackr_dir.join("backups"))?;
        }
        Ok(())
    }

    /// Check if a directory has an existing workspace
    pub fn exists(dir: &Path) -> bool {
        dir.join(".fackr").join("workspace.json").exists()
    }

    /// Detect workspace from a file path (searches parent directories)
    pub fn detect_from_file(file_path: &Path) -> Option<PathBuf> {
        let mut current = file_path.parent()?;
        loop {
            if Self::exists(current) {
                return Some(current.to_path_buf());
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => return None,
            }
        }
    }

    /// Open a workspace, creating .fackr/ if needed
    pub fn open(root: PathBuf) -> Result<Self> {
        let mut workspace = Self::new(root);
        workspace.init()?;

        // Try to load existing state
        if let Err(_e) = workspace.load() {
            // No existing state or failed to load - start fresh
            // (workspace already has default empty tab)
        }

        Ok(workspace)
    }

    /// Open a workspace with a specific file
    pub fn open_with_file(file_path: &Path) -> Result<Self> {
        // Determine workspace root
        let root = Self::detect_from_file(file_path)
            .or_else(|| file_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let mut workspace = Self::open(root)?;

        // Open the file in a tab
        if file_path.exists() {
            workspace.open_file(file_path)?;
        }

        Ok(workspace)
    }

    /// Load workspace state from .fackr/workspace.json
    pub fn load(&mut self) -> Result<()> {
        let state_path = self.root.join(".fackr").join("workspace.json");
        if !state_path.exists() {
            return Ok(());
        }

        // TODO: Implement JSON deserialization
        // For now, just return Ok - we'll add full persistence later
        Ok(())
    }

    /// Save workspace state to .fackr/workspace.json
    pub fn save(&self) -> Result<()> {
        self.init()?; // Ensure .fackr/ exists

        let _state_path = self.root.join(".fackr").join("workspace.json");

        // TODO: Implement JSON serialization
        // For now, just return Ok - we'll add full persistence later
        Ok(())
    }

    /// Get the active tab
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// Get mutable reference to active tab
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    /// Open a file in a new tab
    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        // Check if file is already open
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(tab_path) = &tab.path {
                let full_path = if tab.is_orphan {
                    tab_path.clone()
                } else {
                    self.root.join(tab_path)
                };
                if full_path.canonicalize().ok() == Some(abs_path.clone()) {
                    // File already open - switch to it
                    self.active_tab = i;
                    return Ok(());
                }
            }
        }

        // Open new tab
        let tab = Tab::from_file(path, &self.root)?;
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        Ok(())
    }

    /// Create a new empty tab
    pub fn new_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.active_tab = self.tabs.len() - 1;
    }

    /// Close the active tab
    /// Returns true if the workspace should close (no tabs left)
    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return true; // Last tab - workspace should close
        }

        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        false
    }

    /// Switch to tab by index (0-based)
    pub fn switch_to_tab(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    /// Switch to next tab (wraps around)
    pub fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
    }

    /// Switch to previous tab (wraps around)
    pub fn prev_tab(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = self.tabs.len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }

    /// Get number of tabs
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}
