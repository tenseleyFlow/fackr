//! Fuss mode state management

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
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
        let mut tree = FileTree::new(root_path);
        tree.update_git_status();
        self.tree = Some(tree);
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
            tree.update_git_status();
        }
    }

    /// Refresh git status without reloading file tree
    pub fn refresh_git_status(&mut self) {
        if let Some(ref mut tree) = self.tree {
            tree.update_git_status();
        }
    }

    /// Stage the currently selected file
    /// Returns true on success, false on failure
    pub fn stage_selected(&mut self) -> bool {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return false,
        };

        let path = match self.selected_path() {
            Some(p) => p,
            None => return false,
        };

        // Don't stage directories
        if self.is_dir_selected() {
            return false;
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("add")
            .arg(&path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                self.refresh_git_status();
                return true;
            }
        }
        false
    }

    /// Unstage the currently selected file
    /// Returns true on success, false on failure
    pub fn unstage_selected(&mut self) -> bool {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return false,
        };

        let path = match self.selected_path() {
            Some(p) => p,
            None => return false,
        };

        // Don't unstage directories
        if self.is_dir_selected() {
            return false;
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("restore")
            .arg("--staged")
            .arg(&path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                self.refresh_git_status();
                return true;
            }
        }
        false
    }

    /// Get the root path
    pub fn root_path(&self) -> Option<&Path> {
        self.root_path.as_deref()
    }

    /// Push to remote
    /// Returns (success, message)
    pub fn git_push(&mut self) -> (bool, String) {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return (false, "No workspace".to_string()),
        };

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("push")
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.refresh_git_status();
                (true, "Pushed".to_string())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                (false, format!("Push failed: {}", stderr.lines().next().unwrap_or("unknown error")))
            }
            Err(e) => (false, format!("Failed to run git: {}", e)),
        }
    }

    /// Pull from remote
    /// Returns (success, message)
    pub fn git_pull(&mut self) -> (bool, String) {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return (false, "No workspace".to_string()),
        };

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("pull")
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.refresh_git_status();
                (true, "Pulled".to_string())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                (false, format!("Pull failed: {}", stderr.lines().next().unwrap_or("unknown error")))
            }
            Err(e) => (false, format!("Failed to run git: {}", e)),
        }
    }

    /// Create a git tag
    /// Returns (success, message)
    pub fn git_tag(&mut self, tag_name: &str) -> (bool, String) {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return (false, "No workspace".to_string()),
        };

        if tag_name.trim().is_empty() {
            return (false, "Empty tag name".to_string());
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("tag")
            .arg(tag_name.trim())
            .output();

        match output {
            Ok(out) if out.status.success() => {
                (true, format!("Created tag: {}", tag_name.trim()))
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                (false, format!("Tag failed: {}", stderr.lines().next().unwrap_or("unknown error")))
            }
            Err(e) => (false, format!("Failed to run git: {}", e)),
        }
    }

    /// Fetch from remote
    /// Returns (success, message)
    pub fn git_fetch(&mut self) -> (bool, String) {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return (false, "No workspace".to_string()),
        };

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("fetch")
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.refresh_git_status();
                (true, "Fetched".to_string())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                (false, format!("Fetch failed: {}", stderr.lines().next().unwrap_or("unknown error")))
            }
            Err(e) => (false, format!("Failed to run git: {}", e)),
        }
    }

    /// Commit staged changes with the given message
    /// Returns (success, message)
    pub fn git_commit(&mut self, message: &str) -> (bool, String) {
        let root = match &self.root_path {
            Some(p) => p.clone(),
            None => return (false, "No workspace".to_string()),
        };

        if message.trim().is_empty() {
            return (false, "Empty commit message".to_string());
        }

        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .arg("commit")
            .arg("-m")
            .arg(message)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                self.refresh_git_status();
                (true, "Committed".to_string())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("nothing to commit") {
                    (false, "Nothing to commit".to_string())
                } else {
                    (false, format!("Commit failed: {}", stderr.lines().next().unwrap_or("unknown error")))
                }
            }
            Err(e) => (false, format!("Failed to run git: {}", e)),
        }
    }

    /// Get git diff for the currently selected file
    /// Returns (filename, diff_content) or None if no diff
    pub fn get_diff_for_selected(&self) -> Option<(String, String)> {
        let root = self.root_path.as_ref()?;
        let path = self.selected_path()?;

        // Don't diff directories
        if self.is_dir_selected() {
            return None;
        }

        // Get relative path for display
        let rel_path = path.strip_prefix(root).unwrap_or(&path);
        let filename = rel_path.to_string_lossy().to_string();

        // Run git diff
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .arg("diff")
            .arg("HEAD")
            .arg("--")
            .arg(&path)
            .output()
            .ok()?;

        if output.status.success() {
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if diff.is_empty() {
                Some((filename, "(no changes)".to_string()))
            } else {
                Some((filename, diff))
            }
        } else {
            None
        }
    }
}
