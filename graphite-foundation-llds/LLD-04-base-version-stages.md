# LLD 04 — Base-Version Completion Stages

**Status:** Roadmap draft  
**Target:** Graphite v0.1  
**Purpose:** Establish a manageable implementation order without specifying each component's internals.

## 1. Guiding approach

Graphite should be developed in small, testable stages. The priority is to obtain a minimal end-to-end compiler early, then expand the language segment by segment.

Each stage should leave the repository in a buildable state. A stage is not complete merely because code has been written: its behavior must have tests and the acceptance criteria for that stage must pass.

## 2. Stages

### Stage 1 — Lexer

Implement source reading, tokenization, token kinds, source positions, whitespace handling, comments, literals, identifiers, keywords, operators, punctuation, and lexical diagnostics.

The v0.1 keyword inventory includes `auto` and excludes `class`, `private`, `this`, `public`, `new`, and `constructor`. The lexer emits `PlusPlus` and `MinusMinus`. It does not emit token kinds for compound assignment, `->`, `:`, or `?`.

**Outcome:** Graphite source can be converted into a reliable token stream with source spans and lexical errors. The corrected `import grf.io;` / `io.println(...)` token stream is covered by tests.

### Stage 2 — Parser and AST

Define the grammar for the first supported syntax segments and parse tokens into a structured Graphite AST.

**Outcome:** Valid source produces AST nodes; invalid syntax produces source-located parser diagnostics. No Rust generation is required yet.

### Stage 3 — Minimal end-to-end program

Support the smallest useful program: one entry `.grf` file, imports at the top, top-level functions, exactly one `void main()` with no parameters, and a basic output call through the agreed library mechanism.

**Outcome:** A `.grf` program can pass through the compiler pipeline, generate Rust, compile, and run. This milestone should arrive early. Missing, duplicate, and non-`void` / parameterized `main` declarations fail with semantic diagnostics.

### Stage 4 — Declarations, types, and functions

Implement variable and constant declarations, scopes, primitive types, function declarations/calls, parameters, return checking, and the required semantic validation.

Normalize `int` / `int64`, `uint` / `uint64`, and `flt` / `flt64` to one canonical type each before assignment, argument, operator, and return checks. Collect top-level declarations before checking bodies so a function may call another function declared later in the same file. Insert `auto T` conversions at the call site or initializer as explicit typed-AST nodes.

**Outcome:** Typed Graphite programs can declare values and call functions under documented rules.

### Stage 5 — Expressions and control flow

Implement expression precedence, operators, conditionals, loop forms, `break`, `continue`, and string interpolation according to the frozen syntax specification.

Prefix and postfix `++` and `--` are part of this stage and apply only to mutable integer variables. Compound assignment, `->`, `:`, and `?` are rejected. Interpolation uses the same formatting rules as `auto str`.

**Outcome:** Basic imperative programs execute with predictable control-flow semantics.

### Stage 6 — Library interface resolution

Implement domain-level `grf.json` loading/validation, domain/library identity handling, Graphite-signature validation, Rust path mapping, dependency integration, and the initial `grf.io` manifest.

**Outcome:** Built-in and supported user libraries follow the same interface model rather than per-function compiler special cases. Three-segment imports are rejected. Imports that bind the same local name are rejected.

If library resolution is needed for the first end-to-end output milestone, implement only the minimum vertical slice earlier, then complete this stage afterward.

### Stage 7 — Runtime execution and CLI hardening

Make `grf run` resolve entry source paths, build in a managed cache, launch the executable reliably, propagate failures, and set the process working directory to the entry `.grf` directory.

**Outcome:** Execution is repeatable, relative paths behave as specified, and concurrent runs do not collide.

### Stage 8 — Collections and classes

Implement collections and classes only after their syntax, ownership, mutation, visibility, and Rust-lowering semantics have been decided.

**Outcome:** The selected data structures and object model work consistently and have positive/negative tests.

### Stage 9 — Release readiness

Improve diagnostics, add regression tests, document supported syntax and limitations, test clean builds, validate dependency behavior, and prepare packaging for the initial toolchain.

**Outcome:** A reproducible v0.1 release with known limitations and no undocumented behavior in the supported feature set.

## 3. Cross-stage requirements

Every stage must:
- follow the current LLDs and frozen syntax;
- include positive and negative tests;
- preserve source locations where practical;
- avoid silently accepting invalid source;
- keep compiler phases separated;
- update documentation when a design decision changes;
- avoid implementing deferred features as accidental side effects.

Required acceptance cases for the frozen v0.1 resolutions are listed in `v0.1-acceptance-tests.md`. Those cases become executable tests in the stage that implements the corresponding phase. A specified case is not a passing test until the repository contains an implementation that runs it.

## 4. Recommended order of work

The lexer is the first detailed implementation stage. Before expanding the parser, freeze the token inventory and lexical rules. Then build the parser and AST. Implement the earliest end-to-end executable path as soon as the minimum parser and code generation are available; do not wait until every language feature is implemented.

The library architecture must be designed early even if the full dependency workflow is implemented later. The domain-level `grf.json` contract affects import resolution, AST representation, semantic validation, code generation, and build generation. Import source paths use `import grf.io;`, while calls use `io.print(...)`. Class-related words stay unreserved until the class specification lands. `test.grf` sketches are not acceptance tests.
