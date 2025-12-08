//! High-level LSP client API
//!
//! Provides a convenient interface for the editor to interact with language servers.
//!
//! Note: Many methods here are planned LSP features not yet wired to keybindings/UI.
#![allow(dead_code)]

use anyhow::Result;
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use super::manager::LspManager;
use super::protocol;
use super::types::{
    detect_language, path_to_uri, CompletionItem, Diagnostic, DocumentSymbol, HoverInfo, Location,
    Position, Range, TextEdit, WorkspaceEdit,
};

/// Document state tracked by the LSP client
#[derive(Debug)]
struct DocumentInfo {
    uri: String,
    language_id: String,
    version: i32,
}

/// High-level LSP client for the editor
pub struct LspClient {
    manager: LspManager,
    /// Tracked documents
    documents: HashMap<String, DocumentInfo>,
    /// Channel for receiving async responses
    response_rx: Receiver<LspResponse>,
    response_tx: Sender<LspResponse>,
    /// Pending diagnostics by URI
    diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
}

/// Response types that can be received asynchronously
#[derive(Debug)]
pub enum LspResponse {
    Completions(i64, Vec<CompletionItem>),
    Hover(i64, Option<HoverInfo>),
    Definition(i64, Vec<Location>),
    References(i64, Vec<Location>),
    Symbols(i64, Vec<DocumentSymbol>),
    Formatting(i64, Vec<TextEdit>),
    Rename(i64, WorkspaceEdit),
    CodeActions(i64, Vec<CodeAction>),
    Error(i64, String),
}

/// Code action from the server
#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub command: Option<String>,
}

impl LspClient {
    /// Create a new LSP client
    pub fn new(workspace_root: &str) -> Self {
        let (tx, rx) = mpsc::channel();
        let diagnostics = Arc::new(Mutex::new(HashMap::new()));
        let diag_clone = Arc::clone(&diagnostics);

        let mut manager = LspManager::new(workspace_root);

        // Set up diagnostics callback
        manager.set_diagnostics_callback(move |uri, diags| {
            if let Ok(mut map) = diag_clone.lock() {
                map.insert(uri, diags);
            }
        });

        Self {
            manager,
            documents: HashMap::new(),
            response_rx: rx,
            response_tx: tx,
            diagnostics,
        }
    }

    /// Open a document (notifies the language server)
    pub fn open_document(&mut self, path: &str, content: &str) -> Result<()> {
        let language_id = match detect_language(path) {
            Some(lang) => lang,
            None => return Ok(()), // No LSP support for this file type
        };

        let uri = path_to_uri(path);

        // Check if document is already being tracked
        if self.documents.contains_key(path) {
            // Document already tracked - don't send another didOpen
            // The original didOpen (possibly queued) will be sent eventually
            // Just update the content if needed via didChange
            // But only if content actually differs (to avoid unnecessary messages)
            return Ok(());
        }

        // Track the document
        self.documents.insert(
            path.to_string(),
            DocumentInfo {
                uri: uri.clone(),
                language_id: language_id.to_string(),
                version: 1,
            },
        );

        // Send didOpen notification
        let notification =
            protocol::create_did_open_notification(&uri, language_id, 1, content);
        self.manager.send_notification(language_id, notification)?;

        Ok(())
    }

    /// Notify the server of document changes
    pub fn document_changed(&mut self, path: &str, content: &str) -> Result<()> {
        let doc = match self.documents.get_mut(path) {
            Some(d) => d,
            None => return Ok(()), // Document not tracked
        };

        doc.version += 1;
        let notification =
            protocol::create_did_change_notification(&doc.uri, doc.version, content);
        self.manager
            .send_notification(&doc.language_id, notification)?;

        Ok(())
    }

    /// Notify the server that a document was saved
    pub fn document_saved(&mut self, path: &str, content: Option<&str>) -> Result<()> {
        let doc = match self.documents.get(path) {
            Some(d) => d,
            None => return Ok(()),
        };

        let notification = protocol::create_did_save_notification(&doc.uri, content);
        self.manager
            .send_notification(&doc.language_id, notification)?;

        Ok(())
    }

    /// Close a document
    pub fn close_document(&mut self, path: &str) -> Result<()> {
        let doc = match self.documents.remove(path) {
            Some(d) => d,
            None => return Ok(()),
        };

        let notification = protocol::create_did_close_notification(&doc.uri);
        self.manager
            .send_notification(&doc.language_id, notification)?;

        // Clear diagnostics for this file
        if let Ok(mut diags) = self.diagnostics.lock() {
            diags.remove(&doc.uri);
        }

        Ok(())
    }

    /// Request completions at a position
    pub fn request_completions(&mut self, path: &str, line: u32, character: u32) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request = protocol::create_completion_request(
            id,
            &doc.uri,
            Position::new(line, character),
        );

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => {
                        LspResponse::Completions(req_id, protocol::parse_completion_items(&value))
                    }
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request hover information at a position
    pub fn request_hover(&mut self, path: &str, line: u32, character: u32) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request =
            protocol::create_hover_request(id, &doc.uri, Position::new(line, character));

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => LspResponse::Hover(req_id, protocol::parse_hover(&value)),
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request go-to-definition at a position
    pub fn request_definition(&mut self, path: &str, line: u32, character: u32) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request =
            protocol::create_definition_request(id, &doc.uri, Position::new(line, character));

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => LspResponse::Definition(req_id, protocol::parse_locations(&value)),
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request find-references at a position
    pub fn request_references(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
        include_declaration: bool,
    ) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request = protocol::create_references_request(
            id,
            &doc.uri,
            Position::new(line, character),
            include_declaration,
        );

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => LspResponse::References(req_id, protocol::parse_locations(&value)),
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request document symbols
    pub fn request_document_symbols(&mut self, path: &str) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request = protocol::create_document_symbols_request(id, &doc.uri);

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => {
                        LspResponse::Symbols(req_id, protocol::parse_document_symbols(&value))
                    }
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request document formatting
    pub fn request_formatting(&mut self, path: &str, tab_size: u32, use_spaces: bool) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request = protocol::create_formatting_request(id, &doc.uri, tab_size, use_spaces);

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => {
                        LspResponse::Formatting(req_id, protocol::parse_text_edits(&value))
                    }
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request rename refactoring
    pub fn request_rename(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let request = protocol::create_rename_request(
            id,
            &doc.uri,
            Position::new(line, character),
            new_name,
        );

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => {
                        LspResponse::Rename(req_id, protocol::parse_workspace_edit(&value))
                    }
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Request code actions for a range
    pub fn request_code_actions(
        &mut self,
        path: &str,
        start_line: u32,
        start_char: u32,
        end_line: u32,
        end_char: u32,
    ) -> Result<i64> {
        let doc = self
            .documents
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Document not open: {}", path))?;

        let id = protocol::next_request_id();
        let range = Range::new(
            Position::new(start_line, start_char),
            Position::new(end_line, end_char),
        );
        let request = protocol::create_code_action_request(id, &doc.uri, range);

        let tx = self.response_tx.clone();
        self.manager.send_request(
            &doc.language_id,
            request,
            Box::new(move |req_id, result| {
                let response = match result {
                    Ok(value) => {
                        let actions = parse_code_actions(&value);
                        LspResponse::CodeActions(req_id, actions)
                    }
                    Err(e) => LspResponse::Error(req_id, e.message),
                };
                let _ = tx.send(response);
            }),
        )?;

        Ok(id)
    }

    /// Poll for responses (non-blocking)
    pub fn poll_response(&self) -> Option<LspResponse> {
        self.response_rx.try_recv().ok()
    }

    /// Get diagnostics for a file
    pub fn get_diagnostics(&self, path: &str) -> Vec<Diagnostic> {
        let uri = path_to_uri(path);
        self.diagnostics
            .lock()
            .ok()
            .and_then(|map| map.get(&uri).cloned())
            .unwrap_or_default()
    }

    /// Get all diagnostics
    pub fn get_all_diagnostics(&self) -> HashMap<String, Vec<Diagnostic>> {
        self.diagnostics
            .lock()
            .ok()
            .map(|map| map.clone())
            .unwrap_or_default()
    }

    /// Process pending server messages (call this regularly)
    pub fn process_messages(&mut self) {
        self.manager.process_messages();
    }

    /// Check if LSP is available for a language
    pub fn has_server(&self, language: &str) -> bool {
        self.manager.has_server(language)
    }

    /// Check if LSP is available for a file
    pub fn has_server_for_file(&self, path: &str) -> bool {
        detect_language(path)
            .map(|lang| self.manager.has_server(lang))
            .unwrap_or(false)
    }

    /// Shutdown all servers
    pub fn shutdown(&mut self) {
        self.manager.stop_all();
    }
}

/// Parse code actions from response
fn parse_code_actions(value: &serde_json::Value) -> Vec<CodeAction> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|action| {
                    let title = action.get("title")?.as_str()?.to_string();
                    let kind = action.get("kind").and_then(|v| v.as_str()).map(String::from);
                    let edit = action
                        .get("edit")
                        .map(|e| protocol::parse_workspace_edit(e));
                    let command = action
                        .get("command")
                        .and_then(|c| c.get("command"))
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    Some(CodeAction {
                        title,
                        kind,
                        edit,
                        command,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}
