// Simple key event debugger
// Run with: cargo run --bin keytest

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers, PushKeyboardEnhancementFlags, PopKeyboardEnhancementFlags, KeyboardEnhancementFlags},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;

    // Try to enable keyboard enhancement
    let enhanced = execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    ).is_ok();

    println!("Keyboard enhancement: {}\r", if enhanced { "enabled" } else { "disabled" });
    println!("Press keys to see events (Ctrl+C to quit)\r\n");

    loop {
        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(KeyEvent { code, modifiers, kind, state }) => {
                    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
                    let alt = modifiers.contains(KeyModifiers::ALT);
                    let shift = modifiers.contains(KeyModifiers::SHIFT);

                    println!(
                        "Key: {:?} | Modifiers: ctrl={} alt={} shift={} | Kind: {:?} | State: {:?}\r",
                        code, ctrl, alt, shift, kind, state
                    );

                    // Quit on Ctrl+C
                    if let KeyCode::Char('c') = code {
                        if ctrl {
                            break;
                        }
                    }
                }
                Event::Resize(w, h) => {
                    println!("Resize: {}x{}\r", w, h);
                }
                _ => {}
            }
        }
    }

    if enhanced {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    execute!(io::stdout(), LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    Ok(())
}
