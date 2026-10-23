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

/// One Graphite call crossed while an error propagated.
///
/// Frames are innermost first. `column` is the logical source column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// Structured failure produced by a Graphite runtime helper.
///
/// `kind` is the canonical catch-type name. `message` is the diagnostic
/// title. `column` is the same logical source column the header prints.
/// The renderer derives caret display width from `excerpt` when it draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub kind: &'static str,
    pub code: &'static str,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_column: u32,
    pub excerpt: String,
    pub label: String,
    pub cause: Option<Box<RuntimeError>>,
    pub trace: Vec<TraceFrame>,
}

impl RuntimeError {
    pub fn at(
        site: Site<'_>,
        kind: &'static str,
        code: &'static str,
        message: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
            file: site.file.to_string(),
            line: site.line,
            column: site.column,
            end_column: site.end_column,
            excerpt: site.excerpt.to_string(),
            label: label.into(),
            cause: None,
            trace: Vec::new(),
        }
    }

    pub fn kind_name(&self) -> &'static str {
        self.kind
    }

    pub fn message_text(&self) -> String {
        self.message.clone()
    }

    pub fn source_file(&self) -> String {
        if self.line == 0 {
            String::new()
        } else {
            self.file.clone()
        }
    }

    pub fn line_number(&self) -> u64 {
        u64::from(self.line)
    }

    pub fn column_number(&self) -> u64 {
        if self.line == 0 {
            0
        } else {
            u64::from(self.column)
        }
    }

    pub fn cause_message(&self) -> String {
        self.cause
            .as_ref()
            .map(|cause| cause.message.clone())
            .unwrap_or_default()
    }

    pub fn stack_trace(&self) -> String {
        self.trace
            .iter()
            .map(|frame| {
                if frame.line == 0 || frame.file.is_empty() {
                    frame.function.clone()
                } else {
                    format!(
                        "{} at {}:{}:{}",
                        frame.function, frame.file, frame.line, frame.column
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The pending error becomes the direct cause. An existing cause chain on
    /// either error is kept behind that direct cause.
    pub fn with_cause(mut self, mut pending: RuntimeError) -> Self {
        if let Some(existing) = self.cause.take() {
            append_cause(&mut pending, *existing);
        }
        self.cause = Some(Box::new(pending));
        self
    }

    pub fn enter_call(mut self, function: &str, site: Site<'_>) -> Self {
        let frame = TraceFrame {
            function: function.to_string(),
            file: site.file.to_string(),
            line: site.line,
            column: site.column,
        };
        if self.trace.last().is_some_and(|last| {
            last.function == frame.function
                && last.file == frame.file
                && last.line == frame.line
                && last.column == frame.column
        }) {
            return self;
        }
        self.trace.push(frame);
        self
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

fn append_cause(error: &mut RuntimeError, cause: RuntimeError) {
    match error.cause.as_mut() {
        Some(next) => append_cause(next, cause),
        None => error.cause = Some(Box::new(cause)),
    }
}

pub fn array_bounds(site: Site<'_>, index: impl std::fmt::Display, len: usize) -> RuntimeError {
    RuntimeError::at(
        site,
        "ArrayIndexOutOfBoundsError",
        "GRF-RUN-001",
        "array index out of bounds",
        format!("index {index} is out of bounds for an array of length {len}"),
    )
}

pub fn missing_key(site: Site<'_>, map_type: &str) -> RuntimeError {
    RuntimeError::at(
        site,
        "MissingMapKeyError",
        "GRF-RUN-002",
        "missing map key",
        format!("missing key: lookup on '{map_type}'"),
    )
}

pub fn zero_step(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "InvalidSliceError",
        "GRF-RUN-003",
        "slice step equals zero",
        "step 0",
    )
}

pub fn integer_overflow(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "IntegerOverflowError",
        "GRF-RUN-004",
        "integer overflow",
        "integer overflow",
    )
}

pub fn division_by_zero(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "DivisionByZeroError",
        "GRF-RUN-005",
        "division by zero",
        "division by zero",
    )
}

pub fn remainder_by_zero(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "DivisionByZeroError",
        "GRF-RUN-005",
        "remainder by zero",
        "remainder by zero",
    )
}

pub fn conversion_failed(site: Site<'_>, from: &str, to: &str, category: &str) -> RuntimeError {
    RuntimeError::at(
        site,
        "NumericConversionError",
        "GRF-RUN-006",
        "numeric conversion failed",
        format!("cannot convert {from} to {to}: {category}"),
    )
}

pub fn input_output(detail: impl Into<String>) -> RuntimeError {
    RuntimeError {
        kind: "IoError",
        code: "GRF-RUN-007",
        message: "input/output error".to_string(),
        file: String::new(),
        line: 0,
        column: 0,
        end_column: 0,
        excerpt: String::new(),
        label: detail.into(),
        cause: None,
        trace: Vec::new(),
    }
}

pub fn slice_length(site: Site<'_>, expected: usize, found: usize) -> RuntimeError {
    RuntimeError::at(
        site,
        "InvalidSliceError",
        "GRF-RUN-008",
        "slice assignment length mismatch",
        format!("expected {expected} elements, found {found}"),
    )
}

pub fn destructure_length(site: Site<'_>, expected: usize, found: usize) -> RuntimeError {
    RuntimeError::at(
        site,
        "RuntimeError",
        "GRF-RUN-009",
        "destructuring length mismatch",
        format!("expected {expected} elements, found {found}"),
    )
}

pub fn array_limit(site: Site<'_>, len: u64, limit: u64) -> RuntimeError {
    RuntimeError::at(
        site,
        "RuntimeError",
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
        "RuntimeError",
        "GRF-RUN-011",
        "invalid insertion position",
        format!("index {index}, length {len}"),
    )
}

pub fn numeric_failure(site: Site<'_>, message: impl Into<String>) -> RuntimeError {
    let message = message.into();
    RuntimeError::at(
        site,
        "NumericConversionError",
        "GRF-RUN-006",
        "numeric conversion failed",
        message,
    )
}

pub fn external_library(detail: impl Into<String>) -> RuntimeError {
    RuntimeError {
        kind: "ExternalLibraryError",
        code: "GRF-RUN-012",
        message: "external library error".to_string(),
        file: String::new(),
        line: 0,
        column: 0,
        end_column: 0,
        excerpt: String::new(),
        label: detail.into(),
        cause: None,
        trace: Vec::new(),
    }
}

pub fn empty_pop(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "ValueError",
        "GRF-RUN-016",
        "pop from empty sequence",
        "empty sequence",
    )
}

pub fn value_not_found(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "ValueError",
        "GRF-RUN-016",
        "value was not found",
        "value was not found",
    )
}

pub fn empty_separator(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "ValueError",
        "GRF-RUN-016",
        "empty separator",
        "empty separator",
    )
}

pub fn null_access(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "NullAccessError",
        "GRF-RUN-015",
        "null value",
        "null value",
    )
}

pub fn const_violation(site: Site<'_>) -> RuntimeError {
    RuntimeError::at(
        site,
        "ConstViolationError",
        "GRF-RUN-014",
        "mutation of frozen value",
        "mutation of frozen value",
    )
}

pub fn runtime_internal(detail: impl Into<String>) -> RuntimeError {
    RuntimeError {
        kind: "RuntimeInternalError",
        code: "GRF-RUN-013",
        message: "internal runtime error".to_string(),
        file: String::new(),
        line: 0,
        column: 0,
        end_column: 0,
        excerpt: String::new(),
        label: detail.into(),
        cause: None,
        trace: Vec::new(),
    }
}

/// Maps one controlled library `Err` into a Graphite runtime error.
///
/// This is the adapter boundary. It does not catch panics.
pub fn adapt_library_error(detail: impl Into<String>) -> Result<(), RuntimeError> {
    Err(external_library(detail))
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

    #[test]
    fn kind_message_and_code_stay_distinct() {
        let site = Site {
            file: "main.grf",
            line: 2,
            column: 5,
            end_column: 10,
            excerpt: "\t1 / 0",
        };
        let division = division_by_zero(site);
        assert_eq!(division.message_text(), "division by zero");
        assert_eq!(division.kind_name(), "DivisionByZeroError");
        assert_eq!(division.code, "GRF-RUN-005");
        assert_eq!(division.column_number(), 5);
        let remainder = remainder_by_zero(site);
        assert_eq!(remainder.message_text(), "remainder by zero");
        assert_eq!(remainder.kind_name(), "DivisionByZeroError");
        assert_eq!(remainder.code, "GRF-RUN-005");
        assert_eq!(destructure_length(site, 2, 1).kind_name(), "RuntimeError");
        assert_eq!(input_output("eof").kind_name(), "IoError");
        assert_eq!(input_output("eof").column_number(), 0);
    }

    #[test]
    fn cause_chain_keeps_one_level_message() {
        let site = Site {
            file: "main.grf",
            line: 1,
            column: 1,
            end_column: 2,
            excerpt: "1",
        };
        let division = division_by_zero(site);
        let remainder = remainder_by_zero(site).with_cause(division);
        assert_eq!(remainder.cause_message(), "division by zero");
        let overflow = integer_overflow(site).with_cause(remainder);
        assert_eq!(overflow.cause_message(), "remainder by zero");
        assert_eq!(
            overflow.cause.unwrap().cause.unwrap().message,
            "division by zero"
        );
    }

    #[test]
    fn library_adapter_and_internal_error_have_their_kinds() {
        let Err(error) = adapt_library_error("controlled failure") else {
            panic!("adapter must return the library error");
        };
        assert_eq!(error.kind_name(), "ExternalLibraryError");
        assert_eq!(error.code, "GRF-RUN-012");
        assert_eq!(error.message_text(), "external library error");
        assert!(error.label.contains("controlled failure"));
        let internal = runtime_internal("unexpected");
        assert_eq!(internal.kind_name(), "RuntimeInternalError");
        assert_eq!(internal.code, "GRF-RUN-013");
        assert_ne!(input_output("eof").kind_name(), "ExternalLibraryError");
        let frozen = const_violation(Site {
            file: "main.grf",
            line: 3,
            column: 4,
            end_column: 8,
            excerpt: "alias",
        });
        assert_eq!(frozen.kind_name(), "ConstViolationError");
        assert_eq!(frozen.code, "GRF-RUN-014");
        assert_eq!(frozen.message_text(), "mutation of frozen value");
        assert_eq!(external_library("x").code, "GRF-RUN-012");
    }
}
