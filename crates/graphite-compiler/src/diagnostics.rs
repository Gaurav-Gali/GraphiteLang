use std::fmt;

use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    UnexpectedCharacter,
    UnterminatedString,
    InvalidEscape,
    UnterminatedBlockComment,
    MalformedNumericLiteral,
    UnterminatedInterpolation,
    /// Kept for the LLD-05 diagnostic set. Brace-delimiter escaping is still an
    /// open decision, so the lexer does not invent a case that produces this yet.
    #[allow(dead_code)]
    MalformedInterpolation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub source_name: String,
    pub span: Span,
    pub kind: DiagnosticKind,
    pub message: String,
    pub hint: Option<String>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.source_name, self.span.start.line, self.span.start.column, self.message
        )?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  hint: {hint}")?;
        }
        Ok(())
    }
}
