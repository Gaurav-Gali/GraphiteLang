# LLD-15 --- Stage 11: Structured Diagnostics and Error Reporting

**Project:** Graphite Lang (`.grf`)\
**Stage:** 11 --- Structured diagnostics and error reporting\
**Status:** Implementation specification

## 1. Purpose

Build a Graphite-owned diagnostic system that reports specific,
source-located, actionable errors across lexing, parsing, declaration
collection, name resolution, semantic analysis, type checking, typed-AST
validation, Rust code generation, Cargo/Rust compilation and linking,
dependency resolution, and runtime execution.

Diagnostics are structured data first and terminal text second.
Detection, propagation, and rendering must be separate. This stage
establishes source-location and error-propagation infrastructure
required by exception handling, classes, and Rust-backed libraries.

### Goals

-   Define category, stable code, severity, message, primary source
    span, optional related spans, notes, help, and cause/context.
-   Preserve `.grf` locations across compiler phases.
-   Render filename, line/column, excerpt, caret/range, related
    locations, notes, and actionable help.
-   Report independent errors where recovery is safe, while suppressing
    cascades.
-   Map generated Rust locations to `.grf` spans only when the mapping
    is reliable.
-   Standardize runtime errors, including array bounds and
    missing-map-key errors.
-   Never generate Rust from an invalid or incompletely typed AST.

### Non-goals

-   Language-level exception syntax (`try`/`catch`/`throw`), which
    belongs to Stage 12.
-   Full LSP/IDE integration.
-   LLM-generated compiler output, source-text rewriting, or regex-based
    transpilation.
-   Guaranteeing every Rust build error can be mapped to a `.grf` line.
-   Requiring JSON diagnostics before the core model and terminal
    renderer are complete.

## 2. Pipeline and phase contracts

The pipeline remains:

1.  Source loading
2.  Lexing
3.  Parsing
4.  Declaration collection
5.  Name resolution and semantic analysis
6.  Type checking
7.  Typed-AST validation
8.  Rust code generation
9.  Cargo/Rust build and link
10. Runtime execution

Each phase returns a result and zero or more structured diagnostics.
Compiler phases must not print directly to stderr. The CLI renders
compiler diagnostics. Runtime diagnostics are rendered by the generated
program, as specified in section 9 and section 18.

An implementation may use a type equivalent to:

``` rust
struct PhaseOutput<T> {
    value: Option<T>,
    diagnostics: Vec<Diagnostic>,
}
```

`Some(value)` means the phase produced valid output for the next phase.
`None` means it cannot safely continue. A partial AST may be used
internally for recovery but cannot reach code generation unless fully
validated. Warnings alone do not stop compilation.

  -----------------------------------------------------------------------
  Phase                               Recovery policy
  ----------------------------------- -----------------------------------
  Source loading                      Stop if the entry source cannot be
                                      read; no source span if text is
                                      unavailable

  Lexer                               Continue if token boundaries can be
                                      recovered safely; malformed tokens
                                      retain spans

  Parser                              Recover at safe
                                      statement/declaration boundaries;
                                      mark synthetic/invalid nodes

  Declaration collection              Report independent conflicts; never
                                      create ambiguous bindings

  Name resolution                     Use invalid/error symbols for
                                      unknown names to prevent cascades

  Semantic/type analysis              Continue for independent
                                      constructs; poison invalid
                                      intermediate values

  Typed-AST validation                Collect safe invariant failures;
                                      any error forbids code generation

  Code generation                     Stop on invariant failure; report
                                      compiler/codegen error

  Rust build/link                     Capture process output and exit
                                      status; do not execute on failure

  Runtime                             Propagate structured error; CLI
                                      renders unhandled failure
  -----------------------------------------------------------------------

Every parser recovery attempt must consume input or terminate,
preventing infinite loops.

## 3. Source model and spans

### 3.1 Source file identity

Every loaded file receives a compilation-local `SourceFileId` and is
registered in a source map:

``` rust
struct SourceFile {
    id: SourceFileId,
    display_name: String,
    canonical_path: Option<PathBuf>,
    text: Arc<str>,
    line_starts: Vec<usize>,
}
```

-   `display_name` is the user-facing filename, preferably relative to
    the entry file when safe.
-   `canonical_path` is optional for identity/deduplication; avoid
    exposing absolute paths unnecessarily.
-   `text` preserves the original UTF-8 source.
-   `line_starts` indexes line-start byte offsets.
-   Source IDs are compilation-local, not stable public identifiers.

### 3.2 Span representation

Use half-open byte ranges into the original UTF-8 source:

``` rust
struct Span {
    source_id: SourceFileId,
    start_byte: u32,
    end_byte: u32, // exclusive
}
```

The implementation may use `u64` if supported source sizes require it,
but must choose one width consistently and validate conversions. Both
offsets must be UTF-8 character boundaries and satisfy
`start_byte <= end_byte <= source length`. Empty spans are allowed for
insertion points such as a missing token at EOF.

Offsets are authoritative. Derive line/column from the source map rather
than storing independently maintained values. Lines and displayed
columns are 1-based. Caret placement must account for UTF-8, terminal
display width, and tabs; byte offsets are not visible columns. Handle LF
and CRLF correctly. Multi-line and very long spans must render bounded
excerpts.

A source span may be unavailable for unreadable files, unmapped Rust
failures, dependency failures without tracked config spans, or internal
failures without a source construct. Use `None`; never fabricate `0:0`
or point to the first line as a substitute.

## 4. Structured diagnostic model

### 4.1 Core schema

Use a model equivalent to:

``` rust
enum DiagnosticSeverity { Error, Warning, Note }

enum DiagnosticCategory {
    Lexical, Parse, NameResolution, Semantic, TypeChecking,
    CodeGeneration, RustBuild, DependencyResolution, Runtime, Internal,
}

struct Diagnostic {
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    category: DiagnosticCategory,
    message: String,
    primary_span: Option<Span>,
    related: Vec<RelatedSpan>,
    notes: Vec<String>,
    help: Option<String>,
    cause: Option<Box<DiagnosticCause>>,
}

struct RelatedSpan {
    span: Span,
    message: String,
}
```

`DiagnosticCode` may be an enum or namespace/number internally, but its
observable string must be stable and machine-readable, such as
`GRF-TYP-001`.

### 4.2 Field semantics

-   **code:** stable identity for a diagnostic condition; never reuse
    for a materially different condition.
-   **severity:** error, warning, or note.
-   **category:** broad phase/meaning group, independent of code.
-   **message:** concise explanation, not a pre-rendered terminal block.
-   **primary_span:** most relevant source range, if known.
-   **related:** other source locations with explanations.
-   **notes:** additional context.
-   **help:** actionable suggestion only when reliable; omit speculative
    advice.
-   **cause:** structured lower-level detail, such as Rust diagnostic
    metadata, process status, or I/O cause. Do not put arbitrary Rust
    debug dumps into the primary message.

### 4.3 Stable code registry

Reserve code families:

  Prefix      Category
  ----------- --------------------------------------
  `GRF-LEX`   Lexical
  `GRF-PAR`   Parse
  `GRF-NAM`   Name resolution
  `GRF-SEM`   Semantic analysis
  `GRF-TYP`   Type checking
  `GRF-CGV`   Typed-AST validation/code generation
  `GRF-RST`   Rust compilation/linking
  `GRF-DEP`   Dependency resolution
  `GRF-RUN`   Runtime
  `GRF-INT`   Internal compiler failure

Initial example codes: - `GRF-LEX-001`: invalid character or malformed
token - `GRF-PAR-001`: expected token not found - `GRF-NAM-001`: unknown
name - `GRF-NAM-002`: duplicate declaration - `GRF-TYP-001`: type
mismatch - `GRF-TYP-002`: unsupported operator for operand types -
`GRF-CGV-001`: typed-AST/codegen invariant failure - `GRF-RST-001`: Rust
build failed without reliable `.grf` mapping - `GRF-DEP-001`: dependency
could not be resolved - `GRF-RUN-001`: array index out of bounds -
`GRF-RUN-002`: missing map key - `GRF-INT-001`: unexpected compiler
invariant failure

These are initial examples, not the complete registry. Every existing,
materially distinct diagnostic condition needs its own stable code in
Stage 11. The published list is section 18. Once published, codes must
not be renumbered or repurposed; add codes instead. Different conditions
may share a code only when they are the same condition with different
contextual details.

## 5. Source-location propagation

### Lexer and parser

-   Every token carries a span.
-   AST nodes carry full-construct spans or enough child spans to
    compute one.
-   Synthetic/missing nodes are marked invalid/synthetic and use a
    zero-width insertion span.
-   Parser recovery preserves the offending span.
-   Delimiter errors may relate the missing delimiter to the opening
    delimiter.

### Declaration collection and semantic analysis

-   Symbol-table declarations retain their declaration spans.
-   Unknown-name errors point to the use site.
-   Duplicate declaration errors point to the duplicate and relate the
    first declaration.
-   Invalid declarations have explicit invalid state and cannot be
    treated as valid downstream.
-   Cross-file references preserve the source ID of their original file.

### Type checking and typed AST

-   Typed nodes retain source spans from the AST.
-   Type-check results retain the expression's static type and origin
    span.
-   Mismatches identify the responsible expression and report canonical
    Graphite expected/actual types when known.
-   An internal `Error`/`Invalid` type suppresses diagnostics caused
    only by a previous error; it is not user-declarable.
-   Typed-AST validation confirms every codegen-reachable node has
    required type and source metadata.

## 6. Generated Rust source maps

### 6.1 Mapping model

Generated Rust must be emitted through a source-aware writer, not
arbitrary concatenation that loses provenance. Track generated regions
and their origin:

``` rust
enum GeneratedOrigin {
    Graphite(Span),
    CompilerGenerated { description: String },
    Unmapped,
}

struct GeneratedRange {
    generated_start_line: u32,
    generated_end_line: u32, // inclusive
    origin: GeneratedOrigin,
}
```

Byte-range mappings are preferred where practical. The same writer
operation must emit Rust and mapping metadata so line mappings cannot
drift. Each user-derived generated expression/statement should have a
mapping where feasible. Runtime boilerplate may be compiler-generated or
unmapped.

Store generated Rust and source-map metadata together under the
established `target/grf/builds/<entry-key>` directory. Tie the map to
the exact generated file/build; invalidate it whenever generated output
changes.

### 6.2 Rust and Cargo errors

-   Capture process exit status, stdout, and stderr.
-   Prefer Cargo/rustc machine-readable JSON diagnostics where
    available; do not rely primarily on parsing human-formatted output.
-   Convert failures into Graphite `RustBuild` diagnostics, preserving
    Rust code/message and useful notes in structured cause/context.
-   If the failing generated region maps unambiguously to a `.grf` span,
    use that span and explain the Rust failure in a note.
-   If the failure maps to generated boilerplate, spans mixed regions,
    or is otherwise ambiguous, do not claim a `.grf` location. Include
    generated-file/line context when useful.
-   Linker failures usually have no reliable expression-level mapping;
    report them without a Graphite primary span unless evidence supports
    one.
-   Limit captured output and explicitly indicate truncation.
-   Do not expose environment secrets or a raw panic/backtrace by
    default.
-   Never execute the generated program after a failed build.

## 7. Terminal renderer

The renderer consumes diagnostics plus the source map and does not
perform semantic analysis.

Required output includes severity, stable code, message,
filename/line/column when available, source excerpt, caret/range
highlight, related locations, notes, and help. Provide plain-text
fallback without ANSI color.

Example:

``` text
error[GRF-TYP-001]: type mismatch
  --> examples/main.grf:8:17
   |
 8 | int count = "ten";
   |             ^^^^^ expected `int64`, found `str`
   |
   = note: the variable is declared as `int64`
   = help: convert the value to an integer, or change the variable type
```

Renderer requirements: - Lines and columns are 1-based. - Carets align
with tabs and Unicode display width. - Zero-width spans show an
insertion caret. - Multi-line spans use compact, bounded excerpts. -
Very long lines/ranges are truncated sensibly. - Color is optional and
never the only severity signal. - Sort deterministically by source-file
order, primary start offset, then stable tie-breaker. Diagnostics
without spans render after source-located diagnostics or in a separate
causal section. - Show a summary such as `2 errors, 1 warning`. - Same
structured diagnostic must work in CI logs and non-interactive
terminals.

## 8. Recovery and cascade suppression

### 8.1 Lexer/parser

-   Report the narrowest reliable invalid-token span.
-   Consume enough input to make progress.
-   Synchronize at grammar-safe boundaries such as statement
    terminators, closing braces, or the next declaration.
-   Preserve valid neighboring constructs.
-   Avoid one diagnostic per byte for one malformed token.

### 8.2 Semantic/type recovery

When a name or expression is invalid, record an internal poisoned value.
Example:

``` grf
int value = unknownName;
int next = value + 1;
```

Report the unknown name. Do not report a second mismatch for `value + 1`
if its only cause is the first error. Still report independent errors
elsewhere.

De-duplicate only when same code, same span, and same root cause prove
the diagnostics are the same event. Do not merge distinct errors merely
because messages are similar.

### 8.3 Mandatory codegen gate

**Rust code generation must never run on an invalid or incompletely
typed AST.**

Before code generation: 1. Parsing has produced structurally valid
codegen-reachable AST nodes. 2. Required declarations and names are
resolved. 3. Semantic analysis and type checking are complete for all
generated nodes. 4. Typed-AST validation passes. 5. No unresolved
`Error`/`Invalid` values remain in codegen-reachable nodes.

If any condition fails, render diagnostics and stop. Do not emit/run a
partial executable. A stale executable from a previous build must never
be run after the current compilation fails.

## 9. Runtime error representation

### 9.1 Shared model

Use a structured runtime error type shared by Graphite runtime helpers
and generated code:

``` rust
struct RuntimeError {
    kind: RuntimeErrorKind,
    message: String,
    source_span: Option<Span>,
    context: Vec<RuntimeContext>,
    cause: Option<Box<RuntimeErrorCause>>,
}

enum RuntimeErrorKind {
    ArrayIndexOutOfBounds,
    InvalidSlice,
    MissingMapKey,
    DivisionByZero,
    NumericConversion,
    UserRuntimeError,
    Io,
    ExternalLibrary,
    Internal,
    Other(String),
}

struct RuntimeContext {
    message: String,
    source_span: Option<Span>,
}
```

Concrete types may differ, but every runtime failure retains a stable
kind, message, optional source span, and optional cause/context. Runtime
errors convert to common diagnostics with category `Runtime` and stable
`GRF-RUN` codes.

### 9.2 Span attachment and propagation

-   Generated checks receive or are associated with the original `.grf`
    operation span.
-   Array indexing/slicing, map lookup, numeric conversion, and other
    fallible operations carry the expression span when available.
-   Runtime helpers accept source context explicitly; they must not
    infer it from cwd or a Rust backtrace.
-   Preserve the first meaningful source span when wrapping errors.
-   If no reliable span exists, report no span instead of guessing.
-   Runtime helpers return or propagate structured errors. They do not
    print or exit on their own.
-   The generated program has one entry-point handler. It converts an
    unhandled runtime error into the common diagnostic model, renders it
    with the shared renderer, and exits with a non-zero status.
-   `grf run` does not capture or interpret that runtime output, and
    Stage 11 does not add a structured-record protocol between the child
    process and the CLI. The error model and the renderer stay separate
    so a later version can move rendering into `grf run`.
-   Stage 12 exception handling must build on this representation.

### 9.3 Required existing errors

**Array bounds:** Out-of-range indexing returns `ArrayIndexOutOfBounds`,
code `GRF-RUN-001`, with the indexing expression's `.grf` span. The
label keeps both the attempted index and the array length, for example
`index 5 is out of bounds for an array of length 3`. Negative indices
use the existing from-the-end indexing rules; the diagnostic reports
the index value that was evaluated. Avoid raw Rust panic output. A zero
slice step is its own runtime condition, `GRF-RUN-003`. Clamped slice
bounds follow the array LLD and are not invalid indexing.

**Missing map key:** Checked lookup via `m[key]` or `m.get(key)` returns
`MissingMapKey`, code `GRF-RUN-002`. Do not include the actual key value.
Use the standard diagnostic layout and keep the map's canonical Graphite
type. The substantive wording stays `missing key: lookup on 'map<str, int64>'`.
The filename, line, column, excerpt, and caret come from the real
expression span. If that span is unavailable, render the diagnostic
without inventing a location.

``` text
error[GRF-RUN-002]: missing map key
  --> mintest.grf:5:5
   |
 5 | value = scores["Ada"];
   |         ^^^^^^^^^^^^^^ missing key: lookup on 'map<str, int64>'
```

The example is illustrative. The source line and span come from the
program being run.

Stage 11 also integrates the other existing source-located runtime
failures: integer overflow (`GRF-RUN-004`), division by zero
(`GRF-RUN-005`), numeric conversion (`GRF-RUN-006`), and I/O
(`GRF-RUN-007`). Slice assignment length, destructuring length, maximum
array length, and invalid insertion position have their own `GRF-RUN`
codes in section 18. Unexpected internal failures use `GRF-INT`, not a
user runtime code.

Expected Graphite runtime failures must use explicit error propagation,
not Rust panics. Unexpected internal panics may be caught at a
well-defined CLI boundary if implemented, but must be reported as
internal failures and must not claim a source location unless explicitly
attached. Do not continue execution after an internal panic.

## 10. Type details and actionable suggestions

Where reliable, report canonical Graphite expected/actual types, not
generated Rust types. Suggestions must follow existing language rules
and conversion semantics. Do not suggest `auto` or explicit conversion
if conversion would fail, overflow, lose precision, or otherwise be
invalid. No automatic code edits are required in Stage 11.

Use related spans for duplicate declarations, conflicting definitions,
expected-type declarations, or matching opening delimiters when useful.
Do not attach unrelated spans.

Example mismatch:

``` text
error[GRF-TYP-001]: type mismatch
  --> main.grf:4:18
   |
 4 | int age = "21";
   |           ^^^^ expected `int64`, found `str`
```

Only append conversion help if the centralized conversion registry and
the established conversion rules allow it.

-   A statically known value is recommended only when that specific
    value is valid and representable. `int("21")` may be suggested when
    the rules accept that literal. `int("ten")` is not suggested,
    because that value fails.
-   When the value is not statically known, help may say that an
    explicit conversion is allowed and checked at runtime. It must not
    promise that the conversion succeeds.
-   Respect `auto T` versus strict typed assignment.
-   Never recommend a conversion that may silently lose precision,
    overflow, underflow, or otherwise violate the conversion rules.
    Float-to-integer conversion floors, so it is not a suggestion unless
    the known value is already integral and in range.
-   If no reliable fix can be recommended, omit `help`.

## 11. Dependency-resolution diagnostics

-   Use category `DependencyResolution` and stable `GRF-DEP` codes.
-   Include dependency name and actionable manifest/configuration
    context where available.
-   If an import or `grf.json` entry has a tracked span, attach it;
    otherwise do not pretend a manifest error is a `.grf` location.
-   Distinguish unknown library, unavailable package, invalid
    version/constraint, invalid mapping, and download/build failure when
    reliably known.
-   Do not leak credentials, tokens, or sensitive registry URLs.
-   Failure to resolve a required dependency prevents compilation.

## 12. Optional JSON diagnostics

JSON is optional and must not delay the core system. If implemented: -
Serialize the same diagnostic objects; do not create a second error
model. - Include stable code, category, severity, message, path, byte
span, line/column, related spans, notes, help, and safe cause/build
metadata. - Version the schema before external use. - Consumers branch
on stable codes, not message text. - In JSON mode, stdout contains valid
JSON only; progress logs go to stderr. - Avoid exposing absolute paths
or environment secrets by default.

## 13. CLI behavior

-   Default diagnostics go to stderr.
-   Compile errors, Rust build/link errors, and unhandled runtime errors
    return non-zero exit status.
-   Warnings alone do not fail the build absent a separate warning
    policy.
-   No executable runs after compilation or linking fails.
-   Stale artifacts cannot be mistaken for the current build.
-   Multiple diagnostics include an error/warning count.
-   Any new CLI flag must be documented.

## 14. Implementation sequence

1.  **Source map foundation:** `SourceFileId`, `SourceFile`, `Span`,
    line starts, byte-to-location conversion; attach spans to tokens and
    AST nodes.
2.  **Model and renderer:** stable codes, categories, severities,
    related spans, notes/help; deterministic terminal output and
    summary.
3.  **Front-end integration:** lexer/parser diagnostics, recovery,
    declaration spans, name resolution, type checking, invalid/error
    state.
4.  **Typed-AST gate:** validate invariants and make codegen accept only
    validated typed AST; prevent stale-binary execution.
5.  **Rust mapping:** source-aware code writer, generated-source map,
    Cargo JSON diagnostics, build/link capture.
6.  **Runtime integration:** structured `RuntimeError`, source-aware
    array/map checks, CLI boundary conversion.
7.  **Dependency and optional JSON integration:** standardize dependency
    failures; implement JSON only after the core system works.

## 15. Testing and acceptance criteria

### Source-map tests

-   First/final character, newline boundaries, EOF, LF, CRLF, tabs,
    Unicode, zero-width and multi-line spans.
-   Invalid offsets are rejected, not silently clamped to misleading
    locations.

### Renderer/model tests

-   Correct stable code/category/severity.
-   Notes, related spans, and help render deterministically.
-   Plain-text output works without ANSI.
-   Carets align for tabs/Unicode.
-   Diagnostics sort deterministically.
-   Long lines and process output are bounded.

### Recovery tests

-   Multiple independent lexical, parse, name, and type errors are
    reported where safe.
-   Recovery always makes progress.
-   Invalid declarations do not produce cascading error floods.
-   Unknown names suppress follow-on errors caused only by that unknown
    name.
-   Duplicate declarations show both locations.
-   Invalid or incompletely typed AST never reaches codegen.
-   Failed builds never execute stale binaries.

### Rust build tests

-   A generated Rust error on a mapped user expression maps to the
    original `.grf` span.
-   Generated boilerplate errors are not falsely attributed to `.grf`.
-   Unmapped Rust/linker errors have no fabricated Graphite span.
-   Source maps are invalidated when generated Rust changes.

### Runtime tests

-   Array bounds emits `GRF-RUN-001` and source span when available.
-   Missing map key emits `GRF-RUN-002`, includes map type, and never
    includes key contents.
-   Zero-step slicing emits a structured runtime error.
-   Propagation preserves cause/context and original useful span.
-   Expected runtime errors do not surface as raw Rust panic text.
-   Missing source context stays missing.

### Acceptance gate

Stage 11 is complete only when: 1. All compiler phases use structured
diagnostics rather than ad hoc terminal strings. 2. Source spans survive
through typed-AST validation. 3. Terminal output includes location,
excerpt, caret/range, and notes/help when available. 4. Stable codes
distinguish all required categories. 5. Recovery reports independent
errors and suppresses cascades. 6. An enforced typed-AST gate prevents
invalid code generation. 7. Rust build/linker failures are captured and
mapped only when reliable. 8. Runtime errors share the structured model,
including array bounds and missing map keys. 9. Detection/propagation
are separate from rendering. 10. Automated tests cover source maps,
rendering, recovery, build failures, and runtime errors.

## 16. Locked constraints

-   Graphite owns the diagnostic abstraction and user-facing categories.
-   Source byte offsets are authoritative; line and display column are
    derived.
-   Diagnostics are structured before rendering.
-   No source location is fabricated when unavailable.
-   Rust errors map to Graphite source only with reliable source
    mapping.
-   Runtime errors preserve kind, message, optional span, cause, and
    context.
-   Expected runtime errors do not depend on Rust panics.
-   Code generation is forbidden for invalid or incompletely typed ASTs.
-   Stale artifacts are never executed after a failed build.
-   JSON output is optional and subordinate to the core model.
-   Stage 12 exception handling reuses this runtime error
    representation.

## 17. Deferred implementation choices

The following may be chosen during implementation without changing the
requirements: - Exact Rust crate/module layout. - Enum versus
namespace/number internal code representation. - `u32` versus `u64`
source offsets, based on supported source-size limits. - Terminal color
palette and excerpt styling. - Exact Cargo JSON parser implementation. -
Whether JSON output is added now or deferred. - Internal runtime
context-frame and cause representation.

Any change weakening source-location correctness, stable diagnostic
identity, runtime error structure, or the code-generation safety gate
requires an explicit design revision.

## 18. Locked Stage 11 decisions

These decisions are implementation requirements.

1.  **Who renders runtime errors.** The generated program renders them.
    There is no structured IPC protocol with `grf run` in this stage.
    Helpers return structured errors. One handler in the generated
    program renders the common diagnostic and exits non-zero. Formatting
    is shared with the compiler. Array, map, conversion, numeric, and
    I/O helpers do not format their own text.
2.  **Which runtime failures.** Every existing source-located runtime
    failure joins the structured system, not only the three examples.
    Each materially distinct condition has its own `GRF-RUN` code.
    Existing source locations are preserved. Internal failures stay on
    `GRF-INT`.
3.  **Array bounds text.** The message includes the attempted index and
    the array length, on the real expression span, including negative
    indices as the array rules define them.
4.  **Missing-key text.** Standard layout, canonical map type, no key
    value, and the wording `missing key: lookup on '<map type>'`. No
    fabricated span.
5.  **Registry completeness.** Every existing, materially distinct
    compiler and runtime condition has a stable code. Messages do not
    all have to be rewritten, but every emitted diagnostic goes through
    the structured model and carries its code. Codes are not renumbered
    after publication.
6.  **Conversion help.** Omitted unless the registry and the rules make
    the suggestion valid for that context and, when the value is known,
    for that value.

The shared renderer is linkable from generated Rust, so it lives with
the runtime rather than only inside the compiler. The generated program
embeds the source excerpt for a runtime span when the compiler has that
line, which is how the child renders carets without IPC and without
rereading a file that may have changed.

### Published codes

Lexical: `GRF-LEX-001` unexpected character, `002` unterminated string,
`003` invalid escape, `004` unterminated block comment, `005` malformed
number, `006` unterminated interpolation, `007` malformed interpolation.

Parse: `GRF-PAR-001` expected token, `002` unexpected token, `003`
invalid import, `004` invalid type syntax, `005` invalid declaration,
`006` invalid expression, `007` invalid statement, `008` unterminated
block, `009` unterminated interpolation, `010` unexpected end of input.

Names: `GRF-NAM-001` unknown name or function, `002` duplicate
declaration, `003` reserved name or keyword, `006` unknown type, `007`
unknown field.

Types: `GRF-TYP-001` type mismatch, `002` unsupported operator, `003`
unsupported conversion, `004` void used where a value is required, `005`
condition is not `bln`, `006` illegal map key, `007` value is not an
array or map, `008` `lengthOf` or `typeOf` misuse, `010` integer literal
out of range, `011` floating literal out of range, `012` enumeration
index is not `uint64`, `013` index is not an integer, `015` conversion
arity.

Semantic: `GRF-SEM-001` constant mutation, `002` break or continue
outside a loop, `003` invalid or missing `main`, `004` return-path
error, `005` record cycle, `006` record literal shape, `007` array
literal shape, `008` loop source, `009` destructuring shape, `010`
`auto` not allowed, `011` unspecified short-circuit operator, `012`
unsupported call or member use, `013` illegal assignment target, `014`
increment or decrement target, `015` map slice, `016` method or call
arity, `017` mutation during iteration, `018` untargeted brace literal,
`019` illegal spread, `020` incomplete map type, `021` missing loop
condition, `022` call of a non-function.

Dependencies: `GRF-DEP-001` domain not found, `002` duplicate import,
`003` import name conflict, `004` library not found, `005` function not
exported, `006` unknown library return type.

Code generation and Rust: `GRF-CGV-001` codegen or typed-AST invariant,
`GRF-RST-001` Rust build or link failure without a reliable `.grf` span,
`GRF-INT-001` unexpected internal failure.

Runtime: `GRF-RUN-001` array index out of bounds, `002` missing map key,
`003` zero slice step, `004` integer overflow, `005` division by zero,
`006` numeric conversion failure, `007` input/output failure, `008`
slice assignment length mismatch, `009` destructuring length mismatch,
`010` array exceeds its maximum length, `011` invalid insertion
position. Later stages add `012` external library failure, `013`
structured internal runtime failure, and `014` frozen-value mutation
without renumbering these codes.
