/// Cursor position (0-indexed line and column)
#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
    /// Desired column for vertical movement
    pub desired_col: usize,
}

impl Cursor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, line: usize, col: usize) {
        self.line = line;
        self.col = col;
        self.desired_col = col;
    }

    pub fn move_to(&mut self, line: usize, col: usize) {
        self.line = line;
        self.col = col;
        // Don't update desired_col for horizontal moves
    }
}
