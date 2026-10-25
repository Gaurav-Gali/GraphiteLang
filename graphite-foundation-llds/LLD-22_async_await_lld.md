# Graphite Lang --- LLD: Async/Await

**ID:** LLD-ASYNC-01  
**Status:** Design decisions locked for V1 implementation  
**Target:** Graphite compiler/transpiler to Rust

## 1. Purpose and goals

Define Graphite's async/await feature using explicit generic-like syntax
consistent with `iter<T>`. `async<T>` represents an asynchronous
operation that eventually produces one value of type `T`; `await<T>`
waits for that operation and binds its result. Rust futures and `.await`
provide the implementation machinery.

Goals:

- Explicitly show the eventual result type.
- Type-check async operations in Graphite before Rust generation.
- Support async functions awaiting other async functions.
- Keep V1 small and compatible with the existing type system and
  transpiler.

Out of scope for V1: `spawn` as a user API, detached/background tasks,
cancellation APIs, user-visible channels, async closures,
user-selectable executors, automatic async propagation, nullable
`async<T>?`, async class/interface methods, expression-position await,
and synchronous functions whose return type is `async<T>`.

**In scope and supported:** `async<T>` is composable with existing types,
including `async<iter<T>>` and `async<int[]>` (and other collections).
Await yields a normal `iter<T>` / array that is iterated synchronously.
Also in scope: `asyncIter<T>` (async item streams) with
`loop(await<T> name : stream)` — distinct from `async<iter<T>>`.

## 2. Source syntax

### Async declarations

```grf
async<int> fetchCount() {
    return 42;
}

async<str> fetchName() {
    return "Graphite";
}

async<map<str, int>> fetchData() {
    return {"a": 1, "b": 2};
}

async<void> saveData() {
    return;
}
```

The type parameter is the eventual result type, not the operation
object's type. The syntax `async<T> f(...)` always declares an
**asynchronous function**. It does not declare a synchronous function
that returns an `async<T>` value.

### Await and bind

```grf
await<int> count = fetchCount();
await<map<str, int>> data = fetchData();
await saveData();
```

V1 rules:

- Binding form always declares the result type: `await<T> name = expr;`.
- `await expression;` without a binding is allowed for `async<void>`
  only.
- Await is **statement-only**. Expression-position await is deferred.

### Async nesting and generics

```grf
async<int> calculateTwice() {
    await<int> count = fetchCount();
    return count * 2;
}

async<T> identity<T>(T value) {
    return value;
}

async<int> useIdentity() {
    await<int> result = identity<int>(42);
    return result;
}
```

Follow Graphite's existing generic-call policy: type arguments remain
explicit; do not add general type inference for async calls in this
feature.

### Composing `async` with iterators and collections

`async<T>` wraps any valid result type `T`. Fetching an iterator or
array asynchronously is ordinary composition — not a new iterator kind:

```grf
async<iter<int>> getNumbers() {
    return range(10);
}

async<int[]> fetchNumbers() {
    return [10, 20, 30];
}

async<void> main() {
    await<iter<int>> numbers = getNumbers();
    loop(int n : numbers) {
        // synchronous iteration over the awaited iter
    }

    await<int[]> values = fetchNumbers();
    loop(int n : values) {}
}
```

Rules:

- `async<iter<int>>` means one async operation that produces an `iter<int>`.
- After `await`, the binding has type `iter<int>` (or `int[]`, etc.) and
  uses the normal `loop` rules.
- Graphite array syntax is `int[]`, not `array<int>`.
- An async function may `await` other work (for example `time.sleepMs`)
  before `return`ing an iterator; that is still `async<iter<T>>`, not an
  async stream.

### `asyncIter<T>` (async item streams)

`asyncIter<T>` is a pull stream of `T` where each next item may suspend.
It is **not** `async<iter<T>>` (one async value that is a sync iterator).

```grf
asyncIter<int> getNumbers() {
    int i = 0;
    loop (i < 5) {
        await time.sleep(time.secs(1));
        emit i;
        i = i + 1;
    }
}

async<void> main() {
    loop (await<int> n : getNumbers()) {
        io.println(n);
    }
}
```

Rules:

- An `asyncIter<T>` function may use both `emit` and `await`.
- `emit` produces the next item; the consumer’s
  `loop (await<T> name : stream)` awaits until that item is ready.
- Manual protocol (same names as sync `iter<T>`), only inside
  `async` / `asyncIter` functions:

```grf
asyncIter<str> it = stream().iter();
loop (it.hasNext()) {
    io.print(it.next());   // or it.value() to peek without consuming
}
```

  Graphite types these as `bln` / `T`; codegen inserts `.await` so each
  step can suspend.
- `loop (await<T> …)` is only allowed inside `async` / `asyncIter`
  functions.
- `emit` is not allowed in plain `async<T>` functions.
- Lowering: producer runs as a Tokio task writing to a bounded channel
  (backpressure); consumers use `has_next`/`next`/`value`/`try_next`.

### Reserved keywords

`async` and `await` are reserved keywords in V1.

## 3. Type model

Add a compiler type constructor `Async<T>`, written in source as
`async<T>`.

- `async<int>` means an operation eventually producing `int`.
- `async<void>` means an operation completing without a value.
- `async<map<str, int>>` means an operation eventually producing that
  map.
- Async may wrap any otherwise-valid type argument except that
  `async<T>?` is **rejected in V1**.

Keep these types distinct:

- Function's eventual/body return type: `T`.
- Type returned when the function is called: `async<T>`.

Do not model the function body as returning `async<async<T>>`.

An async function declaration `async<T> f(...)` has call result type
`async<T>`, while `return expression;` inside its body is checked
against `T`.

`auto T` remains Graphite's target-type conversion feature, not type
inference. Await bindings do not implicitly convert `async<U>` into
`T`; in V1 the declared await type must match the operation's result
type exactly after normal alias/type-equivalence resolution.

## 4. Type and context rules

For `await<T> name = expression;`:

1. `expression` must have type `async<U>`.
2. `T` must match `U` under the compiler's existing type-equivalence
   rules.
3. The statement must occur inside an async function.
4. `name` follows normal local-variable scope and declaration rules.
5. `expression` is consumed (moved); see ownership rules below.

For `await expression;`, require `async<void>` in V1.

Async function returns:

- `return expression;` must return a value compatible with declared
  `T`.
- `return;` is valid only for `async<void>`.
- A non-void async function must not complete without returning a
  value, consistent with existing Graphite function rules.

Await is legal only inside async functions in V1. Awaiting a plain
value, awaiting in a synchronous function, or using a mismatched result
type is a Graphite compile-time error. Calling an async function from
synchronous code may create an `async<T>` value (for example as a local
or parameter), but synchronous code cannot await it.

### Generators and async (mutually exclusive)

Reject both combinations in V1:

- An async function must not contain `emit`.
- A generator must not contain `await`.

### Blocking synchronous I/O

Existing synchronous functions may be called from async functions.
Blocking calls remain blocking. V1 does not convert synchronous I/O into
async I/O.

## 5. Async operation values and ownership

### Where `async<T>` values are allowed in V1

Supported:

- Locals: `async<int> pending = fetchCount();`
- Parameters: `async<int> consume(async<int> pending) { ... }`
- Async call results used as expressions of type `async<T>`

Deferred:

- Synchronous functions whose return type is `async<T>` (ambiguous with
  the `async<T> f()` declaration syntax)

### Move-only single consumption

An async operation is a **single-consumption, move-only** value.

```grf
async<void> main() {
    async<int> a = fetchCount();
    async<int> b = a;       // moves a → b
    await<int> x = a;       // compile-time error: use after move
    await<int> y = b;       // valid
}
```

```grf
async<int> pending = fetchCount();
await<int> result = consume(pending);
// await<int> again = pending;  // compile-time error: use after move
```

Rules:

1. Assignment and parameter passing of `async<T>` **move** the
   operation; the source becomes unusable.
2. An operation may be awaited successfully at most once.
3. A second await of the same local is a compile-time error when
   detectable.
4. Do not implicitly clone or recreate an async operation.
5. Aliasing must not bypass the single-consumption guarantee.
6. This is a **compile-time ownership rule**, not a runtime check.
7. Lower `async<T>` values to Rust futures consumed by `.await`.

### Lazy operation creation

Calling an async function creates a pending future-like operation. Its
body is driven when awaited or otherwise polled by the runtime. Merely
calling it does not spawn a background task.

```grf
async<void> caller() {
    async<int> pending = fetchCount();
    await<int> count = pending;
}
```

`async<T>` must be accepted as a normal type in local declarations and
parameter types. An operation dropped without being awaited is not
automatically executed. Emit a **warning** for discarded async results
when the compiler can reliably detect this.

### Sequential awaits

```grf
async<void> run() {
    await<int> a = fetchA();
    await<int> b = fetchB();
}
```

These awaits execute sequentially. V1 does not imply concurrency. There
is no implicit spawn, parallel execution, or detached-task behavior.

## 6. Entry point and executor

### Locked: Tokio (minimal required features)

- Add the required Tokio dependency/features to the generated Rust
  project/template.
- Keep runtime bootstrapping in the generated entry point or runtime
  shim.
- Initialize the runtime exactly once for async programs.
- Do not expose Tokio names in Graphite syntax.
- Preserve the current synchronous entry point path: sync-only programs
  must not initialize Tokio unnecessarily.

`#[tokio::main]` is acceptable when it fits the generated entry-point
structure; an equivalent explicit Tokio runtime bootstrap is also
acceptable if behavior is preserved.

### Async `main` (included in V1)

```grf
async<void> main() {
    await saveData();
}
```

The generated Rust entry point must drive the async main future to
completion. Failures use the existing CLI `RuntimeError` reporting path
and exit with status code `1`.

## 7. Rust lowering and error model

### Locked error model

Preserve Graphite's existing `Result<T, RuntimeError>` representation.
No new Graphite error type in V1.

| Graphite | Rust lowering |
| --- | --- |
| `async<T> f(...)` | `async fn f(...) -> Result<T, RuntimeError>` |
| `await<T> x = expr;` | `let x: T = expr.await?;` |
| `await expr;` | `expr.await?;` |
| Successful return | `Ok(value)` |
| Successful `void` return | `Ok(())` |
| Failure | Propagate `RuntimeError` through `Result` |

Example:

```grf
async<int> fetchCount() {
    return 42;
}

async<int> calculate() {
    await<int> count = fetchCount();
    return count * 2;
}
```

Conceptual Rust:

```rust
async fn fetch_count() -> Result<i64, RuntimeError> {
    Ok(42)
}

async fn calculate() -> Result<i64, RuntimeError> {
    let count: i64 = fetch_count().await?;
    Ok(count * 2)
}
```

Important: the transpiler must consistently handle `Result` propagation.
It must not blindly add `.await?` if the surrounding generated function
cannot propagate that error.

Do not build a custom async state machine: Rust's compiler generates it.
Do not insert executor initialization into each async function.

Use the existing name mangling, import resolution, `grf.json` mappings,
source mapping, and error-reporting pipeline.

## 8. Compiler changes

### Lexer

- Reserve tokens for `async` and `await`.
- Add lexer tests for both keywords.

### Parser and AST

Represent:

- Async function declaration/result type.
- Await-with-binding statement.
- Await-only statement.
- `async<T>` as a type in locals and parameters.

Support nested types such as `async<map<str, int>>`. Distinguish
function declaration syntax from variables of type `async<T>`. Parse
`await<T> name = expression;` without confusing its type delimiters with
comparison operators.

### Symbol table and type checker

Track whether a function is async, its eventual result type `T`, and the
invocation type `async<T>`. Check returns, await context, awaitable
expression types, result-type equality, void rules, generic async calls,
move/use-after-move of async values, emit/await mutual exclusion, and
rejection of `async<T>?`. Report Graphite diagnostics before Rust
generation. Emit discarded-operation warnings where detectable.

### Transpiler/runtime

- Emit async functions as Rust `async fn … -> Result<T, RuntimeError>`.
- Emit await as `.await?` inside async contexts that can propagate
  `RuntimeError`.
- Generate Tokio bootstrap once for async entry points.
- Keep synchronous programs working without Tokio initialization.
- Ensure `grf run main.grf` builds and runs async programs through the
  existing CLI flow.

## 9. Diagnostics

Suggested diagnostics (adapt wording to existing Graphite conventions):

| Condition | Diagnostic |
| --- | --- |
| Await outside async function | `await can only be used inside an async function` |
| Awaiting a plain value | `await requires an async<T> operation` |
| Result mismatch | `await result type 'U' does not match declared type 'T'` |
| Missing non-void result | `async function returning 'T' must return a value` |
| Value returned from `async<void>` | `async<void> function cannot return a value` |
| Non-void await discarded | `await without a binding is only allowed for async<void> in this version` |
| Use after move | `use of moved async operation 'name'` |
| Second await (detectable) | `async operation 'name' has already been awaited` |
| `async<T>?` | `async operations cannot be nullable in this version` |
| `emit` in async / `await` in generator | Clear mutual-exclusion diagnostic |
| Async result discarded | Warning that the async operation is created but never awaited, where detectable |
| Async main / Tokio failure | Clear diagnostic naming the configured target/runtime |

## 10. Test plan

### Lexer/parser

- Parse `async<int>`, `async<void>`, and nested `async<map<str, int>>`.
- Parse await binding and await-only forms.
- Parse generic async functions.
- Parse `async<T>` locals and parameters.
- Reject missing type delimiters, binding name, or expression.
- Reject `async` / `await` as ordinary identifiers.

### Type checker

- Accept matching result types.
- Reject awaiting non-async values and mismatched types.
- Reject await in a synchronous function.
- Validate `async<void>` with `return;`.
- Reject value return from `async<void>`.
- Reject bare return in `async<int>`.
- Reject wrong return type.
- Cover nested result types and explicit generic arguments.
- Ensure `async<T>` is not implicitly assignable to `T`.
- Enforce move-only ownership and use-after-move.
- Reject `async<T>?`, `emit` in async, and `await` in generators.
- Warn on discarded async operations where detectable.

### Integration/transpiler

- Async function returning a literal.
- One async function awaiting another.
- Sequential awaits preserve source order.
- `async<void>` and map return.
- Generic async function with explicit type arguments.
- Async main initializes Tokio once; sync programs do not.
- Parameter-passing moves the operation.
- CLI exposes compile/runtime errors clearly.
- Full compiler regression suite passes, including generics, nullable
  types, maps, imports, `grf.json`, and `iter<T>`/generators.

## 11. Acceptance criteria

1. The syntax in this LLD parses.
2. Invalid await contexts, non-async operands, type mismatches, and
   use-after-move fail with Graphite diagnostics.
3. Async functions lower to valid Rust async functions returning
   `Result<T, RuntimeError>`.
4. Await bindings lower to `.await?` and bind the expected type.
5. Async functions can await other async functions.
6. `async<void>` works; bare `await` is void-only.
7. Async `main` runs under Tokio; sync entry points stay unchanged.
8. `grf run main.grf` supports the implemented async path.
9. Tests cover valid execution, type/context/ownership errors, and
   regressions.
10. `asyncIter<T>` with `loop(await<T> …)` and emit+await in the
    producer is supported; user-level spawn/cancellation APIs stay out
    of scope.

## 12. Locked V1 decisions

| # | Decision | Choice |
| --- | --- | --- |
| 1 | Executor | Tokio, minimal required features |
| 2 | Async `main` | Included in V1 |
| 3 | Bare `await` on non-void | Reject |
| 4 | Discarded async warning | Yes, when detectable |
| 5 | Async class/interface methods | Deferred |
| 6 | Error model | Preserve `Result<T, RuntimeError>` |
| 7 | Await same operation twice | Compile-time error when detectable; move-only |
| 8 | `async<T>` as a value | Locals + parameters + async call results; sync returns of `async<T>` deferred |
| 9 | Nullable `async<T>?` | Reject in V1 |
| 10 | Generators and async | Sync `iter` generators reject `await`; `asyncIter` allows emit+await; plain `async` rejects `emit` |
| 11 | Blocking sync I/O in async | Allowed (remains blocking) |
| 12 | Await expression position | Statement-only |
| 13 | Untyped await | Only `await expr;` for `async<void>` |
| 14 | Keywords | Reserve `async` and `await` |

## 13. End-to-end example

```grf
import grf.io;

async<int> fetchCount() {
    return 21;
}

async<map<str, int>> fetchData() {
    return {"a": 1, "b": 2};
}

async<void> main() {
    await<int> count = fetchCount();
    await<map<str, int>> data = fetchData();

    io.println(count);
    io.println(data);
}
```

This assumes `io.println` supports the values and async `main` is
enabled by the Tokio runtime configuration.

## 14. Summary

The V1 design uses `async<T>` for a single eventual result,
`await<T> name = operation;` to await and bind that result, and
`await operation;` for `async<void>`. Calls create pending move-only
operations rather than spawning tasks. Rust futures provide state
machines, `.await?` provides suspension with existing error
propagation, and Tokio drives async `main`. Item streams use
`asyncIter<T>` with `loop(await<T> name : stream)` so each next item
can suspend independently of `async<iter<T>>`. User-level spawn,
cancellation, concurrency combinators, and nullable async operations
remain separate future features.
