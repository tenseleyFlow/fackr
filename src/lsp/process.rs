//! LSP server process management
//!
//! Handles spawning and communicating with language server processes.
//!
//! Note: Some process methods are for planned features.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;

/// A running language server process
pub struct ServerProcess {
    child: Child,
    stdin: ChildStdin,
    message_rx: Receiver<String>,
    /// Buffer for incomplete messages
    read_buffer: String,
}

impl ServerProcess {
    /// Spawn a new language server process
    pub fn spawn(command: &[String]) -> Result<Self> {
        if command.is_empty() {
            return Err(anyhow!("Empty command"));
        }

        let mut cmd = Command::new(&command[0]);
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn LSP server '{}': {}", command[0], e))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("No stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;

        // Spawn a thread to read from stdout asynchronously
        let (tx, rx) = mpsc::channel();
        spawn_reader_thread(stdout, tx);

        Ok(Self {
            child,
            stdin,
            message_rx: rx,
            read_buffer: String::new(),
        })
    }

    /// Send a message to the server
    pub fn send(&mut self, message: &str) -> Result<()> {
        self.stdin.write_all(message.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Try to receive a complete message from the server (non-blocking)
    pub fn try_recv(&mut self) -> Option<String> {
        // Drain all available data from the channel into our buffer
        loop {
            match self.message_rx.try_recv() {
                Ok(data) => self.read_buffer.push_str(&data),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        // Try to parse a complete message from the buffer
        self.parse_message()
    }

    /// Block until a message is received (with timeout in ms)
    pub fn recv_timeout(&mut self, timeout_ms: u64) -> Option<String> {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        loop {
            // First check if we have a complete message buffered
            if let Some(msg) = self.parse_message() {
                return Some(msg);
            }

            // Wait for more data
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }

            match self.message_rx.recv_timeout(remaining) {
                Ok(data) => self.read_buffer.push_str(&data),
                Err(_) => return None,
            }
        }
    }

    /// Parse a complete LSP message from the buffer
    fn parse_message(&mut self) -> Option<String> {
        // Look for Content-Length header
        let header_end = self.read_buffer.find("\r\n\r\n")?;
        let header = &self.read_buffer[..header_end];

        // Parse Content-Length
        let content_length: usize = header
            .lines()
            .find(|line| line.to_lowercase().starts_with("content-length:"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|len| len.trim().parse().ok())?;

        // Check if we have the full message
        let message_start = header_end + 4;
        let message_end = message_start + content_length;

        if self.read_buffer.len() < message_end {
            return None;
        }

        // Extract the message
        let message = self.read_buffer[message_start..message_end].to_string();

        // Remove from buffer
        self.read_buffer = self.read_buffer[message_end..].to_string();

        Some(message)
    }

    /// Check if the process is still running
    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => false, // Process has exited
            Ok(None) => true,     // Still running
            Err(_) => false,      // Error checking status
        }
    }

    /// Kill the server process
    pub fn kill(&mut self) -> Result<()> {
        let _ = self.child.kill();
        Ok(())
    }

    /// Get the process ID
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

/// Spawn a thread to read from the server's stdout
fn spawn_reader_thread(mut stdout: ChildStdout, tx: Sender<String>) {
    use std::io::ErrorKind;

    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(s) = std::str::from_utf8(&buffer[..n]) {
                        if tx.send(s.to_string()).is_err() {
                            break;
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });
}
