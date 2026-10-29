//! Graphite lexer.
//!
//! The lexer turns one `.grf` buffer into tokens and lexical diagnostics.
//! It does not parse, resolve names, check types, read `grf.json`, or generate Rust.
//!
//! # Provisional lexical choices
//!
//! LLD-05 section 20 leaves the following open. The scanner needs a total rule
//! for each of them. These choices are provisional. They are not language freezes.
//!
//! - A lone CR is one line ending. CRLF is one line ending. LF is one line ending.
//! - `10.` is an integer token followed by a dot. A float requires digits after
//!   the decimal point, which is the conservative grammar in LLD-05 section 8.2.
//! - Leading zeros are decimal digits. `00` is not octal. This is the simplest
//!   initial rule in LLD-05 section 8.1.
//! - Numeric separators, hexadecimal, binary, and suffixes are not recognized.
//! - The escape set is `\"`, `\\`, `\n`, `\r`, `\t`, and `\0`. Any other escape
//!   is a lexical error and adds no decoded character.
//! - In an interpolated string, `{` starts an interpolation and `}` outside an
//!   interpolation is ordinary text. `{{` and `}}` are not escapes.
//! - Block comments do not nest. That is the proposed v0.1 rule.
//! - Identifiers are ASCII, including a leading `_`. Casing and underscore
//!   naming policy are later checks, not lexical errors.

mod scanner;
mod token;

pub use scanner::{LexResult, lex};
pub use token::{Token, TokenKind};
pub(crate) use token::keyword_lexeme;

#[cfg(test)]
mod tests;
