use std::path::PathBuf;

use crate::diagnostics::DiagnosticKind;
use crate::span::SourcePosition;

use super::{TokenKind, lex};

fn kinds(source: &str) -> Vec<TokenKind> {
    let result = lex("test", source);
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics in {source:?}: {:?}",
        result.diagnostics
    );
    result.tokens.into_iter().map(|token| token.kind).collect()
}

fn lexemes(source: &str) -> Vec<String> {
    lex("test", source)
        .tokens
        .into_iter()
        .map(|token| token.lexeme)
        .collect()
}

#[test]
fn empty_source_is_eof() {
    let result = lex("empty.grf", "");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.tokens.len(), 1);
    let eof = &result.tokens[0];
    assert_eq!(eof.kind, TokenKind::Eof);
    assert_eq!(eof.span.start, eof.span.end);
    assert_eq!(
        eof.span.start,
        SourcePosition {
            offset: 0,
            line: 1,
            column: 1
        }
    );
}

#[test]
fn declaration_tokens_and_spans() {
    let result = lex("decl.grf", "int userAge = 21;");
    assert!(result.diagnostics.is_empty());
    let expected = [
        (TokenKind::KwInt, "int", 0, 3, 1, 4),
        (TokenKind::Identifier, "userAge", 4, 11, 5, 12),
        (TokenKind::Equal, "=", 12, 13, 13, 14),
        (TokenKind::IntegerLiteral, "21", 14, 16, 15, 17),
        (TokenKind::Semicolon, ";", 16, 17, 17, 18),
        (TokenKind::Eof, "", 17, 17, 18, 18),
    ];
    assert_eq!(result.tokens.len(), expected.len());
    for (token, (kind, lexeme, start, end, start_col, end_col)) in
        result.tokens.iter().zip(expected)
    {
        assert_eq!(token.kind, kind);
        assert_eq!(token.lexeme, lexeme);
        assert_eq!(token.span.start.offset, start);
        assert_eq!(token.span.end.offset, end);
        assert_eq!(token.span.start.line, 1);
        assert_eq!(token.span.end.line, 1);
        assert_eq!(token.span.start.column, start_col);
        assert_eq!(token.span.end.column, end_col);
    }
}

#[test]
fn import_and_call_token_stream() {
    let source = "import grf.io;\nio.println(\"Hello\");\n";
    let result = lex("call.grf", source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::KwImport,
            TokenKind::Identifier,
            TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::Semicolon,
            TokenKind::Identifier,
            TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::StringLiteral,
            TokenKind::RightParen,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
    assert_eq!(result.tokens[1].lexeme, "grf");
    assert_eq!(result.tokens[3].lexeme, "io");
    assert_eq!(result.tokens[5].lexeme, "io");
    assert_eq!(result.tokens[7].lexeme, "println");
    assert_eq!(result.tokens[9].lexeme, "\"Hello\"");
    assert_eq!(result.tokens[9].decoded.as_deref(), Some("Hello"));
}

#[test]
fn library_names_are_identifiers() {
    let result = kinds("grf io fileIo coolDev netHttp println");
    assert_eq!(
        result,
        vec![
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn auto_is_a_keyword_and_not_an_identifier_prefix() {
    assert_eq!(kinds("auto"), vec![TokenKind::KwAuto, TokenKind::Eof]);
    assert_eq!(
        kinds("automation"),
        vec![TokenKind::Identifier, TokenKind::Eof]
    );
    assert_eq!(
        lexemes("automation"),
        vec!["automation".to_string(), String::new()]
    );
    assert_eq!(
        kinds("int auto = 1;"),
        vec![
            TokenKind::KwInt,
            TokenKind::KwAuto,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
    assert_eq!(kinds("Auto"), vec![TokenKind::Identifier, TokenKind::Eof]);
    assert_eq!(kinds("If"), vec![TokenKind::Identifier, TokenKind::Eof]);
    assert_eq!(kinds("ifElse"), vec![TokenKind::Identifier, TokenKind::Eof]);
}

#[test]
fn v0_1_keywords_and_unreserved_class_words() {
    let keywords = "\
auto const void int uint flt int16 int32 int64 uint16 uint32 uint64 \
flt32 flt64 bln chr str if else loop break continue return type map import true false";
    let result = kinds(keywords);
    assert!(result.iter().all(|kind| *kind != TokenKind::Identifier));
    assert_eq!(result.last(), Some(&TokenKind::Eof));
    assert_eq!(result.len(), 29);

    let ordinary = "class private this public new constructor library";
    assert!(
        kinds(ordinary)
            .iter()
            .filter(|kind| **kind != TokenKind::Eof)
            .all(|kind| *kind == TokenKind::Identifier)
    );
}

#[test]
fn operators_use_longest_match() {
    assert_eq!(
        kinds("== != <= >= && || ++ --"),
        vec![
            TokenKind::EqualEqual,
            TokenKind::BangEqual,
            TokenKind::LessEqual,
            TokenKind::GreaterEqual,
            TokenKind::AndAnd,
            TokenKind::OrOr,
            TokenKind::PlusPlus,
            TokenKind::MinusMinus,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("+ - * / % = ! < >"),
        vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Equal,
            TokenKind::Bang,
            TokenKind::Less,
            TokenKind::Greater,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("+++"),
        vec![TokenKind::PlusPlus, TokenKind::Plus, TokenKind::Eof]
    );
    assert_eq!(
        kinds("index++"),
        vec![TokenKind::Identifier, TokenKind::PlusPlus, TokenKind::Eof]
    );
    assert_eq!(
        kinds("++index"),
        vec![TokenKind::PlusPlus, TokenKind::Identifier, TokenKind::Eof]
    );
    assert_eq!(
        kinds("--index"),
        vec![TokenKind::MinusMinus, TokenKind::Identifier, TokenKind::Eof]
    );
    assert_eq!(
        kinds("index--"),
        vec![TokenKind::Identifier, TokenKind::MinusMinus, TokenKind::Eof]
    );
}

#[test]
fn deferred_operators_are_not_single_tokens() {
    assert_eq!(
        kinds("index += 1"),
        vec![
            TokenKind::Identifier,
            TokenKind::Plus,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("a -= 1"),
        vec![
            TokenKind::Identifier,
            TokenKind::Minus,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("a *= 1"),
        vec![
            TokenKind::Identifier,
            TokenKind::Star,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("a /= 1"),
        vec![
            TokenKind::Identifier,
            TokenKind::Slash,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("a %= 1"),
        vec![
            TokenKind::Identifier,
            TokenKind::Percent,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("a->b"),
        vec![
            TokenKind::Identifier,
            TokenKind::Minus,
            TokenKind::Greater,
            TokenKind::Identifier,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn colon_question_and_lone_ampersand_are_unexpected() {
    for source in ["?", "&", "|"] {
        let result = lex("op.grf", source);
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(result.tokens[0].kind, TokenKind::Eof);
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].kind,
            DiagnosticKind::UnexpectedCharacter
        );
    }
    let result = lex("op.grf", "a:b");
    assert_eq!(
        result
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::Identifier,
            TokenKind::Colon,
            TokenKind::Identifier,
            TokenKind::Eof
        ]
    );
    assert!(result.diagnostics.is_empty());
}

#[test]
fn punctuation() {
    assert_eq!(
        kinds("( ) { } [ ] , . ;"),
        vec![
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            TokenKind::LeftBracket,
            TokenKind::RightBracket,
            TokenKind::Comma,
            TokenKind::Dot,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn numbers_keep_spelling_and_reject_bad_exponents() {
    let integer = lex("n.grf", "0");
    assert_eq!(integer.tokens[0].kind, TokenKind::IntegerLiteral);
    assert_eq!(integer.tokens[0].lexeme, "0");

    assert_eq!(lexemes("00"), vec!["00".to_string(), String::new()]);
    assert_eq!(
        kinds("-42"),
        vec![TokenKind::Minus, TokenKind::IntegerLiteral, TokenKind::Eof]
    );
    assert_eq!(
        lexemes("-42"),
        vec!["-".to_string(), "42".to_string(), String::new()]
    );

    assert_eq!(
        kinds("3.14 0.5 1e6 1.25e-3"),
        vec![
            TokenKind::FloatLiteral,
            TokenKind::FloatLiteral,
            TokenKind::FloatLiteral,
            TokenKind::FloatLiteral,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        lexemes("1.25e-3"),
        vec!["1.25e-3".to_string(), String::new()]
    );

    // Provisional until LLD-05 freezes the `10.` rule: digits are required
    // after a decimal point, so the dot stays a separate token.
    assert_eq!(
        kinds("10."),
        vec![TokenKind::IntegerLiteral, TokenKind::Dot, TokenKind::Eof]
    );
    assert_eq!(kinds("10.0"), vec![TokenKind::FloatLiteral, TokenKind::Eof]);

    assert_eq!(
        kinds("2value"),
        vec![
            TokenKind::IntegerLiteral,
            TokenKind::Identifier,
            TokenKind::Eof
        ]
    );
    assert_eq!(
        kinds("0x10"),
        vec![
            TokenKind::IntegerLiteral,
            TokenKind::Identifier,
            TokenKind::Eof
        ]
    );
    assert_eq!(
        kinds("1_000"),
        vec![
            TokenKind::IntegerLiteral,
            TokenKind::Identifier,
            TokenKind::Eof
        ]
    );

    for source in ["1e", "1e+", "1.2e-"] {
        let result = lex("n.grf", source);
        assert_eq!(result.tokens[0].kind, TokenKind::FloatLiteral);
        assert_eq!(result.tokens[0].lexeme, source);
        assert_eq!(
            result.diagnostics[0].kind,
            DiagnosticKind::MalformedNumericLiteral
        );
    }

    let long = "9".repeat(10_000);
    let result = lex("n.grf", &long);
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.tokens[0].kind, TokenKind::IntegerLiteral);
    assert_eq!(result.tokens[0].lexeme.len(), 10_000);
}

#[test]
fn strings_escapes_and_comments() {
    let plain = lex("s.grf", "\"Hello\"");
    assert_eq!(plain.tokens[0].decoded.as_deref(), Some("Hello"));
    assert_eq!(plain.tokens[0].lexeme, "\"Hello\"");

    let escaped = lex("s.grf", "\"a\\\"b\\\\c\\n\\r\\t\\0\"");
    assert_eq!(
        escaped.tokens[0].decoded.as_deref(),
        Some("a\"b\\c\n\r\t\0")
    );

    let marked = lex("s.grf", "str text = \"not // a comment\"; // comment\n");
    assert!(marked.diagnostics.is_empty());
    assert_eq!(
        marked
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::KwStr,
            TokenKind::Identifier,
            TokenKind::Equal,
            TokenKind::StringLiteral,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        marked.tokens[3].decoded.as_deref(),
        Some("not // a comment")
    );

    let invalid = lex("s.grf", "\"\\q\"");
    assert_eq!(invalid.diagnostics[0].kind, DiagnosticKind::InvalidEscape);
    assert_eq!(invalid.tokens[0].kind, TokenKind::StringLiteral);
    assert!(invalid.tokens[0].decoded.is_none());

    let open = lex("s.grf", "\"abc");
    assert_eq!(open.diagnostics[0].kind, DiagnosticKind::UnterminatedString);

    let line = lex("s.grf", "\"abc\n\"");
    assert_eq!(line.diagnostics[0].kind, DiagnosticKind::UnterminatedString);
    assert!(!line.tokens[0].lexeme.contains('\n'));
}

#[test]
fn interpolated_string_structure() {
    let result = lex("s.grf", "f\"Hello, {userName}!\"");
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::InterpolatedStringStart,
            TokenKind::StringText,
            TokenKind::InterpolationStart,
            TokenKind::Identifier,
            TokenKind::InterpolationEnd,
            TokenKind::StringText,
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );
    assert_eq!(result.tokens[0].lexeme, "f\"");
    assert_eq!(result.tokens[1].decoded.as_deref(), Some("Hello, "));
    assert_eq!(result.tokens[3].lexeme, "userName");
    assert_eq!(result.tokens[5].decoded.as_deref(), Some("!"));
    assert_eq!(result.tokens[6].lexeme, "\"");

    assert_eq!(
        kinds("f = 1"),
        vec![
            TokenKind::Identifier,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Eof,
        ]
    );

    let nested = lex("s.grf", "f\"a { {1} }\"");
    assert!(nested.diagnostics.is_empty(), "{:?}", nested.diagnostics);
    assert_eq!(
        nested
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::InterpolatedStringStart,
            TokenKind::StringText,
            TokenKind::InterpolationStart,
            TokenKind::LeftBrace,
            TokenKind::IntegerLiteral,
            TokenKind::RightBrace,
            TokenKind::InterpolationEnd,
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    let braces_in_string = lex("s.grf", "f\"a {\"{\"}\"");
    assert!(
        braces_in_string.diagnostics.is_empty(),
        "{:?}",
        braces_in_string.diagnostics
    );
    assert_eq!(braces_in_string.tokens[3].kind, TokenKind::StringLiteral);
    assert_eq!(braces_in_string.tokens[3].decoded.as_deref(), Some("{"));

    let commented = lex("s.grf", "f\"x { // c\n y }\"");
    assert!(
        commented.diagnostics.is_empty(),
        "{:?}",
        commented.diagnostics
    );
    assert_eq!(commented.tokens[3].lexeme, "y");

    let open = lex("s.grf", "f\"hello {name");
    assert!(
        open.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::UnterminatedInterpolation)
    );
}

#[test]
fn comments_are_skipped_and_block_comments_do_not_nest() {
    assert_eq!(
        kinds("// comment\nint x = 1;"),
        vec![
            TokenKind::KwInt,
            TokenKind::Identifier,
            TokenKind::Equal,
            TokenKind::IntegerLiteral,
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        kinds("/* comment */ int"),
        vec![TokenKind::KwInt, TokenKind::Eof]
    );
    assert_eq!(kinds("// comment"), vec![TokenKind::Eof]);

    let nested = kinds("/* a /* b */ */");
    assert_eq!(
        nested,
        vec![TokenKind::Star, TokenKind::Slash, TokenKind::Eof]
    );

    let open = lex("c.grf", "/* hello");
    assert_eq!(
        open.diagnostics[0].kind,
        DiagnosticKind::UnterminatedBlockComment
    );
    assert_eq!(
        open.tokens.last().map(|token| token.kind),
        Some(TokenKind::Eof)
    );

    let after = lex("c.grf", "/*\n*/int");
    assert!(after.diagnostics.is_empty());
    assert_eq!(after.tokens[0].kind, TokenKind::KwInt);
    assert_eq!(after.tokens[0].span.start.line, 2);
    assert_eq!(after.tokens[0].span.start.column, 3);
    assert_eq!(after.tokens[0].span.start.offset, 5);
}

#[test]
fn line_endings_and_columns() {
    let crlf = lex("c.grf", "int\r\nx");
    assert_eq!(crlf.tokens[0].span.start.line, 1);
    assert_eq!(crlf.tokens[1].kind, TokenKind::Identifier);
    assert_eq!(crlf.tokens[1].span.start.line, 2);
    assert_eq!(crlf.tokens[1].span.start.column, 1);
    assert_eq!(crlf.tokens[1].span.start.offset, 5);

    // Provisional: a lone CR is one line ending until LLD-05 freezes the rule.
    let cr = lex("c.grf", "a\rb");
    assert_eq!(cr.tokens[1].span.start.line, 2);
    assert_eq!(cr.tokens[1].span.start.column, 1);

    let tab = lex("c.grf", "\tint");
    assert_eq!(tab.tokens[0].kind, TokenKind::KwInt);
    assert_eq!(tab.tokens[0].span.start.offset, 1);
    assert_eq!(tab.tokens[0].span.start.column, 2);

    let unicode = lex("c.grf", "\"é\"");
    assert_eq!(unicode.tokens[0].decoded.as_deref(), Some("é"));
    assert_eq!(unicode.tokens[0].span.end.offset, 4);
    assert_eq!(unicode.tokens[0].span.end.column, 4);

    let unexpected = lex("c.grf", "é");
    assert_eq!(
        unexpected.diagnostics[0].kind,
        DiagnosticKind::UnexpectedCharacter
    );

    let nbsp = lex("c.grf", "\u{00A0}");
    assert_eq!(
        nbsp.diagnostics[0].kind,
        DiagnosticKind::UnexpectedCharacter
    );

    let blank = lex("c.grf", " \n");
    assert_eq!(blank.tokens.len(), 1);
    assert_eq!(blank.tokens[0].span.start.line, 2);
    assert_eq!(blank.tokens[0].span.start.column, 1);
    assert_eq!(blank.tokens[0].span.start.offset, 2);
}

#[test]
fn identifiers_may_contain_digits_and_underscores() {
    assert_eq!(
        kinds("value2 _temporary"),
        vec![TokenKind::Identifier, TokenKind::Identifier, TokenKind::Eof]
    );
}

#[test]
fn sample_programs_lex_without_diagnostics() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in ["test.grf", "examples/hello/main.grf"] {
        let path = root.join(relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let result = lex(relative, &source);
        assert!(
            result.diagnostics.is_empty(),
            "{relative}: {:?}",
            result.diagnostics
        );
        assert_eq!(
            result.tokens.last().map(|token| token.kind),
            Some(TokenKind::Eof)
        );
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::KwImport)
        );
        assert!(result.tokens.iter().any(|token| token.lexeme == "io"));
    }
}

#[test]
fn test_grf_token_kinds() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test.grf");
    let source = std::fs::read_to_string(&path).expect("test.grf");
    let result = lex("test.grf", &source);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .tokens
            .iter()
            .map(|token| token.kind)
            .collect::<Vec<_>>(),
        vec![
            TokenKind::KwImport,
            TokenKind::Identifier,
            TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::Semicolon,
            TokenKind::KwVoid,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::Identifier,
            TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::IntegerLiteral,
            TokenKind::RightParen,
            TokenKind::RightParen,
            TokenKind::Semicolon,
            TokenKind::Identifier,
            TokenKind::Dot,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::IntegerLiteral,
            TokenKind::RightParen,
            TokenKind::RightParen,
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::KwInt,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::KwInt,
            TokenKind::Identifier,
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::KwReturn,
            TokenKind::Identifier,
            TokenKind::Star,
            TokenKind::Identifier,
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::KwStr,
            TokenKind::Identifier,
            TokenKind::LeftParen,
            TokenKind::KwAuto,
            TokenKind::KwStr,
            TokenKind::Identifier,
            TokenKind::RightParen,
            TokenKind::LeftBrace,
            TokenKind::KwReturn,
            TokenKind::InterpolatedStringStart,
            TokenKind::StringText,
            TokenKind::InterpolationStart,
            TokenKind::Identifier,
            TokenKind::InterpolationEnd,
            TokenKind::StringEnd,
            TokenKind::Semicolon,
            TokenKind::RightBrace,
            TokenKind::Eof,
        ]
    );
}
