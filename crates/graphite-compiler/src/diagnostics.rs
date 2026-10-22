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

impl Diagnostic {
    pub fn code(&self) -> &'static str {
        match self.kind {
            DiagnosticKind::UnexpectedCharacter => "GRF-LEX-001",
            DiagnosticKind::UnterminatedString => "GRF-LEX-002",
            DiagnosticKind::InvalidEscape => "GRF-LEX-003",
            DiagnosticKind::UnterminatedBlockComment => "GRF-LEX-004",
            DiagnosticKind::MalformedNumericLiteral => "GRF-LEX-005",
            DiagnosticKind::UnterminatedInterpolation => "GRF-LEX-006",
            DiagnosticKind::MalformedInterpolation => "GRF-LEX-007",
        }
    }
}

/// Stable code for one semantic or type diagnostic. The match is ordered so
/// more specific wording wins. Every arm is a distinct published condition.
pub fn semantic_code(message: &str) -> &'static str {
    if message.starts_with("a try statement needs") {
        "GRF-EXC-001"
    } else if message.starts_with("unknown catch type") {
        "GRF-EXC-002"
    } else if message.starts_with("unreachable catch") {
        "GRF-EXC-003"
    } else if message.contains("inside finally") {
        "GRF-EXC-006"
    } else if message.contains("runtime error") {
        "GRF-EXC-005"
    } else if message.starts_with("type mismatch:") || message.starts_with("expected a ") {
        "GRF-TYP-001"
    } else if message.starts_with("operator ")
        || message.starts_with("unary ")
        || message.contains("cannot compare")
    {
        "GRF-TYP-002"
    } else if message.starts_with("cannot convert ") {
        "GRF-TYP-003"
    } else if message.contains("main") {
        "GRF-SEM-003"
    } else if message.contains("return") {
        "GRF-SEM-004"
    } else if message.contains("void") {
        "GRF-TYP-004"
    } else if message.contains("must have type bln") {
        "GRF-TYP-005"
    } else if message.contains("cannot be a map key") {
        "GRF-TYP-006"
    } else if message.contains("is not an array") || message.contains("is not a map") {
        "GRF-TYP-007"
    } else if message.starts_with("lengthOf ") || message.starts_with("typeOf ") {
        "GRF-TYP-008"
    } else if message.starts_with("integer literal") {
        "GRF-TYP-010"
    } else if message.starts_with("floating-point literal") {
        "GRF-TYP-011"
    } else if message.contains("enumeration index") {
        "GRF-TYP-012"
    } else if message.starts_with("an index must be an integer") {
        "GRF-TYP-013"
    } else if message.starts_with("a conversion expects") {
        "GRF-TYP-015"
    } else if message.starts_with("unknown name ") || message.starts_with("unknown function ") {
        "GRF-NAM-001"
    } else if message.contains("already declared")
        || message.contains("duplicate ")
        || message.starts_with("duplicate ")
        || message.contains("was imported more than once")
    {
        if message.contains("imported more than once") {
            "GRF-DEP-002"
        } else {
            "GRF-NAM-002"
        }
    } else if message.contains("reserved") || message == "'Type' is a built-in type" {
        "GRF-NAM-003"
    } else if message.starts_with("unknown type ") || message.starts_with("unknown class ") {
        "GRF-NAM-006"
    } else if message.starts_with("unknown field ")
        || message.contains("has no field")
        || message.starts_with("unknown method ")
    {
        "GRF-NAM-007"
    } else if message.contains("was not found") && message.starts_with("domain ") {
        "GRF-DEP-001"
    } else if message.contains("was not found in domain") {
        "GRF-DEP-004"
    } else if message.starts_with("import ") {
        "GRF-DEP-003"
    } else if message.contains("is not exported") {
        "GRF-DEP-005"
    } else if message.contains("unknown return type") {
        "GRF-DEP-006"
    } else if message.contains("constant") {
        "GRF-SEM-001"
    } else if message.contains("outside a loop") {
        "GRF-SEM-002"
    } else if message.contains("record type cycle") {
        "GRF-SEM-005"
    } else if message.contains("record literal")
        || message.contains("record field name")
        || message.contains("at least one field")
    {
        "GRF-SEM-006"
    } else if message.contains("array literal") || message.contains("exceeds the maximum") {
        "GRF-SEM-007"
    } else if message.contains("loop iterable")
        || message.contains("can only be used as a loop source")
    {
        "GRF-SEM-008"
    } else if message.contains("destructur") {
        "GRF-SEM-009"
    } else if message.contains("auto is not allowed") {
        "GRF-SEM-010"
    } else if message.contains("short-circuit") {
        "GRF-SEM-011"
    } else if message.contains("not accessible") {
        "GRF-SEM-023"
    } else if message.contains("not initialized")
        || message.contains("before it is initialized")
        || message.contains("before every field")
        || message.contains("has no default")
        || message.contains("used before it is assigned")
    {
        "GRF-SEM-024"
    } else if message.contains("cannot escape") {
        "GRF-SEM-025"
    } else if message.contains("nullable")
        || message.contains("null requires")
        || message.contains("null cannot")
    {
        "GRF-SEM-026"
    } else if message.contains("call") || message.contains("member") || message.contains("library")
    {
        "GRF-SEM-012"
    } else if message.contains("assignment target")
        || message.contains("requires a mutable variable")
        || message.contains("compound assignment")
    {
        "GRF-SEM-013"
    } else if message.contains("increment and decrement") {
        "GRF-SEM-014"
    } else if message.contains("do not support slicing") {
        "GRF-SEM-015"
    } else if message.contains("expects")
        || message.contains("has no method")
        || message.starts_with("construct ")
        || message.contains("requires an instance")
        || message.contains("static method")
        || (message.starts_with("expected ") && message.contains("argument"))
    {
        "GRF-SEM-016"
    } else if message.contains("being iterated") {
        "GRF-SEM-017"
    } else if message.contains("brace literal") {
        "GRF-SEM-018"
    } else if message.contains("spread") {
        "GRF-SEM-019"
    } else if message.contains("map type requires") {
        "GRF-SEM-020"
    } else if message.contains("requires a condition") {
        "GRF-SEM-021"
    } else if message.contains("cannot call a value") {
        "GRF-SEM-022"
    } else {
        "GRF-SEM-900"
    }
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
