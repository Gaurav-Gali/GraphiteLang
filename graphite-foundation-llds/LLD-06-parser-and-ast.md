# LLD-06 — Graphite Parser and Abstract Syntax Tree (AST)

**Project:** Graphite Programming Language  
**Stage:** 2 — Parser and AST  
**Target version:** Graphite v0.1  
**Implementation language:** Rust  
**Source extension:** `.grf`  
**Status:** Implementation specification  
**Depends on:** LLD-01 through LLD-05, especially the general syntax and lexer specifications

---

## 1. Objective

Implement a recursive-descent parser that consumes the existing Graphite lexer token stream and produces a structured Graphite Abstract Syntax Tree (AST).

The parser is responsible for determining whether a token sequence conforms to Graphite v0.1 grammar. It must preserve source locations and report malformed syntax through structured, source-located diagnostics.

The parser must not perform semantic analysis, resolve library manifests, validate types, convert values, or generate Rust.

### 1.1 Required outcome

Given valid Graphite source:

```grf
import grf.io;

void main() {
    io.println(square(5));
    io.println(greet(123));
}

int square(int value) {
    return value * value;
}

str greet(auto str msg) {
    return f"Hello {msg}";
}
```

The parser must produce a structured AST containing:

- One import declaration and three function declarations. The phrase "two import-independent function declarations" was a documentation error.
- A `main` function with a block containing two expression statements.
- Nested function calls and a member-style library call.
- A multiplication expression and a return statement.
- A parameter declared with the `auto str` conversion modifier.
- An interpolated string with a text segment and an expression segment.

The program above must parse even though `square` and `greet` are declared after `main`. Declaration-order validation is not the parser's responsibility.

For malformed source, the parser must return diagnostics with source spans instead of panicking.

---

## 2. Scope

### 2.1 Included

The parser must support the following constructs, provided their tokens are available in the current lexer:

1. Top-level import declarations.
2. Top-level function declarations.
3. Primitive type annotations.
4. `auto T` parameter and variable-declaration syntax.
5. Function parameters and optional return types only where defined by the LLD.
6. Blocks and local variable declarations.
7. Return statements.
8. Expression statements.
9. Identifiers and literals.
10. Function calls and dotted call paths.
11. Unary and binary expressions.
12. Assignment expressions or assignment statements.
13. Prefix and postfix increment/decrement.
14. Conditional statements.
15. Loop statements.
16. `break` and `continue`.
17. String interpolation.
18. Parenthesized expressions.
19. Source-located syntax diagnostics and parser error recovery.

The implementation must support the grammar productions actually approved for v0.1. It must not assume that every lexer token automatically constitutes supported parser syntax.

### 2.2 Explicitly excluded

The following are not part of this stage:

- Type checking and type inference.
- Validation of `auto T` conversion compatibility.
- Function overload resolution.
- Library and domain resolution through `grf.json`.
- Validation that `io.println` exists.
- Rust code generation.
- Cargo project generation.
- Classes, constructors, inheritance, and access modifiers.
- Array and collection declarations unless separately approved in the v0.1 grammar.
- Generic functions or types.
- Lambda expressions and closures.
- Exception-handling constructs.
- Import aliases.
- Multi-file source modules.
- Macro expansion.
- Runtime execution.

The parser may represent unresolved names and calls as AST nodes. It must not reject them merely because their semantic validity is not yet known.

---

## 3. Architectural requirements

The parser must be a distinct component of the compiler.

```text
Source file (.grf)
       |
       v
Existing lexer
       |
       v
Token stream
       |
       v
Parser
       |
       +---- AST
       |
       +---- Parser diagnostics
```

The parser must not depend on the Rust code generator or the semantic analyzer.

### 3.1 Input contract

The parser receives:

- A complete token sequence from the lexer.
- An explicit EOF token at the end.
- Source spans attached to tokens.
- Lexer diagnostics, if the existing compiler API passes them through the same compilation context.

The parser must not re-lex source text.

If lexical errors prevent reliable parsing, the caller may choose not to invoke the parser. If parsing proceeds despite lexical errors, the parser must handle error tokens or malformed token sequences without panicking.

### 3.2 Output contract

The parser returns a result containing:

- The parsed program AST, if recovery produced a usable tree.
- Zero or more parser diagnostics.
- An indication of whether parsing completed successfully.

A recommended API is:

```rust
pub struct ParseOutput {
    pub program: Option<Program>,
    pub diagnostics: Vec<ParserDiagnostic>,
}

pub fn parse(tokens: &[Token]) -> ParseOutput;
```

This API is illustrative. Align names and ownership with the existing Rust project.

A completely valid program must produce a program AST and zero parser diagnostics.

If errors occur, the parser may return a partial AST for diagnostics and tooling, but the CLI must not treat a program with parser errors as compilable.

---

## 4. Source locations and spans

Every AST node should preserve a source span, or enough source-location information to reconstruct one.

Use the existing lexer's span representation wherever possible.

A span should contain:

- Start byte offset.
- End byte offset.
- Start line and column.
- End line and column.

Use half-open spans: the start is inclusive and the end is exclusive.

For example, the token `KwImport` in:

```grf
import grf.io;
```

starts at line 1, column 1 and ends at line 1, column 7.

AST spans should normally cover the complete syntactic construct:

- An import spans from `import` through its semicolon.
- A function declaration spans from its return type through its closing brace.
- A block spans from `{` through `}`.
- A call expression spans from the first identifier through the closing parenthesis.
- An interpolated string spans from its opening string delimiter through its closing delimiter.

If a closing token is missing, diagnostics should point to the unexpected token or EOF and explain what was expected.

Do not duplicate or redefine the lexer's source-location system unless the existing implementation cannot satisfy these requirements.

---

## 5. Token contract

Use the token types already implemented in the lexer.

The parser should consume token kinds rather than matching their debug-formatted names or textual output.

The existing lexer output demonstrates token kinds including:

- `KwImport`
- `KwVoid`
- `KwInt`
- `KwStr`
- `KwAuto`
- `KwReturn`
- `Identifier`
- `IntegerLiteral`
- `Dot`
- `Semicolon`
- `LeftParen`
- `RightParen`
- `LeftBrace`
- `RightBrace`
- `Star`
- `InterpolatedStringStart`
- `StringText`
- `InterpolationStart`
- `InterpolationEnd`
- `StringEnd`
- `Eof`

The full set must be taken from the current lexer implementation and LLD-05.

Do not create new token kinds merely to simplify parsing if the lexer already represents the relevant syntax correctly.

If a grammar production requires a token that the lexer does not yet produce, report the dependency and update the lexer and its tests deliberately.

---

## 6. Grammar notation

The grammar below uses EBNF-style notation.

- `A ::= B` defines a production.
- `|` means alternative.
- `{ X }` means zero or more repetitions.
- `[ X ]` means an optional element.
- Terminals appear in quotes or as token names.
- Comments in grammar examples are explanatory, not terminals.

This grammar is the parser's initial normative target. Keep it synchronized with the frozen syntax LLD. Do not silently expand it to accept syntax from unrelated languages.

---

## 7. Program and declaration grammar

### 7.1 Program

```ebnf
Program ::= { ImportDeclaration } { FunctionDeclaration } EOF ;
```

An entry program may contain zero or more imports followed by zero or more top-level function declarations.

The parser must reject imports that occur after a function declaration has begun.

The parser does not determine whether the program contains exactly one valid `main` function. Entry-point validation belongs to semantic analysis.

### 7.2 Imports

```ebnf
ImportDeclaration ::= "import" Identifier "." Identifier ";" ;
```

Example:

```grf
import grf.io;
import coolDev.netHttp;
```

Rules:

- An import contains exactly two identifier segments.
- The first segment names the domain.
- The second segment names the library.
- Import paths are case-sensitive.
- Import aliases are unsupported.
- Import resolution is not performed by the parser.

The following is syntactically invalid:

```grf
import grf.io.print;
```

The parser must report an import-path error because v0.1 permits exactly two segments.

The following is also invalid because imports must precede declarations:

```grf
void main() {
}

import grf.io;
```

### 7.3 Function declarations

```ebnf
FunctionDeclaration ::=
    Type Identifier "(" [ ParameterList ] ")" Block ;

ParameterList ::= Parameter { "," Parameter } ;

Parameter ::= [ "auto" ] Type Identifier ;
```

Examples:

```grf
void main() {
}

int square(int value) {
    return value * value;
}

str greet(auto str msg) {
    return f"Hello {msg}";
}

int add(int first, int second) {
    return first + second;
}
```

Rules:

- Function declarations are top-level only in v0.1.
- The return type precedes the function name.
- Parentheses are mandatory, including for zero-parameter functions.
- Parameters are separated by commas.
- Each parameter contains a type and an identifier.
- `auto` may precede the parameter's target type.
- A trailing comma is not supported in v0.1 unless explicitly approved.
- Function bodies are blocks.
- Forward function calls are allowed syntactically.
- The parser does not validate duplicate function names, duplicate parameter names, or type compatibility.

Do not add visibility modifiers or overload-specific syntax to this grammar.

`const` is not a v0.1 declaration. The parser must reject `KwConst` with a diagnostic that `const` is not supported. It must not treat `const` as an identifier or ignore it. Add `const` only after its semantics are specified.

### 7.4 Type syntax

The type grammar must use the actual primitive type tokens supported by the lexer and base-version LLD.

Conceptually:

```ebnf
Type ::=
      "void"
    | "int"
    | "uint"
    | "flt"
    | "int16"
    | "int32"
    | "int64"
    | "uint16"
    | "uint32"
    | "uint64"
    | "flt32"
    | "flt64"
    | "bln"
    | "chr"
    | "str" ;
```

This list must be synchronized with the canonical v0.1 type inventory.

The parser recognizes type syntax; semantic analysis later normalizes aliases such as `int` and `int64`.

Do not add unsupported types such as `string`, `any`, or bare `auto`.

**Return-type restriction:** `void` is permitted as a function return type. Using `void` as a variable type or a parameter type is invalid, and semantic analysis rejects those uses. The parser still accepts `void` wherever this type grammar allows it, and preserves the type node so the semantic diagnostic can name the declaration.

---

## 8. Statement grammar

### 8.1 Blocks

```ebnf
Block ::= "{" { Statement } "}" ;
```

A block may contain zero or more statements.

Nested blocks are permitted wherever a statement is permitted.

An unterminated block must produce a diagnostic indicating that `}` was expected.

### 8.2 Local variable declarations

```ebnf
VariableDeclaration ::=
    [ "auto" ] Type Identifier "=" Expression ";" ;
```

Examples:

```grf
int age = 21;
str name = "Alex";
auto int convertedAge = 21;
flt learningRate = 0.001;
```

The parser builds a declaration node containing:

- Whether `auto` was present.
- The declared type.
- The variable name.
- The required initializer.
- The declaration's source span.

The parser does not convert initializer values or verify assignment compatibility.

A declaration without `=` starts at the type's default value: numeric `0` or `0.0`, `false`, `""`, an empty array or map, or a record of those defaults. `const` still requires a value. A class variable has no default and is constructed with `new`. The same rule applies to a C-style loop's variable declaration.

### 8.3 Return statements

```ebnf
ReturnStatement ::= "return" [ Expression ] ";" ;
```

Examples:

```grf
return;
return value;
return value * value;
return f"Hello {name}";
```

The parser must not decide whether a return value is legal for the enclosing function's return type.

### 8.4 Expression statements

```ebnf
ExpressionStatement ::= Expression ";" ;
```

Examples:

```grf
io.println("Hello");
square(5);
age++;
```

Whether a particular expression is meaningful as a standalone statement is a semantic question unless the grammar explicitly excludes it.

### 8.5 Conditional statements

```ebnf
IfStatement ::=
    "if" "(" Expression ")" Block
    [ "else" ( IfStatement | Block ) ] ;
```

Examples:

```grf
if (age >= 18) {
    io.println("Adult");
} else {
    io.println("Minor");
}
```

```grf
if (score >= 90) {
    io.println("A");
} else if (score >= 80) {
    io.println("B");
} else {
    io.println("Other");
}
```

The parser must associate each `else` with the nearest unmatched `if`.

Conditions are parsed as expressions. Their types are validated later.

### 8.6 Loop statements

Support the loop forms specified by the existing syntax LLD.

```ebnf
LoopStatement ::=
      "loop" "(" ")" Block
    | "loop" "(" Expression ")" Block
    | "loop" "(" [ VariableDeclarationWithoutSemicolon ]
      ";" [ Expression ] ";" [ Expression ] ")" Block ;

VariableDeclarationWithoutSemicolon ::=
    [ "auto" ] Type Identifier "=" Expression ;
```

Examples:

```grf
loop (index < 10) {
    io.println(index);
}
```

```grf
loop (int index = 0; index < 10; index++) {
    io.println(index);
}
```

```grf
loop () {
    break;
}
```

The parser must handle the semicolons inside the C-style loop header without confusing them with statement terminators outside the loop.

`loop ()` is the only infinite loop. It has its own AST variant. `loop (;;)` is a syntax error and must not be stored as that infinite-loop variant.

A C-style initializer is a variable declaration with `=`, or it is absent, as in `loop (; index < 10; index++)`. An assignment or other expression in the initializer position is a syntax error.

Do not add `while`, `for`, or other aliases merely because they are common elsewhere.

### 8.7 Break and continue

```ebnf
BreakStatement ::= "break" ";" ;
ContinueStatement ::= "continue" ";" ;
```

The parser recognizes these statements syntactically.

Whether they occur inside a valid loop is a semantic check.

### 8.8 Statement production

The initial statement grammar is:

```ebnf
Statement ::=
      Block
    | VariableDeclaration
    | ReturnStatement
    | IfStatement
    | LoopStatement
    | BreakStatement
    | ContinueStatement
    | ExpressionStatement ;
```

Do not add class declarations, nested function declarations, exception handlers, or other unsupported statements.

---

## 9. Expression grammar and precedence

Expressions must be represented structurally rather than stored as source-text fragments.

Use a precedence-climbing or Pratt parser, or a conventional recursive-descent expression parser with equivalent behavior. Choose the approach that best fits the existing codebase.

The following grammar defines the initial expression structure:

```ebnf
Expression ::= AssignmentExpression ;

AssignmentExpression ::=
    LogicalOrExpression
    [ AssignmentOperator AssignmentExpression ] ;

AssignmentOperator ::= "=" ;

LogicalOrExpression ::=
    LogicalAndExpression { "||" LogicalAndExpression } ;

LogicalAndExpression ::=
    EqualityExpression { "&&" EqualityExpression } ;

EqualityExpression ::=
    ComparisonExpression { ( "==" | "!=" ) ComparisonExpression } ;

ComparisonExpression ::=
    AdditiveExpression
    { ( "<" | "<=" | ">" | ">=" ) AdditiveExpression } ;

AdditiveExpression ::=
    MultiplicativeExpression { ( "+" | "-" ) MultiplicativeExpression } ;

MultiplicativeExpression ::=
    UnaryExpression { ( "*" | "/" | "%" ) UnaryExpression } ;

UnaryExpression ::=
      ( "!" | "-" ) UnaryExpression
    | PrefixUpdateExpression
    | PostfixExpression ;

PrefixUpdateExpression ::= ( "++" | "--" ) PostfixExpression ;

PostfixExpression ::=
    PrimaryExpression
    { CallSuffix | MemberSuffix | PostfixUpdateSuffix } ;

CallSuffix ::= "(" [ ArgumentList ] ")" ;

ArgumentList ::= Expression { "," Expression } ;

MemberSuffix ::= "." Identifier ;

PostfixUpdateSuffix ::= "++" | "--" ;

PrimaryExpression ::=
      Identifier
    | Literal
    | InterpolatedString
    | "(" Expression ")" ;
```

This grammar is illustrative of the parser structure and must be reconciled with the actual v0.1 operator inventory before implementation. In particular, only operators explicitly supported by the lexer and syntax LLD may be accepted.

### 9.1 Operator precedence

From highest to lowest:

| Precedence | Operators |
|---:|---|
| 1 | Function calls, member access, postfix `++` and `--` |
| 2 | Prefix `!`, unary `-`, prefix `++` and `--` |
| 3 | `*`, `/`, `%` |
| 4 | `+`, `-` |
| 5 | `<`, `<=`, `>`, `>=` |
| 6 | `==`, `!=` |
| 7 | `&&` |
| 8 | `||` |
| 9 | Assignment `=` |

Binary arithmetic and comparison operators associate left-to-right. Assignment associates right-to-left.

For example:

```grf
a = b = 5;
```

parses as:

```text
Assign(
    target = a,
    value = Assign(
        target = b,
        value = 5
    )
)
```

Assignment-target validation is semantic analysis, although the parser should preserve enough structure to identify the target expression.

### 9.2 Calls and dotted paths

The parser must support:

```grf
io.println("Hello");
io.println(square(5));
io.println(greet(123));
```

Represent `io.println` as a structured call target or member-access expression. Do not treat the entire expression as a single identifier string.

For example:

```text
CallExpression
├── callee: MemberExpression
│   ├── object: Identifier("io")
│   └── member: Identifier("println")
└── arguments
    └── CallExpression
        ├── callee: Identifier("square")
        └── arguments
            └── IntegerLiteral(5)
```

The parser does not determine whether `io` is imported or whether `println` exists.

### 9.3 Increment and decrement

The parser must recognize both prefix and postfix forms:

```grf
++index;
index++;
--index;
index--;
```

The AST must preserve whether the operator is prefix or postfix because their evaluation semantics differ.

The parser does not validate mutability, integer types, or overflow behavior.

### 9.4 Deferred operators

The following operators are not supported by the v0.1 parser:

- Unary `+`. A `+` token where an operand is expected is a syntax error.
- `+=`, `-=`, `*=`, `/=`, `%=`.
- `->`.
- `?`.
- `:` as a general expression or declaration operator.

The lexer does not emit one token for `+=` or `->`. It emits `+` then `=`, or `-` then `>`. The parser must reject those sequences. It must not build a compound-assignment or arrow node. `:` and `?` are lexer errors and do not reach the parser as tokens.

---

## 10. Literal and interpolated-string grammar

### 10.1 Ordinary literals

The parser must recognize the literal forms the lexer actually emits:

- Integer literals.
- Floating-point literals.
- `true` and `false`, which are keywords, as boolean literals.
- Ordinary string literals.

v0.1 has no character-literal syntax. `chr` remains a type name. A quote that would begin a character literal is a lexical error. Do not add a character-literal token as part of the parser task.

The parser should preserve literal values or their token representations according to the current AST design. Numeric range validation and literal type resolution are semantic responsibilities unless the lexer already reports lexical errors.

### 10.2 Interpolated strings

The existing lexer emits structured tokens for interpolation. The parser must consume these tokens and construct an AST node containing ordered text and expression parts.

Conceptual grammar:

```ebnf
InterpolatedString ::=
    InterpolatedStringStart
    { StringText | Interpolation }
    StringEnd ;

Interpolation ::=
    InterpolationStart Expression InterpolationEnd ;
```

Example:

```grf
str greeting = f"Hello {name}, age {age + 1}";
```

Expected AST shape:

```text
InterpolatedStringExpression
├── TextPart("Hello ")
├── ExpressionPart
│   └── Identifier("name")
├── TextPart(", age ")
└── ExpressionPart
    └── BinaryExpression
        ├── Identifier("age")
        ├── Plus
        └── IntegerLiteral(1)
```

Requirements:

- Preserve the order of text and expression parts.
- Parse expressions inside interpolation using the same expression grammar.
- Support nested function calls and parenthesized expressions within interpolation where the lexer can tokenize them correctly.
- Require each interpolation segment to terminate with `InterpolationEnd`.
- Require the interpolated string to terminate with `StringEnd`.
- Report malformed interpolation with a source-located diagnostic.
- Do not perform interpolation or string conversion at parse time.

Formatting rules and conversion compatibility are semantic/runtime concerns. The parser only constructs the interpolated-string AST.

If the lexer has limitations around nested braces, escaped interpolation delimiters, or nested strings, document and test those boundaries rather than inventing lexer behavior in the parser.

---

## 11. AST design

The AST must model syntax explicitly. It must not represent a function body, statement, or expression as a raw source string.

The names below are recommended. Adapt them to the existing Rust codebase without changing the required structure or semantics.

### 11.1 Program and declarations

```rust
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<FunctionDecl>,
    pub span: Span,
}

pub struct ImportDecl {
    pub domain: Identifier,
    pub library: Identifier,
    pub span: Span,
}

pub struct FunctionDecl {
    pub return_type: TypeSyntax,
    pub name: Identifier,
    pub parameters: Vec<Parameter>,
    pub body: BlockStmt,
    pub span: Span,
}

pub struct Parameter {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub span: Span,
}

pub enum ConversionMode {
    Exact,
    Auto,
}
```

`ConversionMode::Auto` represents the presence of `auto` in source syntax. It does not perform conversion or validate whether a conversion is possible.

### 11.2 Types and identifiers

```rust
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

pub struct TypeSyntax {
    pub kind: TypeKind,
    pub span: Span,
}

pub enum TypeKind {
    Void,
    Int,
    UInt,
    Flt,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Flt32,
    Flt64,
    Bln,
    Chr,
    Str,
}
```

Use the exact supported type inventory from the language specification. If the current implementation uses a different type representation, preserve the same distinctions and source locations.

Do not normalize aliases to canonical types in the parser. Alias normalization belongs to semantic analysis.

### 11.3 Statements

```rust
pub struct BlockStmt {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

pub enum Stmt {
    Block(BlockStmt),
    VariableDecl(VariableDecl),
    Return(ReturnStmt),
    If(IfStmt),
    Loop(LoopStmt),
    Break(Span),
    Continue(Span),
    Expression(ExpressionStmt),
}

pub struct VariableDecl {
    pub name: Identifier,
    pub ty: TypeSyntax,
    pub conversion: ConversionMode,
    pub initializer: Expr,
    pub span: Span,
}

pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: Span,
}

pub struct ExpressionStmt {
    pub expression: Expr,
    pub span: Span,
}

pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: BlockStmt,
    pub else_branch: Option<Box<Stmt>>,
    pub span: Span,
}

pub struct LoopStmt {
    pub kind: LoopKind,
    pub body: BlockStmt,
    pub span: Span,
}
```

```rust
pub enum LoopKind {
    Infinite,
    Condition { condition: Expr },
    CStyle {
        initializer: Option<VariableDecl>,
        condition: Option<Expr>,
        update: Option<Expr>,
    },
}
```

`Infinite` is only `loop ()`. A C-style initializer is either a variable declaration or absent. `loop (;;)` is rejected and is not represented as `Infinite`.

### 11.4 Expressions

```rust
pub enum Expr {
    Identifier(Identifier),
    Literal(LiteralExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Assignment(AssignmentExpr),
    Call(CallExpr),
    Member(MemberExpr),
    Update(UpdateExpr),
    InterpolatedString(InterpolatedStringExpr),
    Parenthesized {
        expression: Box<Expr>,
        span: Span,
    },
}
```

Recommended supporting structures:

```rust
pub struct UnaryExpr {
    pub operator: UnaryOperator,
    pub operand: Box<Expr>,
    pub span: Span,
}

pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub operator: BinaryOperator,
    pub right: Box<Expr>,
    pub span: Span,
}

pub struct AssignmentExpr {
    pub target: Box<Expr>,
    pub value: Box<Expr>,
    pub span: Span,
}

pub struct CallExpr {
    pub callee: Box<Expr>,
    pub arguments: Vec<Expr>,
    pub span: Span,
}

pub struct MemberExpr {
    pub object: Box<Expr>,
    pub member: Identifier,
    pub span: Span,
}

pub struct UpdateExpr {
    pub operator: UpdateOperator,
    pub operand: Box<Expr>,
    pub position: UpdatePosition,
    pub span: Span,
}

pub enum UpdatePosition {
    Prefix,
    Postfix,
}
```

The literal AST must preserve the actual literal kind and its source span. Use concrete Rust types or a validated literal representation consistent with the current compiler.

### 11.5 Interpolated-string AST

```rust
pub struct InterpolatedStringExpr {
    pub parts: Vec<StringPart>,
    pub span: Span,
}

pub enum StringPart {
    Text {
        value: String,
        span: Span,
    },
    Expression {
        expression: Expr,
        span: Span,
    },
}
```

The parser must not convert expression parts to strings. It must preserve their original expression ASTs for later semantic analysis and lowering.

### 11.6 AST design constraints

- All recursive AST edges must use appropriate indirection such as `Box<Expr>` where required by Rust's type system.
- Every node must preserve source-location information.
- AST types must derive appropriate traits such as `Debug` and `PartialEq` where practical for testing.
- AST nodes must be independent of the parser's mutable cursor state.
- Do not store token-debug output as the AST representation.
- Do not store the original source as a substitute for structured expressions.
- Avoid introducing semantic information into the AST prematurely.

---

## 12. Parser implementation strategy

Use a hand-written recursive-descent parser with a dedicated expression parser.

Recommended parser responsibilities:

```text
Parser
├── current token cursor
├── token lookahead
├── parser diagnostics
├── parse_program()
├── parse_import()
├── parse_function()
├── parse_type()
├── parse_parameter_list()
├── parse_block()
├── parse_statement()
├── parse_variable_declaration()
├── parse_return_statement()
├── parse_if_statement()
├── parse_loop_statement()
├── parse_expression()
├── parse_prefix_expression()
├── parse_postfix_expression()
├── parse_primary_expression()
└── parse_interpolated_string()
```

These are suggested method boundaries, not mandatory public APIs.

### 12.1 Token cursor

The parser must provide consistent helpers for:

- Reading the current token.
- Advancing the cursor.
- Checking token kinds.
- Matching and consuming expected tokens.
- Looking ahead without consuming tokens.
- Reporting the current token's span.
- Detecting EOF safely.

Do not advance beyond EOF or panic when the token stream is truncated.

### 12.2 Declaration parsing

At the top level:

1. Parse imports first.
2. Parse function declarations until EOF.
3. Diagnose any unexpected top-level token.
4. Do not attempt to interpret a statement as a top-level declaration.
5. Preserve declaration order in the AST.

A later semantic pass will build symbol tables and resolve forward references.

### 12.3 Expression parsing

The expression parser must implement the precedence rules defined in Section 9.

It must correctly parse nested calls, dotted access, unary operators, arithmetic, comparisons, logical operators, assignment, and update expressions where supported.

Do not use string splitting or regular expressions to parse expressions.

### 12.4 Error recovery

The parser should recover from common syntax errors to report more than one useful diagnostic where feasible.

Recommended synchronization points:

- Semicolons.
- Closing braces.
- The beginning of a new top-level function declaration.
- EOF.

Recovery must guarantee forward progress: after a recoverable error, either consume at least one token or return from the current parsing routine.

Do not emit an unlimited number of duplicate diagnostics for the same missing token.

A malformed construct must never cause an infinite loop.

---

## 13. Parser diagnostics

Use structured diagnostics rather than panic messages or raw strings alone.

A recommended representation:

```rust
pub struct ParserDiagnostic {
    pub kind: ParserDiagnosticKind,
    pub message: String,
    pub span: Span,
}

pub enum ParserDiagnosticKind {
    UnexpectedToken,
    ExpectedToken,
    InvalidImport,
    InvalidTypeSyntax,
    InvalidDeclaration,
    InvalidExpression,
    InvalidStatement,
    UnterminatedBlock,
    UnterminatedInterpolation,
    UnexpectedEof,
}
```

The exact enum may be adapted to the existing diagnostic infrastructure.

Example diagnostics:

```text
test.grf:1:14: error: expected ';' after import declaration
test.grf:4:19: error: expected ')' after function arguments
test.grf:7:1: error: expected '}' to close function body
test.grf:9:15: error: expected an expression after '+'
test.grf:12:20: error: expected '}' to close interpolation
```

These messages are illustrative. Exact wording may differ, but each diagnostic must identify the source location and explain the syntax expectation.

### 13.1 Diagnostic requirements

- Report the most relevant location.
- Identify the expected token or grammar construct where possible.
- Never report a parser panic for ordinary malformed source.
- Avoid cascading errors when one missing delimiter explains several later tokens.
- Preserve diagnostics even when recovery yields a partial AST.
- Do not emit semantic diagnostics from the parser.

For example, an unresolved function name is not a parser error if the call syntax is valid.

---

## 14. Required test suite

Use unit tests for individual grammar productions and integration tests for complete source files.

Tests must use the existing lexer and real token stream rather than hand-constructed tokens for every case. Hand-constructed token tests may be added for specific parser edge cases.

### 14.1 Valid program tests

At minimum, parse the following:

**Test A — Minimal entry point**

```grf
void main() {
}
```

Expected: one function declaration, zero diagnostics.

**Test B — Import and library call**

```grf
import grf.io;

void main() {
    io.println("Hello");
}
```

Expected: one import, one function, one expression statement.

**Test C — Nested calls**

```grf
import grf.io;

void main() {
    io.println(square(5));
}

int square(int value) {
    return value * value;
}
```

Expected: nested `CallExpr` nodes, a multiplication `BinaryExpr`, zero parser diagnostics.

**Test D — Forward function call**

```grf
void main() {
    later();
}

void later() {
}
```

Expected: successful parsing regardless of declaration order.

**Test E — `auto` parameter and interpolation**

```grf
str greet(auto str msg) {
    return f"Hello {msg}";
}
```

Expected: `ConversionMode::Auto` on the parameter and an interpolated-string expression with text and expression parts.

**Test F — Arithmetic precedence**

```grf
int calculate(int a, int b, int c) {
    return a + b * c;
}
```

Expected: the multiplication expression is the right operand of the addition expression.

**Test G — Conditional and loop syntax**

Parse the approved `if`, `else`, condition-loop, and C-style loop examples from the syntax LLD.

Expected: the correct structured statement nodes and zero diagnostics.

**Test H — Prefix and postfix update**

```grf
void main() {
    int index = 0;
    ++index;
    index++;
}
```

Expected: two update expressions with distinct prefix/postfix positions.

### 14.2 Invalid syntax tests

At minimum, reject:

1. Missing semicolon after an import.
2. Three-segment import paths.
3. Imports appearing after a function declaration.
4. Missing function parameter closing parenthesis.
5. Missing function body closing brace.
6. Missing semicolon after a return statement.
7. Missing expression after a binary operator.
8. Missing closing parenthesis in a call.
9. Missing closing brace in interpolation.
10. Unterminated interpolated string.
11. Trailing comma in a parameter list, unless the grammar is explicitly changed to allow it.
12. The arrow sequence `->`. Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`, `**=`, `//=`) and `**` are supported.
13. Unsupported class declarations.
14. Top-level statements that are not declarations.
15. Unexpected tokens after EOF-related malformed input.

For each negative test, verify that the parser returns a diagnostic with a valid source span and does not panic.

### 14.3 Recovery tests

Add tests where one syntax error is followed by a valid declaration.

For example:

```grf
int broken(int value {
    return value;
}

void main() {
}
```

The parser should report the malformed function signature and attempt to recover so that the later `main` declaration can still be recognized if practical.

Recovery behavior must be deterministic and must not loop indefinitely.

### 14.4 Regression test using the current lexer output

Use the current `test.grf` that produced 62 tokens and zero lexer diagnostics.

The parser must successfully parse its supported constructs, including:

- `import grf.io;`
- `void main()`.
- `io.println(square(5))`.
- `io.println(greet(123))`.
- The later declaration of `square`.
- The later declaration of `greet`.
- `auto str msg`.
- `f"Hello {msg}"`.

If the file contains syntax outside the frozen v0.1 grammar, adjust the test fixture to the approved grammar rather than expanding the parser to accept unsupported syntax.

---

## 15. Acceptance criteria

Stage 2 is complete only when all of the following conditions hold:

1. The parser consumes the existing lexer token stream.
2. Valid source produces a structured `Program` AST.
3. Import declarations preserve domain and library as separate identifiers.
4. Function declarations preserve return type, parameters, body, and source spans.
5. `auto T` is represented explicitly in the AST.
6. Nested calls and dotted member access parse correctly.
7. Expressions follow the defined operator precedence and associativity.
8. Interpolated strings preserve ordered text and expression parts.
9. Approved statement forms parse into structured nodes.
10. Forward function declarations are accepted syntactically.
11. Malformed source produces source-located parser diagnostics.
12. Parser error recovery makes forward progress and does not panic or loop indefinitely.
13. The parser does not perform type checking, manifest resolution, conversions, or Rust generation.
14. Unit tests and integration tests cover valid and invalid syntax.
15. `cargo fmt --all`, `cargo check --workspace`, and `cargo test --workspace` pass.
16. The current lexer implementation remains intact except for deliberate, documented changes required to support a grammar production.

---

## 16. Implementation workflow

The coding agent must follow this order:

1. Inspect the current repository and lexer token definitions.
2. Read LLD-02 and LLD-05 before implementing grammar productions.
3. Identify any discrepancies between the current lexer and this LLD.
4. Propose minimal lexer changes only where necessary.
5. Implement AST types.
6. Implement the token cursor and parser infrastructure.
7. Implement imports and function declarations.
8. Implement blocks, statements, and expressions.
9. Implement interpolation parsing.
10. Add source-located diagnostics and error recovery.
11. Add unit and integration tests.
12. Run formatting, workspace checking, and tests.
13. Report the files changed, supported syntax, test results, and unresolved grammar questions.

Do not begin semantic analysis or code generation as part of this task.

## 17. Binding parser decisions

These rules override conflicting examples elsewhere in this LLD.

- `const` is rejected. It is not an AST declaration and not an identifier.
- Character literals are not part of v0.1. `true` and `false` are boolean literals. `chr` is only a type name.
- Unary `+` is rejected. Unary operators are `-`, `!`, and prefix `++` / `--`.
- A C-style initializer is a variable declaration with `=` or is empty. `loop (index = 0; ...)` is rejected.
- `loop ()` is the `Infinite` AST variant. `loop (;;)` is rejected.
- The parser accepts `void` in every type position this grammar allows. Semantic analysis rejects `void` variables and `void` parameters.
- Variable declarations require `=`.
- Parentheses are a dedicated AST node. The span includes both parentheses.
- `+=` and `->` are rejected as token sequences. `:` and `?` are lexer errors.
- Empty `return;` is syntactically legal in every function. Whether the enclosing function may use it is a semantic check.
- `&&`, `||`, and `!` are in the v0.1 operator inventory. The parser implements the precedence table in section 9, with assignment associating right-to-left.
- The shared source span type is `crate::span::Span`. Parser diagnostics are separate from lexer diagnostics and use that span.

---

## Final directive

Implement **Stage 2 — Parser and AST only**.

The result must be a tested parser that transforms valid Graphite tokens into a structured AST and reports useful diagnostics for invalid syntax. Keep semantic analysis, `grf.json` resolution, `auto T` conversion execution, Rust code generation, and native compilation as later stages.