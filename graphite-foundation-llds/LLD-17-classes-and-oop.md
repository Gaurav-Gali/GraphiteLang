# LLD-17 --- Stage 13: Classes and Object-Oriented Programming

**Status:** Implementation specification, updated for inheritance and interfaces\
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
-   Single inheritance, interfaces, overriding, and method overloading.
-   Receiver syntax, visibility, and encapsulation.
-   Explicit identity, assignment, parameter passing, returns, and
    aliasing.
-   `const` rules that remain sound despite shared object references.
-   Integration with records, arrays, maps, Stage 10 descriptors, and
    Stage 12 exceptions.
-   Predictable Rust lowering without exposing Rust panics or borrow
    behavior.

## 3. Deferred features

This stage does **not** add user-defined generics, traits, mixins,
constructor overloading, destructors/finalizers, properties, operator
overloading, user-defined error classes, `throw`, default interface
methods, interface static methods, or general runtime reflection.

Single inheritance, interfaces, overriding, method overloading, static
fields, `super`, and `new` are specified in §22. Composition remains
supported: class fields may contain records, arrays, maps, or other
class references. There is no implicit subclassing of unrelated classes.

## 4. Locked design decisions

  -----------------------------------------------------------------------
  Concern                             Decision
  ----------------------------------- -----------------------------------
  Type model                          Nominal class reference type

  Receiver                            `this`

  Construction                        `new ClassName(arguments)` and
                                      `ClassName(arguments)`. A function
                                      of the same name wins a bare call;
                                      `new` still constructs

  Constructor                         At most one, named after the class.
                                      If absent, `ClassName()` and
                                      `new ClassName()` are possible when
                                      every field is initialized. An
                                      omitted modifier means `public`.
                                      Constructor overloading is not
                                      supported

  Visibility                          `public`, `protected`, and
                                      `private`. Omitted visibility means
                                      `public`. Members do not default to
                                      private

  Fields                              `Type name`, including `auto T`,
                                      with a visibility modifier before
                                      the type. Same fixed-target
                                      conversions as record fields.
                                      `@static` fields are class state

  Methods                             Instance methods, `static` methods,
                                      and `@static` methods. Overloads
                                      share a name when their parameter
                                      signatures differ. Static methods
                                      have no `this` and are called as
                                      `ClassName.method(arguments)`

  Inheritance                         Single inheritance with `extends`,
                                      `super`, `@override`, and dynamic
                                      dispatch. Multiple interfaces with
                                      `implements`. No traits or mixins

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

  Rust lowering                       Per-class data in a runtime heap.
                                      Values are handles. A tracing
                                      collector reclaims unreachable
                                      cycles. Addresses and destructor
                                      timing are not observable

  Exceptions                          Existing Stage 12 runtime-error
                                      propagation; add a structured
                                      frozen-mutation error
  -----------------------------------------------------------------------

## 5. Syntax

### 5.1 Class declaration

``` grf
class Counter {
    private int value = 0;

    Counter(int start) {
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
another class, interface, or record type. Ordinary value and function
names stay in their own namespace, so a function may use the same
spelling as a class. `new Counter(arguments)` always constructs.
`Counter(arguments)` constructs when no function of that name exists;
when both exist, the function wins and `new` is the way to construct.

Class fields must have declared, statically known types. `auto T` is
allowed and has the same meaning as on a record field: the field's type
is the fixed target `T`, not a dynamic type. `auto int` is `int64`.
Initialization and later writes use the centralized conversion registry.
A failed conversion remains `NumericConversionError` (`GRF-RUN-006`).
No untyped escape hatch is introduced.

### 5.2 Fields

``` grf
public str displayName;
private int loginCount = 0;
private const str id = "fixed";
int count = 1;
private auto int converted = "1";
```

Field grammar matches an ordinary declaration. `public` and `private`
sit before the type, the same way `auto` does. The declaration ends
with a semicolon:

``` text
[visibility] [const] [auto] Type fieldName [= initializer] ;
```

An omitted visibility modifier means `public`, so `int age;` is a
public field. `Type` includes `auto T`, written `auto int count`.
Methods keep ordinary function syntax, with the return type before the
name: `[visibility] [static] ReturnType name(parameters) { ... }`.
An omitted method visibility means `public`. Constructor and method
parameters use that same function-parameter form, `Type name`, as in
`Counter(int start)`. A field written as `name: Type` is rejected.
Field initializers are evaluated separately for every instance in
declaration order, before the constructor. An initializer cannot use `this` or
refer to another instance field because the instance is not initialized
yet. Use the constructor for argument-dependent initialization.
Initializers and later assignments type-check under the existing
conversion rules, including `auto` field writes.

A non-const field without an initializer starts at the type's default
value and is already initialized. A const field, or a field whose type
has no default (a class), must still be initialized by an initializer or
by the constructor before it is read or construction completes.

### 5.3 Constructor

``` grf
public Person(str name, int age) {
    this.name = name;
    this.age = age;
}
```

-   The constructor is named after the class; at most one per class.
-   An omitted visibility modifier means `public`.
    `private Person` is callable only from methods of that same class.
    Fields, methods, and the constructor default to `public`.
-   Constructors have no explicit return type and cannot return a value.
-   Construct with `new ClassName(arguments)` or `ClassName(arguments)`.
    If a function of that name exists, the bare call is the function and
    `new` still constructs.
-   If a constructor is declared, calls must match its argument count and
    types.
-   Without a constructor, `ClassName()` and `new ClassName()` are
    allowed only when every field is initialized by its field initializer
    and all const-field rules are satisfied.
-   Constructors cannot be invoked as ordinary instance methods.
-   Constructor overloading is not supported.
-   A child constructor calls its parent with `super(arguments)` as its
    first statement. See §22.
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
A `static` or `@static` method has no receiver. It is called as
`ClassName.method(arguments)` and cannot use `this`. Calling a static
method on an instance evaluates the receiver and then calls the method
without passing it. `@static` fields are specified in §22.

-   Use `this.field` and `this.method(args)` inside instance bodies.
-   Use `object.field` and `object.method(args)` outside.
-   Bare member names are not implicitly rewritten to `this.member`.
-   `this` cannot be reassigned.
-   Methods/fields are not first-class values; taking a member reference
    is unsupported.
-   Ordinary member lookup starts from the receiver's declared type.
    Overridden instance methods use Graphite dynamic dispatch (§22).
    There is no string-based runtime lookup.

## 6. Visibility and encapsulation

-   `public`: accessible wherever the class itself is available under
    the existing import system.
-   `protected`: accessible inside the declaring class and its subclasses.
-   `private`: accessible only inside methods and the constructor
    declared by that same class.
-   Omitted visibility on a field, method, or constructor means `public`.
    Private-by-default was considered and not adopted.
-   No package-private, friend, or new module-visibility system is
    introduced.
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

No partially constructed instance is observable. Before `init` completes:

-   `this.method(...)` is allowed only after every field is definitely
    initialized. A call or field read before that point is a
    compile-time error.
-   `this` must not escape. Reject storing `this` in another object, an
    array, or a map; returning `this`; and passing `this` to a function
    or method other than an instance method of this same class.
-   A same-class method called during `init` is still inside
    construction. It may read and assign this instance's fields. It may
    not publish `this` through another object, collection, return, or
    external call.

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
-   A class value may be assigned to an ancestor class or to an interface
    that the class, or an ancestor, implements. That assignment copies
    the reference. Downcasts and runtime casts are not supported.
-   Forward references to class types are allowed after declaration
    collection. Class-typed fields may mutually refer to each other
    because they store references, not inline object storage.
-   A method signature includes declaring class, method name, ordered
    parameter types, and return type. Parameter names do not affect
    signature identity.
-   The receiver is implicit and has the declaring class type.
-   A class may overload a method name when the parameter signatures
    differ. Return type alone does not distinguish overloads. A field
    and a method may not share a name. Duplicate exact signatures are
    rejected.
-   Calls are checked statically for member existence, visibility,
    argument count, argument types, and return type.
-   `void` methods may be called as statements but not where a value is
    required.
-   Replacing an inherited instance method, or implementing an interface
    method, requires `@override`. The parameter types and return type
    must match. See §22.
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
  Code            Catch type               Message                     Condition
  --------------- ------------------------ --------------------------- ----------------
  `GRF-RUN-014`   `ConstViolationError`    `mutation of frozen value`  Mutation of a
                                                                       frozen object or
                                                                       its reachable
                                                                       frozen state

  -------------------------------------------------------------------------------------

`GRF-RUN-012` remains `ExternalLibraryError`. `GRF-RUN-013` remains
`RuntimeInternalError`. Do not renumber existing codes.
`ConstViolationError` matches `catch (ConstViolationError ...)`,
`catch (RuntimeError ...)`, and `catch (Error ...)`. Matching uses the
structured error kind, not the message or the code text.

## 14. Rust lowering

Each class lowers to data stored in a runtime-owned heap. A Graphite
class value is a handle into that heap. The heap uses a tracing
collector: it traces from handles that are live in the generated program
and reclaims unreachable objects, including cycles. Shared reference
counts alone do not satisfy this requirement when a cycle can remain
allocated after every Graphite handle is gone.

A conceptual layout is:

``` rust
struct CounterData {
    value: i64,
    frozen: bool,
}
// Counter is a handle into the traced heap, not Rc<RefCell<CounterData>>.
```

The collector is a thread-local mark-sweep heap. A Graphite class value
is a `Rooted` handle. Roots are live stack bindings, parameters,
temporaries, record fields, array elements, and map values. Fields
inside a class object store raw handles and are traced, not rooted, so
an unreachable cycle is reclaimed when no root remains. Collection runs
at allocation. `with` and `with_mut` do not hold the heap borrow across
a nested Graphite call; re-entry is `RuntimeInternalError`
(`GRF-RUN-013`) rather than a Rust panic. Freeze traversal uses the same
edges and is cycle-safe. The collector must not expose Rust borrow
panics, object addresses, numeric identity, or destruction timing.

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

Class fields may refer to other instances, including cycles. Stage 13
requires the tracing heap in §14. An unreachable cycle is reclaimed.
Reference counting may be used inside the heap only as part of a
cycle-aware collector, not as the sole reclamation strategy. No
destructor timing or raw address is observable in Graphite. Freeze
traversal must be cycle-safe. A test may observe reclamation through a
runtime allocation counter that is not a Graphite language feature.

## 15. Lexer, parser, declaration collection, and semantic analysis

### 15.1 New syntax

Add tokens/grammar for `class`, `public`, `private`, `protected`,
`this`, `init`, `new`, `extends`, `implements`, `interface`, `abstract`,
`super`, and `@`. Reuse existing `const` and `static`. Treat `init`
consistently as a reserved constructor keyword. Do not add `trait`.

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
15. Reject accidental conversion between unrelated classes, records, and
    structurally similar types. Allow the upcasts in §10.
16. Check `extends`, `implements`, `@override`, abstract methods, `super`,
    `@final`, and overload resolution as specified in §22.

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
-   Invalid inheritance, interface, override, abstract, `super`, final,
    and overload use (§22). Trait syntax remains unsupported.
-   Class instance used as map key.
-   Class/record mismatch and invalid class comparisons.

Runtime mutation of frozen state uses `ConstViolationError` and
`GRF-RUN-014`, as specified in §13. Never make compiler decisions by
parsing human-readable diagnostic messages.

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

-   Class declaration with `Type name` fields, `private` and `public`
    modifiers, methods, and field initializers.
-   `auto int` field initialization and assignment use the fixed `int64`
    conversion registry.
-   `ClassName(arguments)` constructs the class when that class exists,
    including when a function uses the same spelling. A failed
    construction does not call that function.
-   Omitted `init` visibility is public. `private init` succeeds inside
    the class and fails outside it.
-   After every field is initialized, `init` may call `this.method()`.
-   Explicit constructor and default construction when all fields have
    initializers.
-   Forward class type references and mutually referring class fields.
-   Identity preserved through assignment, arguments, returns, arrays,
    maps, and record fields.
-   Distinct instances with equal fields compare unequal.
-   Mutations through aliases are visible before freeze.
-   Const binding freezes aliases and nested reachable objects.
-   Freeze traversal terminates on cycles.
-   An unreachable cycle is reclaimed by the tracing heap. The test
    observes a runtime allocation counter, not a Graphite address or
    destructor.
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
-   `this` escapes before construction completes: stored in another
    object, array, or map; returned; or passed outside the class.
-   C-style field declaration (`int value`) is rejected.
-   `private init` called from outside the declaring class.
-   Invalid `extends`, `implements`, `@override`, abstract construction,
    `super`, `@final`, and ambiguous overloads. Trait syntax.
-   Class as map key; incompatible class equality; ordering class
    instances.
-   `str`, interpolation, or `lengthOf` on a class instance.
-   Mutation through an alias after freeze reports
    `error[GRF-RUN-014]` and is caught by `ConstViolationError`,
    `RuntimeError`, and `Error`.
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
10. Add `ConstViolationError` as `GRF-RUN-014`, matched by
    `RuntimeError` and `Error`, without renumbering earlier codes.
11. Implement the tracing heap. Do not stop at reference counting.
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
-   Rust panics, borrow failures, memory addresses, and Rust trait objects
    do not leak into Graphite semantics. Graphite dynamic dispatch is
    intentional and stays inside the generated runtime.
-   Inheritance, interfaces, overriding, and method overloading follow
    §22. Traits, mixins, generics, and constructor overloading are not
    introduced.
-   Existing record/collection behavior passes regression tests.
-   Code generation accepts only a valid typed AST and artifacts remain
    in the prescribed build directory.
-   Unreachable class cycles are reclaimed, and no Graphite program can
    observe an address or destruction time.

## 21. Locked Stage 13 resolutions

1.  Class fields use `Type name`, for example `public int name = ""`
    and `private int age`. `public` and `private` precede the type.
    Omitted visibility means `public` for fields, methods, and the
    constructor. Methods keep return-type-before-name function syntax.
    The constructor is the class name, for example `Counter(int start)`.
2.  `new Counter(arguments)` constructs a class. A bare `Counter(arguments)`
    also constructs, unless a function named `Counter` exists. In that
    case the bare call is the function and `new` still constructs.
3.  The constructor defaults to `public`. `private Counter` is visible
    only inside its class. `protected` members are visible in the
    declaring class and its subclasses. Fields and methods default to
    `public`. `static` and `@static` methods have no `this` and are
    called as `Counter.sign(1)`.
4.  `this.method()` is allowed in the constructor after definite
    initialization. `this` cannot be stored elsewhere, returned, or
    passed outside the class before the constructor completes.
5.  `ConstViolationError` is `GRF-RUN-014`. Codes `012` and `013` stay
    unchanged. `Error` and `RuntimeError` catch the new kind.
6.  `auto T` class fields use the record conversion rules. `auto int` is
    a fixed `int64` field.
7.  Class storage is a traced heap. Unreachable cycles are reclaimed.
    Reference counting alone is not the Stage 13 memory strategy.
    Class values use reference semantics. Assignment copies the handle
    and preserves identity. Move semantics are not part of the class
    model.

## 22. Locked OOP decisions

These decisions update the class model. They supersede any earlier
sentence in this document that says inheritance, interfaces, overriding,
method overloading, static fields, or `super` are unsupported.

### 22.1 Ownership

Class values are `Rooted` handles on the traced heap. Assignment,
arguments, and returns copy the reference. `==` and `!=` compare
identity for related class and interface types. An upcast from a class
to an ancestor, or from a class to an interface it implements, is the
same handle. There is no move of the object and no copy of its fields.

### 22.2 Construction

Both forms construct an instance:

``` grf
User user = new User("Gaurav", 21);
User other = User("Gaurav", 21);
```

If a function and a class share a name, the function wins the bare call.
`new` still constructs. Abstract classes cannot be constructed with
either form. A child constructor may still run an abstract parent's
constructor through `super`.

### 22.3 Annotations

Recognized annotations are compiler-checked. They are not Python runtime
decorators. Multiple annotations may apply to one declaration. An
unknown `@name` is a parse error. `@static` and the `static` keyword
both declare a static method. `@static` and `@override` together are
rejected.

| Annotation | Meaning |
| --- | --- |
| `@static` | Method or field belongs to the class. No `this` |
| `@override` | Required when replacing an inherited instance method or implementing an interface method |
| `@abstract` | Method has a signature and no body. Also marks a class when written before `class` |
| `@final` | Method cannot be overridden. Class cannot be extended |
| `@deprecated` | Non-fatal warning `GRF-SEM-035` when the member is used |

`@deprecated` on a class declaration itself is ignored. A class that is
both abstract and final is accepted and cannot be constructed or
extended. The `abstract` keyword and `@abstract` both mark a class.

### 22.4 Inheritance

``` grf
class Dog extends Animal {
    Dog(str name) {
        super(name);
    }

    @override
    void speak() {
        super.speak();
    }
}
```

-   Single inheritance. `extends` names one parent class.
-   A class cannot extend itself, extend a `@final` class, or form an
    inheritance cycle.
-   `super(arguments);` is allowed only as the first statement of a
    child constructor. It is not an ordinary call. There is no
    colon-initializer syntax.
-   If the parent has no accessible zero-argument constructor, the child
    must call `super` explicitly. Otherwise the compiler inserts the
    parent constructor.
-   `super.method(arguments)` calls that parent implementation directly.
    It is not a virtual call.
-   `@override` is required. The name, parameter types, and return type
    must match an overridable parent method or an interface method.
    Overriding a `@final` method is an error. Overriding without
    `@override`, or using `@override` with no matching method, is an
    error.
-   Inherited fields keep their declaring class. A child cannot declare
    a field with the same name as an ancestor field.

Construction order is the parent constructor, then the child's field
initializers, then the child's constructor body. A `return;` inside the
parent constructor finishes the parent constructor and does not skip
the child.

### 22.5 Polymorphism

A variable of a parent type may hold a child instance. A call to an
overridden instance method selects the implementation of the object's
class. The static type is what the compiler checks. The dynamic class
selects the method.

Classes in one inheritance hierarchy share one generated object layout:
the root struct, a `grf_type` tag, and the union of instance fields.
Dispatch matches that tag. It does not expose a Rust trait object, a
panic, or an address. `typeOf` still describes the static nominal type
known to the compiler.

### 22.6 Abstract classes

``` grf
abstract class Shape {
    @abstract
    flt area();

    void describe() {
    }
}
```

-   An abstract class cannot be instantiated.
-   An `@abstract` method ends with `;` and has no body.
-   A concrete subclass must implement every inherited abstract method.
-   An abstract method is `public` or `protected`, and it is not static
    or final.
-   Calling `super` on an abstract method is rejected.
-   An abstract class may contain fields and ordinary methods.

### 22.7 Interfaces

``` grf
interface Printable {
    void print();
}

class Report implements Printable {
    @override
    void print() {
    }
}
```

-   An interface declares public method contracts and no fields.
-   Methods have signatures and no bodies. There are no default methods
    and no interface static methods in this version.
-   A class may implement several interfaces.
-   The class, or an ancestor, must provide a public instance method of
    the same signature for every interface method, marked `@override`.
-   An interface is a real type. A class value may be passed where one
    of its interfaces is expected. Dispatch uses the same class tag as
    inheritance.

### 22.8 Overloading

``` grf
class Formatter {
    str format(int value) {
        return "Integer";
    }

    str format(str value) {
        return value;
    }
}
```

-   Overloads are resolved at compile time from the argument types.
-   One matching method uses the existing conversion rules.
-   Several methods require each argument to be an exact match, an
    allowed upcast, an optional wrap, or an `auto` conversion. Ambiguous
    calls and calls that match nothing are errors.
-   Return type alone cannot distinguish overloads.
-   Constructor overloading is not supported.
-   Effect analysis treats every overload of a name as mutating when any
    overload mutates. That is conservative.

### 22.9 Static fields

``` grf
class Counter {
    @static
    int count = 0;
}
```

A static field belongs to the class. Read and write it as
`Counter.count`. An instance access evaluates the receiver and then uses
the class slot. The slot is thread-local, initialized on first read, and
matches the thread-local heap. It is not one process-wide mutable static.
A write before the first read replaces the slot and can skip the
initializer's side effects. A const static field cannot be assigned. A
class-typed static field needs an initializer. A static field cannot
reuse an ancestor's field name.

### 22.10 Diagnostics

| Situation | Code |
| --- | --- |
| Bad override | `GRF-SEM-028` |
| Abstract class or missing implementation | `GRF-SEM-029` |
| Final class, final method, or illegal inheritance | `GRF-SEM-030` |
| Illegal `super` | `GRF-SEM-031` |
| Ambiguous call or no matching overload | `GRF-SEM-032` |
| Interface error | `GRF-SEM-033` |
| Use of a `@deprecated` member | `GRF-SEM-035`, warning, build continues |

### 22.11 Example

`examples/oop/main.grf` covers abstract classes, `extends`, `super`,
`@override`, a static method that takes the parent type, an interface,
overloads, `new`, bare construction, and a static field. The program
prints:

``` text
My name is Buddy
Buddy says woof!
My name is Luna
Luna says meow!
Annual Report
Integer
Hello
2
2
2
```

String concatenation with `+` is not part of this design. Text is built
with interpolation, `f"...{expr}..."`.
