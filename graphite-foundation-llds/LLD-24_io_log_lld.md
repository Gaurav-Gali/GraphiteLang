# LLD: `grf.io.log` --- Extensible Logging Library

**Project:** Graphite Lang\
**Import:** `import grf.io.log;`\
**Imported name:** `log`\
**Status:** Implemented (V1 + structured fields) — see §16–§17

## 1. Purpose

Implement a built-in, extensible logging library for Graphite. The
public `log` API is a **class** with static methods, not merely a
namespace of unrelated functions. Users must be able to import it
directly and write:

``` grf
import grf.io.log;

void main() {
    log.info("hello");
}
```

The library provides standard event types, custom event types,
consistent timestamped formatting, terminal colors, optional per-call
file output, and support for user-defined subclasses.

## 2. Import behavior

-   `import grf.io.log;` imports the built-in class and binds the name
    `log` in the importing module (`log.info(...)`).
-   `import grf.io;` continues to import the `io` library according to
    Graphite's existing import convention, and also exposes nested
    libraries under that binding (`io.log.warn(...)`).
-   Both styles call the same `io.log` backend. Prefer the direct
    import when subclassing (`extends log`); the nested form is for
    qualified calls through the parent library.
-   The direct import must not require calls such as
    `grf.io.log.info(...)`.
-   Use the existing library registry/import resolver (including
    `grf.json` or equivalent if that is the repository's canonical
    mechanism); do not create a parallel registry.
-   Define duplicate-import and name-collision behavior consistently
    with existing imports.

## 3. Public class API

Conceptual API (adapt the declaration mechanism to the repository's
built-in-class implementation):

``` grf
class log {
    @static void info(str message, str save_to = "");
    @static void debug(str message, str save_to = "");
    @static void warn(str message, str save_to = "");
    @static void error(str message, str save_to = "");
    @static void fatal(str message, str save_to = "");
    @static void checkpoint(str message, str save_to = "");

    @static
    void custom(
        str message,
        str level,
        str color = "#FFFFFF",
        str save_to = ""
    );
}
```

  ---------------------------------------------------------------------------
  Method                      Meaning                 Default terminal color
  --------------------------- ----------------------- -----------------------
  `log.info(message)`         Normal application      Blue or terminal
                              event                   default

  `log.debug(message)`        Diagnostic detail       Dim gray

  `log.warn(message)`         Potential problem       Yellow

  `log.error(message)`        Error needing attention Red

  `log.fatal(message)`        Critical failure event  Bold red

  `log.checkpoint(message)`   Execution milestone     Cyan

  `log.custom(...)`           User-defined event type Supplied color
  ---------------------------------------------------------------------------

`fatal()` only records a log event; it does not terminate the process.
`checkpoint()` is an event category, not an implicit save-history or
control-flow operation.

### Structured fields (generic overload)

Each method also accepts a type argument naming a Graphite `type`
(record) that describes additional fields:

``` grf
type training_log {
    epoch: int,
    loss: flt,
    accuracy: flt
}

log.info<training_log>(
    "Training progress",
    { epoch: 50, loss: 0.024, accuracy: 0.97 },
    save_to="logs/training.jsonl"
);
```

- Message-only calls remain valid (`log.info("hello")`).
- The record literal is type-checked against the named record (missing /
  unknown / wrong-type fields are compile errors).
- Field names `timestamp`, `level`, and `message` are reserved and
  rejected at compile time (and again at runtime).
- Supported field types: `str`, `int`/`flt` widths, `bln`, `chr`,
  nested records, arrays of those, and optionals of those. Classes and
  other unsupported types are rejected.
- `save_to` extension selects the file format:
  `.log` / `.txt` (plain text), `.jsonl` (one JSON object per line),
  `.json` (JSON array with correct append), `.csv` (header + rows;
  schema must stay consistent). Unsupported extensions fail at runtime.
- Console and file share one `LogEvent`; file output never contains ANSI.
- Nested values are rejected for CSV cells (clear runtime error).

## 4. Output format

Default format:

``` text
2026-10-27 08:30:01.102 [INFO] Application started
2026-10-27 08:30:01.108 [DEBUG] Loading model
2026-10-27 08:30:01.115 [WARN] Cache miss
2026-10-27 08:30:01.121 [ERROR] Inference failed
2026-10-27 08:30:01.127 [FATAL] Critical failure
2026-10-27 08:30:01.135 [CHECKPOINT] Model initialized
```

Timestamps/messages above are illustrative. Format:
`YYYY-MM-DD HH:mm:ss.SSS [LEVEL] Message` (single space after the
level badge; no fixed-width padding).

Requirements:

-   Use local system time with exactly three fractional-second digits
    (milliseconds).
-   Display the level in uppercase inside square brackets.
-   Keep spacing tight: one space between timestamp, `[LEVEL]`, and
    message. Do not pad short levels to match longer ones.
-   On a color terminal, render a bright timestamp, a bold vivid level
    badge, and a bright message. Never put ANSI codes in saved files.
-   Use one timestamp per event, shared by console and file output.
-   Emit one logical event per record. Define safe behavior for embedded
    newlines so message text cannot forge records with fake
    timestamps/levels.
-   Custom types display in uppercase by default. Normalize consistently
    and preserve a stable type identity for any future event matching.

## 5. Terminal colors

-   `INFO`: blue or terminal default.
-   `DEBUG`: dim gray.
-   `WARN`: yellow.
-   `ERROR`: red.
-   `FATAL`: bold red.
-   `CHECKPOINT`: cyan.
-   `CUSTOM`: caller-provided hexadecimal RGB color.

Render the timestamp dimly, color the level, and leave the message in
the terminal's default foreground unless a later explicit API says
otherwise.

### 5.1 Color detection

-   Automatically detect interactive terminal output.
-   Disable ANSI colors when output is redirected or piped.
-   Never put ANSI escape sequences in saved log files.
-   Respect terminal color capability where practical; degrade
    gracefully if RGB color is unsupported.

### 5.2 Custom color validation

`color` must use `#RRGGBB`, for example `#454545`, `#00A86B`, or
`#4287F5`. Reject malformed values such as `454545`, `#FFF`, `#GGGGGG`,
and `#12345678` with a clear Graphite runtime error. Never emit
malformed ANSI codes.

## 6. Custom events

Example:

``` grf
import grf.io.log;

void main() {
    log.custom(
        "Model initialized",
        level="checkpoint2",
        color="#454545"
    );

    log.custom(
        "Model initialized",
        level="checkpoint2",
        color="#454545",
        save_to="logs/model.log"
    );
}
```

Conceptual output:

``` text
2026-10-27 08:30:01.135 [CHECKPOINT2] Model initialized
```

  Parameter   Type      Required Default       Meaning
  ----------- ------- ---------- ------------- ---------------------------
  `message`   `str`          Yes ---           Event message
  `type`      `str`          Yes ---           Custom event type
  `color`     `str`           No `"#FFFFFF"`   Hex RGB terminal color
  `save_to`   `str`           No `""`          Optional file destination

Validate custom level and color before emission. Reject line breaks in
the type and safely handle control characters. Do not silently accept
invalid metadata.

## 7. Per-call file output

Every public logging method accepts `save_to`, defaulting to the empty
string.

``` grf
import grf.io.log;

void main() {
    log.info("Application started", save_to="logs/app.log");
    log.error("Inference failed", save_to="logs/errors.log");
    log.checkpoint("Model initialized", save_to="logs/checkpoints.log");
}
```

Behavior:

-   Empty `save_to`: console output only.
-   Non-empty `save_to`: emit once to the console and once to the
    specified file.
-   The setting is per call and never changes later calls.
-   Append rather than truncate existing files; create a missing file.
-   Create parent directories if consistent with runtime filesystem
    conventions; otherwise report the failure clearly.
-   File output uses the same timestamp, level/type, and message format,
    but no ANSI codes.
-   File errors must be surfaced using Graphite's existing runtime error
    mechanism; do not silently drop them.
-   Avoid duplicate console emission when saving to a file.
-   Define deterministic error precedence if console and file writes
    both fail.

Within a process, concurrent writes must not interleave individual
records. Reuse existing synchronization/logging infrastructure where
possible. Cross-process file coordination is not required unless already
supported by the runtime.

## 8. User-defined logging classes

Users can extend the built-in class and add reusable event methods:

``` grf
import grf.io.log;

class CustomLog extends log {
    @static
    void checkpoint2(str msg, str save_to = "") {
        log.custom(
            msg,
            level="checkpoint2",
            color="#454545",
            save_to=save_to
        );
    }
}

void main() {
    CustomLog.warn("warnnn");
    CustomLog.fatal("fatal");
    CustomLog.checkpoint("check-1");
    CustomLog.checkpoint2("check-2");
}
```

Requirements:

-   `log` remains a class and is not replaced by a namespace to simplify
    implementation.
-   Built-in methods are static.
-   Verify whether Graphite supports extending library-provided classes
    and inheriting static methods. Do not assume it does.
-   If static inheritance is unsupported, implement it as a scoped
    prerequisite or report the exact incompatibility and smallest
    compatible solution.
-   Custom methods delegate to `log.custom()` and must not reimplement
    timestamp formatting, terminal coloring, or file output.
-   Existing access modifiers, `@final`, and override rules must be
    respected.

### 8.1 Integration with `...args`

Graphite's planned named-argument capture and spread feature should
allow wrappers to avoid redeclaring every forwarded parameter. Once that
feature exists, a wrapper could use:

``` grf
import grf.io.log;

class CustomLog extends log {
    @static
    void checkpoint2(...args) {
        log.custom(
            level="checkpoint2",
            color="#454545",
            ...args
        );
    }
}

void main() {
    CustomLog.checkpoint2(
        message="check-2",
        save_to="logs/checkpoints.log"
    );
}
```

This is conditional on the separate `...args` language feature
supporting this signature and named-call forwarding. If it is not
implemented, use the explicit typed wrapper above and do not claim the
forwarding example already compiles. Define duplicate-key behavior in
the argument-forwarding specification; duplicate explicit/spread keys
must never silently overwrite one another.

## 9. Runtime architecture

All public methods must use a shared backend:

1.  Bind/validate arguments.
2.  Obtain one timestamp for the event.
3.  Build an internal event record.
4.  Format and colorize for terminal output if color is supported.
5.  If `save_to` is non-empty, format the same event without ANSI codes
    and append it to the file.
6.  Propagate validation and I/O errors through the existing runtime
    error mechanism.

The internal event may contain timestamp, type/level, message, optional
custom color, and optional source-location metadata if the
compiler/runtime already makes that practical. Do not introduce a new
public event class unless the current runtime architecture requires it.

Centralize formatting and output logic; standard methods should differ
only in event type and default color. Reuse the existing runtime crate
and dependencies. Inspect the actual Cargo workspace before adding
dependencies or assuming crate names.

## 10. Checkpoint behavior and future diagnostic capture

In this version, `log.checkpoint(message)` emits a normal `CHECKPOINT`
event and optionally writes that event to `save_to`. It does not save
preceding log history, configure future destinations, or terminate
execution.

A future feature may capture prior retained events when a named
checkpoint occurs. That behavior is outside this LLD. Keep the internal
event representation extensible, but do not implement capture rules or
buffering as part of this task.

## 11. Compiler/library integration

Inspect the repository and integrate through its established mechanisms.
Likely areas to inspect include:

-   Built-in class registration and static method lookup.
-   Reserved-domain library mapping (`grf.io.log`) and the existing
    `grf.json`/equivalent mapping.
-   Import resolution and binding the name `log`.
-   Type checking of built-in methods, default parameters, and named
    arguments.
-   Inheritance and static method resolution.
-   Rust code generation and runtime calls.
-   Runtime error propagation and file APIs.

Do not create a parallel registry or hard-code a separate import path if
the compiler already has a canonical mechanism. The following must
compile:

``` grf
import grf.io.log;

void main() {
    log.info("hello");
}
```

## 12. Error handling

Handle at least:

-   Invalid custom color.
-   Invalid custom level.
-   File/directory creation failure.
-   File append failure.
-   Invalid arguments to built-in methods.
-   Import resolution failure.
-   Inheritance/static method resolution errors.

Use Graphite's existing compile-time diagnostics and runtime error
representation. Do not create a separate logging-only error model.

## 13. Testing plan

### Import and API

-   `import grf.io.log;` resolves to the `log` class.
-   `log.info("hello");` compiles and executes.
-   Every standard method is available as a static method.
-   Inherited static methods resolve through a subclass if supported.

### Formatting and colors

-   Timestamp follows `YYYY-MM-DD HH:mm:ss.SSS`.
-   Level is bracketed with tight spacing (no pad).
-   Custom types are displayed consistently.
-   Color mapping is correct.
-   Invalid colors are rejected.
-   Redirected output and file output contain no ANSI codes.
-   Use deterministic formatter tests instead of asserting exact
    wall-clock timestamps.

### File behavior

-   Empty `save_to` does not write a file.
-   Non-empty `save_to` emits to console and file exactly once.
-   Existing files are appended to.
-   Missing files are created.
-   Different calls can use different paths.
-   One call's `save_to` does not affect the next call.
-   File errors are surfaced.
-   Concurrent records do not interleave within one runtime process.

### Custom class

Compile and execute the `CustomLog` example above. Verify inherited
methods and `checkpoint2()` share the same backend and formatting rules.

### Regression

Run relevant tests for imports, built-ins, static methods, inheritance,
default parameters, named arguments, argument capture/spread if
available, Rust generation, and runtime execution. Report only tests
that were actually run.

## 14. Non-goals

Do not implement in this version:

-   Global log-level or tag-level configuration.
-   Global output destinations or global template configuration.
-   Log rotation or retention policies.
-   Remote sinks or OpenTelemetry export.
-   Async logging queues.
-   Automatic process termination for `fatal`.
-   Automatic history capture for checkpoints.
-   A public structured-event type unless required by the current
    architecture.

## 15. Acceptance criteria

1.  `import grf.io.log;` imports the built-in class as `log`.
2.  `log.info("hello");` compiles and executes.
3.  Standard and custom methods use one shared formatter/backend.
4.  Output follows the timestamp/level/message format.
5.  Terminal colors follow the defined policy and never leak into files.
6.  `save_to` writes the same event to a file in append mode and is
    strictly per-call.
7.  `log.custom()` supports a custom level and validated `#RRGGBB` color.
8.  Users can extend `log` and define reusable static logging methods,
    subject to verified class semantics.
9.  Integration uses existing import, type-checking, runtime-error, and
    Rust-generation paths.
10. Relevant tests pass, or unverified tests are explicitly identified.
11. No unrelated language behavior changes.

## 16. Implementation report

### Architecture discovered

- Libraries are declared in `crates/graphite-core/grf.json` and loaded by
  `graphite-compiler`'s manifest module. Calls lower to the function's
  `rustPath`.
- Imports were previously exactly `domain.library` (two segments). Nested
  paths such as `grf.io.log` are now allowed; the manifest key is the
  dotted path (`io.log`) and the bound name is the last segment (`log`).
- There was no prior built-in **class** import path. Libraries were
  function bags only. Extending a library type required a new
  `exportsClass: true` flag that also registers a synthetic class whose
  static methods share the same Rust paths.

### Approach shipped

1. **Manifest** — library `io.log` with `exportsClass: true`, methods
   `info`/`debug`/`warn`/`error`/`fatal`/`checkpoint`/`custom`, optional
   string `default` on parameters (`save_to=""`, `color="#FFFFFF"`).
2. **Import** — `import grf.io.log;` binds `log` and registers the
   built-in class.
3. **Runtime** — `graphite_core::io::log` shared `emit` backend:
   local `YYYY-MM-DD HH:mm:ss.SSS [LEVEL] message` (no level padding),
   bright cyan timestamp + bold level badge + bright message on TTY
   (never ANSI in files), `#RRGGBB` validation, newline sanitization,
   per-call `save_to` append with `create_dir_all`, process mutex
   around writes.
4. **Inheritance** — static methods are inherited via the existing
   parent walk. `class CustomLog extends log` compiles; inherited calls
   codegen through each method's `builtin_path`.
5. **Example** — `examples/log/main.grf` covers built-ins, `custom`, and
   `CustomLog`.

### Limitations / prerequisites

- **`...args` forwarding** is not implemented; wrappers redeclare
  parameters explicitly (as in the example).
- Nested calls via the parent import work (`io.log.warn(...)` /
  `io.log.info<T>(...)`). Subclassing still needs
  `import grf.io.log;` so the bind name `log` is in scope for
  `extends log` (Graphite `extends` takes a simple identifier today).
- Constructing `new log()` is allowed (empty instance); the API is
  static-only in practice.
- Duplicate console emission when also writing a file is intentionally
  avoided: one console write and one file write per call.
- Structured overloads are registered by the compiler for `io.log`
  (not as separate `grf.json` entries). Manifest entries describe the
  message-only signatures; type-parameter overloads are synthetic.
- Nested CSV fields and class-typed fields are unsupported (errors, not
  silent omission). Maps and custom Graphite types beyond records /
  arrays / optionals / primitives are not serialized yet.

## 17. Structured logging report

### Architecture

1. **Synthetic generic overloads** on the built-in `log` class:
   `info<Fields>(message, fields, save_to="")` →
   `graphite_core::io::log::info_structured`, and likewise for every
   level including `custom_structured`.
2. **Semantic** — existing record-literal checking against the
   substituted `Fields` type; plus `validate_structured_log_fields` for
   reserved names and supported types (`GRF-SEM-038`).
3. **Codegen** — record args to `*_structured` lower to
   `LogObject::from_pairs([...])` / nested `LogValue` builders.
4. **Runtime** — shared `LogEvent { timestamp, level, message, fields }`;
   format writers in `io/log/format.rs` selected by extension.

### Tests run

- `cargo test -p graphite-core --lib io::log`
- `cargo test -p graphite-compiler --lib structured_log_type_checks_fields`
- `cargo test -p graphite-compiler --lib driver::tests::io_log_library_runs`
- `cargo test -p graphite-compiler --lib driver::tests::io_log_structured_runs`
- `cargo test -p graphite-compiler --lib driver::tests::nested_library_call_via_parent_import`
- Manual: `grf run mintest.grf`, `grf run examples/log/structured.grf`,
  compile-fail cases for wrong/missing/unknown/reserved fields
