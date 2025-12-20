//! PTY (pseudo-terminal) management
//!
//! Handles spawning the shell process and I/O with it.

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

/// Manages a PTY connection to a shell process
pub struct Pty {
    pair: PtyPair,
    writer: Box<dyn Write + Send>,
    output_rx: Receiver<Vec<u8>>,
    _output_thread: thread::JoinHandle<()>,
}

impl Pty {
    /// Spawn a new PTY with the user's shell
    pub fn spawn(cols: u16, rows: u16) -> Result<Self> {
        let pty_system = native_pty_system();

        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Get the user's shell from $SHELL, fallback to /bin/sh
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

        let mut cmd = CommandBuilder::new(&shell);
        // Start shell as login shell
        cmd.arg("-l");

        // Set working directory to current directory
        if let Ok(cwd) = std::env::current_dir() {
            cmd.cwd(cwd);
        }

        // Spawn the shell
        let _child = pair.slave.spawn_command(cmd)?;

        // Get writer for sending input to the PTY
        let writer = pair.master.take_writer()?;

        // Set up a thread to read output from the PTY
        let mut reader = pair.master.try_clone_reader()?;
        let (output_tx, output_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();

        let output_thread = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            pair,
            writer,
            output_rx,
            _output_thread: output_thread,
        })
    }

    /// Send input bytes to the PTY
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Read any available output from the PTY (non-blocking)
    pub fn read(&mut self) -> Option<Vec<u8>> {
        // Collect all available output
        let mut output = Vec::new();
        while let Ok(data) = self.output_rx.try_recv() {
            output.extend(data);
        }
        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Resize the PTY
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.pair.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}
