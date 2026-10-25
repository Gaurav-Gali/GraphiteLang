# Graphite Lang --- LLD-21: Iterators and Generators

**Status:** Design decisions locked for v1 implementation  
**Target:** Graphite-to-Rust compiler  
**Scope:** Built-in `iter<T>`, iterable protocol, iteration loops, and  
lazy `emit` generators

## 1. Goals

Graphite provides a built-in `iter<T>` type, manual iterator  
consumption, automatic iteration through `loop (T value : iterable)`,  
and lazy generator functions using `emit`. The compiler lowers  
generators to state machines implementing the Graphite iterator runtime  
protocol.

## 2. Built-in iterator API

`iter<T>` is built into Graphite and requires no import. Generic type  
arguments in declarations remain explicit.

```
iter<int> it = values.iter();

loop(it.hasNext()) {
    int value = it.next();
    io.println(value);
}
```

---

Method Signature Semantics

---

`iter()` `iter<T> iter()` Obtain an iterator from  
an iterable; on an  
existing iterator,  
return the same  
iterator/state

`hasNext()` `bln hasNext()` Report whether an  
element remains; may  
prefetch and cache the  
next element

`next()` `T next()` Return and consume the  
next element; runtime  
error if exhausted

---

`hasNext()` must not logically consume an element. For lazy sources, it  
may advance the producer internally and cache the value so that `next()`  
can return it.

An iterator is single-pass. Calling `.iter()` on an existing iterator  
does not rewind, clone, or create a fresh traversal; aliases share  
state.

## 3. Manual consumption

```
int[] numbers = [10, 20, 30];
iter<int> it = numbers.iter();

loop(it.hasNext()) {
    int value = it.next();
    io.println(value);
}
```

The intended output is `10`, `20`, and `30`, in that order.

Graphite currently uses `loop(condition)` rather than  
`while(condition)`. Iterator examples and generated Graphite-level  
pseudocode must therefore use `loop`; this LLD does not introduce  
`while` as an alias.

## 4. Automatic iteration

The iteration form is:

```
loop(int value : iterable) {
    io.println(value);
}
```

The compiler evaluates `iterable` exactly once, obtains one iterator,  
then repeatedly checks availability and consumes one element per  
iteration. The declared loop-variable type must be compatible with the  
iterator element type.

The compiler should reuse the parser's existing `LoopKind::Iteration`  
variant, as identified in the current compiler design, and connect it to  
the common `iter<T>` semantic and code-generation path. Do not treat the  
existing parser enum as proof that iterator semantics or codegen are  
already implemented. The old `enumerate`/`entries` handling must not be  
the only implementation path.

Conceptual lowering:

```
iter<int> __it = iterable.iter();

loop(__it.hasNext()) {
    int value = __it.next();
    // original loop body
}
```

This is explanatory pseudocode. The compiler may lower directly to  
runtime calls or Rust code instead.

## 5. Custom iterable types

A user-defined iterable type participates by providing a method whose  
result is a Graphite iterator:

```
class NumberSequence {
    iter<int> iter() {
        // Return an iterator over this sequence.
    }
}
```

The compiler checks the `.iter()` result and the element type. Iteration  
uses the returned iterator's `hasNext()` and `next()` operations, not a  
separate collection-specific `enumerate` or `entries` path.

The initial implementation should support this protocol only to the  
extent that Graphite's existing class and method system can validate and  
dispatch these methods. Any additional restrictions must be reported as  
compiler diagnostics rather than silently falling back to unrelated  
behavior.

## 6. Generator syntax

A generator declares an `iter<T>` return type and uses  
`emit expression;` to produce elements.

```
iter<int> countTo(int end) {
    loop(int i = 0; i < end; i++) {
        emit i;
    }
}
```

Manual consumption:

```
iter<int> numbers = countTo(4);

loop(numbers.hasNext()) {
    int value = numbers.next();
    io.println(value);
}
```

Automatic consumption:

```
loop(int value : countTo(4)) {
    io.println(value);
}
```

Both consumers observe `0`, `1`, `2`, and `3`.

### 6.1 `emit` rules

- `emit` is a statement only: `emit expression;`.
- `emit` is legal only in a function declared to return `iter<T>`.
- Each emitted expression must be type-compatible with `T`.
- `emit` evaluates its expression, exposes the value as the next  
element, suspends execution, and preserves the state required to  
resume after the statement.
- `emit` may appear inside supported conditionals and loops.
- A generator completes when execution reaches its end or executes a  
bare `return;`.
- `return expression;` is prohibited in a generator.
- `yield` is not the keyword for this feature.

A normal function may return an existing iterator without using `emit`:

```
iter<int> existingNumbers(iter<int> source) {
    return source;
}
```

This is an ordinary function returning an iterator, not a generator  
body. It follows normal function return rules.

### 6.2 Conditional emission

```
iter<int> evenNumbers(int end) {
    loop(int i = 0; i < end; i++) {
        if (i % 2 == 0) {
            emit i;
        }
    }
}
```

A generator need not emit an element on every loop iteration.

### 6.3 Nested iterators

If a generator has return type `iter<iter<U>>`, then  
`emit anotherIterator;` emits one `iter<U>` value. Iterators are **not**  
**implicitly flattened**. Flattening/delegation requires a separate future  
feature.

## 7. Lazy execution

Creating a generator creates its iterator state; it does not run the  
generator body to completion. Execution proceeds on demand as the  
consumer requests elements.

- Side effects in the generator body occur as the generator advances.
- An unconsumed generator may never execute its body.
- Values need not be accumulated into an intermediate array.
- The compiler preserves local variables and control-flow state across  
every `emit`.

Argument evaluation and generator-state allocation must be consistent  
with Graphite's function-call and ownership rules.

## 8. Initial supported sources

**V1 includes:** 1. Arrays. 2. Generators returning `iter<T>`.

**Deferred to later stages:** - String iteration. - Map iteration. -  
Ranges as a built-in iterable API. - Other collection types not already  
supported by the compiler/runtime.

When string iteration is added, one iteration step will produce a  
Unicode scalar value, not a byte or grapheme cluster. The precise  
Graphite character type and UTF-8 decoding behavior must be specified  
with string support.

When map iteration is added, iteration will produce an entry value  
representing both key and value, not keys or values alone. The exact  
entry-record syntax and type will be defined in the map LLD. Existing  
methods such as `entries` may remain available as ordinary APIs but must  
not define the universal iteration protocol.

A range can be implemented as a generator in user/library code before a  
dedicated built-in range iterable is added.

## 9. Collection replay and mutation

Every call to `.iter()` on a supported array or map collection creates a  
fresh iterator, starting at the beginning. This differs from calling  
`.iter()` on an existing `iter<T>`, which returns the same stateful  
iterator.

Structural mutation of an array or map while one of its iterators is  
active causes a runtime error when the iterator next checks or advances.  
Structural mutation means changes that affect the collection's shape or  
iteration order, such as adding/removing elements or resizing it.  
Mutation of an existing element without changing collection structure is  
not classified as structural mutation by this rule.

The runtime must implement this rule explicitly, for example with a  
collection mutation/version counter captured by the iterator. Do not  
rely on undefined behavior or silently iterate over a snapshot. The  
exact invalidation-check timing must be consistent and tested.

## 10. Type inference

Keep explicit type arguments for `iter<T>` declarations:

```
iter<int> it = values.iter();
```

Do not add special inference for iterator bindings or infer a missing  
`iter<T>` type from the initializer. The declared element type is  
explicit and must be checked against the initializer's iterator type.

This decision is separate from the existing/recently introduced rule  
allowing inference for static generic method calls. That  
method-inference feature remains governed by its own rules and is not  
expanded or revoked by this LLD. A non-generic generator call such as  
`countTo(4)` already has the declared return type `iter<int>`; it does  
not need generic-call inference.

## 11. Ownership and runtime representation

Iterator values use Graphite's existing heap-reference/`Rooted`  
conventions where applicable. Iterator aliases share the same mutable  
iteration state. The implementation must follow the project's existing  
ownership, tracing, and lifetime rules; it must not create a second  
incompatible object model solely for iterators.

## 12. Rust lowering

Use Rust's `Iterator` machinery behind a Graphite-compatible runtime  
adapter, and generate a state machine for each Graphite generator.

Rust's `Iterator::next()` returns `Option<Item>`, while Graphite exposes  
`next() -> T` with a runtime error on exhaustion. The adapter translates  
between these contracts.

The adapter maintains a one-element lookahead cache: - `hasNext()`  
ensures a next element is available and caches it. - `hasNext()` returns  
`false` when the underlying Rust iterator is exhausted. - `next()`  
returns and clears the cached element. - `next()` with no cached element  
attempts to obtain the next element if the API permits direct calls; if  
none exists, it raises Graphite's iterator-exhaustion runtime error.

The generated state machine preserves the current control-flow location,  
local variables needed after suspension, loop/branch state, and  
completion state. It resumes until it emits one value or reaches  
completion. The concrete Rust layout is an implementation detail, but  
observable Graphite semantics must remain stable.

### 12.1 Generator creation

Calling a generator function constructs its iterator state and evaluates  
arguments according to normal Graphite call semantics. The generator  
body begins when the iterator is first advanced, not when the generator  
function is called. No eager materialization of all emitted values is  
allowed.

## 13. Diagnostics

The compiler must report errors for: - `emit` in a function not  
returning `iter<T>`. - An emitted expression incompatible with `T`. -  
`return expression;` in a generator. - A loop variable incompatible with  
the iterator's element type. - An unsupported expression used as an  
iterable. - A `.iter()` implementation returning a non-iterator type. -  
Structural collection mutation detected during active iteration. -  
`.next()` called after exhaustion.

Diagnostics should include the source location and expected/actual types  
where relevant.

## 14. Implementation plan

1. Define the built-in `iter<T>` type and runtime representation.
2. Implement `hasNext()` lookahead and `next()` exhaustion behavior.
3. Implement fresh iterators for arrays and generators.
4. Wire `LoopKind::Iteration` into semantic analysis and the shared  
iterator codegen path.
5. Add generator validation for `iter<T>`, `emit`, and generator return  
rules.
6. Implement generator-to-Rust state-machine lowering.
7. Add mutation-version checks for array iteration.
8. Add tests for manual and automatic consumption,  
empty/single/multiple element sequences, conditionals, nested loops,  
early return, invalid emitted types, aliasing, replay, mutation, and  
exhaustion.

## 15. Acceptance criteria

The v1 implementation is complete when: - `iter<T>` works without  
imports. - Manual iteration and `loop (T value : iterable)` share the  
same protocol. - The iterable expression in an iteration loop is  
evaluated once. - Arrays provide fresh iterators; iterator `.iter()`  
returns itself. - Generator creation is lazy and `emit` preserves  
execution state. - Invalid `emit` and incompatible element types fail at  
compile time. - Exhaustion and structural mutation follow the specified  
runtime-error rules. - Rust lowering preserves element order, side  
effects, and state across suspension. - Tests cover empty, one-element,  
and multi-element iterators; early completion; repeated traversal;  
aliases; mutation; and exhaustion.

# LLD-22 Amendment: Unified Iterables, `range()`, and Default Parameters

**Status:** Design amendment based on the latest requested changes  
**Applies to:** LLD-21 iterator/generator design  
**Implementation status:** Specification only; compiler source changes require the Graphite repository.

## 1. Loop syntax

The iteration loop syntax is:

```
loop(int item : source) {
    io.println(item);
}
```

The expression after `:` is an iterable expression. It is not a declaration initializer and must not use `=`.

The compiler evaluates `source` exactly once, obtains an iterator using the iterable protocol, and then repeatedly calls `hasNext()` and `next()`.

## 2. Automatic `.iter()` dispatch

Programmers do not need to write `.iter()` in an iteration loop:

```
loop(int item : values) {
    io.println(item);
}
```

is the normal form. The compiler/runtime obtains the iterator automatically.

The following is also accepted where `.iter()` is an ordinary supported method call, but is redundant in a loop header:

```
loop(int item : values.iter()) {
    io.println(item);
}
```

The compiler must not call `.iter()` twice on an expression that already produces `iter<T>`. Calling `.iter()` on an existing `iter<T>` returns the same iterator state and does not rewind it.

## 3. Unified iterable protocol

Arrays, strings, generators, ranges, and future collection types should be consumable through the same iteration-loop protocol. A source type participates by providing or being associated with an operation that yields `iter<T>`.

The loop protocol must be independent of collection-specific helper methods such as `enumerate` or `entries`. Those APIs may continue to exist for their own purposes, but they must not be required for ordinary iteration.

### 3.1 Arrays

Arrays are iterable directly:

```
int[] values = [10, 20, 30];

loop(int value : values) {
    io.println(value);
}
```

Each iteration of an array creates a fresh iterator starting at the beginning. The array itself remains indexable and sliceable using Graphite's array indexing/slicing syntax. Iterability must not replace or remove those operations.

### 3.2 Strings

Strings should use the same loop syntax:

```
str text = "graphite";

loop(chr character : text) {
    // Process one character value.
}
```

This example is illustrative of the intended syntax; the compiler must use the character type actually defined by Graphite's type system. String iteration must not silently iterate UTF-8 bytes. The iteration unit is a Unicode scalar value, not a grapheme cluster. Exact character-type naming and indexing/slicing semantics must align with the language's string specification.

Strings retain their supported indexing and slicing operations. String iteration is not a separate user-facing API from array iteration.

### 3.3 Generators and custom iterable types

Generator functions returning `iter<T>` are iterable directly:

```
loop(int value : countTo(5)) {
    io.println(value);
}
```

A custom iterable may provide an `iter()` method returning `iter<T>`. The compiler validates the method's return type and the loop variable's compatibility with `T`.

### 3.4 Iterators

An `iter<T>` is itself accepted as a loop source. Its `.iter()` operation returns the same stateful iterator, without copying, rewinding, or restarting it. Therefore, a second loop over an already-consumed iterator continues from its current state and may immediately finish if it is exhausted.

## 4. Built-in `range()`

`range` is a built-in function and requires no import. It returns a lazy `iter<int>` and supports these forms:

```
iter<int> a = range(10);       // 0 through 9
iter<int> b = range(1, 10);    // 1 through 9
iter<int> c = range(1, 10, 1);// 1 through 9, step 1

loop(int i : range(10)) {
    io.println(i);
}
```

### 4.1 Semantics

- `range(end)` uses `start = 0`, `step = 1`, and excludes `end`.
- `range(start, end)` uses `step = 1` and excludes `end`.
- `range(start, end, step)` uses the supplied step and excludes `end`.
- The sequence is lazy and does not allocate an array containing every value.
- A zero step is a runtime error (or a compile-time error when the compiler can prove the step is the constant zero).
- Positive and negative steps should be supported. For a positive step, iteration proceeds while the current value is less than `end`; for a negative step, while the current value is greater than `end`. A step pointing away from the endpoint produces an empty iterator.
- Integer overflow must follow Graphite's specified integer-overflow policy; the implementation must not rely on accidental Rust debug/release differences.

The sample `range(1, 2, 3, 4, 5)` is not part of this API because it conflicts with the one-, two-, and three-argument range signatures. If a sequence literal/helper for explicit values is desired, it should be specified separately.

## 5. Indexing and slicing versus iteration

Iteration is a common protocol, not a replacement for collection APIs.

- Arrays and strings remain indexable and sliceable according to their respective language rules.
- A loop over an array or string uses the common iterator protocol.
- `iter<T>` is a traversal cursor, not an array: indexing and slicing are not implied for iterators.
- A future API that materializes an iterator into an array must be explicit; consuming an iterator must not silently create a collection.

## 6. Function parameter default values

Function parameters may declare default expressions:

```
int sum(int a, int b = 10) {
    return a + b;
}

int? sumOptional(int a, int? b = null) {
    if (b == null) {
        return a;
    }
    return a + b;
}
```

These examples illustrate the intended declaration syntax. The nullable check and any nullable unwrapping must follow Graphite's actual nullable semantics.

### 6.1 Call behavior

Given:

```
int sum(int a, int b = 10) {
    return a + b;
}
```

the following calls are valid:

```
sum(5);      // equivalent to sum(5, 10)
sum(5, 20);  // explicitly supplies b
```

The compiler supplies a default only when the caller omits that trailing argument. Passing `null` is not the same as omitting an argument: `null` is accepted only if the parameter type permits it.

### 6.2 Ordering and validation

- Parameters with defaults must form a trailing suffix of the parameter list in v1. For example, `f(int a, int b = 2, int c = 3)` is valid.
- A required parameter may not follow a defaulted parameter. `f(int a = 1, int b)` is a compile-time error.
- A default expression must type-check against the declared parameter type.
- `null` may be used as a default only when the parameter type is nullable, such as `int?`.
- Defaults are evaluated at the call site for omitted arguments, using the caller's normal expression-evaluation order. Each omitted argument's default expression is evaluated once per call.
- Default expressions may reference only earlier parameters if parameter references are supported by the language; they may not reference later parameters. If Graphite's current expression resolver does not support parameter references in defaults, v1 should initially allow only expressions valid in the declaration's normal scope and explicitly diagnose unsupported references.
- The compiler must diagnose calls that omit a required parameter or supply too many arguments.
- Generic functions and methods apply the same default-argument rules after explicit generic type arguments have been resolved. This does not introduce generic type-argument inference.

### 6.3 Rust lowering and API consistency

The compiler must preserve Graphite call semantics when lowering to Rust. Do not assume Rust default-parameter support exists. A viable lowering is to insert the default expression at each Graphite call site that omits an argument, while keeping the generated Rust function's concrete parameter list explicit. The exact lowering must preserve evaluation order, diagnostics, and source locations.

For public or cross-module functions, default arguments must behave consistently regardless of which module makes the call. If the compiler uses separate compilation or exposes generated Rust APIs directly, the default metadata must be available to every Graphite call site.

## 7. Required compiler/runtime changes

1. Parse and validate `loop(T item : source)` with `:`; do not reinterpret it as `loop(T item = source)`.
2. Resolve the source expression's iterable element type and obtain the iterator automatically.
3. Ensure source expressions are evaluated once.
4. Add the built-in `range` overloads for one, two, and three integer arguments, returning `iter<int>`.
5. Ensure arrays, strings, generators, ranges, and custom iterable types use the common iteration protocol as each is implemented.
6. Preserve array/string indexing and slicing independently of iteration.
7. Parse parameter default expressions and validate trailing-default ordering.
8. Insert defaults at call sites or use another lowering that preserves the specified behavior.
9. Add diagnostics for invalid range arity, zero step, incompatible loop-variable type, invalid defaults, missing required arguments, and excess arguments.

## 8. Acceptance tests

- `loop(int x : arrayValue)` iterates all array values in order.
- `loop(int x : generatorCall())` lazily consumes generator output.
- `loop(int x : range(10))` emits 0–9.
- `loop(int x : range(1, 10))` emits 1–9.
- `loop(int x : range(1, 10, 2))` emits 1, 3, 5, 7, 9.
- `range(10)` is lazy and does not materialize a backing array.
- An iterable expression is evaluated exactly once per loop.
- `.iter()` is not invoked twice for a source that is already an iterator.
- Array and string indexing/slicing continue to work after adding iteration support.
- String iteration operates on Unicode scalar values, not UTF-8 bytes.
- `sum(5)` uses the declared default and `sum(5, 20)` uses the explicit argument.
- `int? value = null` is permitted as a default only for a nullable parameter.
- A required parameter after a defaulted parameter is rejected.
- Explicitly passing `null` differs from omitting an argument.
- Missing required arguments and excess arguments are diagnosed.
