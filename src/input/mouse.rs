//! Mouse event handling

#![allow(dead_code)]

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// Mouse button types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

/// Modifiers held during mouse event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl From<KeyModifiers> for MouseModifiers {
    fn from(m: KeyModifiers) -> Self {
        Self {
            ctrl: m.contains(KeyModifiers::CONTROL),
            alt: m.contains(KeyModifiers::ALT),
            shift: m.contains(KeyModifiers::SHIFT),
        }
    }
}

/// Abstracted mouse event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    /// Click at (column, row) - 0-indexed
    Click { button: Button, col: u16, row: u16, modifiers: MouseModifiers },
    /// Drag to (column, row)
    Drag { button: Button, col: u16, row: u16, modifiers: MouseModifiers },
    /// Scroll up at (column, row)
    ScrollUp { col: u16, row: u16 },
    /// Scroll down at (column, row)
    ScrollDown { col: u16, row: u16 },
}

impl Mouse {
    pub fn from_crossterm(event: MouseEvent) -> Option<Self> {
        let col = event.column;
        let row = event.row;
        let modifiers = MouseModifiers::from(event.modifiers);

        match event.kind {
            MouseEventKind::Down(button) => {
                let button = match button {
                    MouseButton::Left => Button::Left,
                    MouseButton::Right => Button::Right,
                    MouseButton::Middle => Button::Middle,
                };
                Some(Mouse::Click { button, col, row, modifiers })
            }
            MouseEventKind::Drag(button) => {
                let button = match button {
                    MouseButton::Left => Button::Left,
                    MouseButton::Right => Button::Right,
                    MouseButton::Middle => Button::Middle,
                };
                Some(Mouse::Drag { button, col, row, modifiers })
            }
            MouseEventKind::ScrollUp => Some(Mouse::ScrollUp { col, row }),
            MouseEventKind::ScrollDown => Some(Mouse::ScrollDown { col, row }),
            _ => None, // Ignore Up, Moved events for now
        }
    }

    /// Get the column position
    pub fn col(&self) -> u16 {
        match self {
            Mouse::Click { col, .. } => *col,
            Mouse::Drag { col, .. } => *col,
            Mouse::ScrollUp { col, .. } => *col,
            Mouse::ScrollDown { col, .. } => *col,
        }
    }

    /// Get the row position
    pub fn row(&self) -> u16 {
        match self {
            Mouse::Click { row, .. } => *row,
            Mouse::Drag { row, .. } => *row,
            Mouse::ScrollUp { row, .. } => *row,
            Mouse::ScrollDown { row, .. } => *row,
        }
    }
}
