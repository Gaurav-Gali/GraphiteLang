# LLD-16 --- Stage 12: Exception Handling

**Project:** Graphite Lang (`.grf`)\
**Stage:** 12 --- Exception handling\
**Status:** Implementation specification\
**Prerequisite:** Stage 11 --- Structured diagnostics and error
reporting

## 1. Purpose

Implement Graphite-level `try`, `catch`, and `finally` on top of Stage
11's structured runtime error model. Define semantics first, then
implement parsing, AST, semantic checks, runtime propagation, and Rust
lowering.

Exceptions are Graphite runtime errors with preserved identity, source
information, and cause/context. They are not source rewrites and are not
aliases for Rust `panic!`.

## 2. Locked semantic decisions

1.  Implement `try`, `catch`, and `finally`; defer `throw` and
    user-defined error types.
2.  Catch clauses name built-in Graphite runtime-error types only. These
    are special catch types, not ordinary constructible value types.
3.  Each catch clause has exactly one error type and one binding. Union
    catches and omitted bindings are deferred.
4.  Catch clauses are tested in source order. Unreachable catches are
    compile-time errors.
5.  `RuntimeError` and `Error` each match every catchable Graphite runtime error.
6.  A try statement requires at least one `catch` or `finally`;
    `finally` is optional and unique, and must be last.
7.  `finally` runs exactly once when control exits its associated try
    statement, including normal completion, handled/unhandled errors,
    `return`, `break`, and `continue`.
8.  A new runtime error in `finally` replaces the pending outcome. If a
    previous runtime error was pending, preserve it as cause/context.
9.  Graphite runtime errors propagate implicitly across Graphite
    function calls; no `throws` declaration is required.
10. Rust `Result::Err` from supported libraries must be mapped
    explicitly by adapters to Graphite runtime errors.
11. Rust panics are not automatically catchable Graphite exceptions.
    Expected failures use `Result`/structured propagation.
12. Capture Graphite call-trace frames at propagation boundaries where
    feasible. Do not infer Graphite stack frames from Rust backtraces.

## 3. Syntax and grammar

### 3.1 Example

``` grf
try {
    int result = divide(10, 0);
    io.println(result);
} catch (DivisionByZeroError error) {
    io.println(error.message());
} catch (RuntimeError error) {
    io.println(error.message());
} finally {
    io.println("Operation completed");
}
```

### 3.2 Grammar

Illustrative EBNF to integrate with the existing grammar:

``` ebnf
try_statement   = "try", block, catch_clause*, finally_clause ;
catch_clause    = "catch", "(", error_type, identifier, ")", block ;
finally_clause  = "finally", block ;

error_type      = "RuntimeError"
                | "ArrayIndexOutOfBoundsError"
                | "InvalidSliceError"
                | "MissingMapKeyError"
                | "DivisionByZeroError"
                | "IntegerOverflowError"
                | "NumericConversionError"
                | "IoError"
                | "ExternalLibraryError"
                | "RuntimeInternalError" ;
```

Semantic validation requires at least one catch or finally. Prefer
parsing the structure and reporting the missing handler/cleanup
semantically, if the parser architecture supports it.

A try statement is a statement, not an expression. `try`, `catch`,
`finally`, and `throw` are reserved keywords. `throw` is reserved even
though a throw statement is not implemented; using it as a name is a
reserved-keyword error, and using it as a statement reports that
explicit throwing is not supported in this version. Catch/finally
without a preceding try, duplicate finally, catch after finally, and
malformed clauses are rejected. Do not implement these constructs
through source-text rewriting.

## 4. AST representation

Use explicit AST nodes with source spans:

``` rust
struct TryStmt {
    try_block: Block,
    catches: Vec<CatchClause>,
    finally_block: Option<Block>,
    span: Span,
}

struct CatchClause {
    error_type: BuiltinErrorType,
    binding: Identifier,
    error_type_span: Span,
    binding_span: Span,
    block: Block,
    span: Span,
}
```

Names are illustrative. Preserve spans for the full try statement, each
clause, type token, binding, and blocks. Catch types use a dedicated
built-in error-type representation, not ordinary record/type lookup.
Preserve spans into semantic analysis and typed-AST validation.

## 5. Built-in runtime error model

Extend Stage 11's shared `RuntimeError`; do not create a competing
exception model. Conceptually:

``` rust
struct RuntimeError {
    kind: RuntimeErrorKind,
    code: DiagnosticCode,
    message: String,
    source_span: Option<Span>,
    context: Vec<RuntimeContext>,
    cause: Option<Box<RuntimeErrorCause>>,
    trace: Vec<GraphiteTraceFrame>,
}

struct GraphiteTraceFrame {
    function_name: String,
    call_site: Option<Span>,
}
```

If code is derived from kind, centralize that mapping. Codes must remain
stable.

### 5.1 Catch types and matching

  -----------------------------------------------------------------------
  Catch type                      Code            Runtime condition
  ------------------------------- --------------- ---------------------------
  `ArrayIndexOutOfBoundsError`    `GRF-RUN-001`   Array index out of bounds

  `MissingMapKeyError`            `GRF-RUN-002`   Missing map key

  `InvalidSliceError`             `GRF-RUN-003`   Zero slice step

  `IntegerOverflowError`          `GRF-RUN-004`   Checked integer overflow

  `DivisionByZeroError`           `GRF-RUN-005`   Division by zero and
                                                  remainder by zero

  `NumericConversionError`        `GRF-RUN-006`   Numeric conversion failure
                                                  and operator-promotion
                                                  value that does not fit

  `IoError`                       `GRF-RUN-007`   Graphite I/O failure

  `InvalidSliceError`             `GRF-RUN-008`   Slice assignment length
                                                  mismatch

  `RuntimeError` only             `GRF-RUN-009`   Destructuring length
                                                  mismatch

  `RuntimeError` only             `GRF-RUN-010`   Array exceeds its limit

  `RuntimeError` only             `GRF-RUN-011`   Invalid insertion position

  `ExternalLibraryError`          `GRF-RUN-012`   Mapped foreign/Rust
                                                  library error

  `RuntimeInternalError`          `GRF-RUN-013`   Structured unexpected
                                                  runtime failure

  `RuntimeError`                  any of the      Every catchable Graphite
                                  above           runtime error

  `Error`                         any of the      Every catchable Graphite
                                  above           runtime error
  -----------------------------------------------------------------------

Catch matching is based on `RuntimeErrorKind`, never message text.
Compiler diagnostics, syntax/type errors, Cargo build failures, and
dependency-resolution failures are not catchable Graphite runtime
exceptions.

Retain Stage 11 codes `GRF-RUN-001` for array bounds and `GRF-RUN-002`
for missing map key. Allocate distinct stable `GRF-RUN` codes for the
other materially distinct runtime failures through the Stage 11
registry. Do not create conflicting registries.

### 5.2 Catch-bound error API

The catch binding is a special immutable error handle. Initial supported
methods:

-   `error.message() -> str` --- the diagnostic title, such as
    `array index out of bounds` or `missing map key`. Not the caret
    label, source excerpt, or rendered block.
-   `error.code() -> str` --- the stable code, such as `GRF-RUN-005`.
-   `error.kind() -> str` --- the canonical catch-type name, such as
    `DivisionByZeroError`. Conditions with no specific catch type use
    `RuntimeError`. Kind comes from the structured error kind, not from
    the message or the code text.
-   `error.sourceFile() -> str` --- empty if unavailable
-   `error.line() -> uint64` --- `0` if unavailable
-   `error.column() -> uint64` --- the 1-based logical source column
    used by the diagnostic header, or `0` if unavailable. A tab counts
    as one source column. Caret alignment separately expands tabs and
    wide characters for display, derived from the same span.
-   `error.causeMessage() -> str` --- the direct cause's `message()`,
    or empty if there is no cause. Older causes stay in the chain and
    are not concatenated.
-   `error.stackTrace() -> str` --- innermost Graphite call frame first,
    or empty if there are no frames. Each frame is
    `function at file:line:column`.

`message()` does not fabricate or append a location. The logical column
and the display caret are both derived from the original span; they are
not two stored columns. The error handle cannot be constructed directly,
stored, passed, or returned in Stage 12. Catch-type names are not
reserved record names. In a catch clause they always mean the built-in
catch type.

## 6. Catch ordering and scope

-   Evaluate catch clauses in source order; first match handles the
    error.
-   `catch (RuntimeError error)` and `catch (Error error)` each match
    all catchable runtime errors. Any later catch is unreachable and is
    a compile-time error. `Error` does not change `kind()`: the handle
    still reports the specific catch type, or `RuntimeError` when the
    failure has no specific type.
-   Duplicate exact catch types are rejected as unreachable.
-   Unknown catch types are semantic errors, never implicit catch-alls.
-   An error raised inside a catch body is not handled by sibling
    catches of the same try. It propagates outward after that try's
    finally executes.
-   Catch binding is scoped to its own block, immutable, and
    inaccessible outside that block.
-   Each catch clause has its own scope. Reusing a binding name in
    separate clauses is allowed if ordinary Graphite block-scope rules
    allow it.
-   `finally` does not inherit a catch binding's scope.
-   Follow existing lexical scope rules for shadowing and emit ordinary
    structured name-resolution diagnostics where applicable.

## 7. Propagation across Graphite functions

-   Graphite functions may fail at runtime without declaring errors in
    their signatures.
-   An unhandled error propagates to the caller until a matching catch
    handles it or the entry runtime boundary renders it.
-   Preserve original kind, stable code, message, failure span, cause,
    and context.
-   Add a Graphite trace frame at function boundaries where practical,
    including function name and call-site span. Preserve the failure
    span separately.
-   Avoid duplicate consecutive frames for the same call site.
-   Catching an error does not erase its source span or cause.
-   `throw` is deferred. A new failure while handling an error preserves
    the earlier error as cause/context only where the causal
    relationship is real.
-   Render an unhandled error once at the program boundary, not once per
    function.

## 8. `finally` semantics

### 8.1 Execution guarantee

`finally` executes exactly once when its try statement exits: - normal
completion; - completion of a selected catch block; - unhandled runtime
error propagation; - `return` from try/catch; - `break` or `continue`
that exits the try statement's control-flow path; - nested try
statements, with inner finally before outer finally.

If a loop is entirely inside the try block, a `continue` or `break` that
stays within that try does not run finally for that individual
iteration. It runs when the try statement itself is exited.

The guarantee covers Graphite-managed control flow and structured
runtime errors, not process aborts, forced termination, segmentation
faults, or foreign code that aborts/unwinds across an unsupported ABI
boundary.

### 8.2 Pending outcome

Lower the try statement as if it preserves one pending outcome:

``` rust
enum PendingOutcome {
    Normal,
    Return(ReturnValue),
    Break(LoopTarget),
    Continue(LoopTarget),
    Error(RuntimeError),
}
```

The generated Rust need not literally use this enum if an equivalent
lowering is proven correct. `finally` executes before the pending
outcome resumes. Preserve return values and evaluate side effects
exactly once.

### 8.3 Failure inside finally

If finally completes normally, resume the pending outcome unchanged.

If finally produces a new runtime error: - the new error becomes the
active outcome; - a pending error becomes that new error's direct cause,
without dropping either error's existing cause chain; - pending
return/break/continue is cancelled and replaced by the error, and
`causeMessage()` stays empty unless the new error already had a cause; -
normal completion becomes an error.

`causeMessage()` returns only the direct cause's `message()`. Do not
silently discard a pending error or an existing cause chain.

### 8.4 Control flow inside finally

For the initial implementation, reject `return`, `break`, and `continue`
inside a finally block with a semantic diagnostic. This avoids ambiguous
replacement semantics. Revisit only in a later language version.

## 9. Rust lowering and library boundaries

### 9.1 Explicit propagation

Do not implement exceptions by source rewriting, textual labels,
`panic!`, or `catch_unwind` as the language mechanism.

Use an internal generated result type, conceptually:

``` rust
type GrfResult<T> = Result<T, RuntimeError>;
```

Fallible Graphite functions return `GrfResult<T>` internally; a Graphite
`void` function may return `GrfResult<()>`. This is an implementation
detail and does not change source-level signatures.

-   Calls outside a try boundary propagate errors with explicit
    generated control flow (`?` or equivalent).
-   Within a try boundary, capture the error, match catch types in
    source order, execute the first matching catch, then execute
    finally.
-   Do not allow `?` to escape a try boundary before its catches can
    inspect the error.
-   Ensure return/break/continue crossing the try boundary executes
    finally.
-   A fallible finally body participates in the pending-outcome
    replacement rules.
-   Do not evaluate expressions twice during dispatch or cleanup.

Document the chosen lowering and test generated control flow. Do not
rely on Rust destructors alone to implement arbitrary Graphite control
flow.

### 9.2 Rust `Result::Err`

Supported Rust library adapters must explicitly map their error types: -
Map known failures to the closest built-in Graphite runtime kind. - Map
unknown library errors to `ExternalLibraryError`. - Preserve safe
cause/context. - Avoid leaking secrets or sensitive payloads. - Do not
assume arbitrary Rust error types can be converted uniformly without an
adapter.

### 9.3 Rust panics and foreign code

-   A Rust panic is not automatically a catchable Graphite exception.
-   Expected failures use `Result`/structured propagation.
-   No blanket conversion of all panics into Graphite exceptions.
-   An adapter may opt into `catch_unwind` only at a documented
    unwind-safe boundary and may convert the result to
    `RuntimeInternalError` or `ExternalLibraryError`; this is adapter
    policy, not default language behavior.
-   Do not unwind across unsupported FFI boundaries. Foreign aborts and
    process-level failures are not catchable.
-   Internal runtime/compiler panic handling at the process boundary
    must not fabricate a Graphite source span.

## 10. Stack traces, causes, and rendering

Trace frames represent the Graphite call path; the error's `source_span`
remains the original failure location.

-   Add frames as errors cross Graphite function calls.
-   Document frame order, such as innermost-first.
-   `stackTrace()` returns a formatted Graphite trace, possibly empty.
-   Do not infer Graphite frames from generated Rust stack traces.
-   Keep cause chains separate from call traces.
-   Full Rust backtraces are optional debug output, not part of the
    stable Graphite API.

Caught errors are not automatically printed. The catch body decides what
to do. Unhandled errors are rendered once at the generated program's
runtime boundary using Stage 11's renderer, then the program exits
non-zero. If no reliable source span exists, render without a location.

## 11. Compile-time diagnostics

Use Stage 11's structured diagnostic model and stable registry. Add
codes for exception-specific conditions, including: - try with neither
catch nor finally; - unknown/non-built-in catch type; -
duplicate/unreachable catch; - catch after finally or duplicate
finally; - invalid binding/scope use; - `return`, `break`, or `continue`
inside finally; - invalid break/continue target; - error-handle use
outside its catch scope; - unsupported `throw` before it is implemented.

Do not introduce an exception-specific diagnostic framework.

## 12. Implementation plan

1.  **Semantic contract/registry:** register built-in error kinds,
    stable codes, catch matching, scope, and finally rules.
2.  **Parser/AST:** add grammar and explicit AST nodes with spans.
3.  **Semantic analysis:** validate catch types/order, scope,
    handler/cleanup requirement, and finally control-flow restrictions.
4.  **Runtime model:** extend Stage 11 `RuntimeError` with cause/context
    and Graphite trace frames.
5.  **Result propagation:** make generated fallible functions return
    internal `GrfResult`; map supported library `Err`.
6.  **Try/catch lowering:** implement source-order matching and
    propagation at try boundaries.
7.  **Finally lowering:** implement pending-outcome semantics for
    normal/error/return/break/continue and nesting.
8.  **Runtime boundary:** render unhandled errors once through the Stage
    11 renderer.
9.  **Regression tests:** cover syntax, semantics, propagation, cleanup,
    causes/traces, and generated control flow.

## 13. Tests and acceptance criteria

### Syntax/semantic tests

-   Parse try/catch, multiple catches, try/finally, and
    try/catch/finally.
-   Reject missing catch and finally.
-   Reject unknown catch types, duplicate/unreachable catches, catch
    after finally, duplicate finally.
-   Verify binding scope, immutability, and inability to escape the
    catch block.
-   Verify `throw` and user-defined error types remain unavailable.

### Runtime catching tests

-   Catch division by zero, array bounds, missing map key, conversion
    failure, integer overflow, invalid slice, and I/O failure with their
    specific types.
-   Verify missing-key diagnostics never expose the key value.
-   Verify `RuntimeError` catches any catchable Graphite runtime error.
-   Verify first matching catch runs and later catches are skipped.
-   Verify errors in a catch body propagate outward, not to sibling
    catches.
-   Verify errors propagate through multiple Graphite function calls and
    can be caught by callers.
-   Verify Rust library `Err` mappings.
-   Verify Rust panics are not automatically caught as Graphite
    exceptions.

### Finally/control-flow tests

-   Finally runs after normal completion, handled errors, and unhandled
    errors.
-   Finally runs before return, break, and continue leave the try
    statement.
-   A loop wholly inside a try does not run finally on each internal
    iteration.
-   Nested finally executes inner-to-outer.
-   A failure in finally replaces normal completion and pending
    return/break/continue.
-   A failure in finally replaces a pending error while preserving it as
    cause/context.
-   Reject control-flow transfers inside finally.
-   Return expressions and side effects execute exactly once.

### Diagnostics/trace tests

-   Caught errors are not automatically printed.
-   Unhandled errors render once with stable code and source span when
    available.
-   Original failure span survives propagation and catch handling.
-   Cause chain and Graphite call frames remain intact.
-   Missing source spans are not fabricated.
-   Expected Graphite errors do not appear as raw Rust panics.

### Acceptance gate

Stage 12 is complete only when: 1. Grammar and AST explicitly represent
try/catch/finally. 2. Built-in runtime error kinds and stable codes are
registered. 3. Catch ordering, matching, binding scope, and propagation
are deterministic. 4. Errors propagate across Graphite calls and
supported Rust library adapters. 5. Finally works for normal completion,
errors, return, break, continue, and nesting. 6. Finally failures follow
the replacement/cause rules. 7. Rust `Result::Err` mappings are explicit
and tested. 8. Rust panics are not indiscriminately converted into
catchable exceptions. 9. Source spans, codes, causes, and Graphite call
traces are preserved. 10. Regression tests demonstrate correct control
flow and exactly-once expression evaluation.

## 14. Locked constraints

-   Implement try/catch/finally; defer throw and user-defined error
    types.
-   Built-in catch types only; `RuntimeError` matches all catchable
    Graphite runtime errors.
-   Catch order is source order; unreachable catches are compile-time
    errors.
-   Catch bindings are immutable and scoped to their catch blocks.
-   Runtime errors propagate implicitly across Graphite function calls.
-   Stage 11's diagnostic and runtime error model remains the source of
    truth.
-   Finally runs for all Graphite-managed exits, including errors and
    non-local control flow.
-   A finally failure replaces the pending outcome and preserves a prior
    error as cause/context where applicable.
-   Reject return/break/continue inside finally initially.
-   Expected failures use structured propagation, not panic.
-   Rust panics and foreign aborts are not automatically catchable.
-   Rust `Err` mapping requires explicit adapters.
-   Never guess source locations from Rust backtraces.
-   No source rewriting or panic-alias implementation.

## 15. Deferred decisions

May be refined without changing semantic guarantees: - Exact Rust
representation of internal `GrfResult<T>`. - Exact internal
representation of built-in error kinds. - Trace formatting and maximum
retained frame count. - Adapter-specific panic conversion at documented
unwind-safe boundaries. - Future throw, user-defined errors, union
catches, optional bindings, or control-flow transfers inside finally.

Any change to catch matching, finally execution, error replacement,
source-location preservation, or panic boundaries requires an explicit
semantic revision before implementation.

## 16. Locked Stage 12 matching and runtime API

1.  Every existing runtime failure maps to the catch type in section
    5.1. `RuntimeError` and `Error` each match all of them. Destructuring length, array
    limit, and invalid insertion have no specific catch type. Division
    and remainder by zero share `GRF-RUN-005` and
    `DivisionByZeroError`, with messages `division by zero` and
    `remainder by zero`. Catch matching uses the error kind. Sharing a
    code does not merge messages.
2.  `message()` is the diagnostic title. `kind()` is the catch-type
    name. `code()` is the stable code. The caret label stays in the
    rendered diagnostic.
3.  A failure in `finally` makes a pending error the new error's direct
    cause. `causeMessage()` is that cause's `message()` only. A pending
    return, break, continue, or normal completion does not become a
    cause. Cause chains are preserved.
4.  `try`, `catch`, `finally`, and `throw` are reserved keywords.
    Catch-type spellings may still be record names. In `catch` they
    always denote the built-in type. `throw` as a statement reports that
    throwing is not supported.
5.  `ExternalLibraryError` (`GRF-RUN-012`) is produced only by an
    explicit library adapter. `grf.io` stays `IoError`.
    `RuntimeInternalError` (`GRF-RUN-013`) is not an automatic panic
    conversion. Both are tested through the structured value when no
    production path raises them.
6.  `column()` and the diagnostic header use the logical source column.
    A tab is one column. Caret placement derives terminal display width
    from that same span.
