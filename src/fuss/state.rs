//! Fuss mode state management

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use super::tree::FileTree;

/// Fuss mode state
#[derive(Debug)]
pub struct FussMode {
    /// Is fuss mode active?
    pub active: bool,
    /// The file tree
    pub tree: Option<FileTree>,
    /// Currently selected index
    pub selected: usize,
    /// Viewport scroll offset
    pub scroll: usize,
    /// Width as percentage of screen (default 30%)
    pub width_percent: u8,
    /// Show hints expanded
    pub hints_expanded: bool,
    /// Workspace root path
    root_path: Option<PathBuf>,
}

impl Default for FussMode {
    fn default() -> Self {
        Self {
            active: false,
            tree: None,
            selected: 0,
            scroll: 0,
            width_percent: 30,
            hints_expanded: false,
            root_path: None,
        }
    }
}

impl FussMode {
    /// Create new fuss mode state
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize with a root path
    pub fn init(&mut self, root_path: &Path) {
        self.root_path = Some(root_path.to_path_buf());
        self.tree = Some(FileTree::new(root_path));
        self.selected = 0;
        self.scroll = 0;
    }

    /// Toggle fuss mode on/off
    pub fn toggle(&mut self) {
        self.active = !self.active;
        if self.active && self.tree.is_none() {
            if let Some(ref path) = self.root_path {
                self.tree = Some(FileTree::new(path));
            }
        }
    }

    /// Activate fuss mode
    pub fn activate(&mut self, root_path: &Path) {
        if self.tree.is_none() || self.root_path.as_deref() != Some(root_path) {
            self.init(root_path);
        }
        self.active = true;
    }

    /// Deactivate fuss mode
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if let Some(ref tree) = self.tree {
            if self.selected + 1 < tree.len() {
                self.selected += 1;
            }
        }
    }

    /// Toggle expand/collapse of selected directory
    pub fn toggle_expand(&mut self) {
        if let Some(ref mut tree) = self.tree {
            if tree.is_dir_at(self.selected) {
                tree.toggle_at(self.selected);
            }
        }
    }

    /// Get the selected path (if it's a file)
    pub fn selected_file(&self) -> Option<PathBuf> {
        if let Some(ref tree) = self.tree {
            if !tree.is_dir_at(self.selected) {
                return tree.path_at(self.selected).map(|p| p.to_path_buf());
            }
        }
        None
    }

    /// Get the selected path (file or directory)
    pub fn selected_path(&self) -> Option<PathBuf> {
        if let Some(ref tree) = self.tree {
            return tree.path_at(self.selected).map(|p| p.to_path_buf());
        }
        None
    }

    /// Check if selected item is a directory
    pub fn is_dir_selected(&self) -> bool {
        if let Some(ref tree) = self.tree {
            return tree.is_dir_at(self.selected);
        }
        false
    }

    /// Toggle showing hidden files
    pub fn toggle_hidden(&mut self) {
        if let Some(ref mut tree) = self.tree {
            tree.toggle_hidden();
            // Clamp selection
            if self.selected >= tree.len() && tree.len() > 0 {
                self.selected = tree.len() - 1;
            }
        }
    }

    /// Toggle hints expanded/collapsed
    pub fn toggle_hints(&mut self) {
        self.hints_expanded = !self.hints_expanded;
    }

    /// Update viewport to keep selection visible
    pub fn update_viewport(&mut self, visible_rows: usize) {
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible_rows {
            self.scroll = self.selected - visible_rows + 1;
        }
    }

    /// Get calculated width in columns
    pub fn width(&self, screen_cols: u16) -> u16 {
        ((screen_cols as u32 * self.width_percent as u32) / 100) as u16
    }

    /// Reload tree from disk
    pub fn reload(&mut self) {
        if let Some(ref mut tree) = self.tree {
            tree.reload();
        }
    }
}
