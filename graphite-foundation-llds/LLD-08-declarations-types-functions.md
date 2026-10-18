# LLD-08 — Stage 4: Declarations, Types, and Functions

**Project:** Graphite Lang  
**Stage:** 4  
**Status:** Implementation specification  
**Primary components:** Parser/AST, symbol table, semantic analyzer, typed AST  
**Implementation language:** Rust

---

## 1. Objective

Implement the semantic foundation required for typed Graphite programs.

Stage 4 must support:

- Variable declarations and assignments.
- Constant declarations and immutability validation.
- Lexical scopes and symbol resolution.
- Primitive types and canonical type normalization.
- Top-level function declarations and calls.
- Function parameters and argument validation.
- Return-type checking.
- Forward function references.
- Explicit `auto T` conversions.
- Required entry-point validation.
- Source-spanned semantic diagnostics.
- Typed AST generation for subsequent compiler stages.

The implementation must extend the existing lexer, parser, AST, and end-to-end execution pipeline rather than replacing them with a new architecture.

### Outcome

A valid Graphite program can declare typed values and constants, define functions, call functions declared later in the file, and pass semantic analysis before Rust code generation.

Invalid programs must fail during semantic analysis with useful Graphite source diagnostics rather than surfacing as generated Rust compilation errors.

## 2. Architectural placement

The compiler pipeline remains:

```text
.grf source
    |
    v
Lexer
    |
    v
Parser
    |
    v
Parsed AST
    |
    v
Declaration Collection
    |
    v
Type Normalization
    |
    v
Semantic Analysis
    |
    v
Typed AST
    |
    v
Rust Code Generation
    |
    v
Cargo / Native Execution
```

Stage 4 introduces or completes declaration collection, symbol resolution, type checking, and typed-AST construction.

### Responsibilities

**Parser**

- Recognizes declaration and function syntax.
- Constructs AST nodes.
- Preserves source locations.
- Does not determine whether identifiers exist or whether types are compatible.

**Declaration collector**

- Registers top-level function signatures before checking function bodies.
- Detects duplicate declarations.
- Enables forward function references.

**Semantic analyzer**

- Maintains lexical scopes.
- Resolves identifiers and function calls.
- Checks assignments, operators, arguments, and returns.
- Enforces mutability and entry-point rules.
- Inserts explicit conversion nodes into the typed AST.

**Rust code generator**

- Consumes validated typed AST.
- Generates Rust deterministically.
- Does not independently infer Graphite types or repair semantic errors.

## 3. Language syntax

Implement the following declaration and function forms, extending the existing parser without removing currently supported constructs.

### 3.1 Variable declarations

```grf
int age = 21;
str city = "Pune";
bln ready = true;

int64 count = 100;
flt temperature = 36.5;
```

Rules:

1. A declaration must specify a type.
2. A declaration must provide an initializer.
3. The initializer must be compatible with the declared type.
4. A variable is mutable by default.
5. The initializer is evaluated and checked before the declared name becomes available in its own scope.
6. Redeclaring a name in the same scope is an error.

Declarations without initializers are invalid in Stage 4:

```grf
int age; // Error: initializer required
```

Do not introduce implicit type inference for ordinary declarations.

### 3.2 Constant declarations

Use the following syntax:

```grf
const int MAX_RETRIES = 3;
const str APP_NAME = "Graphite";
const bln ENABLE_LOGGING = true;
```

Rules:

1. `const` makes the binding immutable.
2. A constant requires an explicit type and initializer.
3. The initializer must be compatible with the declared type.
4. Reassignment and increment/decrement of a constant are errors.
5. Constants may be declared in function bodies, nested blocks, and a `loop` initializer. They are not top-level declarations. The top-level environment remains functions and imports.
6. A `const` in a `loop` initializer stays immutable. Changing it in the loop update is a semantic error.
7. Constants are not required to have compile-time constant expressions in this stage; the initializer may be a valid runtime expression.
8. Do not introduce global variables, global constants, or constant folding.
9. A constant may be read, used in an expression, passed to a function, and copied into a new variable. That new binding is independent of the constant.

Examples:

```grf
const int limit = 10;
limit = 20; // Error: cannot assign to constant

const int size = 5;
size++; // Error: cannot modify constant
```

The parser must distinguish the `const` modifier from a type name. The semantic analyzer enforces immutability.

### 3.2.1 `lengthOf`

`lengthOf` is a reserved built-in. A program does not import it, and a call always uses the built-in.

```grf
uint64 n = lengthOf("Graphite"); // 8
uint64 unicodeLength = lengthOf("é"); // 1
```

`lengthOf(str)` returns `uint64` and counts Unicode scalar values. Numeric arguments and `bln` are semantic errors. A variable, parameter, or function named `lengthOf` is a semantic error. Array, map, and record lengths are later stages.

### 3.3 Function declarations

```grf
int add(int left, int right) {
    return left + right;
}

str greet(auto str name) {
    return f"Hello {name}";
}

void logMessage(auto str message) {
    io.println(message);
    return;
}
```

Function syntax:

```text
return_type function_name(parameter_list) block
```

Rules:

- Functions are declared at the top level in Stage 4.
- Function names are case-sensitive.
- Parameters require explicit types.
- Parameters are immutable or mutable according to the existing parameter-binding semantics; for this stage, treat parameters as mutable local bindings unless the implementation already specifies otherwise.
- Function overloading is not supported.
- A function cannot be declared twice under the same name.
- Function bodies are checked against the declared return type.
- Functions may call other functions declared later in the same file.

Do not introduce implicit return-type inference.

### 3.4 Entry point

A valid executable program must contain exactly one:

```grf
void main() {
    io.println("Hello, Graphite!");
}
```

Reject:

```grf
int main() { return 0; }
void main(int argc) {}
str main() { return "hello"; }
```

The following conditions must be diagnosed before code generation:

- Missing `main`.
- Duplicate `main`.
- Non-`void` `main`.
- Parameterized `main`.

Apply this requirement to executable entry files. If the compiler has a separate library-compilation mode, that mode must not require `main` unless explicitly specified by its contract.

## 4. Primitive type system

### 4.1 Supported types

The semantic analyzer must recognize the following primitive types.

| Graphite type | Canonical semantic type | Rust representation |
|---|---|---|
| `int` | `Int64` | `i64` |
| `int64` | `Int64` | `i64` |
| `int32` | `Int32` | `i32` |
| `int16` | `Int16` | `i16` |
| `uint` | `UInt64` | `u64` |
| `uint64` | `UInt64` | `u64` |
| `uint32` | `UInt32` | `u32` |
| `uint16` | `UInt16` | `u16` |
| `flt` | `Float64` | `f64` |
| `flt64` | `Float64` | `f64` |
| `flt32` | `Float32` | `f32` |
| `bln` | `Bool` | `bool` |
| `chr` | `Char` | `char` |
| `str` | `String` | `String` |
| `void` | `Void` | No value / Rust unit |

`void` is valid only where a return type or other explicitly supported no-value type is expected. It is invalid for variables, constants, and parameters.

`chr` maps to the canonical character type and Rust `char`. Stage 4 does not add character literal syntax. Because no expression can produce a `chr` value, a `chr` declaration, parameter, or return type fails semantic analysis. Do not invent a default character or an integer-to-character conversion.

### 4.2 Canonical type normalization

Before assignment, argument, operator, and return checks, resolve source-level type names into canonical semantic types.

At minimum:

```text
int     -> Int64
int64   -> Int64

uint    -> UInt64
uint64  -> UInt64

flt     -> Float64
flt64   -> Float64
```

These pairs are equivalent:

```grf
int age = 21;
int64 sameAge = age;
sameAge = age;
```

```grf
uint items = 5;
uint64 total = items;
```

```grf
flt temperature = 36.5;
flt64 reading = temperature;
```

No conversion is required between aliases that normalize to the same canonical type.

Do not implement alias compatibility through repeated string comparisons throughout the compiler. Create a single canonical type representation and use it consistently in semantic analysis and typed AST nodes.

Preserve source spelling and source spans where needed for diagnostics and tooling.

### 4.3 Type representation

Prefer an internal representation similar to:

```rust
enum CanonicalType {
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Bool,
    Char,
    String,
    Void,
}
```

This is a conceptual contract; adapt names to the existing Rust codebase rather than duplicating an existing equivalent type system.

The canonical type should be stored in semantic symbols and typed expressions. Source-level type syntax should remain available separately if required.

### 4.4 No implicit numeric widening

Canonical aliases are equivalent, but different numeric types are not automatically interchangeable.

```grf
int32 width = 10;
int64 height = width; // Error: incompatible types
```

```grf
flt value = 3.5;
int count = value; // Error: incompatible types
```

Do not silently introduce widening, narrowing, truncation, wrapping, or precision-loss conversions.

If explicit numeric casts are not already specified by the language, leave them unsupported and report a semantic error.

### 4.5 Literal typing

Use deterministic literal typing:

- Integer literals: `Int64` by default, subject to representability checks.
- Floating-point literals: `Float64` by default, subject to representability checks.
- Boolean literals: `Bool`.
- String literals and string interpolation expressions: `String`.
- `void` does not represent an expression value.

A numeric literal may initialize a narrower numeric type of the same family when the value is representable in that type. This is a literal-specific initialization rule. It is not implicit conversion of a general expression.

```grf
int32 width = 10;
int16 depth = 8;
uint16 seeds = 3;
flt32 ratio = 1.5;
```

These fail:

```grf
int32 width = 9999999999;
int16 depth = 40000;
uint16 seeds = -1;
flt32 ratio = 1e100;
```

The check covers signedness, the integer range, and floating-point representability. An `flt32` initializer must be exactly representable as `f32`; a value that would round or become non-finite is rejected. An integer literal does not initialize a floating-point type, and a floating-point literal does not initialize an integer type.

A non-literal expression keeps its canonical type. `int32` and `int64` remain incompatible, as do `flt32` and `flt64`.

Integer and floating-point literal parsing must detect values outside the representable range. Never silently wrap, truncate, or accept an out-of-range literal.

## 5. Symbol tables and lexical scopes

### 5.1 Symbol categories

The semantic analyzer must distinguish at least:

- Variables.
- Constants.
- Function parameters.
- Functions.
- Imported library bindings, using the existing import-resolution mechanism.

Each value symbol must record:

- Name.
- Canonical type.
- Mutability.
- Declaration location.
- Scope identity.
- Any additional metadata required by the existing typed AST.

Each function symbol must record:

- Name.
- Canonical return type.
- Ordered parameter types.
- Parameter metadata, including whether a parameter accepts an explicit `auto` conversion.
- Declaration location.

### 5.2 Scope model

Create a global declaration environment for top-level functions and imports, and lexical scopes for executable blocks and function bodies.

```grf
void main() {
    int outer = 1;

    {
        int inner = outer + 1;
        io.println(inner);
    }

    io.println(outer);
}
```

The inner variable must be visible inside its scope but not after the block ends.

Rules:

1. A symbol is visible from its declaration point until the end of its scope.
2. A variable cannot be referenced before its declaration.
3. A variable or constant cannot be redeclared in the same scope.
4. An inner scope may shadow a value from an outer scope.
5. Shadowing must not mutate the shadowed binding.
6. Leaving a scope removes its local symbols from subsequent name resolution.
7. Function parameters and the outermost function-body declarations share the same scope for duplicate-name checking.

If the parser represents the function body as a block, avoid accidentally introducing two nested scopes for the parameters and the first body block.

### 5.3 Name resolution

For value identifiers:

1. Search the current lexical scope.
2. Search parent lexical scopes in order.
3. Resolve applicable imported bindings according to the existing import rules.
4. Report an undeclared identifier if no valid binding exists.

Value bindings and function declarations use separate namespaces. A bare name resolves to the nearest value binding. A call `name(...)` resolves through the function registry, or the imported-function registry, and validates that signature. A local value named `add` does not hide the function `add` in call position, and the value is not callable.

```grf
int add(int left, int right) {
    return left + right;
}

void main() {
    int add = 1;
    io.println(add);       // value: 1
    io.println(add(2, 3)); // function: 5
}
```

Do not resolve a bare value identifier as a function. Duplicate function names remain errors. Function overloading is unsupported.

### 5.4 Duplicate declarations

Reject:

```grf
void main() {
    int value = 1;
    int value = 2;
}
```

Allow:

```grf
void main() {
    int value = 1;

    {
        int value = 2;
        io.println(value);
    }

    io.println(value);
}
```

Reject duplicate top-level function names:

```grf
int calculate() { return 1; }
int calculate() { return 2; }
```

Diagnostics must identify the duplicate declaration and, where practical, point to the earlier declaration.

## 6. Declaration collection and forward references

The compiler must collect all top-level function signatures before analyzing any function body.

Use a multi-pass approach.

### Pass 1: Register declarations

For every top-level function:

1. Normalize its return type.
2. Normalize every parameter type.
3. Record parameter conversion metadata.
4. Register the function signature.
5. Detect duplicate names and invalid declarations.

Do not analyze function bodies in this pass.

### Pass 2: Analyze bodies

For each function:

1. Create the function scope.
2. Register parameters.
3. Analyze declarations and statements in source order.
4. Resolve function calls against the completed registry.
5. Validate expressions, assignments, operators, and returns.
6. Construct typed AST nodes.

This must support forward references:

```grf
void main() {
    io.println(add(20, 22));
}

int add(int left, int right) {
    return left + right;
}
```

The program must not fail because `add` appears after `main`.

A call to an undeclared function must still fail:

```grf
void main() {
    unknownFunction();
}
```

Do not use repeated source scans or generated Rust compilation to resolve forward references.

## 7. Assignment and mutability rules

### 7.1 Initialization

The initializer must be compatible with the declaration's canonical type.

```grf
int age = 21;
str city = "Pune";
bln ready = true;
```

Invalid:

```grf
int age = "twenty-one";
```

The semantic analyzer must reject incompatible initializers before code generation.

### 7.2 Assignment

Assignment is allowed only to mutable value bindings.

```grf
int score = 10;
score = 20;
```

Reject incompatible assignments:

```grf
int score = 10;
score = "high";
```

Reject assignments to constants:

```grf
const int score = 10;
score = 20;
```

For chained assignments already supported by the parser, validate every assignment target and value independently according to the existing AST semantics. Do not silently change evaluation order or result typing.

### 7.3 Increment and decrement

Existing `++` and `--` syntax must be checked semantically.

Requirements:

- The operand must be an assignable mutable integer variable.
- Constants cannot be incremented or decremented.
- Floating-point, string, boolean, and other unsupported operands must be rejected.
- The operation must follow the existing prefix/postfix semantics.
- The generated Rust must preserve the resulting expression value and evaluation order.

Do not permit a merely readable expression to act as an assignment target.

## 8. Function calls and parameter checking

### 8.1 Exact function matching

For a normal parameter, the argument's canonical type must match the parameter's canonical type.

```grf
int add(int left, int right) {
    return left + right;
}

void main() {
    int result = add(10, 20);
}
```

Reject incorrect argument counts:

```grf
add(10);
add(10, 20, 30);
```

Reject incompatible arguments:

```grf
add("10", 20);
```

Validate arguments in their original source order and preserve the existing evaluation semantics.

### 8.2 Nested function calls

Support nested calls:

```grf
int square(int value) {
    return value * value;
}

int add(int left, int right) {
    return left + right;
}

void main() {
    io.println(add(square(3), square(4)));
}
```

The type of each nested call is determined from its resolved function signature.

### 8.3 Return type of calls

A function call expression has the canonical return type of the resolved function.

- Non-`void` calls produce a value of the declared return type.
- `void` calls produce no usable value.
- A `void` call cannot be used as an initializer, assignment value, arithmetic operand, or non-`void` function argument.

Example:

```grf
void notify() {
    io.println("Hello");
}

void main() {
    notify();
    int result = notify(); // Error: void expression has no value
}
```

## 9. Explicit `auto T` conversion

### 9.1 Meaning

`auto T` is a target-directed conversion modifier. It is not dynamic typing, type inference, or permission to convert arbitrary types.

The target type `T` remains explicit.

Examples:

```grf
auto str fromInt = 123;
auto str fromBool = false;
auto str fromText = "Graphite";
```

Each declaration must produce an explicitly typed conversion node in the typed AST whenever a conversion is required.

### 9.2 Supported conversion registry

For this stage, define the following approved conversions to `str`:

| Source type | Target type | Behavior |
|---|---|---|
| `Int16`, `Int32`, `Int64` | `String` | Decimal integer formatting |
| `UInt16`, `UInt32`, `UInt64` | `String` | Decimal integer formatting |
| `Float32`, `Float64` | `String` | Rust-compatible floating-point display formatting |
| `Bool` | `String` | `true` or `false` |
| `String` | `String` | Identity; no conversion node required |

Floating-point formatting uses Rust `Display` (`to_string`) through one shared runtime helper. `auto str` and string interpolation (`f"..."`) both use that helper, so they cannot diverge. The textual representation is the runtime's Rust `Display` result. Stage 4 does not promise a fixed number of decimal places. `NaN`, positive infinity, and negative infinity use that same Rust formatting if they arise at runtime; they are not rejected and they do not get a separate Graphite spelling.

No other conversions are implicitly approved by `auto`. In particular, do not invent numeric narrowing, string parsing, boolean parsing, or arbitrary user-defined conversions.

If the source/target pair is unsupported, report a semantic error.

### 9.3 Variable and constant initialization

Syntax:

```grf
auto str fromInt = 123;
auto str fromBool = false;
const auto str label = 42;
```

The parser must represent the `auto` modifier separately from the declared target type.

The semantic analyzer must:

1. Determine the source expression's canonical type.
2. Determine the declared target type.
3. Check the approved conversion registry.
4. Insert an explicit conversion node if required.
5. Assign the target canonical type to the resulting expression.
6. Store the target type in the declaration symbol.

A normal declaration without `auto` must not receive an implicit conversion.

### 9.4 Function parameters

Syntax:

```grf
str greet(auto str message) {
    return f"Hello {message}";
}

void main() {
    io.println(greet(123));
    io.println(greet("Ada"));
}
```

For a parameter declared `auto T`, the caller must apply the approved conversion before the function receives the argument.

The typed call expression must contain the conversion node at the argument position, not a runtime dynamic type check inside the callee.

For an ordinary parameter:

```grf
str greet(str message) {
    return message;
}
```

This must be rejected:

```grf
greet(123);
```

### 9.5 Typed-AST representation

Introduce an explicit conversion expression in the typed AST, conceptually:

```rust
TypedExprKind::Convert {
    target_type: CanonicalType,
    conversion: ConversionKind,
    value: Box<TypedExpr>,
}
```

The exact names may follow the existing AST design.

Requirements:

- Preserve the original expression and source span.
- Record the canonical target type.
- Identify the selected conversion strategy.
- Ensure code generation handles the node deterministically.
- Never implement `auto` by rewriting source text or adding special cases for individual functions.

A shared conversion registry should drive both initializer conversions and call-site conversions.

## 10. Return checking

### 10.1 Non-void functions

A function with a non-`void` return type must return a compatible value.

```grf
int add(int left, int right) {
    return left + right;
}
```

Reject:

```grf
int calculate() {
    return "invalid";
}
```

A bare `return;` is invalid in a non-`void` function.

### 10.2 Void functions

A `void` function may return without a value.

```grf
void logMessage(str message) {
    io.println(message);
    return;
}
```

Reject:

```grf
void calculate() {
    return 42;
}
```

### 10.3 Return-path validation

Implement control-flow-aware return validation to the extent supported by the existing AST.

At minimum:

- A direct valid return satisfies a non-`void` function when it is the function's reachable terminal path.
- An `if`/`else` statement satisfies return coverage only when both branches satisfy it.
- An `if` without `else` does not guarantee that every path returns.
- A loop must not be assumed to execute.
- An unconditional infinite loop may count as non-fallthrough only if the existing control-flow analysis can prove it safely.

Examples:

```grf
int getValue() {
    if (true) {
        return 1;
    } else {
        return 2;
    }
}
```

This is valid.

```grf
int getValue() {
    if (true) {
        return 1;
    }
}
```

This must be rejected because a path can fall through without returning a value.

Do not attempt to prove arbitrary runtime conditions at compile time.

## 11. Operator type checking

Semantic analysis must validate operators using canonical types.

Minimum requirements:

- `+`, `-`, `*`, and `/` accept two operands of the same canonical integer type or the same canonical floating-point type.
- `%` accepts two operands of the same canonical integer type. It does not accept floating-point operands.
- `<`, `<=`, `>`, `>=`, `==`, and `!=` accept two floating-point operands of the same canonical type. Runtime comparisons follow Rust: ordered comparisons with `NaN` are false, `NaN` equality is false, and `NaN` inequality is true.
- Equality and inequality must also support the existing comparable types, including strings where already implemented.
- Floating-point arithmetic uses Rust IEEE-754 behavior. Division by zero may produce infinity or `NaN` and is not a Graphite runtime diagnostic. Non-finite results are not trapped.
- Integer overflow stays on the existing checked path and is unchanged by the floating-point rules.
- `&&` and `||` remain rejected until short-circuit semantics are frozen. The lexer and parser may recognize them; semantic analysis reports a diagnostic and does not generate Rust for a program that uses them.
- Logical negation requires `Bool`.
- Conditional expressions such as `if` and `loop` conditions require `Bool`.
- Assignment and increment/decrement require valid mutable l-values. A constant is not mutable.
- `void` expressions cannot be used as ordinary values.

Do not silently convert operands to make an expression type-check.

For mixed numeric operands, report an incompatible-types diagnostic unless a conversion rule has already been explicitly defined.

Preserve existing operator semantics and precedence; Stage 4 must not redesign the grammar.

## 12. Typed AST contract

After successful semantic analysis, the typed AST must contain enough information for Rust code generation without repeating type resolution.

For each expression, preserve:

- Expression kind.
- Canonical result type, where the expression produces a value.
- Source span.
- Resolved symbol/function identity where applicable.
- Explicit conversion nodes where applicable.
- L-value and mutability information where needed.

For declarations, preserve:

- Declared name.
- Canonical declared type.
- Mutability.
- Typed initializer.
- Source location.

For functions, preserve:

- Function identity and name.
- Canonical return type.
- Ordered, typed parameter list.
- Parameter conversion metadata.
- Typed body.
- Return-path validation result or equivalent analysis information.

Do not replace the parsed AST with a string-based intermediate representation. Prefer explicit Rust enums and structs consistent with the existing compiler.

## 13. Diagnostics

All semantic errors must include a useful message and source location.

Required diagnostic categories include:

- Unknown type.
- Invalid use of `void`.
- Missing initializer.
- Duplicate declaration.
- Undeclared identifier.
- Undeclared function.
- Duplicate function declaration.
- Incorrect argument count.
- Incompatible argument type.
- Incompatible initializer.
- Incompatible assignment.
- Assignment to constant.
- Invalid increment/decrement target.
- Invalid operator operands.
- Invalid return value.
- Missing return on a possible function exit.
- Unsupported `auto` conversion.
- Invalid `main` declaration.
- Missing or duplicate `main`.

Example:

```text
error[E4xxx]: incompatible argument type
  --> examples/main.grf:8:20
   |
 8 |     add("10", 20);
   |         ^^^^ expected Int64, found String
```

The exact diagnostic code scheme should follow the existing project conventions. Do not create a second incompatible error-reporting framework.

Where practical, report the original source-level type spelling alongside the canonical type, especially when that helps explain a conversion or mismatch.

Collect independent semantic diagnostics where safe, but avoid cascading errors caused by unresolved or invalid nodes. Do not generate executable Rust when fatal semantic errors remain.

## 14. Testing requirements

Tests must cover lexer/parser integration, declaration collection, symbol resolution, canonical typing, conversion insertion, diagnostics, and generated-code compatibility.

### 14.1 Valid programs

- Variable declarations with valid initializers.
- Constant declarations.
- Mutable variable reassignment.
- Nested scopes and outer-variable access.
- Legal inner-scope shadowing.
- Functions with zero and multiple parameters.
- Function calls with matching argument types.
- Forward function references.
- Nested function calls.
- Valid `void` returns.
- Valid non-`void` returns.
- Both-branch return coverage.
- Equivalent canonical aliases.
- All approved `auto str` conversions.
- Call-site conversion for `auto` parameters.
- Exactly one valid `void main()`.

### 14.2 Invalid programs

- Variable without an initializer.
- Constant without an initializer.
- Duplicate variable in the same scope.
- Duplicate constant in the same scope.
- Assignment to a constant.
- Increment/decrement of a constant.
- Undeclared variable.
- Undeclared function.
- Duplicate function.
- Invalid type name.
- `void` variable or parameter.
- Missing function argument.
- Extra function argument.
- Incompatible argument type.
- Incompatible initializer.
- Incompatible assignment.
- Invalid operator operands.
- Unsupported automatic conversion.
- Return value incompatible with the function type.
- Missing return path in a non-`void` function.
- `void` function returning a value.
- Non-`void` function returning no value.
- Missing, duplicate, parameterized, or non-`void` `main`.
- Integer literals outside the representable range.

### 14.3 Canonical alias tests

The following must succeed:

```grf
int age = 21;
int64 sameAge = age;
sameAge = age;
```

```grf
uint items = 5;
uint64 total = items;
```

```grf
flt temperature = 36.5;
flt64 reading = temperature;
```

The following must fail unless an explicit supported conversion is introduced:

```grf
int32 width = 10;
int64 height = width;
```

### 14.4 Typed-AST tests

Verify that:

- Each successfully resolved expression has the expected canonical type.
- Function calls refer to the correct registered signature.
- Forward references resolve successfully.
- `auto` conversions appear at the initializer or call argument.
- Identity conversions do not introduce unnecessary conversion nodes.
- Failed semantic checks do not produce a valid executable typed AST.

### 14.5 Regression tests

Run the existing feature example:

```bash
cargo run -q -p grf -- run examples/features/main.grf
```

Preserve the existing working behavior for arithmetic, comparisons, scopes, functions, loops, string interpolation, I/O, and prefix/postfix increment/decrement.

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

Adapt package names or example paths only when the actual repository layout requires it.

## 15. Implementation constraints

1. Inspect and extend the current AST and semantic-analysis code before adding new abstractions.
2. Reuse existing lexer tokens and parser structures wherever possible.
3. Do not implement type checking through source-text replacements or regular-expression scans.
4. Do not use Rust compiler errors as a substitute for Graphite semantic diagnostics.
5. Do not hardcode special cases for individual user-defined functions.
6. Do not introduce implicit numeric conversions of general expressions. A representable numeric literal may initialize a narrower type of the same family.
7. Do not introduce arbitrary `auto` conversions.
8. Do not add unrelated syntax or language features.
9. Keep canonical type normalization centralized.
10. Keep semantic analysis independent from Rust code generation.
11. Preserve source spans through all semantic and typed-AST transformations.
12. Ensure invalid programs fail before native code generation or execution.

## 16. Acceptance criteria

Stage 4 is complete when:

- [ ] Variable declarations and initializers are validated.
- [ ] Constant declarations and immutability are enforced.
- [ ] Lexical scopes, visibility, and shadowing work correctly.
- [ ] Primitive aliases normalize to canonical types.
- [ ] Assignment and operator checks use canonical types.
- [ ] Function signatures are collected before body analysis.
- [ ] Forward function references work.
- [ ] Function arguments and return types are checked.
- [ ] Non-`void` return-path validation works for supported control flow.
- [ ] Approved `auto T` conversions are represented explicitly in the typed AST.
- [ ] Unsupported conversions produce semantic diagnostics.
- [ ] `main` validation is enforced for executable entry files.
- [ ] Diagnostics include source locations.
- [ ] Invalid programs do not reach code generation as valid typed programs.
- [ ] Existing Graphite feature examples continue to compile and execute.
- [ ] Formatting, workspace checks, and tests pass.

## 17. Out of scope

Unless already implemented and required by the existing language specification, Stage 4 does not introduce:

- Generic functions or generic types.
- Function overloading.
- Type inference for ordinary declarations.
- User-defined classes, structs, or enums.
- Implicit numeric widening or narrowing of general expressions. Literal initialization of a narrower type of the same family is specified above.
- Arbitrary implicit conversions.
- User-defined conversion operators.
- Exception handling or a general error-propagation system.
- Compile-time evaluation of arbitrary constant expressions.
- New import syntax or manifest schema changes.
- Changes to Rust code-generation architecture beyond consuming the validated typed AST.

**Definition of done:** Graphite's semantic analyzer reliably transforms parsed declarations and expressions into a validated typed AST, with canonical types, resolved symbols, correct function signatures, and explicit conversion nodes ready for deterministic Rust code generation.