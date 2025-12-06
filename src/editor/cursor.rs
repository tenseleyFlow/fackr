/// A position in the buffer (0-indexed)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.line.cmp(&other.line) {
            std::cmp::Ordering::Equal => self.col.cmp(&other.col),
            ord => ord,
        }
    }
}

/// Selection represented by anchor and cursor positions
#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    /// Where the selection started
    pub anchor: Position,
    /// Current cursor position (end of selection)
    pub cursor: Position,
}

impl Selection {
    pub fn new(anchor: Position, cursor: Position) -> Self {
        Self { anchor, cursor }
    }

    /// Get the start and end of the selection (ordered)
    pub fn ordered(&self) -> (Position, Position) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    /// Check if selection is empty (anchor == cursor)
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }

    /// Collapse selection to cursor position
    #[allow(dead_code)]
    pub fn collapse(&mut self) {
        self.anchor = self.cursor;
    }
}

/// Cursor with selection support
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    /// Desired column for vertical movement
    pub desired_col: usize,
    /// Selection anchor (if different from cursor, there's a selection)
    pub anchor_line: usize,
    pub anchor_col: usize,
    /// Whether selection mode is active
    pub selecting: bool,
}

impl Cursor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn at(line: usize, col: usize) -> Self {
        Self {
            line,
            col,
            desired_col: col,
            anchor_line: line,
            anchor_col: col,
            selecting: false,
        }
    }

    pub fn position(&self) -> Position {
        Position::new(self.line, self.col)
    }

    pub fn anchor(&self) -> Position {
        Position::new(self.anchor_line, self.anchor_col)
    }

    pub fn selection(&self) -> Selection {
        Selection::new(self.anchor(), self.position())
    }

    pub fn has_selection(&self) -> bool {
        self.selecting && (self.line != self.anchor_line || self.col != self.anchor_col)
    }

    /// Get ordered selection bounds (start, end)
    pub fn selection_bounds(&self) -> Option<(Position, Position)> {
        if self.has_selection() {
            Some(self.selection().ordered())
        } else {
            None
        }
    }

    /// Start selection at current position
    pub fn start_selection(&mut self) {
        self.anchor_line = self.line;
        self.anchor_col = self.col;
        self.selecting = true;
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selecting = false;
        self.anchor_line = self.line;
        self.anchor_col = self.col;
    }

    /// Move cursor, extending selection if in selection mode
    pub fn move_to(&mut self, line: usize, col: usize, extend_selection: bool) {
        if extend_selection && !self.selecting {
            self.start_selection();
        } else if !extend_selection && self.selecting {
            self.clear_selection();
        }
        self.line = line;
        self.col = col;
        if !extend_selection {
            self.anchor_line = line;
            self.anchor_col = col;
        }
    }

    /// Set position and update desired column
    #[allow(dead_code)]
    pub fn set(&mut self, line: usize, col: usize) {
        self.line = line;
        self.col = col;
        self.desired_col = col;
        self.clear_selection();
    }
}

/// Multi-cursor container - manages a set of cursors
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct Cursors {
    /// All cursors, sorted by position (primary cursor is always index 0)
    cursors: Vec<Cursor>,
    /// Index of the primary cursor (receives special treatment in some operations)
    primary: usize,
}

#[allow(dead_code)]
impl Cursors {
    pub fn new() -> Self {
        Self {
            cursors: vec![Cursor::new()],
            primary: 0,
        }
    }

    /// Get the primary cursor
    pub fn primary(&self) -> &Cursor {
        &self.cursors[self.primary]
    }

    /// Get mutable reference to primary cursor
    pub fn primary_mut(&mut self) -> &mut Cursor {
        &mut self.cursors[self.primary]
    }

    /// Get all cursors
    pub fn all(&self) -> &[Cursor] {
        &self.cursors
    }

    /// Get mutable references to all cursors
    pub fn all_mut(&mut self) -> &mut [Cursor] {
        &mut self.cursors
    }

    /// Number of cursors
    pub fn len(&self) -> usize {
        self.cursors.len()
    }

    /// Check if we have only one cursor
    pub fn is_single(&self) -> bool {
        self.cursors.len() == 1
    }

    /// Get the index of the primary cursor
    pub fn primary_index(&self) -> usize {
        self.primary
    }

    /// Add a new cursor at the given position
    /// Returns true if cursor was added, false if position already has a cursor
    pub fn add(&mut self, line: usize, col: usize) -> bool {
        let new_cursor = Cursor::at(line, col);

        // Check for duplicates
        if self.cursors.iter().any(|c| c.line == line && c.col == col) {
            return false;
        }

        self.cursors.push(new_cursor);
        self.sort_and_dedupe();
        true
    }

    /// Remove cursor at the given position if it exists
    /// Returns true if a cursor was removed
    /// Won't remove the last cursor
    pub fn remove_at(&mut self, line: usize, col: usize) -> bool {
        if self.cursors.len() <= 1 {
            return false; // Don't remove the last cursor
        }

        if let Some(idx) = self.cursors.iter().position(|c| c.line == line && c.col == col) {
            self.cursors.remove(idx);
            // Adjust primary index if needed
            if self.primary >= self.cursors.len() {
                self.primary = self.cursors.len() - 1;
            } else if self.primary > idx {
                self.primary -= 1;
            }
            return true;
        }
        false
    }

    /// Toggle cursor at position: add if not present, remove if present
    /// Returns true if cursor was added, false if removed
    pub fn toggle_at(&mut self, line: usize, col: usize) -> bool {
        if self.cursors.iter().any(|c| c.line == line && c.col == col) {
            self.remove_at(line, col);
            false
        } else {
            self.add(line, col);
            true
        }
    }

    /// Add a cursor with selection
    pub fn add_with_selection(&mut self, line: usize, col: usize, anchor_line: usize, anchor_col: usize) -> bool {
        // Check for duplicates at cursor position
        if self.cursors.iter().any(|c| c.line == line && c.col == col) {
            return false;
        }

        let mut new_cursor = Cursor::at(line, col);
        new_cursor.anchor_line = anchor_line;
        new_cursor.anchor_col = anchor_col;
        new_cursor.selecting = true;

        self.cursors.push(new_cursor);
        self.sort_and_dedupe();
        true
    }

    /// Remove secondary cursors, keeping only the primary
    pub fn collapse_to_primary(&mut self) {
        let primary = self.cursors[self.primary].clone();
        self.cursors.clear();
        self.cursors.push(primary);
        self.primary = 0;
    }

    /// Remove cursor at the given index
    pub fn remove(&mut self, index: usize) {
        if self.cursors.len() > 1 && index < self.cursors.len() {
            self.cursors.remove(index);
            if self.primary >= self.cursors.len() {
                self.primary = self.cursors.len() - 1;
            }
        }
    }

    /// Sort cursors by position and remove duplicates
    pub fn sort_and_dedupe(&mut self) {
        // Remember primary cursor's position
        let primary_pos = (self.cursors[self.primary].line, self.cursors[self.primary].col);

        // Sort by line, then by column
        self.cursors.sort_by(|a, b| {
            match a.line.cmp(&b.line) {
                std::cmp::Ordering::Equal => a.col.cmp(&b.col),
                ord => ord,
            }
        });

        // Remove duplicates (same position)
        self.cursors.dedup_by(|a, b| a.line == b.line && a.col == b.col);

        // Find primary cursor's new index
        self.primary = self.cursors.iter()
            .position(|c| c.line == primary_pos.0 && c.col == primary_pos.1)
            .unwrap_or(0);
    }

    /// Apply a function to all cursors
    pub fn for_each<F: FnMut(&mut Cursor)>(&mut self, mut f: F) {
        for cursor in &mut self.cursors {
            f(cursor);
        }
    }

    /// Get iterator over cursors sorted by position (bottom to top for editing)
    /// This ordering is important for edits that change buffer positions
    pub fn iter_for_edit(&self) -> impl Iterator<Item = (usize, &Cursor)> {
        // Return indices with cursors, sorted from bottom-right to top-left
        let mut indices: Vec<usize> = (0..self.cursors.len()).collect();
        indices.sort_by(|&a, &b| {
            let ca = &self.cursors[a];
            let cb = &self.cursors[b];
            match cb.line.cmp(&ca.line) {
                std::cmp::Ordering::Equal => cb.col.cmp(&ca.col),
                ord => ord,
            }
        });
        indices.into_iter().map(move |i| (i, &self.cursors[i]))
    }

    /// Clear selection on all cursors
    pub fn clear_selections(&mut self) {
        for cursor in &mut self.cursors {
            cursor.clear_selection();
        }
    }

    /// Check if any cursor has a selection
    pub fn has_selection(&self) -> bool {
        self.cursors.iter().any(|c| c.has_selection())
    }

    /// Get selection bounds for primary cursor
    pub fn selection_bounds(&self) -> Option<(Position, Position)> {
        self.primary().selection_bounds()
    }

    /// Merge overlapping selections and cursors at same position
    pub fn merge_overlapping(&mut self) {
        self.sort_and_dedupe();
        // TODO: Merge overlapping selections (for now, just dedupe)
    }

    /// Set cursors from a list of positions (for undo/redo)
    /// Primary cursor becomes the first position in the list
    pub fn set_from_positions(&mut self, positions: &[Position]) {
        if positions.is_empty() {
            return;
        }

        self.cursors.clear();
        for pos in positions {
            self.cursors.push(Cursor::at(pos.line, pos.col));
        }
        self.primary = 0;
        self.sort_and_dedupe();
    }
}
