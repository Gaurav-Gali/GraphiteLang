# LLD 09 --- Typed Arrays, Indexing, Slicing, Destructuring, and Iteration

**Status:** Implementation specification\
**Target:** Graphite v0.1\
**Roadmap stage:** Stage 6\
**Prerequisites:** Lexer/parser foundation, typed AST, semantic
analysis, code generation, runtime error handling, Stage 4 `const` and
`lengthOf()` support, and Stage 5 conversion system.

## 1. Purpose

Specify homogeneous, statically typed, dynamically sized arrays with
indexing, Python-style slicing, slice assignment, declaration
destructuring, iteration, and the agreed array convenience methods.

Array syntax and behavior must be represented in Graphite's structured
AST and typed AST. Do not implement arrays by rewriting source text,
using regex substitutions, or injecting generated source fragments
before parsing.

## 2. Goals and non-goals

### Goals

-   Define array type and literal syntax.
-   Statically check element types and index expressions.
-   Support positive and negative indexing for reads and writes.
-   Support Python-style slicing with omitted bounds, negative bounds,
    and positive/negative steps.
-   Support unit-step slice assignment that can resize the array.
-   Support stepped slice assignment with exact replacement-length
    validation.
-   Support destructuring declarations and `enumerate()` loop bindings.
-   Support non-consuming `iter()` and `enumerate()`.
-   Enforce `const` on every mutation path.
-   Define `lengthOf(array)` consistently with `array.length()`.
-   Define runtime bounds and slice errors, diagnostics, and tests.

### Non-goals

-   Multidimensional arrays or NumPy-like vectorization.
-   Array comprehensions, lambdas, filtering callbacks, or user-defined
    iterator protocols.
-   Views, slices that alias the original array, or zero-copy slicing.
-   Heterogeneous arrays or `any`.
-   Fixed-size arrays.
-   Destructuring assignment to existing variables; this LLD specifies
    declaration destructuring and loop bindings only.
-   Array spread syntax. This is a separate feature decision.
-   Custom array types or user-defined generics.
-   Generalizing array slicing to strings or maps.
-   A new exception-handling language feature.

## 3. Array types and literals

### 3.1 Array type syntax

Use `T[]` to represent a dynamically sized array whose elements have
type `T`.

``` grf
int[] numbers = [10, 20, 30];
str[] names = ["Ada", "Linus"];
flt[] measurements = [1.5, 2.0];
```

The element type is the canonical static type. Aliases resolve before
type comparison; for example, `int[]` and `int64[]` have the same
element type.

Nested arrays may be accepted only if the current parser and type system
can represent them without ambiguity. If nested arrays are not supported
by the existing type representation, reject them explicitly in this
stage rather than generating malformed Rust. Do not infer support merely
because `T[]` is recursive in notation.

### 3.2 Array literals

Array literals use square brackets with comma-separated expressions:

``` grf
int[] values = [1, 2, 3];
```

Rules:

-   Every element must have the same canonical static type, except where
    an already-approved literal-specific rule determines the literal's
    type.
-   Do not insert general implicit numeric conversions between array
    elements.
-   An incompatible element produces a compile-time type error.
-   An empty literal `[]` has no element type on its own. It is valid
    only where the expected type supplies `T`, such as
    `int[] values = [];`.
-   If the language supports expected-type propagation, it may use the
    declaration or function argument's known array type to type the
    literal. Otherwise require an explicit typed context.
-   Reject an empty literal with no resolvable expected element type.
-   Evaluate each element exactly once, from left to right.
-   Array literal evaluation must preserve Graphite's existing
    expression evaluation order.

Examples:

``` grf
int[] empty = [];           // Valid: expected type is int[]
int[] values = [1, 2, 3];    // Valid
int[] invalid = [1, "two"]; // Compile-time error
```

### 3.3 Array type and conversion boundaries

Array elements must match the array's canonical element type. Do not
automatically apply Stage 5 conversions to each element unless the
source explicitly requests a conversion or uses an already-approved
target-directed conversion form. The array's declared element type does
not turn every element into an implicit conversion.

## 4. Index expressions

### 4.1 Read syntax

``` grf
int[] arr = [10, 20, 30, 40, 50];

int first = arr[0];
int last = arr[-1];
int secondLast = arr[-2];
```

`arr[index]` is an expression whose type is the array's element type
`T`.

### 4.2 Index expression type

The index must be an integer type supported by the compiler. This
includes currently supported signed and unsigned fixed-width integer
types. Boolean, floating-point, string, character, array, and other
non-integer types are compile-time errors.

Do not implicitly convert an arbitrary numeric expression into an index.
Follow the existing approved integer conversion and literal rules. The
runtime index normalization must safely handle every supported integer
width without overflow or wrapping.

### 4.3 Negative index normalization

For array length `n` and signed index `i`:

-   If `i >= 0`, resolve to `i`.
-   If `i < 0`, resolve to `n + i`.
-   The resolved index must satisfy `0 <= resolved < n`.

Unsigned indexes are nonnegative and do not use negative-index
normalization.

An index outside the valid range raises a runtime bounds error. Do not
wrap, clamp, return a default, or fabricate an element.

Examples for a five-element array:

  Expression   Result
  ------------ ----------------------
  `arr[0]`     First element
  `arr[4]`     Last element
  `arr[-1]`    Last element
  `arr[-5]`    First element
  `arr[5]`     Runtime bounds error
  `arr[-6]`    Runtime bounds error

Avoid integer overflow when converting indexes or computing `n + i`.
Implement normalization using checked arithmetic or a comparison-based
algorithm that does not overflow.

### 4.4 Index assignment

``` grf
int[] arr = [10, 20, 30];
arr[0] = 99;
arr[-1] = 88;
```

The right-hand expression must have the array's element type `T`,
subject only to the language's existing explicit/target-directed
conversion rules. The compiler must not silently convert an incompatible
value.

Index assignment evaluates the array expression, index expression, and
right-hand expression exactly once, in the language's defined evaluation
order. A failed bounds check must not partially mutate the array.

## 5. Slicing

### 5.1 Syntax

Support:

``` grf
arr[start:stop]
arr[start:stop:step]
arr[:stop]
arr[start:]
arr[:]
arr[::step]
arr[start::step]
arr[:stop:step]
```

Each component may be omitted where the grammar allows it. The parser
must distinguish index expressions from slices based on colon tokens in
the bracket expression. Colons used here must not accidentally enable
unrelated syntax elsewhere.

The slice result has type `T[]` when the source has type `T[]`.

### 5.2 Stop exclusivity and defaults

The stop boundary is exclusive.

For a positive step:

-   Default start is `0`.
-   Default stop is `n`.
-   Default step is `1`.

For a negative step:

-   Default start is `n - 1`.
-   Default stop is the sentinel before index `0` (equivalent to
    Python's omitted negative-step stop).
-   Default step is `-1`.

An explicit `-1` stop is not the same as an omitted stop when the step
is negative. Preserve the distinction between an omitted component and
an explicit integer value in the AST.

Examples for `[10, 20, 30, 40, 50]`:

  Expression      Result
  --------------- ------------------------
  `arr[:3]`       `[10, 20, 30]`
  `arr[1:4]`      `[20, 30, 40]`
  `arr[2:]`       `[30, 40, 50]`
  `arr[:]`        `[10, 20, 30, 40, 50]`
  `arr[::2]`      `[10, 30, 50]`
  `arr[::-1]`     `[50, 40, 30, 20, 10]`
  `arr[4:0:-1]`   `[50, 40, 30, 20]`
  `arr[:-1]`      `[10, 20, 30, 40]`
  `arr[-3:]`      `[30, 40, 50]`

### 5.3 Bounds normalization

Use Python-style sequence slicing normalization:

-   Positive-step bounds are normalized relative to the array length,
    negative bounds are interpreted relative to the end, and the
    normalized bounds are clamped to the permitted boundary range.
-   Negative-step bounds are normalized according to Python's
    reverse-slicing rules, including the special default for an omitted
    stop.
-   Out-of-range slice bounds are clamped rather than causing an index
    error.
-   The implementation must distinguish omitted bounds from explicitly
    supplied bounds.
-   Avoid overflow when normalizing extreme integer values.

Document the exact normalization algorithm in code comments and test it
independently from code generation. The result must match Python-style
slicing for all valid integer bounds and nonzero steps, not only the
examples in this LLD.

### 5.4 Step rules

-   A positive step traverses forward.
-   A negative step traverses backward.
-   A zero step raises a runtime slice error.
-   The step expression, if present, must have an integer type.
-   Omitted step means `1`.
-   Slice components are evaluated once, in source order, before
    normalization.
-   Runtime step errors must not mutate the source array.

### 5.5 Slice results are copies

A slice expression creates a new array containing the selected elements
in the selected order.

``` grf
int[] original = [1, 2, 3, 4];
int[] copy = original[:];

copy[0] = 99;
// original remains [1, 2, 3, 4]
```

The new array does not alias the source array's storage. Elements are
copied according to the language's existing value/ownership semantics.
Since this stage does not introduce reference types or user-defined
classes, do not invent deep-copy semantics for future reference-bearing
types.

An empty slice returns a valid empty array with the source's element
type.

## 6. Index and slice assignment

### 6.1 Index assignment

`arr[index] = value` replaces one existing element. A nonnegative
index equal to the current length appends that value, so an empty
dynamic array accepts `arr[0] = value` and then `arr[1] = value`. An
index past that position is still a bounds error; missing elements are
not invented. A bounded array appends only while the new length stays
within its maximum. Negative indexes are resolved as for reads and do
not grow the array.

### 6.2 Unit-step slice assignment

Support:

``` grf
int[] arr = [10, 20, 30, 40, 50];

arr[1:3] = [200, 300];
// arr is [10, 200, 300, 40, 50]

arr[1:2] = [7, 8, 9];
// The selected range is replaced with three elements; the array grows.
```

Unit-step replacement may change the array length. It replaces the
normalized selected range with the replacement array's elements.

The replacement expression must be an array whose element type is
compatible with `T`. In v0.1, require the same canonical element type;
do not add covariant or implicit element conversions.

The implementation must evaluate the replacement before mutating the
target so expressions such as `arr[1:3] = arr[:]` are well-defined. If
the target and replacement refer to the same array, create or retain a
safe temporary copy before modifying the target.

### 6.3 Non-unit-step slice assignment

A slice is a unit-step slice only when its effective step equals
positive `1`. All other explicit steps, including `-1`, use
stepped-slice assignment semantics and require exact replacement length.

For stepped slice assignment:

-   The replacement array must have exactly as many elements as the
    selected positions.
-   Each replacement element must have the canonical element type `T`.
-   A length mismatch raises a runtime slice-assignment error.
-   No mutation may occur when the length check fails.
-   Negative-step assignment maps replacement elements to selected
    positions in traversal order.

Examples:

``` grf
int[] arr = [1, 2, 3, 4, 5];

arr[::2] = [10, 30, 50]; // Valid
arr[::2] = [10, 30];    // Runtime slice-assignment error
arr[::-1] = [5, 4, 3, 2, 1]; // Valid: replaces positions in reverse traversal order
```

The parser/AST must preserve whether the step was omitted or explicitly
supplied. Omitted step and explicit `1` have the same effective behavior
for assignment.

### 6.4 Mutation atomicity

For index or slice assignment:

-   Evaluate all required expressions once before mutating the array.
-   Validate index bounds, slice step, replacement element type (compile
    time), and replacement length (runtime) before modifying storage.
-   A failed operation leaves the original array unchanged.
-   Do not permit partial mutation before a runtime error.

## 7. Array methods

Provide the following built-in typed methods:

  ------------------------------------------------------------------------
  Method                   Signature               Behavior
  ------------------------ ----------------------- -----------------------
  `get(index)`             `T`                     Return element at
                                                   normalized index;
                                                   bounds error if invalid

  `set(index, value)`      `void`                  Replace existing
                                                   element; bounds error
                                                   if invalid

  `push(value)`            `void`                  Append one element

  `insert(index, value)`   `void`                  Insert at a normalized
                                                   insertion position

  `remove(index)`          `void` initially        Remove element at
                                                   normalized index;
                                                   bounds error if invalid

  `length()`               `uint64`                Return number of
                                                   elements

  `iter()`                 typed iterable of `T`   Iterate over values
                                                   without consuming the
                                                   source

  `enumerate()`            typed iterable of       Iterate over zero-based
                           `(uint64, T)`           index/value pairs
  ------------------------------------------------------------------------

### 7.1 Method index behavior

`get`, `set`, and `remove` use element-index rules: valid resolved
indexes are in `[0, n)` and negative indexes resolve from the end.

`insert` uses insertion-position rules: after negative normalization,
valid positions are in `[0, n]`, where `n` appends to the end. For a
signed index `i`, negative insertion positions resolve relative to the
end using `n + i`; therefore `insert(-1, value)` inserts before the
current last element, while `insert(0, value)` inserts at the start. An
index resolving outside `[0, n]` raises a runtime bounds error.

`push` always appends and does not take an index.

All index-taking methods accept only integer types and must avoid
overflow during normalization.

### 7.2 Method return behavior

For v0.1:

-   `get(index)` returns `T`.
-   `set`, `push`, `insert`, and `remove` return `void`.
-   `length()` returns `uint64`.
-   Runtime failures terminate execution using the defined runtime error
    mechanism. Do not return fabricated defaults.
-   Do not add a method return type or optional/result wrapper that the
    current language does not support.

If the current compiler's method-call representation cannot express
built-in array methods directly, add structured typed-AST operations
rather than rewriting method-call source text.

## 8. `lengthOf()` integration

`lengthOf(array)` returns `uint64` equal to the array's element count.

``` grf
int[] arr = [10, 20, 30];

uint64 a = arr.length();
uint64 b = lengthOf(arr);
// a == b
```

Requirements:

-   Extend the existing `lengthOf` built-in's accepted argument types to
    include `T[]`.
-   Preserve the existing string behavior from Stage 4.
-   Reject unsupported argument types at compile time.
-   Array `lengthOf()` is read-only and works on `const` arrays.
-   `lengthOf(arr)` and `arr.length()` must always return identical
    values.
-   The global built-in must use compiler-recognized typed semantics; it
    must not depend on importing an array library.

## 9. Destructuring declarations

### 9.1 Syntax and exact-count rule

Support:

``` grf
int a, int b = [1, 2];
```

This is a declaration that binds two values from one array-valued
initializer. It is not a comma operator and does not create two
independent initializer expressions.

The initializer must have an array type. The number of declared bindings
must exactly match the runtime number of elements. A mismatch raises a
defined runtime destructuring error before any binding is introduced or
initialized.

### 9.2 Type checking and binding

-   Evaluate the initializer expression exactly once.
-   Each binding's declared type must match the array element type under
    existing assignment rules.
-   Do not silently convert array elements to declared binding types.
-   Reject duplicate binding names within the same destructuring
    declaration.
-   Reject conflicts with existing names according to the current
    declaration and scope rules.
-   Create bindings in the current lexical scope only after validation
    succeeds.
-   If the array length is wrong, no partially initialized destructuring
    bindings may become visible.
-   Define behavior for `const` destructuring: each declared binding's
    own `const` modifier, if the current grammar supports it, applies
    independently. Do not add new modifier syntax in this LLD.
-   If the parser does not currently support mixed declaration modifiers
    in a declaration list, define the accepted grammar explicitly and
    reject unsupported forms.

### 9.3 Runtime behavior

If a declaration expects two elements but the source contains one or
three, raise a runtime destructuring error that includes the expected
and actual counts where possible. Do not silently ignore extra elements,
fill missing elements, or return defaults.

This exact-count runtime rule applies to destructuring an array value.
If the compiler can prove the array literal's length statically, it may
report the mismatch at compile time as an additional diagnostic, but
must preserve correct semantics for dynamically sized arrays.

## 10. Iteration and `enumerate()`

### 10.1 Value iteration

``` grf
loop (int value = arr.iter()) {
    io.println(value);
}
```

`iter()` yields each element as `T`, in ascending array index order.

### 10.2 Enumeration

``` grf
loop (uint64 idx, int value = arr.enumerate()) {
    io.println(f"{idx} - {value}");
}
```

The index type is `uint64`, because array indexes/counts and `length()`
use `uint64`. Do not silently convert the `uint64` enumeration index to
`int64`. If the current compiler requires explicit type matches, the
`uint64` declaration is mandatory.

### 10.3 Scope and control flow

-   Evaluate the iterable expression once before iteration.
-   Loop variables are scoped to the loop body and are not visible
    afterward.
-   Loop variables are initialized for each iteration and cannot collide
    with one another.
-   `break` exits the loop; `continue` advances to the next
    element/pair.
-   Empty arrays execute the body zero times.
-   Iteration proceeds in ascending order for both `iter()` and
    `enumerate()`.
-   Iteration does not consume or mutate the source array.
-   A loop must not silently clone the full array just to iterate unless
    the existing Rust lowering requires a documented copy. Prefer
    read-only borrowing.
-   The current language has no concurrency or user-defined callbacks in
    scope here. If mutation of the source array during iteration can
    occur through another binding, define and enforce borrow/alias rules
    rather than relying on accidental Rust behavior.

### 10.4 Ownership and borrowing

For the initial implementation, lower array storage to a representation
compatible with Graphite's value and mutation semantics, such as Rust
`Vec<T>`, where appropriate.

-   Reading an element copies it when `T` is a copyable primitive;
    follow the language's existing value semantics.
-   Slicing creates a new array copy.
-   `iter()` and `enumerate()` borrow the array immutably for the loop's
    duration.
-   Mutation of the iterated array during an active iteration must be
    rejected by the borrow/alias strategy or otherwise prevented from
    invalidating iteration. Do not expose unsafe iterators or
    invalidated references.
-   Do not introduce general user-visible references, ownership
    annotations, or a borrow checker in this stage. Implement only the
    guarantees necessary for array operations and loops.
-   If the current compiler's storage model cannot enforce these rules
    safely, document the concrete restriction and reject the problematic
    construct rather than silently producing undefined behavior.

## 11. `const` and mutation enforcement

A `const T[]` binding is read-only through that binding.

Reads are allowed:

``` grf
const int[] arr = [10, 20, 30];
int last = arr[-1];
int[] copy = arr[:];
uint64 n = lengthOf(arr);
```

All of the following must fail at compile time when `arr` is `const`:

``` grf
arr[0] = 99;
arr[-1] = 88;
arr[1:2] = [10];
arr[::2] = [1, 2];
arr.set(0, 100);
arr.push(60);
arr.insert(1, 15);
arr.remove(1);
```

The semantic checker must apply one consistent mutability check to every
operation that writes to array storage. Do not enforce `const` only in
the assignment operator while leaving method calls or slice assignment
as bypasses.

For this stage, `const` prevents mutation through the constant array
binding. The compiler's current value/alias model must not allow an
alias to bypass the guarantee. If aliasing is not yet expressible,
record that limitation and preserve the guarantee as future reference
types are introduced.

## 12. AST and typed-AST requirements

Represent array features structurally.

The untyped AST must distinguish, at minimum:

-   Array type syntax.
-   Array literals.
-   Index expressions.
-   Slice expressions with optional start/stop/step components.
-   Index assignment.
-   Slice assignment.
-   Destructuring declarations.
-   `iter()` and `enumerate()` loop sources, or a structured general
    iterable expression that can distinguish their semantics.
-   Array built-in method calls if the existing AST does not already
    represent method calls.

The typed AST must retain:

-   Canonical element type.
-   Source span for each expression and operation.
-   Resolved index/start/stop/step types.
-   Whether each slice component was omitted or explicitly provided.
-   The resolved operation for reads, writes, slicing, and method calls.
-   Result types for array expressions and methods.
-   Mutability information for the target binding.
-   Destructuring binding types and expected count.
-   Iteration element type and enumeration index type.
-   Runtime validation requirements, including bounds, zero step, and
    length mismatch.

The exact node names may follow existing repository conventions. These
semantic distinctions must remain explicit even if code generation
lowers several nodes through shared runtime helpers.

## 13. Lexer and parser requirements

-   Recognize `[` and `]` for type suffixes, array literals, indexing,
    and slicing.
-   Parse `:` as a slice separator only in the relevant
    bracket-expression grammar and as a record/map separator where
    already approved by other LLDs.
-   Recognize omitted slice components, including `[:]`, `[::]`,
    `[::step]`, and `[start::step]`.
-   Parse unary negative integer expressions in indexes and slice
    components.
-   Distinguish a negative index expression from a slice bound without
    confusing `arr[-1]` with `arr[-1:]`.
-   Parse `...` only where the relevant map-spread grammar permits it;
    this LLD does not add array spread.
-   Parse declaration destructuring without misinterpreting normal
    comma-separated declarations. Follow the existing declaration
    grammar and define unambiguous parsing rules.
-   Produce source-located diagnostics for malformed brackets,
    extra/missing colons, invalid declaration lists, and unsupported
    syntax.

## 14. Runtime errors and diagnostics

Runtime errors must use the compiler/runtime's shared failure mechanism
and terminate execution with a nonzero exit status, unless the project
already has a documented equivalent contract.

Required runtime error categories:

-   Array index out of bounds.
-   Invalid insertion position.
-   Slice step equals zero.
-   Slice assignment replacement length mismatch.
-   Destructuring source length mismatch.
-   Any checked arithmetic/normalization failure that cannot be safely
    represented.

Diagnostics should identify the source file and line/column where
available, the operation, and useful context such as the invalid index,
array length, expected destructuring count, actual count, or
expected/actual replacement length.

Compile-time diagnostics are required for:

-   Non-integer index or slice component.
-   Wrong element type in index/slice assignment or collection methods.
-   Unsupported array literal typing.
-   Empty array literal without an expected element type.
-   Invalid `const` mutation.
-   Duplicate destructuring bindings.
-   Invalid loop variable types or binding counts.
-   Unsupported nested array/type forms.
-   Invalid method names or argument counts.

Do not silently clamp ordinary indexing; clamping applies only to
slice-bound normalization.

## 15. Testing plan

### 15.1 Types and literals

-   Homogeneous arrays for every currently supported element type.
-   Alias equivalence.
-   Empty arrays with and without expected type.
-   Incompatible element types.
-   Nested array behavior according to the explicitly supported scope.
-   Literal range rules.

### 15.2 Indexing and mutation

-   First/last/middle positive indexes.
-   `-1`, `-2`, and `-n`.
-   Empty-array indexing.
-   Index exactly equal to length.
-   Index below `-n`.
-   Minimum/maximum supported integer index values.
-   All supported integer index types.
-   Rejection of bool, float, string, and character indexes.
-   Positive and negative index assignment.
-   Assignment type mismatch.
-   Mutation atomicity on bounds failure.

### 15.3 Slicing

-   All examples in Section 5.
-   Empty arrays and empty slices.
-   Omitted start, stop, and step in every grammar-supported
    combination.
-   Positive and negative steps.
-   Explicit and omitted negative-step stop distinction.
-   Zero-step runtime failure.
-   Extreme positive and negative bounds and steps.
-   Bounds normalization compared against a Python reference test
    oracle, where suitable for the test harness.
-   Source-located parser errors for malformed slices.
-   Verify slices are independent copies.

### 15.4 Slice assignment

-   Unit-step replacement with equal, shorter, and longer replacement
    arrays.
-   Empty-range insertion and full-range replacement.
-   Self-assignment such as `arr[:] = arr[:]`.
-   Positive stepped assignment.
-   Negative-step assignment.
-   Exact replacement length and mismatch failure.
-   Element type mismatch.
-   Ensure failed operations leave the source array unchanged.
-   Verify `const` rejects every assignment form.

### 15.5 Methods and `lengthOf`

-   `get`, `set`, `push`, `insert`, `remove`, `length`, `iter`, and
    `enumerate`.
-   Positive and negative indexes for `get`, `set`, and `remove`.
-   Insertion positions `0`, `n`, `-1`, and invalid extremes.
-   Return types and argument validation.
-   `lengthOf(arr) == arr.length()` for empty and nonempty arrays.
-   `lengthOf(const_array)` is allowed and read-only.
-   Mutation methods reject `const` receivers.

### 15.6 Destructuring

-   Exact element count.
-   Too few and too many elements.
-   Statically provable mismatch.
-   Initializer evaluated exactly once.
-   Correct element types.
-   Duplicate binding names.
-   Scope visibility and shadowing rules.
-   No partially initialized bindings on failure.
-   Const binding behavior where supported.

### 15.7 Iteration

-   Empty, singleton, and multi-element arrays.
-   Correct value order.
-   Zero-based enumeration.
-   `uint64` index type.
-   `break` and `continue`.
-   Loop variable scope.
-   Iterable expression evaluated once.
-   Source array is not consumed.
-   No unsafe mutation during iteration.
-   `const` source iteration.
-   Regression tests for existing loops.

### 15.8 Regression and quality

-   Preserve the current feature program as a regression baseline.
-   Verify no source-text rewriting or regex-based array lowering
    exists.
-   Run formatting, workspace checks, and the full test suite.
-   Verify invalid programs fail before Rust code generation.
-   Verify runtime failures produce nonzero exit status and useful
    diagnostics.

## 16. Acceptance criteria

Stage 6 is complete only when:

1.  Arrays have canonical static element types and homogeneous literal
    validation.
2.  Index reads/writes support safe positive and negative indexing.
3.  Slicing matches Python-style normalization for all tested bounds and
    nonzero steps.
4.  Slice results are independent copies.
5.  Unit-step slice assignment can resize arrays.
6.  Stepped slice assignment requires exact replacement length and is
    atomic on failure.
7.  All listed methods are typed, documented, and tested.
8.  `lengthOf(array)` and `array.length()` agree and return `uint64`.
9.  Declaration destructuring validates types and exact element count.
10. `iter()` and `enumerate()` are non-consuming and have defined
    scope/ownership behavior.
11. `const` prevents every mutation route through the constant array
    binding.
12. AST and typed AST represent the feature semantics explicitly.
13. Compile-time and runtime failures have documented diagnostics.
14. The implementation does not introduce `any`, source rewriting, or
    accidental support for deferred features.
15. Existing regression tests, formatting, workspace checks, and the
    full test suite pass.

## 17. Implementation order

1.  Audit current lexer/parser support for brackets, colons, negative
    expressions, declaration lists, and loop sources.
2.  Add canonical array types and array literal typing.
3.  Add typed index expressions and index assignment.
4.  Implement checked index normalization and runtime bounds errors.
5.  Add slice AST nodes preserving omitted components.
6.  Implement Python-compatible slice normalization and copy-producing
    slice reads.
7.  Implement atomic index and slice assignments, including resizing and
    stepped-length checks.
8.  Add typed array methods and integrate `lengthOf(array)`.
9.  Implement declaration destructuring with exact-count validation.
10. Implement typed `iter()` and `enumerate()` loops, using `uint64`
    indexes.
11. Apply consistent `const` enforcement across every mutation route.
12. Add diagnostics, boundary tests, regression tests, and full
    validation.

## 18. Locked decisions

1.  Arrays are homogeneous, statically typed, and dynamically sized.
2.  Array type syntax is `T[]`; array literals use `[ ... ]`.
3.  Indexing supports positive and negative integer indexes.
4.  Negative index `i` resolves relative to the array end; invalid
    indexes raise runtime bounds errors.
5.  Slice syntax is `arr[start:stop:step]`, with omitted components and
    Python-style bounds normalization.
6.  Slice stops are exclusive; negative steps traverse backward; zero
    step is a runtime error.
7.  Slice expressions return independent array copies.
8.  Unit-step slice assignment may resize the array.
9.  Non-unit-step slice assignment requires an exact replacement count.
10. `get`, `set`, `push`, `insert`, `remove`, and `length` are typed
    array methods.
11. `lengthOf(array)` returns `uint64` and agrees with `array.length()`.
12. Declaration destructuring requires an exact number of values and
    evaluates its initializer once.
13. `iter()` yields values; `enumerate()` yields zero-based
    `(uint64, T)` pairs.
14. Iteration does not consume or mutate the source array.
15. `const` rejects all writes through the constant array binding,
    including element writes, slice writes, and mutating methods.
16. All array features use structured AST/typed-AST representations, not
    source-text rewriting.
17. `any`, lambdas, array spread, views, user-defined iterators, and
    general user-defined generics are not introduced by this LLD.

## 19. Stage 6 decisions

These decisions resolve the open points in this specification.

1.  Nested arrays of any representable depth are supported. `int[][]`
    is an array of `int[]` and is distinct from `int[]`. Empty nested
    literals are valid only when the expected type supplies every
    missing element type. Indexing an outer array yields its element
    type. Slicing it yields an array of that element type.
2.  Arrays are values. Assignment, argument passing, return, and
    slicing copy the array, including nested arrays. A `const` binding
    cannot be mutated through another binding, because array values do
    not share mutable storage. `iter()` and `enumerate()` borrow the
    source for the loop and do not consume it. Writes through the
    binding being iterated are compile-time errors.
3.  `iter()` and `enumerate()` are valid only as loop-header sources.
    `enumerate()` index bindings are `uint64`. The index is not
    converted to `int64`. A use outside a loop header is a compile-time
    error.
4.  Destructuring accepts two or more bindings. Each binding is
    `const`, a type, and a name, in that order, with `const` optional
    and applying only to that binding. One initializer follows the
    list. `auto`, a trailing comma, and destructuring assignment to
    existing variables are rejected. `int a = [1]` remains an ordinary
    declaration.
5.  A trailing comma in an array literal is a syntax error. `[]` remains
    valid when the element type is known. Function-call commas are
    unchanged.
6.  Array `==` and `!=` are rejected. `str(array)`, `auto str`, and
    interpolation share one display: `[elem, elem]`, with elements
    formatted by the same `str(element)` policy. `str` and `chr`
    elements are quoted.
    `void` is not an array element type. A unit-step slice assignment
    may replace its range with an empty array and delete that range.
    The empty array takes its element type from the target. A stepped
    assignment accepts an empty replacement only when the selected
    slice is empty.
7.  `int[]` and `int[3]` are the same array type: an array of `int`.
    The `[3]` is that array's maximum length, not a separate type.
    `int[]` has no maximum. `int[3]` must stay at most 3 elements.
    The size is a non-negative integer literal, and each dimension has
    its own bound (`int[2][1]` is a maximum of one array, each holding
    at most two ints). A shorter array, including `[]`, is valid. A
    literal longer than its bound is a compile-time error. Storing a
    longer array into a bounded array, and `push`, `insert`, or a
    unit-step slice assignment that would pass the bound, fail at
    runtime and leave the array unchanged. `typeOf` reports both as
    `int64[]`.
