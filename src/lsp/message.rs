//! LSP message handling and callback management
//!
//! Handles routing of LSP responses to appropriate callbacks.
//!
//! Note: ParsedResponse helpers are for planned features.
#![allow(dead_code)]

use serde_json::Value;
use std::collections::HashMap;

use super::protocol::{LspMessage, ResponseError};
use super::types::{
    CompletionItem, Diagnostic, DocumentSymbol, HoverInfo, Location, TextEdit, WorkspaceEdit,
};

/// Result type for LSP responses
pub type LspResult<T> = Result<T, ResponseError>;

/// Callback for LSP responses
pub type ResponseCallback = Box<dyn FnOnce(i64, LspResult<Value>) + Send>;

/// Callback for diagnostics notifications
pub type DiagnosticsCallback = Box<dyn Fn(String, Vec<Diagnostic>) + Send>;

/// Tracks pending requests and their callbacks
pub struct MessageHandler {
    /// Pending request callbacks indexed by request ID
    pending: HashMap<i64, ResponseCallback>,
    /// Callback for diagnostics notifications
    diagnostics_callback: Option<DiagnosticsCallback>,
}

impl MessageHandler {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            diagnostics_callback: None,
        }
    }

    /// Register a callback for a request
    pub fn register_callback(&mut self, id: i64, callback: ResponseCallback) {
        self.pending.insert(id, callback);
    }

    /// Set the diagnostics callback
    pub fn set_diagnostics_callback(&mut self, callback: DiagnosticsCallback) {
        self.diagnostics_callback = Some(callback);
    }

    /// Handle an incoming message
    pub fn handle_message(&mut self, message: LspMessage) -> Option<LspMessage> {
        match message {
            LspMessage::Response { id, result, error } => {
                self.handle_response(id, result, error);
                None
            }
            LspMessage::Notification { method, params } => {
                self.handle_notification(&method, params);
                None
            }
            LspMessage::Request { id, method, params } => {
                // Handle server-to-client requests
                self.handle_server_request(id, &method, params)
            }
        }
    }

    /// Handle a response message
    fn handle_response(&mut self, id: i64, result: Option<Value>, error: Option<ResponseError>) {
        if let Some(callback) = self.pending.remove(&id) {
            let response = if let Some(err) = error {
                Err(err)
            } else {
                Ok(result.unwrap_or(Value::Null))
            };
            callback(id, response);
        }
    }

    /// Handle a notification message
    fn handle_notification(&mut self, method: &str, params: Option<Value>) {
        match method {
            "textDocument/publishDiagnostics" => {
                if let (Some(params), Some(callback)) = (params, &self.diagnostics_callback) {
                    let (uri, diagnostics) = super::protocol::parse_diagnostics(&params);
                    callback(uri, diagnostics);
                }
            }
            "window/logMessage" | "window/showMessage" => {
                // Silently ignore server log messages
                // These could be surfaced to the status bar via a callback if needed
                let _ = params;
            }
            _ => {
                // Ignore other notifications
            }
        }
    }

    /// Handle a server-to-client request (return a response if needed)
    fn handle_server_request(
        &mut self,
        id: i64,
        method: &str,
        _params: Option<Value>,
    ) -> Option<LspMessage> {
        match method {
            "workspace/configuration" => {
                // Return empty configuration
                Some(LspMessage::Response {
                    id,
                    result: Some(Value::Array(vec![])),
                    error: None,
                })
            }
            "client/registerCapability" | "client/unregisterCapability" => {
                // Acknowledge capability registration
                Some(LspMessage::Response {
                    id,
                    result: Some(Value::Null),
                    error: None,
                })
            }
            "window/workDoneProgress/create" => {
                // Acknowledge progress creation
                Some(LspMessage::Response {
                    id,
                    result: Some(Value::Null),
                    error: None,
                })
            }
            _ => {
                // Unknown request - return method not found error
                Some(LspMessage::Response {
                    id,
                    result: None,
                    error: Some(ResponseError {
                        code: -32601, // Method not found
                        message: format!("Method not found: {}", method),
                        data: None,
                    }),
                })
            }
        }
    }

    /// Check if there are pending requests
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Get number of pending requests
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

impl Default for MessageHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed LSP response types for convenience
pub enum ParsedResponse {
    Completions(Vec<CompletionItem>),
    Hover(Option<HoverInfo>),
    Locations(Vec<Location>),
    Symbols(Vec<DocumentSymbol>),
    TextEdits(Vec<TextEdit>),
    WorkspaceEdit(WorkspaceEdit),
    Empty,
}

impl ParsedResponse {
    /// Parse a completion response
    pub fn parse_completions(result: &Value) -> Self {
        ParsedResponse::Completions(super::protocol::parse_completion_items(result))
    }

    /// Parse a hover response
    pub fn parse_hover(result: &Value) -> Self {
        ParsedResponse::Hover(super::protocol::parse_hover(result))
    }

    /// Parse a definition/references response
    pub fn parse_locations(result: &Value) -> Self {
        ParsedResponse::Locations(super::protocol::parse_locations(result))
    }

    /// Parse a document symbols response
    pub fn parse_symbols(result: &Value) -> Self {
        ParsedResponse::Symbols(super::protocol::parse_document_symbols(result))
    }

    /// Parse a formatting response
    pub fn parse_text_edits(result: &Value) -> Self {
        ParsedResponse::TextEdits(super::protocol::parse_text_edits(result))
    }

    /// Parse a rename response
    pub fn parse_workspace_edit(result: &Value) -> Self {
        ParsedResponse::WorkspaceEdit(super::protocol::parse_workspace_edit(result))
    }
}
