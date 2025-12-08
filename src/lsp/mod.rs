//! LSP (Language Server Protocol) client module
//!
//! Provides LSP support for fackr, enabling:
//! - Code completion
//! - Hover information
//! - Go to definition
//! - Find references
//! - Diagnostics
//! - Code actions
//! - Document symbols
//! - Rename refactoring
//! - Document formatting

mod client;
mod manager;
mod message;
mod process;
mod protocol;
pub mod server_manager;
mod types;

pub use client::{LspClient, LspResponse};
pub use server_manager::ServerManagerPanel;
pub use types::{
    CompletionItem, Diagnostic, DiagnosticSeverity, HoverInfo, Location, TextEdit, uri_to_path,
};
