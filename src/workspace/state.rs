//! Workspace state management
//!
//! The Workspace is the defining unit of fackr. Every editing session
//! operates within a workspace context.

#![allow(dead_code)]

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::buffer::Buffer;
use crate::editor::{Cursors, History};
use crate::fuss::FussMode;

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

/// A buffer entry in a tab (file content with its undo history)
#[derive(Debug)]
pub struct BufferEntry {
    /// File path (relative to workspace for workspace files, absolute for orphans)
    /// None means unsaved new file
    pub path: Option<PathBuf>,
    /// The text buffer
    pub buffer: Buffer,
    /// Undo/redo history for this buffer
    pub history: History,
    /// File is outside workspace directory
    pub is_orphan: bool,
    /// Hash of buffer content at last save (None for new unsaved buffers)
    saved_hash: Option<u64>,
    /// Length of buffer at last save (sentinel for quick modified check)
    saved_len: Option<usize>,
}

impl BufferEntry {
    pub fn new() -> Self {
        let buffer = Buffer::new();
        let saved_hash = Some(buffer.content_hash()); // Empty buffer is "saved"
        let saved_len = Some(buffer.len_chars());
        Self {
            path: None,
            buffer,
            history: History::new(),
            is_orphan: false,
            saved_hash,
            saved_len,
        }
    }

    /// Create a buffer from string content (for diff views, etc.)
    /// The buffer is considered "saved" so it won't prompt for save on close
    pub fn from_content(content: &str, display_name: Option<&str>) -> Self {
        let buffer = Buffer::from_str(content);
        let saved_hash = Some(buffer.content_hash());
        let saved_len = Some(buffer.len_chars());
        Self {
            path: display_name.map(PathBuf::from),
            buffer,
            history: History::new(),
            is_orphan: true, // Mark as orphan so path isn't prefixed with workspace root
            saved_hash,
            saved_len,
        }
    }

    pub fn from_file(path: &Path, workspace_root: &Path) -> Result<Self> {
        let buffer = Buffer::load(path)?;
        let saved_hash = Some(buffer.content_hash()); // Hash at load time
        let saved_len = Some(buffer.len_chars());
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
            is_orphan,
            saved_hash,
            saved_len,
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

    /// Check if buffer has been modified since last save
    pub fn is_modified(&self) -> bool {
        match (self.saved_hash, self.saved_len) {
            (Some(hash), Some(len)) => {
                // Quick check: if length differs, definitely modified
                if self.buffer.len_chars() != len {
                    return true;
                }
                // Length matches - need to check content hash
                self.buffer.content_hash() != hash
            },
            _ => true, // No saved state means never saved
        }
    }

    /// Mark the buffer as saved (updates hash and length for change detection)
    pub fn mark_saved(&mut self) {
        self.saved_hash = Some(self.buffer.content_hash());
        self.saved_len = Some(self.buffer.len_chars());
    }
}

impl Default for BufferEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// A pane is a view into a buffer with its own cursor and viewport
#[derive(Debug)]
pub struct Pane {
    /// Index into the tab's buffers vector
    pub buffer_idx: usize,
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
            buffer_idx: 0,
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

    pub fn with_buffer_idx(buffer_idx: usize) -> Self {
        Self {
            buffer_idx,
            ..Default::default()
        }
    }
}

/// A tab represents a view group with one or more panes viewing buffers
#[derive(Debug)]
pub struct Tab {
    /// All buffers open in this tab (panes reference these by index)
    pub buffers: Vec<BufferEntry>,
    /// Views into buffers
    pub panes: Vec<Pane>,
    /// Which pane is active (index into panes)
    pub active_pane: usize,
}

impl Tab {
    /// Create a new empty tab
    pub fn new() -> Self {
        Self {
            buffers: vec![BufferEntry::new()],
            panes: vec![Pane::new()],
            active_pane: 0,
        }
    }

    /// Create a tab from a file
    pub fn from_file(path: &Path, workspace_root: &Path) -> Result<Self> {
        let buffer_entry = BufferEntry::from_file(path, workspace_root)?;
        Ok(Self {
            buffers: vec![buffer_entry],
            panes: vec![Pane::new()],
            active_pane: 0,
        })
    }

    /// Create a tab from string content (for diff views, etc.)
    pub fn from_content(content: &str, display_name: &str) -> Self {
        let buffer_entry = BufferEntry::from_content(content, Some(display_name));
        Self {
            buffers: vec![buffer_entry],
            panes: vec![Pane::new()],
            active_pane: 0,
        }
    }

    /// Get the display name for the tab bar (uses primary buffer's name)
    pub fn display_name(&self) -> String {
        self.buffers.first()
            .map(|b| b.display_name())
            .unwrap_or_else(|| "[new]".to_string())
    }

    /// Check if any buffer has been modified
    pub fn is_modified(&self) -> bool {
        self.buffers.iter().any(|b| b.is_modified())
    }

    /// Get the active pane
    pub fn active_pane(&self) -> &Pane {
        &self.panes[self.active_pane]
    }

    /// Get mutable reference to active pane
    pub fn active_pane_mut(&mut self) -> &mut Pane {
        &mut self.panes[self.active_pane]
    }

    /// Get the buffer for the active pane
    pub fn active_buffer(&self) -> &BufferEntry {
        let buffer_idx = self.panes[self.active_pane].buffer_idx;
        &self.buffers[buffer_idx]
    }

    /// Get mutable reference to the buffer for the active pane
    pub fn active_buffer_mut(&mut self) -> &mut BufferEntry {
        let buffer_idx = self.panes[self.active_pane].buffer_idx;
        &mut self.buffers[buffer_idx]
    }

    /// Get the buffer for a specific pane
    pub fn buffer_for_pane(&self, pane_idx: usize) -> &BufferEntry {
        let buffer_idx = self.panes[pane_idx].buffer_idx;
        &self.buffers[buffer_idx]
    }

    /// Get mutable buffer for a specific pane
    pub fn buffer_for_pane_mut(&mut self, pane_idx: usize) -> &mut BufferEntry {
        let buffer_idx = self.panes[pane_idx].buffer_idx;
        &mut self.buffers[buffer_idx]
    }

    /// Split the active pane vertically (new pane to the right, same buffer)
    pub fn split_vertical(&mut self) {
        let active = &self.panes[self.active_pane];
        let buffer_idx = active.buffer_idx;
        let old_bounds = active.bounds.clone();
        let mid_x = (old_bounds.x_start + old_bounds.x_end) / 2.0;

        // Shrink active pane
        self.panes[self.active_pane].bounds.x_end = mid_x;

        // Create new pane to the right
        let mut new_pane = Pane::with_buffer_idx(buffer_idx);
        new_pane.bounds = PaneBounds {
            x_start: mid_x,
            y_start: old_bounds.y_start,
            x_end: old_bounds.x_end,
            y_end: old_bounds.y_end,
        };

        self.panes.push(new_pane);
        self.active_pane = self.panes.len() - 1;
    }

    /// Split the active pane horizontally (new pane below, same buffer)
    pub fn split_horizontal(&mut self) {
        let active = &self.panes[self.active_pane];
        let buffer_idx = active.buffer_idx;
        let old_bounds = active.bounds.clone();
        let mid_y = (old_bounds.y_start + old_bounds.y_end) / 2.0;

        // Shrink active pane
        self.panes[self.active_pane].bounds.y_end = mid_y;

        // Create new pane below
        let mut new_pane = Pane::with_buffer_idx(buffer_idx);
        new_pane.bounds = PaneBounds {
            x_start: old_bounds.x_start,
            y_start: mid_y,
            x_end: old_bounds.x_end,
            y_end: old_bounds.y_end,
        };

        self.panes.push(new_pane);
        self.active_pane = self.panes.len() - 1;
    }

    /// Split vertical with a new file in the new pane
    pub fn split_vertical_with_file(&mut self, path: &Path, workspace_root: &Path) -> Result<()> {
        let buffer_entry = BufferEntry::from_file(path, workspace_root)?;
        let new_buffer_idx = self.buffers.len();
        self.buffers.push(buffer_entry);

        let active = &self.panes[self.active_pane];
        let old_bounds = active.bounds.clone();
        let mid_x = (old_bounds.x_start + old_bounds.x_end) / 2.0;

        // Shrink active pane
        self.panes[self.active_pane].bounds.x_end = mid_x;

        // Create new pane to the right with the new buffer
        let mut new_pane = Pane::with_buffer_idx(new_buffer_idx);
        new_pane.bounds = PaneBounds {
            x_start: mid_x,
            y_start: old_bounds.y_start,
            x_end: old_bounds.x_end,
            y_end: old_bounds.y_end,
        };

        self.panes.push(new_pane);
        self.active_pane = self.panes.len() - 1;
        Ok(())
    }

    /// Split horizontal with a new file in the new pane
    pub fn split_horizontal_with_file(&mut self, path: &Path, workspace_root: &Path) -> Result<()> {
        let buffer_entry = BufferEntry::from_file(path, workspace_root)?;
        let new_buffer_idx = self.buffers.len();
        self.buffers.push(buffer_entry);

        let active = &self.panes[self.active_pane];
        let old_bounds = active.bounds.clone();
        let mid_y = (old_bounds.y_start + old_bounds.y_end) / 2.0;

        // Shrink active pane
        self.panes[self.active_pane].bounds.y_end = mid_y;

        // Create new pane below with the new buffer
        let mut new_pane = Pane::with_buffer_idx(new_buffer_idx);
        new_pane.bounds = PaneBounds {
            x_start: old_bounds.x_start,
            y_start: mid_y,
            x_end: old_bounds.x_end,
            y_end: old_bounds.y_end,
        };

        self.panes.push(new_pane);
        self.active_pane = self.panes.len() - 1;
        Ok(())
    }

    /// Close the active pane
    /// Returns true if the tab should be closed (no panes left)
    pub fn close_active_pane(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return true; // Last pane - tab should close
        }

        // Remove the pane
        self.panes.remove(self.active_pane);
        if self.active_pane >= self.panes.len() {
            self.active_pane = self.panes.len() - 1;
        }

        // Recalculate bounds - for now just expand remaining panes equally
        // This is a simplified approach; a proper tiling system would be more complex
        self.recalculate_pane_bounds();
        false
    }

    /// Recalculate pane bounds after closing a pane
    fn recalculate_pane_bounds(&mut self) {
        // Simple approach: split screen equally among remaining panes
        let n = self.panes.len();
        if n == 1 {
            self.panes[0].bounds = PaneBounds::default();
        } else {
            // Arrange panes horizontally for now
            for (i, pane) in self.panes.iter_mut().enumerate() {
                let width = 1.0 / n as f32;
                pane.bounds = PaneBounds {
                    x_start: i as f32 * width,
                    y_start: 0.0,
                    x_end: (i + 1) as f32 * width,
                    y_end: 1.0,
                };
            }
        }
    }

    /// Navigate to the next pane
    pub fn next_pane(&mut self) {
        self.active_pane = (self.active_pane + 1) % self.panes.len();
    }

    /// Navigate to the previous pane
    pub fn prev_pane(&mut self) {
        if self.active_pane == 0 {
            self.active_pane = self.panes.len() - 1;
        } else {
            self.active_pane -= 1;
        }
    }

    /// Navigate to pane in direction (for vim-style navigation)
    pub fn navigate_pane(&mut self, direction: PaneDirection) {
        if self.panes.len() <= 1 {
            return;
        }

        let current = &self.panes[self.active_pane];
        let current_center_x = (current.bounds.x_start + current.bounds.x_end) / 2.0;
        let current_center_y = (current.bounds.y_start + current.bounds.y_end) / 2.0;

        let mut best_idx = None;
        let mut best_score = f32::MAX;

        for (i, pane) in self.panes.iter().enumerate() {
            if i == self.active_pane {
                continue;
            }

            let center_x = (pane.bounds.x_start + pane.bounds.x_end) / 2.0;
            let center_y = (pane.bounds.y_start + pane.bounds.y_end) / 2.0;

            let (is_valid, score) = match direction {
                PaneDirection::Left => (center_x < current_center_x, current_center_x - center_x),
                PaneDirection::Right => (center_x > current_center_x, center_x - current_center_x),
                PaneDirection::Up => (center_y < current_center_y, current_center_y - center_y),
                PaneDirection::Down => (center_y > current_center_y, center_y - current_center_y),
            };

            if is_valid && score < best_score {
                best_score = score;
                best_idx = Some(i);
            }
        }

        if let Some(idx) = best_idx {
            self.active_pane = idx;
        }
    }

    /// Get number of panes
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Get the path of the primary buffer (for tab display and workspace tracking)
    pub fn path(&self) -> Option<&PathBuf> {
        self.buffers.first().and_then(|b| b.path.as_ref())
    }

    /// Check if the primary buffer is an orphan
    pub fn is_orphan(&self) -> bool {
        self.buffers.first().map(|b| b.is_orphan).unwrap_or(false)
    }
}

/// Direction for pane navigation
#[derive(Debug, Clone, Copy)]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

impl Default for Tab {
    fn default() -> Self {
        Self::new()
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
pub struct Workspace {
    /// Root directory of the workspace
    pub root: PathBuf,
    /// All open tabs
    pub tabs: Vec<Tab>,
    /// Currently active tab index
    pub active_tab: usize,
    /// Fuss mode (file tree) state
    pub fuss: FussMode,
    /// Workspace configuration
    pub config: WorkspaceConfig,
}

impl Workspace {
    /// Create a new workspace for a directory
    pub fn new(root: PathBuf) -> Self {
        let mut fuss = FussMode::new();
        fuss.init(&root);
        Self {
            root,
            tabs: vec![Tab::new()],
            active_tab: 0,
            fuss,
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
        // Canonicalize the path to handle relative paths
        let abs_path = file_path.canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf());

        // Determine workspace root
        let root = Self::detect_from_file(&abs_path)
            .or_else(|| abs_path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let mut workspace = Self::open(root)?;

        // Open the file in a tab
        if abs_path.exists() {
            workspace.open_file(&abs_path)?;
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
        // Check if file is already open in any tab's primary buffer
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(tab_path) = tab.path() {
                let full_path = if tab.is_orphan() {
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

    /// Open a file in a vertical split pane in the current tab
    pub fn open_file_in_vsplit(&mut self, path: &Path) -> Result<()> {
        self.tabs[self.active_tab].split_vertical_with_file(path, &self.root)
    }

    /// Open a file in a horizontal split pane in the current tab
    pub fn open_file_in_hsplit(&mut self, path: &Path) -> Result<()> {
        self.tabs[self.active_tab].split_horizontal_with_file(path, &self.root)
    }

    /// Create a new empty tab
    pub fn new_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.active_tab = self.tabs.len() - 1;
    }

    /// Open a content tab (for diff views, etc.)
    pub fn open_content_tab(&mut self, content: &str, display_name: &str) {
        let tab = Tab::from_content(content, display_name);
        self.tabs.push(tab);
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

    // === Backup functionality ===

    /// Get the backups directory path
    fn backups_dir(&self) -> PathBuf {
        self.root.join(".fackr").join("backups")
    }

    /// Generate a backup filename for a buffer path
    /// Uses a hash of the path to create a unique but deterministic name
    fn backup_filename(&self, path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:016x}.bak", hasher.finish())
    }

    /// Write a backup for a modified buffer
    pub fn write_backup(&self, path: &Path, content: &str) -> Result<()> {
        let backups_dir = self.backups_dir();
        std::fs::create_dir_all(&backups_dir)?;

        let backup_path = backups_dir.join(self.backup_filename(path));

        // Store as simple format: first line is original path, rest is content
        let backup_content = format!("{}\n{}", path.display(), content);
        std::fs::write(&backup_path, backup_content)?;

        Ok(())
    }

    /// Delete backup for a buffer (called after successful save)
    pub fn delete_backup(&self, path: &Path) -> Result<()> {
        let backup_path = self.backups_dir().join(self.backup_filename(path));
        if backup_path.exists() {
            std::fs::remove_file(backup_path)?;
        }
        Ok(())
    }

    /// Delete all backups (called on discard)
    pub fn delete_all_backups(&self) -> Result<()> {
        let backups_dir = self.backups_dir();
        if backups_dir.exists() {
            for entry in std::fs::read_dir(&backups_dir)? {
                let entry = entry?;
                if entry.path().extension().map_or(false, |e| e == "bak") {
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }

    /// Check if there are any backups to restore
    pub fn has_backups(&self) -> bool {
        let backups_dir = self.backups_dir();
        if !backups_dir.exists() {
            return false;
        }
        if let Ok(entries) = std::fs::read_dir(&backups_dir) {
            for entry in entries.flatten() {
                if entry.path().extension().map_or(false, |e| e == "bak") {
                    return true;
                }
            }
        }
        false
    }

    /// Get list of backup info (original path, backup path)
    pub fn list_backups(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut backups = Vec::new();
        let backups_dir = self.backups_dir();

        if !backups_dir.exists() {
            return backups;
        }

        if let Ok(entries) = std::fs::read_dir(&backups_dir) {
            for entry in entries.flatten() {
                let backup_path = entry.path();
                if backup_path.extension().map_or(false, |e| e == "bak") {
                    // Read first line to get original path
                    if let Ok(content) = std::fs::read_to_string(&backup_path) {
                        if let Some(first_line) = content.lines().next() {
                            backups.push((PathBuf::from(first_line), backup_path));
                        }
                    }
                }
            }
        }

        backups
    }

    /// Restore a backup into its buffer
    /// Returns the original path and content
    pub fn read_backup(&self, backup_path: &Path) -> Result<(PathBuf, String)> {
        let content = std::fs::read_to_string(backup_path)?;
        let mut lines = content.lines();

        let original_path = lines.next()
            .ok_or_else(|| anyhow::anyhow!("Invalid backup file: missing path"))?;

        let content: String = lines.collect::<Vec<_>>().join("\n");

        Ok((PathBuf::from(original_path), content))
    }

    /// Check if any buffer in the workspace has unsaved changes
    pub fn has_unsaved_changes(&self) -> bool {
        for tab in &self.tabs {
            for buffer_entry in &tab.buffers {
                if buffer_entry.is_modified() {
                    return true;
                }
            }
        }
        false
    }

    /// Get list of modified buffer paths
    pub fn modified_buffers(&self) -> Vec<PathBuf> {
        let mut modified = Vec::new();
        for tab in &self.tabs {
            for buffer_entry in &tab.buffers {
                if buffer_entry.is_modified() {
                    if let Some(path) = &buffer_entry.path {
                        // Convert relative path to absolute
                        let full_path = if buffer_entry.is_orphan {
                            path.clone()
                        } else {
                            self.root.join(path)
                        };
                        modified.push(full_path);
                    }
                }
            }
        }
        modified
    }

    /// Save all modified buffers
    pub fn save_all(&mut self) -> Result<()> {
        // Collect paths to save first to avoid borrow issues
        let mut to_save: Vec<(usize, usize, PathBuf)> = Vec::new();

        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            for (buf_idx, buffer_entry) in tab.buffers.iter().enumerate() {
                if buffer_entry.is_modified() {
                    if let Some(path) = &buffer_entry.path {
                        let full_path = if buffer_entry.is_orphan {
                            path.clone()
                        } else {
                            self.root.join(path)
                        };
                        to_save.push((tab_idx, buf_idx, full_path));
                    }
                }
            }
        }

        // Now save each buffer
        for (tab_idx, buf_idx, full_path) in to_save {
            self.tabs[tab_idx].buffers[buf_idx].buffer.save(&full_path)?;
            self.tabs[tab_idx].buffers[buf_idx].mark_saved();
            // Delete backup after successful save
            let _ = self.delete_backup(&full_path);
        }

        Ok(())
    }

    /// Write backups for all modified buffers
    pub fn backup_all_modified(&self) -> Result<()> {
        for tab in &self.tabs {
            for buffer_entry in &tab.buffers {
                if buffer_entry.is_modified() {
                    if let Some(path) = &buffer_entry.path {
                        let full_path = if buffer_entry.is_orphan {
                            path.clone()
                        } else {
                            self.root.join(path)
                        };
                        let content = buffer_entry.buffer.contents();
                        self.write_backup(&full_path, &content)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Get the workspace directory name (repo name)
    pub fn repo_name(&self) -> String {
        self.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace")
            .to_string()
    }

    /// Get the current git branch name, if in a git repo
    pub fn git_branch(&self) -> Option<String> {
        use std::process::Command;

        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .arg("branch")
            .arg("--show-current")
            .output()
            .ok()?;

        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            if branch.is_empty() {
                // Detached HEAD - try to get short SHA
                let sha_output = Command::new("git")
                    .arg("-C")
                    .arg(&self.root)
                    .arg("rev-parse")
                    .arg("--short")
                    .arg("HEAD")
                    .output()
                    .ok()?;
                if sha_output.status.success() {
                    let sha = String::from_utf8_lossy(&sha_output.stdout)
                        .trim()
                        .to_string();
                    return Some(format!("({})", sha));
                }
                None
            } else {
                Some(branch)
            }
        } else {
            None // Not a git repo
        }
    }

    /// Check if this workspace is a git repository
    pub fn is_git_repo(&self) -> bool {
        self.root.join(".git").exists()
    }
}
