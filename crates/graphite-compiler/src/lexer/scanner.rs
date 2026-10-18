use crate::diagnostics::{Diagnostic, DiagnosticKind};
use crate::span::{SourcePosition, Span};

use super::token::{Token, TokenKind, keyword};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(source_name: &str, source: &str) -> LexResult {
    let mut scanner = Scanner::new(source_name, source);
    scanner.tokenize();
    LexResult {
        tokens: scanner.tokens,
        diagnostics: scanner.diagnostics,
    }
}

struct Scanner<'a> {
    source_name: String,
    source: &'a str,
    offset: usize,
    line: u32,
    column: u32,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Scanner<'a> {
    fn new(source_name: &str, source: &'a str) -> Self {
        Self {
            source_name: source_name.to_string(),
            source,
            offset: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn tokenize(&mut self) {
        while !self.is_at_end() {
            let before = self.offset;
            self.skip_trivia();
            if self.is_at_end() {
                break;
            }
            self.scan_token();
            if self.offset == before {
                let start = self.position();
                self.bump();
                self.unexpected(start);
            }
        }
        self.emit_eof();
    }

    fn scan_token(&mut self) {
        if self.peek() == Some('f') && self.peek_second() == Some('"') {
            self.scan_interpolated_string();
            return;
        }
        if self.peek() == Some('"') {
            self.scan_plain_string();
            return;
        }
        if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.scan_number();
            return;
        }

        let start = self.position();
        let Some(ch) = self.bump() else {
            return;
        };

        match ch {
            '(' => self.emit(TokenKind::LeftParen, start, None),
            ')' => self.emit(TokenKind::RightParen, start, None),
            '{' => self.emit(TokenKind::LeftBrace, start, None),
            '}' => self.emit(TokenKind::RightBrace, start, None),
            '[' => self.emit(TokenKind::LeftBracket, start, None),
            ']' => self.emit(TokenKind::RightBracket, start, None),
            ',' => self.emit(TokenKind::Comma, start, None),
            '.' => self.emit(TokenKind::Dot, start, None),
            ';' => self.emit(TokenKind::Semicolon, start, None),
            '*' => self.emit(TokenKind::Star, start, None),
            '%' => self.emit(TokenKind::Percent, start, None),
            '/' => self.emit(TokenKind::Slash, start, None),
            '+' => {
                if self.consume('+') {
                    self.emit(TokenKind::PlusPlus, start, None);
                } else {
                    self.emit(TokenKind::Plus, start, None);
                }
            }
            '-' => {
                if self.consume('-') {
                    self.emit(TokenKind::MinusMinus, start, None);
                } else {
                    self.emit(TokenKind::Minus, start, None);
                }
            }
            '=' => {
                if self.consume('=') {
                    self.emit(TokenKind::EqualEqual, start, None);
                } else {
                    self.emit(TokenKind::Equal, start, None);
                }
            }
            '!' => {
                if self.consume('=') {
                    self.emit(TokenKind::BangEqual, start, None);
                } else {
                    self.emit(TokenKind::Bang, start, None);
                }
            }
            '<' => {
                if self.consume('=') {
                    self.emit(TokenKind::LessEqual, start, None);
                } else {
                    self.emit(TokenKind::Less, start, None);
                }
            }
            '>' => {
                if self.consume('=') {
                    self.emit(TokenKind::GreaterEqual, start, None);
                } else {
                    self.emit(TokenKind::Greater, start, None);
                }
            }
            '&' => {
                if self.consume('&') {
                    self.emit(TokenKind::AndAnd, start, None);
                } else {
                    self.error(
                        DiagnosticKind::UnexpectedCharacter,
                        start,
                        "unexpected character '&'",
                        Some("'&&' is the logical operator".to_string()),
                    );
                }
            }
            '|' => {
                if self.consume('|') {
                    self.emit(TokenKind::OrOr, start, None);
                } else {
                    self.error(
                        DiagnosticKind::UnexpectedCharacter,
                        start,
                        "unexpected character '|'",
                        Some("'||' is the logical operator".to_string()),
                    );
                }
            }
            ':' => self.emit(TokenKind::Colon, start, None),
            '?' => self.error(
                DiagnosticKind::UnexpectedCharacter,
                start,
                "unexpected character '?'",
                Some("'?' is not a v0.1 operator".to_string()),
            ),
            ch if is_ident_start(ch) => self.finish_identifier(start),
            _ => self.unexpected(start),
        }
    }

    fn finish_identifier(&mut self, start: SourcePosition) {
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }
        let lexeme = &self.source[start.offset..self.offset];
        let kind = keyword(lexeme).unwrap_or(TokenKind::Identifier);
        self.emit(kind, start, None);
    }

    fn scan_number(&mut self) {
        let start = self.position();
        self.consume_digits();

        let mut is_float = false;
        if self.peek() == Some('.') && self.peek_second().is_some_and(|ch| ch.is_ascii_digit()) {
            is_float = true;
            self.bump();
            self.consume_digits();
        }

        let mut malformed = false;
        if matches!(self.peek(), Some('e' | 'E')) {
            is_float = true;
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                self.consume_digits();
            } else {
                malformed = true;
            }
        }

        let kind = if is_float {
            TokenKind::FloatLiteral
        } else {
            TokenKind::IntegerLiteral
        };
        self.emit(kind, start, None);
        if malformed {
            self.error(
                DiagnosticKind::MalformedNumericLiteral,
                start,
                "malformed numeric literal",
                Some("an exponent needs at least one digit".to_string()),
            );
        }
    }

    fn consume_digits(&mut self) {
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            self.bump();
        }
    }

    fn scan_plain_string(&mut self) {
        let start = self.position();
        self.bump();
        let mut decoded = String::new();
        let mut valid = true;

        loop {
            match self.peek() {
                None => {
                    self.emit_text(TokenKind::StringLiteral, start, &decoded, false);
                    self.error(
                        DiagnosticKind::UnterminatedString,
                        start,
                        "unterminated string",
                        Some("add a closing quote".to_string()),
                    );
                    return;
                }
                Some('\n' | '\r') => {
                    self.emit_text(TokenKind::StringLiteral, start, &decoded, false);
                    self.error(
                        DiagnosticKind::UnterminatedString,
                        start,
                        "line ending inside a string",
                        Some("close the string before the line ending".to_string()),
                    );
                    return;
                }
                Some('"') => {
                    self.bump();
                    self.emit_text(TokenKind::StringLiteral, start, &decoded, valid);
                    return;
                }
                Some('\\') => {
                    if !self.scan_escape(&mut decoded) {
                        valid = false;
                    }
                }
                Some(ch) => {
                    decoded.push(ch);
                    self.bump();
                }
            }
        }
    }

    fn scan_interpolated_string(&mut self) {
        let start = self.position();
        self.bump();
        self.bump();
        self.emit(TokenKind::InterpolatedStringStart, start, None);

        loop {
            match self.scan_string_text() {
                StringTextEnd::Quote => {
                    let quote = self.position();
                    self.bump();
                    self.emit(TokenKind::StringEnd, quote, None);
                    return;
                }
                StringTextEnd::Interpolation => {
                    let brace = self.position();
                    self.bump();
                    self.emit(TokenKind::InterpolationStart, brace, None);
                    if !self.scan_interpolation_body() {
                        return;
                    }
                }
                StringTextEnd::Unterminated => return,
            }
        }
    }

    fn scan_string_text(&mut self) -> StringTextEnd {
        let start = self.position();
        let mut decoded = String::new();
        let mut valid = true;

        loop {
            match self.peek() {
                None => {
                    self.emit_text_if_present(start, &decoded, false);
                    self.error(
                        DiagnosticKind::UnterminatedInterpolation,
                        start,
                        "unterminated interpolated string",
                        Some("add a closing quote".to_string()),
                    );
                    return StringTextEnd::Unterminated;
                }
                Some('\n' | '\r') => {
                    self.emit_text_if_present(start, &decoded, false);
                    self.error(
                        DiagnosticKind::UnterminatedString,
                        start,
                        "line ending inside a string",
                        Some("close the string before the line ending".to_string()),
                    );
                    return StringTextEnd::Unterminated;
                }
                Some('"') => {
                    self.emit_text_if_present(start, &decoded, valid);
                    return StringTextEnd::Quote;
                }
                Some('{') => {
                    self.emit_text_if_present(start, &decoded, valid);
                    return StringTextEnd::Interpolation;
                }
                Some('\\') => {
                    if !self.scan_escape(&mut decoded) {
                        valid = false;
                    }
                }
                Some(ch) => {
                    decoded.push(ch);
                    self.bump();
                }
            }
        }
    }

    /// Scans the expression inside an interpolation that is already open.
    /// Returns false when the interpolation runs to EOF.
    fn scan_interpolation_body(&mut self) -> bool {
        let mut depth = 1u32;
        loop {
            self.skip_trivia();
            if self.is_at_end() {
                let here = self.position();
                self.error(
                    DiagnosticKind::UnterminatedInterpolation,
                    here,
                    "unterminated interpolation",
                    Some("close the interpolation with '}'".to_string()),
                );
                return false;
            }

            match self.peek() {
                Some('}') => {
                    let start = self.position();
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        self.emit(TokenKind::InterpolationEnd, start, None);
                        return true;
                    }
                    self.emit(TokenKind::RightBrace, start, None);
                }
                Some('{') => {
                    let start = self.position();
                    self.bump();
                    depth += 1;
                    self.emit(TokenKind::LeftBrace, start, None);
                }
                _ => {
                    let before = self.offset;
                    self.scan_token();
                    if self.offset == before && !self.is_at_end() {
                        let start = self.position();
                        self.bump();
                        self.unexpected(start);
                    }
                }
            }
        }
    }

    fn scan_escape(&mut self, decoded: &mut String) -> bool {
        let start = self.position();
        self.bump();
        let Some(ch) = self.bump() else {
            self.error(
                DiagnosticKind::InvalidEscape,
                start,
                "unterminated escape sequence",
                None,
            );
            return false;
        };
        match ch {
            '"' => decoded.push('"'),
            '\\' => decoded.push('\\'),
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            '0' => decoded.push('\0'),
            _ => {
                self.error(
                    DiagnosticKind::InvalidEscape,
                    start,
                    format!("invalid escape sequence '\\{ch}'"),
                    None,
                );
                return false;
            }
        }
        true
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r') => {
                    self.bump();
                }
                Some('/') if self.peek_second() == Some('/') => self.skip_line_comment(),
                Some('/') if self.peek_second() == Some('*') => self.skip_block_comment(),
                _ => return,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        self.bump();
        self.bump();
        while matches!(self.peek(), Some(ch) if ch != '\n' && ch != '\r') {
            self.bump();
        }
    }

    fn skip_block_comment(&mut self) {
        let start = self.position();
        self.bump();
        self.bump();
        loop {
            match self.peek() {
                None => {
                    self.error(
                        DiagnosticKind::UnterminatedBlockComment,
                        start,
                        "unterminated block comment",
                        Some("close the comment with '*/'".to_string()),
                    );
                    return;
                }
                Some('*') if self.peek_second() == Some('/') => {
                    self.bump();
                    self.bump();
                    return;
                }
                Some(_) => {
                    self.bump();
                }
            }
        }
    }

    fn emit_text(&mut self, kind: TokenKind, start: SourcePosition, decoded: &str, valid: bool) {
        let decoded = if valid {
            Some(decoded.to_string())
        } else {
            None
        };
        self.emit(kind, start, decoded);
    }

    fn emit_text_if_present(&mut self, start: SourcePosition, decoded: &str, valid: bool) {
        if self.offset != start.offset {
            self.emit_text(TokenKind::StringText, start, decoded, valid);
        }
    }

    fn emit(&mut self, kind: TokenKind, start: SourcePosition, decoded: Option<String>) {
        let end = self.position();
        let lexeme = self.source[start.offset..end.offset].to_string();
        self.tokens.push(Token {
            kind,
            lexeme,
            span: Span { start, end },
            decoded,
        });
    }

    fn emit_eof(&mut self) {
        let pos = self.position();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            lexeme: String::new(),
            span: Span {
                start: pos,
                end: pos,
            },
            decoded: None,
        });
    }

    fn error(
        &mut self,
        kind: DiagnosticKind,
        start: SourcePosition,
        message: impl Into<String>,
        hint: Option<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            source_name: self.source_name.clone(),
            span: Span {
                start,
                end: self.position(),
            },
            kind,
            message: message.into(),
            hint,
        });
    }

    fn unexpected(&mut self, start: SourcePosition) {
        let ch = self.source[start.offset..self.offset].chars().next();
        let message = match ch {
            Some(ch) => format!("unexpected character {ch:?}"),
            None => "unexpected character".to_string(),
        };
        self.error(DiagnosticKind::UnexpectedCharacter, start, message, None);
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn position(&self) -> SourcePosition {
        SourcePosition {
            offset: self.offset,
            line: self.line,
            column: self.column,
        }
    }

    fn is_at_end(&self) -> bool {
        self.offset >= self.source.len()
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        let mut chars = self.source[self.offset..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.offset += ch.len_utf8();
        if ch == '\r' {
            if self.source.as_bytes().get(self.offset) == Some(&b'\n') {
                self.offset += 1;
            }
            self.line += 1;
            self.column = 1;
        } else if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }
}

enum StringTextEnd {
    Quote,
    Interpolation,
    Unterminated,
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}
