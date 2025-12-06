mod buffer;
mod editor;
mod input;
mod render;
mod util;
mod workspace;

use anyhow::Result;
use editor::Editor;
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let filename = args.get(1).map(|s| s.as_str());

    let mut editor = Editor::new()?;

    if let Some(path) = filename {
        editor.open(path)?;
    }

    editor.run()
}
