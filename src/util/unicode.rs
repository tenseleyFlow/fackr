#![allow(dead_code)]

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Get the display width of a string (handling wide chars like CJK)
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Count grapheme clusters in a string
pub fn grapheme_count(s: &str) -> usize {
    s.graphemes(true).count()
}

/// Get the nth grapheme from a string
pub fn nth_grapheme(s: &str, n: usize) -> Option<&str> {
    s.graphemes(true).nth(n)
}

/// Convert a grapheme index to byte offset
pub fn grapheme_to_byte_offset(s: &str, grapheme_idx: usize) -> usize {
    s.graphemes(true)
        .take(grapheme_idx)
        .map(|g| g.len())
        .sum()
}

/// Convert a byte offset to grapheme index
pub fn byte_to_grapheme_offset(s: &str, byte_idx: usize) -> usize {
    let mut count = 0;
    let mut bytes = 0;
    for g in s.graphemes(true) {
        if bytes >= byte_idx {
            break;
        }
        bytes += g.len();
        count += 1;
    }
    count
}
