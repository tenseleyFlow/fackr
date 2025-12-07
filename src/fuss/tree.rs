//! File tree data structure

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use std::collections::HashMap;

/// Git status for a file
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    /// File is staged (added to index)
    pub staged: bool,
    /// File has unstaged changes
    pub unstaged: bool,
    /// File is untracked
    pub untracked: bool,
    /// File has incoming changes (after fetch)
    pub incoming: bool,
    /// File is gitignored
    pub gitignored: bool,
}

/// A node in the file tree
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// File/directory name
    pub name: String,
    /// Full path
    pub path: PathBuf,
    /// Is this a directory?
    pub is_dir: bool,
    /// Is directory expanded?
    pub expanded: bool,
    /// Children (only for directories)
    pub children: Vec<TreeNode>,
    /// Depth in tree (for indentation)
    pub depth: usize,
    /// Git status for this file
    pub git_status: GitStatus,
}

impl TreeNode {
    /// Create a new tree node
    pub fn new(path: PathBuf, depth: usize) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let is_dir = path.is_dir();

        Self {
            name,
            path,
            is_dir,
            expanded: depth == 0, // Root is expanded by default
            children: Vec::new(),
            depth,
            git_status: GitStatus::default(),
        }
    }

    /// Check if this is a hidden file (starts with .)
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    /// Load children for a directory
    pub fn load_children(&mut self, show_hidden: bool) {
        if !self.is_dir {
            return;
        }

        self.children.clear();

        if let Ok(entries) = fs::read_dir(&self.path) {
            let mut children: Vec<TreeNode> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let name = e.file_name();
                    let name_str = name.to_string_lossy();
                    show_hidden || !name_str.starts_with('.')
                })
                .map(|e| TreeNode::new(e.path(), self.depth + 1))
                .collect();

            // Sort: directories first, then alphabetically
            children.sort_by(|a, b| {
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });

            self.children = children;
        }
    }

    /// Toggle expanded state
    pub fn toggle_expand(&mut self) {
        if self.is_dir {
            self.expanded = !self.expanded;
            if self.expanded && self.children.is_empty() {
                self.load_children(false);
            }
        }
    }
}

/// File tree for the workspace
#[derive(Debug)]
pub struct FileTree {
    /// Root node
    pub root: TreeNode,
    /// Show hidden files
    pub show_hidden: bool,
    /// Flattened visible items (for rendering and navigation)
    visible_items: Vec<VisibleItem>,
}

/// A visible item in the flattened tree
#[derive(Debug, Clone)]
pub struct VisibleItem {
    /// Path to the item
    pub path: PathBuf,
    /// Display name
    pub name: String,
    /// Is directory
    pub is_dir: bool,
    /// Is expanded (for directories)
    pub expanded: bool,
    /// Indentation depth
    pub depth: usize,
    /// Git status
    pub git_status: GitStatus,
}

impl FileTree {
    /// Create a new file tree rooted at the given path
    pub fn new(root_path: &Path) -> Self {
        let mut root = TreeNode::new(root_path.to_path_buf(), 0);
        root.load_children(false);

        let mut tree = Self {
            root,
            show_hidden: false,
            visible_items: Vec::new(),
        };
        tree.rebuild_visible();
        tree
    }

    /// Rebuild the flattened visible items list
    pub fn rebuild_visible(&mut self) {
        self.visible_items.clear();
        self.collect_visible(&self.root.clone());
    }

    fn collect_visible(&mut self, node: &TreeNode) {
        // Don't include root in visible items, but process its children
        if node.depth > 0 {
            self.visible_items.push(VisibleItem {
                path: node.path.clone(),
                name: node.name.clone(),
                is_dir: node.is_dir,
                expanded: node.expanded,
                depth: node.depth,
                git_status: node.git_status.clone(),
            });
        }

        if node.is_dir && (node.expanded || node.depth == 0) {
            for child in &node.children {
                self.collect_visible(child);
            }
        }
    }

    /// Get visible items
    pub fn visible_items(&self) -> &[VisibleItem] {
        &self.visible_items
    }

    /// Number of visible items
    pub fn len(&self) -> usize {
        self.visible_items.len()
    }

    /// Toggle expand/collapse at index
    pub fn toggle_at(&mut self, index: usize) {
        if index >= self.visible_items.len() {
            return;
        }

        let path = self.visible_items[index].path.clone();
        self.toggle_path(&path);
        self.rebuild_visible();
    }

    fn toggle_path(&mut self, path: &Path) {
        Self::toggle_path_recursive(&mut self.root, path);
    }

    fn toggle_path_recursive(node: &mut TreeNode, path: &Path) -> bool {
        if node.path == path {
            node.toggle_expand();
            return true;
        }

        for child in &mut node.children {
            if Self::toggle_path_recursive(child, path) {
                return true;
            }
        }

        false
    }

    /// Get path at index
    pub fn path_at(&self, index: usize) -> Option<&Path> {
        self.visible_items.get(index).map(|i| i.path.as_path())
    }

    /// Check if item at index is a directory
    pub fn is_dir_at(&self, index: usize) -> bool {
        self.visible_items.get(index).map(|i| i.is_dir).unwrap_or(false)
    }

    /// Toggle showing hidden files
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.reload();
    }

    /// Reload tree from disk
    pub fn reload(&mut self) {
        Self::reload_node(&mut self.root, self.show_hidden);
        self.rebuild_visible();
    }

    fn reload_node(node: &mut TreeNode, show_hidden: bool) {
        if node.is_dir && node.expanded {
            node.load_children(show_hidden);
            for child in &mut node.children {
                Self::reload_node(child, show_hidden);
            }
        }
    }

    /// Update git status for all files in the tree
    pub fn update_git_status(&mut self) {
        let root_path = self.root.path.clone();
        let status_map = get_git_status(&root_path);
        Self::apply_git_status(&mut self.root, &status_map, &root_path);
        // Smart collapse: only expand directories with dirty files
        Self::smart_collapse_node(&mut self.root, true);
        self.rebuild_visible();
    }

    fn apply_git_status(node: &mut TreeNode, status_map: &HashMap<PathBuf, GitStatus>, root: &Path) {
        // Get relative path from root
        if let Ok(rel_path) = node.path.strip_prefix(root) {
            if let Some(status) = status_map.get(rel_path) {
                node.git_status = status.clone();
            } else {
                node.git_status = GitStatus::default();
            }
        }

        // Recurse into children
        for child in &mut node.children {
            Self::apply_git_status(child, status_map, root);
        }
    }

    /// Check if tree has any dirty files (staged, unstaged, or untracked)
    pub fn has_dirty_files(&self) -> bool {
        Self::node_has_dirty(&self.root)
    }

    fn node_has_dirty(node: &TreeNode) -> bool {
        if node.git_status.staged || node.git_status.unstaged || node.git_status.untracked {
            return true;
        }
        for child in &node.children {
            if Self::node_has_dirty(child) {
                return true;
            }
        }
        false
    }

    /// Smart collapse: Only expand directories that contain dirty files
    /// Root is always expanded
    pub fn smart_collapse(&mut self) {
        Self::smart_collapse_node(&mut self.root, true);
        self.rebuild_visible();
    }

    /// Returns true if node or any descendant has dirty files
    fn smart_collapse_node(node: &mut TreeNode, is_root: bool) -> bool {
        if !node.is_dir {
            // Files: return whether they're dirty
            return node.git_status.staged || node.git_status.unstaged || node.git_status.untracked;
        }

        // Directory: check all children first
        let mut has_dirty_descendant = false;
        for child in &mut node.children {
            if Self::smart_collapse_node(child, false) {
                has_dirty_descendant = true;
            }
        }

        // Root stays expanded, others only expand if they have dirty descendants
        if is_root {
            node.expanded = true;
        } else {
            node.expanded = has_dirty_descendant;
        }

        has_dirty_descendant
    }
}

/// Parse git status --porcelain output and return a map of file paths to git status
fn get_git_status(root: &Path) -> HashMap<PathBuf, GitStatus> {
    let mut status_map = HashMap::new();

    // Run git status --porcelain
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 4 {
                    continue;
                }

                // Format: XY filename
                // X = index status, Y = worktree status
                let index_status = line.chars().next().unwrap_or(' ');
                let worktree_status = line.chars().nth(1).unwrap_or(' ');
                let filename = line[3..].trim();

                // Handle renamed files (format: "R  old -> new")
                let filename = if filename.contains(" -> ") {
                    filename.split(" -> ").last().unwrap_or(filename)
                } else {
                    filename
                };

                let path = PathBuf::from(filename);
                let mut status = GitStatus::default();

                // Check for ignored (!! status)
                if index_status == '!' && worktree_status == '!' {
                    status.gitignored = true;
                }
                // Check for untracked
                else if index_status == '?' && worktree_status == '?' {
                    status.untracked = true;
                } else {
                    // Staged: any non-space, non-? in index position
                    if index_status != ' ' && index_status != '?' {
                        status.staged = true;
                    }
                    // Unstaged: any non-space, non-? in worktree position
                    if worktree_status != ' ' && worktree_status != '?' {
                        status.unstaged = true;
                    }
                }

                status_map.insert(path, status);
            }
        }
    }

    // Also get ignored files using --ignored flag
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain")
        .arg("--ignored")
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 4 {
                    continue;
                }

                let index_status = line.chars().next().unwrap_or(' ');
                let worktree_status = line.chars().nth(1).unwrap_or(' ');

                // !! means ignored
                if index_status == '!' && worktree_status == '!' {
                    let filename = line[3..].trim();
                    let path = PathBuf::from(filename);

                    // Only add if not already in map with other status
                    status_map.entry(path).or_insert_with(|| {
                        let mut s = GitStatus::default();
                        s.gitignored = true;
                        s
                    });
                }
            }
        }
    }

    // Get files with incoming changes (differ from upstream)
    // Use git diff --name-only @{u}...HEAD to see files changed in upstream but not in local
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--name-only")
        .arg("HEAD...@{u}")
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let filename = line.trim();
                if filename.is_empty() {
                    continue;
                }
                let path = PathBuf::from(filename);

                // Mark as having incoming changes
                status_map
                    .entry(path)
                    .and_modify(|s| s.incoming = true)
                    .or_insert_with(|| {
                        let mut s = GitStatus::default();
                        s.incoming = true;
                        s
                    });
            }
        }
        // If command fails (no upstream), that's fine - just no incoming indicators
    }

    status_map
}
