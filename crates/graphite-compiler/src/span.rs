/// A position in one Graphite source buffer.
///
/// `offset` is a zero-based UTF-8 byte index. `line` and `column` are one-based.
/// Columns count Unicode scalar values. A tab is one column. Byte offsets stay
/// authoritative for slicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub offset: usize,
    pub line: u32,
    pub column: u32,
}

/// A half-open source range. `end` is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: SourcePosition,
    pub end: SourcePosition,
}
