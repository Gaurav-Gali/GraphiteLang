# LLD-10 --- Stage 7: Named Custom Record Types

**Project:** Graphite Lang\
**Stage:** 7 --- Named custom structural types\
**Status:** Implementation specification\
**Depends on:** Stage 4 primitive types and universal `const`; Stage 5
unified conversions; Stage 6 arrays, indexing, slicing, destructuring,
and iteration.

## 1. Purpose

Implement named record types using `type` declarations. A record defines
a fixed set of named, typed fields. Values are constructed using record
literals and fields are accessed using dot notation.

``` grf
type User {
    name: str,
    age: int,
}

void main() {
    User user = {
        name: "Gaurav",
        age: 21,
    };

    const User fixedUser = {
        name: "Gaurav",
        age: 21,
    };

    io.println(user.name);
    user.age = 22;

    uint64 userFieldCount = lengthOf(fixedUser); // 2
    // fixedUser.age = 22; // Compile-time error
}
```

This stage introduces records, not classes. Methods, constructors,
inheritance, visibility modifiers, and user-defined generics are out of
scope.

## 2. Locked design decisions

1.  **Nominal identity:** Each declared record type has a unique
    semantic identity. Matching field layouts do not make two named
    types interchangeable.
2.  **Top-level declarations:** `type` declarations are top-level only
    in Stage 7.
3.  **Declaration-first analysis:** Collect all record type names before
    resolving field types and analyzing function bodies. Forward
    references are valid when the dependency graph is acyclic.
4.  **Complete construction:** A record literal must provide every
    declared field exactly once. Missing, unknown, duplicate, or wrongly
    typed fields are compile-time errors.
5.  **Expected type:** In expression position, the expected type selects
    the brace literal. An expected named record type makes it a record
    literal. An expected `map<K, V>` type makes it a map literal. Any
    other expected type, or no resolvable expected type, is a
    compile-time error. Do not infer an anonymous record or a map from
    the brace contents. A record literal still requires every declared
    field exactly once.
6.  **Value semantics:** Assignment, parameter passing, returns, and
    storing records in fields or collections copy values. No shared
    mutable alias may change Graphite semantics.
7.  **Universal `const`:** `const` prevents reassignment and mutation
    through that binding, including nested records and arrays/maps
    reached through it.
8.  **Field mutability:** Stage 7 has no field-level `const` modifier.
    Mutability is controlled by the root binding used to reach a field.
9.  **No recursive cycles:** Reject direct, indirect, and
    collection-mediated recursive record dependency cycles before code
    generation. Cycles through arrays and through map values are
    included. Map fields do not create an exception.
10. **`lengthOf`:** Returns `uint64` equal to the number of fields
    declared directly by the record's named type. It does not inspect
    field values or recursively count nested fields.
11. **No structural coercion:** A record of one named type cannot be
    assigned to another merely because the fields match.
12. **No dynamic typing:** All fields are statically typed. No `any` or
    arbitrary record conversions are introduced.
13. **Structured implementation:** Use AST nodes, a semantic record
    registry, resolved field identities, and typed expressions. Do not
    implement records using source rewriting or regex substitution.

## 3. Syntax

### 3.1 Type declarations

``` ebnf
type_declaration = "type", IDENTIFIER, "{", field_declaration_list, "}" ;
field_declaration_list = field_declaration, { ",", field_declaration }, [ "," ] ;
field_declaration = IDENTIFIER, ":", [ "auto" ], type_reference ;
```

Reuse the existing type-reference grammar, including supported
primitive, named, array, and map types. A trailing comma in a field
declaration list is accepted. `void` is not a valid field type. Empty
record declarations are rejected in Stage 7.

`height: auto int` stores `int`. Writes to that field, including record
literals and later assignments, use the existing `auto T` conversion
registry. `height: int` stays an ordinary typed slot and does not
convert. `auto` does not make one record type convertible to another.

``` grf
type Profile {
    name: str,
    scores: int[],
    labels: map<str, str>,
}

type User {
    name: str,
    scores: map<str, int>,
}

type Group {
    members: map<str, User>,
}
```

A field may use any supported non-`void` type: a primitive, an array, a
map, or another named record. A map field is accepted only when its key
and value types are valid under the map rules.

### 3.2 Record literals and member access

``` ebnf
record_literal = "{", [ field_initializer, { ",", field_initializer }, [ "," ] ], "}" ;
field_initializer = IDENTIFIER, ":", expression ;
member_access = expression, ".", IDENTIFIER ;
```

Integrate member access into the existing postfix-expression grammar so
it composes with indexing and calls where valid, such as
`users[0].name`.

In expression position, the expected type determines the brace-literal
kind:

-   Expected named record type: record literal.
-   Expected `map<K, V>` type: map literal.
-   Any other expected type, or no resolvable expected type:
    compile-time error.

An invalid record literal must not be reinterpreted as a map literal.
Do not infer a map or an anonymous record from the contents of the
braces. In statement position, `{}` remains a block.

When the expected type is a named record, the literal is validated as a
record literal: every declared field is required, unknown and duplicate
fields are rejected, and each initializer is checked against that
field's type.

## 4. Declaration scope, names, and forward references

-   `type` declarations are top-level declarations in Stage 7. A
    declaration inside a function or nested block is a compile-time
    error.
-   Type names use the existing type namespace and reserved-name rules.
-   Type and value namespaces remain separate; type references resolve
    through the type namespace.
-   Duplicate type names are errors.
-   Field names are scoped to their declaring record; different records
    may use the same field name.
-   Duplicate fields within a record are errors.
-   Field names must follow existing identifier and keyword rules.
-   Collect all record type names before resolving field types. A type
    may refer to a later-declared type if the dependency graph is
    acyclic.

Valid forward reference:

``` grf
type Team {
    lead: Person,
}

type Person {
    name: str,
}
```

Unknown named field types are compile-time errors. Do not invent
cross-module type-import behavior; use only import capabilities already
implemented by the compiler.

## 5. Semantic identity and type representation

Assign each record declaration a unique `RecordTypeId`. Assignability
uses this identity, not the name or the field layout. This is nominal
type identity. It does not define `==` or `!=` for record values.

Suggested conceptual structures (adapt to current code conventions):

``` rust
struct RecordTypeDecl {
    id: RecordTypeId,
    name: Identifier,
    fields: Vec<RecordFieldDecl>,
    source_span: Span,
}

struct RecordFieldDecl {
    name: Identifier,
    ty: Type,
    index: usize,
    source_span: Span,
}

enum Type {
    // Existing variants...
    Record(RecordTypeId),
}
```

Keep source AST distinct from resolved semantic types. Every resolved
field should have a stable index or handle and canonical type. A typed
member-access expression should reference the resolved record identity
and field handle, rather than repeating string lookup during code
generation.

`User` and `Customer` are incompatible even if both contain `name: str`
and `age: int`. No implicit record-to-record conversion exists. Existing
conversion rules must not introduce arbitrary record conversions.

## 6. Record construction validation

A brace literal in expression position is first classified by its
expected type, as in section 3.2. This section applies when that type is
a named record. The expected type may come from a variable declaration,
field initializer, function parameter, or function return type.

Validate all of the following:

-   The expected type is a named record type.
-   Every declared field is present.
-   No unknown field is supplied.
-   No field appears more than once.
-   Every initializer expression is assignable to its field's declared
    type under existing rules.
-   Every initializer expression is evaluated exactly once.
-   Initializer expressions are evaluated in source order; the resulting
    values are stored in declared field order.

Examples of compile-time errors:

``` grf
User missing = { name: "Ada" }; // Missing age
User unknown = { name: "Ada", age: 21, active: true }; // Unknown field
User duplicate = { name: "Ada", name: "Grace", age: 21 }; // Duplicate name
User wrong = { name: 123, age: "21" }; // Field type mismatches
```

No field defaults are introduced. Omitted fields are not
zero-initialized. If the expected type is not a named record, do not
validate the braces as a record literal and do not turn an invalid
record literal into a map literal. A map literal is chosen only when the
expected type is already `map<K, V>`. If there is no resolvable record
or map type, reject the literal. Do not infer an anonymous record or a
map from the brace contents.

An empty record declaration is invalid:

``` grf
type Empty {
} // Error: record types must declare at least one field
```

## 7. Field access and assignment

### 7.1 Reads

For `base.field`, validate that `base` has a named record type, the
field exists on that exact type, and the expression's type is the
field's declared type. Unknown fields are compile-time errors. Dot
access on primitive values, arrays, or maps is unsupported unless an
existing language feature explicitly defines it.

### 7.2 Writes

``` grf
user.age = 22;
user.name = "Grace";
```

Field assignment is valid only if the base path resolves to a record
field, the root binding is mutable, the field exists, and the assigned
expression is compatible with the declared field type. A field declared
`auto T` accepts a value that the existing conversion registry can
convert to `T`. Assignment through a const root, assignment to an
unknown field, or assignment of an incompatible value is a compile-time
error.

### 7.3 Nested values

Field access composes with existing array and map operations:

``` grf
type Profile {
    name: str,
    scores: int[],
}

void main() {
    Profile p = { name: "Ada", scores: [10, 20] };
    p.scores[0] = 99; // Valid: mutable root
}
```

The same mutation through `const Profile p` must be rejected. Const
checking must follow the full l-value path: `p.child.name`,
`p.scores[0]`, and `p.lookup["key"]` cannot mutate through a const root.
Do not check only the final field or collection and overlook
root-binding mutability.

## 8. Universal `const` and value semantics

`const User user = ...` prevents reassignment and mutation through that
binding. It does not create a new record type.

``` grf
const User user = { name: "Ada", age: 36 };
// user = other;    // Error
// user.age = 37;   // Error
```

Const applies recursively through nested records, arrays, and maps
reachable through that binding. Reads and `lengthOf()` remain allowed.

Record value semantics:

-   Assignment copies the record.
-   Passing a record to a function passes a value copy.
-   Returning a record returns a value.
-   Storing a record in another record or collection copies the value.
-   Mutating a mutable copy does not mutate the source.

``` grf
User first = { name: "Ada", age: 36 };
User second = first;
second.age = 37;
// first.age remains 36
```

Generated Rust must preserve value semantics for heap-backed nested
fields. Use clone/deep-value-copy behavior where needed; do not
introduce shared mutable references that make changes visible across
copies. Passing a const binding to a by-value parameter is allowed when
the types match, because the callee receives a copy. Stage 7 adds no
reference or borrow parameter syntax.

## 9. Recursive type detection

After resolving field type references, construct a dependency graph of
record types. A record depends on named record types reachable through
its fields, including through array element types and map value types.
A map field participates only when its key and value types are valid
under the map rules; a record, array, map, or float key remains
rejected by those rules. A record used as a map value still creates a
dependency. Reject every cycle in this graph, including a cycle that
passes through a map, before Rust code generation.

Rejected examples:

``` grf
type Node {
    next: Node,
}

type A {
    items: B[],
}

type B {
    parent: A,
}

type Directory {
    groups: map<str, Crew>,
}

type Crew {
    parent: Directory,
}
```

The diagnostic should identify the cycle or a useful field/type chain.
Do not rely on Rust's recursive-type rules to determine what Graphite
accepts. Recursive records may be reconsidered only in a future design
with explicit representation and ownership rules.

## 10. `lengthOf(record)`

`lengthOf` remains the reserved compiler built-in and returns `uint64`.
For a record, it returns the number of fields declared directly by that
named type. It works for mutable and const bindings, does not mutate the
record, does not inspect field values, and does not expose field
contents.

``` grf
type Address {
    city: str,
    country: str,
}

type Profile {
    name: str,
    address: Address,
}

void main() {
    Profile p = {
        name: "Ada",
        address: { city: "London", country: "UK" },
    };

    uint64 count = lengthOf(p); // 2, not 3
}
```

Rules:

-   Return type is `uint64`.
-   Count is fixed by the record type, independent of field values.
-   Count only direct fields; do not recursively count nested record
    fields.
-   Mutable and const instances of the same type return the same count.
-   `lengthOf` remains reserved and cannot be shadowed under existing
    rules.
-   Existing behavior remains consistent: strings count Unicode scalar
    values, arrays count elements, maps count entries, records count
    declared fields. Numeric and boolean inputs remain compile-time
    errors.

The result may be lowered to a constant after semantic resolution.
Preserve the established `lengthOf` argument-evaluation policy and never
duplicate expression evaluation.

## 11. AST, semantic analysis, and typed l-values

Add structured nodes using existing AST conventions:

``` rust
struct TypeDeclaration {
    name: Identifier,
    fields: Vec<FieldDeclaration>,
    span: Span,
}

struct FieldDeclaration {
    name: Identifier,
    type_ref: TypeRef,
    span: Span,
}

struct RecordLiteralField {
    name: Identifier,
    value: Expression,
    span: Span,
}

struct MemberAccessExpression {
    base: Box<Expression>,
    member: Identifier,
    span: Span,
}
```

The typed AST should preserve the resolved owner `RecordTypeId`, field
handle/index, field type, and l-value mutability. Use a shared
mutability/l-value analysis path for variable assignment, field
assignment, array element assignment, slice assignment, and map
mutation.

Required semantic pipeline:

1.  Parse all top-level declarations.
2.  Collect type names and allocate `RecordTypeId`s.
3.  Diagnose duplicate type names.
4.  Resolve field type references against the complete registry.
5.  Reject unknown or non-value field types, including `void`.
6.  Diagnose duplicate fields and empty record declarations.
7.  Detect recursive dependency cycles.
8.  Finalize each record's resolved field layout.
9.  Collect function signatures with the completed type registry.
10. Type-check function bodies and record literals.
11. Resolve member access to specific fields.
12. Validate assignment types and root-binding mutability.
13. Type-check `lengthOf(record)`.
14. Generate Rust only if no compile errors remain.

## 12. Rust code generation

Generate one Rust struct per Graphite record type, preserving field
declaration order and using the compiler's shared canonical type
mapping:

``` rust
struct User {
    name: String,
    age: i64,
}
```

This is illustrative; use actual existing mappings for strings, numeric
types, arrays, and maps. Maintain deterministic Graphite-to-Rust
identifier mapping and ensure emitted names are valid Rust identifiers.

-   Generate record literals with resolved fields and single evaluation
    in Graphite source order.
-   Lower `record.field` to the resolved Rust field access.
-   Emit field assignment only after semantic validation confirms a
    mutable target.
-   Generate or derive cloning as appropriate to preserve Graphite value
    semantics, including nested heap-backed fields.
-   Do not depend on Rust compiler errors for ordinary Graphite
    diagnostics.
-   If compilation fails, do not emit or run a new executable for that
    source. Ensure a stale executable is not presented as the result of
    a failed compilation.
-   Keep generated artifacts in the existing Graphite build directory;
    do not emit generated Rust beside the source file.

## 13. Diagnostics

Provide useful source spans and names for these errors:

-   Duplicate type name.
-   Type declaration in unsupported scope.
-   Unresolved field type.
-   Duplicate field name.
-   Empty record.
-   `void` or unsupported field type.
-   Direct, indirect, or map-mediated recursive cycle.
-   Brace literal with no resolvable record or map expected type.
-   Record `==` or `!=`.
-   Missing, unknown, or duplicate literal field.
-   Field initializer type mismatch.
-   Member access on non-record.
-   Unknown field.
-   Mutation through const root.
-   Incompatible field assignment.
-   Incompatible nominal record types.
-   Unsupported record conversion.
-   Invalid `lengthOf` argument.

Suppress derivative errors where possible, such as cascading errors
after an unresolved field type.

## 14. Testing plan

### Positive tests

1.  Declare and instantiate one-field and multi-field records.
2.  Resolve a forward reference to a later type.
3.  Pass records to functions and return records.
4.  Read fields using dot notation.
5.  Mutate a field through a mutable binding.
6.  Mutate nested record fields and array/map values through a mutable
    root.
7.  Copy a record and verify mutation of the copy does not change the
    source.
8.  Read fields from a const record.
9.  Verify direct and nested mutation through const is rejected.
10. Verify `lengthOf` returns the direct field count for mutable and
    const values.
11. Verify nested fields are not recursively counted.
12. Verify same-layout named types remain incompatible.
13. Verify initializer expressions execute exactly once and in source
    order.
14. Verify record values work in existing function calls, returns, and
    local declarations.

### Negative tests

1.  Duplicate type declaration.
2.  Type declaration inside a function or block.
3.  Unknown field type.
4.  Duplicate field declaration.
5.  Empty record declaration.
6.  `void` field.
7.  Direct recursion, indirect recursion, and cycles through
    arrays/maps.
8.  Brace literal with no resolvable record or map expected type.
9.  Missing, unknown, duplicate, or wrongly typed literal field.
10. Unknown field access.
11. Member access on primitive, array, or map.
12. Field assignment through const.
13. Nested collection mutation through const record.
14. Assign one named record type to another with identical fields.
15. Attempt record conversion through `auto` or explicit conversion.
16. Record `==` and `!=`.
17. `lengthOf` on numeric/boolean values.
18. Failed compilation must not run a stale executable.

### Regression coverage

Run the existing suites for primitive types/functions, forward function
references, universal `const`, Stage 5 conversion registry and `auto T`,
arrays/indexing/slicing/destructuring/iteration, maps and map mutation,
`lengthOf(str)` and `lengthOf(array)`, CLI build/run behavior, and
generated-artifact isolation.

## 15. Acceptance criteria

Stage 7 is complete when:

-   The parser emits structured type declarations, record literals, and
    member-access expressions.
-   Type names are collected before field resolution; valid forward
    references work.
-   Every record has unique nominal identity.
-   Construction and field operations are statically checked before Rust
    code generation.
-   Universal `const` blocks direct and nested mutation through const
    record bindings.
-   Assignment, calls, and returns preserve value semantics.
-   All recursive dependency cycles are rejected.
-   `lengthOf(record)` returns the number of directly declared fields as
    `uint64`.
-   No class features or anonymous structural typing are introduced.
-   Existing tests pass, and invalid programs do not generate or run a
    new executable.

## 16. Explicitly out of scope

-   Methods, constructors, inheritance, interfaces/traits, and
    polymorphism.
-   Visibility modifiers and field-level `const`.
-   User-defined generic types.
-   Anonymous structural types or structural record compatibility.
-   Default or optional fields.
-   Recursive records.
-   References, pointers, or explicit borrow syntax.
-   Reflection exposing field names or values.
-   Record `==` and `!=`. Comparing records by fields or by identity is
    unsupported in this stage. The compiler rejects both operators with
    a compile-time diagnostic. Ordering comparisons of records are also
    unsupported.
-   Serialization/deserialization.
-   New cross-module import behavior.

## 17. Implementation checklist

-   [x] Add type-declaration AST/parser support.
-   [x] Add record-literal AST/parser support.
-   [x] Add member-access AST/postfix parsing.
-   [x] Add record registry and `RecordTypeId`.
-   [x] Implement declaration collection and forward type resolution.
-   [x] Validate field declarations and field types.
-   [x] Detect recursive dependency cycles.
-   [x] Implement nominal identity and assignability.
-   [x] Validate record literals and source-order single evaluation.
-   [x] Implement field reads/writes and nested l-value analysis.
-   [x] Enforce universal `const` recursively through fields and
    collections.
-   [x] Preserve value-copy semantics across assignments, calls,
    returns, and nested storage.
-   [x] Extend `lengthOf()` to records.
-   [x] Add Rust struct and field code generation.
-   [x] Add positive, negative, and regression tests.
-   [x] Verify diagnostics and stale-build behavior.
-   [x] Update the language specification and stage tracker after tests
    pass.

## 18. Resolved implementation decisions

Stage 7 implements named records only.

-   Field types are any supported non-`void` type: primitives, arrays
    (including bounded and nested arrays), maps, and other records. A
    map field is accepted only when its key and value types are valid
    under the map rules. Map values that are records participate in
    cycle detection, with no exception for map-mediated cycles.
-   A field written `T` is an ordinary typed slot. A field written
    `auto T` stores `T` and applies the existing `auto T` conversion
    registry on initialization and assignment. That does not add a
    conversion between two record types.
-   `User(value)` is a function call, not a constructor. A function and
    a record may share a spelling. `type` is a reserved keyword.
-   Record names are collected before field resolution and function
    bodies, so a function may return a record declared later in the
    file. Dependency cycles, including cycles through arrays and through
    map values, are compile-time errors.
-   In expression position, the expected type selects the brace literal:
    a named record type selects a record literal, and `map<K, V>`
    selects a map literal. Any other expected type, or no resolvable
    expected type, is a compile-time error. An invalid record literal is
    not reinterpreted as a map literal, and the brace contents do not
    infer an anonymous record or a map. A statement `{}` stays a block.
    Trailing commas are allowed. Record `==` and `!=` are rejected at
    compile time. Records are not compared by fields or by identity.
-   Field writes require an l-value rooted at a mutable binding. A const
    root blocks the whole path. Copying a const record into a mutable
    binding copies the value, including nested arrays and records.
-   `io.println`, `str(record)`, and interpolation display a record as
    `{ field: value, field: value }` in declaration order. Field values
    use the same text as `str(field)`. Strings and characters are
    quoted. This display is not a record parser and does not make two
    record types convertible.
