# LLD 02 — General Graphite Syntax

**Status:** Working specification; individual segments must be frozen before implementation  
**Target:** Graphite v0.1  
**Source extension:** `.grf`

This document describes the proposed source syntax and its intended meaning. Examples are normative only where a rule is stated as frozen. Items marked **Open decision** must be settled before the corresponding feature is considered complete.

## 1. Source-file conventions

- Graphite source files use `.grf`.
- Source is case-sensitive.
- Whitespace separates tokens where needed but is otherwise not significant outside string literals.
- Simple statements end with `;`.
- Blocks use `{` and `}`.
- The lexer preserves source positions for diagnostics.
- The initial language uses ASCII identifiers unless Unicode identifiers are explicitly added later.
- A source file may contain imports, declarations, and the executable entry point as permitted by the module/program rules.



## 2. Comments



### 2.1 Single-line comments

```grf
// This is a comment.
int count = 3; // A comment may follow a statement.
```

The lexer ignores comment contents for parsing while preserving source positions. A `//` comment ends before the line-ending character(s), or at end of file.

### 2.2 Block comments

```grf
/*
   This is a block comment.
   It can span multiple lines.
*/
int count = 3;
```

**Proposed v0.1 rule:** block comments are not nested. The first `*/` after the opening `/`* closes the comment. An unterminated block comment is a lexical error.

Comment markers inside string literals are ordinary string characters, not comment delimiters.

## 3. Naming policy

Graphite identifiers are case-sensitive and follow a strict camel-casing policy.


| Category                        | Policy                                   | Examples                                  |
| ------------------------------- | ---------------------------------------- | ----------------------------------------- |
| Domain and module path segments | lower camel case                         | `grf.io`, `grf.fileIo`, `coolDev.netHttp` |
| Functions and methods           | lower camel case                         | `println`, `readFile`, `getUserName`      |
| Variables and parameters        | lower camel case                         | `userName`, `filePath`                    |
| Types and classes               | UpperCamelCase                           | `Person`, `NeuralNetwork`                 |
| Generic type parameters         | UpperCamelCase, enforced by the compiler | `T`, `Element`                            |
| Constants                       | Open decision; proposed lower camel case | `maxRetries`, `defaultTimeout`            |
| Reserved keyword `auto`         | Not an identifier                        | `auto int`, `auto str`                    |


Acronyms use ordinary camel casing: `httpClient`, `urlParser`, `fileIo`; not `HTTPClient`, `URLParser`, or `fileIO` under the proposed policy.

The language specification must decide whether casing violations are compiler errors, linter diagnostics, or warnings. The compiler MUST NOT silently change identifier spelling.

`auto` is a reserved keyword. It cannot be declared or referenced as a variable, parameter, function, domain, or library name.

## 4. Program entry point

v0.1 executable shape:

```grf
import grf.io;

int square(int value) {
    return value * value;
}

void main() {
    io.println(square(5));
}
```

A v0.1 executable has:

- one entry `.grf` file passed to the compiler or CLI;
- zero or more `import` statements at the beginning of that file, before any declaration;
- top-level function declarations;
- exactly one `void main()` with no parameters.

Libraries are not executable programs and do not require `main()`. v0.1 does not accept classes, constructors, local source modules, or other unsupported declarations in the entry file. The entry file may import libraries whose implementations live in separate Rust crates described by `grf.json`.

Forward calls are supported. A top-level function may call another top-level function declared later in the same file. Semantic analysis collects top-level declarations first, then checks function bodies. Call order does not have to match declaration order.

```grf
import grf.io;

void main() {
    io.println(square(5));
}

int square(int value) {
    return value * value;
}
```

Missing `main`, more than one `main`, and any `main` signature other than `void main()` are semantic errors.

**Open decision:** command-line arguments and return-based process exit codes are not part of this initial signature unless added explicitly.

## 5. Primitive types

`int`, `uint`, and `flt` are aliases of a canonical Graphite type. They are not separate nominal types that happen to share a Rust representation.

| Graphite spelling | Canonical Graphite type | Rust representation |
| ----------------- | ----------------------- | ------------------- |
| `int`, `int64`    | `int64`                 | `i64`               |
| `uint`, `uint64`  | `uint64`                | `u64`               |
| `flt`, `flt64`    | `flt64`                 | `f64`               |
| `int16`           | `int16`                 | `i16`               |
| `int32`           | `int32`                 | `i32`               |
| `uint16`          | `uint16`                | `u16`               |
| `uint32`          | `uint32`                | `u32`               |
| `flt32`           | `flt32`                 | `f32`               |
| `bln`             | `bln`                   | `bool`              |
| `chr`             | `chr`                   | `char`              |
| `str`             | `str`                   | `String`            |
| `void`            | `void`                  | no explicit return value / Rust unit semantics |

The semantic analyzer normalizes aliases to the canonical type before checking assignments, arguments, operators, and returns. Diagnostics may keep the spelling written in source.

```grf
int64 source = 42;
int destination = source;
```

This assignment is valid with no conversion. `flt` and `flt64` are the same type. `uint` and `uint64` are the same type.

`int32` and `int64` remain distinct types. A conversion between different widths is never implied by the alias rule. No adaptive-width numeric types are used. `flt16` is not part of the frozen initial type set. The string type spelling is `str`.

Numeric conversions, mixed-width operations, overflow, division by zero, and cast syntax require explicit rules before arithmetic semantics are frozen.

**Open decision:** the integer overflow policy is not frozen. Increment, decrement, and other integer arithmetic must not silently wrap. Until a policy is specified, the compiler and runtime must not introduce unchecked wrapping behavior.

## 6. Variable declarations

```grf
int age = 21;
flt64 height = 1.75;
bln isActive = true;
str userName = "Alex";
```

A declaration binds a name in the current scope. Reassignment is allowed for mutable bindings:

```grf
int age = 21;
age = 22;
```

The declared type governs later assignments. Reading an undeclared or out-of-scope name is a semantic error.

v0.1 declarations use an explicit type. `auto T` is a conversion modifier on that type, not inference.

## 7. Automatic conversion and explicit casting

Graphite distinguishes an explicit conversion from target-directed automatic conversion.

### 7.1 Explicit conversion

A type constructor such as `int(value)` explicitly requests conversion to the named type:

```grf
str rawAge = "21";
int age = int(rawAge);
flt score = flt("12.5");
```

A conversion can fail. In particular, parsing arbitrary text as a number is fallible. The language's error-handling representation for failed conversions must be specified before the conversion feature is considered complete. The compiler MUST NOT silently invent a value for an invalid conversion.

### 7.2 Target-directed conversion with `auto T`

`auto` is a reserved keyword. `auto T` declares a target type `T` and requests conversion of a compatible initializer or function argument to that target type. It does not create a dynamically typed value. Once converted, the value has the declared target type. Bare `auto`, with no target type, is a semantic error. `auto age = 21;` is not valid.

```grf
import grf.io;

void main() {
    auto int age = io.input("Age: ");
}
```

The initializer returns `str`; Graphite attempts to parse it as `int`. A valid input such as `21` produces an integer value. Invalid input must follow the language's explicitly defined conversion-failure behavior.

The same target-directed rule applies to function parameters:

```grf
str greet(auto str msg) {
    return f"hello {msg}";
}

void main() {
    io.println(greet(123));
    io.println(greet("hi"));
    io.println(greet(12.433));
}
```

The `auto str` parameter accepts values that Graphite can format as a string. In this example, `123` becomes `"123"`, `"hi"` remains unchanged, and `12.433` becomes its defined string representation.

Conversion for an `auto T` parameter is inserted at the call site, before the call. The function body receives an ordinary value of type `T`. `auto` changes which argument types the caller may pass. It does not change the parameter's type inside the function or the type the generated Rust function receives.

For `str greet(auto str msg)`, a call `greet(123)` becomes a conversion of `123` to `str`, then a call passing that `str`. The generated Rust function receives the target type. For the current mapping, Graphite `str` corresponds to Rust `String`. The same rule applies to manifest-declared parameters such as `io.println`'s `auto str` value: Graphite inserts the conversion, and `graphite_core::io::println` receives a `String`. Rust must not be left to invent Graphite conversion rules.

Illustrative lowering, not a required helper name:

```rust
greet(graphite_core::format::to_string(&123));
```

The actual helper and ownership strategy must match the runtime implementation and must be documented when chosen. The semantic analyzer records the conversion as an explicit node in the typed AST, or an equivalent explicit lowering representation, before code generation.

### 7.3 Conversion policy

- `auto str` may format supported built-in values as text.
- `auto int`, `auto flt`, and fixed-width targets such as `auto int32` or `auto flt32` request conversion to that exact target type.
- Safe, well-defined conversions may happen implicitly under the `auto` rule.
- Potentially lossy numeric conversions, overflow, and invalid text parsing MUST NOT silently truncate, wrap, or invent a value.
- The conversion table must explicitly define accepted source/target pairs, precision and range checks, and failure behavior.
- `auto` without a following target type is a semantic error.
- `auto T` is not general type inference. A declaration such as `auto age = 21;` is not valid.
- `auto` cannot be used as an ordinary identifier.

The built-in console functions should use the same general conversion model rather than special-casing each primitive type. In particular, `io.print` and `io.println` should accept supported printable values and format them as text.

### 7.4 Conversion tests required

Positive tests must cover `auto str` with `str`, integer, floating-point, and boolean inputs; safe widening conversions; and valid numeric text parsing. Negative tests must cover malformed numeric text, out-of-range fixed-width conversions, unsupported source/target pairs, and any conversion that could silently lose information.

## 8. Constants

```grf
const flt pi = 3.14159;
```

A `const` binding cannot be reassigned. In Stage 4, `const` is an immutable local binding inside a function, block, or loop initializer. It is not a top-level declaration and it does not require compile-time evaluation. Do not equate it with Rust `const`.

## 9. Expressions and operators

Proposed operators:

- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Increment and decrement: prefix and postfix `++` and `--`
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Logical: `&&`, `||`, `!`
- Assignment: `=`
- Grouping: `(`, `)`

v0.1 does not include `+=`, `-=`, `*=`, `/=`, `%=`, `->`, `:`, or `?`. The lexer must not define token kinds for them. The parser must reject those character sequences rather than treating them as operators.

`++` and `--` apply only to mutable integer variables. They must not silently wrap on overflow. Overflow follows the integer-overflow policy; that policy is an open decision, and implementations must not introduce unchecked wrapping while it remains open.

Prefix and postfix forms use conventional evaluation order:

- `++index` increments first and evaluates to the updated value.
- `index++` evaluates to the previous value and then increments.
- `--index` decrements first and evaluates to the updated value.
- `index--` evaluates to the previous value and then decrements.

The parser distinguishes prefix and postfix positions. The lexer emits one `PlusPlus` or `MinusMinus` token; it does not decide which form is in use.

The parser must use an explicit precedence and associativity table. At minimum, grouping must bind more tightly than unary operators; unary operators, including prefix `++` and `--`, must bind more tightly than multiplicative operators; multiplicative operators more tightly than additive operators; comparisons more tightly than logical conjunction; and logical conjunction more tightly than logical disjunction. Postfix `++` and `--` bind more tightly than prefix operators. The exact table, including assignment associativity, must be frozen in the parser specification.

`&&` and `||` are proposed to short-circuit. Stage 4 rejects them until that behavior is frozen. Unary `!` remains available for `bln` values.

## 10. Conditional statements

```grf
if (age >= 18) {
    io.println("Adult");
} else if (age >= 13) {
    io.println("Teenager");
} else {
    io.println("Child");
}
```

The condition must have type `bln`. Branches are evaluated in order; only the first matching branch executes. `else` is optional and appears last.

## 11. Loops

### 11.1 C-style loop

```grf
loop (int index = 0; index < 10; index++) {
    io.println(index);
}
```

The three clauses are initializer, condition, and update. The initializer runs once; the condition is checked before each iteration; the update runs after the body, including after a `continue`.

### 11.2 Condition loop

```grf
int index = 0;
loop (index < 10) {
    io.println(index);
    index++;
}
```

The condition must evaluate to `bln`.

### 11.3 Infinite loop

```grf
loop () {
    // Exits with break.
}
```



### 11.4 Loop-control statements

```grf
break;
continue;
```

`break` and `continue` are valid only inside a loop and apply to the nearest enclosing loop. A C-style loop's `continue` behavior must preserve execution of the update expression before the next condition check.

## 12. Functions

```grf
int square(int number) {
    return number * number;
}

flt average(flt first, auto flt second) {
    return (first + second) / 2.0;
}

void greet(str userName) {
    io.println(f"Hello, {userName}!");
}
```

Rules:

- Return type precedes function name.
- Parameters have explicit types in the initial version. An `auto T` parameter is still an explicit target type `T`; the `auto` modifier is resolved at each call site.
- A non-`void` function must return a compatible value on every reachable path.
- A `void` function cannot return a value.
- Call argument count and types must match the signature after alias normalization and after any call-site `auto T` conversion.
- Top-level functions in the entry file may call each other regardless of source order.
- Function overloading and nested functions are excluded unless specified later.

The public visibility mechanism for functions in Graphite-authored libraries must be aligned with the `grf.json` library contract.

## 13. Strings and interpolation

```grf
str greeting = "Hello";
str message = f"Welcome, {userName}!";
```

Interpolated strings begin with `f` immediately before the opening quote. Interpolated expressions are parsed as expressions, not pasted into source code as raw text.

Each interpolation expression is evaluated once, in source order, and converted to `str` with the same accepted-source formatting rules as `auto str`. The compiler lowers interpolation through that same formatting implementation.

v0.1 formatting is defined for these primitive types:

- `str`
- signed and unsigned integer types, after alias normalization
- floating-point types
- `bln`, formatted as `true` or `false`
- `chr`

The exact digit, sign, and floating-point spellings must be documented with the formatting implementation. They must be deterministic. Types outside this set, including classes and collections, which are not part of v0.1, produce a compile-time diagnostic. Interpolation does not invent a representation for them.

Escape sequences, escaped braces, multiline strings, and formatting specifiers remain to be specified. Malformed interpolation is a compile-time error.

## 14. Imports, domains, packages, and modules

Example:

```grf
import grf.io;
import coolDev.netHttp;

void main() {
    io.println("Hello");
    // After importing coolDev.netHttp, calls use netHttp.<function>(...).
}
```

A domain is the namespace root under which packages are identified. `grf` is the reserved domain for Graphite-provided core/inbuilt packages. Third-party package domains use their assigned or configured domain names.

v0.1 import grammar is exactly two segments:

```text
import <domain>.<library>;
```

The first segment is the domain. The second is the library. `import grf.io.print;` is invalid because functions are not imported. Import aliases are not supported. Import paths are case-sensitive. Import statements must appear at the beginning of the source file, before declarations. An import resolves a library through its domain `grf.json`. It does not import a function or a nested module.

The local namespace is the library name. `import grf.io;` permits `io.print(...)` and `io.println(...)`. `grf.io.print(...)` is not the call syntax for an imported library. A missing domain, library, function, or exported symbol is an error.

Two imports that bind the same local name are a compile error:

```grf
import grf.io;
import other.io;
```

Importing the same domain and library more than once is a compile error. The diagnostic identifies the repeated import. The compiler does not ignore the repeat and does not register a second binding.

## 15. Collections and classes

Collections and classes are planned but their examples are illustrative until their semantics are frozen. They are outside v0.1. `class`, `private`, `this`, `public`, `new`, and `constructor` are ordinary identifiers until a class specification reserves them. A scratch program that uses arrays, `library`, `enumerate`, or the type spelling `string` is not a v0.1 program. The Graphite string type is `str`.

Illustrative array:

```grf
int32[] ages = [18, 21, 25];
```

Illustrative class:

```grf
class Person {
    private str name;

    Person(str name) {
        this.name = name;
    }

    str getName() {
        return this.name;
    }
}
```

Before implementation, define collection size and mutation rules, indexing behavior, ownership, class construction, field visibility, `this`, and Rust lowering. Inheritance is not assumed for v0.1.

## 16. Error handling

No exception syntax is frozen in this document. `try/catch/finally` is deferred until the language's error model is designed.

The compiler must distinguish lexical errors, parse errors, semantic/type errors, unresolved imports, library contract errors, and downstream Rust build errors. Diagnostics should point to Graphite source locations where possible.

## 17. Syntax specification completion criteria

A syntax segment is not frozen until it has:

- accepted examples;
- rejected examples;
- lexical and grammar rules;
- semantic rules;
- type/conversion behavior where applicable;
- Rust-lowering expectations;
- diagnostics;
- positive and negative tests;
- acceptance criteria.

An example alone does not freeze a feature.