# Graphite Foundation LLDs — Updated Draft Bundle

This archive contains five LLDs for use as context by a Graphite implementation agent.

1. `LLD-01-purpose-of-graphite.md` — language purpose, design principles, initial scope, compilation model, and agent instructions.
2. `LLD-02-general-syntax.md` — source syntax, naming, primitive types, statements, imports, conversion/casting rules, and open decisions.
3. `LLD-03-library-policy-and-creation.md` — domain-level `grf.json`, multi-library registry, API declarations, Rust mappings, locked import/call syntax, validation, and library authoring workflow.
4. `LLD-04-base-version-stages.md` — implementation stages and acceptance expectations.
5. `LLD-05-lexer.md` — detailed lexer architecture, tokens, scanning rules, source spans, diagnostics, tests, and boundaries with JSON manifest parsing.
6. `v0.1-acceptance-tests.md` — required acceptance cases for the frozen v0.1 resolutions. These cases are specified tests, not evidence that a compiler exists or that any case has been executed.

The root `test.grf` file is a v0.1 sample program. It is not a language specification. Scratch constructs that are absent from these LLDs are outside v0.1.

## Decisions reflected in this revision

- Graphite source files use `.grf`.
- The built-in domain is `grf`.
- A domain-level `grf.json` manifest describes the domain and multiple libraries, their purposes, exported functions, parameter types, return types, documentation, and Rust mappings.
- `import grf.io;` imports the library, but calls use `io.print(...)`, `io.println(...)`, and `io.input(...)`.
- An import path is exactly `domain.library`. Imports appear before declarations. Two imports that bind the same local library name are a compile error.
- `auto` is a reserved keyword. `auto T` is a target-directed conversion modifier inserted at the use site; it is not dynamic typing or type inference. Bare `auto` is not valid.
- `int` / `int64`, `uint` / `uint64`, and `flt` / `flt64` are aliases of one canonical type each. Different widths stay distinct.
- `++` and `--` are supported on mutable integer variables, in prefix and postfix form. `+=`, `-=`, `*=`, `/=`, `%=`, `->`, `:`, and `?` are outside v0.1.
- `class`, `private`, `this`, `public`, `new`, and `constructor` are ordinary identifiers in v0.1.
- A v0.1 executable is one entry `.grf` file with exactly one `void main()`. Top-level functions may call functions declared later in that file.
- Interpolation uses the same formatting rules as conversion to `auto str`.
- `io.print` and `io.println` accept supported printable values through conversion to text. The Rust function receives the target type.
- Rust remains the implementation and native compilation/linking target.
- The lexer, parser, semantic analysis, manifest resolution, and code generation remain separate compiler phases.

These LLDs are design drafts, not evidence that an implementation exists or has passed tests. Where conversion-failure behavior, integer overflow, duplicate imports of the same library, or manifest schema details remain open, the documents explicitly identify them rather than silently inventing behavior.
