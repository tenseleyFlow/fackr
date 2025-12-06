# Testing fac-rust

## Build

```bash
cargo build --release
```

## Run

```bash
./target/release/fac [filename]
```

## Test Checklist

### Basic Editing
- [ ] Open a file: `./target/release/fac testfile.txt`
- [ ] Type some text - characters appear at cursor
- [ ] Press Enter - new line created
- [ ] Press Tab - 4 spaces inserted
- [ ] Press Backspace - deletes character before cursor
- [ ] Press Delete - deletes character after cursor
- [ ] Ctrl+S - saves file (status bar shows "Saved")
- [ ] Ctrl+Q - quits editor

### Cursor Movement
- [ ] Arrow keys - move cursor
- [ ] Home / Ctrl+A - go to line start (smart: toggles between col 0 and first non-whitespace)
- [ ] End / Ctrl+E - go to line end
- [ ] PgUp/PgDn - scroll by page
- [ ] Alt+Left - jump word backward
- [ ] Alt+Right - jump word forward

### Selection
- [ ] Shift+Right - select characters forward
- [ ] Shift+Left - select characters backward
- [ ] Shift+Up/Down - select lines
- [ ] Shift+Home - select to line start
- [ ] Shift+End - select to line end
- [ ] Shift+Alt+Left - select word backward
- [ ] Shift+Alt+Right - select word forward
- [ ] Selection shows with blue highlight
- [ ] Escape - clears selection

### Clipboard
- [ ] Select text, Ctrl+C - copies (status: "Copied")
- [ ] Ctrl+V - pastes copied text
- [ ] Select text, Ctrl+X - cuts (status: "Cut")
- [ ] No selection, Ctrl+C - copies entire line (status: "Copied line")
- [ ] No selection, Ctrl+X - cuts entire line (status: "Cut line")

### Undo/Redo
- [ ] Type some text, Ctrl+Z - undoes typing (status: "Undo")
- [ ] Ctrl+] or Ctrl+Shift+Z - redoes (status: "Redo")
- [ ] Multiple undos work in sequence
- [ ] Redo stack clears when new edits are made

### Line Operations
- [ ] Alt+Up - moves current line up
- [ ] Alt+Down - moves current line down
- [ ] Alt+Shift+Up - duplicates line above (cursor stays)
- [ ] Alt+Shift+Down - duplicates line below (cursor moves to new line)
- [ ] Ctrl+J - joins current line with next line

### Word/Line Deletion
- [ ] Ctrl+W - deletes word backward
- [ ] Alt+D - deletes word forward
- [ ] Shift+Tab - removes up to 4 leading spaces (dedent)

### Display
- [ ] Line numbers show on left
- [ ] Status bar shows filename and position
- [ ] Modified indicator [+] appears after edits
- [ ] Status messages appear on right side of status bar

## Quick Test Script

Create a test file and try these operations in sequence:

```bash
echo -e "Hello World\nThis is a test\n    indented line\nAnother line" > /tmp/test.txt
./target/release/fac /tmp/test.txt
```

Then:
1. Move to "World", select it (Shift+Right x5), Ctrl+C
2. Move to end of line 2, Ctrl+V - should paste "World"
3. Ctrl+Z twice - should undo both operations
4. Go to line 3, Ctrl+A twice - cursor toggles between col 0 and col 4
5. Alt+Down - moves indented line down
6. Ctrl+S to save, Ctrl+Q to quit
7. `cat /tmp/test.txt` to verify changes
