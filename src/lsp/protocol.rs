//! LSP JSON-RPC protocol implementation
//!
//! Handles message creation, serialization, and parsing for the Language Server Protocol.
//!
//! Note: Some request builders and parsers are for planned features.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicI64, Ordering};

use super::types::{Capabilities, Position, Range};

/// Global request ID counter
static NEXT_REQUEST_ID: AtomicI64 = AtomicI64::new(1);

/// Get the next unique request ID
pub fn next_request_id() -> i64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// LSP message types
#[derive(Debug, Clone)]
pub enum LspMessage {
    Request {
        id: i64,
        method: String,
        params: Option<Value>,
    },
    Response {
        id: i64,
        result: Option<Value>,
        error: Option<ResponseError>,
    },
    Notification {
        method: String,
        params: Option<Value>,
    },
}

/// LSP response error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

impl LspMessage {
    /// Serialize message to JSON-RPC format with Content-Length header
    pub fn to_string(&self) -> String {
        let json = match self {
            LspMessage::Request { id, method, params } => {
                let mut obj = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                });
                if let Some(p) = params {
                    obj["params"] = p.clone();
                }
                obj
            }
            LspMessage::Response { id, result, error } => {
                let mut obj = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                });
                if let Some(r) = result {
                    obj["result"] = r.clone();
                }
                if let Some(e) = error {
                    obj["error"] = serde_json::to_value(e).unwrap_or(Value::Null);
                }
                obj
            }
            LspMessage::Notification { method, params } => {
                let mut obj = json!({
                    "jsonrpc": "2.0",
                    "method": method,
                });
                if let Some(p) = params {
                    obj["params"] = p.clone();
                }
                obj
            }
        };

        let content = serde_json::to_string(&json).unwrap_or_default();
        format!("Content-Length: {}\r\n\r\n{}", content.len(), content)
    }

    /// Parse a JSON-RPC message from JSON value
    pub fn from_json(value: Value) -> Option<Self> {
        let obj = value.as_object()?;

        // Check for response (has id and result/error)
        if let Some(id) = obj.get("id").and_then(|v| v.as_i64()) {
            if obj.contains_key("method") {
                // Request
                let method = obj.get("method")?.as_str()?.to_string();
                let params = obj.get("params").cloned();
                Some(LspMessage::Request { id, method, params })
            } else {
                // Response
                let result = obj.get("result").cloned();
                let error = obj
                    .get("error")
                    .and_then(|e| serde_json::from_value(e.clone()).ok());
                Some(LspMessage::Response { id, result, error })
            }
        } else if let Some(method) = obj.get("method").and_then(|v| v.as_str()) {
            // Notification (no id)
            let params = obj.get("params").cloned();
            Some(LspMessage::Notification {
                method: method.to_string(),
                params,
            })
        } else {
            None
        }
    }
}

// ============================================================================
// Request Builders
// ============================================================================

/// Create initialize request
pub fn create_initialize_request(
    id: i64,
    workspace_root: &str,
    client_name: &str,
) -> LspMessage {
    let capabilities = json!({
        "textDocument": {
            "completion": {
                "completionItem": {
                    "snippetSupport": false,
                    "documentationFormat": ["plaintext", "markdown"],
                    "deprecatedSupport": true,
                    "labelDetailsSupport": true
                },
                "contextSupport": true
            },
            "hover": {
                "contentFormat": ["plaintext", "markdown"]
            },
            "definition": {
                "linkSupport": true
            },
            "references": {},
            "documentSymbol": {
                "hierarchicalDocumentSymbolSupport": true
            },
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": {
                        "valueSet": [
                            "quickfix",
                            "refactor",
                            "refactor.extract",
                            "refactor.inline",
                            "refactor.rewrite",
                            "source",
                            "source.organizeImports"
                        ]
                    }
                }
            },
            "rename": {
                "prepareSupport": true
            },
            "publishDiagnostics": {
                "relatedInformation": true,
                "tagSupport": {
                    "valueSet": [1, 2]
                }
            },
            "signatureHelp": {
                "signatureInformation": {
                    "documentationFormat": ["plaintext", "markdown"],
                    "parameterInformation": {
                        "labelOffsetSupport": true
                    }
                }
            },
            "formatting": {},
            "synchronization": {
                "didSave": true,
                "willSave": false,
                "willSaveWaitUntil": false
            }
        },
        "workspace": {
            // Note: workspaceFolders must be false for pyright to send diagnostics
            // after didOpen. With true, pyright waits for workspace folder change events.
            "workspaceFolders": false,
            "symbol": {
                "symbolKind": {
                    "valueSet": (1..=26).collect::<Vec<i32>>()
                }
            },
            "applyEdit": true,
            "workspaceEdit": {
                "documentChanges": true
            }
        }
    });

    let params = json!({
        "processId": std::process::id(),
        "clientInfo": {
            "name": client_name,
            "version": env!("CARGO_PKG_VERSION")
        },
        "rootUri": format!("file://{}", workspace_root),
        "rootPath": workspace_root,
        "capabilities": capabilities,
        "workspaceFolders": [{
            "uri": format!("file://{}", workspace_root),
            "name": workspace_root.rsplit('/').next().unwrap_or(workspace_root)
        }]
    });

    LspMessage::Request {
        id,
        method: "initialize".to_string(),
        params: Some(params),
    }
}

/// Create initialized notification (sent after initialize response)
pub fn create_initialized_notification() -> LspMessage {
    LspMessage::Notification {
        method: "initialized".to_string(),
        params: Some(json!({})),
    }
}

/// Create shutdown request
pub fn create_shutdown_request(id: i64) -> LspMessage {
    LspMessage::Request {
        id,
        method: "shutdown".to_string(),
        params: None,
    }
}

/// Create exit notification
pub fn create_exit_notification() -> LspMessage {
    LspMessage::Notification {
        method: "exit".to_string(),
        params: None,
    }
}

// ============================================================================
// Document Synchronization
// ============================================================================

/// Create textDocument/didOpen notification
pub fn create_did_open_notification(uri: &str, language_id: &str, version: i32, text: &str) -> LspMessage {
    LspMessage::Notification {
        method: "textDocument/didOpen".to_string(),
        params: Some(json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": version,
                "text": text
            }
        })),
    }
}

/// Create textDocument/didChange notification (full sync)
pub fn create_did_change_notification(uri: &str, version: i32, text: &str) -> LspMessage {
    LspMessage::Notification {
        method: "textDocument/didChange".to_string(),
        params: Some(json!({
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": [{
                "text": text
            }]
        })),
    }
}

/// Create textDocument/didSave notification
pub fn create_did_save_notification(uri: &str, text: Option<&str>) -> LspMessage {
    let mut params = json!({
        "textDocument": {
            "uri": uri
        }
    });
    if let Some(t) = text {
        params["text"] = json!(t);
    }
    LspMessage::Notification {
        method: "textDocument/didSave".to_string(),
        params: Some(params),
    }
}

/// Create textDocument/didClose notification
pub fn create_did_close_notification(uri: &str) -> LspMessage {
    LspMessage::Notification {
        method: "textDocument/didClose".to_string(),
        params: Some(json!({
            "textDocument": {
                "uri": uri
            }
        })),
    }
}

// ============================================================================
// Language Features
// ============================================================================

fn position_params(uri: &str, pos: Position) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": pos.line, "character": pos.character }
    })
}

/// Create textDocument/completion request
pub fn create_completion_request(id: i64, uri: &str, pos: Position) -> LspMessage {
    let mut params = position_params(uri, pos);
    params["context"] = json!({ "triggerKind": 1 }); // Invoked
    LspMessage::Request {
        id,
        method: "textDocument/completion".to_string(),
        params: Some(params),
    }
}

/// Create textDocument/hover request
pub fn create_hover_request(id: i64, uri: &str, pos: Position) -> LspMessage {
    LspMessage::Request {
        id,
        method: "textDocument/hover".to_string(),
        params: Some(position_params(uri, pos)),
    }
}

/// Create textDocument/definition request
pub fn create_definition_request(id: i64, uri: &str, pos: Position) -> LspMessage {
    LspMessage::Request {
        id,
        method: "textDocument/definition".to_string(),
        params: Some(position_params(uri, pos)),
    }
}

/// Create textDocument/references request
pub fn create_references_request(
    id: i64,
    uri: &str,
    pos: Position,
    include_declaration: bool,
) -> LspMessage {
    let mut params = position_params(uri, pos);
    params["context"] = json!({ "includeDeclaration": include_declaration });
    LspMessage::Request {
        id,
        method: "textDocument/references".to_string(),
        params: Some(params),
    }
}

/// Create textDocument/rename request
pub fn create_rename_request(id: i64, uri: &str, pos: Position, new_name: &str) -> LspMessage {
    let mut params = position_params(uri, pos);
    params["newName"] = json!(new_name);
    LspMessage::Request {
        id,
        method: "textDocument/rename".to_string(),
        params: Some(params),
    }
}

/// Create textDocument/codeAction request
pub fn create_code_action_request(id: i64, uri: &str, range: Range) -> LspMessage {
    LspMessage::Request {
        id,
        method: "textDocument/codeAction".to_string(),
        params: Some(json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character }
            },
            "context": {
                "diagnostics": []
            }
        })),
    }
}

/// Create textDocument/documentSymbol request
pub fn create_document_symbols_request(id: i64, uri: &str) -> LspMessage {
    LspMessage::Request {
        id,
        method: "textDocument/documentSymbol".to_string(),
        params: Some(json!({
            "textDocument": { "uri": uri }
        })),
    }
}

/// Create workspace/symbol request
pub fn create_workspace_symbols_request(id: i64, query: &str) -> LspMessage {
    LspMessage::Request {
        id,
        method: "workspace/symbol".to_string(),
        params: Some(json!({ "query": query })),
    }
}

/// Create textDocument/signatureHelp request
pub fn create_signature_help_request(id: i64, uri: &str, pos: Position) -> LspMessage {
    LspMessage::Request {
        id,
        method: "textDocument/signatureHelp".to_string(),
        params: Some(position_params(uri, pos)),
    }
}

/// Create textDocument/formatting request
pub fn create_formatting_request(id: i64, uri: &str, tab_size: u32, use_spaces: bool) -> LspMessage {
    LspMessage::Request {
        id,
        method: "textDocument/formatting".to_string(),
        params: Some(json!({
            "textDocument": { "uri": uri },
            "options": {
                "tabSize": tab_size,
                "insertSpaces": use_spaces,
                "trimTrailingWhitespace": true,
                "insertFinalNewline": true
            }
        })),
    }
}

// ============================================================================
// Response Parsing
// ============================================================================

/// Parse server capabilities from initialize response
pub fn parse_capabilities(result: &Value) -> Capabilities {
    let caps = result.get("capabilities").unwrap_or(result);

    Capabilities {
        completion: caps.get("completionProvider").is_some(),
        hover: caps.get("hoverProvider").map_or(false, |v| !v.is_null()),
        definition: caps.get("definitionProvider").map_or(false, |v| !v.is_null()),
        references: caps.get("referencesProvider").map_or(false, |v| !v.is_null()),
        rename: caps.get("renameProvider").map_or(false, |v| !v.is_null()),
        code_actions: caps.get("codeActionProvider").map_or(false, |v| !v.is_null()),
        formatting: caps.get("documentFormattingProvider").map_or(false, |v| !v.is_null()),
        diagnostics: true, // Always assume diagnostics are supported
        document_symbols: caps.get("documentSymbolProvider").map_or(false, |v| !v.is_null()),
        workspace_symbols: caps.get("workspaceSymbolProvider").map_or(false, |v| !v.is_null()),
        signature_help: caps.get("signatureHelpProvider").is_some(),
    }
}

/// Parse Position from JSON
pub fn parse_position(value: &Value) -> Option<super::types::Position> {
    Some(super::types::Position {
        line: value.get("line")?.as_u64()? as u32,
        character: value.get("character")?.as_u64()? as u32,
    })
}

/// Parse Range from JSON
pub fn parse_range(value: &Value) -> Option<super::types::Range> {
    Some(super::types::Range {
        start: parse_position(value.get("start")?)?,
        end: parse_position(value.get("end")?)?,
    })
}

/// Parse Location from JSON
pub fn parse_location(value: &Value) -> Option<super::types::Location> {
    Some(super::types::Location {
        uri: value.get("uri")?.as_str()?.to_string(),
        range: parse_range(value.get("range")?)?,
    })
}

/// Parse completion items from response
pub fn parse_completion_items(result: &Value) -> Vec<super::types::CompletionItem> {
    let items = if let Some(arr) = result.as_array() {
        arr
    } else if let Some(arr) = result.get("items").and_then(|v| v.as_array()) {
        arr
    } else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            Some(super::types::CompletionItem {
                label,
                kind: item
                    .get("kind")
                    .and_then(|v| v.as_u64())
                    .and_then(|k| super::types::CompletionItemKind::from_u32(k as u32)),
                detail: item.get("detail").and_then(|v| v.as_str()).map(String::from),
                documentation: item
                    .get("documentation")
                    .and_then(|v| {
                        if let Some(s) = v.as_str() {
                            Some(s.to_string())
                        } else {
                            v.get("value").and_then(|v| v.as_str()).map(String::from)
                        }
                    }),
                insert_text: item.get("insertText").and_then(|v| v.as_str()).map(String::from),
                text_edit: item.get("textEdit").and_then(|te| {
                    Some(super::types::TextEdit {
                        range: parse_range(te.get("range")?)?,
                        new_text: te.get("newText")?.as_str()?.to_string(),
                    })
                }),
                sort_text: item.get("sortText").and_then(|v| v.as_str()).map(String::from),
                filter_text: item.get("filterText").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect()
}

/// Parse hover info from response
pub fn parse_hover(result: &Value) -> Option<super::types::HoverInfo> {
    let contents = result.get("contents")?;
    let text = if let Some(s) = contents.as_str() {
        s.to_string()
    } else if let Some(arr) = contents.as_array() {
        arr.iter()
            .filter_map(|v| {
                if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else {
                    v.get("value").and_then(|v| v.as_str()).map(String::from)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else if let Some(value) = contents.get("value") {
        value.as_str()?.to_string()
    } else {
        return None;
    };

    Some(super::types::HoverInfo {
        contents: text,
        range: result.get("range").and_then(parse_range),
    })
}

/// Parse locations from definition/references response
pub fn parse_locations(result: &Value) -> Vec<super::types::Location> {
    if let Some(loc) = parse_location(result) {
        vec![loc]
    } else if let Some(arr) = result.as_array() {
        arr.iter().filter_map(parse_location).collect()
    } else {
        Vec::new()
    }
}

/// Parse document symbols from response
pub fn parse_document_symbols(result: &Value) -> Vec<super::types::DocumentSymbol> {
    fn parse_symbol(value: &Value) -> Option<super::types::DocumentSymbol> {
        let name = value.get("name")?.as_str()?.to_string();
        let kind = value.get("kind")?.as_u64()?;

        // Handle both DocumentSymbol and SymbolInformation formats
        let (range, selection_range) = if let Some(r) = value.get("range") {
            let range = parse_range(r)?;
            let sel = value.get("selectionRange").and_then(parse_range).unwrap_or(range);
            (range, sel)
        } else if let Some(loc) = value.get("location") {
            let range = parse_range(loc.get("range")?)?;
            (range, range)
        } else {
            return None;
        };

        let children = value
            .get("children")
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().filter_map(parse_symbol).collect())
            .unwrap_or_default();

        Some(super::types::DocumentSymbol {
            name,
            kind: super::types::SymbolKind::from_u32(kind as u32)?,
            range,
            selection_range,
            children,
        })
    }

    result
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_symbol).collect())
        .unwrap_or_default()
}

/// Parse diagnostics from publishDiagnostics notification
pub fn parse_diagnostics(params: &Value) -> (String, Vec<super::types::Diagnostic>) {
    let uri = params
        .get("uri")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let diagnostics = params
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    Some(super::types::Diagnostic {
                        range: parse_range(d.get("range")?)?,
                        severity: d
                            .get("severity")
                            .and_then(|v| v.as_u64())
                            .and_then(|s| super::types::DiagnosticSeverity::from_u32(s as u32)),
                        code: d.get("code").and_then(|v| {
                            if let Some(s) = v.as_str() {
                                Some(s.to_string())
                            } else if let Some(n) = v.as_i64() {
                                Some(n.to_string())
                            } else {
                                None
                            }
                        }),
                        source: d.get("source").and_then(|v| v.as_str()).map(String::from),
                        message: d.get("message")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    (uri, diagnostics)
}

/// Parse text edits from formatting response
pub fn parse_text_edits(result: &Value) -> Vec<super::types::TextEdit> {
    result
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|edit| {
                    Some(super::types::TextEdit {
                        range: parse_range(edit.get("range")?)?,
                        new_text: edit.get("newText")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse workspace edit from rename response
pub fn parse_workspace_edit(result: &Value) -> super::types::WorkspaceEdit {
    let mut edit = super::types::WorkspaceEdit::default();

    if let Some(changes) = result.get("changes").and_then(|v| v.as_object()) {
        for (uri, edits) in changes {
            let text_edits = edits
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            Some(super::types::TextEdit {
                                range: parse_range(e.get("range")?)?,
                                new_text: e.get("newText")?.as_str()?.to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            edit.changes.insert(uri.clone(), text_edits);
        }
    }

    // Handle documentChanges format
    if let Some(doc_changes) = result.get("documentChanges").and_then(|v| v.as_array()) {
        for change in doc_changes {
            if let Some(text_doc) = change.get("textDocument") {
                let uri = text_doc.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                let text_edits = change
                    .get("edits")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                Some(super::types::TextEdit {
                                    range: parse_range(e.get("range")?)?,
                                    new_text: e.get("newText")?.as_str()?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                edit.changes.insert(uri.to_string(), text_edits);
            }
        }
    }

    edit
}
