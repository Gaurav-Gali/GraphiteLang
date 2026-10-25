# Graphite Language --- Generics Low-Level Design (LLD)

**Status:** Locked for the initial implementation\
**Feature:** Compile-time generics for functions, classes, and methods\
**Target:** Graphite (`.grf`) transpiled to Rust

## 1. Purpose

Specify the first implementation of generics in Graphite so reusable,
statically typed abstractions can be built before standard libraries
such as `grf.array` and `grf.string`.

The first release supports generic functions, classes, and methods;
generic type substitution; explicit type arguments; compile-time
checking; and monomorphized Rust emission for each used specialization.
The programmer writes every type argument. The compiler validates those
arguments and does not infer them. Explicit constraint syntax,
associated types, and inference are deferred; operations on type
parameters are checked at each instantiation.

## 2. Design principles

1.  Generic declarations are statically checked by Graphite.
2.  Generic parameters use angle brackets after the function or class
    name.
3.  Existing non-generic syntax remains unchanged.
4.  Generic arguments do not introduce new implicit conversions.
5.  Generic classes may still lower to Rust generics; generic functions
    that are called are monomorphized per concrete type-argument list.
6.  Operations on a type parameter are allowed in the generic body and
    validated when that body is instantiated with concrete types.
7.  Type arguments are always explicit. The compiler does not infer
    them from arguments or from an expected type.

## 3. Syntax

### 3.1 Generic functions

Graphite keeps its return-type-first function syntax. Type parameters
appear immediately after the function name.

``` graphite
T identity<T>(T value) return value;

U convert<T, U>(T value) {
    return /* a value of type U */;
}
```

`T add<T>(T a, T b) { return a + b; }` is valid. Operations on `T` are
checked when the function is instantiated: `add<int>(1, 2)` is accepted,
and `add<str>("a", "b")` is rejected because `str` does not support `+`.

The type parameter list introduces `T` and `U`; the return type and
parameters can refer to those parameters.

### 3.2 Generic classes

``` graphite
class Box<T> {
    T value;

    Box(T value) {
        this.value = value;
    }

    T get() {
        return this.value;
    }

    void set(T value) {
        this.value = value;
    }
}
```

Multiple parameters are supported:

``` graphite
class Pair<T, U> {
    T first;
    U second;

    Pair(T first, U second) {
        this.first = first;
        this.second = second;
    }
}
```

### 3.3 Generic methods

A method can declare its own type parameters, independently of any
class-level parameters.

``` graphite
class Converter {
    @static
    T identity<T>(T value) {
        return value;
    }
}

class Container<T> {
    T value;

    Container(T value) {
        this.value = value;
    }

    U transform<U>(U replacement) {
        return replacement;
    }
}
```

In `Container<T>`, `T` belongs to the class and `U` belongs to
`transform<U>`.

### 3.4 Generic use

``` graphite
int n = identity<int>(10);
Box<int> box = Box<int>(10);
Box<int> other = new Box<int>(10);
Pair<int, str> pair = Pair<int, str>(10, "Graphite");
Box<Pair<int, str>> nested = Box<Pair<int, str>>(pair);
```

Every use writes the type arguments. `identity(10)`, `Box(10)`, and
`new Box(10)` are compile-time errors.

### 3.5 Grammar sketch

This is a conceptual grammar to integrate with the existing parser, not
a complete grammar specification.

``` ebnf
TypeParameterList ::= "<" Identifier ("," Identifier)* ">"
GenericArguments  ::= "<" Type ("," Type)* ">"
GenericFunction   ::= ReturnType Identifier TypeParameterList
                      "(" ParameterList? ")" FunctionBody
GenericClass      ::= "class" Identifier TypeParameterList? ClassBody
GenericMethod     ::= ReturnType Identifier TypeParameterList
                      "(" ParameterList? ")" FunctionBody
GenericType       ::= TypeName "<" Type ("," Type)* ">"
```

Ordinary functions and classes must remain valid without a
type-parameter list. Since `<` and `>` can also be relational operators,
the parser must distinguish generic syntax using declaration, type, and
call context. It must not treat every `<` following an identifier as a
generic list.

`>>` stays the right-shift token in expressions. While parsing nested
type arguments, the parser reads one `>>` as two closing `>` delimiters.
`Box<Pair<int, str>>` is valid without a space. This is a parse-context
rule, not a lexer change.

## 4. Type-system model

### 4.1 Type parameters and scope

Each generic declaration introduces a scoped set of compile-time type
parameters. Type parameters are placeholders for types, not runtime
values.

-   Function type parameters are visible throughout that function's
    signature and body.
-   Class type parameters are visible throughout the class body,
    including fields, constructors, and instance methods.
-   Method type parameters are visible within that method.
-   A type parameter name is UpperCamelCase, such as `T` or `Element`.
    Any other spelling is a compile-time error.
-   Duplicate parameters in the same list are errors.
-   For the first release, reject a method-level parameter that repeats
    a class-level parameter name, avoiding ambiguous shadowing.
-   Internally, type parameters should have symbol identities/IDs rather
    than being represented only by their text names.

### 4.2 Generic type identity

`Box<int>` and `Box<str>` are distinct types. Represent a parameterized
type structurally, for example:

``` text
Box<int>       -> GenericType(Box, [int])
Pair<int, str> -> GenericType(Pair, [int, str])
```

The number of type arguments must match the number of parameters
declared by the generic type.

### 4.3 Type substitution

When a generic declaration is used with concrete arguments, substitute
those arguments for the corresponding parameters throughout the
signature and member types.

For `Box<int>`, `T -> int`; therefore, its `value` field has type `int`,
`get()` returns `int`, and `set` accepts an `int`. Substitution must
recurse through arrays, nullable types, nested generic types, and
callable signatures. For example, `T[]` becomes `int[]` and `T?` becomes
`int?` when `T = int`.

A type argument may be any concrete type Graphite already supports:
numbers, `bln`, `chr`, `str`, arrays, maps, records, classes,
interfaces, nullable types, callables, and other constructed generic
types.

### 4.4 Type compatibility

Use Graphite's existing type compatibility and conversion rules. Do not
invent new implicit conversions for generic calls. All occurrences of a
type parameter must resolve consistently.

## 5. Generic function calls

### 5.1 Explicit arguments

For:

``` graphite
T identity<T>(T value) {
    return value;
}
```

`identity<int>(10)` binds `T` to `int`, checks the argument against
`int`, and has result type `int`. Supplying type arguments to a
non-generic declaration is an error. Omitting them on a generic
declaration is also an error:

``` graphite
int a = identity<int>(10);  // valid
int b = identity(10);       // compile-time error
int n = parse<int>(text);   // valid
int m = parse(text);        // compile-time error
```

The explicit type argument determines each parameter. Every value
argument must then be compatible with the substituted parameter under
Graphite's existing conversion rules. A known expected type does not
fill in a missing type argument. `same<int32>(1, n)` is valid when `1`
is a valid `int32` and `n` is an `int32`. `same<int32>(n, "text")` is a
type error.

### 5.2 No inference in this version

The compiler does not infer type arguments from call arguments, from
literals, or from the expected type of the call. Inference, including
bidirectional inference through lambdas, is deferred.

A generic function is called with type arguments. It is not a value:

``` graphite
identity<int>(10);  // valid
identity<int>;      // compile-time error
```

Specializing a generic function and then storing that specialization as
a callable is a future extension.

## 6. Generic classes and members

### 6.1 Construction

``` graphite
Box<int> box = Box<int>(10);
Box<int> other = new Box<int>(10);
```

Both `Box<int>(arguments)` and `new Box<int>(arguments)` construct.
`Box(arguments)` and `new Box(arguments)` are compile-time errors. The
compiler does not infer class type arguments from the constructor
arguments or from the variable's type. If a function and a generic class
share a name, `Name<int>(arguments)` still constructs the class.

### 6.2 Member access

For a `Box<int>`, `get()` returns `int` and `set` accepts `int`.
`box.set("hello")` must fail at compile time.

### 6.3 Fields and nested types

Fields may use class type parameters, including nested forms such as
`T[]`, `T?`, or supported generic containers. Substitution must preserve
the complete type structure.

A field whose type is a type parameter has no default value. The
constructor must initialize it. `Box<T>` remains a heap reference type:
assignment copies the handle, and `==` compares identity, as for any
other class.

`typeOf` on a value whose type is a constructed generic class is a
compile-time error in this version. Existing `typeOf` support for
ordinary types stays unchanged.

A generic class cannot use `extends` or `implements`. A class cannot
extend or implement a constructed generic type such as `Box<int>`.
Generic records are not part of this version.

### 6.4 Static members

Static fields still cannot refer to a containing class's type
parameters: there is no per-specialization static storage in this
version.

Static methods may use class type parameters. At the call site, type
arguments are taken from an explicit list (`Util.forEach<int>(...)`) or
inferred from the value arguments when omitted
(`Util.forEach(intArray, ...)`).

``` graphite
class Box<T> {
    @static
    T defaultValue; // Error: static fields cannot use T.

    @static
    T identity(T value) {
        return value; // Valid: T comes from the call site.
    }
}

int n = Box.identity(10);       // infers T = int
int m = Box.identity<int>(10);  // explicit
```

## 7. Callable parameters and lambdas

Generic substitution must recurse through callable signatures. For
example:

``` graphite
U mapOne<T, U>(T value, U transform(T input)) {
    return transform(value);
}
```

The existing Graphite rules for lambda parameter types, callable
compatibility, and captures remain unchanged. A lambda passed to a
generic function uses its written parameter and return types. The call
still writes the generic type arguments explicitly.

## 8. Constraints and operations

### 8.1 Instantiation checking

Constraint syntax such as `T: Addable` is not part of this version.
Operations on type parameters are allowed in a generic body and are
checked when the body is instantiated with concrete type arguments.

``` graphite
T add<T>(T a, T b) {
    return a + b;
}

int n = add<int>(2, 4);   // valid: int supports +
str s = add<str>("a", "b"); // error: str does not support +
```

Each distinct call-site specialization is type-checked and lowered to a
concrete monomorphized function. A future LLD can add explicit
constraints so invalid operations are rejected at the declaration.

## 9. Rust code generation

### 9.1 Strategy

Preserve generic declarations in generated Rust where possible. Graphite
must perform its own name resolution, type-argument validation,
and type checking; Rust compilation alone is not an adequate
Graphite diagnostic system.

### 9.2 Function lowering

Each used specialization is emitted as a concrete Rust function.
`identity<int>(10)` lowers to a monomorphized `identity` over `i64`,
not a Rust `fn identity<T>`. That lets bodies with operators on `T`
type-check and compile after instantiation.

### 9.3 Class lowering

Graphite:

``` graphite
class Box<T> {
    T value;
}
```

Conceptual Rust structure:

``` rust
struct Box<T> {
    value: T,
}
```

Methods should be emitted in an appropriate `impl<T> ...` block. The
Rust type stays generic. Graphite does not monomorphize it before
emission. The value is still a traced heap handle, with the same
construction, assignment, and identity rules as a non-generic class.
The emitter supplies whatever Rust bounds are required to clone and
trace `T`. It does not lower `Box<T>` to a plain untraced struct.

### 9.4 Mapping

The emitter must map Graphite type parameters, generic arguments, nested
generic types, calls, and generic `impl` blocks to valid Rust syntax.
Existing Rust identifier escaping/mangling rules must be used where
needed. Generic parameters are compile-time constructs; runtime
descriptors, reflection, and dynamic type checks are not required.

## 10. Compiler pipeline changes

1.  **Lexer:** Reuse `<`, `>`, and `>>`. Expressions keep `>>` as a
    shift. The parser, not the lexer, splits `>>` while closing nested
    type arguments.
2.  **Parser/AST:** Represent type-parameter declarations, generic type
    references, explicit type arguments on calls/construction, and
    generic functions/classes/methods.
3.  **Symbol collection:** Register generic declarations according to
    existing declaration-order rules; create scopes and record parameter
    count, order, symbol identity, and source location.
4.  **Type resolution:** Resolve type parameters and concrete types;
    validate argument counts; represent generic types structurally.
5.  **Type checking:** Implement recursive substitution and explicit
    argument validation. Reject a generic call or construction that
    omits its type arguments. Reject unsupported operations on
    unconstrained parameters.
6.  **Rust emitter:** Emit generic declarations, arguments, and generic
    `impl` blocks.
7.  **Diagnostics/tests:** Report errors against Graphite source
    locations rather than relying only on generated Rust errors.

## 11. Diagnostics

Provide Graphite-level diagnostics for:

-   Wrong number of generic arguments, or a generic use with none.
-   Duplicate, out-of-scope, or non-UpperCamelCase type parameters.
-   Type arguments supplied to a non-generic declaration.
-   Argument/member type mismatch after substitution.
-   Operation that the concrete type arguments do not support (reported
    at the operation, attributed to the instantiation).
-   Static member referring to a containing class's type parameter.
-   `typeOf` applied to a constructed generic class value.
-   A generic function used as a value.
-   `extends` or `implements` on a generic class, or a parent type that
    is a constructed generic.
-   Parser ambiguity/regressions involving `<`, `>`, and `>>`.

Where practical, diagnostics should include expected and actual types
and the source location of the relevant declaration or call.

## 12. Standard-library enablement

Generics are intended to support reusable APIs such as: - mapping `T[]`
to `U[]`; - filtering `T[]` while preserving `T`; - generic containers
such as `Box<T>` and `Pair<T, U>`; - generic utility functions and
callable APIs.

This LLD does not define the complete `grf.array` or `grf.string` API.
Those should be designed after generic type checking and array
allocation/mutation semantics are stable.

## 13. Testing plan

### Positive tests

1.  Generic identity with explicit type arguments.
2.  Multiple type parameters.
3.  Generic class with one and multiple parameters.
4.  `Box<int>(...)` and `new Box<int>(...)`, fields, and member access.
5.  Generic method in a non-generic class.
6.  Generic method using both class-level and method-level parameters.
7.  Nested generic types, including `>>`, generic arrays, and `T?`.
8.  Generic callable parameter with explicitly typed lambdas.
9.  Valid Rust generation for all supported cases.

### Negative tests

1.  Wrong argument count, and a generic call or construction with no
    type arguments.
2.  Duplicate, out-of-scope, or lowercase type parameters.
3.  Type mismatch after substitution.
4.  Type arguments on a non-generic declaration.
5.  Unsupported operation on an unconstrained parameter, including `+`
    inside `add<T>` even when a call site passes `int`.
6.  Static member referring to class-level type parameter.
7.  `typeOf` on a generic class value, and a generic function used as a
    value.
8.  `<`, `>`, and shift `>>` still parse as operators in expressions.

### Regression tests

-   Existing non-generic functions/classes compile unchanged.
-   Existing lambda, callable, constructor, static method, visibility,
    and inheritance behavior remains unchanged except where explicitly
    specified.
-   Existing generated Rust fixtures continue to compile.

## 14. Implementation order

1.  Inspect the current lexer, parser, AST, symbol table, type
    representation, type checker, and Rust emitter.
2.  Add generic parameters to function and class declarations.
3.  Add parameterized type references and argument-count validation.
4.  Implement structural type substitution and generic member checking.
5.  Support explicit generic calls and generic class construction.
6.  Reject missing type arguments on calls and construction.
7.  Support generic methods and callable signature substitution.
8.  Add Rust generic code generation.
9.  Add diagnostics and positive, negative, and regression tests.
10. Use the implementation to design `grf.array` and `grf.string`.

## 15. Acceptance criteria

The implementation is complete when:

-   Generic functions, classes, and methods parse using the syntax in
    this document, including nested `>>` in type arguments.
-   Type parameters resolve within the correct scopes and use
    UpperCamelCase.
-   Every generic call and generic construction writes its type
    arguments. A missing list is a Graphite error.
-   `Box<int>(...)` and `new Box<int>(...)` both construct.
-   Members and callable signatures are substituted, and a `T` field is
    initialized by the constructor.
-   Operators and other operations on an unconstrained type parameter
    are rejected while checking the generic body.
-   `typeOf` on a constructed generic class, and a generic function used
    as a value, are rejected.
-   Supported generic programs produce valid Rust and keep class values
    as traced heap handles.
-   Existing non-generic code, comparisons, and shift expressions remain
    valid.
-   Tests cover positive, negative, and regression cases.

## 16. Deferred decisions

The following are intentionally not in this version:

-   Generic type inference of any kind.
-   Constraint syntax, interface/trait bounds, and operator constraints.
-   Specialization, and using a specialized generic function as a value.
-   Associated types and advanced type-level features.
-   Variance rules.
-   Generic inheritance, generic interfaces, and extending or
    implementing a constructed generic type.
-   Generic records.
-   `typeOf` and other runtime descriptors for constructed generic
    values.
-   Generic static members that depend on the class type arguments,
    beyond the prohibition in §6.4.

## 17. Locked initial rules

| Feature | Rule |
| --- | --- |
| Nested generics using `>>` | Supported by the parser while reading type arguments. In expressions, `>>` remains shift |
| Generic type inference | Not supported |
| Explicit generic function calls | Required |
| Explicit generic class construction | Required, both `Box<int>(...)` and `new Box<int>(...)` |
| Generic constraints | Deferred |
| Generic class semantics | Heap reference type, same as any other class |
| Generic fields of type `T` | No default; the constructor initializes them |
| Allowed type arguments | Any concrete type Graphite already supports, including arrays, maps, records, classes, nullables, and callables |
| Generic inheritance and interfaces | Deferred |
| Generic records | Deferred |
| `typeOf` on a constructed generic value | Compile-time error |
| Generic functions as values | Compile-time error |
| Type parameter names | UpperCamelCase, enforced |
| Generic body checking | Independent of call sites |
| Operators on an unconstrained type parameter | Compile-time error |

------------------------------------------------------------------------

**End of LLD**
