//! LSP Server Manager
//!
//! Detects available language servers and helps users install them.
//!
//! Note: Some fields are for planned UI features.
#![allow(dead_code)]

use std::collections::HashSet;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// Result of an install operation
pub struct InstallResult {
    pub server_index: usize,
    pub server_name: String,
    pub check_cmd: String,
    pub success: bool,
    pub message: String,
}

/// A known language server with installation instructions
#[derive(Debug, Clone)]
pub struct KnownServer {
    pub name: &'static str,
    pub language: &'static str,
    pub check_cmd: &'static str,
    pub install_cmd: &'static str,
    pub description: &'static str,
    pub is_installed: bool,
}

impl KnownServer {
    const fn new(
        name: &'static str,
        language: &'static str,
        check_cmd: &'static str,
        install_cmd: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            language,
            check_cmd,
            install_cmd,
            description,
            is_installed: false,
        }
    }
}

/// All known language servers
pub fn get_known_servers() -> Vec<KnownServer> {
    vec![
        // Python
        KnownServer::new(
            "pyright",
            "Python",
            "pyright-langserver",
            "pip install pyright",
            "Python type checker and language server",
        ),
        KnownServer::new(
            "ruff",
            "Python",
            "ruff",
            "pip install ruff",
            "Fast Python linter with LSP support",
        ),
        KnownServer::new(
            "pylsp",
            "Python",
            "pylsp",
            "pip install python-lsp-server",
            "Python LSP server with plugin support",
        ),
        // Rust
        KnownServer::new(
            "rust-analyzer",
            "Rust",
            "rust-analyzer",
            "rustup component add rust-analyzer",
            "Rust language server",
        ),
        // C/C++
        KnownServer::new(
            "clangd",
            "C/C++",
            "clangd",
            "# Install via package manager (apt/brew/etc)",
            "C/C++ language server from LLVM",
        ),
        // Go
        KnownServer::new(
            "gopls",
            "Go",
            "gopls",
            "go install golang.org/x/tools/gopls@latest",
            "Go language server",
        ),
        // TypeScript/JavaScript
        KnownServer::new(
            "typescript-language-server",
            "JS/TS",
            "typescript-language-server",
            "npm i -g typescript-language-server typescript",
            "TypeScript and JavaScript language server",
        ),
        KnownServer::new(
            "vtsls",
            "JS/TS",
            "vtsls",
            "npm i -g @vtsls/language-server",
            "Fast TypeScript language server",
        ),
        // Lua
        KnownServer::new(
            "lua-language-server",
            "Lua",
            "lua-language-server",
            "# Install via package manager",
            "Lua language server",
        ),
        // Ruby
        KnownServer::new(
            "solargraph",
            "Ruby",
            "solargraph",
            "gem install solargraph",
            "Ruby language server",
        ),
        // Java
        KnownServer::new(
            "jdtls",
            "Java",
            "jdtls",
            "# Install via package manager",
            "Eclipse JDT Language Server",
        ),
        // HTML/CSS/JSON
        KnownServer::new(
            "vscode-langservers",
            "HTML/CSS/JSON",
            "vscode-html-language-server",
            "npm i -g vscode-langservers-extracted",
            "HTML, CSS, JSON language servers",
        ),
        // Bash
        KnownServer::new(
            "bash-language-server",
            "Bash",
            "bash-language-server",
            "npm i -g bash-language-server",
            "Bash/Shell language server",
        ),
        // YAML
        KnownServer::new(
            "yaml-language-server",
            "YAML",
            "yaml-language-server",
            "npm i -g yaml-language-server",
            "YAML language server",
        ),
        // Docker
        KnownServer::new(
            "dockerfile-langserver",
            "Docker",
            "docker-langserver",
            "npm i -g dockerfile-language-server-nodejs",
            "Dockerfile language server",
        ),
        // Zig
        KnownServer::new(
            "zls",
            "Zig",
            "zls",
            "# Install via package manager or zigtools",
            "Zig language server",
        ),
        // Haskell
        KnownServer::new(
            "haskell-language-server",
            "Haskell",
            "haskell-language-server-wrapper",
            "ghcup install hls",
            "Haskell language server",
        ),
        // Terraform
        KnownServer::new(
            "terraform-ls",
            "Terraform",
            "terraform-ls",
            "# Install from HashiCorp",
            "Terraform language server",
        ),
        // Fortran
        KnownServer::new(
            "fortls",
            "Fortran",
            "fortls",
            "pip install fortls",
            "Fortran language server",
        ),
        // Elixir
        KnownServer::new(
            "elixir-ls",
            "Elixir",
            "elixir-ls",
            "# Download from GitHub releases",
            "Elixir language server",
        ),
        // Markdown
        KnownServer::new(
            "marksman",
            "Markdown",
            "marksman",
            "# Install from GitHub releases or brew install marksman",
            "Markdown language server with wiki-links support",
        ),
        // Kotlin
        KnownServer::new(
            "kotlin-language-server",
            "Kotlin",
            "kotlin-language-server",
            "# Install from GitHub releases",
            "Kotlin language server",
        ),
        // Swift
        KnownServer::new(
            "sourcekit-lsp",
            "Swift",
            "sourcekit-lsp",
            "# Included with Xcode or Swift toolchain",
            "Swift/Objective-C language server",
        ),
        // PHP
        KnownServer::new(
            "intelephense",
            "PHP",
            "intelephense",
            "npm i -g intelephense",
            "PHP language server",
        ),
        // C#
        KnownServer::new(
            "omnisharp",
            "C#",
            "OmniSharp",
            "# Install from GitHub releases",
            "C# language server",
        ),
        // Scala
        KnownServer::new(
            "metals",
            "Scala",
            "metals",
            "# Install via coursier: cs install metals",
            "Scala language server",
        ),
        // OCaml
        KnownServer::new(
            "ocamllsp",
            "OCaml",
            "ocamllsp",
            "opam install ocaml-lsp-server",
            "OCaml language server",
        ),
        // Nim
        KnownServer::new(
            "nimlangserver",
            "Nim",
            "nimlangserver",
            "nimble install nimlangserver",
            "Nim language server",
        ),
        // Julia
        KnownServer::new(
            "julia-lsp",
            "Julia",
            "julia",
            "# Install LanguageServer.jl package in Julia",
            "Julia language server",
        ),
        // Erlang
        KnownServer::new(
            "erlang_ls",
            "Erlang",
            "erlang_ls",
            "# Install from GitHub releases",
            "Erlang language server",
        ),
        // Clojure
        KnownServer::new(
            "clojure-lsp",
            "Clojure",
            "clojure-lsp",
            "# Install from GitHub releases or brew",
            "Clojure language server",
        ),
        // Perl
        KnownServer::new(
            "perlnavigator",
            "Perl",
            "perlnavigator",
            "npm i -g perlnavigator-server",
            "Perl language server",
        ),
        // R
        KnownServer::new(
            "r-languageserver",
            "R",
            "R",
            "# Install languageserver package in R",
            "R language server",
        ),
        // Dart/Flutter
        KnownServer::new(
            "dart-language-server",
            "Dart",
            "dart",
            "# Included with Dart SDK",
            "Dart language server",
        ),
        // Vue
        KnownServer::new(
            "vue-language-server",
            "Vue",
            "vue-language-server",
            "npm i -g @vue/language-server",
            "Vue.js language server",
        ),
        // Svelte
        KnownServer::new(
            "svelte-language-server",
            "Svelte",
            "svelteserver",
            "npm i -g svelte-language-server",
            "Svelte language server",
        ),
        // TOML
        KnownServer::new(
            "taplo",
            "TOML",
            "taplo",
            "cargo install taplo-cli --features lsp",
            "TOML language server",
        ),
        // Nix
        KnownServer::new(
            "nil",
            "Nix",
            "nil",
            "nix profile install nixpkgs#nil",
            "Nix language server",
        ),
        // GraphQL
        KnownServer::new(
            "graphql-lsp",
            "GraphQL",
            "graphql-lsp",
            "npm i -g graphql-language-service-cli",
            "GraphQL language server",
        ),
        // SQL
        KnownServer::new(
            "sqls",
            "SQL",
            "sqls",
            "go install github.com/sqls-server/sqls@latest",
            "SQL language server",
        ),
        // LaTeX
        KnownServer::new(
            "texlab",
            "LaTeX",
            "texlab",
            "# Install from GitHub releases or cargo install texlab",
            "LaTeX language server",
        ),
        // CMake
        KnownServer::new(
            "cmake-language-server",
            "CMake",
            "cmake-language-server",
            "pip install cmake-language-server",
            "CMake language server",
        ),
        // D
        KnownServer::new(
            "serve-d",
            "D",
            "serve-d",
            "# Install from GitHub releases",
            "D language server",
        ),
        // V
        KnownServer::new(
            "v-analyzer",
            "V",
            "v-analyzer",
            "# Install from GitHub releases",
            "V language server",
        ),
        // Odin
        KnownServer::new(
            "ols",
            "Odin",
            "ols",
            "# Install from GitHub releases",
            "Odin language server",
        ),
        // F#
        KnownServer::new(
            "fsautocomplete",
            "F#",
            "fsautocomplete",
            "dotnet tool install -g fsautocomplete",
            "F# language server",
        ),
        // Groovy
        KnownServer::new(
            "groovy-language-server",
            "Groovy",
            "groovy-language-server",
            "# Install from GitHub releases",
            "Groovy language server",
        ),
    ]
}

/// Check if a command exists in PATH
pub fn check_command_exists(cmd: &str) -> bool {
    if cmd.is_empty() {
        return false;
    }

    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detect which servers are installed
pub fn detect_installed_servers() -> Vec<KnownServer> {
    let mut servers = get_known_servers();
    for server in &mut servers {
        server.is_installed = check_command_exists(server.check_cmd);
    }
    servers
}

/// Sanitize a string for display - remove ANSI escape codes and control characters
fn sanitize_for_display(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        // Skip ANSI escape sequences (ESC [ ... m and similar)
        if c == '\x1b' {
            // Skip until we hit a letter (end of escape sequence)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }

        // Skip other control characters except space
        if c.is_control() {
            if c == '\n' || c == '\t' {
                result.push(' '); // Replace newlines/tabs with space
            }
            continue;
        }

        result.push(c);
    }

    // Collapse multiple spaces
    let mut prev_space = false;
    let collapsed: String = result.chars().filter(|&c| {
        if c == ' ' {
            if prev_space {
                return false;
            }
            prev_space = true;
        } else {
            prev_space = false;
        }
        true
    }).collect();

    collapsed.trim().to_string()
}

/// Run an install command
pub fn run_install_command(cmd: &str) -> Result<String, String> {
    // Skip comments
    if cmd.starts_with('#') {
        return Err("Manual installation required. See instructions.".to_string());
    }

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|e| format!("Failed to run command: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(sanitize_for_display(&stderr))
    }
}

/// LSP Server Manager Panel state
pub struct ServerManagerPanel {
    pub visible: bool,
    pub servers: Vec<KnownServer>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub confirm_mode: bool,
    pub confirm_index: usize,
    /// Set of server indices currently being installed
    pub installing_indices: HashSet<usize>,
    pub status_message: Option<String>,
    /// Show manual install info dialog
    pub manual_info_mode: bool,
    pub manual_info_index: usize,
    /// Text that was copied to clipboard (for status message)
    pub copied_to_clipboard: bool,
    /// Channel to receive install completion results
    install_rx: Option<Receiver<InstallResult>>,
    install_tx: Option<Sender<InstallResult>>,
}

impl Default for ServerManagerPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerManagerPanel {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            visible: false,
            servers: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            confirm_mode: false,
            confirm_index: 0,
            installing_indices: HashSet::new(),
            status_message: None,
            manual_info_mode: false,
            manual_info_index: 0,
            copied_to_clipboard: false,
            install_rx: Some(rx),
            install_tx: Some(tx),
        }
    }

    /// Check if a specific server is currently being installed
    pub fn is_installing(&self, index: usize) -> bool {
        self.installing_indices.contains(&index)
    }

    /// Check if any installs are in progress
    pub fn has_active_installs(&self) -> bool {
        !self.installing_indices.is_empty()
    }

    /// Poll for completed installs (non-blocking)
    /// Returns true if there was an update (caller should re-render)
    pub fn poll_installs(&mut self) -> bool {
        let rx = match &self.install_rx {
            Some(rx) => rx,
            None => return false,
        };

        let mut had_update = false;

        // Drain all available results
        while let Ok(result) = rx.try_recv() {
            had_update = true;
            self.installing_indices.remove(&result.server_index);

            if result.success {
                // Update the server's installed status
                if let Some(server) = self.servers.get_mut(result.server_index) {
                    // Re-check if it's actually installed now
                    server.is_installed = check_command_exists(&result.check_cmd);
                    if server.is_installed {
                        self.status_message = Some(format!("✓ {} installed successfully", result.server_name));
                    } else {
                        self.status_message = Some(format!("Installed {} (may need shell restart)", result.server_name));
                    }
                }
            } else {
                self.status_message = Some(result.message);
            }
        }

        had_update
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.confirm_mode = false;
        self.manual_info_mode = false;
        self.status_message = None;
        self.copied_to_clipboard = false;

        // Detect servers if not already done
        if self.servers.is_empty() {
            self.refresh();
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.confirm_mode = false;
        self.manual_info_mode = false;
    }

    pub fn refresh(&mut self) {
        self.servers = detect_installed_servers();
        self.status_message = Some("Server status refreshed".to_string());
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_down(&mut self, max_visible: usize) {
        if self.selected_index < self.servers.len().saturating_sub(1) {
            self.selected_index += 1;
            if self.selected_index >= self.scroll_offset + max_visible {
                self.scroll_offset = self.selected_index - max_visible + 1;
            }
        }
    }

    pub fn enter_confirm_mode(&mut self) {
        if self.selected_index < self.servers.len() {
            let server = &self.servers[self.selected_index];
            if server.is_installed {
                self.status_message = Some(format!("{} is already installed", server.name));
            } else if server.install_cmd.starts_with('#') {
                // Show manual install info dialog
                self.manual_info_mode = true;
                self.manual_info_index = self.selected_index;
                self.copied_to_clipboard = false;
            } else {
                self.confirm_mode = true;
                self.confirm_index = self.selected_index;
            }
        }
    }

    pub fn cancel_confirm(&mut self) {
        self.confirm_mode = false;
        self.manual_info_mode = false;
        self.status_message = None;
        self.copied_to_clipboard = false;
    }

    /// Get the manual install info for clipboard
    pub fn get_manual_install_text(&self) -> Option<String> {
        self.servers.get(self.manual_info_index).map(|s| {
            // Remove the leading "# " from the install command
            let cmd = s.install_cmd.trim_start_matches('#').trim();
            format!("{} - {}\n{}", s.name, s.language, cmd)
        })
    }

    /// Mark that text was copied to clipboard
    pub fn mark_copied(&mut self) {
        self.copied_to_clipboard = true;
        self.status_message = Some("Copied to clipboard".to_string());
    }

    /// Get the server for manual info dialog
    pub fn manual_info_server(&self) -> Option<&KnownServer> {
        self.servers.get(self.manual_info_index)
    }

    /// Start the install process - spawns a background thread
    pub fn start_install(&mut self) {
        if self.confirm_index >= self.servers.len() {
            self.confirm_mode = false;
            return;
        }

        // Don't allow installing the same server twice
        if self.installing_indices.contains(&self.confirm_index) {
            self.status_message = Some("Already installing...".to_string());
            self.confirm_mode = false;
            return;
        }

        let server = &self.servers[self.confirm_index];
        let name = server.name.to_string();
        let cmd = server.install_cmd.to_string();
        let check_cmd = server.check_cmd.to_string();
        let server_index = self.confirm_index;

        // Mark as installing
        self.installing_indices.insert(server_index);
        self.confirm_mode = false;
        self.status_message = Some(format!("Installing {}...", name));

        // Clone the sender for the thread
        let tx = match &self.install_tx {
            Some(tx) => tx.clone(),
            None => return,
        };

        // Spawn install thread
        thread::spawn(move || {
            let result = run_install_command(&cmd);
            let (success, message) = match &result {
                Ok(_) => (true, String::new()),
                Err(e) => {
                    let err_msg = if e.len() > 50 { format!("{}...", &e[..50]) } else { e.clone() };
                    (false, format!("Install failed: {}", err_msg))
                }
            };

            let _ = tx.send(InstallResult {
                server_index,
                server_name: name,
                check_cmd,
                success,
                message,
            });
        });
    }

    pub fn selected_server(&self) -> Option<&KnownServer> {
        self.servers.get(self.selected_index)
    }

    pub fn confirm_server(&self) -> Option<&KnownServer> {
        self.servers.get(self.confirm_index)
    }
}
