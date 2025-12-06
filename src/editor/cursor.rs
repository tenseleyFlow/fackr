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
#[derive(Debug, Clone, Copy, Default)]
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
