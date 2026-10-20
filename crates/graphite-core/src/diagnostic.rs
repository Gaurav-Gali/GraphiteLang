//! Shared diagnostic model and renderer.
//!
//! Compiler phases and the generated program both build this view. Only the
//! renderer turns it into terminal text. Runtime helpers return `RuntimeError`
//! and do not print.

use std::fmt::Write as _;

use crate::convert::Site;

/// One rendered diagnostic. Line and column are one-based. A zero line means
/// the failure has no source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticView {
    pub code: &'static str,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_column: u32,
    pub excerpt: String,
    pub label: String,
    pub help: Option<String>,
}

/// Structured failure produced by a Graphite runtime helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub code: &'static str,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_column: u32,
    pub excerpt: String,
    pub label: String,
}

impl RuntimeError {
    pub fn at(
        site: Site<'_>,
        code: &'static str,
        message: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            file: site.file.to_string(),
            line: site.line,
            column: site.column,
            end_column: site.end_column,
            excerpt: site.excerpt.to_string(),
            label: label.into(),
        }
    }

    pub fn view(&self) -> DiagnosticView {
        DiagnosticView {
            code: self.code,
            message: self.message.clone(),
            file: self.file.clone(),
            line: self.line,
            column: self.column,
            end_column: self.end_column,
            excerpt: self.excerpt.clone(),
            label: self.label.clone(),
            help: None,
        }
    }
}

pub fn array_bounds(site: Site<'_>, index: impl std::fmt::Display, len: usize) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-001",
        "array index out of bounds",
        format!("index {index} is out of bounds for an array of length {len}"),
    )
}

pub fn missing_key(site: Site<'_>, map_type: &str) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-002",
        "missing map key",
        format!("missing key: lookup on '{map_type}'"),
    )
}

pub fn zero_step(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(site, "GRF-RUN-003", "slice step equals zero", "step 0")
}

pub fn integer_overflow(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(site, "GRF-RUN-004", "integer overflow", "integer overflow")
}

pub fn division_by_zero(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(site, "GRF-RUN-005", "division by zero", "division by zero")
}

pub fn conversion_failed(site: Site<'_>, from: &str, to: &str, category: &str) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-006",
        "numeric conversion failed",
        format!("cannot convert {from} to {to}: {category}"),
    )
}

pub fn input_output(detail: impl Into<String>) -> RuntimeError {
    RuntimeError {
        code: "GRF-RUN-007",
        message: "input/output error".to_string(),
        file: String::new(),
        line: 0,
        column: 0,
        end_column: 0,
        excerpt: String::new(),
        label: detail.into(),
    }
}

pub fn slice_length(site: Site<'_>, expected: usize, found: usize) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-008",
        "slice assignment length mismatch",
        format!("expected {expected} elements, found {found}"),
    )
}

pub fn destructure_length(site: Site<'_>, expected: usize, found: usize) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-009",
        "destructuring length mismatch",
        format!("expected {expected} elements, found {found}"),
    )
}

pub fn array_limit(site: Site<'_>, len: u64, limit: u64) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-010",
        "array exceeds its maximum length",
        format!("length {len}, maximum {limit}"),
    )
}

pub fn insertion_position(
    site: Site<'_>,
    index: impl std::fmt::Display,
    len: usize,
) -> RuntimeError {
    RuntimeError::at(
        site,
        "GRF-RUN-011",
        "invalid insertion position",
        format!("index {index}, length {len}"),
    )
}

pub fn numeric_failure(site: Site<'_>, message: impl Into<String>) -> RuntimeError {
    let message = message.into();
    RuntimeError::at(site, "GRF-RUN-006", "numeric conversion failed", message)
}

/// The single runtime renderer. The generated program calls this from its
/// entry-point handler and then exits.
pub fn report(error: &RuntimeError) {
    eprint!("{}", render(&error.view()));
}

pub fn render(diagnostic: &DiagnosticView) -> String {
    let mut out = format!("error[{}]: {}\n", diagnostic.code, diagnostic.message);
    if diagnostic.line == 0 || diagnostic.file.is_empty() {
        if !diagnostic.label.is_empty() {
            let _ = writeln!(out, "{}", diagnostic.label);
        }
    } else {
        let column = diagnostic.column.max(1);
        let _ = writeln!(
            out,
            "  --> {}:{}:{}",
            diagnostic.file, diagnostic.line, column
        );
        if !diagnostic.excerpt.is_empty() {
            let (display, starts) = display_map(&diagnostic.excerpt);
            let line_no = diagnostic.line.to_string();
            let pad = " ".repeat(line_no.len());
            let _ = writeln!(out, "{pad} |");
            let _ = writeln!(out, "{line_no} | {display}");
            let start = display_index(&starts, column);
            let end_column = if diagnostic.end_column > column {
                diagnostic.end_column
            } else {
                column.saturating_add(1)
            };
            let end = display_index(&starts, end_column).max(start.saturating_add(1));
            let caret = "^".repeat((end - start).max(1));
            let spaces = " ".repeat(start);
            if diagnostic.label.is_empty() {
                let _ = writeln!(out, "{pad} | {spaces}{caret}");
            } else {
                let _ = writeln!(out, "{pad} | {spaces}{caret} {}", diagnostic.label);
            }
        } else if !diagnostic.label.is_empty() {
            let _ = writeln!(out, "{}", diagnostic.label);
        }
    }
    if let Some(help) = &diagnostic.help {
        if !help.is_empty() {
            let _ = writeln!(out, "help: {help}");
        }
    }
    out
}

/// Maps a source line to a display line. A tab advances to the next multiple
/// of 4. Other scalars take one column, matching the compiler's column count
/// except for the tab expansion that keeps the caret aligned.
fn display_map(line: &str) -> (String, Vec<usize>) {
    let mut display = String::new();
    let mut starts = Vec::new();
    let mut width = 0usize;
    for ch in line.chars() {
        starts.push(width);
        if ch == '\t' {
            let spaces = 4 - (width % 4);
            display.push_str(&" ".repeat(spaces));
            width += spaces;
        } else {
            display.push(ch);
            width += 1;
        }
    }
    starts.push(width);
    (display, starts)
}

fn display_index(starts: &[usize], column: u32) -> usize {
    let index = column.saturating_sub(1) as usize;
    if index < starts.len() {
        starts[index]
    } else {
        starts.last().copied().unwrap_or(0)
    }
}

pub fn source_line(source: &str, line: u32) -> String {
    if line == 0 {
        return String::new();
    }
    source
        .lines()
        .nth((line - 1) as usize)
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_bounds_include_index_and_length() {
        let site = Site {
            file: "main.grf",
            line: 5,
            column: 12,
            end_column: 20,
            excerpt: "io.println(items[5]);",
        };
        let rendered = render(&array_bounds(site, 5, 3).view());
        assert!(rendered.contains("error[GRF-RUN-001]: array index out of bounds"));
        assert!(rendered.contains("--> main.grf:5:12"));
        assert!(rendered.contains("index 5 is out of bounds for an array of length 3"));
        assert!(rendered.contains("items[5]"));
    }

    #[test]
    fn missing_key_omits_the_key_and_keeps_the_map_type() {
        let site = Site {
            file: "mintest.grf",
            line: 5,
            column: 9,
            end_column: 23,
            excerpt: "value = scores[\"Ada\"];",
        };
        let rendered = render(&missing_key(site, "map<str, int64>").view());
        assert!(rendered.contains("error[GRF-RUN-002]: missing map key"));
        assert!(rendered.contains("missing key: lookup on 'map<str, int64>'"));
        assert!(!rendered.contains("missing key: Ada"));
        assert!(!rendered.contains("missing key: \"Ada\""));
    }

    #[test]
    fn input_output_has_no_fabricated_location() {
        let rendered = render(&input_output("unexpected end of input").view());
        assert!(rendered.starts_with("error[GRF-RUN-007]: input/output error\n"));
        assert!(rendered.contains("unexpected end of input"));
        assert!(!rendered.contains("-->"));
    }
}
