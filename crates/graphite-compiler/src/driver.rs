//! Compile and execute one Graphite entry file.
//!
//! # Workspace discovery
//!
//! Generated projects live under the workspace `target/` directory. The
//! workspace root is the nearest ancestor directory that contains a
//! `Cargo.toml` with a `[workspace]` table and `crates/graphite-core/Cargo.toml`.
//!
//! The search starts at each of these locations, in order:
//!
//! 1. The current executable.
//! 2. The process working directory.
//! 3. The directory where this compiler crate was compiled
//!    (`CARGO_MANIFEST_DIR`).
//!
//! The shell's current directory is not assumed to be the workspace root.
//! Step 3 is the development fallback. An installed compiler can later replace
//! it with its install prefix; the generated manifest does not contain a
//! home-directory path.
//!
//! The build directory is `target/grf/builds/<key>/`. `<key>` is the 16-digit
//! hex FNV-1a 64 hash of the canonical entry-file path, so two source files do
//! not share generated Rust. The generated `Cargo.toml` depends on
//! `graphite-core` through the relative path
//! `../../../../crates/graphite-core`.
//!
//! The executed program's working directory is the directory that contains the
//! entry `.grf` file. It is not the generated-project directory and it is not
//! the shell's current directory unless that is also the source directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::codegen::generate_with_source;
use crate::lexer::lex;
use crate::manifest::builtin_manifest;
use crate::parser::{format_ast, parse};
use crate::semantic::analyze;

#[derive(Debug)]
pub struct RunOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub build_dir: Option<PathBuf>,
    pub working_directory: Option<PathBuf>,
}

pub fn run_path(path: &Path, print_ast: bool, emit_rust: bool) -> i32 {
    match prepare(path, print_ast, emit_rust) {
        Err(output) => {
            eprint!("{}", output.stderr);
            if print_ast && !output.stdout.is_empty() {
                println!("{}", output.stdout);
            }
            output.status
        }
        Ok(prepared) => {
            eprint!("{}", prepared.compiler_stderr);
            if let Some(ast) = &prepared.ast {
                println!("{ast}");
            }
            match Command::new(&prepared.binary)
                .current_dir(&prepared.working_directory)
                .status()
            {
                Ok(status) => status.code().unwrap_or(1),
                Err(error) => {
                    eprintln!(
                        "grf: failed to execute {}: {error}",
                        prepared.binary.display()
                    );
                    1
                }
            }
        }
    }
}

pub fn run_captured(path: &Path) -> RunOutput {
    match prepare(path, false, false) {
        Err(output) => output,
        Ok(prepared) => {
            let output = Command::new(&prepared.binary)
                .current_dir(&prepared.working_directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();
            match output {
                Ok(output) => RunOutput {
                    status: output.status.code().unwrap_or(1),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: format!(
                        "{}{}",
                        prepared.compiler_stderr,
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    build_dir: Some(prepared.build_dir),
                    working_directory: Some(prepared.working_directory),
                },
                Err(error) => RunOutput {
                    status: 1,
                    stdout: String::new(),
                    stderr: format!("grf: failed to execute: {error}"),
                    build_dir: Some(prepared.build_dir),
                    working_directory: Some(prepared.working_directory),
                },
            }
        }
    }
}

struct Prepared {
    binary: PathBuf,
    build_dir: PathBuf,
    working_directory: PathBuf,
    ast: Option<String>,
    compiler_stderr: String,
}

fn render_diagnostic(
    source: &str,
    file: &str,
    span: crate::span::Span,
    code: &'static str,
    message: &str,
    help: Option<&str>,
) -> String {
    let excerpt = if span.start.line == 0 {
        String::new()
    } else {
        graphite_core::diagnostic::source_line(source, span.start.line)
    };
    graphite_core::diagnostic::render(&graphite_core::diagnostic::DiagnosticView {
        code,
        message: message.to_string(),
        file: file.to_string(),
        line: span.start.line,
        column: span.start.column,
        end_column: span.end.column,
        excerpt,
        label: String::new(),
        help: help.map(str::to_string),
    })
}

fn failed(stderr: impl Into<String>) -> RunOutput {
    RunOutput {
        status: 1,
        stdout: String::new(),
        stderr: stderr.into(),
        build_dir: None,
        working_directory: None,
    }
}

fn prepare(path: &Path, print_ast: bool, emit_rust: bool) -> Result<Prepared, RunOutput> {
    let bytes = fs::read(path)
        .map_err(|error| failed(format!("grf: cannot read {}: {error}\n", path.display())))?;
    let source = String::from_utf8(bytes)
        .map_err(|_| failed(format!("grf: {} is not valid UTF-8\n", path.display())))?;
    let source_name = path.display().to_string();
    let lexed = lex(&source_name, &source);
    if !lexed.diagnostics.is_empty() {
        let mut stderr = String::new();
        for diagnostic in &lexed.diagnostics {
            stderr.push_str(&render_diagnostic(
                &source,
                &diagnostic.source_name,
                diagnostic.span,
                diagnostic.code(),
                &diagnostic.message,
                diagnostic.hint.as_deref(),
            ));
        }
        return Err(failed(stderr));
    }
    let parsed = parse(&source_name, &lexed.tokens);
    let ast = parsed.program.as_ref().map(format_ast);
    if !parsed.diagnostics.is_empty() {
        let mut stderr = String::new();
        for diagnostic in &parsed.diagnostics {
            stderr.push_str(&render_diagnostic(
                &source,
                &diagnostic.source_name,
                diagnostic.span,
                diagnostic.code(),
                &diagnostic.message,
                None,
            ));
        }
        return Err(RunOutput {
            status: 1,
            stdout: if print_ast {
                ast.unwrap_or_default()
            } else {
                String::new()
            },
            stderr,
            build_dir: None,
            working_directory: None,
        });
    }
    let program = parsed.program.expect("parse produced a program");
    let manifest = builtin_manifest();
    let analysis = analyze(&source_name, &program, &manifest);
    if !analysis.diagnostics.is_empty() {
        let mut stderr = String::new();
        for diagnostic in &analysis.diagnostics {
            stderr.push_str(&render_diagnostic(
                &source,
                &diagnostic.source_name,
                diagnostic.span,
                diagnostic.code,
                &diagnostic.message,
                diagnostic.help.as_deref(),
            ));
        }
        return Err(RunOutput {
            status: 1,
            stdout: if print_ast {
                ast.unwrap_or_default()
            } else {
                String::new()
            },
            stderr,
            build_dir: None,
            working_directory: None,
        });
    }
    let mut compiler_stderr = String::new();
    for warning in &analysis.warnings {
        compiler_stderr.push_str(&format!("warning[{}]: {}\n", warning.code, warning.message));
    }
    let checked = analysis.program.expect("checked program");
    let rust = generate_with_source(&checked, &source_name, &source);
    let workspace = discover_workspace().map_err(|error| failed(format!("grf: {error}\n")))?;
    let working_directory = entry_directory(path);
    if !workspace.join("crates/graphite-core/Cargo.toml").is_file() {
        return Err(failed(
            "grf: could not resolve crates/graphite-core from the workspace\n",
        ));
    }
    let entry_key = build_key(path);
    let build_dir = workspace.join("target/grf/builds").join(&entry_key);
    fs::create_dir_all(build_dir.join("src"))
        .map_err(|error| failed(format!("grf: cannot create build directory: {error}\n")))?;
    let resolved_core = build_dir.join("../../../../crates/graphite-core/Cargo.toml");
    if !resolved_core.is_file() {
        return Err(failed(format!(
            "grf: relative graphite-core path does not resolve from {}\n",
            build_dir.display()
        )));
    }
    let package = format!("grf-{entry_key}");
    let needs_tokio = checked.functions.iter().any(|f| f.is_async || f.is_async_iter)
        || checked
            .specializations
            .iter()
            .any(|f| f.is_async || f.is_async_iter);
    let tokio_dep = if needs_tokio {
        "tokio = { version = \"1\", features = [\"rt\", \"macros\", \"time\", \"sync\"] }\n"
    } else {
        ""
    };
    let manifest_text = format!(
        r#"[package]
name = "{package}"
version = "0.0.0"
edition = "2024"
publish = false

[[bin]]
name = "{package}"
path = "src/main.rs"

[dependencies]
graphite-core = {{ path = "../../../../crates/graphite-core" }}
{tokio_dep}
[workspace]
resolver = "3"
"#
    );
    fs::write(build_dir.join("Cargo.toml"), manifest_text)
        .and_then(|_| fs::write(build_dir.join("src/main.rs"), &rust))
        .map_err(|error| failed(format!("grf: cannot write generated files: {error}\n")))?;
    if emit_rust {
        let sidecar = path.with_extension("rs");
        fs::write(&sidecar, &rust).map_err(|error| {
            failed(format!(
                "grf: cannot write {}: {error}\n",
                sidecar.display()
            ))
        })?;
    }

    let target_dir = workspace.join("target/grf/cargo-target");
    let cargo = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(build_dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .map_err(|error| failed(format!("grf: failed to start cargo: {error}\n")))?;
    if !cargo.status.success() {
        let mut stderr = String::from("error[GRF-RST-001]: generated Rust failed to compile\n");
        stderr.push_str(&String::from_utf8_lossy(&cargo.stderr));
        stderr.push_str(&String::from_utf8_lossy(&cargo.stdout));
        return Err(RunOutput {
            status: cargo.status.code().unwrap_or(1),
            stdout: String::new(),
            stderr,
            build_dir: Some(build_dir),
            working_directory: Some(working_directory),
        });
    }
    let binary = target_dir.join("debug").join(&package);
    Ok(Prepared {
        binary,
        build_dir,
        working_directory,
        ast: if print_ast { ast } else { None },
        compiler_stderr,
    })
}

pub fn entry_directory(path: &Path) -> PathBuf {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical
        .parent()
        .map(Path::to_path_buf)
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn build_key(path: &Path) -> String {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("{:016x}", fnv1a64(canonical.to_string_lossy().as_bytes()))
}

pub fn discover_workspace() -> Result<PathBuf, String> {
    let mut starts = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        starts.push(exe);
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    for start in starts {
        if let Some(root) = find_workspace(&start) {
            return Ok(root);
        }
    }
    Err(
        "could not find the Graphite workspace (a Cargo.toml workspace containing crates/graphite-core)"
            .to_string(),
    )
}

fn find_workspace(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let cargo = current.join("Cargo.toml");
        let core = current.join("crates/graphite-core/Cargo.toml");
        if cargo.is_file() && core.is_file() {
            let text = fs::read_to_string(&cargo).ok()?;
            if text.contains("[workspace]") {
                return Some(current);
            }
        }
        if !current.pop() {
            return None;
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    use super::*;

    static BUILD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn workspace_discovery_finds_graphite_core() {
        let root = discover_workspace().expect("workspace");
        assert!(root.join("crates/graphite-core/Cargo.toml").is_file());
        let text = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(text.contains("[workspace]"));
    }

    #[test]
    fn working_directory_is_the_entry_file_directory() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test.grf");
        let directory = entry_directory(&path);
        let canonical = fs::canonicalize(&path).unwrap();
        assert_eq!(directory, canonical.parent().unwrap());
    }

    #[test]
    fn structured_type_mismatch_keeps_conversion_help_honest() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-diag-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(&path, "void main() {\n    int age = \"ten\";\n}\n").unwrap();
        let rejected = run_captured(&path);
        assert_ne!(rejected.status, 0);
        assert!(
            rejected.stderr.contains("error[GRF-TYP-001]:"),
            "{}",
            rejected.stderr
        );
        assert!(rejected.stderr.contains("-->"), "{}", rejected.stderr);
        assert!(!rejected.stderr.contains("help:"), "{}", rejected.stderr);

        fs::write(&path, "void main() {\n    int age = \"21\";\n}\n").unwrap();
        let hinted = run_captured(&path);
        assert_ne!(hinted.status, 0);
        assert!(hinted.stderr.contains("int64(\"21\")"), "{}", hinted.stderr);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hello_world_compiles_and_prints() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-hello-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(\"Hello, Graphite!\");\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "Hello, Graphite!\n");
        let build_dir = output.build_dir.expect("build dir");
        let workspace = discover_workspace().unwrap();
        assert!(build_dir.starts_with(workspace.join("target/grf/builds")));
        assert!(!dir.join("Cargo.toml").exists());
        assert!(!dir.join("src").join("main.rs").exists());
        let manifest = fs::read_to_string(build_dir.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("path = \"../../../../crates/graphite-core\""));
        if let Ok(home) = std::env::var("HOME") {
            assert!(!manifest.contains(&home), "{manifest}");
        }
        assert_eq!(
            output.working_directory.unwrap(),
            fs::canonicalize(&dir).unwrap()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn logical_and_bitwise_operators_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-bits-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

bln boom() {
    int x = 1 / 0;
    return true;
}

void main() {
    io.println(false && boom());
    io.println(true || boom());
    io.println(true and false);
    io.println(false or true);
    io.println(6 & 3);
    io.println(6 | 1);
    io.println(6 ^ 3);
    io.println(1 << 3);
    io.println(8 >> 1);
    io.println(~0);
    io.println(1 << 2 + 3);
    io.println(1 & 2 == 2);
    int n = 6;
    n &= 3;
    n |= 1;
    n ^= 1;
    n <<= 1;
    n >>= 1;
    io.println(n);
    uint u = 1;
    u <<= 2;
    io.println(u);
    int16 s = -8;
    s >>= 1;
    io.println(s);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "false\ntrue\nfalse\ntrue\n2\n7\n5\n8\n4\n-1\n32\nfalse\n2\n4\n-4\n"
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(1 << 64);\n}\n",
        )
        .unwrap();
        let overflow = run_captured(&path);
        assert_ne!(overflow.status, 0);
        assert!(
            overflow.stderr.contains("GRF-RUN-004"),
            "{}",
            overflow.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_statement_bodies_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-stmt-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

int add(int a, int b) return a + b;

void main() {
    if (1 == 2) io.println("hi");
    if (1 == 1) io.println("yes");
    else io.println("no");
    io.println(add(2, 3));
    int n = 3;
    loop (n-- > 0) io.println(n);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "yes\n5\n2\n1\n0\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ternary_operators_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-ternary-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

class Counter {
    int n;
    Counter() this.n = 0;
    int bump() {
        this.n = this.n + 1;
        return this.n;
    }
}

int pick(bln ready) return ready ? 1 : 2;

bln boom() {
    int x = 1 / 0;
    return true;
}

void main() {
    Counter c = new Counter();
    io.println(c.bump() == 1 ? 10 : 20);
    io.println(c.n);
    io.println(false ? boom() : false);
    io.println(false ? 1 : true ? 2 : 3);
    true ? io.println("yes");
    false ? io.println("no");
    auto int digit = true ? 10.5 : 2;
    io.println(digit);
    int? value = false ? null : 7;
    io.println(value);
    io.println(pick(true));
    io.println(pick(false));
    int count = 2;
    loop (count > 0) {
        count = count - 1;
        count == 0 ? break;
        io.println(count);
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "10\n1\nfalse\n2\nyes\n10\n7\n1\n2\n1\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn oop_example_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/oop/main.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "My name is Buddy\nBuddy says woof!\nMy name is Luna\nLuna says meow!\nAnnual Report\nInteger\nHello\n2\n2\n2\n"
        );
    }

    #[test]
    fn generics_example_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/generics/main.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "10\n10\n20\n30\n10\nGraphite\n10\nGraphite\n40\nok\n"
        );
    }

    #[test]
    fn lambdas_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/lambdas/main.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "25\n25\n36\n12\n7\n50\n80\n26\n21\n16\n26\nCallback executed!\n42\n0\n1\n2\n-12\n15\n81\n11\n50\n0\n"
        );
    }

    #[test]
    fn sample_program_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "uint64\n");
    }

    #[test]
    fn async_await_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/async/main.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "42\n0\n1\n2\n3\n4\n10\n20\n30\n",
            "{}",
            output.stdout
        );
        let rust = fs::read_to_string(
            discover_workspace()
                .unwrap()
                .join("target/grf/builds")
                .join(build_key(&path))
                .join("Cargo.toml"),
        )
        .unwrap();
        assert!(rust.contains("tokio"), "{rust}");
    }

    #[test]
    fn time_library_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/time/main.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        let lines: Vec<_> = output.stdout.lines().collect();
        assert!(lines.len() >= 13, "{}", output.stdout);
        assert_eq!(lines[5], "25");
        assert_eq!(lines[6], "2026-02-28");
        assert!(lines[8].starts_with("2000"), "{}", lines[8]);
        assert!(lines[9].starts_with("86400000"), "{}", lines[9]);
        assert_eq!(lines[10], "0.5");
        assert_eq!(lines[11], "2026-10-26T14:30:00Z");
        assert_eq!(lines[12], "14");
    }

    #[test]
    fn arrays_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-array-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

int[] copy_of(int[] items) {
    return items;
}

void mutate(int[] items) {
    items[0] = 99;
}

void main() {
    int[] original = [10, 20, 30, 40, 50];
    io.println(original[0]);
    io.println(original[-1]);
    int[] copy = original;
    copy[0] = 7;
    io.println(original[0]);
    io.println(copy[0]);
    mutate(original);
    io.println(original[0]);
    int[] returned = copy_of(original);
    returned[1] = 8;
    io.println(original[1]);
    int[] prefix = original[:3];
    io.println(prefix[-1]);
    int[] reversed = original[::-1];
    io.println(reversed[0]);
    original[1:3] = [];
    io.println(lengthOf(original));
    io.println(original.length());
    io.println(original[1]);
    int[][] matrix = [[1, 2], [3, 4]];
    int[] row = matrix[0];
    row[0] = 9;
    io.println(matrix[0][0]);
    io.println(row[0]);
    matrix[1][0] = 30;
    io.println(matrix[1][0]);
    int a, const int b = [4, 5];
    io.println(a);
    io.println(b);
    loop (uint64 idx, int value : [8].enumerate()) {
        io.println(idx);
        io.println(value);
    }
    original.push(60);
    io.println(original[-1]);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "10\n50\n10\n7\n10\n20\n30\n50\n3\n3\n40\n1\n9\n30\n4\n5\n0\n8\n60\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn array_iteration_rejects_modification() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-iter-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

void main() {
    int[] arr = [1, 2, 3];
    int i = 0;
    loop (int a : arr) {
        arr[i] = i * i;
        i++;
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "expected compile failure");
        assert!(
            output.stderr.contains("while it is being iterated"),
            "{}",
            output.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn iterators_and_generators_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-iters-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

void main() {
    int[] numbers = [10, 20];
    iter<int> it = numbers.iter();
    loop (it.hasNext()) {
        io.println(it.next());
    }
    loop (int value : range(1, 4)) {
        io.println(value);
    }
    loop (int value : numbers) {
        io.println(value);
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "10\n20\n1\n2\n3\n10\n20\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generic_generators_strings_and_range_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-gen-iter-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

iter<T> repeat<T>(T value, int count) {
    loop (int i = 0; i < count; i++) {
        emit value;
    }
}

iter<int> countdown(int start) {
    loop (int i = start; i >= 0; i--) {
        if (i == 1) {
            return;
        }
        emit i;
    }
}

iter<int> sideEffect(int n) {
    io.println("body");
    emit n;
}

void main() {
    loop (str w : repeat<str>("hi", 2)) {
        io.println(w);
    }
    loop (int n : repeat<int>(7, 2)) {
        io.println(n);
    }

    io.println("lazy");
    iter<int> pending = sideEffect(9);
    io.println("created");
    io.println(pending.next());

    io.println("early");
    loop (int v : countdown(3)) {
        io.println(v);
    }

    io.println("chars");
    loop (chr c : "a😀b") {
        io.println(c);
    }

    io.println("range");
    loop (int i : range(10)) {
        io.println(i);
    }
    loop (int i : range(1, 10, 2)) {
        io.println(i);
    }
    loop (int i : range(5, 0, -1)) {
        io.println(i);
    }

    io.println("alias");
    int[] nums = [1, 2, 3];
    iter<int> a = nums.iter();
    iter<int> b = a.iter();
    io.println(a.next());
    io.println(b.next());
    io.println(nums[1:3]);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
hi
hi
7
7
lazy
created
body
9
early
3
2
chars
a
😀
b
range
0
1
2
3
4
5
6
7
8
9
1
3
5
7
9
5
4
3
2
1
alias
1
2
[2, 3]
"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generic_generator_requires_type_arguments() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-gen-args-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
iter<T> repeat<T>(T value, int count) {
    loop (int i = 0; i < count; i++) {
        emit value;
    }
}

void main() {
    iter<str> words = repeat("hello", 3);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "expected compile failure");
        assert!(
            output.stderr.contains("requires type arguments")
                || output.stderr.contains("GRF-SEM-034"),
            "{}",
            output.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generator_rejects_valued_return_and_bad_emit() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-gen-diag-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
iter<int> bad() {
    emit "nope";
    return 1;
}

void main() {}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "expected compile failure");
        assert!(
            output.stderr.contains("expected 'int64'")
                || output.stderr.contains("cannot return a value"),
            "{}",
            output.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn arrays_print() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-array-print-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

void main() {
    int[] values = [1, 2, 3];
    int[][] matrix = [[1], []];
    int[] empty = [];
    str[] names = ["Ada", "A\"B"];
    bln[] flags = [true, false];
    chr[] letters = [chr("A")];
    io.println(values);
    io.println(str(values));
    io.println(f"n={values}");
    io.println(matrix);
    io.println(empty);
    io.println(names);
    io.println(flags);
    io.println(letters);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "[1, 2, 3]\n[1, 2, 3]\nn=[1, 2, 3]\n[[1], []]\n[]\n[\"Ada\", \"A\\\"B\"]\n[true, false]\n[\"A\"]\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bounded_arrays_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-array-bound-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

int[] echo(int[] items) {
    return items;
}

void main() {
    int[3] values = [];
    values.push(1);
    values.push(2);
    values.push(3);
    io.println(values);
    values[1:3] = [8];
    values.push(4);
    io.println(values);
    int[2][] rows = [[1], []];
    io.println(rows);
    int[] dynamic = [];
    dynamic.push(1);
    dynamic.push(2);
    dynamic.push(3);
    dynamic.push(4);
    io.println(dynamic);
    int[3] shared = [1, 2];
    shared = echo(shared);
    io.println(shared);
    shared.push(3);
    io.println(shared.length());
    int[] grown = [];
    grown[0] = 7;
    grown[1] = 8;
    io.println(grown);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "[1, 2, 3]\n[1, 8, 4]\n[[1], []]\n[1, 2, 3, 4]\n[1, 2]\n3\n[7, 8]\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn array_failures_terminate() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-array-fail-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[] values = [1];\n    io.println(values[1]);\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("error[GRF-RUN-001]: array index out of bounds"),
            "{}",
            output.stderr
        );
        assert!(
            output
                .stderr
                .contains("index 1 is out of bounds for an array of length 1"),
            "{}",
            output.stderr
        );
        assert!(output.stderr.contains("values[1]"), "{}", output.stderr);

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[] values = [1, 2, 3];\n    values[::2] = [9];\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("slice assignment length mismatch"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[] values = [1, 2, 3];\n    int[] copy = values[::0];\n    io.println(copy.length());\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("slice step equals zero"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[] values = [1];\n    int a, int b = values;\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("destructuring length mismatch"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[3] values = [1, 2, 3];\n    values.push(4);\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("array exceeds its maximum length"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[3] values = [1, 2, 3];\n    values[0:0] = [8, 9];\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("array exceeds its maximum length"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[] values = [];\n    values[1] = 1;\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("index 1 is out of bounds for an array of length 0"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    int[] dynamic = [1, 2, 3, 4];\n    int[3] bounded = dynamic;\n    io.println(bounded);\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("array exceeds its maximum length"),
            "{}",
            output.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn numeric_promotion_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-numeric-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

void main() {
    int a = 10;
    uint b = 5;
    flt c = 2.5;
    io.println(a + b);
    io.println(a - b);
    io.println(a * b);
    io.println(a / b);
    io.println(a + c);
    io.println(b * c);
    io.println(c / a);
    int neg = -10;
    uint pos = 5;
    io.println(neg < pos);
    io.println(neg == pos);
    io.println(neg != pos);
    flt x = 10.5;
    int y = 10;
    io.println(x > y);
    io.println(x == y);
    int[] arr = [1, 2, 3, 4];
    int16 i16 = 0;
    int32 i32 = 1;
    int64 i64v = 2;
    uint16 u16 = 3;
    uint32 u32 = 0;
    uint64 u64v = 1;
    int16 back = -1;
    io.println(arr[i16]);
    io.println(arr[i32]);
    io.println(arr[i64v]);
    io.println(arr[u16]);
    io.println(arr[u32]);
    io.println(arr[u64v]);
    io.println(arr[back]);
    loop (uint64 i = 0; i < lengthOf(arr); i++) {
        if (arr[i] % 2 == 0) {
            io.println(arr[i]);
        }
    }
    int i = 0;
    uint64 n = lengthOf(arr);
    loop (i = 0; i < n; i++) {
        io.println(i);
    }
    int huge = 9007199254740993;
    flt approx = 9007199254740992.0;
    io.println(huge == approx);
    flt nan = 0.0 / 0.0;
    io.println(nan == nan);
    io.println(nan != 1.0);
    io.println(nan < 1.0);
    flt inf = 1.0 / 0.0;
    io.println(inf > 10);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "15\n5\n50\n2\n12.5\n12.5\n0.25\ntrue\nfalse\ntrue\ntrue\nfalse\n1\n2\n3\n4\n1\n2\n4\n2\n4\n0\n1\n2\n3\nfalse\nfalse\ntrue\nfalse\ntrue\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn numeric_failures_terminate() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-numeric-fail-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        let cases = [
            (
                "import grf.io;\n\nvoid main() {\n    int16 a = 32767;\n    int16 b = 1;\n    io.println(a + b);\n}\n",
                "error[GRF-RUN-004]: integer overflow",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    int a = 1;\n    int b = 0;\n    io.println(a / b);\n}\n",
                "error[GRF-RUN-005]: division by zero",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    int worst = -9223372036854775808;\n    io.println(worst / -1);\n}\n",
                "error[GRF-RUN-004]: integer overflow",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    io.println(1 % 0);\n}\n",
                "error[GRF-RUN-005]: remainder by zero",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    uint big = 9223372036854775808;\n    int small = 1;\n    io.println(small + big);\n}\n",
                "does not fit in 'int64'",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    int[] arr = [1];\n    uint64 index = 5;\n    io.println(arr[index]);\n}\n",
                "array index out of bounds",
            ),
        ];
        for (source, diagnostic) in cases {
            fs::write(&path, source).unwrap();
            let output = run_captured(&path);
            assert_ne!(output.status, 0, "{source}\n{}", output.stdout);
            assert!(
                output.stderr.contains(diagnostic),
                "{diagnostic} missing from {}\n{source}",
                output.stderr
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn exceptions_match_runtime_kinds_and_preserve_finally() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-try-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

type DivisionByZeroError {
    value: int,
}

int boom() {
    return 1 / 0;
}

void take(auto int value) {
    io.println(value);
}

void main() {
    DivisionByZeroError item = { value: 4 };
    try {
        int a = 1;
        int b = 0;
        io.println(a / b);
    } catch (DivisionByZeroError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        io.println(1 % 0);
    } catch (DivisionByZeroError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        int16 a = 32767;
        int16 b = 1;
        io.println(a + b);
    } catch (IntegerOverflowError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        int[] items = [1, 2, 3];
        io.println(items[5]);
    } catch (ArrayIndexOutOfBoundsError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        map<str, int> scores = {};
        io.println(scores["absent-key"]);
    } catch (MissingMapKeyError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        take("abc");
    } catch (NumericConversionError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        uint big = 9223372036854775808;
        int small = 1;
        io.println(small + big);
    } catch (NumericConversionError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        int[] values = [1, 2, 3];
        int[] copy = values[::0];
        io.println(copy.length());
    } catch (InvalidSliceError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        int[] values = [1, 2, 3];
        values[::2] = [9];
    } catch (InvalidSliceError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        int[] values = [1];
        int a, int b = values;
        io.println(a);
    } catch (InvalidSliceError error) {
        io.println("slice");
    } catch (RuntimeError error) {
        io.println(error.kind());
        io.println(error.code());
    }
    try {
        int[3] values = [1, 2, 3];
        values.push(4);
    } catch (ArrayIndexOutOfBoundsError error) {
        io.println("bounds");
    } catch (RuntimeError error) {
        io.println(error.kind());
        io.println(error.code());
    }
    try {
        int[] items = [1];
        items.insert(5, 9);
    } catch (ArrayIndexOutOfBoundsError error) {
        io.println("bounds");
    } catch (RuntimeError error) {
        io.println(error.kind());
        io.println(error.code());
    }
    try {
        str line = io.input("");
        io.println(line);
    } catch (IoError error) {
        io.println(error.kind());
        io.println(error.message());
        io.println(error.code());
    }
    try {
        io.println(1 / 0);
    } catch (DivisionByZeroError error) {
        io.println("specific");
    } catch (RuntimeError error) {
        io.println("any");
    }
    try {
        int x = 1 / 0;
        io.println(x);
    } catch (DivisionByZeroError error) {
        io.println(lengthOf(error.stackTrace()));
    }
    try {
        io.println(boom());
    } catch (DivisionByZeroError error) {
        io.println(error.stackTrace());
    }
    try {
        io.println("body");
    } finally {
        io.println("final");
    }
    try {
        loop (int i = 0; i < 2; i++) {
            if (i == 1) {
                break;
            }
            io.println(i);
        }
    } finally {
        io.println("once");
    }
    loop () {
        try {
            io.println("brk");
            break;
        } finally {
            io.println("after-break");
        }
    }
    int n = 0;
    loop () {
        try {
            n = n + 1;
            if (n == 1) {
                continue;
            }
            break;
        } finally {
            io.println("after-continue");
        }
    }
    try {
        try {
            io.println(1 / 0);
        } finally {
            io.println("inner");
        }
    } catch (DivisionByZeroError error) {
        io.println(lengthOf(error.causeMessage()));
        io.println("outer-catch");
    } finally {
        io.println("outer");
    }
    try {
        try {
            io.println(1 / 0);
        } finally {
            io.println(1 % 0);
        }
    } catch (DivisionByZeroError error) {
        io.println(error.message());
        io.println(error.causeMessage());
    }
    try {
        io.println(1 / 0);
    } catch (Error error) {
        io.println(error.kind());
        io.println(error.message());
    }
    try {
        int[] values = [1];
        int left, int right = values;
        io.println(left);
    } catch (Error error) {
        io.println(error.kind());
        io.println(error.code());
    }
    io.println(item.value);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert!(!output.stdout.contains("absent-key"), "{}", output.stdout);
        assert!(
            output.stdout.contains("boom at ") && output.stdout.contains(".grf:"),
            "{}",
            output.stdout
        );
        let without_trace = output
            .stdout
            .lines()
            .filter(|line| !line.starts_with("boom at "))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            without_trace,
            "\
DivisionByZeroError
division by zero
GRF-RUN-005
DivisionByZeroError
remainder by zero
GRF-RUN-005
IntegerOverflowError
integer overflow
GRF-RUN-004
ArrayIndexOutOfBoundsError
array index out of bounds
GRF-RUN-001
MissingMapKeyError
missing map key
GRF-RUN-002
NumericConversionError
numeric conversion failed
GRF-RUN-006
NumericConversionError
numeric conversion failed
GRF-RUN-006
InvalidSliceError
slice step equals zero
GRF-RUN-003
InvalidSliceError
slice assignment length mismatch
GRF-RUN-008
RuntimeError
GRF-RUN-009
RuntimeError
GRF-RUN-010
RuntimeError
GRF-RUN-011
IoError
input/output error
GRF-RUN-007
specific
0
body
final
0
once
brk
after-break
after-continue
after-continue
inner
0
outer-catch
outer
remainder by zero
division by zero
DivisionByZeroError
division by zero
RuntimeError
GRF-RUN-009
4"
        );

        fs::write(
            &path,
            r#"import grf.io;

void main() {
    try {
        io.println(1 / 0);
    } catch (DivisionByZeroError error) {
        io.println(1 % 0);
    } catch (RuntimeError error) {
        io.println("sibling");
    } finally {
        io.println("fin");
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "{}", output.stdout);
        assert_eq!(output.stdout, "fin\n");
        assert!(!output.stdout.contains("sibling"));
        assert!(
            output
                .stderr
                .contains("error[GRF-RUN-005]: remainder by zero")
        );
        assert_eq!(output.stderr.matches("error[GRF-RUN-005]").count(), 1);

        fs::write(
            &path,
            r#"import grf.io;

int give() {
    try {
        return 7;
    } finally {
        int z = 1 / 0;
        io.println(z);
    }
}

void main() {
    try {
        io.println(give());
    } catch (DivisionByZeroError error) {
        io.println(error.message());
        io.println(lengthOf(error.causeMessage()));
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "division by zero\n0\n");

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n\tio.println(1 / 0);\n}\n",
        )
        .unwrap();
        let uncaught = run_captured(&path);
        let header = uncaught
            .stderr
            .lines()
            .find(|line| line.contains("--> "))
            .unwrap_or("");
        let column = header.rsplit(':').next().unwrap_or("");
        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n\ttry {\n\tio.println(1 / 0);\n\t} catch (DivisionByZeroError error) {\n\tio.println(error.column());\n\t}\n}\n",
        )
        .unwrap();
        let caught = run_captured(&path);
        assert_eq!(caught.status, 0, "{}", caught.stderr);
        assert_eq!(caught.stdout, format!("{column}\n"), "{}", uncaught.stderr);

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    try {\n        io.println(1 / 0);\n    } finally {\n        io.println(\"still\");\n    }\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert_eq!(output.stdout, "still\n");
        assert_eq!(output.stderr.matches("error[GRF-RUN-005]").count(), 1);
        assert!(output.stderr.contains("--> "));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_reassignment_converts_to_the_declared_type() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-auto-assign-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

void take(auto int value) {
    value = "9";
    io.println(value);
}

void main() {
    auto int age = "21";
    age = "12";
    age = 25;
    io.println(age);
    auto uint count = "12";
    count = "13";
    io.println(count);
    auto flt price = "12.5";
    price = "13.5";
    io.println(price);
    auto int32 width = "40000";
    width = "7";
    io.println(width);
    take("123");
    io.println(age + 1);
    auto int rounded = 10.55;
    io.println(rounded);
    auto int lowered = -10.2;
    io.println(lowered);
    auto flt fromInt = 10;
    io.println(fromInt);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "25\n13\n13.5\n7\n9\n26\n10\n-11\n10\n");

        let failures = [
            (
                "import grf.io;\n\nvoid main() {\n    auto int age = \"21\";\n    age = \"hello\";\n}\n",
                "invalid decimal integer",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    auto int16 age = \"1\";\n    age = \"40000\";\n}\n",
                "out of range",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    auto int age = 1e20;\n}\n",
                "out of range",
            ),
        ];
        for (source, diagnostic) in failures {
            fs::write(&path, source).unwrap();
            let output = run_captured(&path);
            assert_ne!(output.status, 0, "{source}");
            assert!(
                output.stderr.contains(diagnostic),
                "{diagnostic} missing from {}\n{source}",
                output.stderr
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_copy_and_keep_their_field_count() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-records-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

type Address {
    city: str,
    country: str,
}

type Profile {
    name: str,
    age: int,
    scores: int[],
    address: Address,
}

int next(int label) {
    io.println(label);
    return label;
}

type Pair {
    left: int,
    right: int,
}

Profile make() {
    return {
        name: "Ada",
        age: 36,
        scores: [10, 20],
        address: { city: "London", country: "UK" },
    };
}

void main() {
    Profile first = make();
    io.println(first.name);
    io.println(first.age);
    io.println(lengthOf(first));
    const Profile fixed = first;
    io.println(lengthOf(fixed));
    Profile copy = fixed;
    copy.age = 37;
    copy.scores[0] = 99;
    copy.address.city = "Paris";
    io.println(first.age);
    io.println(first.scores[0]);
    io.println(first.address.city);
    io.println(copy.age);
    io.println(copy.scores[0]);
    io.println(copy.address.city);
    Profile[] people = [first];
    people[0].name = "Grace";
    io.println(people[0].name);
    io.println(first.name);
    Pair value = { right: next(1), left: next(2) };
    io.println(value.left);
    io.println(value.right);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "Ada\n36\n4\n4\n36\n10\nLondon\n37\n99\nParis\nGrace\nAda\n1\n2\n2\n1\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn records_print() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-record-print-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

type Address {
    city: str,
    country: str,
}

type User {
    name: str,
    age: int,
    scores: int[],
    address: Address,
}

void main() {
    User user = {
        name: "Krishnaa",
        age: 20,
        scores: [1, 2],
        address: { city: "Pune", country: "IN" },
    };
    io.println(user);
    io.println(str(user));
    io.println(f"{user}");
    User[] users = [user];
    io.println(users);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        let line = "{ name: \"Krishnaa\", age: 20, scores: [1, 2], address: { city: \"Pune\", country: \"IN\" } }\n";
        assert_eq!(
            output.stdout,
            format!("{line}{line}{line}[{}]\n", line.trim_end())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn conversions_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-convert-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

void take(auto int value) {
    io.println(value);
}

void main() {
    io.println(int("123"));
    io.println(int(" -42 "));
    io.println(int(" 123"));
    io.println(int32("123"));
    io.println(int32("40000"));
    io.println(int32("-2147483648"));
    io.println(int32("2147483647"));
    io.println(int16("-32768"));
    io.println(uint16("65535"));
    io.println(uint32("4294967295"));
    io.println(flt("12.5"));
    io.println(flt("1e-2"));
    io.println(flt("+1.5"));
    io.println(flt32("1.5"));
    io.println(bln("TRUE"));
    io.println(str(chr("é")));
    io.println(int(chr("1")));
    auto int digit = chr("2");
    io.println(digit);
    io.println(flt(chr("3")));
    io.println(uint(chr("4")));
    io.println(chr(1));
    io.println(str(flt32(1)));
    io.println(int64("9"));
    io.println(uint("8"));
    io.println(uint64("8"));
    io.println(flt64("2.5"));
    io.println(flt64(flt32(1.5)));
    io.println(flt32(flt(1.5)));
    take("456");
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "123\n-42\n123\n123\n40000\n-2147483648\n2147483647\n-32768\n65535\n4294967295\n12.5\n0.01\n1.5\n1.5\ntrue\né\n1\n2\n3\n4\n1\n1\n9\n8\n8\n2.5\n1.5\n1.5\n456\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn values_convert_to_chr_through_their_text() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-chr-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

void main() {
    io.println(chr(7));
    int? number = 6;
    io.println(chr(number));
    int[] values = [1];
    io.println(chr(values));
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "{}", output.stdout);
        assert!(
            output
                .stderr
                .contains("cannot convert int64[] to chr: invalid character string"),
            "{}",
            output.stderr
        );
        assert!(output.stdout.contains("7\n6\n"), "{}", output.stdout);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn conversion_failures_terminate() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-convert-fail-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            "import grf.io;\n\nvoid take(auto int value) {\n    io.println(value);\n}\n\nvoid main() {\n    take(\"abc\");\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "{}", output.stdout);
        assert!(
            output
                .stderr
                .contains("cannot convert str to int64: invalid decimal integer"),
            "{}",
            output.stderr
        );
        assert!(output.stdout.is_empty(), "{}", output.stdout);

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(int32(\"2147483648\"));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("cannot convert str to int32: out of range"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(int16(\"40000\"));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("error[GRF-RUN-006]: numeric conversion failed")
                && output
                    .stderr
                    .contains("cannot convert str to int16: out of range"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(flt(\"1e309\"));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("cannot convert str to flt64: out of range"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(flt32(\"1e-50\"));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("cannot convert str to flt32: underflow"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    flt wide = 16777217.0;\n    io.println(flt32(wide));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("cannot convert flt64 to flt32: not exactly representable"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(flt32(16777217));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("cannot convert int64 to flt32: not exactly representable"),
            "{}",
            output.stderr
        );

        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(flt(\".5\"));\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output
                .stderr
                .contains("cannot convert str to flt64: invalid decimal number"),
            "{}",
            output.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn const_copy_and_string_length_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-length-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

void change(int value) {
    value = 4;
}

void rename(str value) {
    value = "other";
}

void main() {
    const int limit = 3;
    const str name = "Graphite";
    int copy = limit;
    str text = name;
    change(limit);
    rename(name);
    io.println(limit);
    io.println(copy);
    io.println(name);
    io.println(text);
    io.println(lengthOf(name));
    io.println(lengthOf("é"));
    io.println(lengthOf("𐍈"));
    io.println(lengthOf(""));
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "3\n3\nGraphite\nGraphite\n8\n1\n1\n0\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage4_values_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-stage4-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

int add(int left, int right) {
    return left + right;
}

void main() {
    int add = 1;
    io.println(add);
    io.println(add(2, 3));
    auto str value = 12.433;
    io.println(value);
    flt reading = 12.433;
    io.println(f"Reading: {reading}");
    flt32 ratio = 1.5;
    io.println(ratio);
    flt infinite = 1.0 / 0.0;
    flt missing = 0.0 / 0.0;
    io.println(infinite);
    io.println(missing);
    io.println(missing < missing);
    io.println(missing == missing);
    io.println(missing != missing);
    const int MAX_RETRIES = 3;
    io.println(MAX_RETRIES);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "1\n5\n12.433\nReading: 12.433\n1.5\ninf\nNaN\nfalse\nfalse\ntrue\n3\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn input_failure_does_not_invent_a_string() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-input-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    str line = io.input(\"\");\n    io.println(line);\n}\n",
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0, "stdout {:?}", output.stdout);
        assert!(
            output
                .stderr
                .contains("error[GRF-RUN-007]: input/output error")
                && output.stderr.contains("unexpected end of input")
                && !output.stderr.contains("-->"),
            "{}",
            output.stderr
        );
        assert!(output.stdout.is_empty(), "{:?}", output.stdout);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn maps_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-maps-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

type User {
    name: str,
    age: int,
}

int next(int label) {
    io.println(label);
    return label;
}

int compute() {
    io.println("default");
    return 99;
}

void main() {
    map<int, int> built = {
        next(1): next(2),
        next(3): next(4),
    };
    io.println(built.get(1));
    io.println(built.get(3));

    map<str, int> scores = {
        "Ada": 10,
        "Linus": 20,
    };
    io.println(scores);
    io.println(str(scores));
    io.println(f"Scores: {scores}");

    scores["Grace"] = 30;
    scores["Ada"] = 11;
    io.println(scores.get("Ada"));
    io.println(scores["Grace"]);
    io.println(scores.containsKey("Ada"));
    scores.set("Bob", 7);
    scores.set("Bob", 8);
    io.println(scores.get("Bob"));
    io.println(scores.remove("Linus"));
    io.println(scores.remove("missing"));
    io.println(scores.containsKey("Linus"));
    io.println(scores.length());
    io.println(lengthOf(scores));

    int kept = scores.getOrDefault("Ada", compute());
    int filled = scores.getOrDefault("Nobody", compute());
    io.println(kept);
    io.println(filled);
    io.println(scores.containsKey("Nobody"));
    io.println(lengthOf(scores));

    map<str, int> zeros = { "Ada": 0 };
    io.println(zeros.getOrDefault("Ada", 5));
    io.println(zeros.getOrDefault("Bob", 5));
    io.println(zeros.containsKey("Bob"));

    str[] names = scores.keys();
    scores["Zoe"] = 1;
    io.println(lengthOf(names));
    int seen = 0;
    loop (str name : names) {
        if (name == "Ada") { seen = seen + 1; }
        if (name == "Grace") { seen = seen + 1; }
        if (name == "Bob") { seen = seen + 1; }
    }
    io.println(seen);
    io.println(scores.containsKey("Zoe"));

    int rounds = 0;
    loop (str key, int value : scores.entries()) {
        scores.set("Later", 9);
        rounds = rounds + 1;
        io.println(lengthOf(key) > 0);
        io.println(value > 0);
    }
    io.println(rounds);
    io.println(scores.containsKey("Later"));

    map<str, int> extra = { "Lin": 5 };
    map<str, int> copy = { "Ada": 1, ...extra, "Ada": 9, ...extra };
    io.println(copy.get("Ada"));
    io.println(copy.get("Lin"));
    io.println(extra.containsKey("Ada"));
    io.println(lengthOf(extra));

    map<str, int> dups = { "Ada": next(5), "Ada": next(6) };
    io.println(dups.get("Ada"));

    map<str, int> left = { "Ada": 1 };
    map<str, int> right = left;
    right["Ada"] = 2;
    io.println(left.get("Ada"));
    io.println(right.get("Ada"));

    map<int, str> labels = {};
    labels[-1] = "tail";
    io.println(labels[-1]);
    io.println(labels.containsKey(-1));

    const map<str, int> fixed = { "Ada": 4 };
    io.println(fixed.get("Ada"));
    io.println(fixed.containsKey("Ada"));
    io.println(fixed.length());
    io.println(lengthOf(fixed));
    io.println(fixed.getOrDefault("No", 0));

    map<str, User> users = { "Ada": { name: "Ada", age: 20 } };
    users["Ada"].age = 22;
    io.println(users["Ada"].age);
    io.println(users["Ada"].name);

    map<str, int[3]> histories = { "Ada": [1, 2, 3] };
    histories["Ada"][0] = 9;
    io.println(histories["Ada"][0]);

    map<str, map<str, int>> nested = { "group": { "count": 1 } };
    nested["group"]["count"] = 3;
    io.println(nested["group"]["count"]);
    io.println(nested);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        let lines: Vec<&str> = output.stdout.lines().collect();
        let prints: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|line| line.contains("Ada") && line.starts_with('{'))
            .collect();
        assert!(
            prints.len() >= 2,
            "expected map display lines, got {:?}",
            output.stdout
        );
        for line in &prints {
            assert!(line.contains("\"Ada\""), "{line}");
            assert!(line.contains(": "), "{line}");
        }
        assert!(
            lines
                .iter()
                .any(|line| line.contains("\"Ada\": 10") || line.contains("\"Linus\": 20")),
            "{:?}",
            output.stdout
        );
        assert!(
            lines.iter().any(|line| line.starts_with("Scores: {")),
            "{:?}",
            output.stdout
        );
        assert!(lines.contains(&"default"), "{:?}", output.stdout);
        let default_at = lines.iter().position(|line| *line == "default").unwrap();
        assert_eq!(lines.get(default_at + 2), Some(&"11"));
        assert!(lines.contains(&"99"));
        assert!(lines.contains(&"tail"));
        assert!(lines.contains(&"22"));
        assert_eq!(lines.first(), Some(&"1"));
        assert_eq!(lines.get(1), Some(&"2"));
        assert_eq!(lines.get(2), Some(&"3"));
        assert_eq!(lines.get(3), Some(&"4"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn maps_missing_keys_and_default_evaluation_fail() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-map-miss-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let index = dir.join("index.grf");
        fs::write(
            &index,
            "import grf.io;\n\nvoid main() {\n    map<str, int> scores = { \"Ada\": 10 };\n    io.println(scores[\"Grace\"]);\n}\n",
        )
        .unwrap();
        let indexed = run_captured(&index);
        assert_eq!(indexed.status, 1, "{}", indexed.stderr);
        assert!(
            indexed
                .stderr
                .contains("error[GRF-RUN-002]: missing map key"),
            "{}",
            indexed.stderr
        );
        assert!(
            indexed
                .stderr
                .contains("missing key: lookup on 'map<str, int64>'"),
            "{}",
            indexed.stderr
        );
        assert!(
            !indexed.stderr.contains("missing key: \"Grace\""),
            "{}",
            indexed.stderr
        );
        assert!(indexed.stdout.is_empty(), "{:?}", indexed.stdout);

        let get = dir.join("get.grf");
        fs::write(
            &get,
            "import grf.io;\n\nvoid main() {\n    map<str, int> scores = { \"Ada\": 10 };\n    io.println(scores.get(\"Grace\"));\n}\n",
        )
        .unwrap();
        let got = run_captured(&get);
        assert_eq!(got.status, 1, "{}", got.stderr);
        assert!(
            got.stderr
                .contains("missing key: lookup on 'map<str, int64>'"),
            "{}",
            got.stderr
        );

        let nested = dir.join("nested.grf");
        fs::write(
            &nested,
            "import grf.io;\n\nvoid main() {\n    map<str, map<str, int>> nested = {};\n    nested[\"group\"][\"count\"] = 3;\n    io.println(\"wrote\");\n}\n",
        )
        .unwrap();
        let missed = run_captured(&nested);
        assert_eq!(missed.status, 1, "{}", missed.stderr);
        assert!(
            missed
                .stderr
                .contains("missing key: lookup on 'map<str, map<str, int64>>'"),
            "{}",
            missed.stderr
        );
        assert!(!missed.stdout.contains("wrote"), "{:?}", missed.stdout);

        let boom = dir.join("boom.grf");
        fs::write(
            &boom,
            "import grf.io;\n\nint boom() {\n    int16 bad = int16(\"40000\");\n    return int(bad);\n}\n\nvoid main() {\n    map<str, int> scores = { \"Ada\": 10 };\n    int result = scores.getOrDefault(\"Ada\", boom());\n    io.println(result);\n}\n",
        )
        .unwrap();
        let failed = run_captured(&boom);
        assert_ne!(failed.status, 0, "{}", failed.stdout);
        assert!(
            failed
                .stderr
                .contains("error[GRF-RUN-006]: numeric conversion failed")
                && failed.stderr.contains("cannot convert str to int16"),
            "{}",
            failed.stderr
        );
        assert!(failed.stdout.is_empty(), "{:?}", failed.stdout);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn type_descriptors_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-type-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

type User {
    name: str,
    age: int,
}

type Account {
    name: str,
    age: int,
}

type Metadata {
    descriptor: Type,
}

int next(str label) {
    io.println(label);
    return 1;
}

Type describe(int value) {
    return typeOf(value);
}

void take(auto str value) {
    io.println(value);
}

void main() {
    int Type = 1;
    io.println(Type);
    io.println(typeOf(Type).name());

    int age = 21;
    str name = "hello";
    int[] ages = [age];
    int[3] fixed = [1, 2, 3];
    map<str, int> scores = { "Ada": 10 };
    map<str, int64> sameScores = { "Ada": 10 };
    User user = { name: "Ada", age: 36 };
    Account account = { name: "Ada", age: 36 };
    User[] users = [user];
    map<str, User> byName = { "Ada": user };

    Type number = typeOf(age);
    Type text = typeOf(name);
    Type numbers = typeOf(ages);
    Type bounded = typeOf(fixed);
    Type mapOfInt = typeOf(scores);
    Type mapOfInt64 = typeOf(sameScores);
    Type person = typeOf(user);
    Type other = typeOf(account);
    Type people = typeOf(users);
    Type mapOfUser = typeOf(byName);
    Type nested = typeOf(number);

    io.println(number.name());
    io.println(number);
    io.println(text.name());
    io.println(text);
    io.println(numbers.name());
    io.println(numbers);
    io.println(bounded.name());
    io.println(bounded);
    io.println(mapOfInt.name());
    io.println(mapOfInt);
    io.println(person.name());
    io.println(person);
    io.println(people.name());
    io.println(people);
    io.println(mapOfUser.name());
    io.println(mapOfUser);
    io.println(nested.name());
    io.println(nested);

    io.println(mapOfInt == mapOfInt64);
    io.println(number == typeOf(1));
    io.println(number == describe(age));
    io.println(person == other);
    io.println(person == people);
    io.println(number != text);

    io.println(str(number));
    io.println(f"{number}");
    auto str shown = number;
    io.println(shown);
    shown = text;
    io.println(shown);
    take(person);

    Type[] descriptors = [number, text];
    io.println(descriptors);
    io.println(descriptors[0].name());

    map<str, Type> descriptorByName = {
        "age": number,
        "name": text,
    };
    io.println(descriptorByName.get("age"));
    io.println(descriptorByName.get("name").name());

    Metadata metadata = { descriptor: number };
    io.println(metadata);
    Type copied = metadata.descriptor;
    const Type frozen = number;
    io.println(copied == frozen);

    io.println(typeOf(next("seen")).name());
    Type kept = typeOf(next("once"));
    io.println(kept.name());
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        let lines: Vec<&str> = output.stdout.lines().collect();
        assert_eq!(
            lines,
            [
                "1",
                "int64",
                "int64",
                "<class 'int64'>",
                "str",
                "<class 'str'>",
                "int64[]",
                "<class 'int64[]'>",
                "int64[]",
                "<class 'int64[]'>",
                "map<str, int64>",
                "<class 'map<str, int64>'>",
                "User",
                "<type 'User'>",
                "User[]",
                "<class 'User[]'>",
                "map<str, User>",
                "<class 'map<str, User>'>",
                "Type",
                "<class 'Type'>",
                "true",
                "true",
                "true",
                "false",
                "false",
                "true",
                "<class 'int64'>",
                "<class 'int64'>",
                "<class 'int64'>",
                "<class 'str'>",
                "<type 'User'>",
                "[<class 'int64'>, <class 'str'>]",
                "int64",
                "<class 'int64'>",
                "str",
                "{ descriptor: <class 'int64'> }",
                "true",
                "seen",
                "int64",
                "once",
                "int64",
            ]
        );

        let failed = dir.join("failed.grf");
        fs::write(
            &failed,
            "import grf.io;\n\nvoid main() {\n    Type descriptor = typeOf(int16(\"40000\"));\n    io.println(descriptor.name());\n    io.println(\"after\");\n}\n",
        )
        .unwrap();
        let boom = run_captured(&failed);
        assert_ne!(boom.status, 0, "{}", boom.stdout);
        assert!(
            boom.stderr.contains("cannot convert str to int16"),
            "{}",
            boom.stderr
        );
        assert!(boom.stdout.is_empty(), "{:?}", boom.stdout);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_record_fields_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-auto-record-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

void main() {
    rectangle r1 = {
        height: 10,
        width: "200",
    };
    io.println(area(r1));
    r1.width = "3";
    io.println(area(r1));
}

type rectangle {
    height: auto int,
    width: auto int,
}

flt area(auto rectangle rect) {
    return flt(rect.height * rect.width);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "2000\n30\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rs_flag_writes_generated_rust_beside_the_source() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-rs-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.grf");
        fs::write(
            &path,
            "import grf.io;\n\nvoid main() {\n    io.println(\"beside\");\n}\n",
        )
        .unwrap();
        let prepared = prepare(&path, false, true).unwrap_or_else(|output| {
            panic!("{}", output.stderr);
        });
        let sidecar = dir.join("sample.rs");
        let rust = fs::read_to_string(&sidecar).unwrap();
        assert!(rust.contains("fn main()"), "{rust}");
        assert!(rust.contains("beside"), "{rust}");
        assert!(!dir.join("Cargo.toml").exists());
        let output = std::process::Command::new(&prepared.binary)
            .current_dir(&prepared.working_directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "beside\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn classes_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-classes-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

int Counter(int start) {
    io.println("function");
    return start;
}

int boom() {
    io.println("before");
    return 1 / 0;
}

class Counter {
    public int value;
    Counter(int start) {
        this.value = start;
        io.println("built");
    }
    public int getValue() {
        return this.value;
    }
    public void setValue(int next) {
        this.value = next;
    }
    public Counter same(Counter item) {
        return item;
    }
    static int sign(int value) {
        if (value < 0) {
            return 0 - 1;
        }
        return 1;
    }
    void grow() {
        this.value *= 2;
    }
}

class Account {
    public auto int balance = "10";
    public void setBalance(str amount) {
        this.balance = amount;
    }
    public int getBalance() {
        return this.balance;
    }
}

class Node {
    public Node next;
    Node() {
        this.next = this;
    }
    public int ping() {
        return 1;
    }
}

class Boom {
    public int value = boom();
}

type Wrap {
    item: Counter,
}

class Box {
    public Wrap wrap;
    public Counter[] items;
    Box(Counter item) {
        Wrap wrap = { item: item };
        this.wrap = wrap;
        Counter[] made = [item];
        this.items = made;
    }
    public int wrapped() {
        return this.wrap.item.getValue();
    }
    public int first() {
        return this.items[0].getValue();
    }
}

class Holder {
    public const Counter item;
    public int value = 0;
    Holder(Counter start) {
        this.item = start;
    }
    public void setValue(int next) {
        this.value = next;
    }
    public int itemValue() {
        return this.item.getValue();
    }
}

void main() {
    Counter first = new Counter(1);
    Counter alias = first;
    Counter second = new Counter(2);
    io.println(first == alias);
    io.println(first == second);
    io.println(first != second);
    alias.setValue(4);
    io.println(first.getValue());
    io.println(first.same(first) == first);

    Counter live = new Counter(5);
    const Counter frozen = live;
    io.println(live.getValue());
    try {
        live.setValue(9);
        io.println("mutated");
    } catch (ConstViolationError error) {
        io.println(error.kind());
        io.println(error.code());
        io.println(error.message());
    }
    try {
        live.setValue(9);
    } catch (RuntimeError error) {
        io.println(error.kind());
    }
    try {
        live.setValue(9);
    } catch (Error error) {
        io.println(error.kind());
    }
    io.println(frozen.getValue());

    Account account = new Account();
    io.println(account.getBalance());
    account.setBalance("4");
    io.println(account.getBalance());
    try {
        account.setBalance("nope");
        io.println("converted");
    } catch (NumericConversionError error) {
        io.println(error.kind());
        io.println(error.code());
    }

    Type kind = typeOf(new Counter(7));
    io.println(kind.name());
    io.println(str(kind));

    Counter[] items = [new Counter(3)];
    Counter fromArray = items[0];
    io.println(fromArray == items[0]);
    fromArray.setValue(8);
    io.println(items[0].getValue());

    map<str, Counter> byName = { "a": new Counter(6) };
    Counter found = byName.get("a");
    io.println(found == byName.get("a"));
    found.setValue(11);
    io.println(byName.get("a").getValue());

    Node node = new Node();
    io.println(node.next == node);
    io.println(node.next.ping());

    try {
        Boom failed = new Boom();
        io.println(failed.value);
    } catch (DivisionByZeroError error) {
        io.println(error.kind());
        io.println(error.message());
    }

    Counter shared = new Counter(1);
    Box box = new Box(shared);
    io.println(box.wrapped());
    io.println(box.items[0] == shared);
    io.println(box.first());
    Holder holder = new Holder(shared);
    holder.setValue(2);
    io.println(holder.value);
    io.println(holder.itemValue());
    try {
        shared.setValue(3);
        io.println("shared");
    } catch (ConstViolationError error) {
        io.println(error.message());
    }
    io.println(Counter.sign(0 - 4));
    int power = 2 ** 10;
    io.println(power);
    int bucket = 0 - 5;
    bucket //= 2;
    io.println(bucket);
    int grown = 3;
    grown += 4;
    grown *= 2;
    grown -= 1;
    grown /= 3;
    grown %= 5;
    io.println(grown);
    first.grow();
    io.println(first.getValue());
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
built
built
true
false
true
4
true
built
5
ConstViolationError
GRF-RUN-014
mutation of frozen value
ConstViolationError
ConstViolationError
5
10
4
NumericConversionError
GRF-RUN-006
built
Counter
<class 'Counter'>
built
true
8
built
true
11
true
1
before
DivisionByZeroError
division by zero
built
1
true
1
2
1
mutation of frozen value
-1
1024
-3
4
8
"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn defaults_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-defaults-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"import grf.io;

type Point {
    x: flt,
    y: flt,
}

class Origin {
    Point point;
    int count;
    Origin() {
    }
    flt x() {
        return this.point.x;
    }
}

void main() {
    int count;
    io.println(count);
    count = 10;
    io.println(count);
    str name;
    io.println(name);
    flt scale;
    io.println(scale);
    bln flag;
    io.println(flag);
    Origin here = new Origin();
    io.println(here.x());
    io.println(here.count);
    int[] nums;
    io.println(nums.length());
    int[2] pair;
    io.println(pair[0]);
    io.println(pair[1]);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
0
10

0
false
0
0
0
0
0
"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_class_record_fields_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-nested-fields-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

type rect {
    height: auto flt,
    width: auto flt,
    area: auto flt,
}

type pair {
    value: int,
}

class Calc {
    public rect r;
    public pair extra;

    Calc(auto flt h, auto flt w) {
        this.r.height = h;
        this.r.width = w;
        this.extra.value = 1;
    }

    Calc area() {
        this.r.area = this.r.height * this.r.width;
        return this;
    }
}

void main() {
    Calc c = new Calc(2.0, 5.0);
    c.area();
    io.println(c.r.height);
    io.println(c.r.width);
    io.println(c.r.area);
    io.println(c.extra.value);
    c.r.height = 4.0;
    io.println(c.r.height);
    c.extra.value += 3;
    io.println(c.extra.value);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
2
5
10
1
4
4
"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nullable_values_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-nullable-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

class node {
    int value;
    node? next;

    node(int value) {
        this.value = value;
    }
}

void main() {
    int? number;
    io.println(number);
    number = 10;
    io.println(number);

    node item;
    item = new node(1);
    io.println(item.value);

    node? head = new node(2);
    if (head != null) {
        head.next = new node(3);
        io.println(head.value);
        node? link = head.next;
        if (link != null) {
            io.println(link.value);
        }
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
null
10
1
2
3
"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nullable_field_chain_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-null-chain-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

class node {
    int value;
    node? next;

    node(int value) {
        this.value = value;
    }
}

void main() {
    node head = new node(1);
    head.next = new node(2);
    head.next.next = new node(3);
    io.println(head.value);
    io.println(head.next.value);
    io.println(head.next.next.value);

    node? temp = head;
    loop (temp != null) {
        io.println(temp.value);
        temp = temp.next;
    }
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
1
2
3
1
2
3
"
        );

        fs::write(
            &path,
            r#"
class node {
    int value;
    node? next;

    node(int value) {
        this.value = value;
    }
}

void main() {
    node head = new node(1);
    head.next.next = new node(2);
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_ne!(output.status, 0);
        assert!(
            output.stderr.contains("error[GRF-RUN-015]: null value"),
            "{}",
            output.stderr
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nullable_auto_text_and_sequence_methods_run() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("grf-text-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.grf");
        fs::write(
            &path,
            r#"
import grf.io;

void main() {
    auto int? number = 10.5;
    io.println(number);
    auto int? missing = null;
    io.println(missing);
    io.println(int?(4.2));

    flt[] values = [10.5, 20.9];
    auto int first, auto int? second = values;
    io.println(first);
    io.println(second);

    str name = "abcd";
    io.println(name[::-1]);
    io.println(name[1:3]);
    str word = "abcd";
    word[1:3] = "XY";
    io.println(word);
    name[0] = chr("z");
    name.append(chr("e"));
    name.extend("!");
    io.println(name);
    io.println(name.length());
    io.println(name.pop());

    str line = "a,b,c";
    str[] parts = line.split(",");
    io.println(parts[0]);
    io.println(parts[2]);
    io.println(line.upper());
    io.println("  hi  ".strip());
    io.println(",".join(parts));

    int[] scores = [3, 1, 2];
    scores.append(0);
    scores.sort();
    io.println(scores);
    io.println(scores.pop());
    io.println(scores.index(1));
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "\
10
null
4
10
20
dcba
bc
aXYd
zbcde!
6
!
a
c
A,B,C
hi
a,b,c
[0, 1, 2, 3]
3
1
"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
