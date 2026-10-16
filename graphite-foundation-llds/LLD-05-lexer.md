# LLD 05 — Graphite Lexer

**Status:** Detailed implementation draft  
**Target:** Graphite v0.1  
**Stage:** 1 — Lexer  
**Audience:** Compiler implementer and AI coding agent

## 1. Purpose

The lexer converts a Graphite source file into a sequence of tokens. It recognizes identifiers, keywords, numeric and string literals, operators, punctuation, comments, and invalid lexical sequences.

The lexer MUST NOT parse program structure, resolve names, validate types, resolve library APIs, or generate Rust. Those responsibilities belong to later phases.

The lexer must preserve source locations so parser and semantic diagnostics can point to the original `.grf` source.

## 2. Inputs and outputs

### 2.1 Input

The lexer receives:
- the source text of one `.grf` file;
- a source-file identifier/path used in diagnostics.

It should operate on an in-memory source buffer for the first implementation. File loading and path canonicalization belong to the CLI/source manager, not the lexical scanner itself.

### 2.2 Output

The lexer returns:
- an ordered token stream;
- source spans for tokens;
- lexical diagnostics.

Recommended result shape:

```rust
struct LexResult {
    tokens: Vec<Token>,
    diagnostics: Vec<LexDiagnostic>,
}
```

The actual Rust type names may differ, but the behavior must remain equivalent.

An explicit EOF token should terminate the token stream. If fatal lexical errors occur, the implementation may still return tokens and diagnostics for error recovery, but the parser must not mistake an invalid stream for a fully valid source file.

## 3. Responsibilities and non-responsibilities

### The lexer MUST

- scan the source in order;
- recognize token categories defined by this LLD;
- skip insignificant whitespace;
- recognize and skip comments;
- distinguish identifiers from reserved keywords;
- recognize numeric literals without deciding all semantic type rules;
- recognize string literals and interpolation-related lexical boundaries;
- apply longest-match behavior to overlapping operators;
- attach source spans;
- report malformed or unterminated lexical constructs;
- terminate deterministically at EOF.

### The lexer MUST NOT

- construct the Graphite AST;
- determine whether a variable exists;
- resolve `grf.io` or any domain/library;
- decide whether a function call is valid;
- infer or check expression types;
- generate Rust code;
- silently repair malformed tokens;
- treat library-specific names as keywords unless they are reserved language keywords.

## 4. Source positions and spans

Every token must carry a span identifying its location in the original source.

Recommended representation:

```rust
struct SourcePosition {
    offset: usize, // byte offset into UTF-8 source
    line: u32,     // 1-based
    column: u32,   // 1-based display/code-point column, policy to be frozen
}

struct Span {
    start: SourcePosition,
    end: SourcePosition, // exclusive
}
```

Recommended rules:
- Byte offsets are zero-based UTF-8 byte offsets.
- Line and column numbers are one-based.
- End positions are exclusive.
- A token span covers the token's source characters, not leading whitespace.
- EOF has a zero-length span at the end of source.
- CRLF (`\r\n`) is treated as one line ending; LF (`\n`) is a line ending. Whether lone CR is accepted as a line ending should be explicitly tested and frozen.
- The column counting policy must be consistent. For the initial ASCII-oriented identifier policy, counting Unicode scalar values for columns is acceptable; byte offsets remain authoritative for slicing.

Source spans are required for diagnostics and should survive through the AST.

## 5. Token data model

Recommended token structure:

```rust
struct Token {
    kind: TokenKind,
    lexeme: String,
    span: Span,
}
```

`lexeme` preserves the exact source spelling for diagnostics and debugging. Literal tokens may additionally store parsed literal payloads in a separate field or in token-kind variants, but parsing numeric values must not discard the original spelling.

Recommended enum shape:

```rust
enum TokenKind {
    // Structural
    Eof,

    // Names and literals
    Identifier,
    IntegerLiteral,
    FloatLiteral,
    StringLiteral,
    InterpolatedStringStart,
    StringText,
    InterpolationStart,
    InterpolationEnd,

    // Keywords
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
    KwTrue,
    KwFalse,

    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Dot,
    Semicolon,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
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
```

This enum is the v0.1 inventory. Keep keyword variants aligned with the frozen syntax LLD. `class`, `private`, `this`, `public`, `new`, and `constructor` are ordinary identifiers. `+=`, `-=`, `*=`, `/=`, `%=`, `->`, `:`, and `?` have no token kinds. Do not allow the lexer inventory to accidentally define the language.

`grf.json` is a JSON manifest, not Graphite source. It MUST be loaded by a JSON parser and manifest validator, not by the `.grf` lexer. The Graphite lexer must tokenize import paths such as `import grf.io;` and call paths such as `io.println(...)` without resolving their meaning.

## 6. Character classes and identifiers

### 6.1 Whitespace

The lexer skips spaces, horizontal tabs, and recognized line endings outside literals and comments. It preserves line/column positions while skipping them.

Other Unicode whitespace should either be explicitly accepted or reported as an unexpected character. The initial implementation should avoid platform-dependent whitespace behavior.

### 6.2 Identifier proposal

For the first lexer milestone, use ASCII identifiers:

```text
identifier-start := A-Z | a-z | _
identifier-part  := identifier-start | 0-9
```

Equivalent regular expressions:

```regex
[A-Za-z_][A-Za-z0-9_]*
```

Examples:
- valid lexical identifiers: `age`, `userName`, `fileIo`, `_temporary`, `value2`;
- lexically valid but style-invalid names may include `UserName` in a variable position; casing is a naming-policy check, not necessarily a lexical error;
- `2value` begins with a number and is not a single identifier token.

Whether identifiers may start with `_`, and whether leading underscores violate naming policy, must be frozen in the naming specification. The lexer should recognize the token first; naming-policy validation may report the style violation later.

The lexer is case-sensitive. `if` is a keyword; `If` is an identifier.

## 7. Keywords

The lexer recognizes reserved keywords by matching the complete identifier lexeme against a fixed keyword table.

v0.1 keywords:
- declarations/types: `auto`, `const`, `void`, `int`, `uint`, `flt`, `int16`, `int32`, `int64`, `uint16`, `uint32`, `uint64`, `flt32`, `flt64`, `bln`, `chr`, `str`;
- control flow: `if`, `else`, `loop`, `break`, `continue`, `return`;
- imports: `import`;
- boolean literals: `true`, `false`.

`auto` is always `KwAuto`, never `Identifier`. Bare `auto` is still that keyword; rejecting a missing target type is a later phase.

`class`, `private`, `this`, `public`, `new`, `constructor`, `library`, `extends`, `super`, `override`, `try`, `catch`, and `finally` are identifiers in v0.1. They become keywords only when a later specification adds them, together with grammar, semantic rules, and tests.

Library names such as `grf`, `io`, `fileIo`, `coolDev`, and `netHttp` are identifiers, not keywords.

Implementation recommendation: scan an identifier first, then consult a deterministic keyword table. Do not use prefix matching (`ifElse` must be one identifier, not `if` plus `Else`).

## 8. Numeric literals

### 8.1 Integer literals

Proposed initial decimal syntax:

```text
digit       := 0-9
integer     := digit+
```

Examples: `0`, `7`, `42`, `100000`.

Leading-zero policy is an open decision. The simplest initial rule is to accept decimal digit sequences including leading zeros and interpret them as decimal, not octal. Hexadecimal, binary, separators, and suffixes are not part of the initial lexer unless explicitly specified.

### 8.2 Floating-point literals

Proposed decimal forms include:

```text
digits "." digits
digits exponent
digits "." digits exponent
```

where:

```text
exponent := ("e" | "E") ("+" | "-")? digits
```

Examples:
- `3.14`
- `0.5`
- `10.`
- `1e6`
- `1.25e-3`

**Open decision:** whether `10.` is accepted as a float or tokenized as integer `10` followed by dot. Choose one policy and test it. A conservative initial grammar may require digits after the decimal point (`10.0`) to reduce ambiguity with member/module access.

A leading sign is a separate unary operator, not part of the numeric literal: `-42` tokenizes as `Minus`, `IntegerLiteral("42")`.

### 8.3 Lexical versus semantic numeric errors

The lexer identifies a numeric token's spelling. Whether its value fits `int32`, `int64`, or a selected type is decided by parsing/semantic analysis or literal validation, depending on the chosen compiler architecture.

Malformed exponents such as `1e`, `1e+`, or `1.2e-` must produce a lexical diagnostic rather than being silently split into plausible tokens, unless the chosen recovery policy explicitly documents another behavior.

The implementation must avoid panics when parsing very large numeric lexemes.

## 9. String literals

### 9.1 Plain strings

Proposed syntax uses double quotes:

```grf
"Hello"
"Line one\nLine two"
```

The lexer recognizes a string as a token or as a sequence of string-mode tokens, depending on how interpolation is implemented.

Proposed escape sequences:
- `\"` — quote
- `\\` — backslash
- `\n` — newline
- `\r` — carriage return
- `\t` — tab
- `\0` — NUL, if accepted

The escape list must be frozen. Unknown escapes should be diagnosed or preserved according to one explicit policy; do not silently interpret them inconsistently.

A raw line ending inside a regular quoted string is an error unless multiline strings are deliberately added. An unterminated string at EOF is a lexical error.

### 9.2 Interpolated strings

Proposed source form:

```grf
f"Hello, {userName}!"
```

A robust lexer must not treat interpolation as a blind string replacement. It must expose token boundaries so the parser can parse the expression inside `{ ... }`.

One recommended token sequence for:

```grf
f"Hello, {userName}!"
```

is conceptually:

```text
InterpolatedStringStart  `f"`
StringText               `Hello, `
InterpolationStart       `{`
Identifier                `userName`
InterpolationEnd          `}`
StringText               `!`
StringEnd                 `"`
```

The exact token names are implementation details, but the token stream must preserve enough structure to parse the embedded expression.

Lexer mode requirements:
1. Enter interpolated-string mode after recognizing `f"` as a prefix immediately followed by a quote.
2. Emit string-text segments until an unescaped `{` or the closing quote.
3. On `{`, enter normal expression tokenization for the interpolation.
4. Track nested brace depth inside the interpolation so braces belonging to nested blocks/expressions do not close the interpolation prematurely.
5. Return to string mode when the interpolation's matching `}` is reached.
6. Continue until the terminating quote.
7. Report an unterminated interpolated string or interpolation with a useful span.

The exact escape rule for literal `{` and `}` inside interpolated strings is an open decision and must be frozen. A possible design is `{{` and `}}` for literal braces, but do not implement this without approval.

If the language later supports arbitrary expressions in interpolation, the lexer must allow normal expression tokens inside the interpolation rather than only identifiers.

## 10. Comments

### 10.1 Line comments

When the scanner encounters `//` outside a string:
- consume characters until but not including the line ending;
- emit no comment token in the ordinary token stream;
- preserve source position accounting.

### 10.2 Block comments

When the scanner encounters `/*` outside a string:
- consume through the first following `*/`;
- emit no comment token in the ordinary stream;
- update line/column for all consumed characters;
- if EOF occurs first, emit an unterminated-block-comment diagnostic.

Nested block comments are not supported in the proposed v0.1 rules. For `/* a /* b */`, the first `*/` closes the comment; the remaining `*/` is then lexed as ordinary tokens and will likely produce a later parser error. This rule must be covered by tests.

## 11. Operators and longest match

When multiple tokens share prefixes, use maximal munch / longest match.

Examples:
- `==` is `EqualEqual`, not two `Equal` tokens.
- `!=` is `BangEqual`, not `Bang` plus `Equal`.
- `<=` is `LessEqual`, not `Less` plus `Equal`.
- `>=` is `GreaterEqual`, not `Greater` plus `Equal`.
- `&&` is `AndAnd`; a lone `&` is an unexpected character unless a single-ampersand operator is added.
- `||` is `OrOr`; a lone `|` is an unexpected character unless explicitly supported.
- `++` is `PlusPlus`. `--` is `MinusMinus`. Both are v0.1 operators. The lexer does not distinguish prefix from postfix.
- `->` is not an operator. It tokenizes as `Minus` then `Greater`. The parser rejects that sequence as an arrow.
- `+=`, `-=`, `*=`, `/=`, and `%=` are not operators. Each tokenizes as the arithmetic operator followed by `Equal`. The parser rejects that sequence as a compound assignment.
- `:` and `?` are unexpected characters in v0.1.

The scanner should test multi-character operators before their one-character prefixes. `++` must win over `+`. `==` must win over `=`. There is no `+=` token that could win over `+`.

## 12. Punctuation

Proposed punctuation inventory:
- `(` `)`
- `{` `}`
- `[` `]`
- `,`
- `.`
- `;`

`:` and `?` are unexpected characters, not punctuation tokens. `[` and `]` remain punctuation so the lexer can report a single token; array syntax is still outside v0.1 and the parser must reject it.

The lexer recognizes punctuation; the parser determines whether it is valid in the current context.

A dot is required for import paths such as `grf.io` and member/call paths such as `io.println`. The lexer emits one `Dot` token for each dot and does not resolve domains, libraries, or function names.

## 13. Tokenization examples

### Example A — declaration

Source:

```grf
int userAge = 21;
```

Expected token kinds:

```text
KwInt
Identifier("userAge")
Equal
IntegerLiteral("21")
Semicolon
Eof
```

### Example B — import and call

Source:

```grf
import grf.io;
io.println("Hello");
```

Expected token kinds. `StringLiteral("Hello")` is the token kind plus the decoded text; the lexeme keeps the source spelling, including the quotes.

```text
KwImport
Identifier("grf")
Dot
Identifier("io")
Semicolon
Identifier("io")
Dot
Identifier("println")
LeftParen
StringLiteral("Hello")
RightParen
Semicolon
Eof
```

The import and the call are separate statements. The call begins with `Identifier("io")`, not with another `grf` segment. `grf.io.println("Hello")` is not this program's token stream and is not the call syntax for this import.

The lexer does not know that `grf.io` is a built-in library. All import path segments and call path segments are ordinary identifiers; semantic/library resolution happens after parsing. `import` is `KwImport`. `io`, `grf`, and `println` are identifiers.

### Example C — condition

Source:

```grf
if (age >= 18) {
    break;
}
```

Expected token kinds:

```text
KwIf
LeftParen
Identifier("age")
GreaterEqual
IntegerLiteral("18")
RightParen
LeftBrace
KwBreak
Semicolon
RightBrace
Eof
```

Whether `break` is semantically valid depends on parser/semantic context, not the lexer.

### Example D — comment and string marker

Source:

```grf
str text = "not // a comment"; // comment
```

The `//` inside the string remains string content. The second `//` begins a comment.

## 14. Lexical diagnostics

At minimum, define diagnostic codes or stable categories for:
- unexpected character;
- unterminated string;
- invalid escape sequence, if escapes are validated lexically;
- unterminated block comment;
- malformed numeric literal;
- unterminated interpolation;
- malformed interpolation delimiter.

A diagnostic should include:
- source identifier/path;
- start and end span;
- a concise message;
- optional hint where a likely correction is safe to suggest.

Diagnostics must not panic or terminate the compiler process unexpectedly. The compiler may stop after lexical errors for the first milestone, but that behavior must be explicit. Error recovery can be improved later.

## 15. Lexer algorithm

Recommended single-pass scanning algorithm:

1. Initialize byte offset at zero, line at one, column at one.
2. While the current offset is before EOF:
   1. Skip recognized whitespace while updating position.
   2. If the next characters start a line comment, consume it.
   3. If they start a block comment, consume it or report an unterminated comment.
   4. If the next character starts an identifier, scan the full identifier and classify it through the keyword table.
   5. If it starts a numeric literal, scan according to the numeric grammar and diagnose malformed forms.
   6. If it starts a plain string, scan escapes and termination.
   7. If it starts an interpolated string, enter the interpolation-aware string scanner.
   8. Otherwise, match the longest recognized operator or punctuation token.
   9. If nothing matches, report an unexpected-character diagnostic and advance safely by one Unicode scalar value.
3. Emit EOF with a zero-length span at the end of source.
4. Return tokens and diagnostics.

The scanner must always make progress after an error. It must never loop indefinitely on an invalid character or malformed literal.

## 16. Complexity and performance

The lexer should be a single pass over the source and should aim for **O(n)** time in the source length, excluding allocation costs and diagnostic formatting. It should avoid repeatedly rescanning the entire prefix of the source to compute line/column.

For the initial implementation, straightforward code and correct spans are more important than micro-optimization. Avoid unnecessary per-character heap allocations where practical; preserve lexemes using spans or slices internally if that makes ownership manageable.

## 17. Tests

### 17.1 Positive tests

- Empty source emits EOF.
- Simple declaration tokenizes correctly.
- All declared primitive keywords tokenize correctly.
- Identifiers with digits after the first character work.
- All supported one-character operators and punctuation tokenize correctly.
- All supported multi-character operators use longest match.
- Whitespace and line endings update spans correctly.
- Line comments and block comments are skipped.
- Comment markers inside strings remain string content.
- Escaped quotes and backslashes work under the selected escape policy.
- Numeric integer, decimal, and exponent forms accepted by the final grammar tokenize correctly.
- `import grf.io;` followed by `io.println("Hello");` produces the Example B stream, including `KwImport`, the semicolon, and a second `Identifier("io")` on the call.
- `auto` is `KwAuto`. `automation` is one identifier. `int auto = 1;` yields `KwInt`, `KwAuto`, `Equal`, not an identifier named `auto`.
- `++` is `PlusPlus` and `--` is `MinusMinus`, including in `index++` and `++index`.
- `class`, `private`, `this`, `public`, `new`, and `constructor` are `Identifier` tokens.
- Interpolated strings produce the expected structured token sequence.
- EOF span is correct.

### 17.2 Negative tests

- Unexpected character.
- Unterminated string.
- Unterminated block comment.
- Malformed exponent.
- Invalid escape, if the chosen policy rejects it.
- Unterminated interpolation.
- Invalid interpolation delimiter.
- Unsupported single `&` or `|`.
- `:` and `?` are unexpected characters.
- `+=` tokenizes as `Plus` then `Equal`, not as a compound-assignment token. The same split applies to `-=`, `*=`, `/=`, and `%=`.
- `->` tokenizes as `Minus` then `Greater`, not as an arrow token.
- Correct handling of CRLF and EOF immediately after a comment.
- Very long numeric literal does not panic.
- Invalid UTF-8 is impossible once input is a Rust `String`; file decoding errors must be reported by the source-loading layer.

### 17.3 Span tests

For each token kind used in representative examples, verify:
- start byte offset;
- end byte offset;
- start line and column;
- end line and column;
- correct behavior after tabs and CRLF under the selected column policy.

## 18. Acceptance criteria

The lexer stage is complete when:
1. All supported token kinds are enumerated and aligned with the syntax LLD.
2. Tokenization is deterministic and preserves lexemes/spans.
3. Comments and whitespace are handled according to the documented rules.
4. Strings and interpolation are tokenized according to the rules frozen in this LLD. Open items in section 20 are not filled in by the implementation.
5. Longest-match operator recognition works.
6. Lexical errors are structured and source-located.
7. The lexer never hangs or panics on malformed source in the test suite.
8. Unit tests cover positive cases, negative cases, and source spans.
9. The parser can consume the token stream without needing to reinterpret raw source text.
10. Unsupported syntax is not accidentally introduced by extra token kinds.

## 19. Frozen lexical decisions

- The v0.1 keyword inventory is the list in section 7. `auto` is reserved. `class`, `private`, `this`, `public`, `new`, and `constructor` are not.
- `++` and `--` are tokens. Prefix versus postfix is a parser decision.
- `->`, `:`, `?`, and compound assignments are not tokens.
- The Example B import/call stream in section 13 is normative.
- JSON manifest validation and schema policy are separate from the `.grf` lexer.

## 20. Decisions that remain open

- ASCII-only versus Unicode identifiers. v0.1 currently specifies ASCII identifiers.
- Whether `_` may begin identifiers and the casing policy for underscore names.
- Float grammar, including whether `10.` is valid.
- Numeric separators, bases, suffixes, and leading-zero policy.
- String escape set and invalid-escape behavior.
- Literal brace escaping inside interpolated strings.
- Nested block-comment policy (proposed: not nested). The interpolation scanner still tracks nested braces inside an interpolation expression, as specified in section 9.2.
- CR-only newline handling and column counting.

Do not silently make these decisions during implementation. Update this LLD and its tests when a decision is approved. Semantic acceptance cases that depend on later phases are listed in `v0.1-acceptance-tests.md`.
