mod buffer;
mod editor;
mod fuss;
mod input;
mod lsp;
mod render;
mod syntax;
mod terminal;
mod util;
mod workspace;

use anyhow::Result;
use editor::{Editor, WelcomeMenu};
use render::Screen;
use std::env;
use workspace::recents_add_or_update;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let filename = args.get(1).map(|s| s.as_str());

    if let Some(path) = filename {
        // File/directory provided - open directly
        let mut editor = Editor::new()?;
        editor.open(path)?;

        // Track this workspace in recents
        let _ = recents_add_or_update(&editor.workspace_root());

        editor.run()
    } else {
        // No arguments - show welcome menu
        let mut screen = Screen::new()?;
        screen.enter_raw_mode()?;

        match WelcomeMenu::run(&mut screen)? {
            Some(workspace_path) => {
                // Track this workspace in recents
                let _ = recents_add_or_update(&workspace_path);

                // Create editor with selected workspace, reusing the screen
                let mut editor = Editor::new_with_screen_and_workspace(screen, workspace_path)?;
                editor.run()
            }
            None => {
                // User quit from welcome menu
                screen.leave_raw_mode()?;
                Ok(())
            }
        }
    }
}
