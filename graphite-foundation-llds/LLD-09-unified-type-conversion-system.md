# LLD 08 --- Unified Type Conversion System

**Status:** Implementation specification\
**Target:** Graphite v0.1\
**Stage:** Stage 5 --- Unified type conversion system\
**Scope:** Primitive conversions, `auto T`, explicit `T(value)` calls,
conversion diagnostics, and tests.

## 1. Purpose

Define one deterministic conversion system for Graphite's currently
supported primitive types. The same conversion registry must drive both
target-directed `auto T` declarations and explicit conversion calls such
as `int("123")`.

This LLD specifies which conversions are permitted, how values are
converted, which conversions can fail at runtime, and how the compiler
represents them. It does not introduce dynamic typing, a general-purpose
`any` type, user-defined conversions, or new primitive types.

## 2. Goals and non-goals

### Goals

-   Support approved conversions among the currently supported primitive
    types.
-   Keep `auto T` and explicit `T(value)` behavior consistent.
-   Use a centralized conversion registry shared by semantic analysis,
    typed-AST construction, and code generation.
-   Reject unsupported source/target pairs at compile time.
-   Define deterministic parsing, numeric range, floating-point,
    boolean, and character rules.
-   Make runtime conversion failures explicit and diagnosable.
-   Preserve Graphite's static type safety and existing interpolation
    formatting rules.
-   Provide positive, negative, boundary, and runtime-failure tests.

### Non-goals

-   Arrays, maps, named records, `Type`, classes, or user-defined
    generics as conversion targets. Displaying an array, record, map, or
    `Type` descriptor through `str`, `io.println`, interpolation, or
    `auto str` is formatting, not a conversion into or out of that
    value. A plain `str` variable still does not accept a `Type`
    descriptor.
-   `any`, implicit dynamic typing, or runtime type discovery.
-   User-defined conversion operators.
-   Changes to the syntax or semantics of interpolation.
-   General implicit numeric coercion between expressions.
-   Changes to literal-specific initialization rules already defined in
    LLD-04.
-   A new error-handling syntax such as `Result`, exceptions, or
    `try/catch`.

## 3. Supported types and canonical names

Conversion rules operate on canonical semantic types, not on the
spelling used in source.

  -----------------------------------------------------------------------
  Source            Canonical type    Canonical display Rust
  spelling(s)                         name              representation
  ----------------- ----------------- ----------------- -----------------
  `int`, `int64`    `Int64`           `int64`           `i64`

  `uint`, `uint64`  `UInt64`          `uint64`          `u64`

  `flt`, `flt64`    `Float64`         `flt64`           `f64`

  `bln`             `Boolean`         `bln`             `bool`

  `str`             `String`          `str`             Graphite's
                                                        existing string
                                                        runtime
                                                        representation

  `chr`             `Character`       `chr`             Rust `char`
  -----------------------------------------------------------------------

Aliases are identical types. For example, `int(5)` and `int64(5)` target
the same canonical type. A conversion registry must never contain
separate behavioral rules for aliases.

The registry also includes every fixed-width numeric type that exists:
`int16`, `int32`, `uint16`, `uint32`, and `flt32`. The matrix columns
`int64`, `uint64`, and `flt64` state the 64-bit rules. Those rules apply
to the narrower types using that type's range and exact representability.
`flt32` to `flt64` is exact. `flt64` to `flt32` is accepted only when the
value is exactly representable, without overflow or nonzero underflow.
Ranges are the real ranges of those types. `int32("40000")` succeeds
because 40000 is inside the signed 32-bit range. `int16("40000")` and
`int32("2147483648")` fail at runtime.
`auto T` uses this same registry at every existing site, including
function parameters. Float text accepts the same shapes as a Graphite
float literal, plus an optional leading sign and surrounding ASCII
whitespace (space, tab, line feed, carriage return, vertical tab, and
form feed). `.5` and `5.` are rejected.

`chr` represents one Unicode scalar value. It is not a byte and is not a
grapheme cluster. A grapheme cluster may contain more than one Unicode
scalar value.

## 4. Source syntax

### 4.1 Explicit conversions

Explicit conversion calls use the target type name as a call-like
expression:

``` grf
int("123")
uint("123")
flt("12.433")
bln("true")
chr("A")
str(123)
```

The aliases `int64`, `uint64`, and `flt64` are accepted and resolve to
the same targets as their short aliases.

A conversion expression has exactly one argument. Wrong argument counts
are compile-time errors. A call to a type name with one argument is
interpreted as a conversion only when that name is a supported canonical
primitive type/alias; it is not a normal user-function call.

### 4.2 Target-directed `auto T`

Preserve the existing `auto T` declaration grammar. The target type `T`
selects a conversion from the initializer's static type:

``` grf
auto str a = 123;
auto int b = "123";
auto bln c = "true";
```

These examples illustrate target-directed conversion; use the exact
declaration grammar already accepted by the parser. `auto T` is not
inference, dynamic typing, or permission to accept an arbitrary value.
It requests the same conversion represented by `T(value)`. The same
registry applies to `auto T` parameters. Conversion runs at the call
site, before the function body executes. Later assignments to that same
`auto T` binding, including an `auto T` parameter, use the same
registry and the declared target type. An ordinary typed variable does
not gain that conversion on assignment.

If a target/source pair is not approved by the conversion matrix,
compilation fails. If the pair is approved but the actual value is
invalid, execution fails with a conversion error.

### 4.3 No implicit general numeric coercion

Do not make an ordinary assignment, function argument, or return
statement silently convert an arbitrary numeric expression to another
numeric type. For example, an `int64` expression assigned to a `uint64`
variable remains a type mismatch unless an explicit conversion or
approved `auto uint64` conversion is requested.

LLD-04 separately permits representable numeric *literals* to initialize
narrower numeric types under literal-specific range checking. That
literal exception is not a general expression-conversion rule and must
not be expanded by this LLD.

## 5. Conversion matrix

The table below is normative. "Identity" means the value is already of
the canonical target type and no value-changing operation is needed.
"Allowed" means the conversion is supported and follows the rules in
Section 6. "No" means a compile-time unsupported-conversion error.

  ---------------------------------------------------------------------------------------------
  Source ↓ / `int64`    `uint64`   `flt64`          `bln`            `str`           `chr`
  Target →                                                                           
  ---------- ---------- ---------- ---------------- ---------------- --------------- ----------
  `int64`    Identity   Allowed    Allowed, exact   Allowed: 0 or 1  Allowed         Allowed:
                                   only             only                             valid
                                                                                     Unicode
                                                                                     scalar
                                                                                     value

  `uint64`   Allowed,   Identity   Allowed, exact   Allowed: 0 or 1  Allowed         Allowed:
             range                 only             only                             valid
             checked                                                                 Unicode
                                                                                     scalar
                                                                                     value

  `flt64`    Allowed,   Allowed,   Identity         Allowed: exactly Allowed         No
             finite     finite                      0.0 or 1.0 only                  
             integral   integral                                                     
             value and  value and                                                    
             range      range                                                        
             checked    checked                                                      

  `bln`      Allowed:   Allowed:   Allowed:         Identity         Allowed:        No
             false→0,   false→0,   false→0.0,                        `false` or      
             true→1     true→1     true→1.0                          `true`          

  `str`      Allowed:   Allowed:   Allowed: strict  Allowed: parse   Identity        Allowed:
             strict     strict     finite decimal   `true`/`false`                   exactly
             decimal    unsigned   parsing                                           one
             integer    decimal                                                      Unicode
             parsing    parsing                                                      scalar
                                                                                     value

  `chr`      Allowed:   Allowed:   Allowed: exact   No               Allowed:        Identity
             Unicode    Unicode    numeric                           one-character   
             scalar     scalar     representation                    string          
             numeric    numeric    only                                              
             value      value                                                        
  ---------------------------------------------------------------------------------------------

The matrix is closed: a conversion not listed as allowed must not be
implemented implicitly or accepted as an explicit conversion. In
particular, do not invent conversions from arbitrary values to booleans
or characters.

### 5.1 Identity conversions

A conversion to the same canonical type succeeds without changing the
value. Aliases count as the same canonical type. `str(value)` where
`value` is already `str`, for example, preserves the string.

### 5.2 Conversion registry

Implement a single declarative conversion registry keyed by
`(source canonical type, target canonical type)`. Each registry entry
identifies:

-   Whether the conversion is identity, total, or fallible.
-   The semantic rule/conversion operation.
-   Any required runtime validation.
-   The resulting canonical target type.
-   The diagnostic category for a failure.

Semantic analysis must query this registry. The typed AST must represent
approved conversions explicitly. Code generation/runtime lowering must
consume that representation rather than independently deciding which
conversions exist.

Do not implement conversions using regex-based source rewriting,
generated-source text replacement, or scattered per-function special
cases.

## 6. Conversion semantics

### 6.1 String to `int64`

-   Trim leading and trailing ASCII whitespace.
-   Accept an optional leading `+` or `-`, followed by one or more ASCII
    decimal digits (`0`--`9`).
-   Do not accept decimal points, exponent notation, digit separators,
    hexadecimal notation, or trailing non-whitespace characters.
-   Parse within the signed 64-bit range: `-9223372036854775808` through
    `9223372036854775807`.
-   Empty strings, malformed strings, and out-of-range values produce
    runtime conversion errors.

Examples:

``` grf
int a = int("123");       // succeeds: 123
int b = int(" -42 ");     // succeeds: -42
int c = int("abc123");    // runtime conversion error
int d = int("1.0");       // runtime conversion error
```

### 6.2 String to `uint64`

-   Trim leading and trailing ASCII whitespace.
-   Accept an optional leading `+` followed by one or more ASCII decimal
    digits.
-   Reject a leading minus sign, even if the mathematical value would be
    zero.
-   Reject decimal points, exponent notation, separators, and trailing
    non-whitespace characters.
-   Enforce the range `0` through `18446744073709551615`.
-   Malformed, negative, empty, or out-of-range input produces a runtime
    conversion error.

### 6.3 String to `flt64`

-   Trim leading and trailing ASCII whitespace.
-   Accept a decimal floating-point grammar with an optional sign,
    optional fractional part, and optional decimal exponent (`e` or
    `E`).
-   Require at least one digit overall.
-   Reject separators, hexadecimal forms, trailing junk, and textual
    `NaN`/`Infinity` spellings.
-   Reject overflow to infinity and underflow of a nonzero input to
    zero, rather than silently losing the value.
-   Finite decimal values are rounded to the nearest representable
    IEEE-754 binary64 value using the platform's correctly rounded
    parser.
-   A decimal fraction generally cannot be represented exactly in binary
    floating point; this rounding is the defined representation
    behavior, not integer truncation.
-   Malformed or unrepresentable input produces a runtime conversion
    error.

The parser must use a well-defined numeric grammar and must not rely on
permissive parsing that accepts trailing characters.

### 6.4 Numeric conversions to `int64` and `uint64`

-   Floating-point inputs must be finite. A fractional value rounds down
    toward negative infinity before the integer conversion: `10.55`
    becomes `10`, and `-10.55` becomes `-11`. `10` converts to `flt64`
    as `10.0`.
-   Check the target range after rounding.
-   `flt64` to `int64` requires the rounded value to fit in the signed
    64-bit range. Because binary64 cannot represent every integer near
    the upper boundary, range validation must be performed without
    first performing an overflowing or saturating cast.
-   `flt64` to `uint64` requires a nonnegative value whose rounded
    result fits in the unsigned 64-bit range, validated without a
    saturating cast. A negative float fails before rounding.
-   `uint64` to `int64` fails above `9223372036854775807`.
-   `int64` to `uint64` fails for negative values.
-   No conversion may wrap or saturate. Float-to-integer conversion
    rounds down as specified above and does not use any other rounding
    mode.

### 6.5 Integer to `flt64`

`int64` and `uint64` values may convert to `flt64` only if the integer
is exactly representable as an IEEE-754 binary64 value. Reject values
that would lose integer precision.

This is stricter than many host-language casts, intentionally preserving
the no-silent-precision-loss policy. Tests must cover values around the
exact-integer boundary (2\^{53}), as well as signed and unsigned
endpoints.

### 6.6 `flt64` to `bln`

Only exact numeric zero and one are accepted:

-   `0.0` → `false`
-   `1.0` → `true`
-   Any other finite number, NaN, or infinity → runtime conversion error

This avoids introducing implicit truthiness semantics.

### 6.7 Integer to `bln`

Only `0` and `1` are accepted:

-   `0` → `false`
-   `1` → `true`
-   All other values → runtime conversion error

This rule applies to both signed and unsigned integers.

### 6.8 String to `bln`

-   Trim leading and trailing ASCII whitespace.
-   Accept `true` and `false`, case-insensitively.
-   Reject all other strings. Do not treat nonempty strings as true.
-   Invalid text produces a runtime conversion error.

Examples:

``` grf
bln a = bln("true");   // true
bln b = bln("FALSE");  // false
bln c = bln("yes");    // runtime conversion error
```

### 6.9 `bln` to numeric types

Use explicit, exact mappings:

-   `false` → `0` or `0.0`
-   `true` → `1` or `1.0`

These conversions are total and cannot overflow.

### 6.10 `str` conversions

`str(value)` uses Graphite's centralized primitive display-formatting
policy. It must not use unrelated ad hoc formatting logic in the
conversion implementation.

-   `int64` and `uint64`: base-10 digits, no grouping separators.
-   `bln`: lowercase `true` or `false`.
-   `chr`: the character's UTF-8 text.
-   `flt64`: use the canonical float formatting policy already used by
    Graphite interpolation. The same value must format identically
    through `str(value)` and interpolation.

If the existing interpolation policy does not fully specify float
formatting, document and test that policy before claiming float
conversion is complete. Do not silently introduce a second formatting
convention.

`str(str_value)` is identity.

### 6.11 Character conversions

A Graphite `chr` is exactly one Unicode scalar value.

-   `chr(str_value)` succeeds only if the string contains exactly one
    Unicode scalar value. An empty string or a string with multiple
    scalar values fails. Do not use grapheme-cluster count.
-   `chr(int64)` and `chr(uint64)` interpret the input as a Unicode code
    point. Valid values are `0` through `0x10FFFF`, excluding the
    surrogate range `0xD800` through `0xDFFF`.
-   Negative integers and invalid Unicode scalar values fail.
-   `chr(chr_value)` is identity.
-   `str(chr_value)` returns that scalar's UTF-8 text.
-   `int64(chr_value)`, `uint64(chr_value)`, and `flt64(chr_value)`
    return the scalar's numeric code point, with the `flt64` conversion
    subject to the exact-representability rule.

No character literal syntax is introduced by this LLD.

## 7. Runtime failure contract

Some approved conversions depend on the actual runtime value and can
fail even though the source and target types are compatible.

Examples include parsing malformed strings, a negative value converted
to `uint64`, a float that rounds outside the target integer range, and
a string with multiple Unicode scalar values converted to `chr`.

For v0.1:

-   A failed conversion raises a Graphite runtime conversion error and
    terminates the current program with a nonzero exit status.
-   The diagnostic must identify the source type, target type, and
    failure category. Include the source location of the conversion
    expression where available.
-   Do not return a default, zero, empty string, truncated value,
    wrapped value, or fabricated value.
-   Do not confuse a runtime conversion failure with a compile-time
    unsupported-conversion/type-mismatch error.
-   Do not add a new recoverable error-handling construct as part of
    this LLD.
-   Use one shared runtime conversion-error mechanism for conversions
    rather than per-conversion ad hoc process exits.

Suggested diagnostic shape:

``` text
Runtime conversion error at main.grf:12:15:
cannot convert str to int64: invalid decimal integer
```

Exact wording may follow the repository's established diagnostic style,
but the distinction between compile-time and runtime failures is
mandatory.

## 8. Typed AST and compiler architecture

### 8.1 Semantic analysis

For each explicit conversion call or `auto T` initializer:

1.  Resolve the source expression and its static type.
2.  Resolve the target to its canonical type.
3.  Query the centralized conversion registry.
4.  If the pair is unsupported, emit a compile-time diagnostic and do
    not construct a valid conversion node.
5.  If supported, construct a typed conversion node carrying source
    type, target type, conversion operation, and source span.

### 8.2 Typed-AST representation

Represent conversions as explicit typed-AST nodes. The exact Rust
enum/struct names may follow repository conventions, but the
representation must preserve at least:

-   Source expression.
-   Canonical source type.
-   Canonical target type.
-   Conversion operation/registry identifier.
-   Whether runtime validation may fail.
-   Source span for diagnostics.

An `auto T` initializer must lower through the same conversion node and
operation as explicit `T(value)`. Do not leave `auto T` as a separate
code-generation special case.

### 8.3 Code generation and runtime

-   Generate Rust only from a semantically valid typed AST.
-   Lower conversion nodes through a shared runtime conversion layer or
    equivalent shared conversion helpers.
-   Ensure Rust's `as` casts are not used where they can wrap, saturate,
    truncate, or lose integer precision contrary to this LLD.
-   Preserve source spans for runtime conversion diagnostics as far as
    the current runtime architecture permits.
-   Keep target-specific conversion implementation details out of parser
    logic.
-   Do not add special-case compiler branches for `int(...)`,
    `str(...)`, and similar calls when the registry can describe them.

## 9. Diagnostics

Compile-time diagnostics are required for:

-   Unsupported source/target conversion pairs.
-   Unknown target type or unsupported conversion target.
-   Wrong conversion argument count.
-   Invalid use of conversion syntax.
-   An `auto T` initializer whose source/target pair is not in the
    matrix.

Runtime diagnostics are required for:

-   Malformed textual input.
-   Numeric range violations.
-   Non-finite float-to-integer conversion, or a rounded value outside
    the target range.
-   Integer-to-float precision loss.
-   Invalid boolean numeric values.
-   Invalid boolean text.
-   Invalid Unicode scalar value or invalid character string length.
-   Float text overflow or prohibited underflow.

Messages should include the conversion location and enough information
to distinguish the failure reason.

## 10. Test plan

### 10.1 Registry and type checking

-   Test every matrix cell, including identity conversions and all
    unsupported pairs.
-   Test aliases (`int`/`int64`, `uint`/`uint64`, `flt`/`flt64`) resolve
    to the same canonical conversion behavior.
-   Test wrong argument counts and unknown conversion targets.
-   Verify unsupported conversions fail before code generation.
-   Verify ordinary assignments, arguments, and returns do not gain
    implicit numeric conversions.
-   Verify the separate literal-specific range-checking rule from LLD-04
    remains intact.

### 10.2 Text parsing

-   Valid signed and unsigned decimal integers.
-   Minimum and maximum `int64`.
-   Maximum `uint64`.
-   Leading/trailing ASCII whitespace.
-   Empty strings, signs without digits, embedded whitespace,
    separators, trailing junk, decimal strings passed to integer
    conversions, and out-of-range numbers.
-   Valid decimal floats with fractions and exponents.
-   Malformed floats, textual NaN/Infinity, overflow, and prohibited
    nonzero underflow.
-   Boolean text in mixed case and with surrounding ASCII whitespace.
-   Invalid boolean strings.

### 10.3 Numeric boundaries

-   Negative signed values converted to unsigned.
-   Unsigned values above the signed maximum converted to signed.
-   Float values with fractional parts.
-   NaN, positive/negative infinity, and signed/unsigned float range
    boundaries.
-   Integer-to-float exactness around (2\^{53}), including values just
    below, at, and above the boundary.
-   Values that are representable as `f64` but not exactly as an integer
    target.

### 10.4 Character boundaries

-   One ASCII scalar.
-   One non-ASCII scalar.
-   Empty string.
-   Multiple Unicode scalar values.
-   Combining marks and multi-scalar grapheme clusters.
-   Valid code points at the lower and upper bounds.
-   Surrogate values, negative values, and values above `0x10FFFF`.
-   Character-to-number and character-to-string conversions.

### 10.5 `auto T` parity

For every approved source/target pair that can be expressed by the
language, verify that explicit conversion and `auto T`:

-   Produce the same converted value on success.
-   Fail for the same invalid runtime values.
-   Use the same runtime failure category.
-   Resolve aliases identically.

Do not duplicate the entire test matrix manually if table-driven tests
can validate the same registry entries.

### 10.6 Formatting

-   Verify `str(value)` agrees with Graphite interpolation for every
    supported primitive source type.
-   Include negative integers, unsigned values, booleans, characters,
    and representative finite floats.
-   Preserve any already-approved float formatting behavior.

## 11. Acceptance criteria

Stage 5 is complete only when:

1.  The conversion matrix is implemented as one central registry.
2.  `auto T` and explicit conversions use the same semantic operation.
3.  All approved conversions have tests.
4.  All unsupported conversions fail at compile time.
5.  Runtime-invalid values produce defined conversion diagnostics and
    nonzero termination.
6.  Numeric conversions do not silently wrap or saturate. Float-to-integer
    conversion rounds down. Integer-to-float conversion does not lose
    integer precision.
7.  Character conversions enforce Unicode scalar validity.
8.  String conversions follow the strict parsing grammars in this LLD.
9.  String formatting is consistent with interpolation.
10. Existing Stage 4 behavior and the current working feature program
    remain regression-tested.
11. Formatting, workspace checks, and the full test suite pass.
12. No `any` type, dynamic conversion escape hatch, regex source
    rewriting, or per-function compiler special case is introduced.

## 12. Implementation order

1.  Audit current type representations, `auto T`, explicit conversion
    syntax, interpolation formatting, and runtime error infrastructure.
2.  Add canonical source/target conversion descriptors and the central
    registry.
3.  Implement compile-time conversion validation and typed-AST
    conversion nodes.
4.  Implement string parsing and range-checked numeric conversions.
5.  Implement boolean and character conversions.
6.  Route `auto T` through the same conversion nodes as explicit calls.
7.  Unify string formatting with interpolation.
8.  Add table-driven registry tests, boundary tests, runtime-failure
    tests, and regression tests.
9.  Run formatting, workspace checks, and all tests; report any
    deviations from this LLD explicitly.

## 13. Locked decisions

1.  `auto T` means target-directed conversion, not inference or dynamic
    typing.
2.  Explicit conversions use `T(value)` and share the `auto T`
    conversion registry.
3.  Only matrix-approved source/target pairs are allowed.
4.  No `any` type or equivalent untyped escape hatch.
5.  No silent wrap or saturation.
6.  Float-to-integer conversion rounds a finite value down toward
    negative infinity, then requires the result to be in range.
7.  Integer-to-float conversion requires exact representability. A value
    such as `10` becomes `10.0`.
8.  String parsing is strict and follows the grammars in this LLD.
9.  Boolean conversions do not use truthiness; numeric inputs must be
    exactly zero or one.
10. `chr` represents one Unicode scalar value and rejects invalid code
    points.
11. Conversion failures at runtime terminate execution with a conversion
    diagnostic and nonzero exit status.
12. Interpolation and `str(value)` share one formatting policy.
13. Conversion availability is decided centrally and does not depend on
    scattered compiler special cases.
14. Arrays, maps, records, `Type`, classes, user-defined conversions,
    and new error-handling syntax remain outside this LLD.
