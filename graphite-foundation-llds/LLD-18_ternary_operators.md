# Graphite Lang --- Low-Level Design

## Ternary Conditional Operators

**Status:** Proposed implementation specification\
**Scope:** Conditional expressions and no-`else` conditional statements\
**Out of scope:** Inline functions/lambdas, array callbacks, pattern
matching, and unrelated control-flow features

## 1. Purpose

Specify the syntax, semantics, compiler integration, diagnostics, and
tests for ternary conditional operators in Graphite Lang.

Graphite supports two forms:

1.  Value-producing expression:
    `condition ? value_if_true : value_if_false`
2.  No-`else` conditional statement: `condition ? action;`

The first selects and evaluates one of two expressions. The second
evaluates an action only when the condition is true; otherwise, it does
nothing.

Integrate this feature into the existing lexer → parser → AST →
declaration collection → semantic analysis → typed AST → Rust code
generation pipeline. Do not implement it through regex-based source
rewriting or emit code from an untyped AST.

## 2. Goals

-   Provide concise, statically typed conditional expressions.
-   Support nested ternaries with deterministic parsing.
-   Support a no-`else` conditional action.
-   Evaluate only the selected branch.
-   Produce Graphite diagnostics for invalid conditions and incompatible
    branch types.
-   Preserve existing operator precedence, conversion rules, error
    behavior, and compiler architecture.

## 3. Non-goals

This LLD does not implement lambdas, inline functions, array callbacks,
pattern matching, dynamic typing, or unrelated conversion rules.

## 4. Syntax

### 4.1 Value-producing form

``` grf
bool result = 1 == 2 ? true : false;
int score = points >= 50 ? 1 : 0;
str label = is_ready ? "ready" : "waiting";
```

Grammar sketch:

``` text
conditional_expression :=
    logical_or_expression "?" expression ":" conditional_expression
```

This is illustrative grammar and must be integrated with the actual
parser productions. The false branch recurses at the
conditional-expression level, making the operator right-associative.

The condition must be `bool`. The branches must have the same type or be
compatible under Graphite's existing type-directed conversion rules.

### 4.2 No-`else` form

``` grf
is_ready ? io.println("Ready");
score < 0 ? io.println("Warning: negative score");
```

If the condition is true, evaluate the action once. If false, do nothing
and continue with the next statement.

This form is a **statement**, not a value-producing expression. It must
not be accepted where a value is required:

``` grf
// Invalid: the no-else form does not produce a value.
int result = condition ? calculate();
```

A missing `:` must not cause the compiler to invent a default value.
Follow Graphite's existing statement terminator rules.

### 4.3 Nested ternaries

``` grf
int category =
    score >= 90 ? 1 :
    score >= 75 ? 2 :
    score >= 50 ? 3 : 4;
```

The operator is right-associative:

``` grf
a ? b : c ? d : e
```

means:

``` grf
a ? b : (c ? d : e)
```

Parentheses can make grouping explicit.

## 5. Precedence and associativity

Add ternary conditional syntax at a precedence lower than logical OR and
higher than assignment, subject to the actual existing grammar. Do not
change the precedence of existing operators.

Illustrative precedence, highest to lowest:

1.  Primary expressions and grouping
2.  Unary operators
3.  Multiplicative operators
4.  Additive operators
5.  Comparisons and equality
6.  Logical AND
7.  Logical OR
8.  Ternary `? :`
9.  Assignment, if assignment is an expression in the current grammar

Examples:

``` grf
bool result = a > 0 && b > 0 ? true : false;
int result2 = condition ? a + b : c * d;
```

The parser must correctly terminate the true branch at the corresponding
`:` and parse the false branch recursively. The no-`else` statement must
be recognized by an explicit grammar/parser rule, not regex or detached
token-text heuristics. If the current grammar cannot distinguish it
unambiguously, document and resolve the ambiguity at the parser level.

## 6. Semantic rules

### 6.1 Condition type

The condition must evaluate to `bool`.

Valid:

``` grf
bool enabled = true;
enabled ? io.println("Enabled");
int value = enabled ? 10 : 20;
```

Invalid:

``` grf
int value = 1 ? 10 : 20;
```

Do not introduce truthiness for integers, strings, or other types unless
it is already part of the language specification.

### 6.2 Branch type compatibility

For value-producing ternaries:

-   Type-check both branches.
-   Determine the result type at compile time.
-   Require equal or compatible branch types under existing conversion
    rules.
-   Do not add implicit conversions specifically for ternaries.
-   If incompatible, emit a Graphite semantic diagnostic that identifies
    the branch types where practical.

Examples:

``` grf
int result = condition ? 10 : 20;
str label = condition ? "yes" : "no";
```

Invalid branch combinations must fail during semantic analysis, not
later as Rust type errors.

### 6.3 Lazy evaluation

For `condition ? then_expr : else_expr`:

1.  Evaluate the condition exactly once.
2.  If true, evaluate only `then_expr`.
3.  If false, evaluate only `else_expr`.

The unselected branch must not execute side effects, perform I/O, mutate
state, call functions, or raise runtime errors.

For `condition ? action;`:

1.  Evaluate the condition exactly once.
2.  Execute the action exactly once if true.
3.  Do not evaluate the action if false.
4.  Continue execution after the statement.

### 6.4 Nested ternaries

Nested expressions follow right associativity. Each nested condition is
evaluated only if control flow reaches it, and only its selected branch
executes.

### 6.5 Conversions and mutability

Use Graphite's existing centralized type-directed conversion rules if
branch conversion is permitted. Do not add ternary-specific exceptions.

Preserve existing `const`, aliasing, and mutation restrictions. A
ternary must not introduce a mutation path that semantic analysis would
otherwise reject.

## 7. AST and typed AST

Adapt names to the repository's conventions.

### 7.1 Value-producing expression

Conceptual AST:

``` text
TernaryExpr
  condition: Expr
  then_branch: Expr
  else_branch: Expr
```

The typed node should record the resolved result type and typed
children. It is code-generation-ready only after semantic validation
succeeds.

### 7.2 No-`else` statement

Conceptual AST:

``` text
ConditionalActionStmt
  condition: Expr
  action: Expr or existing expression-statement representation
```

Reuse existing AST nodes where they cleanly represent the construct;
avoid duplicate concepts unnecessarily.

### 7.3 Parser implementation

-   Add the ternary production at the correct precedence level.
-   Parse the true branch up to its matching `:`.
-   Parse the false branch recursively to enforce right associativity.
-   Add a deliberate statement grammar rule for the no-`else` form.
-   Keep parser errors local and recoverable where the existing parser
    supports recovery.
-   Do not use regex, source rewriting, or token-text scanning detached
    from parser state.

## 8. Rust code generation

Generate Rust only from validated typed AST nodes.

### 8.1 Value-producing form

Graphite:

``` grf
int result = condition ? left() : right();
```

Conceptual Rust lowering:

``` rust
let result = if condition { left() } else { right() };
```

### 8.2 No-`else` form

Graphite:

``` grf
condition ? io.println("Ready");
```

Conceptual Rust lowering:

``` rust
if condition {
    /* generated action */
}
```

### 8.3 Requirements

-   Emit the condition exactly once.
-   Preserve lazy branch evaluation and source evaluation order.
-   Ensure generated Rust has the resolved result type.
-   Preserve source-location reporting to the extent supported by the
    compiler.
-   Never generate code from an AST that failed semantic analysis.
-   Do not rely on Rust compiler errors as Graphite's primary type
    checker.

## 9. Diagnostics

Use the existing structured diagnostic framework and code-allocation
policy. Do not invent or reuse diagnostic IDs without checking the
repository's registry.

Cover:

-   Missing `:` in a value-producing ternary.
-   Missing true or false branch.
-   Non-boolean condition.
-   Incompatible branch types.
-   No-`else` form used in a value-required context.
-   Malformed nested ternary.
-   Invalid statement termination, where applicable.

Diagnostics should point to the relevant source span and provide concise
explanations. Include actual types and a correction hint when practical.

## 10. Testing plan

### 10.1 Parser tests

Test simple and nested ternaries, right associativity, parentheses,
precedence with arithmetic/comparison/logical operators, no-`else`
statements, missing `:`, missing branches, malformed nesting,
semicolons, and following statements.

### 10.2 Semantic tests

Test boolean conditions, rejection of non-boolean conditions, matching
branch types, existing permitted conversions, rejection of incompatible
branches, rejection of no-`else` forms in value contexts, and
preservation of `const`/mutability rules.

### 10.3 Runtime and end-to-end tests

Test true and false branch selection, exactly-once condition evaluation,
lazy branch evaluation, absence of side effects from unselected
branches, nested result correctness, conditional action execution,
false-condition no-op behavior, and successful generated Rust
compilation and execution.

### 10.4 Regression tests

Run the existing compiler/runtime suite. Ensure no changes to existing
precedence or parsing of `if` statements, function calls, declarations,
or expression statements.

## 11. Implementation sequence

1.  Audit the existing grammar, precedence, AST, semantic analysis,
    diagnostics, and Rust code generation.
2.  Confirm precedence against the current grammar.
3.  Implement the value-producing ternary expression.
4.  Add condition and branch-type checking.
5.  Implement lazy lowering to Rust.
6.  Implement the no-`else` conditional statement as a distinct
    statement form.
7.  Add parser, semantic, runtime, and regression tests.
8.  Update language documentation and examples.
9.  Run formatting, unit, integration, and end-to-end tests.
10. Report actual results and unresolved grammar issues.

Implement in verifiable increments. Do not implement lambdas or inline
functions as part of this task.

## 12. Acceptance criteria

-   Both forms parse according to the documented grammar.
-   Nested ternaries are right-associative.
-   Conditions must be `bool`.
-   Value-producing branches are statically type-checked.
-   No-`else` form is statement-only and cannot be used as a value.
-   Branches are lazy and the condition is evaluated exactly once.
-   Rust is generated from the typed AST and is valid.
-   Invalid programs produce Graphite diagnostics rather than relying on
    Rust errors.
-   Existing tests pass and new behavior is tested.
-   Documentation matches the implementation.

**Deliverable:** a tested implementation of ternary conditional
expressions and no-`else` conditional statements, with updated language
documentation. Inline functions and lambdas remain out of scope.

## 13. Locked decisions

These choices override earlier illustrative wording where they differ.
The condition type is Graphite `bln`. The examples above that say `bool`
mean `bln`.

| Area | Decision |
|---|---|
| Condition type | Must be `bln`. No truthiness for other types. |
| Precedence | Lower than logical OR and higher than assignment. Bitwise precedence stays where it already is. |
| Associativity | Right-associative. `a ? b : c ? d : e` means `a ? b : (c ? d : e)`. |
| Branch typing | When the ternary has an expected type, check each branch against that type with the existing conversion rules. Otherwise the branches must already have the same type. Do not invent a common numeric type. |
| Ordinary assignment | No new implicit conversions. `flt n = ready ? 1 : 2.5` is a type error. |
| `auto` initialization | Apply the existing target-type conversion rules to both branches. `auto int n = ready ? 10.5 : 2` is valid. |
| Nullable branches | Use existing nullable compatibility. `int? n = ready ? 1 : null` is valid because the expected type is `int?`. |
| No-`else` action | A statement, including expression statements, `return`, `break`, `continue`, nested `if` and `loop`, declarations, and blocks. It wraps that statement; it does not add special semantics per statement kind. |
| Control flow | `return`, `break`, and `continue` keep their existing context restrictions. |
| Value context | `int n = ready ? 1` is rejected. A missing `:` in a value does not invent a default. |
| Nullable narrowing | Reuse `if` narrowing. `x != null` narrows `x` in the true branch, and `x == null` narrows `x` in the false branch, for the same identifier-only conditions `if` already supports. Narrowing does not escape the branch. A call is not narrowed. |
| Evaluation | Evaluate the condition once. Evaluate only the selected branch or action. |
| Nullable `?` | `int?` and `int?(x)` stay nullable types. |
| Colon | Slice colons and record-field colons stay as they are. |
| Lambdas | Out of scope. |
