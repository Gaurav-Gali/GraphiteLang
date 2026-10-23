# Graphite Language --- Lambda Functions LLD

**Status:** Draft for implementation\
**Scope:** Anonymous functions (lambdas), callable parameters, type
checking, and Rust lowering\
**Language:** Graphite (`.grf`)\
**Compiler target:** Rust

------------------------------------------------------------------------

## 1. Purpose

Define lambda functions as anonymous callable expressions in Graphite,
using Graphite's existing function-declaration conventions rather than
JavaScript-style `=>` syntax or a new `fn` keyword.

The design reuses these established principles: - Functions have an
explicit return type and typed parameters. - A one-statement body may
omit braces. - Multi-statement bodies use braces. - Value-returning
functions use explicit `return`. - `auto T` follows Graphite's existing
target-type inference rules; `auto` is not a standalone type.

Lambdas can be passed inline, stored in variables, and invoked like
other callables.

## 2. Goals and non-goals

### Goals

1.  Provide anonymous-function syntax consistent with named Graphite
    functions.
2.  Let function parameters accept callables using function-declaration
    syntax.
3.  Preserve static typing and compile-time signature validation.
4.  Reuse existing parsing, type-checking, and Rust transpilation
    infrastructure.

### Non-goals

-   No new `fn` keyword or standalone source-level function-type
    notation.
-   No JavaScript-style `=>` syntax.
-   No new generic syntax, garbage collector, async-lambda semantics, or
    ownership model.
-   No lambda-specific implicit conversions.
-   Capture/mutation behavior must not be guessed from Rust behavior;
    see Section 9.

## 3. Lambda syntax

### 3.1 General form

``` text
<return_type> (<parameter_declarations>) <body>
```

Examples:

``` graphite
int (int x) return x**2
bln (int x) return x > 0
void (int x) io.println(x)
```

The return type and parameter types are explicit. The body follows
Graphite's existing single-statement/block rules.

### 3.2 Single-statement body

Braces may be omitted for a single statement, as in other Graphite
constructs:

``` graphite
int (int x) return x**2
```

Value-returning lambdas must use explicit `return`; the final expression
is not implicitly returned.

An unbraced body ends at the delimiter of the enclosing expression. The
body's statement does not consume that delimiter, and the source does
not use a second semicolon:

``` graphite
auto int square = int (int x) return x**2;
transform(numbers, int (int x) return x**2);
```

The parser chooses the end from the syntactic context:

-   `,` ends the lambda argument before the next argument.
-   `)` ends the lambda argument or enclosing expression.
-   `;` ends the containing statement.

A multi-statement body uses braces, which are an unambiguous boundary.

### 3.3 Block body

Multi-statement bodies use braces:

``` graphite
int (int x) {
    int d = x**2;
    return d/x**3;
}
```

Every valid control-flow path in a non-`void` lambda must return a
compatible value. A path that falls through is a compile-time error.

A `void` lambda can perform side effects without returning a value:

``` graphite
void (int x) {
    io.println(x);
    io.println(x * 2);
}
```

Whether `return;` is allowed in a `void` lambda follows Graphite's
existing return rules.

## 4. Storing and invoking lambdas

Graphite's existing target-typed `auto T` form is used; `auto` is not a
standalone type:

``` graphite
auto int square = int (int x) return x**2;

int result = square(5);
io.println(result);
```

`auto T` initialized by a lambda is a special case, not a change to the
general meaning of `auto T`. The compiler must recognize that
initializer before applying ordinary variable-initializer conversion.
Otherwise it will try to convert the lambda to `T`.

For this declaration the variable is a callable whose return type is
`T`. The parameter list is taken from the lambda:

``` text
Parameters: (int)
Return type: int
```

`int result = square(5)` is valid. `int n = square` is an error because
the callable is not an `int`. `int square = int (int x) return x**2` is
an error because a lambda cannot be converted to `int`. Reassignment
requires a compatible callable signature.

Callable fields, record fields, and returning a lambda are unsupported.
Graphite has no source syntax for those callable types.

A block-bodied stored lambda is also valid:

``` graphite
auto int calculate = int (int x) {
    int d = x**2;
    return d + 10;
};

int result = calculate(5);
```

## 5. Callable parameters reuse function declaration syntax

Do not introduce a separate source-level callable type syntax. A
function parameter may declare a callable by using a nested
function-shaped signature:

``` graphite
int[] transform(int[] arr, int op(auto int x)) {
    loop(int i = 0; i < arr.iter; i++) {
        arr[i] = op(arr[i]);
    }
    return arr;
}
```

In `int op(auto int x)`: - `op` is the parameter name. - The outer `int`
is the callable's return type. - `(auto int x)` declares the callable's
parameter. - The body can invoke `op(value)` as an ordinary function
call.

`auto int x` must follow Graphite's existing `auto` rules; it does not
change the callable's return type.

Example call:

``` graphite
int[] numbers = [1, 2, 3, 4, 5];

int[] result = transform(
    numbers,
    int (int x) return x**2
);
```

This example illustrates the intended callable syntax. Loop syntax and
array mutation must follow the existing Graphite implementation.

### 5.1 Callable compatibility

Validate at compile time:

-   Parameter count.
-   Exact declared parameter types. `int (flt x)` does not match
    `int op(auto int x)`.
-   Return type, including `void` versus a value.
-   Return-path validity inside the lambda.

`auto` on a callable parameter, as in `int op(auto int x)`, does not
change the parameter type. The signature is `(int) -> int`. When the
body calls `op(value)`, that argument uses Graphite's existing `auto`
conversion rules. Argument conversion at the call must not make
incompatible lambda signatures interchangeable.

Do not add lambda-specific conversions. A `bln`-returning lambda must
not satisfy an `int`-returning callable parameter.

## 6. Semantics

### Invocation

A lambda is invoked with normal call syntax, e.g. `square(5)`. Argument
checking and return behavior match ordinary function calls.

### Return behavior

-   The lambda's return type is explicit.
-   Non-`void` lambdas must return a compatible value on every valid
    path.
-   `return` exits the lambda, not the enclosing function.
-   A final expression in a block is not an implicit return.
-   A one-statement body may omit braces but still uses explicit
    `return` for value-returning behavior.

### Evaluation

-   Constructing a lambda does not execute its body.
-   Inline lambda expressions participate in the enclosing call's
    existing evaluation order.
-   Invocation executes the body under normal Graphite rules.

## 7. Parser and AST

### Parser requirements

Recognize lambda expressions in expression positions, including call
arguments and variable initializers. Reuse existing parsers for types,
parameters, blocks, statements, `return`, expressions, and call
arguments.

The parser must distinguish lambdas from named function declarations and
other expressions using syntactic context. It must determine where an
unbraced single-statement lambda body ends inside an argument list.

### Conceptual AST

``` text
LambdaExpr {
    return_type,
    parameters,
    body,
    inferred_callable_type,
    capture_information
}
```

The exact node and field names should follow the current compiler's
conventions.

A callable parameter may use the existing parameter node with a callable
signature type, or an equivalent nested-signature representation.

### Internal callable type

``` text
CallableType {
    parameter_types,
    return_type
}
```

This is an internal representation, not source syntax. It allows the
type checker to validate invocation and compatibility without exposing
`fn(...)` notation.

## 8. Type checking

1.  Parse the explicit return type and parameter declarations.
2.  Create a local scope for the lambda parameters.
3.  Type-check the body using normal Graphite rules.
4.  Validate each return against the declared return type.
5.  Verify return coverage on all required paths for non-`void` lambdas.
6.  Construct the callable signature.
7.  Check compatibility with a receiving callable parameter, if present.
8.  For `auto T` initialized by a lambda, store a callable whose return
    type is `T` before ordinary initializer conversion. An ordinary `T`
    variable initialized by a lambda is an error.
9.  Validate captures according to supported ownership/mutability rules.
10. Generate Rust only after Graphite-level type checking succeeds.

## 9. Captures and ownership

A lambda that refers to names outside its parameter list is a capturing
lambda. Graphite must define capture behavior rather than accidentally
inheriting it from Rust.

Recommended boundary: - Permit captures only when they can be
represented safely under Graphite's existing ownership and mutability
rules. - Do not add a garbage collector to support lambdas. - Reject
unsupported captures with a Graphite diagnostic before Rust
compilation. - Do not promise mutable captures until their
mutation/borrowing semantics are specified.

The first implementation rejects captures at compile time. A Graphite
diagnostic names the outer variable. It must not surface as a Rust
compilation error.

Allowed references are the lambda's parameters and calls to top-level
or imported functions. `math.abs(x)` and `add(x, 10)` are calls, not
captures.

An outer local, an enclosing parameter, and `this` are rejected:

``` graphite
int factor = 10;
auto int multiply = int (int x) return x * factor;
```

A reference to the lambda's own variable is also a capture and is
rejected. Recursion through that variable needs a later design:

``` graphite
auto int fact = int (int n) {
    if (n <= 1) return 1;
    return n * fact(n - 1);
};
```

## 10. Rust lowering

Rust closures are the intended target where they preserve Graphite
semantics.

### Simple lambda

Graphite:

``` graphite
auto int square = int (int x) return x**2;
```

Illustrative Rust:

``` rust
let square = |x: i64| -> i64 {
    x.pow(2)
};
```

This assumes Graphite `int` maps to Rust `i64`. The exact lowering for
`**` must follow Graphite's existing exponentiation implementation;
`pow(2)` is illustrative only.

### Block lambda

Graphite:

``` graphite
auto int calculate = int (int x) {
    int d = x**2;
    return d + 10;
};
```

Illustrative Rust:

``` rust
let calculate = |x: i64| -> i64 {
    let d: i64 = x.pow(2);
    return d + 10;
};
```

### Callable parameter

Graphite:

``` graphite
int apply(int value, int op(int x)) {
    return op(value);
}
```

Illustrative Rust shape:

``` rust
fn apply<F>(value: i64, op: F) -> i64
where
    F: Fn(i64) -> i64,
{
    op(value)
}
```

This is not a final lowering decision. The choice between `Fn`, `FnMut`,
and `FnOnce` depends on supported capture, mutation, and consumption
semantics. The compiler must preserve Graphite semantics rather than
blindly choosing a Rust trait.

## 11. Decisions and open items

### Established by this design

-   Lambda syntax: `return_type (parameters) body`.
-   Explicit return type and typed parameters.
-   Explicit `return` for value-returning lambdas.
-   Optional braces for a single statement; braces for multiple
    statements.
-   Inline lambda arguments and stored callable variables.
-   Callable parameters use nested function-declaration syntax,
    e.g. `int op(auto int x)`.
-   Callables use ordinary invocation syntax, e.g. `op(value)`.
-   No `=>`, `fn` keyword, or separate source-level function-type
    syntax.

### Locked decisions

| Area | Decision |
|---|---|
| Stored lambda | `auto int square = int (int x) return x**2;` |
| Meaning of `auto int` for lambda initialization | Callable variable with return type `int`; parameters derived from the lambda |
| Ordinary `int` variable initialized with a lambda | Compile-time error |
| Callable reassignment | Requires a compatible signature |
| Unbraced body termination | Uses enclosing delimiters; no doubled semicolons |
| Multi-statement body | Requires braces |
| Captures | Rejected in the first implementation |
| Top-level and imported function calls | Allowed; not captures |
| Recursive lambda variable reference | Rejected as a capture in the first implementation |
| Callable parameter syntax | `int op(auto int x)` |
| Invocation conversion | Uses existing `auto` rules |
| Lambda parameter compatibility | Exact declared parameter types must match |
| Callable fields and returned lambdas | Out of scope |

`void` lambdas follow the existing function return rules. A named
function may be passed where a callable of the same signature is
expected. Non-capturing lambdas lower to Rust function pointers. That
lowering is not a Graphite generic and is not source syntax.

## 12. Diagnostics

Provide Graphite-level diagnostics for: - Return type mismatch. -
Missing return on a path in a non-`void` lambda. - Incompatible callable
argument. - Wrong argument count when invoking a callable. -
Incompatible argument type when invoking a callable parameter. - Invalid
`return` use according to existing rules. - Unsupported or invalid
capture. - Unresolved target/parameter type under existing `auto`
rules. - Ambiguous or malformed lambda syntax.

Diagnostics should point to the lambda, parameter, return statement, or
call site instead of exposing raw Rust errors when the compiler can
detect the issue itself.

## 13. Test plan

### Parsing

-   Single-statement lambda.
-   Block-bodied lambda.
-   `void` lambda.
-   Lambda passed as a function argument.
-   Lambda stored in a variable.
-   Multiple parameters.
-   Nested calls around lambdas.
-   Ternary and arithmetic expressions inside lambda bodies.
-   Malformed and ambiguous lambda syntax.
-   Termination of an unbraced lambda body inside a call argument.

### Type checking

-   Matching parameter and return types.
-   Return-type mismatch.
-   Wrong callable invocation arity.
-   Incompatible callable argument.
-   Missing return path.
-   Valid `void` lambda.
-   Existing `auto T` behavior for stored callable variables.
-   Unsupported capture rejected cleanly.

### Transpilation and execution

-   Store and invoke an integer-square lambda.
-   Pass a transformation lambda to a higher-order function.
-   Pass a block-bodied lambda.
-   Invoke a callable parameter from the receiving function.
-   Transform array elements using a callable parameter.
-   Verify return values and side effects.
-   Verify that lambda `return` does not exit the enclosing function.
-   Compile and run generated Rust.

### Regression

-   Named function declarations remain unchanged.
-   Existing one-statement brace-omission behavior remains unchanged.
-   Existing `auto T` rules remain unchanged.
-   Nullable `?`, ternary `? :`, slices, and record fields remain
    unaffected.
-   Existing calls and generic resolution, if present, remain
    unaffected.

## 14. Implementation sequence

1.  Inspect the existing grammar, AST, type system, `auto T` handling,
    and Rust code generation.
2.  Add a lambda AST node using existing type and parameter
    representations.
3.  Parse single-statement lambdas with explicit `return` and
    block-bodied lambdas.
4.  Add callable signatures to the internal type system.
5.  Implement lambda-local scope and return-path validation.
6.  Parse callable parameters using nested function-declaration syntax.
7.  Type-check inline lambda arguments and stored lambda variables.
8.  Lower non-capturing lambdas to Rust closures.
9.  Add higher-order function and execution tests.
10. Implement captures only where Graphite ownership/mutation rules
    support them; otherwise emit a Graphite diagnostic.
11. Run the complete compiler regression suite.

## 15. Acceptance criteria

-   Documented lambda forms parse successfully.
-   Return types and parameters are statically checked.
-   Single-statement bodies follow Graphite's brace-omission rule.
-   Value-returning lambdas use explicit `return`.
-   Lambdas can be stored and invoked using Graphite's target-typed
    `auto T` convention.
-   Functions can accept callable parameters using existing
    function-declaration syntax.
-   Inline lambda arguments are checked against callable signatures.
-   Supported lambdas transpile to valid Rust and execute correctly.
-   Unsupported captures and invalid signatures produce Graphite
    diagnostics.
-   Existing syntax and type inference pass regression tests.

------------------------------------------------------------------------

**Implementation note:** Stored `auto T`, unbraced-body termination,
capture rejection, and callable compatibility are locked in section 11.
Recognize a lambda initializer before ordinary conversion to `T`.
