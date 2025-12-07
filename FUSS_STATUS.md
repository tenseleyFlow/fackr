# Fuss Mode Feature Parity Status

Tracking progress on closing the feature gap between Rust fackr and Fortran facsimile fuss modes.

## Completed

- [x] **Header with repo:branch** - Display workspace name and git branch at top of fuss pane (cyan:yellow coloring)
- [x] **Git status indicators** - Show file status in tree:
  - `↑` (green) - staged changes
  - `✗` (red) - unstaged changes
  - `?` (gray) - untracked files
  - `↓` (blue) - incoming changes (after fetch)
- [x] **Git staging** (`a`) - Stage selected file
- [x] **Git unstaging** (`u`) - Unstage selected file
- [x] **Git diff** (`d`) - Show diff in new tab for selected file
- [x] **Git commit** (`m`) - Create commit with message prompt
- [x] **Git push** (`p`) - Push to remote
- [x] **Git pull** (`l`) - Pull from remote
- [x] **Git fetch** (`f`) - Fetch from remote
- [x] **Git tag** (`t`) - Create tag with name prompt
- [x] **Updated hints** - Show git operation keybindings in expanded hints
- [x] **Smart collapse** - Only auto-expand directories with dirty files
- [x] **Gitignored marking** - Gitignored files shown in dark gray
- [x] **Incoming changes indicator** - Files with incoming changes after fetch show `↓` (blue)

## Feature Parity Complete! 🎉

All major features from the Fortran facsimile fuss mode have been implemented in the Rust version.

---

## Implementation Notes

### Git Status Indicators
- `↑` (green) - staged changes
- `✗` (red) - unstaged changes
- `?` (gray) - untracked files
- `↓` (blue) - incoming changes (files differ from upstream after fetch)
- Gitignored files are rendered entirely in dark gray (no indicator)

### Keybindings (in fuss mode)
- `j`/`k` - Navigate up/down
- `Space` - Toggle expand/collapse directory
- `o`/`Enter` - Open file / toggle directory
- `.` - Toggle hidden files
- `a` - Stage file
- `u` - Unstage file
- `d` - Show diff
- `m` - Commit (prompts for message)
- `p` - Push
- `l` - Pull
- `f` - Fetch
- `t` - Tag (prompts for tag name)
- `Ctrl+/` - Toggle hints
- `Ctrl+B`/`Esc` - Close fuss mode

### Files Modified
- `src/fuss/tree.rs` - TreeNode git status fields, git status parsing (including gitignored and incoming)
- `src/fuss/state.rs` - Git operations (stage, unstage, commit, push, pull, fetch, tag, diff)
- `src/render/screen.rs` - Status symbol rendering, gitignored styling, updated hints
- `src/editor/state.rs` - Keybinding handlers, text input prompt for commit/tag
- `src/workspace/state.rs` - Content tab support for diff view
