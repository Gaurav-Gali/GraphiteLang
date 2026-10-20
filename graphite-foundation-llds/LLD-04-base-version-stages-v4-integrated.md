# LLD 04 --- Base-Version Completion Stages

**Status:** Revised roadmap\
**Target:** Graphite v0.1\
**Purpose:** Define the implementation order for the current Graphite
foundation and upcoming type-system milestones.

## 1. Guiding approach

Graphite must be developed in small, testable stages. Every stage must
leave the repository buildable, include positive and negative tests, and
preserve separation between lexing, parsing, semantic analysis,
typed-AST construction, code generation, runtime support, and CLI
orchestration.

A stage is not complete merely because code has been written: its
acceptance criteria must pass in the repository. Existing working
behavior is a regression baseline, not proof that every edge case has
been validated.

Graphite prioritizes **static type safety**. There will be no `any`
type. Variables, parameters, return values, array elements, and map
keys/values must have known types under the language rules.

## 2. Current implementation baseline

The current feature program demonstrates working support for:

-   Imports and I/O through `import grf.io;` and `io.println(...)`.
-   Comments, declarations, variables, constants, lexical scopes,
    shadowing, and assignment.
-   Primitive types and aliases: `int`/`int64`, `uint`/`uint64`, and
    `flt`/`flt64`.
-   Top-level functions, forward function references, parameters, and
    returns.
-   Current `auto str` conversions.
-   Arithmetic, comparisons, boolean negation, conditionals, loops,
    `break`, `continue`, increment/decrement, and interpolation.
-   Running `.grf` programs through the current CLI.

This is a regression baseline. Negative cases, boundary cases, and all
semantic edge cases still require tests.

## 3. Stages

### Stage 1 --- Lexer

Implement source reading, tokenization, token kinds, source positions,
whitespace, comments, literals, identifiers, keywords, operators,
punctuation, and lexical diagnostics.

The v0.1 keyword inventory includes `auto` and excludes `class`,
`private`, `this`, `public`, `new`, and `constructor`. Stage 7 reserves
`type` for named record declarations. Stage 8 reserves `map` for the
built-in map type. `PlusPlus` and `MinusMinus` are recognized. Syntax
punctuation required by approved features must be deliberately specified
and implemented rather than rejected by an outdated token inventory. In
particular, `:` is required in record/map literals and array slice
expressions, and `...` is required for map spread; the parser must
accept these only in their specified grammar contexts.

**Outcome:** Reliable tokens, source spans, and lexical errors, with
tests for `import grf.io;` and `io.println(...)`.

### Stage 2 --- Parser and AST

Define the grammar for supported syntax and parse tokens into a
structured Graphite AST.

**Outcome:** Valid source produces AST nodes; invalid syntax produces
source-located diagnostics.

### Stage 3 --- Minimal end-to-end program

Support one entry `.grf` file, imports at the top, top-level functions,
exactly one parameterless `void main()` for executable entry files, and
basic output through the agreed library interface.

**Outcome:** A `.grf` program passes through the compiler, generates
Rust, compiles, and runs. Invalid entry points fail with semantic
diagnostics.

### Stage 4 --- Declarations, primitive types, and functions

Implement variables, constants, scopes, primitive types, function
declarations/calls, parameters, return checking, and semantic
validation.

Normalize `int`/`int64`, `uint`/`uint64`, and `flt`/`flt64` to canonical
types before assignment, argument, operator, and return checks. Collect
top-level function declarations before checking bodies to enable forward
references. Represent supported `auto T` conversions as explicit
typed-AST nodes.

Continue testing invalid assignments, invalid arguments, duplicate
declarations, invalid returns, constant mutation, entry-point errors,
and numeric boundaries.

**Outcome:** Typed Graphite programs declare values and call functions
under documented rules; invalid programs fail before code generation.

**Universal** `const` **semantics:** `const` applies to every supported
type, including primitives, arrays, maps, named records, `Type`
descriptors, and future
classe`s. It makes the binding non-reassignable and forbids mutation through that binding, including nested record-field writes, array index/slice writes, and map insert/update/removal. Reads and read-only methods are allowed. The semantic analyzer must enforce this at compile time. Alias/reference rules must not permit a mutable alias to bypass the guarantee; detailed ownership/aliasing rules belong in the relevant stage LLD.`const\`
does not mean a value is copied automatically.

**Global** `lengthOf(value)` **built-in:** Graphite provides a global
`lengthOf(value) -> uint64` operation for every supported type with a
defined length: strings, arrays, maps, and named record types. It is a
compiler-recognized built-in with type-directed semantics, not a
user-defined generic function and not an `any`-based escape hatch.
Strings report their Unicode scalar-value count (not UTF-8 byte length);
arrays report element count; maps report entry count; named records
report the number of fields declared by their type. For a record, the
count is structural and fixed by its type definition, not the number of
non-empty or non-default fields. Types with no meaningful length, such
as numeric and boolean primitives, are rejected at compile time. Future
types may support `lengthOf` only after their length semantics are
explicitly specified. `lengthOf` is read-only and valid for `const`
values. Collection `.length()` methods may remain as convenience APIs,
but `lengthOf(value)` is the uniform global API.

### Stage 5 --- Unified type conversion system

Expand `auto T` beyond its current `str`-focused implementation and add
explicit conversion calls, including forms such as:

    int("123")
    str(123)
    chr("A")
    flt("12.433")
    bln("true")

Requirements:

-   Define a conversion matrix for every supported source/target pair.
-   Make `auto T` and explicit conversions use the same centralized
    conversion rules.
-   Reject invalid textual conversions such as `"abc123"` to `int`.
-   Define range, signedness, floating-point, boolean parsing, and
    character validity rules.
-   Explicitly define float-to-integer behavior; never silently truncate
    or invent rounding rules.
-   Preserve the distinction between conversion failures and ordinary
    type mismatches.
-   Insert explicit typed-AST conversion nodes for `auto T`.
-   Keep interpolation formatting consistent with the approved
    formatting policy.
-   Do not use source-text replacement, regex rewriting, or per-function
    compiler special cases.

No conversion may silently wrap, truncate, or fabricate a value. Specify
failure behavior before implementing conversions that can fail at
runtime.

**Outcome:** A single documented conversion system powers both
target-directed `auto T` and explicit `T(value)` conversions.

### Stage 6 --- Typed arrays, indexing, slicing, destructuring, and iteration

Implement homogeneous, statically typed, dynamically sized arrays and
Python-style sequence indexing/slicing. Array features must be
represented in the AST and typed AST; do not implement them with
source-text rewriting.

Initial syntax:

    int[] arr = [10, 20, 30, 40, 50];

    // Indexing and mutation
    int first = arr[0];
    int last = arr[-1];
    arr[0] = 99;
    arr[-2] = 88;

    // Slicing: stop is exclusive
    int[] prefix = arr[:3];
    int[] middle = arr[1:4];
    int[] suffix = arr[2:];
    int[] copy = arr[:];
    int[] everyOther = arr[::2];
    int[] reversed = arr[::-1];
    int[] backwards = arr[4:0:-1];

    // Collection methods remain available alongside indexing/slicing
    int firstViaMethod = arr.get(0);
    arr.set(0, 100);
    arr.push(60);
    arr.insert(1, 15);
    uint64 arrayLength = arr.length(); // Collection convenience method
    uint64 uniformArrayLength = lengthOf(arr); // Global built-in
    arr.remove(1);

    // Destructuring: exact element count required
    int a, int b = [1, 2];

    // Iteration
    loop (int value = arr.iter()) {
        io.println(value);
    }

    loop (uint64 idx, int value = arr.enumerate()) {
        io.println(f"{idx} - {value}");
    }

Requirements:

-   Define and test array type syntax, literal typing, empty-array
    typing, element type checking, and index-expression typing.
-   Support `arr[index]` reads and `arr[index] = value` writes. The
    index must be an integer type under the approved integer rules. Also
    provide typed methods `get(index) -> T`, `set(index, value)`,
    `push(value)`, `insert(index, value)`, `remove(index)`, and
    `length() -> uint64`; their bounds and return behavior must be
    documented consistently with indexing.
-   Support `lengthOf(arr) -> uint64`, returning the number of elements.
    It must agree with `arr.length()` and must not mutate the array.
-   Support negative indexes for reads and writes. For array length `n`,
    negative index `i` resolves to `n + i`. An index still outside
    `[0, n)` raises a defined runtime bounds error; it must not wrap or
    fabricate a value.
-   Support `arr[start:stop:step]`, with each component optional,
    including `arr[:stop]`, `arr[start:]`, `arr[:]`, `arr[::step]`, and
    negative bounds/steps.
-   Slice stops are exclusive. Slice bounds are normalized/clamped using
    Python-style sequence slicing rules. Negative steps traverse
    backward. A zero step is a runtime error.
-   A slice expression returns a new array copy, not a view or alias
    into the source array.
-   Support slice assignment such as `arr[1:3] = [20, 30]`. A unit-step
    slice assignment may resize the array. A non-unit-step slice
    assignment must replace exactly as many elements as the selected
    slice contains; otherwise it raises a defined runtime error. Element
    types must match the array element type.
-   Support declaration destructuring such as `int a, int b = [1, 2];`
    and loop pair destructuring from `enumerate()`. Evaluate the
    initializer/iterable expression once. For declaration destructuring,
    require the source to provide exactly the declared number of values
    and type-check each binding. Define scope and reject duplicate
    bindings in the same declaration.
-   `iter()` yields values; `enumerate()` yields zero-based index/value
    pairs. Define the index type explicitly and use it consistently.
    Empty iteration, `break`, `continue`, variable scope, and iteration
    behavior must be tested.
-   Iteration must not consume or mutate the source array unless a
    future explicitly consuming API is designed. Specify
    borrow/ownership behavior in the implementation LLD.
-   Enforce `const` on every write path, including element assignment
    and slice assignment.
-   Arrays are homogeneous. Do not use `any` as an escape hatch. A Rust
    `Vec<T>` may be an implementation detail, not a Graphite language
    requirement.

**Outcome:** Arrays support typed indexing, Python-style slicing,
mutation, destructuring, and iteration with documented bounds and
ownership semantics.

### Stage 7 --- Named custom structural types

Implement named record types using `type` and apply the same universal
`const` rules used for primitive values and collections.

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
        user.age = 22;          // Valid for a mutable binding
        // fixedUser.age = 22;   // Compile-time error

        uint64 userFieldCount = lengthOf(fixedUser); // Number of fields declared by User
    }

Requirements:

-   Register type declarations before body analysis where supported
    forward references require it.
-   Give each named custom type a distinct nominal identity.
-   Validate construction: reject missing, unknown, duplicate, or
    incorrectly typed fields.
-   Validate field access and field assignment.
-   Define declaration scope, duplicate names, forward references,
    recursion, and field mutability.
-   `const User user = ...` makes the value immutable through that
    binding: field assignments and nested mutations through it are
    rejected. A mutable binding permits field mutation when the
    field/type rules allow it.
-   `lengthOf(record)` returns the number of fields declared by the
    record's named type. This count is fixed by the type definition and
    does not depend on field values. It works for mutable and `const`
    record bindings and does not mutate or expose field contents.
-   Reject unsupported recursive cycles rather than generating invalid
    Rust.
-   Keep record types separate from classes. Methods, constructors,
    inheritance, visibility modifiers, and user-defined generics are
    deferred.
-   Use structured AST and semantic type definitions, not textual
    expansion.

Named types are not interchangeable just because their fields match.

**Outcome:** Custom record types have compile-time field validation,
stable named identity, and consistent const enforcement.

### Stage 8 --- Statically typed maps, indexing, and spread

Implement maps with explicit key and value types. Built-in type
parameters do not by themselves enable user-defined generics. Maps are
key-addressed collections, not positional sequences; array-style
negative indexing and positional slicing do not apply to maps.

Initial syntax and API:

    map<str, int> scores = {
        "Ada": 10,
        "Linus": 20,
    };

    int ada = scores["Ada"];
    int adaViaMethod = scores.get("Ada");
    scores["Ada"] = 11;          // Update existing key
    scores["Grace"] = 30;        // Insert a new key

    int unknown = scores.getOrDefault("Unknown", 0);
    bln hasAda = scores.containsKey("Ada");
    scores.remove("Linus");
    uint64 count = scores.length(); // Collection convenience method
    uint64 uniformCount = lengthOf(scores); // Global built-in

    map<str, int> moreScores = {
        ...scores,
        "Grace": 31,
        "Dennis": 25,
    };

Requirements:

-   Validate map literal syntax and statically enforce key type `K` and
    value type `V` for every entry and operation. Do not implement
    `map<K, any>` or an equivalent heterogeneous escape hatch.
-   Support `m[key]` reads and `m[key] = value` writes. A write inserts
    the key if absent or updates its value if present. The key and
    assigned value must match the map's declared types. Also provide
    `get(key) -> V` as a checked lookup with the same missing-key
    failure behavior as `m[key]`.
-   Reading a missing key through `m[key]` raises a defined runtime
    missing-key error; it must not fabricate a default value.
-   Support `getOrDefault(key, defaultValue) -> V`. Return the stored
    value when the key exists; otherwise return `defaultValue`. The
    default must be type-compatible with `V`. This method is read-only
    and must not insert the default or mutate the map. A present value
    equal to a default-like value still counts as present.
-   Include `containsKey(key) -> bln`, `set(key, value)`, `remove(key)`,
    and `length()`. Define the return types and missing-key behavior of
    `remove` in the map API LLD. Prefer a predictable, documented
    behavior rather than silently inventing a result.
-   Support `lengthOf(m) -> uint64`, returning the number of entries and
    agreeing with `m.length()`. It is read-only and works for `const`
    maps.
-   Include typed `keys()`, `values()`, and `entries()` APIs. Define
    their return types and iteration behavior. Since map order is not
    inherently positional, specify whether iteration order is
    unspecified or guaranteed by the implementation; do not let tests
    accidentally rely on an unspecified order.
-   Support iteration/destructuring over key/value entries, for example
    `loop (str key, int value = scores.entries())`, if the parser and
    typed loop model support this form. The loop variables must be
    statically typed as `K` and `V`.
-   Support map spread inside map literals: `...sourceMap`. The source
    map's key and value types must be compatible with the destination
    map's types. Spread copies entries into the new map and does not
    mutate the source.
-   Define evaluation order for map literal entries and spreads. A later
    entry overrides an earlier entry with the same key; this includes
    explicit entries after a spread and later spreads. Specify duplicate
    explicit-key behavior consistently with this rule.
-   Do not provide positional slicing for maps. Use explicit key lookup,
    `keys()`/`values()`/`entries()`, and future filtering APIs instead.
    Lambda-based filtering is deferred.
-   Enforce `const` on every mutation path, including index assignment,
    `set`, `remove`, and any future mutating map methods. Read-only
    operations, including `getOrDefault`, remain permitted.
-   Define key equality/hash requirements and support custom record
    values only where ownership and type rules permit them.
-   Reserve `map` as a keyword. Brace literals are record literals when
    the expected type is a named record and map literals when it is
    `map<K, V>`. `...` is valid only in a map literal.
-   Print maps through `io.println`, `str(...)`, and interpolation.
    Display order is unspecified. Use the standard randomized map
    hasher; do not promise stable iteration.
-   Reject `==` and `!=` for maps. Do not include the missing key's text
    in the runtime diagnostic.
-   Evaluate the `getOrDefault` default expression once on every call,
    including when the key is already present.

Do not use `any` or equivalent untyped escape hatches.

**Outcome:** Maps enforce static key/value types and provide predictable
lookup, default lookup, mutation, iteration, and spread semantics.

### Stage 9 --- Global `lengthOf()` built-in

Implement the global `lengthOf(value) -> uint64` built-in with
statically resolved behavior for all supported length-bearing types. It
is a language built-in, not a library import, not a method that requires
receiver syntax, and not a generic function implemented through `any`.

    str title = "Graphite";
    int[] values = [10, 20, 30];
    map<str, int> scores = {"Ada": 10, "Linus": 20};
    type User {
        name: str,
        age: int,
    }
    User user = {name: "Ada", age: 21};

    io.println(lengthOf(title));  // 8 Unicode scalar values
    io.println(lengthOf(values)); // 3 elements
    io.println(lengthOf(scores)); // 2 entries
    io.println(lengthOf(user));   // 2 declared fields

Semantics:

-   `lengthOf(str)` returns the number of Unicode scalar values, not
    UTF-8 bytes. Grapheme-cluster counting is deferred unless a separate
    Unicode text API is approved.
-   `lengthOf(T[])` returns the number of array elements.
-   `lengthOf(map<K, V>)` returns the number of entries.
-   `lengthOf(NamedRecord)` returns the number of fields declared by
    that named record type. The result is fixed by the type definition
    and does not depend on field values.
-   The return type is canonical `uint64`.
-   The operation is read-only and works on `const` bindings.
-   `lengthOf` on types without a defined length, including numeric and
    boolean primitives and `Type` descriptors, is a compile-time error.
    Future types, including classes, may be accepted only after their
    length semantics are explicitly specified.
-   Existing `arr.length()` and `map.length()` may remain as convenience
    methods. Their results must agree with `lengthOf(arr)` and
    `lengthOf(map)` respectively.
-   The compiler must resolve the accepted argument type and lower the
    operation through typed AST/code generation. Do not implement it
    through textual rewriting, reflection, runtime `any`, or
    per-user-type compiler special cases.
-   Tests must cover empty and non-empty strings, Unicode strings, empty
    and non-empty arrays/maps, records with different field counts,
    const arguments, and compile-time rejection of types without defined
    length semantics.

**Outcome:** Graphite has one consistent, statically checked global
length operation for strings, arrays, maps, and named records.

### Stage 10 --- Type descriptors and `typeOf()`

Implement a dedicated built-in `Type` descriptor. `typeOf(expression)`
returns `Type`, not `str`.

    int age = 21;
    Type t = typeOf(age);

    io.println(t.name());
    io.println(t);
    io.println(typeOf("123"));

Expected output:

    int64
    <class 'int64'>
    <class 'str'>

Requirements:

-   `Type` is a dedicated compiler-recognized type, not `any` or a
    string alias.
-   `typeOf(expression)` describes the expression's static type; it does
    not parse or convert its contents.
-   `Type.name()` returns the canonical type name as `str`.
-   Printing a descriptor uses `<class 'CanonicalName'>` for built-in
    and compound types. `<type 'Name'>` is only for a named record type
    itself.
-   `Type.name()` applies the existing canonical spelling recursively,
    so `int[]` is `int64[]` and `map<str, int>` is `map<str, int64>`.
-   Aliases report canonical names: `int`/`int64` → `int64`,
    `uint`/`uint64` → `uint64`, and `flt`/`flt64` → `flt64`.
-   Custom types report their declared names. `User[]` and
    `map<str, User>` are compound types and use the `<class '...'>`
    display.
-   `typeOf(typeOf(1))` describes `Type` and displays `<class 'Type'>`.
-   `io.println`, `str(...)`, and interpolation share that display.
    Ordinary `str` assignment rejects a `Type` value. `auto str` accepts
    it through the existing conversion path.
-   `Type` is reserved only as a type name. `typeOf` is reserved
    everywhere `lengthOf` is reserved.
-   A `Type` value may be stored in arrays, map values, and record
    fields. It is not a map key.
-   Descriptor equality compares canonical type identity. `<`, `<=`,
    `>`, and `>=` between descriptors are compile-time errors.
-   Initially support type identity and name/display behavior only.
    Further reflection, generic arguments, class metadata, and dynamic
    runtime discovery are deferred.
-   Type descriptors must not weaken static typing.

**Outcome:** Graphite supports type introspection without dynamic
typing.

### Stage 11 --- Structured diagnostics and error reporting

Build a Graphite-owned diagnostic system for precise, actionable errors
across compiler and runtime phases. This establishes the source-location
infrastructure needed by exceptions, classes, and Rust-backed libraries.

Requirements:

-   Define a structured diagnostic model with category, stable error
    code, severity, message, primary source span, and optional related
    spans, notes, and help.
-   Preserve source file, line, and column information from lexing and
    parsing through semantic analysis, typed-AST validation, code
    generation, and runtime reporting.
-   Render terminal diagnostics with filename, line/column, source
    excerpt, caret/range highlighting, and explanatory notes.
-   Distinguish lexical, parse, name-resolution, semantic,
    type-checking, code-generation, Rust build, dependency-resolution,
    and runtime errors.
-   Include expected/actual types and actionable suggestions where
    reliable.
-   Report multiple independent errors where recovery is safe; avoid
    cascading diagnostics from invalid intermediate state.
-   Define source maps or another reliable mechanism for mapping
    generated Rust locations to original `.grf` spans.
-   Capture Rust compiler/linker failures and present useful details,
    mapping them to Graphite locations only when the mapping is
    reliable.
-   Define a shared runtime error representation with error kind,
    message, source span where available, and optional cause/context.
    Integrate every existing runtime failure, including array bounds
    and missing map keys. The generated program renders the diagnostic
    through the shared renderer. Stage 11 does not add a protocol
    between that process and `grf run`.
-   Separate error detection/propagation from terminal rendering.
-   JSON diagnostics may be added if they do not delay the required
    structured model.

Never generate Rust from an invalid or incompletely typed AST, and do
not claim a Graphite source location for a Rust error without a reliable
mapping.

**Outcome:** Graphite reports specific, source-located, actionable
errors across compiler and runtime phases.

### Stage 12 --- Exception handling

Implement Graphite-level `try`, `catch`, and `finally` on top of Stage
11's runtime error model. Define semantics before adding syntax or Rust
lowering.

Proposed syntax:

    try {
        int result = divide(10, 0);
        io.println(result);
    } catch (DivisionByZeroError error) {
        io.println(error.message());
    } catch (RuntimeError error) {
        io.println(error.message());
    } finally {
        io.println("Operation completed");
    }

Requirements:

-   Specify grammar and AST representation for `try`, `catch`, and
    `finally`.
-   Decide whether `throw` and user-defined error types are included in
    the initial implementation.
-   Define built-in runtime errors and catch matching, including array
    bounds, missing map keys, conversion failures, and Rust library
    failures.
-   Define catch ordering, variable scope, unmatched exceptions, and
    propagation across Graphite function calls.
-   Specify `finally` behavior on normal completion, handled/unhandled
    errors, `return`, `break`, and `continue`, including nested try
    blocks.
-   Preserve source locations, error codes, messages, and causes when
    catching or rethrowing.
-   Define stack-trace behavior and how Rust `Result`/`Err` maps to
    Graphite errors.
-   Do not equate every Rust `panic!` with a catchable Graphite
    exception; specify panic and foreign-library boundaries explicitly.
-   Ensure generated Rust preserves cleanup and control flow.
-   Add positive, negative, runtime, and control-flow regression tests.

Do not implement exceptions as source rewriting or as aliases for
`panic!`. Resolve open semantics in the Stage 12 LLD first.

**Outcome:** Graphite has predictable, source-aware exception
propagation, catching, and cleanup.

### Stage 13 --- Classes and object-oriented programming

Extend Graphite with a separately specified class/object model. Existing
named records remain nominal value types and must not silently become
classes.

Requirements:

-   Define class declarations, instance creation,
    constructors/initialization, fields, and instance methods.
-   Define receiver syntax (`this` or the chosen Graphite equivalent),
    method lookup, method calls, and field access.
-   Define visibility and encapsulation, including any
    `public`/`private` modifiers.
-   Explicitly decide whether inheritance, interfaces, traits, or
    composition are supported. Do not add inheritance or dynamic
    dispatch without a design.
-   Define object identity, assignment, parameter passing, returns, and
    mutable aliasing. Do not accidentally introduce shared references
    through existing record value-copy semantics.
-   Define `const` for class bindings, fields, nested values, and
    references.
-   Define constructor failure and its interaction with Stage 12
    exceptions.
-   Specify nominal identity, method signatures, access diagnostics,
    type checking, and Rust lowering.
-   Specify interaction with Stage 10 `Type` and `typeOf()` without
    expanding reflection beyond its approved scope.
-   Define whether class instances may be map values and what
    restrictions apply to class instances as keys.
-   Keep user-defined generics, method overloading/overriding,
    interfaces, and inheritance deferred unless explicitly approved and
    specified in this stage's LLD.

The LLD must explicitly compare class ownership/identity with Stage 7
record value semantics.

**Outcome:** Graphite gains a statically typed OOP model that composes
with records, collections, const, descriptors, and exceptions.

### Stage 14 --- Custom Rust libraries and global installation

Allow users to implement libraries in Rust, declare Graphite-facing APIs
and Rust mappings in `grf.json`, and install libraries globally.
Preserve the import/call distinction:

    import grf.io;
    io.println("Hello");

Requirements:

-   Preserve the existing `grf.json` schema unless a separately approved
    schema version is required.
-   Define library domain/name, version, Graphite API signatures, Rust
    symbol/path mappings, dependencies, and compatibility metadata.
-   Validate API signatures statically; avoid per-function compiler
    special cases for user libraries.
-   Specify the Rust integration strategy: Cargo dependencies, compiled
    crates, or another explicit mechanism.
-   Define the boundary between generated Graphite Rust and library
    code, including types, ownership, errors, and supported value
    representations.
-   Provide global installation, listing, inspection, update, and
    uninstall commands as appropriate (for example,
    `grf install <domain>/<library>`).
-   Store installed libraries and metadata in a managed global
    registry/cache so projects can resolve them from any working
    directory.
-   Define versions, dependency resolution, conflicts, offline behavior,
    and missing-dependency diagnostics.
-   Avoid developer-specific absolute paths and preserve reproducible
    dependency resolution.
-   Define a trust policy for native code; do not imply third-party Rust
    code is sandboxed unless it actually is.
-   Validate manifests, paths, symbol mappings, API signatures, and
    imports with source-located diagnostics.
-   Use the same manifest-driven interface model for built-in and user
    libraries where practical.
-   Test local/global libraries, invalid manifests, version conflicts,
    missing dependencies, and clean-machine resolution.

Global installation makes a library discoverable system-wide; it does
not place generated code in arbitrary source directories.

**Outcome:** Users can author Rust-backed libraries, expose statically
checked APIs through `grf.json`, and install/version them for use across
projects.

### Stage 15 --- Runtime execution, build system, and CLI hardening

Harden the compiler-to-Rust path after the language, diagnostics,
exception, class, and library contracts are validated.

Requirements:

-   Preserve `grf run`; add or stabilize `grf build`, `grf check`, and
    `grf clean`.
-   Define consistent CLI arguments, help output, exit codes, and
    failure categories.
-   `grf check` performs parsing, name resolution, semantic analysis,
    and type checking without producing or executing a native binary.
-   `grf build` compiles and links without executing the program.
-   `grf run` reuses a valid build or rebuilds when inputs change, then
    executes it.
-   Use a managed cache such as `target/grf/builds/<entry-key>`.
-   Keep generated Rust, temporary Cargo projects, metadata, and
    executables out of source directories.
-   Resolve `graphite-core`, installed libraries, and dependencies
    without developer-specific absolute paths.
-   Set the program working directory to the directory containing the
    entry `.grf`, regardless of the CLI's invocation directory.
-   Forward arguments and standard streams and propagate the program
    exit status.
-   Report build, link, dependency, and launch failures through Stage 11
    diagnostics.
-   Invalidate cache when relevant sources, manifests, compiler
    versions, dependencies, target settings, or build configuration
    change.
-   Use complete build fingerprints; publish cache metadata only after
    successful builds.
-   Prevent concurrent builds from corrupting shared artifacts; use
    per-entry locking or equivalent coordination and atomic publication
    where practical.
-   Recover from interrupted builds and detect missing/corrupt
    artifacts.
-   Never execute a stale binary after the current source fails
    checking/building.
-   Never generate code from an invalid typed AST.
-   Ensure `grf clean` only removes Graphite-managed artifacts.

**Outcome:** Native builds and execution are repeatable, portable,
cache-correct, concurrency-safe, and diagnosable.

### Stage 16 --- Release readiness

Prepare Graphite v0.1 for repeatable use.

Requirements:

-   Document syntax, semantics, CLI commands, supported platforms,
    library installation, error handling, and known limitations.
-   Run full positive, negative, runtime, and regression suites.
-   Verify diagnostics for lexer, parser, type checker, runtime, Rust
    build, and library-resolution failures.
-   Test clean builds, cache invalidation, parallel invocations, and
    recovery after interruption.
-   Verify library resolution on a clean machine without
    developer-specific paths.
-   Ensure generated files stay out of source directories and
    `grf clean` is safe.
-   Clearly label deferred features.

**Outcome:** A reproducible v0.1 release with tested compiler behavior,
reliable diagnostics, documented extensions, and no undocumented
semantics.

## 4. Cross-stage requirements

Every stage must:

-   Follow current LLDs and frozen syntax.
-   Include positive and negative tests, plus runtime tests where
    applicable.
-   Preserve source locations through compiler phases and runtime
    boundaries where practical.
-   Reject invalid source rather than silently accepting it.
-   Keep lexing, parsing, semantic analysis, typed-AST construction,
    code generation, runtime support, diagnostics, and CLI orchestration
    separate.
-   Update documentation when a design decision changes.
-   Avoid accidental implementation of deferred features.
-   Preserve static type safety and avoid `any` or equivalent untyped
    escape hatches.
-   Centralize type normalization and conversion rules.
-   Ensure the typed AST has resolved type information before code
    generation.
-   Use structured diagnostics rather than ad hoc strings as the only
    error representation.
-   Map generated Rust errors back to Graphite source locations only
    when the mapping is reliable.
-   Keep generated files and build artifacts in managed locations.
-   Ensure a failed check/build cannot execute a stale binary.
-   Test interactions between const enforcement, collection mutation,
    descriptors, exceptions, classes, and library boundaries.

A specified case is not a passing test until the repository contains a
running test for it.

## 5. Recommended order of work

1.  Validate Stage 4 negative cases, return-path analysis, and numeric
    boundaries.
2.  Freeze and implement the conversion matrix and explicit conversion
    semantics.
3.  Implement typed arrays, Python-style indexing/slicing,
    destructuring, and iteration.
4.  Implement named custom record types and their `lengthOf()`
    semantics, including map fields where allowed by the locked
    record/cycle rules.
5.  Implement statically typed maps, bracket access/assignment,
    `getOrDefault()`, spread, map formatting, and their `lengthOf()`
    semantics.
6.  Implement global `lengthOf()` for strings, arrays, maps, and named
    records.
7.  Implement `Type` descriptors and `typeOf()`.
8.  Implement structured diagnostics and source mapping before
    language-level exception handling.
9.  Specify and implement exception handling and runtime error
    propagation.
10. Specify and implement classes and OOP, preserving the distinction
    between record value semantics and class identity/ownership.
11. Complete manifest-driven Rust library interfaces and global library
    installation.
12. Harden native builds, caching, execution, and CLI behavior against
    the completed language and library surface.
13. Complete release-readiness checks and documentation.

The current feature program is the regression baseline throughout this
sequence.

## 6. Locked type-system and tooling decisions

1.  **No `any` type.** Graphite prioritizes static type safety.
2.  `auto T` **means approved conversion.** It is not type inference or
    dynamic typing.
3.  `T(value)` **means explicit conversion.** It shares the centralized
    conversion rules with `auto T`.
4.  **Arrays are homogeneous and statically typed.**
5.  **Maps have explicit key and value types.** Built-in map type
    parameters do not enable user-defined generics.
6.  **Custom record types use `type Name { ... }`.** Named records have
    nominal identity, compile-time field validation, and value
    semantics; they are not classes.
7.  `typeOf(expression)` **returns a dedicated `Type` descriptor.** It
    does not return a string or convert the expression.
8.  `Type.name()` **returns a canonical type name.** Printing a
    descriptor uses the agreed class/type display representation.
9.  **Aliases use canonical identities.** `int`/`int64` report `int64`,
    `uint`/`uint64` report `uint64`, and `flt`/`flt64` report `flt64`.
10. **Classes and OOP are a separate stage.** Inheritance, interfaces,
    and user-defined generics require explicit design and are not
    implied by adding classes.
11. `const` **is universal.** It prevents reassignment and mutation
    through the const binding for every supported type, including nested
    records and collection contents. Class-specific identity and alias
    rules must be defined in the class LLD.
12. **Arrays support Python-style indexing and slicing.** Negative
    indexes count from the end; slice stops are exclusive; omitted
    bounds and negative steps follow documented Python-style rules;
    slice expressions produce copies.
13. **Array slice assignment is supported.** Unit-step slices may resize
    arrays; stepped slices require replacement length to match the
    selected positions.
14. **Array destructuring is statically checked.** The source expression
    is evaluated once, element counts must match exactly, and each
    binding is type-checked.
15. **Maps use key-based bracket syntax.** `m[key]` reads and
    `m[key] = value` inserts or updates; maps do not gain positional
    negative indexing or array-style slicing.
16. **Maps provide `getOrDefault(key, defaultValue)`.** The default has
    the map value type, is evaluated exactly once on every call, is used
    only when the key is absent, and is never inserted.
17. **Map spread is supported in map literals.** `...map` copies
    entries, types must be compatible, and later entries override
    earlier entries.
18. **Map iteration order is unspecified.** Tests must not rely on an
    order; map printing and spread must preserve semantics regardless of
    enumeration order.
19. **Lambdas are deferred.** Map filtering or other lambda-based APIs
    must not be added until lambda syntax and typing are designed
    separately.
20. **Global `lengthOf(value)` returns `uint64`** for strings, arrays,
    maps, and named records. Strings count Unicode scalar values; arrays
    count elements; maps count entries; records count declared fields.
    Types without defined length semantics are rejected at compile time.
21. **Collection `.length()` methods may remain as convenience APIs**,
    but their results must agree with global `lengthOf()` for arrays and
    maps.
22. **Structured diagnostics are shared infrastructure.** Compiler,
    runtime, Rust build, and library-resolution errors use a structured
    model with source spans where available.
23. **Exception semantics are explicit.** Graphite exceptions are not
    synonymous with arbitrary Rust panics; propagation and
    foreign-library boundaries must be specified.
24. **Rust-backed libraries are manifest-driven.** `grf.json` defines
    Graphite-facing APIs and Rust mappings under the approved schema.
    Installed libraries are managed and versioned rather than copied
    into source trees.
25. **Build artifacts are managed and cache-keyed.** Relevant inputs
    determine cache validity; failed builds cannot publish valid cache
    metadata or execute stale binaries.

## 7. Deferred details that require stage-specific LLDs

The roadmap fixes feature boundaries and implementation order but does
not replace detailed stage specifications. Before implementation, the
corresponding LLD must define:

-   The complete conversion matrix and runtime failure mechanism.
-   Float-to-integer conversion behavior.
-   Character construction and Unicode rules.
-   Array ownership, universal `const` enforcement, indexing, negative
    indexing, slicing, slice assignment, destructuring, and iteration
    semantics.
-   Map key constraints, bracket access/assignment, `getOrDefault()`,
    removal and missing-key behavior, iteration ordering, formatting,
    and map spread semantics.
-   Custom record initialization, recursive type rules (including
    map-mediated cycles), field mutability, and expected-type
    brace-literal disambiguation.
-   `Type` descriptor identity, canonical naming for compound types,
    display, equality, and runtime representation.
-   `lengthOf()` lowering, Unicode scalar-count behavior, record
    field-count behavior, and extension rules for future length-bearing
    types.
-   Structured diagnostic schema, stable error-code policy, source maps,
    Rust error mapping, and terminal rendering.
-   Exception syntax and semantics, catch matching, `throw` policy,
    `finally` control flow, stack traces, and Rust error/panic
    boundaries.
-   Class syntax, constructors, method resolution, visibility, object
    identity, ownership/aliasing, `const`, descriptor interaction, and
    any explicitly approved inheritance or interface model.
-   Rust library manifest/API contracts, ABI/linking strategy,
    dependency/version resolution, global install layout, CLI commands,
    and native-code trust model.
-   Build fingerprints, cache invalidation, concurrent-build
    coordination, artifact publication, process management, and safe
    clean behavior.

Do not guess these details during implementation. Resolve them in the
relevant stage LLD first.
