//! LSP server manager
//!
//! Manages multiple language server instances, handles initialization,
//! and routes requests to appropriate servers.
//!
//! Note: Some methods are planned features not yet wired to the UI.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::message::{DiagnosticsCallback, MessageHandler, ResponseCallback};
use super::process::ServerProcess;
use super::protocol::{self, LspMessage};
use super::types::{Capabilities, ServerConfig};

/// State of a language server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    Starting,
    Initializing,
    Ready,
    ShuttingDown,
    Stopped,
}

/// A managed language server instance
pub struct ManagedServer {
    pub config: ServerConfig,
    pub process: ServerProcess,
    pub state: ServerState,
    pub capabilities: Capabilities,
    pub handler: MessageHandler,
    /// Queued didOpen notifications (for files opened before initialization)
    pending_opens: Vec<LspMessage>,
}

impl ManagedServer {
    fn new(config: ServerConfig, process: ServerProcess) -> Self {
        Self {
            config,
            process,
            state: ServerState::Starting,
            capabilities: Capabilities::default(),
            handler: MessageHandler::new(),
            pending_opens: Vec::new(),
        }
    }
}

/// Manager for multiple language servers
pub struct LspManager {
    /// Workspace root path
    workspace_root: String,
    /// Server configurations by language
    configs: HashMap<String, Vec<ServerConfig>>,
    /// Active servers by language
    servers: HashMap<String, Vec<ManagedServer>>,
    /// Global diagnostics callback
    diagnostics_callback: Option<Arc<Mutex<DiagnosticsCallback>>>,
}

impl LspManager {
    /// Create a new LSP manager
    pub fn new(workspace_root: &str) -> Self {
        let mut manager = Self {
            workspace_root: workspace_root.to_string(),
            configs: HashMap::new(),
            servers: HashMap::new(),
            diagnostics_callback: None,
        };
        manager.register_default_configs();
        manager
    }

    /// Set the global diagnostics callback
    pub fn set_diagnostics_callback<F>(&mut self, callback: F)
    where
        F: Fn(String, Vec<super::types::Diagnostic>) + Send + 'static,
    {
        self.diagnostics_callback = Some(Arc::new(Mutex::new(Box::new(callback))));
    }

    /// Register default server configurations
    fn register_default_configs(&mut self) {
        // Rust - rust-analyzer
        self.register_config(ServerConfig::new("rust-analyzer", "rust", vec!["rust-analyzer"]));

        // Python - pyright and ruff
        self.register_config(ServerConfig::new(
            "pyright",
            "python",
            vec!["pyright-langserver", "--stdio"],
        ));
        self.register_config(
            ServerConfig::new("ruff", "python", vec!["ruff", "server"]).with_capabilities(
                Capabilities {
                    completion: false,
                    hover: false,
                    definition: false,
                    references: false,
                    rename: false,
                    code_actions: true,
                    formatting: true,
                    diagnostics: true,
                    document_symbols: false,
                    workspace_symbols: false,
                    signature_help: false,
                },
            ),
        );

        // TypeScript/JavaScript - typescript-language-server
        self.register_config(ServerConfig::new(
            "typescript-language-server",
            "typescript",
            vec!["typescript-language-server", "--stdio"],
        ));
        self.register_config(ServerConfig::new(
            "typescript-language-server",
            "typescriptreact",
            vec!["typescript-language-server", "--stdio"],
        ));
        self.register_config(ServerConfig::new(
            "typescript-language-server",
            "javascript",
            vec!["typescript-language-server", "--stdio"],
        ));
        self.register_config(ServerConfig::new(
            "typescript-language-server",
            "javascriptreact",
            vec!["typescript-language-server", "--stdio"],
        ));

        // Go - gopls
        self.register_config(ServerConfig::new("gopls", "go", vec!["gopls"]));

        // C/C++ - clangd
        self.register_config(ServerConfig::new("clangd", "c", vec!["clangd"]));
        self.register_config(ServerConfig::new("clangd", "cpp", vec!["clangd"]));

        // Java - jdtls (Eclipse JDT Language Server)
        self.register_config(ServerConfig::new("jdtls", "java", vec!["jdtls"]));

        // Kotlin - kotlin-language-server
        self.register_config(ServerConfig::new(
            "kotlin-language-server",
            "kotlin",
            vec!["kotlin-language-server"],
        ));

        // Ruby - solargraph
        self.register_config(ServerConfig::new(
            "solargraph",
            "ruby",
            vec!["solargraph", "stdio"],
        ));

        // PHP - intelephense
        self.register_config(ServerConfig::new(
            "intelephense",
            "php",
            vec!["intelephense", "--stdio"],
        ));

        // C# - omnisharp
        self.register_config(ServerConfig::new(
            "omnisharp",
            "csharp",
            vec!["omnisharp", "--languageserver"],
        ));

        // Lua - lua-language-server
        self.register_config(ServerConfig::new("lua-ls", "lua", vec!["lua-language-server"]));

        // Zig - zls
        self.register_config(ServerConfig::new("zls", "zig", vec!["zls"]));

        // Haskell - haskell-language-server
        self.register_config(ServerConfig::new(
            "hls",
            "haskell",
            vec!["haskell-language-server-wrapper", "--lsp"],
        ));

        // OCaml - ocamllsp
        self.register_config(ServerConfig::new("ocamllsp", "ocaml", vec!["ocamllsp"]));

        // Elixir - elixir-ls
        self.register_config(ServerConfig::new("elixir-ls", "elixir", vec!["elixir-ls"]));

        // Erlang - erlang_ls
        self.register_config(ServerConfig::new("erlang_ls", "erlang", vec!["erlang_ls"]));

        // Julia - julia-ls
        self.register_config(ServerConfig::new("julia-ls", "julia", vec!["julia", "--project=@.", "-e", "using LanguageServer; runserver()"]));

        // Bash - bash-language-server
        self.register_config(ServerConfig::new(
            "bash-ls",
            "shellscript",
            vec!["bash-language-server", "start"],
        ));

        // HTML - vscode-html-language-server
        self.register_config(ServerConfig::new(
            "html-ls",
            "html",
            vec!["vscode-html-language-server", "--stdio"],
        ));

        // CSS - vscode-css-language-server
        self.register_config(ServerConfig::new(
            "css-ls",
            "css",
            vec!["vscode-css-language-server", "--stdio"],
        ));

        // JSON - vscode-json-language-server
        self.register_config(ServerConfig::new(
            "json-ls",
            "json",
            vec!["vscode-json-language-server", "--stdio"],
        ));

        // YAML - yaml-language-server
        self.register_config(ServerConfig::new(
            "yaml-ls",
            "yaml",
            vec!["yaml-language-server", "--stdio"],
        ));

        // TOML - taplo
        self.register_config(ServerConfig::new("taplo", "toml", vec!["taplo", "lsp", "stdio"]));

        // Markdown - marksman
        self.register_config(ServerConfig::new("marksman", "markdown", vec!["marksman", "server"]));

        // Docker - dockerfile-language-server
        self.register_config(ServerConfig::new(
            "docker-ls",
            "dockerfile",
            vec!["docker-langserver", "--stdio"],
        ));

        // Terraform - terraform-ls
        self.register_config(ServerConfig::new(
            "terraform-ls",
            "terraform",
            vec!["terraform-ls", "serve"],
        ));

        // Nix - nil
        self.register_config(ServerConfig::new("nil", "nix", vec!["nil"]));

        // SQL - sqls
        self.register_config(ServerConfig::new("sqls", "sql", vec!["sqls"]));

        // Vue - volar
        self.register_config(ServerConfig::new(
            "vue-ls",
            "vue",
            vec!["vue-language-server", "--stdio"],
        ));

        // Svelte - svelte-language-server
        self.register_config(ServerConfig::new(
            "svelte-ls",
            "svelte",
            vec!["svelteserver", "--stdio"],
        ));

        // Elm - elm-language-server
        self.register_config(ServerConfig::new("elm-ls", "elm", vec!["elm-language-server"]));

        // Scala - metals
        self.register_config(ServerConfig::new("metals", "scala", vec!["metals"]));

        // Dart - dart analysis server
        self.register_config(ServerConfig::new(
            "dart-ls",
            "dart",
            vec!["dart", "language-server", "--protocol=lsp"],
        ));

        // Clojure - clojure-lsp
        self.register_config(ServerConfig::new("clojure-lsp", "clojure", vec!["clojure-lsp"]));

        // Fortran - fortls
        self.register_config(ServerConfig::new("fortls", "fortran", vec!["fortls"]));

        // D - serve-d
        self.register_config(ServerConfig::new("serve-d", "d", vec!["serve-d"]));

        // Nim - nimlsp
        self.register_config(ServerConfig::new("nimlsp", "nim", vec!["nimlsp"]));

        // V - vls
        self.register_config(ServerConfig::new("vls", "v", vec!["vls"]));

        // Perl - perlnavigator
        self.register_config(ServerConfig::new(
            "perlnavigator",
            "perl",
            vec!["perlnavigator"],
        ));

        // R - languageserver
        self.register_config(ServerConfig::new(
            "r-ls",
            "r",
            vec!["R", "--slave", "-e", "languageserver::run()"],
        ));

        // GraphQL - graphql-lsp
        self.register_config(ServerConfig::new(
            "graphql-lsp",
            "graphql",
            vec!["graphql-lsp", "server", "-m", "stream"],
        ));

        // CMake - cmake-language-server
        self.register_config(ServerConfig::new(
            "cmake-ls",
            "cmake",
            vec!["cmake-language-server"],
        ));

        // Groovy - groovy-language-server
        self.register_config(ServerConfig::new(
            "groovy-ls",
            "groovy",
            vec!["groovy-language-server"],
        ));

        // Swift - sourcekit-lsp
        self.register_config(ServerConfig::new(
            "sourcekit-lsp",
            "swift",
            vec!["sourcekit-lsp"],
        ));

        // F# - fsautocomplete
        self.register_config(ServerConfig::new(
            "fsautocomplete",
            "fsharp",
            vec!["fsautocomplete", "--adaptive-lsp-server-enabled"],
        ));

        // PowerShell - PowerShellEditorServices
        self.register_config(ServerConfig::new(
            "pwsh-ls",
            "powershell",
            vec![
                "pwsh",
                "-NoLogo",
                "-NoProfile",
                "-Command",
                "Import-Module PowerShellEditorServices; Start-EditorServices -HostName 'fackr' -HostProfileId 'fackr' -HostVersion '1.0.0' -Stdio",
            ],
        ));

        // Protocol Buffers - buf
        self.register_config(ServerConfig::new("buf-ls", "proto", vec!["buf", "lsp"]));

        // Assembly - asm-lsp
        self.register_config(ServerConfig::new("asm-lsp", "asm", vec!["asm-lsp"]));

        // Odin - ols
        self.register_config(ServerConfig::new("ols", "odin", vec!["ols"]));
    }

    /// Register a server configuration
    pub fn register_config(&mut self, config: ServerConfig) {
        self.configs
            .entry(config.language.clone())
            .or_default()
            .push(config);
    }

    /// Start a server for a language
    pub fn start_server(&mut self, language: &str) -> Result<()> {
        let configs = self
            .configs
            .get(language)
            .ok_or_else(|| anyhow!("No LSP server configured for language: {}", language))?
            .clone();

        // Start the first available server for the language
        for config in configs {
            match self.start_server_with_config(&config) {
                Ok(()) => return Ok(()),
                Err(_) => {
                    // Server not available, try next one
                    continue;
                }
            }
        }

        Err(anyhow!(
            "Failed to start any LSP server for language: {}",
            language
        ))
    }

    /// Start all configured servers for a language
    pub fn start_all_servers(&mut self, language: &str) -> Result<()> {
        let configs = self
            .configs
            .get(language)
            .ok_or_else(|| anyhow!("No LSP server configured for language: {}", language))?
            .clone();

        let mut started = 0;
        for config in configs {
            match self.start_server_with_config(&config) {
                Ok(()) => started += 1,
                Err(_) => {
                    // Server not available, continue
                }
            }
        }

        if started > 0 {
            Ok(())
        } else {
            Err(anyhow!(
                "Failed to start any LSP server for language: {}",
                language
            ))
        }
    }

    /// Start a server with a specific config
    fn start_server_with_config(&mut self, config: &ServerConfig) -> Result<()> {
        // Check if server is already running
        if let Some(servers) = self.servers.get(&config.language) {
            if servers.iter().any(|s| s.config.name == config.name) {
                return Ok(()); // Already running
            }
        }

        // Spawn the server process
        let process = ServerProcess::spawn(&config.command)?;

        // Create managed server
        let mut server = ManagedServer::new(config.clone(), process);

        // Set up diagnostics callback if configured
        if let Some(ref callback) = self.diagnostics_callback {
            let cb = Arc::clone(callback);
            server.handler.set_diagnostics_callback(Box::new(
                move |uri, diags| {
                    if let Ok(cb) = cb.lock() {
                        cb(uri, diags);
                    }
                },
            ));
        }

        // Send initialize request
        let id = protocol::next_request_id();
        let init_msg = protocol::create_initialize_request(id, &self.workspace_root, "fackr");

        server.process.send(&init_msg.to_string())?;
        server.state = ServerState::Initializing;

        // Store the server
        self.servers
            .entry(config.language.clone())
            .or_default()
            .push(server);

        Ok(())
    }

    /// Get a server for a language (start if needed)
    pub fn get_or_start_server(&mut self, language: &str) -> Result<&mut ManagedServer> {
        if !self.servers.contains_key(language) || self.servers.get(language).map_or(true, |s| s.is_empty()) {
            self.start_server(language)?;
        }

        self.servers
            .get_mut(language)
            .and_then(|servers| servers.first_mut())
            .ok_or_else(|| anyhow!("No server available for language: {}", language))
    }

    /// Get a server with a specific capability
    pub fn get_server_with_capability(
        &mut self,
        language: &str,
        check: impl Fn(&Capabilities) -> bool,
    ) -> Option<&mut ManagedServer> {
        self.servers
            .get_mut(language)?
            .iter_mut()
            .find(|s| s.state == ServerState::Ready && check(&s.capabilities))
    }

    /// Process messages from all servers (call this regularly)
    pub fn process_messages(&mut self) {
        for (_lang, servers) in self.servers.iter_mut() {
            for server in servers.iter_mut() {
                Self::process_server_messages(server, &self.workspace_root);
            }
        }
    }

    /// Process messages for a single server
    fn process_server_messages(server: &mut ManagedServer, _workspace_root: &str) {
        while let Some(json_str) = server.process.try_recv() {
            if let Ok(value) = serde_json::from_str::<Value>(&json_str) {
                if let Some(msg) = LspMessage::from_json(value.clone()) {
                    // Handle initialization response specially
                    if let LspMessage::Response { ref result, .. } = msg {
                        if server.state == ServerState::Initializing {
                            if let Some(result) = result {
                                // Parse capabilities
                                server.capabilities = protocol::parse_capabilities(result);
                                server.state = ServerState::Ready;

                                // Send initialized notification
                                let init_notif = protocol::create_initialized_notification();
                                let _ = server.process.send(&init_notif.to_string());

                                // Send any pending didOpen notifications
                                for pending in server.pending_opens.drain(..) {
                                    let _ = server.process.send(&pending.to_string());
                                }
                            }
                        }
                    }

                    // Let the handler process the message
                    if let Some(response) = server.handler.handle_message(msg) {
                        // Send response back to server
                        let _ = server.process.send(&response.to_string());
                    }
                }
            }
        }
    }

    /// Send a request to a server and register callback
    pub fn send_request(
        &mut self,
        language: &str,
        message: LspMessage,
        callback: ResponseCallback,
    ) -> Result<()> {
        // Clone workspace_root to avoid borrow conflict
        let workspace_root = self.workspace_root.clone();
        let server = self.get_or_start_server(language)?;

        // Wait for server to be ready
        if server.state != ServerState::Ready {
            // Process messages until ready or timeout
            for _ in 0..50 {
                Self::process_server_messages(server, &workspace_root);
                if server.state == ServerState::Ready {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }

        if let LspMessage::Request { id, .. } = &message {
            server.handler.register_callback(*id, callback);
        }

        server.process.send(&message.to_string())?;
        Ok(())
    }

    /// Send a notification to a server
    pub fn send_notification(&mut self, language: &str, message: LspMessage) -> Result<()> {
        let server = self.get_or_start_server(language)?;

        // Queue didOpen if server not ready
        if server.state != ServerState::Ready {
            if let LspMessage::Notification { ref method, .. } = message {
                if method == "textDocument/didOpen" {
                    server.pending_opens.push(message);
                    return Ok(());
                }
            }
        }

        server.process.send(&message.to_string())?;
        Ok(())
    }

    /// Stop a server for a language
    pub fn stop_server(&mut self, language: &str) -> Result<()> {
        if let Some(servers) = self.servers.get_mut(language) {
            for server in servers.iter_mut() {
                server.state = ServerState::ShuttingDown;

                // Send shutdown request
                let id = protocol::next_request_id();
                let shutdown = protocol::create_shutdown_request(id);
                let _ = server.process.send(&shutdown.to_string());

                // Wait briefly for shutdown acknowledgment
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Send exit notification
                let exit = protocol::create_exit_notification();
                let _ = server.process.send(&exit.to_string());

                // Kill the process
                let _ = server.process.kill();
                server.state = ServerState::Stopped;
            }
            servers.clear();
        }
        Ok(())
    }

    /// Stop all servers
    pub fn stop_all(&mut self) {
        let languages: Vec<String> = self.servers.keys().cloned().collect();
        for lang in languages {
            let _ = self.stop_server(&lang);
        }
    }

    /// Check if a server is running for a language
    pub fn has_server(&self, language: &str) -> bool {
        self.servers
            .get(language)
            .map_or(false, |s| !s.is_empty() && s.iter().any(|s| s.state == ServerState::Ready))
    }

    /// Get the workspace root
    pub fn workspace_root(&self) -> &str {
        &self.workspace_root
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}
