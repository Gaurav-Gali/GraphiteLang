# LLD-15 --- Stage 13: Classes and Object-Oriented Programming

**Status:** Proposed implementation specification\
**Stage:** 13\
**Depends on:** Stages 7--12, especially records, collections, universal
`const`, `Type`/`typeOf()`, structured diagnostics, and exceptions.

## 1. Purpose and governing rule

Stage 13 adds a statically typed class/object model to Graphite.
**Classes are nominal reference types; Stage 7 records remain nominal
value types.** This stage must not silently change record identity,
copying, equality, or assignment semantics.

The compiler pipeline remains: lexer → parser → declaration collection →
semantic analysis → typed AST → Rust code generation → Cargo build/run.
No source-text rewriting, regex-based transpilation, or code generation
from an invalid/incompletely typed AST is permitted.

## 2. Goals

-   Class declarations, instance fields, constructors, and instance
    methods.
-   Receiver syntax and statically resolved member access/calls.
-   Visibility and encapsulation.
-   Explicit identity, assignment, parameter passing, returns, and
    aliasing.
-   `const` rules that remain sound despite shared object references.
-   Integration with records, arrays, maps, Stage 10 descriptors, and
    Stage 12 exceptions.
-   Predictable Rust lowering without exposing Rust panics or borrow
    behavior.

## 3. Deferred features

This stage does **not** add user-defined generics, inheritance,
interfaces, traits, mixins, virtual methods, overriding, method
overloading, constructor overloading, static fields/methods, `super`,
`new`, destructors/finalizers, properties, operator overloading,
user-defined error classes, `throw`, or general runtime reflection.

Composition is supported: class fields may contain records, arrays,
maps, or other class references. There is no implicit subclassing or
dynamic dispatch.

## 4. Locked design decisions

  -----------------------------------------------------------------------
  Concern                             Decision
  ----------------------------------- -----------------------------------
  Type model                          Nominal class reference type

  Receiver                            `this`

  Construction                        `ClassName(arguments)`

  Constructor                         At most one `init(...)`; if absent,
                                      only zero-argument construction is
                                      possible when all fields are
                                      initialized

  Visibility                          `public` / `private`; omitted
                                      modifier defaults to `private`

  Methods                             Instance methods only; one method
                                      per name per class

  Inheritance/interfaces/traits       Unsupported and deferred

  Assignment / arguments / returns    Copy the reference, preserving
                                      object identity

  Equality                            `==`/`!=` compare identity for
                                      compatible class types

  Structural equality                 Not supported for classes

  `const`                             Freezes the reachable mutable
                                      object graph, globally across
                                      aliases

  Map values                          Class references permitted

  Map keys                            Class instances prohibited

  `Type`                              Descriptor describes nominal class
                                      type only; no object/member
                                      introspection

  Rust lowering                       Generated per-class representation
                                      and shared-identity handle;
                                      implementation must preserve this
                                      spec

  Exceptions                          Existing Stage 12 runtime-error
                                      propagation; add a structured
                                      frozen-mutation error
  -----------------------------------------------------------------------

## 5. Syntax

### 5.1 Class declaration

``` grf
class Counter {
    private int value = 0;

    public init(int start) {
        this.value = start;
    }

    public int getValue() {
        return this.value;
    }

    public void increment() {
        this.value = this.value + 1;
    }
}
```

A class body contains typed field declarations, zero or one constructor,
and instance method declarations. Members can appear in any order
because declaration collection builds member tables before checking
bodies. Fields and methods are declarations, not executable statements.

Class names occupy the existing type namespace and must not collide with
another class or record type. Ordinary value/function namespace behavior
remains unchanged. Class fields must have declared, statically known
types; no untyped escape hatch is introduced.

### 5.2 Fields

``` grf
public str displayName;
private int loginCount = 0;
private const str id = "fixed";
```

Conceptual field grammar:

``` text
[visibility] [const] Type fieldName [= initializer] ;
```

Use the language's established field/type declaration punctuation. Field
initializers are evaluated separately for every instance in declaration
order, before `init`. An initializer cannot use `this` or refer to
another instance field because the instance is not initialized yet. Use
the constructor for argument-dependent initialization. Initializers must
type-check under the existing conversion rules.

No implicit zero/null default is invented by Stage 13. A field must be
initialized by a valid initializer or by the constructor before it is
read or construction completes.

### 5.3 Constructor

``` grf
public init(str name, int age) {
    this.name = name;
    this.age = age;
}
```

-   `init` is the sole constructor name; at most one per class.
-   Constructors have no explicit return type and cannot return a value.
-   Construct with `ClassName(arguments)`, not `new`.
-   If `init` is declared, calls must match its argument count and
    types.
-   Without `init`, `ClassName()` is allowed only when every field is
    initialized by its field initializer and all const-field rules are
    satisfied.
-   Constructors cannot be invoked as ordinary instance methods
    (`object.init(...)` is an error).
-   Constructor overloading is not supported.
-   `return;` may be used only if it exits after all required fields are
    definitely initialized; no value may be returned.

### 5.4 Methods and receiver

``` grf
public int totalWith(int amount) {
    return this.value + amount;
}
```

Methods have an explicit return type, including `void`, and use ordinary
Graphite parameter syntax. `this` is an implicit receiver, cannot be
redeclared, and is valid only inside a constructor or instance method.

-   Use `this.field` and `this.method(args)` inside class bodies.
-   Use `object.field` and `object.method(args)` outside.
-   Bare member names are not implicitly rewritten to `this.member`.
-   `this` cannot be reassigned.
-   Methods/fields are not first-class values; taking a member reference
    is unsupported.
-   Member lookup is static from the receiver's declared class type. No
    string-based runtime lookup or dynamic dispatch is permitted.

## 6. Visibility and encapsulation

-   `public`: accessible wherever the class itself is available under
    the existing import system.
-   `private`: accessible only inside methods and the constructor
    declared by that same class.
-   Omitted visibility means `private`.
-   No `protected`, package-private, friend, or new module-visibility
    system is introduced.
-   Public fields can be read/written subject to type and const rules.
    Private fields can only be accessed within the declaring class.
-   Public methods can be called externally; private methods only from
    within their declaring class.
-   Access is based on lexical declaration, not dynamic caller type.
    Containing another class's instance does not grant access to its
    private members.
-   Inaccessible member access is a compile-time error, with the
    declaration location available as a related diagnostic span.

``` grf
class Account {
    private int balance = 0;

    public void deposit(int amount) {
        this.balance = this.balance + amount;
    }

    public int getBalance() {
        return this.balance;
    }
}

void main() {
    Account account = Account();
    account.deposit(50);
    // account.balance = 100; // compile-time access error
}
```

## 7. Initialization, construction, and failure

### 7.1 Construction order

For `ClassName(args)`:

1.  Evaluate arguments once, left to right.
2.  Allocate an internal instance in a non-published construction state.
3.  Evaluate field initializers in declaration order.
4.  Initialize fields from initializers and constructor assignments.
5.  Invoke `init`, if declared.
6.  Mark the instance initialized and return its reference.

No reference to a partially initialized object may escape. In
particular, `this` cannot be returned, stored globally, inserted into a
collection, passed to an external function, or otherwise published
before construction completes. Calls to instance methods during
construction are allowed only after all fields are initialized;
otherwise reject the call or field read at compile time.

### 7.2 Definite initialization

-   Every field must be initialized before it is read and before
    construction completes.
-   A const field is initialized exactly once: either by its field
    initializer or in `init`, never both.
-   The compiler performs definite-initialization analysis across
    branches and early returns.
-   Constructor completion with an uninitialized field is a compile-time
    error.
-   Assigning a const field after initialization is a compile-time
    error.
-   Field initializers cannot refer to `this` or other instance fields.

### 7.3 Constructor failure

Construction either returns a fully initialized object or propagates a
structured runtime error; a partial object is never visible to Graphite
code.

-   Errors in field initializers and constructor bodies use Stage 12
    propagation.
-   A surrounding `try`/`catch` can catch the error according to its
    structured runtime kind.
-   Preserve the original stable code, source span, cause, and Graphite
    trace; do not wrap every failure in a generic constructor error.
-   Side effects completed before failure are not rolled back.
-   No destructor/finalizer hook is added.
-   Rust cleanup releases temporary resources on failed construction,
    according to the implementation's memory strategy.

## 8. Object identity versus record value semantics

  -----------------------------------------------------------------------
  Operation               Stage 7 record          Stage 13 class
  ----------------------- ----------------------- -----------------------
  Type identity           Nominal record type     Nominal class type

  Construction            Expected-type record    `ClassName(args)`
                          literal                 

  Assignment              Deep value copy         Copy reference; same
                                                  object

  Parameter passing       Deep value copy         Same identity passed by
                                                  reference handle

  Return                  Deep value copy         Same identity returned

  Mutation through        Not shared after value  Visible through aliases
  another variable        copy                    

  `==` / `!=`             Rejected for records    Identity comparison

  Structural comparison   Unsupported             Unsupported

  Identity/address API    None                    None
  -----------------------------------------------------------------------

A class construction creates a fresh identity even when field values
equal those of another instance. Assigning a class variable to another
variable does not clone the object. No user-defined clone/copy
constructor is added. Class values cannot be ordered. `==` and `!=` are
supported only for compatible class types and compare object identity,
not fields.

``` grf
Counter first = Counter(1);
Counter second = first;
second.increment();
// first.getValue() observes the updated value.
```

A record containing a class field is copied as a record value, but the
class-typed field copies the reference and therefore preserves the
referenced object's identity. Arrays/maps containing class references
likewise preserve identity of their class-valued elements. This does not
change the existing value semantics of records, arrays, or maps; it
specifies how class references behave when stored inside them.

## 9. `const` and shared references

The language's existing universal `const` rule forbids mutable-alias
bypass. Because class variables share identity, merely marking one
variable binding read-only is insufficient.

### 9.1 Object-graph freeze

Binding an instance to a `const` class variable freezes that instance
and its reachable mutable state. Freeze is object-level, monotonic, and
visible through all aliases.

-   Fields on the frozen object cannot be reassigned.
-   Reachable arrays, maps, and records follow existing deep-const
    rules.
-   Reachable class instances are recursively frozen.
-   A visited-object set prevents infinite traversal through cycles.
-   Existing aliases cannot mutate the frozen graph; an object cannot be
    unfrozen.

``` grf
Counter mutableCounter = Counter(0);
const Counter fixedCounter = mutableCounter;
// Both names refer to the same now-frozen object.
// mutableCounter.increment(); // forbidden
```

Where static analysis cannot prove an operation safe, generated runtime
checks enforce frozen state and raise the structured error in §13. The
implementation must never silently allow mutation through a mutable
alias.

### 9.2 Const fields

A const field is initialized exactly once and cannot be reassigned after
construction. If it contains an array, map, record, or class reference,
its reachable mutable state is frozen under the universal deep-const
rule. A const field alone does not make unrelated fields of its
containing object const.

### 9.3 Methods and const

Stage 13 does not add separate const-method overloads. Methods may
mutate a receiver only while it is unfrozen. Read-only methods work on
frozen instances; a method that attempts mutation fails with
`ConstViolationError`. The compiler should reject obvious mutation
through const bindings statically, with runtime enforcement for aliasing
the compiler cannot prove safe.

### 9.4 Parameters and references

No explicit pointer/reference syntax is added. Class parameters and
returns preserve reference identity. Do not add a `const ClassName`
parameter syntax unless the existing grammar already supports const in
type position. Passing a frozen object never makes it mutable again.

## 10. Nominal typing and method signatures

-   Every class declaration introduces a distinct nominal type.
    Structurally identical classes are incompatible.
-   Class types are not compatible with record types of similar layout.
-   No implicit upcast, downcast, subtype conversion, or runtime cast
    exists.
-   Forward references to class types are allowed after declaration
    collection. Class-typed fields may mutually refer to each other
    because they store references, not inline object storage.
-   A method signature includes declaring class, method name, ordered
    parameter types, and return type. Parameter names do not affect
    signature identity.
-   The receiver is implicit and has the declaring class type.
-   A class may declare only one method with a given name. A field and
    method may not share a name.
-   Calls are checked statically for member existence, visibility,
    argument count, argument types, and return type.
-   `void` methods may be called as statements but not where a value is
    required.
-   No override rules exist because inheritance is deferred.
-   A method returning a class type can be chained if each receiver's
    static type is known.

## 11. `Type` and `typeOf()`

Stage 10 reflection remains intentionally narrow.

-   `typeOf(instance)` returns a `Type` descriptor for the instance's
    declared nominal class type.
-   Descriptor display follows Stage 10 class convention,
    e.g. `<class 'Counter'>`; `Type.name()` returns the canonical class
    name.
-   `typeOf(this)` describes the current class type.
-   `typeOf(ClassName())` evaluates the expression exactly once and
    constructs the instance; known result type is not permission to
    remove observable constructor side effects.
-   A descriptor does not expose fields, methods, visibility, field
    values, object identity, frozen state, or construction history.
-   `typeOf(typeOf(instance))` continues to describe `Type` as specified
    in Stage 10.
-   Existing record descriptors and aliases remain unchanged.
-   No runtime member enumeration, reflection APIs, dynamic lookup, or
    class metadata APIs are added.

## 12. Arrays, maps, and records

-   `ClassName[]` is a valid array type.
-   `map<K, ClassName>` is valid if `K` is already an allowed map key
    type.
-   `map<ClassName, V>` is rejected: class instances cannot be map keys.
-   Array indexing and map lookup return the stored class reference and
    preserve identity.
-   `push`, `insert`, `set`, field assignment, and method calls obey
    existing collection semantics plus class aliasing and const rules.
-   `keys`, `values`, and `entries` keep their existing snapshot
    behavior; class references in values/entries preserve identity.
-   Class fields may contain records, arrays, maps, or other class
    references.
-   Map/array/record values must not clone class objects merely because
    the containing collection or record is copied.
-   Existing map key whitelist, duplicate-key behavior, iteration-order
    policy, and lookup errors remain unchanged.

## 13. Exceptions and runtime errors

Constructors and methods use Stage 12's internal
`GrfResult<T>`/`RuntimeError` propagation model. Graphite signatures
gain no `throws` clause.

-   Field initializer errors propagate from the construction expression.
-   Constructor and method errors propagate to the caller and can be
    handled by ordinary Stage 12 `try`/`catch`/`finally`.
-   Preserve original diagnostic code, span, cause, and Graphite trace.
-   Do not add `throw` or user-defined exception classes.
-   Do not catch arbitrary Rust panics and treat them as Graphite
    exceptions.

Add a structured runtime kind for attempted mutation of frozen state:

  ----------------------------------------------------------------------------------------
  Proposed code     Catch type              Message                      Condition
  ----------------- ----------------------- ---------------------------- -----------------
  `GRF-RUN-012`     `ConstViolationError`   `mutation of frozen value`   Mutation
                                                                         attempted on
                                                                         frozen object or
                                                                         reachable frozen
                                                                         state

  ----------------------------------------------------------------------------------------

Add `ConstViolationError` to the Stage 12 catch-type registry.
`RuntimeError` must catch it through the existing broad-catch behavior.
If an equivalent stable code already exists in the implementation, reuse
it instead of duplicating it. Map by structured error kind, never by
parsing message text. Update Stage 11/12 registries and tests as needed.

## 14. Rust lowering

Each class lowers to a dedicated generated representation containing
declared fields and internal initialization/frozen-state metadata. A
shared-identity handle is required. A conceptual example is:

``` rust
struct CounterData {
    value: i64,
    frozen: bool,
}
type CounterRef = Rc<RefCell<CounterData>>;
```

This is illustrative, not a mandatory exact implementation. Any
representation must preserve Graphite behavior and must not expose Rust
borrow panics.

-   Assignment clones the handle, not the object.
-   Equality compares handle/object identity without comparing fields.
-   Constructors allocate a fresh identity.
-   Member selection is resolved in the typed AST and codegen uses that
    resolution, not strings.
-   Private access is enforced by semantic analysis and may be
    reinforced by Rust visibility.
-   Generated accessors/helpers enforce initialization and frozen-state
    checks.
-   Avoid holding a mutable borrow across arbitrary Graphite calls if
    re-entrant calls could panic. Use narrow borrow scopes or
    deterministic Graphite runtime errors for unsupported conflicts.
-   Methods/constructors return `GrfResult<T>` internally; Graphite
    signatures remain unchanged.
-   Failed builds must never execute stale binaries.
-   Generated artifacts remain under `target/grf/builds/<entry-key>`;
    the runtime working directory remains the entry `.grf` file's
    directory.

### 14.1 Memory and cycles

Class references can form cycles. Plain reference counting alone does
not reclaim strongly connected unreachable cycles. Before release,
choose and document a reclamation strategy that handles cycles, such as
tracing collection, a cycle-aware arena/heap, or another sound strategy.
A prototype may temporarily use reference counting only if the
limitation is explicitly documented; do not claim full cycle
reclamation. No destructor timing or raw address is observable in
Graphite. Freeze traversal must be cycle-safe.

## 15. Lexer, parser, declaration collection, and semantic analysis

### 15.1 New syntax

Add tokens/grammar for `class`, `public`, `private`, `this`, and `init`.
Reuse existing `const`. Treat `init` consistently as a reserved
constructor keyword according to the language's keyword policy. Do not
add `extends`, `implements`, `trait`, `interface`, `super`, or `new`.

### 15.2 Declaration collection

Collect class names, field declarations/types/visibility/const status,
initializer presence, constructor signature, and method signatures
before checking bodies. This enables forward type/member references
without permitting duplicates.

### 15.3 Required semantic checks

1.  Legal declaration/modifier placement.
2.  Duplicate class/member/constructor declarations and field/method
    name collisions.
3.  Valid declared types.
4.  Member existence and visibility.
5.  `this` only in instance contexts and never redeclared.
6.  Constructor arity/type checking.
7.  Definite field initialization and const-field single assignment.
8.  No field read before initialization or escape of partially
    initialized `this`.
9.  No post-initialization const-field assignment.
10. Identity equality only for compatible class types; reject ordering.
11. Reject class instances as map keys.
12. Integrate `Type`/`typeOf()` without broad reflection.
13. Enforce freeze across aliases when static proof is unavailable.
14. Preserve Stage 12 exception propagation and stable mappings.
15. Reject accidental conversion between classes, records, and
    structurally similar types.

### 15.4 Typed AST gate

Store resolved class/member identities, access permission, receiver
type, and constructor selection in the typed AST. Code generation must
never operate on invalid or incompletely typed AST. Do not implement
classes using source rewriting, regex substitutions, or text-based
member lookup.

## 16. Diagnostics

Use Stage 11's structured diagnostic model, source spans, related spans,
stable codes, and one-time unhandled-error renderer.

Required compile-time diagnostics cover:

-   Unknown class type or member.
-   Private member access (include declaration span as related context).
-   Constructor argument count/type mismatch.
-   Duplicate constructor/class/member or field/method collision.
-   `this` outside class instance context.
-   Field read before initialization or constructor completion with
    uninitialized field.
-   Const-field reassignment.
-   Unsupported inheritance/interface/trait/overload syntax.
-   Class instance used as map key.
-   Class/record mismatch and invalid class comparisons.

Runtime mutation of frozen state uses the structured code/kind in §13.
Reuse an existing code if the implementation already defines the same
failure. Never make compiler decisions by parsing human-readable
diagnostic messages.

## 17. Formatting and built-ins

-   `str(classInstance)` is rejected unless a future explicit formatting
    protocol is specified.
-   Interpolating a class instance is rejected.
-   `io.println(classInstance)` is rejected; do not print object fields
    or internal addresses implicitly.
-   `lengthOf(classInstance)` is rejected.
-   `Type.name()` works for class descriptors as specified.
-   Public fields do not imply automatic JSON serialization or map
    conversion.

## 18. Test plan

### Positive tests

-   Class declaration, private/public fields, methods, field
    initializers.
-   Explicit constructor and default construction when all fields have
    initializers.
-   Forward class type references and mutually referring class fields.
-   Identity preserved through assignment, arguments, returns, arrays,
    maps, and record fields.
-   Distinct instances with equal fields compare unequal.
-   Mutations through aliases are visible before freeze.
-   Const binding freezes aliases and nested reachable objects.
-   Freeze traversal terminates on cycles.
-   `typeOf(instance)`, `typeOf(this)`, `Type.name()`, and descriptor
    display.
-   Constructor/method errors are catchable using Stage 12.
-   Private access from inside versus outside the declaring class.

### Negative tests

-   Duplicate declarations, field/method name collision, unknown/private
    member.
-   `this` outside instance context or redeclared.
-   Wrong constructor arity/type; constructor returns a value.
-   Read of uninitialized field, incomplete constructor, const field
    assigned twice.
-   `this` escapes before construction completes.
-   Inheritance/interface/trait/overload/override syntax.
-   Class as map key; incompatible class equality; ordering class
    instances.
-   `str`, interpolation, or `lengthOf` on a class instance.
-   Mutation through an alias after freeze.
-   Rust borrow conflicts must never leak as Rust panics.

### Cross-stage regression tests

-   Stage 7 record construction/copy semantics, nested values, equality
    rejection, and `lengthOf(record)` unchanged.
-   Stage 8 map-key rules, last-write-wins, and iteration-order policy
    unchanged.
-   Stage 9 `lengthOf` behavior unchanged for supported values.
-   Stage 10 descriptor naming, canonical aliases, and single evaluation
    of `typeOf()` unchanged.
-   Stage 11 diagnostic codes/rendering unchanged except approved
    registry additions.
-   Stage 12 catch ordering, `finally`, cause chain, and trace
    preservation unchanged.
-   Universal `const` tests pass, including no mutable-alias bypass.

## 19. Implementation sequence

1.  Add class/member/receiver grammar and AST nodes.
2.  Add class declaration collection and nominal type identity.
3.  Add member tables and duplicate diagnostics.
4.  Add static member resolution and visibility checking.
5.  Add constructor checking and definite-initialization analysis.
6.  Implement runtime object handles, identity, and freeze tracking.
7.  Implement member access/method calls and structured errors.
8.  Integrate classes with arrays, maps, and record fields.
9.  Integrate class descriptors with `Type` and `typeOf()`.
10. Add `ConstViolationError` to Stage 11/12 registries and exception
    tests.
11. Implement Rust lowering and cycle-safe memory management.
12. Add positive, negative, and cross-stage regression suites.
13. Update language reference and stable error registry.

## 20. Acceptance criteria

Stage 13 is complete only when:

-   Classes are nominal reference types and records remain nominal value
    types.
-   Construction returns a fully initialized instance or propagates a
    structured error without exposing partial state.
-   Field/method access is statically resolved and visibility checked.
-   Assignment, arguments, returns, aliasing, and equality match this
    specification.
-   `const` freezes the reachable graph and cannot be bypassed through
    aliases.
-   Class references work in supported collections/records, but cannot
    be map keys.
-   `Type` exposes only approved class type identity/name/display.
-   Constructor and method exceptions compose with Stage 12.
-   Rust panics, borrow failures, memory addresses, and dynamic dispatch
    do not leak into Graphite semantics.
-   No inheritance, interfaces, traits, generics, overloading, or
    overriding is accidentally introduced.
-   Existing record/collection behavior passes regression tests.
-   Code generation accepts only a valid typed AST and artifacts remain
    in the prescribed build directory.
