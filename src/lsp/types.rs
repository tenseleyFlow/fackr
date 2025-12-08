//! LSP type definitions
//!
//! Core types used throughout the LSP client implementation.
//!
//! Note: Some types and methods are for planned features.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;

/// Position in a document (0-based line and character)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Range in a document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn point(pos: Position) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }
}

/// Location in a document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

impl Location {
    /// Convert URI to file path
    pub fn to_path(&self) -> Option<PathBuf> {
        if self.uri.starts_with("file://") {
            Some(PathBuf::from(&self.uri[7..]))
        } else {
            None
        }
    }
}

/// Text edit operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// Workspace edit (multiple file edits)
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub changes: HashMap<String, Vec<TextEdit>>,
}

/// Diagnostic severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl DiagnosticSeverity {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Error),
            2 => Some(Self::Warning),
            3 => Some(Self::Information),
            4 => Some(Self::Hint),
            _ => None,
        }
    }
}

/// A diagnostic message from the language server
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: Option<DiagnosticSeverity>,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

/// Completion item kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

impl CompletionItemKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Text),
            2 => Some(Self::Method),
            3 => Some(Self::Function),
            4 => Some(Self::Constructor),
            5 => Some(Self::Field),
            6 => Some(Self::Variable),
            7 => Some(Self::Class),
            8 => Some(Self::Interface),
            9 => Some(Self::Module),
            10 => Some(Self::Property),
            11 => Some(Self::Unit),
            12 => Some(Self::Value),
            13 => Some(Self::Enum),
            14 => Some(Self::Keyword),
            15 => Some(Self::Snippet),
            16 => Some(Self::Color),
            17 => Some(Self::File),
            18 => Some(Self::Reference),
            19 => Some(Self::Folder),
            20 => Some(Self::EnumMember),
            21 => Some(Self::Constant),
            22 => Some(Self::Struct),
            23 => Some(Self::Event),
            24 => Some(Self::Operator),
            25 => Some(Self::TypeParameter),
            _ => None,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Text => "t",
            Self::Method => "m",
            Self::Function => "f",
            Self::Constructor => "C",
            Self::Field => "F",
            Self::Variable => "v",
            Self::Class => "c",
            Self::Interface => "i",
            Self::Module => "M",
            Self::Property => "p",
            Self::Unit => "u",
            Self::Value => "V",
            Self::Enum => "E",
            Self::Keyword => "k",
            Self::Snippet => "s",
            Self::Color => "#",
            Self::File => "f",
            Self::Reference => "r",
            Self::Folder => "D",
            Self::EnumMember => "e",
            Self::Constant => "K",
            Self::Struct => "S",
            Self::Event => "!",
            Self::Operator => "o",
            Self::TypeParameter => "T",
        }
    }
}

/// A completion item
#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub kind: Option<CompletionItemKind>,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    pub text_edit: Option<TextEdit>,
    pub sort_text: Option<String>,
    pub filter_text: Option<String>,
}

/// Symbol kind (for document/workspace symbols)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

impl SymbolKind {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::File),
            2 => Some(Self::Module),
            3 => Some(Self::Namespace),
            4 => Some(Self::Package),
            5 => Some(Self::Class),
            6 => Some(Self::Method),
            7 => Some(Self::Property),
            8 => Some(Self::Field),
            9 => Some(Self::Constructor),
            10 => Some(Self::Enum),
            11 => Some(Self::Interface),
            12 => Some(Self::Function),
            13 => Some(Self::Variable),
            14 => Some(Self::Constant),
            15 => Some(Self::String),
            16 => Some(Self::Number),
            17 => Some(Self::Boolean),
            18 => Some(Self::Array),
            19 => Some(Self::Object),
            20 => Some(Self::Key),
            21 => Some(Self::Null),
            22 => Some(Self::EnumMember),
            23 => Some(Self::Struct),
            24 => Some(Self::Event),
            25 => Some(Self::Operator),
            26 => Some(Self::TypeParameter),
            _ => None,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::File => "󰈔",
            Self::Module => "󰆧",
            Self::Namespace => "󰅩",
            Self::Package => "󰏗",
            Self::Class => "󰠱",
            Self::Method => "󰆧",
            Self::Property => "󰜢",
            Self::Field => "󰜢",
            Self::Constructor => "󰆧",
            Self::Enum => "󰒻",
            Self::Interface => "󰜰",
            Self::Function => "󰊕",
            Self::Variable => "󰀫",
            Self::Constant => "󰏿",
            Self::String => "󰀬",
            Self::Number => "󰎠",
            Self::Boolean => "󰨙",
            Self::Array => "󰅪",
            Self::Object => "󰅩",
            Self::Key => "󰌋",
            Self::Null => "󰟢",
            Self::EnumMember => "󰒻",
            Self::Struct => "󰙅",
            Self::Event => "󰉁",
            Self::Operator => "󰆕",
            Self::TypeParameter => "󰊄",
        }
    }
}

/// A document symbol
#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

/// Hover information
#[derive(Debug, Clone)]
pub struct HoverInfo {
    pub contents: String,
    pub range: Option<Range>,
}

/// Server capabilities
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub completion: bool,
    pub hover: bool,
    pub definition: bool,
    pub references: bool,
    pub rename: bool,
    pub code_actions: bool,
    pub formatting: bool,
    pub diagnostics: bool,
    pub document_symbols: bool,
    pub workspace_symbols: bool,
    pub signature_help: bool,
}

impl Capabilities {
    pub fn all() -> Self {
        Self {
            completion: true,
            hover: true,
            definition: true,
            references: true,
            rename: true,
            code_actions: true,
            formatting: true,
            diagnostics: true,
            document_symbols: true,
            workspace_symbols: true,
            signature_help: true,
        }
    }
}

/// Configuration for an LSP server
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub language: String,
    pub command: Vec<String>,
    pub file_patterns: Vec<String>,
    pub capabilities: Capabilities,
}

impl ServerConfig {
    pub fn new(name: &str, language: &str, command: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            language: language.to_string(),
            command: command.into_iter().map(String::from).collect(),
            file_patterns: Vec::new(),
            capabilities: Capabilities::all(),
        }
    }

    pub fn with_patterns(mut self, patterns: Vec<&str>) -> Self {
        self.file_patterns = patterns.into_iter().map(String::from).collect();
        self
    }

    pub fn with_capabilities(mut self, caps: Capabilities) -> Self {
        self.capabilities = caps;
        self
    }
}

/// Language ID detection from file extension
pub fn detect_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext.to_lowercase().as_str() {
        "rs" => Some("rust"),
        "py" | "pyw" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("typescriptreact"),
        "jsx" => Some("javascriptreact"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        "go" => Some("go"),
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "swift" => Some("swift"),
        "rb" | "erb" => Some("ruby"),
        "php" => Some("php"),
        "cs" => Some("csharp"),
        "fs" | "fsi" | "fsx" => Some("fsharp"),
        "scala" | "sc" => Some("scala"),
        "hs" | "lhs" => Some("haskell"),
        "lua" => Some("lua"),
        "pl" | "pm" => Some("perl"),
        "r" | "R" => Some("r"),
        "jl" => Some("julia"),
        "ex" | "exs" => Some("elixir"),
        "erl" | "hrl" => Some("erlang"),
        "clj" | "cljs" | "cljc" => Some("clojure"),
        "f90" | "f95" | "f03" | "f08" | "for" | "ftn" => Some("fortran"),
        "zig" => Some("zig"),
        "nim" => Some("nim"),
        "odin" => Some("odin"),
        "v" => Some("v"),
        "d" => Some("d"),
        "sh" | "bash" => Some("shellscript"),
        "zsh" => Some("shellscript"),
        "fish" => Some("fish"),
        "ps1" | "psm1" => Some("powershell"),
        "sql" => Some("sql"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" => Some("scss"),
        "less" => Some("less"),
        "json" => Some("json"),
        "jsonc" => Some("jsonc"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        "xml" => Some("xml"),
        "md" | "markdown" => Some("markdown"),
        "dockerfile" => Some("dockerfile"),
        "tf" | "tfvars" => Some("terraform"),
        "nix" => Some("nix"),
        "ml" | "mli" => Some("ocaml"),
        "dart" => Some("dart"),
        "groovy" | "gradle" => Some("groovy"),
        "vue" => Some("vue"),
        "svelte" => Some("svelte"),
        "elm" => Some("elm"),
        "asm" | "s" => Some("asm"),
        "cmake" => Some("cmake"),
        "proto" => Some("proto"),
        "graphql" | "gql" => Some("graphql"),
        _ => None,
    }
}

/// Convert file path to LSP URI
pub fn path_to_uri(path: &str) -> String {
    if path.starts_with('/') {
        format!("file://{}", path)
    } else {
        format!("file:///{}", path)
    }
}

/// Convert LSP URI to file path
pub fn uri_to_path(uri: &str) -> Option<String> {
    if uri.starts_with("file://") {
        Some(uri[7..].to_string())
    } else {
        None
    }
}
