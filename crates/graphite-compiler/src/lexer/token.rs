use std::fmt;

use crate::span::Span;

/// v0.1 token kinds from LLD-05, plus `StringEnd`.
///
/// `StringEnd` is the closing quote of an interpolated string. LLD-05 section 9.2
/// uses it so the token stream has a boundary the parser can see. It is not new
/// Graphite syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,

    Identifier,
    IntegerLiteral,
    FloatLiteral,
    StringLiteral,
    InterpolatedStringStart,
    StringText,
    InterpolationStart,
    InterpolationEnd,
    StringEnd,

    KwImport,
    KwAuto,
    KwConst,
    KwVoid,
    KwInt,
    KwUint,
    KwFlt,
    KwInt16,
    KwInt32,
    KwInt64,
    KwUint16,
    KwUint32,
    KwUint64,
    KwFlt32,
    KwFlt64,
    KwBln,
    KwChr,
    KwStr,
    KwIf,
    KwElse,
    KwLoop,
    KwBreak,
    KwContinue,
    KwReturn,
    KwType,
    KwMap,
    KwTrue,
    KwFalse,
    KwTry,
    KwCatch,
    KwFinally,
    KwThrow,
    KwClass,
    KwPublic,
    KwPrivate,
    KwThis,
    KwInit,
    KwNew,
    KwStatic,
    KwNull,

    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Dot,
    DotDotDot,
    Colon,
    Question,
    Semicolon,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    StarStar,
    StarStarEqual,
    SlashSlashEqual,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Exact source spelling. Numeric tokens keep this spelling even when the
    /// value is too large for a Rust integer.
    pub lexeme: String,
    pub span: Span,
    /// Decoded text for a well-formed `StringLiteral` or `StringText`.
    /// Absent when the token is not text, or when an escape error makes the
    /// decoded value unreliable.
    pub decoded: Option<String>,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            TokenKind::Identifier => write!(f, "Identifier(\"{}\")", self.lexeme),
            TokenKind::IntegerLiteral => write!(f, "IntegerLiteral(\"{}\")", self.lexeme),
            TokenKind::FloatLiteral => write!(f, "FloatLiteral(\"{}\")", self.lexeme),
            TokenKind::StringLiteral | TokenKind::StringText => {
                let text = self.decoded.as_deref().unwrap_or("");
                write!(f, "{:?}(\"{text}\")", self.kind)
            }
            other => write!(f, "{other:?}"),
        }
    }
}

impl TokenKind {
    pub fn is_keyword(self) -> bool {
        keyword_lexeme(self).is_some()
    }
}

fn keyword_lexeme(kind: TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::KwImport => "import",
        TokenKind::KwAuto => "auto",
        TokenKind::KwConst => "const",
        TokenKind::KwVoid => "void",
        TokenKind::KwInt => "int",
        TokenKind::KwUint => "uint",
        TokenKind::KwFlt => "flt",
        TokenKind::KwInt16 => "int16",
        TokenKind::KwInt32 => "int32",
        TokenKind::KwInt64 => "int64",
        TokenKind::KwUint16 => "uint16",
        TokenKind::KwUint32 => "uint32",
        TokenKind::KwUint64 => "uint64",
        TokenKind::KwFlt32 => "flt32",
        TokenKind::KwFlt64 => "flt64",
        TokenKind::KwBln => "bln",
        TokenKind::KwChr => "chr",
        TokenKind::KwStr => "str",
        TokenKind::KwIf => "if",
        TokenKind::KwElse => "else",
        TokenKind::KwLoop => "loop",
        TokenKind::KwBreak => "break",
        TokenKind::KwContinue => "continue",
        TokenKind::KwReturn => "return",
        TokenKind::KwType => "type",
        TokenKind::KwMap => "map",
        TokenKind::KwTrue => "true",
        TokenKind::KwFalse => "false",
        TokenKind::KwTry => "try",
        TokenKind::KwCatch => "catch",
        TokenKind::KwFinally => "finally",
        TokenKind::KwThrow => "throw",
        TokenKind::KwClass => "class",
        TokenKind::KwPublic => "public",
        TokenKind::KwPrivate => "private",
        TokenKind::KwThis => "this",
        TokenKind::KwInit => "init",
        TokenKind::KwNew => "new",
        TokenKind::KwStatic => "static",
        TokenKind::KwNull => "null",
        _ => return None,
    })
}

pub fn keyword(ident: &str) -> Option<TokenKind> {
    Some(match ident {
        "import" => TokenKind::KwImport,
        "auto" => TokenKind::KwAuto,
        "const" => TokenKind::KwConst,
        "void" => TokenKind::KwVoid,
        "int" => TokenKind::KwInt,
        "uint" => TokenKind::KwUint,
        "flt" => TokenKind::KwFlt,
        "int16" => TokenKind::KwInt16,
        "int32" => TokenKind::KwInt32,
        "int64" => TokenKind::KwInt64,
        "uint16" => TokenKind::KwUint16,
        "uint32" => TokenKind::KwUint32,
        "uint64" => TokenKind::KwUint64,
        "flt32" => TokenKind::KwFlt32,
        "flt64" => TokenKind::KwFlt64,
        "bln" => TokenKind::KwBln,
        "chr" => TokenKind::KwChr,
        "str" => TokenKind::KwStr,
        "if" => TokenKind::KwIf,
        "else" => TokenKind::KwElse,
        "loop" => TokenKind::KwLoop,
        "break" => TokenKind::KwBreak,
        "continue" => TokenKind::KwContinue,
        "return" => TokenKind::KwReturn,
        "type" => TokenKind::KwType,
        "map" => TokenKind::KwMap,
        "true" => TokenKind::KwTrue,
        "false" => TokenKind::KwFalse,
        "try" => TokenKind::KwTry,
        "catch" => TokenKind::KwCatch,
        "finally" => TokenKind::KwFinally,
        "throw" => TokenKind::KwThrow,
        "class" => TokenKind::KwClass,
        "public" => TokenKind::KwPublic,
        "private" => TokenKind::KwPrivate,
        "this" => TokenKind::KwThis,
        "init" => TokenKind::KwInit,
        "new" => TokenKind::KwNew,
        "static" => TokenKind::KwStatic,
        "null" => TokenKind::KwNull,
        _ => return None,
    })
}
