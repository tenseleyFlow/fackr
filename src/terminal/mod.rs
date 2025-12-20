//! Integrated terminal panel
//!
//! Provides an embedded terminal emulator that can be toggled with Ctrl+`

mod panel;
mod pty;
mod screen;

pub use panel::TerminalPanel;
