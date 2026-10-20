# LLD-07: Stage 3 — Minimal End-to-End Program

**Project:** Graphite Lang  
**Stage:** 3 — Minimal End-to-End Program  
**Version:** Graphite v0.1  
**Priority:** Critical  
**Status:** Implementation specification

---

## 1. Objective

Implement the first complete, executable Graphite compilation pipeline.

After this stage, a developer must be able to write a `.grf` source file, compile it through the Graphite compiler, generate Rust, compile the generated Rust into a native executable, and run the resulting program through the `grf` CLI.

This milestone introduces or connects three major capabilities:

1. **CLI execution:** `grf run main.grf` orchestrates the complete compilation and execution pipeline.
2. **Graphite Core:** A Rust-backed standard library initially exposing `grf.io` through the approved import mechanism.
3. **Manifest-driven transpilation:** Graphite source is translated into Rust using the compiler's supported generation strategies and metadata defined in `grf.json`.

The implementation must preserve the existing lexer, parser, AST, and manifest infrastructure wherever possible.

### Definition of success

Given:

```grf
import grf.io;

void main() {
    io.println("Hello, Graphite!");
}
```

Running:

```bash
grf run main.grf
```

must successfully compile and execute the program, producing:

```text
Hello, Graphite!
```

Invalid entry-point declarations must fail with Graphite semantic diagnostics before Rust code generation begins.

---

## 2. Scope

### 2.1 Included

- One entry `.grf` file per compilation.
- Imports at the beginning of the source file.
- Top-level function declarations.
- Exactly one parameterless `void main()` entry point.
- Initial `grf.io` functions: `print`, `println`, and `input`.
- Domain-level `grf.json` manifest loading and validation.
- A shared transpilation registry for language and library mappings.
- Minimal semantic analysis required by the supported subset.
- Deterministic Rust code generation from the validated AST.
- Generated Cargo project creation.
- Native compilation and execution.
- Useful compiler and runtime diagnostics.
- Unit, integration, and end-to-end tests.

### 2.2 Excluded

Do not implement unrelated features during this milestone:

- Classes, inheritance, interfaces, and generics.
- Multi-file Graphite programs and user-defined packages.
- Async execution and concurrency.
- Networking, filesystem, and operating-system APIs.
- Advanced optimization passes.
- AI/ML types, model training, tensor operations, and monitoring.
- Dynamic typing or runtime reflection.
- A general-purpose plugin execution engine.
- Arbitrary executable code embedded in JSON manifests.

The architecture must support future extensions without requiring these features now.

---

## 3. Core design principles

The following decisions are normative.

### 3.1 Rust is the compilation target

Graphite source is parsed into an AST, validated, and translated into Rust source. Cargo/rustc then compiles that source into a native executable.

The transpiler must not use LLM-generated code, regex-based source replacement, or direct token-to-string substitutions as a substitute for AST-based code generation.

### 3.2 `grf.json` is the declarative source of truth

The manifest system describes:

- Graphite-to-Rust type mappings.
- Supported operator mappings.
- Language construct generation strategies.
- Library exports and their signatures.
- Rust implementation paths.
- Declared argument conversions.
- Library dependency metadata, where required by the finalized schema.

The Rust compiler implementation provides the actual parser, semantic rules, validation, and supported code-generation strategies.

A manifest declares which supported strategy to use; it does not contain arbitrary executable compiler code.

### 3.3 Libraries are implemented in Rust

Graphite Core is a Rust crate that implements the initial standard-library functionality.

For example:

- Graphite-facing API: `io.println(...)`
- Manifest entry: `rustPath` identifying the implementation.
- Rust implementation: a function exposed by the `graphite-core` crate.

The Graphite-facing library name and the Rust crate/module name are distinct concepts.

### 3.4 Invalid programs must not reach code generation

Semantic validation must succeed before Rust code generation starts. Invalid `main` declarations, unresolved library calls, and other detected semantic errors must produce Graphite diagnostics.

---

## 4. Compilation architecture

The end-to-end pipeline is:

```text
main.grf
   |
   v
Source loading
   |
   v
Lexer
   |
   v
Parser
   |
   v
Program AST
   |
   v
Manifest loading and validation
   |
   v
Transpilation registry construction
   |
   v
Semantic analysis
   |
   +---- Errors ----> Graphite diagnostics; stop
   |
   v
Validated / typed program representation
   |
   v
Rust code generation
   |
   v
Generated Cargo project
   |
   v
Cargo build
   |
   v
Native executable
   |
   v
Program execution
```

Manifest loading may occur before parsing if the implementation requires it, but the semantic analyzer must have access to validated manifest metadata when resolving imported APIs.

### Responsibility boundaries

| Component | Responsibility |
|---|---|
| Lexer | Convert source text into tokens |
| Parser | Construct the existing Graphite AST |
| Manifest subsystem | Load and validate `grf.json` files |
| Transpilation registry | Provide validated language and library mapping lookups |
| Semantic analyzer | Validate declarations, entry points, types, imports, and calls |
| Code generator | Translate validated AST nodes into Rust |
| Graphite Core | Implement runtime library functions in Rust |
| Build subsystem | Create the generated project and invoke Cargo |
| CLI | Coordinate stages, execute the binary, and propagate results |

Do not merge these responsibilities into a single compiler function or add special cases for individual libraries throughout the pipeline.

---

## 5. CLI requirements

The `grf` CLI must support the end-to-end execution command:

```bash
grf run main.grf
```

The command must:

1. Validate the input path and read the source file.
2. Invoke the existing lexer and parser.
3. Load and validate the required manifests.
4. Build the transpilation registry.
5. Run semantic analysis.
6. Stop and report diagnostics if semantic analysis fails.
7. Generate Rust source.
8. Create or update the generated Cargo project.
9. Compile the generated project.
10. Execute the native executable.
11. Forward the program's stdout and stderr.
12. Return a nonzero exit status for compilation or execution failures.

The existing `--ast` functionality must continue to work. `--rs` runs the
program the same way and also writes the generated Rust source beside
the entry file, replacing the `.grf` extension with `.rs`. Generated
build files stay in the compiler build directory unless that flag is
set.

If the CLI already has a `run` command, extend its implementation instead of creating a competing command.

### Working directory

The executed program's current working directory must default to the directory containing the entry `.grf` file, consistent with the existing CLI contract.

It must not accidentally use the generated Rust directory, build-cache directory, or an unrelated shell working directory.

### Build artifacts

- Use a predictable compiler-managed build directory.
- Avoid placing generated Rust files beside the user's source by default.
- Do not embed developer-specific absolute paths into generated Cargo manifests.
- Propagate Cargo errors and process exit codes.
- Do not report successful execution before the executable has actually run successfully.

---

## 6. Minimal supported Graphite syntax

The initial end-to-end subset must support the constructs required by the following program:

```grf
import grf.io;

void main() {
    io.println("Hello, Graphite!");
}
```

The compiler must preserve the existing frozen syntax.

### Required constructs

- Imports at the beginning of a source file.
- Top-level function declarations.
- Function parameter lists.
- Function return types.
- Function bodies and blocks.
- Expression statements.
- Function calls.
- String literals.
- Return statements where required by existing supported functions.
- The existing primitive type syntax needed for the supported subset.

Do not introduce new language syntax just to make code generation easier.

This milestone does not require the entire language to be semantically complete. However, it must provide a sound validation path for every construct accepted by the end-to-end subset.

---

## 7. Entry-point semantics

A valid program must contain exactly one `main` function with the signature:

```grf
void main() {
    // Program body
}
```

Semantic analysis must collect top-level function declarations before validating function bodies, allowing forward references.

### Required validation rules

| Case | Required result |
|---|---|
| Exactly one `void main()` with no parameters | Valid entry point |
| No `main` declaration | Semantic error |
| Two or more `main` declarations | Semantic error |
| `int main()` | Semantic error |
| `str main()` | Semantic error |
| `void main(int count)` | Semantic error |
| `void main(str value)` | Semantic error |
| Duplicate declarations named `main` | Semantic error |

The semantic analyzer must not silently select one of multiple entry points.

For duplicate declarations, report the duplicate and identify the original declaration when source information is available.

The parser remains responsible for syntax and AST construction. The semantic analyzer is responsible for enforcing the entry-point contract.

No Rust source may be generated for a program that fails entry-point validation.

---

## 8. Manifest-driven transpilation architecture

The transpilation system must use a shared registry for language constructs and library APIs.

Conceptually:

```rust
struct TranspilationRegistry {
    types: TypeRegistry,
    operators: OperatorRegistry,
    constructs: ConstructRegistry,
    conversions: ConversionRegistry,
    libraries: LibraryRegistry,
}
```

These names are illustrative. Inspect the current repository and reuse existing abstractions rather than duplicating them.

### 8.1 Language mappings

The core language metadata must describe the approved Graphite-to-Rust mappings.

Examples include:

| Graphite type | Rust type |
|---|---|
| `int` | `i64` |
| `int64` | `i64` |
| `uint` | `u64` |
| `uint64` | `u64` |
| `flt` | `f64` |
| `flt64` | `f64` |
| `flt32` | `f32` |
| `bln` | `bool` |
| `chr` | `char` |
| `str` | `String` |
| `void` | Unit return behavior |

These mappings must remain consistent with the frozen type specification.

Aliases must normalize to the same semantic type before type checking.

The manifest may declare type mappings and supported construct strategy identifiers. The Rust code generator must implement the corresponding strategies.

### 8.2 Construct strategies

Use a finite, versioned set of compiler-supported strategies for constructs such as:

- Typed variable declarations.
- Function declarations.
- Return statements.
- Function calls.
- Binary and unary expressions.
- String interpolation.
- Blocks and control-flow constructs as they are implemented.

For this milestone, implement only the strategies necessary for the minimal program and existing supported tests.

Unknown strategy identifiers must result in manifest-validation errors. The compiler must not dynamically execute arbitrary code from the manifest.

### 8.3 Registry construction

The registry must:

1. Load the core language mapping metadata.
2. Load required domain-level manifests.
3. Validate schema versions and required fields.
4. Validate declared types and strategy identifiers.
5. Build lookup structures for types, operators, conversions, libraries, and functions.
6. Provide resolved mappings to semantic analysis and code generation.

Avoid re-reading JSON independently in every compiler stage.

### 8.4 Library mapping

A library manifest entry must identify the Graphite-facing API and its implementation mapping.

For example, a function entry may contain:

- Name and description.
- Parameter names and Graphite types.
- Return type.
- Rust implementation path.
- Argument conversion declarations.
- Supported code-generation strategy.
- Rust crate dependency metadata, if required by the finalized manifest schema.

The exact field structure must remain compatible with the existing manifest specification. This LLD does not authorize replacing the established schema with a new incompatible format.

---

## 9. Graphite Core library

Graphite Core provides the Rust implementation of the initial built-in library APIs.

### 9.1 Initial Rust crate

Use the existing workspace layout where possible. The intended structure is conceptually:

```text
crates/
├── grf-cli/
│   └── src/main.rs
├── graphite-compiler/
│   └── src/
│       ├── manifest/
│       ├── semantic/
│       └── codegen/
└── graphite-core/
    └── src/
        ├── lib.rs
        └── io.rs
```

The Rust crate is named `graphite-core`, and its Rust module paths may begin with `graphite_core`.

Do not rename existing packages without a concrete need.

### 9.2 Initial `grf.io` API

Support these Graphite-facing functions:

```grf
io.print(value);
io.println(value);
str input = io.input(prompt);
```

The exact accepted argument types and conversion behavior must follow the approved manifest and conversion rules.

`print` and `println` write to standard output. `input` displays a prompt and reads a line from standard input.

The runtime must have defined behavior for input failures. Do not silently return fabricated data without documenting that behavior.

### 9.3 Rust implementations

Implement the functionality in ordinary Rust code inside Graphite Core.

For example, the Rust API might expose:

```rust
pub fn println(value: String) {
    std::println!("{}", value);
}
```

This is illustrative. The actual signature must match the final manifest and conversion contract.

The Rust library must not depend on the Graphite lexer, parser, or AST.

### 9.4 Dependency resolution

The generated Cargo project must be able to resolve and link Graphite Core.

Support the current development workspace and an installable compiler layout without embedding developer-specific absolute paths.

Use a proper dependency-resolution strategy. A Rust path in `grf.json` identifies a function, but does not by itself make the corresponding crate available to Cargo.

Do not add remote dependency downloads or package publishing to this milestone unless already required by the existing build architecture.

---

## 10. Manifest-driven library resolution

For the following Graphite source:

```grf
import grf.io;

void main() {
    io.println("Hello, Graphite!");
}
```

The compiler must:

1. Recognize the `grf.io` import.
2. Resolve the domain and library through manifest metadata.
3. Register `io` as the imported local library name.
4. Resolve `io.println` to the exported function.
5. Validate the argument against the declared parameter type and conversion rules.
6. Obtain the Rust implementation path from the manifest.
7. Generate a Rust call using that resolved path.

The compiler must not contain special-case branches such as:

```text
if library == "io" ...
if function == "println" ...
```

to implement general library resolution or code generation.

The registry and strategy implementation must be reusable for future libraries such as `grf.math` and `grf.ml`.

### Import constraints

- Imports must occur before top-level declarations.
- The supported import syntax remains `import domain.library;`.
- Calls use the imported library name directly.
- Import aliases are not supported in v0.1.
- Conflicting imports that bind the same local name must fail.
- Unknown libraries and exported functions must produce useful diagnostics.
- Duplicate exact imports must follow the established import specification.

---

## 11. Conversion handling

The conversion system must be centralized.

The manifest may declare a supported conversion strategy, but the compiler must implement its actual semantics.

For example, a manifest entry for `auto str` may identify a formatting conversion strategy for supported primitive values.

Semantic analysis must determine whether the conversion is valid and represent the approved conversion explicitly in the validated program representation or equivalent compiler metadata.

The code generator must apply only conversions approved by semantic analysis.

### Requirements

- Do not duplicate conversion logic across individual libraries.
- Do not silently truncate numeric values.
- Do not silently wrap out-of-range values.
- Do not invent values when parsing fails.
- Do not infer unspecified conversion behavior from a Rust implementation detail.
- Do not expand `auto T` semantics beyond the frozen language rules.

If the current manifest schema or conversion specification does not define a required behavior, document the blocker and implement only the already-specified behavior.

---

## 12. Rust code generation

Implement a dedicated Rust code-generation module that consumes the validated program representation.

It must generate valid, deterministic Rust code for the supported subset.

### 12.1 Minimal program

Graphite source:

```grf
import grf.io;

void main() {
    io.println("Hello, Graphite!");
}
```

One valid generated Rust equivalent is:

```rust
fn main() {
    graphite_core::io::println("Hello, Graphite!".to_string());
}
```

The exact emitted call must follow the real Rust runtime signature and approved conversion strategy. This example illustrates the expected result rather than requiring this exact conversion expression.

### 12.2 Code-generation requirements

- Map Graphite types through the language registry.
- Emit Rust constructs using registered strategies.
- Resolve library calls through manifest metadata.
- Generate exactly one Rust `fn main()` entry point.
- Avoid producing duplicate Rust entry points if the Graphite entry function is represented separately.
- Escape string literals correctly.
- Use structured AST data rather than source-text substitutions.
- Do not generate Rust from semantically invalid programs.
- Return errors through the compiler's normal result and diagnostic mechanisms.

The code generator must not hardcode `graphite_core::io::println` as a universal rule. It must use the resolved path for the particular exported function.

---

## 13. Native build and execution

The generated Rust project must be compiled using Cargo/rustc.

The build subsystem must:

- Create a valid Cargo project or use the established generated-project structure.
- Include the necessary Graphite Core dependency.
- Write generated Rust to a compiler-managed location.
- Compile the project and capture build failures.
- Locate and execute the resulting binary.
- Forward stdout and stderr.
- Return a nonzero exit status when compilation or execution fails.
- Avoid reporting success before the process exits successfully.

The compiler should reuse build artifacts when safe and appropriate, but caching optimizations must not compromise correctness.

Do not implement a custom Rust compiler or linker.

---

## 14. Required diagnostics

At minimum, implement tests for the following cases.

### Missing `main`

```grf
import grf.io;

void greet() {
    io.println("Hello");
}
```

Expected: semantic diagnostic indicating that the required `void main()` entry point is missing.

### Duplicate `main`

```grf
void main() {}
void main() {}
```

Expected: semantic diagnostic for duplicate entry-point declarations.

### Non-`void` `main`

```grf
int main() {
    return 0;
}
```

Expected: semantic diagnostic stating that `main` must return `void`.

### Parameterized `main`

```grf
void main(int count) {}
```

Expected: semantic diagnostic stating that `main` cannot have parameters.

### Unknown library

```grf
import grf.unknown;

void main() {}
```

Expected: diagnostic identifying the unresolved library, provided the import is syntactically valid.

### Unknown function

```grf
import grf.io;

void main() {
    io.unknownFunction("Hello");
}
```

Expected: semantic diagnostic identifying the unknown exported function.

### Import after a declaration

```grf
void main() {}

import grf.io;
```

Expected: parser diagnostic because imports must precede declarations.

### Valid program

```grf
import grf.io;

void main() {
    io.println("Hello, Graphite!");
}
```

Expected: successful semantic analysis, Rust generation, native compilation, and execution.

The compiler must report errors in Graphite terms wherever possible. Do not rely on downstream Rust compilation to catch source-language errors.

---

## 15. Testing strategy

### Unit tests

Cover:

- Manifest parsing and validation.
- Registry construction and lookup.
- Type normalization.
- Entry-point validation.
- Import resolution.
- Function signature validation.
- Conversion validation.
- Rust code generation for supported constructs.
- Generated Cargo project configuration.
- Build and execution error propagation.

### Integration tests

Use temporary source files and run the actual CLI.

Required successful test:

```bash
grf run main.grf
```

Assert that the process exits successfully and prints:

```text
Hello, Graphite!
```

Required failure tests must verify that invalid entry points and unresolved library calls fail before Rust code generation or native execution.

### Regression tests

All existing lexer, parser, AST, manifest, and semantic tests must continue to pass.

Do not remove or weaken existing tests to make the end-to-end example succeed.

### Required validation commands

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

Run the actual end-to-end CLI test after the workspace tests.

---

## 16. Acceptance criteria

This milestone is complete only when:

- [ ] The existing lexer and parser are integrated without regressions.
- [ ] One entry `.grf` file can pass through the complete pipeline.
- [ ] Imports are restricted to the beginning of the file.
- [ ] Top-level functions are supported within the agreed subset.
- [ ] Exactly one parameterless `void main()` is required.
- [ ] Missing `main` fails with a semantic diagnostic.
- [ ] Duplicate `main` fails with a semantic diagnostic.
- [ ] Non-`void` `main` fails with a semantic diagnostic.
- [ ] Parameterized `main` fails with a semantic diagnostic.
- [ ] `grf.io` resolves through the manifest system.
- [ ] `io.println` resolves to the declared exported function.
- [ ] Library function paths are not hardcoded in general transpilation logic.
- [ ] The initial Graphite Core library is implemented in Rust.
- [ ] Language and library mappings are exposed through the shared registry.
- [ ] Only validated ASTs reach code generation.
- [ ] Valid Rust source is generated deterministically.
- [ ] The generated project compiles into a native executable.
- [ ] `grf run main.grf` executes the program successfully.
- [ ] The example prints `Hello, Graphite!`.
- [ ] Compilation and execution errors propagate correctly.
- [ ] Existing regression tests pass.

---

## 17. Implementation order

Implement this milestone in the following order to achieve a working vertical slice as early as possible.

### Step 1 — Inspect and preserve existing infrastructure

Inspect the repository, current LLDs, AST definitions, manifest schema, CLI implementation, and tests.

Identify which stages already exist and which are missing. Do not rewrite completed stages.

### Step 2 — Entry-point validation

Implement the minimal semantic checks for a single parameterless `void main()`.

Add tests for missing, duplicate, non-`void`, and parameterized entry points.

### Step 3 — Graphite Core I/O

Implement the initial Rust-backed I/O API and ensure the declared signatures match the manifest.

### Step 4 — Manifest and transpilation registry

Connect the existing `grf.json` mechanism to library resolution and the Rust code-generation strategies.

Do not create a competing manifest format.

### Step 5 — Minimal Rust code generation

Generate Rust for the minimal valid program, including the entry point and the manifest-resolved output call.

### Step 6 — Cargo build integration

Create or update the generated Cargo project, resolve the Graphite Core dependency, and compile the generated Rust.

### Step 7 — CLI execution

Connect the entire pipeline to `grf run main.grf` and forward the program's output and exit status.

### Step 8 — End-to-end and regression tests

Verify the successful program, all required entry-point failures, import failures, and existing tests.

Do not postpone native execution until unrelated language features are complete.

---

## 18. Completion report

When implementation finishes, report:

1. Files created and modified.
2. Existing compiler stages reused.
3. New semantic rules implemented.
4. Graphite Core functions implemented.
5. Manifest schema and registry integration completed.
6. Example generated Rust source.
7. The actual result of running `grf run main.grf`.
8. Unit, integration, and regression test results.
9. Remaining limitations and unresolved language-design decisions.

Do not claim that the full semantic analyzer, standard library, or transpiler is complete if only the minimal subset is implemented.

**Final definition of done:** A valid Graphite source file passes through lexing, parsing, semantic validation, manifest-driven transpilation, Rust code generation, native compilation, and execution. Invalid entry-point declarations fail with Graphite semantic diagnostics before code generation begins.

## 19. Stage 3 binding decisions

These decisions are normative for this milestone. Where an earlier section of this LLD says the manifest may grow `language`, `types`, `operators`, `constructs`, or strategy-name fields, this section wins. Those fields are not added in Stage 3.

1. Programs are not rejected only because they are larger than the hello-world sample. `test.grf` compiles and runs. Its constructs are already specified: forward calls, parameters, `return`, integer multiplication, `auto str` of an integer, interpolation of `str`, and `io.println`. A parsed construct whose semantics are not specified produces a source-spanned diagnostic. The compiler does not emit Rust for that program and does not leave the error for rustc.
2. `auto str` accepts `str` unchanged, signed and unsigned integers as base-10 text (`123` becomes `123`), and `bln` as `true` or `false`. Floating-point and `chr` formatting stay unspecified, so those conversions are diagnostics. No other implicit conversion is invented.
3. `io.input` returns the line as `str` on success, including a successful empty line. A failed read reports the I/O error and exits with status 1. It does not return `""` in place of the error. The manifest return type stays `str`.
4. A duplicate exact import is a diagnostic. Conflicting imports that bind the same local library name remain diagnostics. Import syntax stays `domain.library` with no aliases.
5. Generated Cargo projects live at `target/grf/builds/<entry-key>/`. The key is the FNV-1a 64 hex of the canonical entry path. `graphite-core` is a relative path dependency. The child process working directory is the entry file's directory.
6. Unknown `grf.json` fields are rejected. That keeps the existing schema closed without adding sections. Type, operator, and construct strategies stay in the compiler.