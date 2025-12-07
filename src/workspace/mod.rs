//! Workspace module - the defining unit of fackr
//!
//! A Workspace represents the editor state for a particular directory.
//! It is the container for all tabs, panes, cursors, and configuration.
//!
//! Directory structure:
//! ```
//! <workspace_root>/
//!   .fackr/
//!     workspace.json    # Persisted state (tabs, panes, cursors)
//!     backups/          # Auto-backups of dirty files
//! ```
//!
//! Workspace initialization:
//! - `fackr <dir>` - Opens directory as workspace
//! - `fackr <file>` - Implicitly opens containing directory as workspace
//! - `fackr` (no args) - Opens current directory as workspace

mod recents;
mod state;

pub use recents::{recents_add_or_update, recents_get, Recent};
#[allow(unused_imports)]
pub use state::{BufferEntry, Pane, PaneBounds, PaneDirection, Tab, Workspace, WorkspaceConfig};
