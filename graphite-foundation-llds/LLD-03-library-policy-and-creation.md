# LLD 03 — Domain Manifest and Library Policy

**Status:** Architecture draft with import syntax locked  
**Target:** Graphite v0.1  
**Manifest format:** JSON  
**Source extension:** `.grf`  
**Core concepts:** Domains, libraries, imported namespaces, JSON API contracts, Rust implementation mappings.

## 1. Purpose

The library system is Graphite's primary extension mechanism. Built-in and third-party capabilities MUST follow one consistent manifest contract so that adding an ordinary library function does not require a dedicated compiler special case.

Each domain has a `grf.json` manifest describing the domain and the libraries it contains. A manifest may describe multiple libraries, and each library may expose multiple functions. The manifest is declarative metadata; implementations remain separate. For the initial native toolchain, Rust provides the implementations and Cargo/rustc compile and link them.

`grf.json` is not Graphite source and MUST NOT be passed through the `.grf` source lexer/parser as if it were Graphite code. It must be loaded and validated as JSON using a separate manifest-loading path.

## 2. Domain and library model



### 2.1 Domain

A **domain** is the top-level namespace/owner of a family of libraries. It provides identity and avoids naming collisions.

- `grf` is reserved for Graphite-provided core/inbuilt libraries.
- `coolDev` is an illustrative third-party domain.
- Domain identifiers are case-sensitive and follow the language's lower-camel-case policy.

A domain manifest describes the domain's purpose and its available libraries.

### 2.2 Library

A **library** is a user-facing namespace that can be imported into Graphite source. A domain can contain multiple libraries, for example:

- `grf.io` — console input/output.
- `grf.fileIo` — a possible future file I/O library; not in the initial API.
- `coolDev.netHttp` — an illustrative third-party library.

A library has a purpose/description and a set of exported functions. Each function declares its Graphite-visible parameter types and return type, documentation, and Rust implementation path.

In the current source syntax, the library's local namespace is its library name. The full import path identifies the domain and library; calls use the imported library name directly.

### 2.3 Packages and implementation crates

A Rust crate/package is an implementation/build unit, not automatically the same thing as a Graphite domain or source-level library. One Rust crate may implement multiple libraries, and a Graphite domain manifest may map its libraries to one or more Rust crates. Keep these concepts separate in compiler data structures.

The initial version does not require a remote package registry. Bundled core crates and local path dependencies are sufficient for the first vertical slice.

## 3. Locked import and call syntax

The import syntax is locked as follows:

```grf
import grf.io;

void main() {
    io.print("Hello");
    io.println(123);
    str name = io.input("Enter your name: ");
}
```

Rules:

1. `import grf.io;` identifies the `io` library in the reserved `grf` domain.
2. A v0.1 import path has exactly two segments: `import <domain>.<library>;`.
3. Calls after this import use `io.functionName(...)`.
4. `grf.io.functionName(...)` is not the call syntax for an imported library.
5. `import grf.io.print;` is invalid. Functions are not imported.
6. The import path and local call namespace are different concepts: the first resolves the library; the second names it in source code.
7. The local namespace is the library name. Import aliases are not part of v0.1.
8. Importing `coolDev.netHttp` similarly makes `netHttp.<function>(...)` available.
9. Imports are case-sensitive and follow the strict naming policy.
10. Import statements appear at the beginning of the source file, before declarations.
11. Two imports that bind the same local library name are a compile error. `import grf.io;` together with `import other.io;` is rejected.

**Open decision:** a repeated import of the same domain and library must not create a second binding. Diagnose it as redundant, or accept it as idempotent. Choose one, document it, and test it.

The lexer recognizes `import`, identifiers, and dots. It does not resolve domains or libraries. Name resolution occurs later.

## 4. Domain-level `grf.json` manifest



### 4.1 Example schema

The following illustrates the intended structure for the built-in `grf` domain. The exact schema versioning and some optional fields remain design items, but the hierarchy and core fields are the intended direction.

```json
{
  "formatVersion": 1,
  "domain": {
    "name": "grf",
    "description": "Graphite's official standard library domain."
  },
  "libraries": {
    "io": {
      "description": "Provides console input and output functions.",
      "functions": {
        "print": {
          "description": "Prints a value to standard output without a newline.",
          "params": [
            {
              "name": "value",
              "type": "auto str"
            }
          ],
          "returns": "void",
          "rustPath": "graphite_core::io::print"
        },
        "println": {
          "description": "Prints a value to standard output followed by a newline.",
          "params": [
            {
              "name": "value",
              "type": "auto str"
            }
          ],
          "returns": "void",
          "rustPath": "graphite_core::io::println"
        },
        "input": {
          "description": "Displays a prompt and reads one line of user input.",
          "params": [
            {
              "name": "prompt",
              "type": "auto str"
            }
          ],
          "returns": "str",
          "rustPath": "graphite_core::io::input"
        }
      }
    }
  }
}
```

The `auto str` parameter notation expresses target-directed conversion to text under the Graphite conversion rules. `io.print` and `io.println` therefore accept supported printable values without needing a separate overload for every primitive type. The exact manifest type representation should ideally be structured rather than relying forever on a display string; this is a schema detail to settle before the manifest is treated as stable.

### 4.2 Required metadata

At minimum, the manifest must represent:

- `formatVersion`: version of the manifest schema.
- `domain.name`: the domain identifier.
- `domain.description`: human-readable purpose of the domain.
- `libraries`: a map of library identifiers to library definitions.
- `libraries.<name>.description`: purpose of each library.
- `functions`: a map of exported function names to declarations.
- Function `description`.
- Ordered `params`, each with a name and Graphite type.
- Graphite `returns` type.
- `rustPath`: the fully qualified Rust function path.

The manifest MAY later add deprecation state, examples, feature requirements, version constraints, platform conditions, generic signatures, async metadata, and richer documentation. Do not add unsupported fields to the initial implementation without updating the schema and tests.

### 4.3 Manifest validation

The manifest loader MUST reject:

- invalid JSON;
- missing required fields;
- unsupported `formatVersion`;
- invalid domain, library, or function identifiers;
- duplicate/ambiguous symbols;
- malformed parameter lists;
- unknown Graphite types;
- invalid or empty Rust paths;
- unknown fields if strict schema validation is selected (the policy must be consistent and documented).

Diagnostics should identify the manifest path and relevant JSON field. The loader must not silently repair malformed metadata.

## 5. Call resolution and transpilation

Given:

```grf
import grf.io;

void main() {
    io.println("Hello");
    io.println(123);
}
```

The compiler must:

1. Parse the import into an AST import declaration whose resolved source path is domain `grf`, library `io`.
2. Load the domain's `grf.json` manifest.
3. Resolve the imported library to the `io` library definition.
4. Resolve each `io.println` call to its function declaration.
5. Validate the argument against the declared target conversion (`auto str`).
6. Insert the conversion at the call site as an explicit typed-AST node, or an equivalent explicit lowering representation. The Rust function receives the target type (`String` for Graphite `str`), not a dynamically typed value.
7. Emit a Rust call using `rustPath`.
8. Include the Rust crate/dependency required by the mapped implementation.
9. Let Cargo/rustc compile and link the program.

The compiler MUST NOT implement `io.print` or `io.println` as hardcoded language constructs. Their declarations come from the manifest. Core runtime support may provide their Rust implementations, but the same resolution path must be used as for third-party libraries.

Graphite owns `auto T` conversion. The generated call passes an already converted value. For `io.println(123)`, the compiler formats `123` as `str` using the `auto str` rules and calls `graphite_core::io::println` with that `String`. The Rust function must not accept arbitrary Graphite types in order to perform those conversions itself.

The Rust implementation's actual signature must agree with the Graphite-facing contract. A JSON declaration alone does not prove that the Rust symbol exists. The build or a dedicated validation test must catch missing symbols and incompatible Rust signatures.

## 6. Rust implementation mapping

Illustrative project layout:

```text
graphite-core/
├── Cargo.toml
├── grf.json
└── src/
    ├── lib.rs
    └── io.rs
```

The public Rust functions are exposed from the crate at the declared paths. The manifest controls which API is exposed to Graphite; it does not create Rust functions or make private items public.

Graphite-to-Rust boundary rules must be explicit. For example, if Graphite `str` maps to Rust `String`, generated code must respect ownership and avoid accidental repeated moves. The implementation must specify conversion/borrowing strategy rather than emitting `.to_string()` blindly in every context.

## 7. Library authoring workflow

1. Choose or register the domain identifier.
2. Create the domain's `grf.json`.
3. Document the domain purpose and each library purpose.
4. Implement the exported functions in one or more Rust crates.
5. Expose the Rust functions through public crate paths.
6. Add function declarations to `grf.json`, including Graphite signatures and Rust paths.
7. Validate JSON schema and identifiers.
8. Test that each Rust mapping resolves and has compatible behavior/signatures.
9. Make the domain manifest and implementation crates available to the compiler.
10. In Graphite source, write `import <domain>.<library>;`.
11. Call functions using `<library>.<function>(...)`.
12. Compile/link through Cargo/rustc.

Built-in and third-party libraries use the same authoring model. Built-in libraries may be bundled with the compiler distribution; that packaging difference must not create different source syntax or manifest rules.

## 8. Initial `grf.io` API

The reserved `grf` domain contains an initial `io` library with exactly these public functions:


| Imported call       | Graphite-facing contract       | Behavior                                                                             |
| ------------------- | ------------------------------ | ------------------------------------------------------------------------------------ |
| `io.print(value)`   | `void print(auto str value)`   | Formats a supported value as text and writes without a newline                       |
| `io.println(value)` | `void println(auto str value)` | Formats a supported value as text and writes with a newline                          |
| `io.input(prompt)`  | `str input(auto str prompt)`   | Formats/writes the prompt, reads one line, removes the line ending, and returns text |


The exact spelling of the signature in the manifest may be represented as `"auto str"` or a structured type-plus-conversion flag. This representation must be consistent between the JSON schema, semantic analyzer, and tests.

No `grf.os`, `cwd()`, filesystem, process, networking, or additional console APIs are part of the initial core scope.

I/O failure and EOF behavior must be explicitly specified. Rust `Result` values must not be silently discarded without a documented policy.

## 9. Source-relative working directory

When the CLI runs `grf run path/to/main.grf`, the executable's process working directory defaults to the directory containing the entry source file.

The CLI should resolve the entry path, compile into a managed cache, then launch the executable with its working directory set to the entry source's parent directory. It must not change the parent shell's working directory. Imported library locations and generated Rust/cache directories must not affect runtime CWD. Concurrent builds/runs must not accidentally launch stale artifacts.

This is a CLI contract, not an OS API. Relative file operations in future libraries follow the process working directory unless their API documents otherwise.

## 10. Error and security boundaries

Compilation/build must fail clearly for:

- unknown domain or library;
- missing or malformed `grf.json`;
- unknown function or incompatible Graphite signature;
- unsupported conversion;
- missing Rust dependency or invalid `rustPath`;
- Rust compile/link failure.

Resolution must be deterministic. A Rust-backed library is native code and runs with host-process permissions. A manifest limits the Graphite-visible API but does not sandbox native code.

## 11. Open decisions

These items must be settled before the library system is considered stable:

Settled for v0.1, and no longer open:

- Import paths have exactly two segments. Import aliases are not supported.
- Imports that bind the same local name are compile errors.
- `auto T` conversion is inserted at the call site. The Rust function receives the target type.
- Interpolation to text uses the same formatting rules as `auto str`.
- Graphite-authored libraries are deferred. v0.1 library implementations are Rust code described by `grf.json`.

These items remain open:

1. Exact JSON schema and whether unknown fields are rejected.
2. Whether the manifest is named `grf.json` for every domain root or whether non-`grf` domains use another canonical name. Current recommendation: `grf.json` for each domain.
3. How domains are located/registered and how dependencies/version locking work.
4. How manifest-declared Rust dependencies are represented.
5. How compiler/build validation confirms each `rustPath` has a compatible signature.
6. Graphite-to-Rust ownership, borrowing, and the concrete string-formatting helpers. Call-site conversion is settled; the helper names and ownership are not.
7. Exact structured representation of `auto T` in JSON. The display form `"auto str"` is the current illustration.
8. Conversion-failure semantics for explicit casts and `auto T`. Failures must not invent a value. The diagnostic or runtime mechanism is not yet specified.
9. Whether a repeated import of the same domain and library is a redundant-import diagnostic or an idempotent no-op.
10. Library re-exports, generics, async functions, and platform-specific APIs.



## 12. Acceptance criteria

- A domain manifest describes the domain purpose and multiple libraries.
- Each library describes its purpose and exported functions.
- Each function declares parameter names/types, return type, documentation, and Rust mapping.
- `import grf.io;` resolves the `io` library.
- Imported calls use `io.functionName(...)`, not `grf.io.functionName(...)`.
- Three-segment imports and import aliases are rejected.
- Two imports that bind the same local name are rejected.
- An `auto str` library parameter is converted at the call site before the Rust function runs.
- Built-in and third-party libraries use the same manifest model.
- JSON metadata is parsed separately from Graphite source.
- API calls are resolved through metadata, not hardcoded compiler special cases.
- Invalid metadata, unresolved symbols, unsupported conversions, and invalid Rust mappings fail clearly.
- Rust handles native compilation and linking.

