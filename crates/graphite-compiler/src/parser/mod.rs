//! Recursive-descent parser for Graphite v0.1.
//!
//! The parser consumes lexer tokens and builds an AST. It does not check types,
//! resolve `grf.json`, convert `auto T` values, or generate Rust.

mod ast;
mod parse;
mod tree;

pub use ast::*;
pub use parse::{ParseOutput, ParserDiagnostic, ParserDiagnosticKind, parse};
pub use tree::format_ast;

#[cfg(test)]
mod tests;
