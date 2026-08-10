//! LSP server process management
//!
//! Handles spawning and communicating with language server processes.
//!
//! Note: Some process methods are for planned features.
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;

/// How many stderr lines to keep for diagnosis. The pipe is drained
/// unconditionally (see `spawn_stderr_thread`); this ring only decides how
/// much of it survives long enough to be looked at.
const STDERR_TAIL_LINES: usize = 64;

/// Framing for LSP's `Content-Length` messages.
///
/// **Bytes, not chars.** `Content-Length` counts bytes, and a read from the
/// server's stdout can split a multi-byte character across two reads. Decoding
/// each read on its own therefore both corrupts the message and desynchronizes
/// the framing, so accumulation and framing happen on `Vec<u8>` and only whole
/// messages are decoded.
#[derive(Debug, Default)]
pub struct FrameBuffer {
    buf: Vec<u8>,
}

impl FrameBuffer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Append raw bytes as they arrive from the transport.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Pop the next complete message, or `None` if one has not fully arrived.
    ///
    /// A frame whose body is not valid UTF-8 is still consumed — it is decoded
    /// lossily and handed on, because dropping it would leave the remaining
    /// bytes of a message the sender already framed correctly at the head of
    /// the buffer and desynchronize every message after it.
    pub fn next_message(&mut self) -> Option<String> {
        let header_end = find(&self.buf, b"\r\n\r\n")?;
        // Headers are ASCII by protocol; lossy keeps a malformed one from
        // wedging the stream.
        let header = String::from_utf8_lossy(&self.buf[..header_end]);

        let content_length: usize = header
            .lines()
            .find(|line| line.to_lowercase().starts_with("content-length:"))
            .and_then(|line| line.split(':').nth(1))
            .and_then(|len| len.trim().parse().ok())?;

        let message_start = header_end + 4;
        let message_end = message_start + content_length;

        if self.buf.len() < message_end {
            return None;
        }

        let message = String::from_utf8_lossy(&self.buf[message_start..message_end]).into_owned();
        self.buf.drain(..message_end);

        Some(message)
    }
}

/// Index of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A running language server process
pub struct ServerProcess {
    child: Child,
    stdin: ChildStdin,
    message_rx: Receiver<Vec<u8>>,
    /// Buffer for incomplete messages
    frames: FrameBuffer,
    /// Last few stderr lines, for diagnosing a server that failed to start
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
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
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("No stderr"))?;

        // Spawn a thread to read from stdout asynchronously
        let (tx, rx) = mpsc::channel();
        spawn_reader_thread(stdout, tx);

        // stderr is piped, so it must also be *read*: a chatty server fills the
        // pipe buffer (~64 KiB) and then blocks forever on its next write.
        let stderr_tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
        spawn_stderr_thread(stderr, Arc::clone(&stderr_tail));

        Ok(Self {
            child,
            stdin,
            message_rx: rx,
            frames: FrameBuffer::new(),
            stderr_tail,
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
                Ok(data) => self.frames.push(&data),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }

        // Try to parse a complete message from the buffer
        self.frames.next_message()
    }

    /// Block until a message is received (with timeout in ms)
    pub fn recv_timeout(&mut self, timeout_ms: u64) -> Option<String> {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        loop {
            // First check if we have a complete message buffered
            if let Some(msg) = self.frames.next_message() {
                return Some(msg);
            }

            // Wait for more data
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }

            match self.message_rx.recv_timeout(remaining) {
                Ok(data) => self.frames.push(&data),
                Err(_) => return None,
            }
        }
    }

    /// The most recent stderr lines the server produced.
    ///
    /// A server that dies during `initialize` says why here and nowhere else.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail
            .lock()
            .map(|tail| tail.iter().cloned().collect())
            .unwrap_or_default()
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
fn spawn_reader_thread(mut stdout: ChildStdout, tx: Sender<Vec<u8>>) {
    use std::io::ErrorKind;

    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    // Send bytes: a character may straddle this boundary and
                    // only the framing layer knows where the message ends.
                    if tx.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });
}

/// Spawn a thread to drain the server's stderr, keeping the tail.
fn spawn_stderr_thread(stderr: ChildStderr, tail: Arc<Mutex<VecDeque<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if let Ok(mut tail) = tail.lock() {
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(body: &str) -> Vec<u8> {
        let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
        bytes.extend_from_slice(body.as_bytes());
        bytes
    }

    #[test]
    fn frames_a_whole_message() {
        let mut fb = FrameBuffer::new();
        fb.push(&frame(r#"{"jsonrpc":"2.0"}"#));
        assert_eq!(fb.next_message().as_deref(), Some(r#"{"jsonrpc":"2.0"}"#));
        assert!(fb.next_message().is_none());
        assert!(fb.is_empty());
    }

    #[test]
    fn frames_two_messages_from_one_read() {
        let mut fb = FrameBuffer::new();
        let mut bytes = frame(r#"{"id":1}"#);
        bytes.extend_from_slice(&frame(r#"{"id":2}"#));
        fb.push(&bytes);
        assert_eq!(fb.next_message().as_deref(), Some(r#"{"id":1}"#));
        assert_eq!(fb.next_message().as_deref(), Some(r#"{"id":2}"#));
        assert!(fb.next_message().is_none());
    }

    #[test]
    fn waits_for_the_rest_of_a_split_header() {
        let mut fb = FrameBuffer::new();
        let bytes = frame(r#"{"id":1}"#);
        let (head, rest) = bytes.split_at(10);
        fb.push(head);
        assert!(fb.next_message().is_none());
        fb.push(rest);
        assert_eq!(fb.next_message().as_deref(), Some(r#"{"id":1}"#));
    }

    /// The regression this rewrite exists for. Diagnostics are not ASCII, so
    /// a read boundary lands inside a multi-byte character sooner or later;
    /// decoding per-read dropped the entire read and desynced the stream.
    #[test]
    fn survives_a_character_split_across_reads() {
        let body = r#"{"message":"expected ’…’ here 🐺"}"#;
        let bytes = frame(body);

        // Split at every byte offset: each one is a plausible read boundary.
        for cut in 1..bytes.len() {
            let mut fb = FrameBuffer::new();
            fb.push(&bytes[..cut]);
            fb.push(&bytes[cut..]);
            assert_eq!(
                fb.next_message().as_deref(),
                Some(body),
                "message lost when the read boundary fell at byte {cut}"
            );
        }
    }

    /// `Content-Length` counts bytes; a non-ASCII body must not be truncated
    /// or over-read because someone counted characters.
    #[test]
    fn content_length_is_bytes_not_chars() {
        let body = r#"{"m":"🐺"}"#;
        assert_ne!(body.len(), body.chars().count());
        let mut fb = FrameBuffer::new();
        let mut bytes = frame(body);
        bytes.extend_from_slice(&frame(r#"{"m":"next"}"#));
        fb.push(&bytes);
        assert_eq!(fb.next_message().as_deref(), Some(body));
        assert_eq!(fb.next_message().as_deref(), Some(r#"{"m":"next"}"#));
    }

    #[test]
    fn header_name_is_case_insensitive() {
        let mut fb = FrameBuffer::new();
        fb.push(b"content-length: 2\r\n\r\n{}");
        assert_eq!(fb.next_message().as_deref(), Some("{}"));
    }

    #[test]
    fn extra_headers_are_ignored() {
        let mut fb = FrameBuffer::new();
        fb.push(
            b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\nContent-Length: 2\r\n\r\n{}",
        );
        assert_eq!(fb.next_message().as_deref(), Some("{}"));
    }
}
