use anyhow::Result;
use ropey::Rope;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

/// Text buffer using rope data structure for efficient editing
#[derive(Debug)]
pub struct Buffer {
    text: Rope,
    pub modified: bool,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer {
    pub fn new() -> Self {
        Self {
            text: Rope::new(),
            modified: false,
        }
    }

    #[allow(dead_code)]
    pub fn from_str(s: &str) -> Self {
        Self {
            text: Rope::from_str(s),
            modified: false,
        }
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let text = Rope::from_reader(reader)?;
        Ok(Self {
            text,
            modified: false,
        })
    }

    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        self.text.write_to(writer)?;
        self.modified = false;
        Ok(())
    }

    /// Insert text at character index
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let idx = char_idx.min(self.text.len_chars());
        self.text.insert(idx, text);
        self.modified = true;
    }

    /// Delete characters in range [start, end)
    pub fn delete(&mut self, start: usize, end: usize) {
        let start = start.min(self.text.len_chars());
        let end = end.min(self.text.len_chars());
        if start < end {
            self.text.remove(start..end);
            self.modified = true;
        }
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }

    /// Get total character count
    #[allow(dead_code)]
    pub fn char_count(&self) -> usize {
        self.text.len_chars()
    }

    /// Get a line's content (0-indexed)
    pub fn line(&self, line_idx: usize) -> Option<ropey::RopeSlice<'_>> {
        if line_idx < self.text.len_lines() {
            Some(self.text.line(line_idx))
        } else {
            None
        }
    }

    /// Get line as String (without trailing newline)
    pub fn line_str(&self, line_idx: usize) -> Option<String> {
        self.line(line_idx).map(|l| {
            let s: String = l.chars().collect();
            s.trim_end_matches('\n').to_string()
        })
    }

    /// Get character count for a line (excluding newline)
    pub fn line_len(&self, line_idx: usize) -> usize {
        self.line(line_idx)
            .map(|l| {
                let len = l.len_chars();
                // Subtract 1 for newline if not last line
                if line_idx + 1 < self.text.len_lines() && len > 0 {
                    len - 1
                } else {
                    len
                }
            })
            .unwrap_or(0)
    }

    /// Convert (line, col) to absolute char index
    pub fn line_col_to_char(&self, line: usize, col: usize) -> usize {
        if line >= self.text.len_lines() {
            return self.text.len_chars();
        }
        let line_start = self.text.line_to_char(line);
        let line_len = self.line_len(line);
        line_start + col.min(line_len)
    }

    /// Convert absolute char index to (line, col)
    #[allow(dead_code)]
    pub fn char_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let idx = char_idx.min(self.text.len_chars());
        let line = self.text.char_to_line(idx);
        let line_start = self.text.line_to_char(line);
        let col = idx - line_start;
        (line, col)
    }

    /// Get character at position
    #[allow(dead_code)]
    pub fn char_at(&self, char_idx: usize) -> Option<char> {
        if char_idx < self.text.len_chars() {
            Some(self.text.char(char_idx))
        } else {
            None
        }
    }

    /// Check if buffer is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.text.len_chars() == 0
    }

    /// Get total character count
    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    /// Get rope slice for a range
    #[allow(dead_code)]
    pub fn slice(&self, start: usize, end: usize) -> ropey::RopeSlice<'_> {
        let start = start.min(self.text.len_chars());
        let end = end.min(self.text.len_chars());
        self.text.slice(start..end)
    }

    /// Find matching bracket for the character at the given position
    /// Returns (line, col) of matching bracket, or None if not found
    pub fn find_matching_bracket(&self, line: usize, col: usize) -> Option<(usize, usize)> {
        let char_idx = self.line_col_to_char(line, col);
        let ch = self.char_at(char_idx)?;

        let (target, direction) = match ch {
            '(' => (')', 1i32),
            ')' => ('(', -1),
            '[' => (']', 1),
            ']' => ('[', -1),
            '{' => ('}', 1),
            '}' => ('{', -1),
            '<' => ('>', 1),
            '>' => ('<', -1),
            _ => return None,
        };

        let mut depth = 1;
        let mut pos = char_idx as i32 + direction;
        let len = self.text.len_chars() as i32;

        while pos >= 0 && pos < len {
            if let Some(c) = self.char_at(pos as usize) {
                if c == target {
                    depth -= 1;
                    if depth == 0 {
                        return Some(self.char_to_line_col(pos as usize));
                    }
                } else if c == ch {
                    depth += 1;
                }
            }
            pos += direction;
        }

        None
    }

    /// Find surrounding brackets containing the cursor position (across lines)
    /// Returns (open_char_idx, close_char_idx, open_char, close_char)
    pub fn find_surrounding_brackets(&self, line: usize, col: usize) -> Option<(usize, usize, char, char)> {
        let cursor_idx = self.line_col_to_char(line, col);

        // Search backward for an opening bracket that contains cursor
        for search_pos in (0..cursor_idx).rev() {
            let ch = self.char_at(search_pos)?;
            let (open, close) = match ch {
                '(' => ('(', ')'),
                '{' => ('{', '}'),
                '[' => ('[', ']'),
                _ => continue,
            };

            // Find matching close
            let mut depth = 1;
            let mut pos = search_pos + 1;
            let len = self.text.len_chars();

            while pos < len {
                if let Some(c) = self.char_at(pos) {
                    if c == close {
                        depth -= 1;
                        if depth == 0 {
                            // Check if cursor is inside this pair
                            if cursor_idx > search_pos && cursor_idx <= pos {
                                return Some((search_pos, pos, open, close));
                            }
                            break;
                        }
                    } else if c == open {
                        depth += 1;
                    }
                }
                pos += 1;
            }
        }
        None
    }

    /// Find surrounding quotes containing the cursor position (across lines)
    /// Returns (open_char_idx, close_char_idx, quote_char)
    pub fn find_surrounding_quotes(&self, line: usize, col: usize) -> Option<(usize, usize, char)> {
        let cursor_idx = self.line_col_to_char(line, col);

        // Search backward for an opening quote
        for search_pos in (0..cursor_idx).rev() {
            let ch = self.char_at(search_pos)?;
            if ch != '"' && ch != '\'' && ch != '`' {
                continue;
            }

            // Find matching close (same quote char)
            let mut pos = search_pos + 1;
            let len = self.text.len_chars();

            while pos < len {
                if let Some(c) = self.char_at(pos) {
                    if c == ch {
                        // Check if cursor is inside this pair
                        if cursor_idx > search_pos && cursor_idx <= pos {
                            return Some((search_pos, pos, ch));
                        }
                        break;
                    }
                }
                pos += 1;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buf = Buffer::new();
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.char_count(), 0);
    }

    #[test]
    fn test_insert() {
        let mut buf = Buffer::new();
        buf.insert(0, "Hello");
        assert_eq!(buf.line_str(0), Some("Hello".to_string()));
        assert!(buf.modified);
    }

    #[test]
    fn test_multiline() {
        let buf = Buffer::from_str("Hello\nWorld\n");
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line_str(0), Some("Hello".to_string()));
        assert_eq!(buf.line_str(1), Some("World".to_string()));
    }

    #[test]
    fn test_line_col_conversion() {
        let buf = Buffer::from_str("Hello\nWorld");
        assert_eq!(buf.line_col_to_char(0, 0), 0);
        assert_eq!(buf.line_col_to_char(0, 5), 5);
        assert_eq!(buf.line_col_to_char(1, 0), 6);
        assert_eq!(buf.line_col_to_char(1, 3), 9);

        assert_eq!(buf.char_to_line_col(0), (0, 0));
        assert_eq!(buf.char_to_line_col(5), (0, 5));
        assert_eq!(buf.char_to_line_col(6), (1, 0));
    }

    #[test]
    fn test_delete() {
        let mut buf = Buffer::from_str("Hello World");
        buf.delete(5, 11);
        assert_eq!(buf.line_str(0), Some("Hello".to_string()));
    }
}
