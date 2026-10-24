# LLD 14 --- Stage 10: Type Descriptors and `typeOf()`

**Status:** Implementation specification\
**Target:** Graphite v0.1\
**Roadmap stage:** Stage 10\
**Prerequisites:** Canonical type system, semantic analysis, typed AST,
built-in function/method resolution, runtime value formatting, and named
record types.

## 1. Purpose

Implement a dedicated built-in `Type` descriptor and the built-in
`typeOf(expression)` operation. `typeOf` returns a descriptor
representing the **static Graphite type** of its expression argument. It
does not inspect, parse, convert, or dynamically discover the
expression's contents.

The descriptor supports only: - Canonical type name through
`Type.name() -> str`. - Standard descriptor display through
`io.println(TypeValue)` and existing string interpolation/formatting
paths where supported. - Type identity equality and inequality.

Reflection beyond type identity and name/display behavior is explicitly
deferred.

## 2. Goals and non-goals

### Goals

-   Add a dedicated compiler-recognized `Type` type.
-   Add `typeOf(expression) -> Type`.
-   Ensure `Type.name()` returns a canonical type name.
-   Define built-in and custom-record display forms.
-   Preserve aliases through canonical type identity.
-   Define descriptor equality by type identity, not rendered text.
-   Ensure descriptors never weaken static typing.
-   Specify AST/type-checking/codegen integration and tests.

### Non-goals

-   `any`, dynamic typing, or runtime type discovery.
-   Parsing a value from its contents.
-   Conversions between arbitrary types.
-   Reflection over record fields, methods, module metadata, generic
    arguments, or inheritance.
-   Type construction or user-defined `Type` values.
-   `Type` descriptors for values that have no valid static type.
-   Exposing Rust `TypeId`, `std::any::type_name`, or other
    backend-specific representations as language semantics.

## 3. Source syntax

``` grf
int age = 21;
Type t = typeOf(age);

io.println(t.name());
io.println(t);
io.println(typeOf("123"));
```

Expected output:

``` text
int64
<class 'int64'>
<class 'str'>
```

`Type` is reserved in the type namespace only. It cannot be redeclared
as a record or another type. It is not a lowercase keyword, and it does
not occupy the value namespace. A variable or function may be named
`Type` when the existing namespace rules otherwise allow that spelling.
Type positions still resolve the built-in:

``` grf
int Type = 1;
Type descriptor = typeOf(Type);
```

`typeOf` is reserved everywhere `lengthOf` is reserved. It cannot be a
variable, parameter, function, field, loop binding, destructuring
binding, or type name. Do not merge the type and value namespaces to
implement descriptors.

## 4. Static type semantics

### 4.1 `typeOf(expression)`

The expression argument is checked normally by the compiler. The result
has static type `Type`, regardless of the argument's static type.

``` grf
int age = 21;
Type t = typeOf(age);       // valid
Type s = typeOf("hello");   // valid
Type b = typeOf(true);      // valid
```

`typeOf` describes the static type of the expression, not the runtime
value or the value's textual contents.

``` grf
int age = 21;
Type t = typeOf(age);       // describes int64

str text = "123";
Type s = typeOf(text);      // describes str, not int64
```

The following does not parse or convert `"123"` into an integer:

``` grf
Type t = typeOf("123");     // describes str
```

### 4.2 Expression checking and side effects

The argument must be a syntactically and semantically valid expression.
Its static type must be resolvable. `typeOf` must not become a way to
bypass undeclared-name checks, invalid-operation checks, or other
semantic diagnostics.

For the initial implementation, evaluate the argument expression exactly
once before producing its descriptor, preserving ordinary expression
evaluation and side effects. The descriptor result is determined by the
expression's compile-time type, not by the evaluated value. If
evaluation of the argument fails, normal runtime failure behavior
applies and no descriptor is returned.

Do not silently erase side effects by compiling `typeOf(expression)`
into a descriptor constant if doing so would skip required evaluation.
An optimizer may elide evaluation only if the compiler's existing
optimization rules prove the expression side-effect-free and safe to
remove.

### 4.3 Canonical type identity

Resolve aliases before creating or comparing descriptors. The descriptor
identifies the canonical type represented by the type system.

`Type.name()` uses the existing canonical spelling for every supported
type. Canonicalization is recursive inside compound types. `map<str, int>`
and `map<str, int64>` describe the same map type and produce equal
descriptors.

  Expression type      `name()`            Descriptor display
  -------------------- ------------------- -------------------------
  `int` / `int64`      `int64`             `<class 'int64'>`
  `int[]` / `int[3]`   `int64[]`           `<class 'int64[]'>`
  `map<str, int>`      `map<str, int64>`   `<class 'map<str, int64>'>`
  `User`               `User`              `<type 'User'>`
  `User[]`             `User[]`            `<class 'User[]'>`
  `map<str, User>`     `map<str, User>`    `<class 'map<str, User>'>`
  `Type`               `Type`              `<class 'Type'>`

Use `<type 'Name'>` only for a named custom record type itself. Built-in
types, arrays, maps, and `Type` itself use
`<class 'CanonicalName'>`. Include the other built-in spellings already
implemented by Graphite, such as `int16`, `int32`, `uint16`, `uint32`,
`uint64`, `flt32`, `flt64`, `bln`, `chr`, and `str`. Do not invent names
for unimplemented types. Do not add reflection over generic arguments or
compound-type components.

`typeOf(typeOf(1))` describes the static type of the inner call, which
is `Type`. Its name is `Type` and its display is `<class 'Type'>`.

### 4.4 Invalid or unsupported expressions

`typeOf` requires an expression with a valid, resolved static type.

-   Undeclared identifiers and invalid expressions remain compile-time
    errors.
-   A `void`-returning call has no value type and is rejected as an
    argument to `typeOf`.
-   An expression whose type is unresolved or invalid is rejected.
-   Do not return a sentinel descriptor, `null`, `any`, or a string for
    an invalid expression.

## 5. The `Type` descriptor

### 5.1 Dedicated type representation

`Type` is a dedicated built-in Graphite type recognized by the type
checker. It must not be represented as `str`, `any`, a record alias, or
an untyped object.

The type system should represent it with a distinct internal type kind
(for example, `BuiltinTypeDescriptor`) or the project's equivalent
canonical type identifier. Use the existing type registry and built-in
type conventions where practical.

### 5.2 `Type.name()`

`name()` is a built-in method available on a `Type` descriptor:

``` grf
Type t = typeOf(21);
str name = t.name();
```

Static return type: `str`.

It returns the canonical Graphite type name. Built-in types return their
canonical names. Aliases return the canonical target name. Custom record
types return their declared record name. Arrays and maps return the
recursive canonical spelling, such as `int64[]` and `map<str, int64>`.
A maximum length such as `int[3]` is not part of the type name.

`name()` does not return a formatted display string. For example,
`typeOf(21).name()` returns `int64`, not `<class 'int64'>`.

No other methods or fields are added to `Type` in Stage 10.

### 5.3 Descriptor display

Printing a descriptor uses the existing value-formatting/printing
integration. Use these forms:

-   Built-in type: `<class 'int64'>`
-   String type: `<class 'str'>`
-   Named record type itself: `<type 'User'>`
-   Compound type, including a compound whose leaf is a record: `<class 'User[]'>`
-   The descriptor type: `<class 'Type'>`

For example:

``` grf
io.println(typeOf(21));       // <class 'int64'>
io.println(typeOf("123"));    // <class 'str'>
io.println(typeOf(user));     // <type 'User'>
```

The descriptor's display form is separate from its `name()` value. Do
not derive descriptor identity from the display string.

If the existing interpolation implementation formats values through the
same formatter as `io.println`, interpolation of a `Type` descriptor
must use the same display representation. Do not add a separate
formatting policy for interpolation.

### 5.4 Where a `Type` value may be stored

`Type` is a normal copyable value. It may be stored wherever an ordinary
value of that type is allowed, including arrays, map values, and record
fields:

``` grf
Type[] descriptors = [typeOf(1), typeOf("hello")];

map<str, Type> descriptorByName = {
    "age": typeOf(1),
    "name": typeOf("hello"),
};

type Metadata {
    descriptor: Type,
}
```

Copying a descriptor copies its canonical type identity. It does not
copy or retain the original expression's value. The Stage 8 map-key
whitelist stays `str`, `bln`, `chr`, and the supported signed and
unsigned integer types. `Type` is not a map key. Do not add special
reference semantics or dynamic behavior for stored descriptors.

## 6. Descriptor equality

### 6.1 Supported operators

Support `==` and `!=` between two values of type `Type`. The result type
is `bln`.

``` grf
Type a = typeOf(1);
Type b = typeOf(2);
Type c = typeOf("hello");

bln same = a == b;       // true: both describe int64
bln different = a != c;  // true
```

### 6.2 Identity rule

Equality compares canonical type identity, not: - The descriptor's
display string. - The source spelling used for an alias. - The runtime
value. - The address of a descriptor allocation.

Two descriptors are equal if and only if they refer to the same
canonical Graphite type identity.

Therefore:

``` grf
typeOf(1) == typeOf(2)       // true if both expressions are int64
typeOf(1) == typeOf("1")     // false
```

If a type alias and its canonical type name refer to the same type,
their descriptors compare equal. Distinct named record types remain
unequal even if their fields have identical layouts.

Do not introduce structural type equality or record equality as part of
this stage.

### 6.3 Invalid comparisons

Comparing a `Type` descriptor to a string or arbitrary value is a
compile-time error unless an existing explicit conversion produces
another supported comparison expression.

``` grf
Type t = typeOf(1);
bln invalid = t == "int64"; // compile-time error
```

Do not implicitly convert descriptors to strings for comparison.

Ordering comparisons are rejected. `<`, `<=`, `>`, and `>=` between
`Type` values are compile-time errors. Do not order descriptors by
name, display text, internal ID, or declaration order. Only `==` and
`!=` compare canonical type identity. A comparison between `Type` and an
unrelated type is also a compile-time error.

## 7. Type identity and custom records

Named record descriptors use the existing nominal record identity from
Stage 7.

``` grf
type User {
    name: str,
    age: int,
}

User user = { name: "Ada", age: 36 };
Type t = typeOf(user);

io.println(t.name()); // User
io.println(t);        // <type 'User'>
```

Custom record descriptors: - Return the declared record name from
`name()`. - Display as `<type 'User'>`. - Compare by the record's
canonical type identity. - Remain distinct for different named records,
even when layouts match.

This stage does not expose record fields through `Type`, create record
instances from descriptors, or permit runtime discovery of unknown
record types.

## 8. Runtime representation and code generation

### 8.1 Descriptor representation

Use a lightweight descriptor representation referring to the
compiler/runtime's canonical type identity. A descriptor may be
implemented as: - A stable internal type ID plus canonical name/display
metadata; or - A static descriptor reference/table entry associated with
each canonical type.

Choose the representation that best fits the existing runtime
architecture. Do not expose backend-specific Rust type names or raw
pointer values as language-visible behavior.

The representation must support: - `name()`. - Standard display
formatting. - Identity equality. - Built-in and named-record type
identities.

Descriptors should be cheap to copy and must not own or retain the value
passed to `typeOf`.

### 8.2 Code generation

During semantic analysis, resolve the expression's canonical type and
annotate the `typeOf` node with its descriptor identity.

At runtime, preserve the required evaluation of the argument expression
exactly once, then produce the descriptor for the statically known type.
The generated code must not inspect the argument's runtime contents to
determine the descriptor.

Possible implementation strategies include a helper that evaluates the
argument before returning a compile-time descriptor constant, or direct
code generation that preserves evaluation while returning the
descriptor. Follow existing compiler architecture and error propagation.

Do not use Rust's `std::any::type_name` or `TypeId` as the definition of
Graphite type identity. Rust implementation details may support the
internal representation only if Graphite's canonical type identity and
display rules remain explicit and stable.

### 8.3 Printing integration

`io.println`, `str(...)`, and interpolation format a descriptor through
the existing shared formatter. All three produce the display form, for
example `<class 'int64'>`.

`Type.name()` returns the bare canonical name. Do not confuse that name
with the display form.

Ordinary typed assignment stays strict:

``` grf
str text = typeOf(1);      // compile-time type mismatch
```

`auto str` uses the existing target-directed conversion rules and may
convert a descriptor to its display representation, consistent with
records and other supported values:

``` grf
auto str text = typeOf(1); // <class 'int64'>
```

Register that conversion in the centralized conversion/formatting
mechanism. It applies at initialization, reassignment, parameters, and
other existing `auto str` sites. It does not weaken ordinary typed
assignment and does not add a separate formatting path.

## 9. Type checker and compiler integration

The implementation must:

1.  Register `Type` as a reserved built-in type.
2.  Register `typeOf` as a reserved built-in operation.
3.  Resolve the argument expression normally and require a valid
    non-void static type.
4.  Annotate the expression with its canonical type identity.
5.  Type `typeOf(expression)` as `Type`.
6.  Resolve `Type.name()` to return `str`.
7.  Permit `==` and `!=` between `Type` values, returning `bln`.
8.  Reject comparisons between `Type` and unrelated types.
9.  Use the canonical type registry for aliases and record identities.
10. Integrate descriptor display with the standard formatter.
11. Preserve the existing type rules for all unrelated expressions.

Do not implement `typeOf` as a library function that receives an
arbitrary value and guesses its type. It is compiler-recognized because
its semantics depend on static type information.

## 10. Diagnostics

Provide actionable diagnostics for at least:

-   `Type` used as a conflicting user-defined type name.
-   `typeOf` redefined or shadowed contrary to the built-in naming
    policy.
-   `typeOf` applied to a `void` expression.
-   `typeOf` applied to an invalid or unresolved expression.
-   Unknown method on a `Type` descriptor.
-   Invalid argument count for `typeOf` or `Type.name()`.
-   Comparison between `Type` and a non-`Type` value.
-   Any attempt to use `Type` as `any` or as a general dynamic value.

Diagnostics should follow the existing source-span and error-reporting
conventions.

## 11. Test plan

### 11.1 Positive compile-and-run tests

1.  `typeOf(intValue)` returns a `Type` descriptor.
2.  `typeOf("123")` reports `str`, not `int64`.
3.  `Type.name()` returns canonical names.
4.  `int` and `int64` report `int64`.
5.  `uint` and `uint64` report `uint64`.
6.  `flt` and `flt64` report `flt64`.
7.  Other supported fixed-width numeric types report their canonical
    names.
8.  Boolean and character values report their canonical built-in names.
9.  Custom record types return their declared names.
10. Built-in descriptor display matches `<class 'canonical-name'>`.
11. Record descriptor display matches `<type 'RecordName'>`. Compound
    types, including `User[]` and `map<str, User>`, use
    `<class 'CanonicalName'>`.
12. Descriptors for the same canonical type compare equal, including
    `map<str, int>` and `map<str, int64>`.
13. Descriptors for different canonical types compare unequal.
14. Different named record types with identical layouts compare unequal.
15. `Type.name()` is statically typed as `str`.
16. Descriptor printing works through `io.println`.
17. `str(...)`, interpolation, and `auto str` use the same display.
    Ordinary `str` assignment of a descriptor is rejected.
18. `typeOf` evaluates its argument exactly once, including side
    effects.
19. A failing argument expression propagates its normal runtime failure.
20. A `Type` value can be stored in an array, a map value, and a record
    field, and copying it preserves type identity.
21. `typeOf(typeOf(1))` reports `Type` and displays `<class 'Type'>`.
22. A value or function may be named `Type` without hiding the built-in
    type.

### 11.2 Negative compile tests

Reject:

-   Redeclaring `Type` as a record.
-   Declaring `typeOf` as a variable, parameter, function, field, or
    type name.
-   Calling `typeOf` with the wrong number of arguments.
-   Calling `typeOf` on a `void` expression.
-   Passing an undeclared or invalid expression to `typeOf`.
-   Calling an unsupported descriptor method.
-   Comparing `Type` with `str`, numeric, array, map, or record values.
-   Ordering two descriptors with `<`, `<=`, `>`, or `>=`.
-   Using `Type` as a map key.
-   Assigning a descriptor to an ordinary `str` variable.
-   Using `Type` as an untyped escape hatch.

### 11.3 Regression tests

Verify that: - Existing canonical aliases remain unchanged. - Existing
type equality and record nominal identity remain unchanged. -
`io.println` behavior for primitives, arrays, records, and maps is
unchanged. - String interpolation and `str(...)` behavior for existing
supported types is unchanged. - No dynamic typing or implicit conversion
has been introduced. - Existing compiler diagnostics remain stable where
unaffected.

## 12. Implementation sequence

1.  Inspect the current type registry, alias canonicalization, type
    identity representation, built-in function/method resolution,
    expression evaluation, formatting, interpolation, and Rust code
    generation.
2.  Report the actual files and functions that will be changed before
    editing.
3.  Add the dedicated `Type` type and reserve its name.
4.  Add compiler-recognized `typeOf(expression)` typing and canonical
    identity annotation.
5.  Add `Type.name()`.
6.  Add descriptor identity equality and inequality.
7.  Add descriptor display formatting through the existing formatter.
8.  Preserve argument evaluation exactly once.
9.  Add diagnostics and tests.
10. Update language documentation and this LLD if implementation reveals
    an architectural constraint; do not silently weaken the locked
    semantics.

Keep lexer/parser, type registry, semantic analysis, typed AST, code
generation, runtime formatting, and CLI behavior separated. Do not use
source-text rewriting, regular-expression matching, or runtime content
inspection to implement static type introspection.

## 13. Acceptance criteria

Stage 10 is complete when:

-   `Type` is a dedicated built-in type, not `str` or `any`.
-   `typeOf(expression)` returns `Type` and describes the expression's
    static type.
-   The argument is validated normally and evaluated exactly once.
-   `Type.name()` returns canonical names.
-   Built-in and compound descriptors display as `<class 'name'>`.
-   A named record descriptor itself displays as `<type 'Name'>`.
-   Aliases and compound types resolve to canonical descriptor identity
    and names.
-   Descriptor equality is based on canonical type identity.
-   Ordering comparisons between descriptors are rejected.
-   Comparisons between `Type` and unrelated values are rejected.
-   Printing and interpolation use the established formatting path.
-   No reflection beyond name/display/identity is added.
-   Existing static typing, conversions, aliases, records, arrays, maps,
    and formatting tests pass.

## 14. Locked decisions summary

-   `Type` is a dedicated compiler-recognized type.
-   `typeOf(expression) -> Type`.
-   `typeOf` describes static type, not runtime contents.
-   The expression is type-checked normally and evaluated exactly once.
-   `Type.name() -> str` returns the canonical type name.
-   `Type.name()` uses the existing canonical spelling recursively.
    Built-in and compound display uses `<class 'CanonicalName'>`. Only a
    named record type itself uses `<type 'Name'>`. `Type` displays as
    `<class 'Type'>`.
-   `io.println`, `str(...)`, and interpolation share that display.
    Ordinary `str` assignment rejects a descriptor. `auto str` converts
    it through the centralized formatter.
-   `Type` is reserved in the type namespace only. `typeOf` is reserved
    everywhere `lengthOf` is reserved.
-   A `Type` value may be stored in arrays, map values, and record
    fields. It is not a map key. Copying copies canonical identity.
-   `==` and `!=` compare canonical type identity. `<`, `<=`, `>`, and
    `>=` between descriptors are rejected.
-   Different nominal record types remain distinct even if their layouts
    match.
-   Initial scope is identity, name, and display only.
-   No `any`, dynamic typing, generic reflection, or change to ordinary
    typed assignment is introduced.
