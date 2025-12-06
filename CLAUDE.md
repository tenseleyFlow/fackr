# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

fackr is a terminal text editor written in Rust, a port of "facsimile" (fac) from Fortran. The goal is to achieve better performance, especially for cursor operations on remote connections.

## Build Commands

```bash
cargo build --release  # Release build (with LTO)
cargo test             # Run all tests
cargo test <name>      # Run specific test
./target/release/fackr [file]  # Run editor
```

## Architecture

### Module Structure

- **`main.rs`** - Entry point, parses args and runs editor
- **`editor/`** - Core editor logic
  - `state.rs` - Main `Editor` struct, event loop, keybindings (central file)
  - `cursor.rs` - `Cursor`, `Position`, `Selection` types
  - `history.rs` - Operation-based undo/redo system
- **`buffer/`** - Text storage
  - `rope.rs` - `Buffer` wrapping `ropey::Rope` for efficient text operations
- **`input/`** - Input handling
  - `key.rs` - `Key` enum and `Modifiers`, abstracts crossterm events
- **`render/`** - Terminal rendering
  - `screen.rs` - `Screen` struct, double-buffered rendering with crossterm
- **`util/`** - Utilities (unicode handling)

### Key Design Patterns

1. **Rope-based buffer**: Uses `ropey` crate for O(log n) text operations
2. **Operation-based undo**: Records insert/delete operations rather than full state snapshots
3. **Selection model**: Anchor + cursor position, supporting shift+arrow selection
4. **Blocking event loop**: Uses `event::read()` to block until input (no busy polling)
5. **Alt key detection**: Manual ESC sequence parsing with configurable timeout (`FAC_ESCAPE_TIME` env var)

### Keybindings (defined in `state.rs:handle_key_with_mods`)

- Movement: arrows, Home/End, PageUp/Down
- Word movement: Alt+Left/Right, Alt+B/F
- Selection: Shift+movement keys
- Editing: Backspace, Delete, Tab, Shift+Tab (dedent)
- Clipboard: Ctrl+C/X/V
- Undo/Redo: Ctrl+Z, Ctrl+Shift+Z
- Line ops: Alt+Up/Down (move line), Alt+Shift+Up/Down (duplicate)
- Save: Ctrl+S, Quit: Ctrl+Q

### Dependencies

- `crossterm` - Terminal I/O, raw mode, keyboard enhancement
- `ropey` - Rope data structure for text buffer
- `unicode-segmentation`, `unicode-width` - Unicode handling
- `thiserror`, `anyhow` - Error handling

## Development Notes

- Escape timeout defaults to 5ms; override with `FAC_ESCAPE_TIME=<ms>` for terminals with high latency
- Keyboard enhancement (kitty protocol) is attempted but falls back gracefully
- Line numbers are rendered with minimum 3-character width
- Status bar shows filename, modified indicator, and cursor position
