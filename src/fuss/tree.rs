//! File tree data structure

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::fs;

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
}
