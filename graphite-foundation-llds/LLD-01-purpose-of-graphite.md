# LLD 01 — Purpose of Graphite

**Status:** Foundational draft  
**Target:** Graphite v0.1  
**Audience:** Language designer, compiler implementer, AI coding agent  
**Normative language:** “MUST” means required; “SHOULD” means recommended unless a documented reason justifies otherwise; “MAY” means optional.

## 1. Purpose

Graphite is a general-purpose programming language intended to make software development—especially AI, machine learning, and data-oriented development—more productive while retaining a dependable native compilation toolchain.

Graphite source files use `.grf`. The initial compiler translates Graphite into Rust source code, then delegates compilation and linking to Cargo/rustc. Graphite defines its own syntax, type system, semantics, library interface model, diagnostics, and developer experience. Rust is the initial compilation target, not the user-facing language.

## 2. Problem statement

Graphite is intended to provide:
- A readable, consistent language for general-purpose programming.
- A foundation for future AI/ML development, data processing, tensor operations, model tooling, visualization, and deployment.
- A deterministic translation pipeline backed by a mature native toolchain.
- A library architecture that allows new capabilities to be added without a compiler change for every new function.
- Stable conventions for syntax, types, naming, module paths, library APIs, and compilation behavior.

Graphite is not merely a source-text replacement layer for Rust. The compiler must parse Graphite into structured representations and generate Rust from those representations.

## 3. Initial compilation model

```text
.grf source
   -> Lexer
   -> Parser
   -> Graphite AST
   -> Semantic validation
   -> Rust code generation
   -> generated Rust build project
   -> Cargo / rustc
   -> executable
```

The lexer turns source characters into tokens. The parser turns tokens into an AST. Semantic validation checks language rules that cannot be established by parsing alone. Code generation maps valid Graphite constructs and library calls into Rust. Cargo/rustc compile and link the generated program.

The exact internal compiler architecture may evolve, but the separation of lexical analysis, parsing, semantic validation, and code generation MUST remain clear.

## 4. Core principles

### 4.1 Deterministic translation

For the same Graphite source, compiler version, dependency set, and build configuration, Graphite SHOULD produce equivalent generated Rust and build behavior. The compiler MUST NOT use arbitrary text replacement as its primary translation method, and MUST NOT rely on an LLM to generate compiler output.

### 4.2 Explicit semantics

Every implemented feature MUST have defined syntax, valid and invalid examples, type rules, runtime behavior, diagnostics, and tests. If behavior is not specified, an implementation MUST NOT invent behavior silently.

### 4.3 Library-driven extensibility

Ordinary capabilities SHOULD be implemented as libraries. A domain-level `grf.json` manifest describes the domain and its libraries, including each library's Graphite-facing API and mapping from Graphite calls to Rust calls. The compiler resolves the manifest and generates the appropriate Rust call; Rust's toolchain compiles and links the implementation.

### 4.4 Shared policy for built-in and third-party libraries

Built-in and third-party domains MUST use the same JSON manifest convention. The manifest filename is `grf.json` for the reserved built-in `grf` domain; other domains use the same manifest filename in their domain root. One domain manifest may describe multiple libraries. The `grf` domain is reserved for Graphite-provided core libraries.

### 4.5 Strict naming policy

Graphite uses a case-sensitive, strict camel-casing policy. Module segments, function names, methods, variables, and parameters use lower camel case, such as `grf.io`, `grf.fileIo`, `println`, and `userName`. An import path names the source domain/library; after `import grf.io;`, source calls use `io.print(...)`, not `grf.io.print(...)`. Types and classes use UpperCamelCase, such as `Person` and `NeuralNetwork`. Constant naming and generic parameter casing must be explicitly frozen in the language specification.

`auto` is a reserved keyword in v0.1. It is a target-directed conversion modifier and cannot be used as an ordinary identifier.

### 4.6 Source-relative program execution

When run through `grf run path/to/main.grf`, the program's working directory MUST default to the directory containing the entry `.grf` file. The CLI should set the child executable's working directory. It MUST NOT change the parent shell's directory. Generated-code and cache locations MUST NOT change this behavior. Importing a library MUST NOT change the process working directory.

This is a CLI execution contract; it does not imply that the core library needs an OS module or a `cwd()` API.

## 5. Base-version scope

The base version establishes:
- `.grf` source handling and CLI execution.
- Lexer, parser, AST, semantic validation, and deterministic Rust code generation.
- One entry `.grf` file: imports at the top, top-level functions, and exactly one `void main()` with no parameters. A top-level function may call another top-level function declared later in the same file.
- Variables, constants, primitive types, expressions, operators, conditions, loops, functions, strings, and imports.
- Canonical primitive aliases: `int` is `int64`, `uint` is `uint64`, and `flt` is `flt64`. Different numeric widths remain distinct types.
- Prefix and postfix `++` and `--` on mutable integer variables. Compound assignment, `->`, `:`, and `?` are outside v0.1.
- Library API resolution through domain-level `grf.json` manifests. An import path is exactly `domain.library`.
- The initial built-in `io` library API containing only `print`, `println`, and `input`, imported from the `grf` domain with `import grf.io;` and called as `io.print(...)`.
- Call-site conversion for `auto T`, including library parameters declared as `auto str`. Interpolation uses those same formatting rules when the target is `str`.
- Collections and classes only after their semantics are specified and scheduled in the base-version plan. `class`, `private`, `this`, and `public` are not reserved in v0.1.

## 6. Explicitly deferred

The following are future directions, not implied v0.1 requirements:
- ML/DL-specific types and APIs.
- Parallel execution blocks and task scheduling.
- Monitoring directives or a monitoring UI.
- Package registry and remote publishing.
- Advanced OS, filesystem, networking, and process APIs.
- Inheritance, exceptions, async semantics, dynamic plugins, or a stable cross-language ABI unless separately specified.
- Class syntax, including reservation of `class`, `private`, `this`, `public`, `new`, and `constructor`.
- Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`), `->`, `:`, `?`, and import aliases.
- Graphite-authored libraries as a source-level library format. v0.1 library implementations remain Rust code reached through `grf.json`.

## 7. Success criteria

The foundation is successful when:
1. A minimal `.grf` program can be parsed, validated, transpiled, compiled, and run.
2. Language behavior is determined by documented Graphite rules, not accidental Rust behavior.
3. Library calls are resolved using `grf.json` API contracts rather than hardcoded per-function compiler logic.
4. Built-in and third-party domains share the same JSON manifest convention.
5. Domain and module names resolve deterministically.
6. Relative runtime paths follow the entry-source-directory rule.
7. Invalid programs fail with actionable diagnostics.

## 8. Agent instructions

An AI coding agent implementing Graphite MUST:
- Treat the LLDs and frozen language decisions as the source of truth.
- Avoid adding unrequested syntax or APIs.
- Keep lexer, parser, semantic validation, library resolution, and code generation responsibilities separate.
- Never claim a feature is implemented or tested unless repository evidence supports that claim.
- Record unresolved decisions rather than guessing.
- Add tests alongside each implemented language feature.
