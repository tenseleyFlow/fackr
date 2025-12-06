//! Fuss mode - file tree sidebar
//!
//! Fuss mode provides a file tree view for navigating and opening files.
//! Toggle with Ctrl+B.

mod tree;
mod state;

pub use state::FussMode;
#[allow(unused_imports)]
pub use tree::{FileTree, TreeNode, VisibleItem};
