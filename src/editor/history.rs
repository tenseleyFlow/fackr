use super::Position;

/// An atomic edit operation that can be undone/redone
#[derive(Debug, Clone)]
pub enum Operation {
    /// Insert text at position
    Insert {
        pos: usize,        // char index
        text: String,
        cursor_before: Position,
        cursor_after: Position,
    },
    /// Delete text at position
    Delete {
        pos: usize,        // char index
        text: String,      // the deleted text (for undo)
        cursor_before: Position,
        cursor_after: Position,
    },
}

impl Operation {
    pub fn cursor_before(&self) -> Position {
        match self {
            Operation::Insert { cursor_before, .. } => *cursor_before,
            Operation::Delete { cursor_before, .. } => *cursor_before,
        }
    }

    pub fn cursor_after(&self) -> Position {
        match self {
            Operation::Insert { cursor_after, .. } => *cursor_after,
            Operation::Delete { cursor_after, .. } => *cursor_after,
        }
    }
}

/// A group of operations that should be undone/redone together
#[derive(Debug, Clone, Default)]
pub struct OperationGroup {
    pub ops: Vec<Operation>,
    /// Cursor positions before this group (for multi-cursor undo)
    pub cursors_before: Vec<Position>,
    /// Cursor positions after this group (for multi-cursor redo)
    pub cursors_after: Vec<Position>,
}

impl OperationGroup {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            cursors_before: Vec::new(),
            cursors_after: Vec::new(),
        }
    }

    pub fn push(&mut self, op: Operation) {
        self.ops.push(op);
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn set_cursors_before(&mut self, positions: Vec<Position>) {
        if self.cursors_before.is_empty() {
            self.cursors_before = positions;
        }
    }

    pub fn set_cursors_after(&mut self, positions: Vec<Position>) {
        self.cursors_after = positions;
    }
}

/// Undo/redo history using operation-based approach
#[derive(Debug, Default)]
pub struct History {
    undo_stack: Vec<OperationGroup>,
    redo_stack: Vec<OperationGroup>,
    current_group: OperationGroup,
    /// Whether we're in the middle of a group (e.g., typing a word)
    grouping: bool,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new operation group
    pub fn begin_group(&mut self) {
        if !self.current_group.is_empty() {
            self.commit_group();
        }
        self.grouping = true;
    }

    /// End current operation group
    pub fn end_group(&mut self) {
        if !self.current_group.is_empty() {
            self.commit_group();
        }
        self.grouping = false;
    }

    /// Add an operation to the current group
    pub fn push(&mut self, op: Operation) {
        self.current_group.push(op);
        self.redo_stack.clear();
    }

    /// Set cursor positions before current operation group (for multi-cursor undo)
    pub fn set_cursors_before(&mut self, positions: Vec<Position>) {
        self.current_group.set_cursors_before(positions);
    }

    /// Set cursor positions after current operation group (for multi-cursor redo)
    pub fn set_cursors_after(&mut self, positions: Vec<Position>) {
        self.current_group.set_cursors_after(positions);
    }

    /// Record an insert operation
    pub fn record_insert(
        &mut self,
        pos: usize,
        text: String,
        cursor_before: Position,
        cursor_after: Position,
    ) {
        self.push(Operation::Insert {
            pos,
            text,
            cursor_before,
            cursor_after,
        });
    }

    /// Record a delete operation
    pub fn record_delete(
        &mut self,
        pos: usize,
        text: String,
        cursor_before: Position,
        cursor_after: Position,
    ) {
        self.push(Operation::Delete {
            pos,
            text,
            cursor_before,
            cursor_after,
        });
    }

    /// Commit current group to undo stack
    fn commit_group(&mut self) {
        if !self.current_group.is_empty() {
            let group = std::mem::take(&mut self.current_group);
            self.undo_stack.push(group);
        }
    }

    /// Check if we should break the current group (e.g., on non-typing command)
    pub fn maybe_break_group(&mut self) {
        if !self.grouping && !self.current_group.is_empty() {
            self.commit_group();
        }
    }

    /// Get operations to undo, returns (operations, cursor_positions_after_undo)
    pub fn undo(&mut self) -> Option<(Vec<Operation>, Vec<Position>)> {
        self.commit_group();

        if let Some(group) = self.undo_stack.pop() {
            // Use stored cursors_before if available, otherwise fall back to first op's cursor_before
            let cursor_positions = if !group.cursors_before.is_empty() {
                group.cursors_before.clone()
            } else {
                vec![group.ops.first().map(|op| op.cursor_before()).unwrap_or_default()]
            };
            self.redo_stack.push(group.clone());
            Some((group.ops, cursor_positions))
        } else {
            None
        }
    }

    /// Get operations to redo, returns (operations, cursor_positions_after_redo)
    pub fn redo(&mut self) -> Option<(Vec<Operation>, Vec<Position>)> {
        if let Some(group) = self.redo_stack.pop() {
            // Use stored cursors_after if available, otherwise fall back to last op's cursor_after
            let cursor_positions = if !group.cursors_after.is_empty() {
                group.cursors_after.clone()
            } else {
                vec![group.ops.last().map(|op| op.cursor_after()).unwrap_or_default()]
            };
            self.undo_stack.push(group.clone());
            Some((group.ops, cursor_positions))
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() || !self.current_group.is_empty()
    }

    #[allow(dead_code)]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all history
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_group = OperationGroup::new();
    }

    /// Get mutable reference to last operation in current group or undo stack
    pub fn undo_stack_last_mut(&mut self) -> Option<&mut Operation> {
        if !self.current_group.is_empty() {
            self.current_group.ops.last_mut()
        } else {
            self.undo_stack.last_mut().and_then(|g| g.ops.last_mut())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_undo() {
        let mut history = History::new();
        let before = Position::new(0, 0);
        let after = Position::new(0, 5);

        history.record_insert(0, "hello".to_string(), before, after);
        history.end_group();

        assert!(history.can_undo());
        let (ops, positions) = history.undo().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], before);
    }

    #[test]
    fn test_redo() {
        let mut history = History::new();
        let before = Position::new(0, 0);
        let after = Position::new(0, 5);

        history.record_insert(0, "hello".to_string(), before, after);
        history.end_group();

        history.undo();
        assert!(history.can_redo());

        let (ops, positions) = history.redo().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], after);
    }
}
