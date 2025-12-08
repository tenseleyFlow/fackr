//! Core syntax highlighting engine

#![allow(dead_code)]

use super::languages::{Language, LanguageDef};
use crossterm::style::Color;

/// Token types for syntax highlighting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Plain,
    Keyword,
    String,
    Number,
    Comment,
    Operator,
    Type,
    Function,
    Preprocessor,
    Attribute,
    Punctuation,
}

impl TokenType {
    /// Get the foreground color for this token type
    pub fn color(&self) -> Color {
        match self {
            TokenType::Plain => Color::Reset,
            TokenType::Keyword => Color::Blue,
            TokenType::String => Color::Green,
            TokenType::Number => Color::Magenta,
            TokenType::Comment => Color::DarkGrey,
            TokenType::Operator => Color::Yellow,
            TokenType::Type => Color::Cyan,
            TokenType::Function => Color::Cyan,
            TokenType::Preprocessor => Color::Magenta,
            TokenType::Attribute => Color::Yellow,
            TokenType::Punctuation => Color::DarkGrey,
        }
    }

    /// Whether this token type should be bold
    pub fn bold(&self) -> bool {
        matches!(self, TokenType::Keyword | TokenType::Function)
    }
}

/// A token in a line of text
#[derive(Debug, Clone)]
pub struct Token {
    /// Token type
    pub token_type: TokenType,
    /// Start column (character index, not byte)
    pub start: usize,
    /// End column (exclusive, character index)
    pub end: usize,
}

/// State for multiline constructs (comments, strings)
#[derive(Debug, Clone, Default)]
pub struct HighlightState {
    /// Currently in a multiline comment
    pub in_block_comment: bool,
    /// Currently in a multiline string (stores delimiter for matching)
    pub in_multiline_string: Option<String>,
}

/// Syntax highlighter for a specific language
#[derive(Debug)]
pub struct Highlighter {
    /// Current language definition
    language: Option<LanguageDef>,
    /// State for multiline constructs
    state: HighlightState,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    /// Create a new highlighter with no language
    pub fn new() -> Self {
        Self {
            language: None,
            state: HighlightState::default(),
        }
    }

    /// Detect and set language based on filename
    pub fn detect_language(&mut self, filename: &str) {
        self.language = Language::detect(filename).map(|l| l.definition());
        self.state = HighlightState::default();
    }

    /// Set language explicitly
    pub fn set_language(&mut self, lang: Language) {
        self.language = Some(lang.definition());
        self.state = HighlightState::default();
    }

    /// Clear language (disable highlighting)
    pub fn clear_language(&mut self) {
        self.language = None;
        self.state = HighlightState::default();
    }

    /// Check if highlighting is enabled
    pub fn is_enabled(&self) -> bool {
        self.language.is_some()
    }

    /// Get current language name
    pub fn language_name(&self) -> Option<&str> {
        self.language.as_ref().map(|l| l.name)
    }

    /// Reset multiline state (call when buffer changes significantly)
    pub fn reset_state(&mut self) {
        self.state = HighlightState::default();
    }

    /// Tokenize a single line, returning tokens and updated state
    /// The state should be passed from the previous line for correct multiline handling
    pub fn tokenize_line(&self, line: &str, state: &mut HighlightState) -> Vec<Token> {
        let lang = match &self.language {
            Some(l) => l,
            None => return vec![],
        };

        let mut tokens = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // Handle continuing multiline comment
            if state.in_block_comment {
                if let Some((end_start, end_len)) = self.find_block_comment_end(lang, &chars, i) {
                    tokens.push(Token {
                        token_type: TokenType::Comment,
                        start: i,
                        end: end_start + end_len,
                    });
                    i = end_start + end_len;
                    state.in_block_comment = false;
                    continue;
                } else {
                    // Rest of line is comment
                    tokens.push(Token {
                        token_type: TokenType::Comment,
                        start: i,
                        end: chars.len(),
                    });
                    break;
                }
            }

            // Handle continuing multiline string
            if let Some(ref delim) = state.in_multiline_string.clone() {
                if let Some(end_pos) = self.find_string_end(&chars, i, delim) {
                    tokens.push(Token {
                        token_type: TokenType::String,
                        start: i,
                        end: end_pos,
                    });
                    i = end_pos;
                    state.in_multiline_string = None;
                    continue;
                } else {
                    // Rest of line is string
                    tokens.push(Token {
                        token_type: TokenType::String,
                        start: i,
                        end: chars.len(),
                    });
                    break;
                }
            }

            // Skip whitespace
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }

            // Check for line comment
            if let Some(ref comment) = lang.line_comment {
                if self.matches_at(&chars, i, comment) {
                    tokens.push(Token {
                        token_type: TokenType::Comment,
                        start: i,
                        end: chars.len(),
                    });
                    break;
                }
            }

            // Check for block comment start
            if let (Some(ref start), Some(_)) = (&lang.block_comment_start, &lang.block_comment_end) {
                if self.matches_at(&chars, i, start) {
                    let comment_start = i;
                    i += start.chars().count();

                    if let Some((end_start, end_len)) = self.find_block_comment_end(lang, &chars, i) {
                        tokens.push(Token {
                            token_type: TokenType::Comment,
                            start: comment_start,
                            end: end_start + end_len,
                        });
                        i = end_start + end_len;
                    } else {
                        // Multiline comment continues
                        tokens.push(Token {
                            token_type: TokenType::Comment,
                            start: comment_start,
                            end: chars.len(),
                        });
                        state.in_block_comment = true;
                        break;
                    }
                    continue;
                }
            }

            // Check for strings
            if let Some((token, new_i, multiline_delim)) = self.try_parse_string(lang, &chars, i) {
                tokens.push(token);
                i = new_i;
                if let Some(delim) = multiline_delim {
                    state.in_multiline_string = Some(delim);
                    break;
                }
                continue;
            }

            // Check for numbers
            if let Some((token, new_i)) = self.try_parse_number(&chars, i) {
                tokens.push(token);
                i = new_i;
                continue;
            }

            // Check for preprocessor directives
            if lang.has_preprocessor && chars[i] == '#' && self.is_line_start(&chars, i) {
                tokens.push(Token {
                    token_type: TokenType::Preprocessor,
                    start: i,
                    end: chars.len(),
                });
                break;
            }

            // Check for attributes (Rust #[], Python @)
            if let Some((token, new_i)) = self.try_parse_attribute(lang, &chars, i) {
                tokens.push(token);
                i = new_i;
                continue;
            }

            // Check for identifiers (keywords, types, functions)
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();

                let token_type = if lang.keywords.contains(&word.as_str()) {
                    TokenType::Keyword
                } else if lang.types.contains(&word.as_str()) {
                    TokenType::Type
                } else if i < chars.len() && chars[i] == '(' {
                    TokenType::Function
                } else {
                    TokenType::Plain
                };

                if token_type != TokenType::Plain {
                    tokens.push(Token {
                        token_type,
                        start,
                        end: i,
                    });
                }
                continue;
            }

            // Check for operators
            if let Some((token, new_i)) = self.try_parse_operator(lang, &chars, i) {
                tokens.push(token);
                i = new_i;
                continue;
            }

            // Check for punctuation
            if lang.punctuation.contains(&chars[i]) {
                tokens.push(Token {
                    token_type: TokenType::Punctuation,
                    start: i,
                    end: i + 1,
                });
                i += 1;
                continue;
            }

            // Skip unknown character
            i += 1;
        }

        tokens
    }

    fn matches_at(&self, chars: &[char], pos: usize, pattern: &str) -> bool {
        let pattern_chars: Vec<char> = pattern.chars().collect();
        if pos + pattern_chars.len() > chars.len() {
            return false;
        }
        for (i, &pc) in pattern_chars.iter().enumerate() {
            if chars[pos + i] != pc {
                return false;
            }
        }
        true
    }

    fn find_block_comment_end(&self, lang: &LanguageDef, chars: &[char], start: usize) -> Option<(usize, usize)> {
        let end_pattern = lang.block_comment_end.as_ref()?;
        let end_chars: Vec<char> = end_pattern.chars().collect();

        for i in start..chars.len() {
            if self.matches_at(chars, i, end_pattern) {
                return Some((i, end_chars.len()));
            }
        }
        None
    }

    fn try_parse_string(&self, lang: &LanguageDef, chars: &[char], start: usize) -> Option<(Token, usize, Option<String>)> {
        let c = chars[start];

        // Check for string delimiters
        if !lang.string_delimiters.contains(&c) {
            return None;
        }

        // Check for triple-quoted strings (Python, etc.)
        if lang.multiline_strings {
            let triple: String = std::iter::repeat(c).take(3).collect();
            if self.matches_at(chars, start, &triple) {
                let delim_len = 3;
                let mut i = start + delim_len;

                while i < chars.len() {
                    if self.matches_at(chars, i, &triple) {
                        return Some((
                            Token {
                                token_type: TokenType::String,
                                start,
                                end: i + delim_len,
                            },
                            i + delim_len,
                            None,
                        ));
                    }
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 2;
                    } else {
                        i += 1;
                    }
                }

                // String continues on next line
                return Some((
                    Token {
                        token_type: TokenType::String,
                        start,
                        end: chars.len(),
                    },
                    chars.len(),
                    Some(triple),
                ));
            }
        }

        // Regular string
        let mut i = start + 1;
        while i < chars.len() {
            if chars[i] == c {
                return Some((
                    Token {
                        token_type: TokenType::String,
                        start,
                        end: i + 1,
                    },
                    i + 1,
                    None,
                ));
            }
            if chars[i] == '\\' && i + 1 < chars.len() {
                i += 2;
            } else {
                i += 1;
            }
        }

        // Unterminated string - highlight to end of line
        Some((
            Token {
                token_type: TokenType::String,
                start,
                end: chars.len(),
            },
            chars.len(),
            None,
        ))
    }

    fn find_string_end(&self, chars: &[char], start: usize, delim: &str) -> Option<usize> {
        let mut i = start;
        while i < chars.len() {
            if self.matches_at(chars, i, delim) {
                return Some(i + delim.chars().count());
            }
            if chars[i] == '\\' && i + 1 < chars.len() {
                i += 2;
            } else {
                i += 1;
            }
        }
        None
    }

    fn try_parse_number(&self, chars: &[char], start: usize) -> Option<(Token, usize)> {
        let c = chars[start];

        // Must start with digit, or . followed by digit
        if !c.is_ascii_digit() {
            if c == '.' && start + 1 < chars.len() && chars[start + 1].is_ascii_digit() {
                // .5 style float
            } else {
                return None;
            }
        }

        let mut i = start;
        let mut has_dot = c == '.';
        let mut has_exp = false;

        // Handle hex, octal, binary
        if c == '0' && i + 1 < chars.len() {
            match chars[i + 1] {
                'x' | 'X' => {
                    i += 2;
                    while i < chars.len() && (chars[i].is_ascii_hexdigit() || chars[i] == '_') {
                        i += 1;
                    }
                    return Some((Token { token_type: TokenType::Number, start, end: i }, i));
                }
                'o' | 'O' => {
                    i += 2;
                    while i < chars.len() && (chars[i].is_digit(8) || chars[i] == '_') {
                        i += 1;
                    }
                    return Some((Token { token_type: TokenType::Number, start, end: i }, i));
                }
                'b' | 'B' => {
                    i += 2;
                    while i < chars.len() && (chars[i] == '0' || chars[i] == '1' || chars[i] == '_') {
                        i += 1;
                    }
                    return Some((Token { token_type: TokenType::Number, start, end: i }, i));
                }
                _ => {}
            }
        }

        // Decimal number (possibly float)
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_ascii_digit() || ch == '_' {
                i += 1;
            } else if ch == '.' && !has_dot && !has_exp {
                // Check it's not a method call like 5.to_string()
                if i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                    has_dot = true;
                    i += 1;
                } else if i + 1 >= chars.len() {
                    has_dot = true;
                    i += 1;
                } else {
                    break;
                }
            } else if (ch == 'e' || ch == 'E') && !has_exp {
                has_exp = true;
                i += 1;
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                    i += 1;
                }
            } else {
                break;
            }
        }

        // Handle type suffixes (f32, i64, etc.)
        if i < chars.len() && chars[i].is_alphabetic() {
            let suffix_start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            // Common numeric suffixes
            let suffix: String = chars[suffix_start..i].iter().collect();
            let valid_suffixes = ["f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize",
                                  "u8", "u16", "u32", "u64", "u128", "usize", "f", "d", "l", "L"];
            if !valid_suffixes.contains(&suffix.as_str()) {
                i = suffix_start; // Not a valid suffix, rollback
            }
        }

        if i > start {
            Some((Token { token_type: TokenType::Number, start, end: i }, i))
        } else {
            None
        }
    }

    fn try_parse_operator(&self, lang: &LanguageDef, chars: &[char], start: usize) -> Option<(Token, usize)> {
        // Try longer operators first
        for &op in &lang.operators {
            if self.matches_at(chars, start, op) {
                let len = op.chars().count();
                return Some((
                    Token {
                        token_type: TokenType::Operator,
                        start,
                        end: start + len,
                    },
                    start + len,
                ));
            }
        }
        None
    }

    fn try_parse_attribute(&self, lang: &LanguageDef, chars: &[char], start: usize) -> Option<(Token, usize)> {
        // Rust attributes: #[...] or #![...]
        if lang.name == "Rust" && chars[start] == '#' {
            let mut i = start + 1;
            if i < chars.len() && chars[i] == '!' {
                i += 1;
            }
            if i < chars.len() && chars[i] == '[' {
                let attr_start = start;
                let mut bracket_depth = 1;
                i += 1;
                while i < chars.len() && bracket_depth > 0 {
                    match chars[i] {
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                return Some((
                    Token {
                        token_type: TokenType::Attribute,
                        start: attr_start,
                        end: i,
                    },
                    i,
                ));
            }
        }

        // Python decorators: @name
        if lang.name == "Python" && chars[start] == '@' {
            let mut i = start + 1;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '.') {
                i += 1;
            }
            if i > start + 1 {
                return Some((
                    Token {
                        token_type: TokenType::Attribute,
                        start,
                        end: i,
                    },
                    i,
                ));
            }
        }

        None
    }

    fn is_line_start(&self, chars: &[char], pos: usize) -> bool {
        for i in 0..pos {
            if !chars[i].is_whitespace() {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_keywords() {
        let mut hl = Highlighter::new();
        hl.set_language(Language::Rust);
        let mut state = HighlightState::default();

        let tokens = hl.tokenize_line("let x = 42;", &mut state);
        assert!(tokens.iter().any(|t| t.token_type == TokenType::Keyword)); // let
        assert!(tokens.iter().any(|t| t.token_type == TokenType::Number));  // 42
    }

    #[test]
    fn test_string_parsing() {
        let mut hl = Highlighter::new();
        hl.set_language(Language::Rust);
        let mut state = HighlightState::default();

        let tokens = hl.tokenize_line(r#"let s = "hello";"#, &mut state);
        assert!(tokens.iter().any(|t| t.token_type == TokenType::String));
    }

    #[test]
    fn test_comment_parsing() {
        let mut hl = Highlighter::new();
        hl.set_language(Language::Rust);
        let mut state = HighlightState::default();

        let tokens = hl.tokenize_line("// this is a comment", &mut state);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Comment);
    }
}
