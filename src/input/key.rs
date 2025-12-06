use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Key modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl From<KeyModifiers> for Modifiers {
    fn from(m: KeyModifiers) -> Self {
        Self {
            ctrl: m.contains(KeyModifiers::CONTROL),
            alt: m.contains(KeyModifiers::ALT),
            shift: m.contains(KeyModifiers::SHIFT),
        }
    }
}

/// Abstracted key input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Null,
}

impl Key {
    pub fn from_crossterm(event: KeyEvent) -> (Self, Modifiers) {
        let modifiers = Modifiers::from(event.modifiers);
        let key = match event.code {
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Delete => Key::Delete,
            KeyCode::Enter => Key::Enter,
            KeyCode::Tab => Key::Tab,
            KeyCode::Esc => Key::Escape,
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,
            KeyCode::F(n) => Key::F(n),
            KeyCode::Null => Key::Null,
            _ => Key::Null,
        };
        (key, modifiers)
    }
}
