# LSP Test Files for fackr

This directory contains Python files for testing LSP (Language Server Protocol)
features in the fackr editor.

## Prerequisites

Make sure you have a Python LSP server installed:
- `pylsp` (python-lsp-server): `pip install python-lsp-server`
- `ruff`: `pip install ruff` (for linting/formatting)
- `pyright`: `pip install pyright` (for type checking)

## Test Files

| File | Feature | Keybinding |
|------|---------|------------|
| 01_hover.py | Hover Information | F1 |
| 02_completion.py | Code Completion | Ctrl+Space |
| 03_diagnostics.py | Diagnostics | Automatic |
| 04_goto_definition.py | Go to Definition | F12 |
| 05_references.py | Find References | Shift+F12 |
| 06_rename.py | Rename Symbol | F2 |
| 07_formatting.py | Code Formatting | Ctrl+Shift+F |

## How to Test

1. Open fackr: `fackr test_lsp/01_hover.py`
2. Follow the instructions in the comments at the top of each file
3. Each file is self-contained and tests a specific LSP feature

## LSP Keybindings Reference

| Key | Action |
|-----|--------|
| F1 | Show hover information |
| Ctrl+Space | Trigger code completion |
| F12 | Go to definition |
| Shift+F12 | Find all references |
| F2 | Rename symbol |
| Ctrl+Shift+F | Format document |
| Alt+M | Open LSP server manager |

## Troubleshooting

- If LSP features don't work, check that the language server is running
- Use Alt+M to open the server manager and verify server status
- Some features may take a moment to initialize when first opening a file
