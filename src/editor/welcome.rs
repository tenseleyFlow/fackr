//! Welcome menu for workspace selection
//!
//! Displays when fackr is launched without arguments, allowing the user to:
//! - Select the current directory as workspace
//! - Choose from recently opened workspaces
//! - Browse for a directory

use anyhow::Result;
use crossterm::event::{self, Event};
use std::path::PathBuf;

use crate::input::{Key, Modifiers};
use crate::render::Screen;
use crate::workspace::{recents_get, Recent};

/// Result of the welcome menu interaction
#[derive(Debug)]
pub enum WelcomeResult {
    /// User selected a workspace
    Selected(PathBuf),
    /// User quit without selecting
    Quit,
}

/// Welcome menu state
pub struct WelcomeMenu {
    /// Current directory option (always shown at top)
    current_dir: PathBuf,
    /// Recent workspaces
    recents: Vec<Recent>,
    /// Currently selected index (0 = current dir, 1+ = recents)
    selected: usize,
    /// Scroll offset for the list
    scroll: usize,
}

impl WelcomeMenu {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let recents = recents_get();

        Self {
            current_dir,
            recents,
            selected: 0,
            scroll: 0,
        }
    }

    /// Total number of items (current dir + recents)
    pub fn item_count(&self) -> usize {
        1 + self.recents.len()
    }

    /// Get the selected path
    pub fn selected_path(&self) -> PathBuf {
        if self.selected == 0 {
            self.current_dir.clone()
        } else {
            self.recents[self.selected - 1].path.clone()
        }
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.ensure_visible();
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if self.selected + 1 < self.item_count() {
            self.selected += 1;
            self.ensure_visible();
        }
    }

    /// Move selection to top
    pub fn move_to_top(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    /// Move selection to bottom
    pub fn move_to_bottom(&mut self) {
        if self.item_count() > 0 {
            self.selected = self.item_count() - 1;
            self.ensure_visible();
        }
    }

    /// Ensure selected item is visible
    fn ensure_visible(&mut self) {
        // We'll update scroll based on visible area in render
    }

    /// Update scroll to ensure selection is visible within given visible_rows
    pub fn update_viewport(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible_rows {
            self.scroll = self.selected - visible_rows + 1;
        }
    }

    /// Get items to display, returns (label, path_display, is_selected, is_current_dir)
    pub fn visible_items(&self) -> Vec<(String, String, bool, bool)> {
        let mut items = Vec::new();

        // Current directory is always first
        let current_label = self
            .current_dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.current_dir.to_string_lossy().to_string());
        let current_path = self.current_dir.to_string_lossy().to_string();
        items.push((
            format!(" {} (current directory)", current_label),
            current_path,
            self.selected == 0,
            true,
        ));

        // Recent workspaces
        for (i, recent) in self.recents.iter().enumerate() {
            let path_display = recent.path.to_string_lossy().to_string();
            items.push((
                format!(" {}", recent.label),
                path_display,
                self.selected == i + 1,
                false,
            ));
        }

        items
    }

    /// Get current scroll offset
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Handle a key press, returns Some(result) if menu should close
    pub fn handle_key(&mut self, key: Key, _mods: Modifiers) -> Option<WelcomeResult> {
        match key {
            Key::Up | Key::Char('k') => {
                self.move_up();
                None
            }
            Key::Down | Key::Char('j') => {
                self.move_down();
                None
            }
            Key::Home => {
                self.move_to_top();
                None
            }
            Key::End => {
                self.move_to_bottom();
                None
            }
            Key::Enter => Some(WelcomeResult::Selected(self.selected_path())),
            Key::Escape | Key::Char('q') => Some(WelcomeResult::Quit),
            _ => None,
        }
    }

    /// Run the welcome menu, returns selected path or None if user quit
    /// Assumes screen is already in raw mode
    pub fn run(screen: &mut Screen) -> Result<Option<PathBuf>> {
        let mut menu = WelcomeMenu::new();

        loop {
            // Update viewport based on visible area
            let visible_rows = screen.rows.saturating_sub(10) as usize;
            menu.update_viewport(visible_rows);

            // Render
            screen.render_welcome(&menu.visible_items(), menu.scroll())?;

            // Wait for input
            if let Event::Key(key_event) = event::read()? {
                let (key, mods) = Key::from_crossterm(key_event);
                if let Some(result) = menu.handle_key(key, mods) {
                    return match result {
                        WelcomeResult::Selected(path) => Ok(Some(path)),
                        WelcomeResult::Quit => Ok(None),
                    };
                }
            }
        }
    }
}
