# LLD 11 --- Stage 8: Statically Typed Maps, Indexing, APIs, and Spread

**Status:** Implementation specification\
**Target:** Graphite v0.1\
**Roadmap stage:** Stage 8\
**Prerequisites:** Lexer/parser foundation, typed AST, semantic
analysis, Rust code generation, runtime error handling, Stage 5 unified
conversion system, Stage 6 arrays/iteration/`const`/`lengthOf()`, and
Stage 7 named record types.

## 1. Purpose

Specify homogeneous, statically typed maps with explicit key and value
types, checked lookup, default lookup, mutation, typed key/value/entry
access, loop iteration, map literals, and spread.

All map features must be represented in Graphite's structured AST and
typed AST. Do not implement maps by rewriting source text, using regular
expressions, injecting source fragments before parsing, or adding
isolated compiler special cases for each method.

Built-in type parameters do not enable user-defined generics. The
language supports the built-in `map<K, V>` type described here; this LLD
does not add generic functions, generic records, or user-defined generic
types.

## 2. Goals and non-goals

### Goals

-   Define `map<K, V>` and map literal syntax.
-   Statically validate every key and value expression.
-   Support key-based indexing for reads and writes.
-   Define missing-key behavior for checked lookups and removal.
-   Provide `get`, `getOrDefault`, `containsKey`, `set`, `remove`,
    `length`, `keys`, `values`, and `entries`.
-   Support typed iteration over keys, values, and key/value entries
    where compatible with the existing loop model.
-   Support map spread and deterministic evaluation/overwrite rules.
-   Extend `lengthOf()` to maps and keep it consistent with
    `map.length()`.
-   Enforce `const` through every direct and nested mutation path.
-   Specify key equality/hash requirements, value semantics,
    diagnostics, tests, and acceptance criteria.

### Non-goals

-   User-defined generics or generic functions.
-   `any`, heterogeneous maps, or implicit dynamic typing.
-   Positional indexing, negative indexing, or slicing for maps.
-   Lambda-based filtering, mapping, or other callback APIs.
-   A general exception-handling syntax.
-   A public tuple type or a user-defined iterator protocol.
-   A guarantee of map iteration order.
-   Implicit conversions between a map's declared key/value types and
    expressions used with the map.
-   Map comprehensions.
-   Concurrent mutation or thread-safe shared maps.

## 3. Map types

### 3.1 Syntax

A map type has an explicit key type and value type:

``` grf
map<str, int> scores;
map<int, str> labels;
map<str, int[]> histories;
map<str, User> users;
```

`K` and `V` are placeholders in this LLD, not literal source
identifiers. Both type arguments must be valid, fully resolved Graphite
types. Resolve aliases before checking type identity; for example, `int`
and `int64` identify the same canonical type.

The parser must recognize the comma between type arguments and the
closing `>` without confusing it with comparison operators. Follow the
parser's existing type-context disambiguation approach; do not add
whitespace requirements to make the grammar work.

### 3.2 Static typing

A map is homogeneous: every key has canonical type `K`, and every value
has canonical type `V`.

``` grf
map<str, int> scores = {
    "Ada": 10,
    "Linus": 20,
};
```

The following are compile-time errors:

``` grf
map<str, int> badKey = {
    "Ada": 10,
    20: 30,               // key is not str
};

map<str, int> badValue = {
    "Ada": 10,
    "Linus": "twenty",    // value is not int
};
```

Do not use `map<K, any>` or another hidden escape hatch. Do not silently
coerce keys or values. Apply only conversions explicitly allowed by the
existing Graphite conversion rules and only where that expression form
already permits target-directed conversion. In particular, ordinary
typed map assignment and map entry typing must not gain a new implicit
conversion rule.

### 3.3 Supported key types and hashability

Map keys must have stable equality and hashing semantics. The initial
supported key types are:

-   `str`
-   `bln`
-   `chr`
-   All supported signed and unsigned integer types, including `int16`,
    `int32`, `int64`, `uint16`, `uint32`, and `uint64`, plus their
    existing aliases.

Aliases are canonicalized before map type comparison and key operations.

Floating-point keys are rejected in this stage because NaN and
floating-point equality make ordinary equality/hash expectations
unsuitable without a separate language-level policy. Arrays and maps are
not valid keys. Named records are not valid keys in Stage 8 unless the
compiler already has an explicit, correct, stable structural
hashing/equality implementation for records; do not synthesize one
implicitly. The default Stage 8 implementation must reject record keys
with a clear compile-time diagnostic.

This restriction applies to keys, not values. Values may be any
supported non-`void` Graphite type that can be stored and copied under
the existing type/value rules, including primitives, arrays, and named
records. A map cannot have `void` as either key or value type.

### 3.4 Equality and hash contract

For every supported key type:

-   Key equality is Graphite equality for that canonical key type.
-   Hashing must be consistent with equality: equal keys must produce
    equal hashes.
-   Inserting a key equal to an existing key addresses the same logical
    entry.
-   Do not compare keys by source spelling, object address, or generated
    Rust debug text.
-   String keys use string value equality, not pointer identity.
-   Integer aliases share the same canonical key semantics.
-   The implementation must not expose hash values as language-visible
    values.

The Rust backend may use `std::collections::HashMap` with an appropriate
hasher, provided generated code preserves these Graphite semantics.

## 4. Map literals

### 4.1 Syntax

A map literal is a brace-delimited sequence of `key: value` entries
and/or spread entries:

``` grf
map<str, int> scores = {
    "Ada": 10,
    "Linus": 20,
};

map<str, int> empty = {};
```

A map literal requires an expected `map<K, V>` type from a declaration,
assignment target, function parameter, return type, record field, or
another supported typed context. Do not infer a map's key/value types
from a heterogeneous or ambiguous set of entries. An empty map literal
`{}` is valid only when the expected type supplies both `K` and `V`.

The same brace syntax is shared with record literals. The expected type
selects the literal kind:

-   Expected named record type: record literal. Keys must be field names.
-   Expected `map<K, V>`: map literal. Keys are expressions of type `K`.
-   No expected record or map type: compile-time error.
-   `{}` in statement position remains a block.

Do not infer an anonymous record or map type from the entries. Empty
`{}` is a valid map only when the expected type supplies both `K` and
`V`. The spread form `...scores` is valid only inside a map literal.
Reject it in array literals, record literals, and ordinary expressions.

Trailing commas are permitted in map literals:

``` grf
map<str, int> scores = {
    "Ada": 10,
    "Linus": 20,
};
```

### 4.2 Entry validation

For each ordinary entry `keyExpr: valueExpr`:

1.  Type-check `keyExpr` against the map's declared key type `K`.
2.  Type-check `valueExpr` against the map's declared value type `V`.
3.  Reject a key or value whose canonical type is incompatible with the
    declared type.
4.  Evaluate the key expression once, then the value expression once,
    from left to right.
5.  Insert or replace the corresponding entry in the map being
    constructed.

The map literal must not create an untyped intermediate map and infer a
broader type from later entries.

### 4.3 Duplicate keys

Duplicate keys are allowed. The later evaluated entry wins:

``` grf
map<str, int> scores = {
    "Ada": 10,
    "Ada": 11, // final value for "Ada" is 11
};
```

This rule applies consistently to explicit entries and spread entries.
Do not make duplicate explicit keys a compile-time error while allowing
spread collisions; the specified rule is last-write-wins.

### 4.4 Evaluation order

Map literal evaluation is strictly left-to-right in source order:

-   Evaluate each entry's key expression, then its value expression.
-   For a spread entry, evaluate the source-map expression exactly once
    at the point where the spread appears, then copy its entries into
    the destination map.
-   Continue with the next literal entry only after the preceding
    entry/spread has completed.
-   If a later entry or spread contains a key equal to one already
    inserted, replace the previous value.
-   If evaluation fails, do not continue evaluating later entries. No
    partially constructed map is returned to the caller.

Do not rely on the iteration order of the source map when spreading it;
the final logical mapping is determined by key equality and
last-write-wins semantics, not by the source map's enumeration order.

## 5. Indexing

### 5.1 Read

``` grf
map<str, int> scores = {"Ada": 10};
int ada = scores["Ada"];
```

`m[key]` is a checked lookup expression whose static result type is the
map's value type `V`. The key expression must have the map's canonical
key type `K`.

Reading a missing key raises a defined runtime `MissingMapKey` error. It
must not return zero, `null`, an arbitrary value, or a fabricated
default.

### 5.2 Write

``` grf
map<str, int> scores = {"Ada": 10};
scores["Ada"] = 11;    // update
scores["Grace"] = 30;  // insert
```

An index assignment inserts the key if absent or updates the value if
present. The key must match `K`; the assigned expression must match `V`
under the existing assignment rules. The operation is a mutation and is
rejected when the map is accessed through a `const` binding or through a
path rooted at a `const` binding.

Map index assignment is a statement/l-value operation. Do not permit
assignment to a temporary map expression.

### 5.3 No positional semantics

Maps are not positional sequences:

-   Negative keys have no special meaning.
-   Integer keys are ordinary key values, not array indexes.
-   `m[-1]` is a lookup for key `-1` only when the map's key type is
    compatible with that integer type.
-   Maps do not support positional slicing.
-   Array bounds/index normalization rules do not apply to maps.

## 6. Map API

The following APIs are built-in map operations, not user-defined generic
functions. Method names are case-sensitive.

  -------------------------------------------------------------------------------------
  Operation                             Static result           Behavior
  ------------------------------------- ----------------------- -----------------------
  `m[key]`                              `V`                     Checked lookup; missing
                                                                key raises
                                                                `MissingMapKey`.

  `m.get(key)`                          `V`                     Same checked lookup and
                                                                missing-key behavior as
                                                                `m[key]`.

  `m.getOrDefault(key, defaultValue)`   `V`                     Returns stored value if
                                                                present; otherwise
                                                                returns `defaultValue`
                                                                without mutation.

  `m.containsKey(key)`                  `bln`                   `true` if the key
                                                                exists, else `false`.

  `m.set(key, value)`                   `void`                  Inserts or replaces an
                                                                entry.

  `m.remove(key)`                       `bln`                   Removes the entry and
                                                                returns `true` if it
                                                                existed; returns
                                                                `false` if absent.

  `m.length()`                          `uint64`                Number of entries.

  `m.keys()`                            `K[]`                   Snapshot array of keys.

  `m.values()`                          `V[]`                   Snapshot array of
                                                                values.

  `m.entries()`                         Internal typed entry    Snapshot iterable of
                                        iterable                `(K, V)` pairs for
                                                                loop-header
                                                                destructuring; not a
                                                                public tuple value.

  `lengthOf(m)`                         `uint64`                Number of entries,
                                                                equal to `m.length()`.
  -------------------------------------------------------------------------------------

### 6.1 `get` and checked lookup

`get(key)` has exactly the same missing-key behavior as `m[key]`. Do not
implement it as a defaulting lookup or return an optional type in this
stage.

### 6.2 `getOrDefault`

``` grf
int unknown = scores.getOrDefault("Unknown", 0);
```

The `defaultValue` expression must be type-compatible with `V` under the
existing assignment and conversion rules. It is evaluated exactly once
on every call, whether or not the key exists. If the key exists, return
the stored value and discard the evaluated default. If it does not
exist, return the default. The method does not insert the default,
change the map's length, or mutate the map. If evaluating the default
fails, that failure propagates even when the key exists, because the
expression is always evaluated.

A present value equal to a default-like value is still present:

``` grf
map<str, int> counts = {"seen": 0};
int result = counts.getOrDefault("seen", 99); // 0, not 99
```

`getOrDefault` is permitted on a `const` map.

### 6.3 `containsKey`

`containsKey(key)` returns `true` only when a matching key exists. It
does not mutate the map and is permitted on `const` maps.

### 6.4 `set`

`set(key, value)` has the same insertion/update semantics as
`m[key] = value` and returns `void`. It is rejected for a `const` map.
Key and value expressions are each evaluated exactly once, with key
evaluated before value.

### 6.5 `remove`

`remove(key)` returns `bln`:

-   `true` if the key existed and was removed.
-   `false` if the key was absent; no error is raised in this case.

It is a mutation and is rejected for a `const` map. Evaluate the key
once. Removing a missing key must not change the map's contents or
length.

### 6.6 `length` and `lengthOf`

Both `m.length()` and `lengthOf(m)` return `uint64` and report the
number of entries, not capacity or allocated storage. They are read-only
and valid for `const` maps. Their results must always agree.

### 6.7 `keys()` and `values()`

`keys()` returns a newly allocated `K[]` containing each key once.
`values()` returns a newly allocated `V[]` containing the value
corresponding to each key in the same enumeration pass. Both are
snapshots: modifying the returned array does not mutate the map, and
subsequent map changes do not alter an already returned array.

Because map order is unspecified, neither result has a guaranteed order.
For `values()`, each value corresponds to the key at the same position
in the result of the same enumeration pass only if the implementation
explicitly produces both from one shared traversal; independent calls to
`keys()` and `values()` must not be assumed to align positionally. Do
not write tests that rely on any order unless the test
sorts/canonicalizes the results first.

These methods are read-only and valid on `const` maps.

### 6.8 `entries()`

`entries()` returns an internal typed iterable whose element type is the
pair `(K, V)`. This pair is a compiler/runtime iteration representation,
not a newly introduced public tuple type. It is intended for the
existing loop-header binding mechanism:

``` grf
loop (str key, int value = scores.entries()) {
    io.println(key);
    io.println(value);
}
```

The compiler must statically verify that the first loop variable has
type `K` and the second has type `V`. The iterable is a snapshot of the
map entries at the point `entries()` is evaluated, so later mutation of
the original map does not change the active iteration sequence.
Iteration order is unspecified.

If the current parser or typed loop representation cannot express this
without destabilizing existing array loops, implement the smallest
shared typed-iterable extension and update the loop LLD/tests. Do not
implement `entries()` as a public string encoding or a heterogeneous
array. If loop-header destructuring cannot be supported in the current
stage, the compiler must reject this form clearly rather than generating
malformed Rust; however, implementation acceptance for Stage 8 requires
the shown entry-loop form.

### 6.9 `iter()` compatibility

Maps do not gain a generic `iter()` method that ambiguously yields
either keys or values. Use `keys()`, `values()`, or `entries()` to
select the intended iteration shape explicitly.

Where the existing loop model supports a single binding over an array
expression, these are valid:

``` grf
loop (str key = scores.keys()) {
    io.println(key);
}

loop (int value = scores.values()) {
    io.println(value);
}
```

Map iteration must use the snapshot semantics defined above.

## 7. Map spread

### 7.1 Syntax

Spread is permitted inside a map literal:

``` grf
map<str, int> moreScores = {
    ...scores,
    "Grace": 31,
    "Dennis": 25,
};
```

A spread entry has syntax `...sourceMap`. It must appear in a map
literal and its expression must have a map type. It is not valid in
array literals or ordinary expressions in this stage.

### 7.2 Type compatibility

The source map's key and value types must be compatible with the
destination `map<K, V>` according to canonical type identity and
existing assignment compatibility. Do not introduce implicit per-entry
conversion during spread. A map with an incompatible key or value type
is a compile-time error.

### 7.3 Copy and overwrite behavior

Spread copies entries into the newly constructed destination map. It
does not mutate the source map and does not alias the source map's
mutable storage.

Entries are processed in source order. Later entries override earlier
ones:

``` grf
map<str, int> scores = {
    "Ada": 10,
    "Grace": 20,
};

map<str, int> updated = {
    ...scores,
    "Grace": 31, // overrides spread value
};
```

Multiple spreads are permitted. If the same key appears in multiple
spreads or explicit entries, the last processed entry wins. Duplicate
explicit keys follow the same rule.

For a value type with value semantics (including arrays and records),
spread must preserve Graphite's value-copy behavior. Do not expose
shared mutable aliases through a spread result.

## 8. `const` and mutation rules

A `const` map binding may be read but not mutated through that binding.
The following are rejected:

``` grf
const map<str, int> fixed = {"Ada": 10};

fixed["Ada"] = 11;          // compile-time error
fixed.set("Ada", 11);       // compile-time error
fixed.remove("Ada");        // compile-time error
```

Read-only operations remain valid:

``` grf
int ada = fixed.getOrDefault("Ada", 0);
bln exists = fixed.containsKey("Ada");
uint64 count = lengthOf(fixed);
str[] names = fixed.keys();
int[] values = fixed.values();
```

Apply const checks to the full l-value/access path, not only to direct
map identifiers. If a map is reached through a field or array element
rooted in a const binding, mutation through that path is rejected. For
example, if `const` rules already reject mutation of nested
arrays/records, they must reject `fixed.childMap["key"] = value` and
`fixed["key"].field = value` under the same root-const rule.

A copy made from a const map is independent under Graphite's established
value semantics. A mutable destination may be changed without changing
the const source.

The compiler must enforce const statically. Do not rely on a runtime
wrapper as the only enforcement mechanism.

## 9. Value semantics and ownership

Graphite's existing array and record decisions specify value semantics:
assignments, argument passing, returns, and slicing/copy operations
produce independent values rather than shared mutable aliases. Maps must
follow the same rule.

-   Assigning a map copies its entries and recursively copies values
    according to the established Graphite value-copy semantics.
-   Passing a map to a function and returning a map preserve value
    semantics.
-   A map literal spread copies entries and values.
-   `keys()` and `values()` return independent snapshot arrays.
-   `entries()` captures a snapshot for iteration.
-   Mutating a copied map must not mutate the original map, and vice
    versa.

Do not add implicit reference semantics, shared mutable storage, or
reference-counted aliasing as a shortcut. The Rust implementation may
use internal ownership mechanisms to implement copying, but externally
observable Graphite behavior must match the value semantics above.

For custom record values, support a record as `V` only if the Stage 7
record representation and copy semantics support independent value
copies. Nested arrays and records inside map values must retain their
existing recursive copy behavior. Record values do not become valid keys
merely because they can be stored as values.

## 10. Runtime errors and diagnostics

### 10.1 Runtime errors

At minimum, define and test:

-   `MissingMapKey`: checked lookup (`m[key]` or `m.get(key)`) for a
    missing key. `m[key]` and `m.get(key)` use the same diagnostic.
-   The diagnostic names the source location and the canonical map type.
    It does not include the key's value, for any key type:

    ```text
    Runtime map error at mintest.grf:5:5:
    missing key: lookup on 'map<str, int64>'
    ```
-   `getOrDefault` does not raise `MissingMapKey` for absence.
-   `remove` does not raise an error when the key is absent.
-   Runtime failure during expression evaluation follows the existing
    runtime error propagation behavior; do not continue evaluating later
    map literal entries after a failure.

Do not introduce new source-level `try/catch` syntax as part of this
LLD.

### 10.2 Compile-time diagnostics

Provide actionable diagnostics for at least:

-   Map type missing a key or value type argument.
-   Unsupported or non-hashable key type.
-   `void` key/value type.
-   Map literal without an expected map type.
-   Key type mismatch in a literal, index, API call, or spread.
-   Value type mismatch in a literal, assignment, `set`, or spread.
-   `getOrDefault` default type mismatch.
-   Invalid method name or argument count.
-   Mutation through `const`.
-   Map slicing or positional-only operation.
-   `entries()` used outside supported iteration context.
-   Invalid loop destructuring types for `entries()`.
-   Attempted map index assignment through a temporary expression.

Diagnostics should identify the source span and expected/actual
canonical types where the existing diagnostic framework permits it.

## 11. AST, typed AST, semantic analysis, and code generation

### 11.1 AST nodes

Add structured AST representation for:

-   Map type `map<K, V>`.
-   Map literal entry (explicit key/value pair).
-   Map spread entry.
-   Map index expression.
-   Map index assignment/l-value.
-   Built-in map method call (or a shared built-in collection
    method-call representation).

Do not encode map behavior as a string-valued special case in the parser
or perform textual substitution after parsing.

### 11.2 Typed representation

The typed AST must retain:

-   Canonical key type `K`.
-   Canonical value type `V`.
-   Whether an access is read-only or mutating.
-   Whether the receiver is a mutable l-value or a temporary.
-   Expected map type propagated into map literals.
-   Iterable item shape for `keys`, `values`, and `entries`.
-   The map operation kind for runtime/codegen dispatch.

### 11.3 Semantic analysis

Semantic analysis must:

1.  Resolve both type arguments and reject invalid types.
2.  Validate key hashability/equality support.
3.  Type-check map literals against expected types.
4.  Check every key/value expression.
5.  Resolve map index and built-in method types.
6.  Enforce missing-key operation distinctions in the typed operation.
7.  Enforce const and l-value rules for all mutation paths.
8.  Check spread type compatibility.
9.  Propagate typed iteration item types into loop bindings.
10. Extend `lengthOf()` overload resolution for maps without regressing
    existing arrays and supported types.

Do not add a separate type conversion system for maps. Use the
compiler's canonical type identity and existing conversion policy.

### 11.4 Rust code generation

The Rust backend represents maps with `std::collections::HashMap<K, V>`
and Rust's default randomized hasher. Do not introduce a deterministic
hasher to make printing or iteration stable. Iteration order is
unspecified and may change between runs.

Code generation must:

-   Generate checked lookup for `m[key]` and `get`, converting absence
    into `MissingMapKey`.
-   Generate correct insert/update behavior for index assignment and
    `set`.
-   Generate boolean removal result.
-   Generate read-only membership, default lookup, and length
    operations.
-   Generate snapshot arrays and snapshot entry iteration.
-   Generate left-to-right map literal construction and last-write-wins
    behavior.
-   Generate spread as copying, not moving or aliasing the source.
-   Preserve value semantics for nested arrays/records.
-   Respect `const` and l-value decisions already made by semantic
    analysis.

Avoid generated code that relies on unspecified HashMap enumeration
order for correctness. Do not expose Rust's `Option<V>` or `HashMap`
implementation details as Graphite types.

### 11.5 Runtime representation and error model

Integrate `MissingMapKey` into the existing Graphite runtime error
mechanism. If the current runtime uses `Result`/error propagation,
follow that mechanism. Do not invent a second independent runtime-error
architecture for maps.

The exact Rust helper function names are implementation details; their
semantics and tests are mandatory.

## 12. Testing plan

### 12.1 Positive compile-and-run tests

1.  Declare and read `map<str, int>`.
2.  Empty map with explicit expected type.
3.  Insert a missing key using index assignment.
4.  Update an existing key using index assignment.
5.  `get` returns the stored value.
6.  Missing `get` and missing index lookup produce the defined runtime
    error.
7.  `getOrDefault` returns stored values and defaults without mutation.
8.  A stored zero/default-like value is distinguished from an absent
    key.
9.  `containsKey` before and after insertion/removal.
10. `set` inserts and updates.
11. `remove` returns `true` for present keys and `false` for absent
    keys.
12. `length()` and `lengthOf()` agree after each mutation.
13. `keys()` and `values()` return snapshots.
14. `entries()` loop binds `K` and `V` with correct static types.
15. Map spread copies source entries.
16. Explicit entries after a spread override prior values.
17. Later spreads override earlier spreads.
18. Duplicate explicit keys use last-write-wins.
19. Map assignment and spread preserve independent value semantics.
20. Primitive key equality works by value.
21. Nested array/record values work where existing copy semantics
    permit.
22. All read-only operations work through `const` maps.
23. `keys()`, `values()`, and `entries()` can be consumed without
    relying on iteration order.
24. Integer-key maps treat negative integer keys as ordinary keys, not
    negative positions.

### 12.2 Negative compile tests

Reject:

-   Missing map type arguments.
-   `map<void, int>` and `map<str, void>`.
-   `map<flt, int>` and other unsupported key types.
-   `map<int[], str>` and `map<map<str, int>, str>` as key types.
-   A map literal without a resolvable expected type.
-   Heterogeneous key/value types.
-   Wrong key type in index or method call.
-   Wrong value type in assignment or `set`.
-   Incompatible `getOrDefault` default.
-   Incompatible spread source.
-   Mutating a const map via index assignment, `set`, or `remove`.
-   Mutating a nested map through a const-rooted path.
-   Map slicing.
-   Entry destructuring with wrong variable types/count.
-   Index assignment on a temporary map.
-   Invalid method name or argument count.

### 12.3 Runtime and regression tests

-   Missing-key errors are consistent between `m[key]` and `m.get(key)`.
-   `getOrDefault` does not insert a missing key or change `length()`.
-   `remove` of a missing key leaves the map unchanged.
-   Later duplicate entries and spreads override earlier values.
-   Map literal expressions are evaluated in source order exactly once.
-   Spread does not mutate its source.
-   Snapshot iteration is unaffected by later mutations to the original
    map.
-   Map copies do not alias mutable values.
-   Existing array iteration/destructuring and record behavior remain
    unchanged.
-   Existing `lengthOf(array)` behavior remains unchanged.
-   Existing numeric compatibility and conversion tests remain
    unchanged.

Do not assert an ordering of map keys, values, or entries. If an
expected result must compare all elements, sort/canonicalize the test
output or compare it as an unordered collection.

## 13. Implementation sequence

1.  Inspect the existing lexer, parser, type representation, typed AST,
    semantic analyzer, l-value/const checks, collection code generation,
    runtime errors, loop binding, and `lengthOf()` implementation.
2.  Report the actual files and extension points before changing code.
3.  Add map type parsing and type representation.
4.  Add map literals and expected-type propagation.
5.  Add key type/hashability checks.
6.  Add index read/write and const/l-value checks.
7.  Add built-in API typing and code generation.
8.  Add `lengthOf(map)`.
9.  Add snapshot `keys()`, `values()`, and `entries()`
    iteration/destructuring.
10. Add spread and evaluation-order tests.
11. Add runtime missing-key handling and regression tests.
12. Update language documentation and this LLD with any
    implementation-driven changes; do not silently change the locked
    semantics.

Keep parsing, semantic analysis, typed AST, code generation, runtime
helpers, and CLI behavior separated. Do not implement the feature with a
single large parser/compiler branch.

## 14. Acceptance criteria

Stage 8 is complete when:

-   `map<K, V>` is statically typed with no `any` escape hatch.
-   All supported key types have documented equality/hash behavior.
-   Index reads, writes, and API calls enforce exact declared types
    under existing conversion rules.
-   Missing-key behavior is defined and tested.
-   `getOrDefault` is read-only and does not insert.
-   `remove` has predictable boolean semantics.
-   `lengthOf(map)` equals `map.length()`.
-   Typed `keys`, `values`, and `entries` APIs work with snapshot
    semantics.
-   Entry iteration binds variables with static types `K` and `V`.
-   Spread copies entries, obeys evaluation order, and uses
    last-write-wins semantics.
-   `const` blocks every mutation path.
-   Map values preserve Graphite's value-copy semantics, including
    nested supported values.
-   Map iteration order is explicitly unspecified and tests do not rely
    on it.
-   Negative tests and existing array/record/compiler regression suites
    pass.
-   No source-text rewriting, regex-based parsing, heterogeneous map
    representation, or untyped escape hatch is introduced.

## 15. Locked Stage 8 decisions summary

-   Map syntax: `map<K, V>`.
-   Literal syntax: `{ key: value, ...sourceMap }`, requiring expected
    map type.
-   Duplicate keys: allowed; last processed entry wins.
-   Evaluation: left-to-right; key before value; spread expression
    evaluated once.
-   Missing checked lookup: `MissingMapKey`.
-   `getOrDefault`: non-mutating, does not insert.
-   `remove`: returns `bln`, `true` if removed, `false` if absent.
-   `keys()` and `values()`: independent snapshot arrays.
-   `entries()`: snapshot typed iterable, usable with two-variable loop
    destructuring.
-   Iteration order: unspecified.
-   Key types: `str`, `bln`, `chr`, supported signed/unsigned integer
    types. Float, array, map, and record keys rejected in the initial
    implementation.
-   Value types: any supported non-`void` type compatible with
    Graphite's value semantics, including arrays and named records.
-   Map values use value semantics; no shared mutable aliasing.
-   `const`: prohibits index assignment, `set`, `remove`, and nested
    mutation through a const-rooted access path.
-   No map slicing or user-defined generics.
-   `map` is a reserved keyword, on the same footing as `type`.
    `map<K, V>` is the built-in map type. `map` cannot be a variable,
    function, or record-field name. Other future type names stay
    unreserved.
-   Brace literals use the expected type: a named record type selects a
    record literal, `map<K, V>` selects a map literal, and any other
    context is a compile-time error. Statement `{}` remains a block.
    `...expr` is valid only inside a map literal.
-   Maps are printable through `io.println`, `str(...)`, and string
    interpolation, using the same value-formatting path as arrays and
    records. A representative display is `{ "Ada": 10, "Linus": 20 }`.
    Entry order is unspecified. String keys use the same quoting and
    escaping as strings inside arrays. Printing does not mutate the map.
-   `==` and `!=` are rejected for maps. Stage 8 does not compare maps
    by entries, by identity, or by iteration order.
-   Map values may be nested maps, arrays, and records. Keys still
    cannot be floats, arrays, maps, or records. Nested writes such as
    `users["Ada"].age = 22` are allowed through a mutable l-value. A
    missing key on that path raises `MissingMapKey` before the write.
    `make()["Ada"] = 1` and `make().set("Ada", 1)` are compile-time
    errors because the receiver is not a mutable l-value.
-   `getOrDefault` evaluates its default expression exactly once on
    every call, including when the key is present. A failure in that
    expression propagates even if the key exists.

## 16. Locked decisions for the remaining open questions

These decisions are part of Stage 8. Later text in this LLD is
subordinate to them.

1.  **Printing.** Maps print through `io.println`, `str(...)`, and
    interpolation. Formatting goes through the existing value-display
    path. Order is not significant and must not be stabilized.
2.  **Iteration and hashing.** Use `std::collections::HashMap` with its
    default hasher. `keys()`, `values()`, `entries()`, spread
    enumeration, and printing may change order between runs. Equality of
    keys, lookup, insertion, removal, and spread overwrite stay correct
    regardless of that order. Tests must not depend on an order.
3.  **Missing-key diagnostic.** Report
    `missing key: lookup on '<canonical map type>'` at the source
    location. Do not format or include the key. `getOrDefault` does not
    raise this for an absent key. `remove` returns `false` for an absent
    key.
4.  **`map` keyword.** Reserve `map` only. Do not reserve a list of
    speculative type names in this change.
5.  **Brace literals and spread.** Expected record type means a record
    literal. Expected map type means a map literal. Anything else is an
    error. Spread is map-literal-only. No anonymous record or map
    inference.
6.  **Equality.** Reject `==` and `!=` with a compile-time diagnostic
    that map equality is unsupported. Do not implement reference
    equality or structural equality in this stage.
7.  **Nested values and evaluation.** Nested maps, arrays, and records
    are valid values. In-place nested mutation follows the existing
    l-value and `const` rules for the whole access path. The
    `getOrDefault` default expression always runs once and is discarded
    when the key is already present. If that expression fails, the
    failure propagates even when the key exists.
