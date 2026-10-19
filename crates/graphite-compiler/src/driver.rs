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

use crate::codegen::generate;
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

pub fn run_path(path: &Path, print_ast: bool) -> i32 {
    match prepare(path, print_ast) {
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
    match prepare(path, false) {
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

fn failed(stderr: impl Into<String>) -> RunOutput {
    RunOutput {
        status: 1,
        stdout: String::new(),
        stderr: stderr.into(),
        build_dir: None,
        working_directory: None,
    }
}

fn prepare(path: &Path, print_ast: bool) -> Result<Prepared, RunOutput> {
    let bytes = fs::read(path)
        .map_err(|error| failed(format!("grf: cannot read {}: {error}\n", path.display())))?;
    let source = String::from_utf8(bytes)
        .map_err(|_| failed(format!("grf: {} is not valid UTF-8\n", path.display())))?;
    let source_name = path.display().to_string();
    let lexed = lex(&source_name, &source);
    if !lexed.diagnostics.is_empty() {
        let mut stderr = String::new();
        for diagnostic in &lexed.diagnostics {
            stderr.push_str(&diagnostic.to_string());
            stderr.push('\n');
        }
        return Err(failed(stderr));
    }
    let parsed = parse(&source_name, &lexed.tokens);
    let ast = parsed.program.as_ref().map(format_ast);
    if !parsed.diagnostics.is_empty() {
        let mut stderr = String::new();
        for diagnostic in &parsed.diagnostics {
            stderr.push_str(&diagnostic.to_string());
            stderr.push('\n');
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
            stderr.push_str(&diagnostic.to_string());
            stderr.push('\n');
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
    let checked = analysis.program.expect("checked program");
    let rust = generate(&checked);
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

[workspace]
resolver = "3"
"#
    );
    fs::write(build_dir.join("Cargo.toml"), manifest_text)
        .and_then(|_| fs::write(build_dir.join("src/main.rs"), rust))
        .map_err(|error| failed(format!("grf: cannot write generated files: {error}\n")))?;

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
        let mut stderr = String::from("grf: generated Rust failed to compile\n");
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
        compiler_stderr: String::new(),
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
    fn sample_program_runs() {
        let _guard = BUILD_LOCK.lock().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test.grf");
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "25\nHello 123\n");
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
    loop (uint64 idx, int value = [8].enumerate()) {
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
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(
            output.stdout,
            "[1, 2, 3]\n[1, 8, 4]\n[[1], []]\n[1, 2, 3, 4]\n"
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
            output.stderr.contains("array index out of bounds"),
            "{}",
            output.stderr
        );

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
                "integer overflow",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    int a = 1;\n    int b = 0;\n    io.println(a / b);\n}\n",
                "division by zero",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    int worst = -9223372036854775808;\n    io.println(worst / -1);\n}\n",
                "integer overflow",
            ),
            (
                "import grf.io;\n\nvoid main() {\n    io.println(1 % 0);\n}\n",
                "division by zero",
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
}
"#,
        )
        .unwrap();
        let output = run_captured(&path);
        assert_eq!(output.status, 0, "{}", output.stderr);
        assert_eq!(output.stdout, "25\n13\n13.5\n7\n9\n26\n");

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
                "import grf.io;\n\nvoid main() {\n    auto int age = \"1\";\n    age = 1.5;\n}\n",
                "not an integer",
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
    io.println(int(chr("A")));
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
            "123\n-42\n123\n123\n40000\n-2147483648\n2147483647\n-32768\n65535\n4294967295\n12.5\n0.01\n1.5\n1.5\ntrue\né\n65\n1\n9\n8\n8\n2.5\n1.5\n1.5\n456\n"
        );
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
            output.stderr.contains("unexpected end of input"),
            "{}",
            output.stderr
        );
        assert!(output.stdout.is_empty(), "{:?}", output.stdout);
        let _ = fs::remove_dir_all(&dir);
    }
}
