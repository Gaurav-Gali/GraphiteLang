use std::env;
use std::fs;
use std::path::Path;
use std::process;

use graphite_compiler::{format_ast, lex, parse};

fn main() {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        process::exit(2);
    };

    match command.as_str() {
        "run" => {
            let Some(path) = args.next() else {
                eprintln!("grf run: missing source file");
                usage();
                process::exit(2);
            };
            let mut show_ast = false;
            for argument in args {
                if argument == "--ast" {
                    show_ast = true;
                } else {
                    eprintln!("grf run: unexpected extra argument '{argument}'");
                    process::exit(2);
                }
            }
            run(Path::new(&path), show_ast);
        }
        other => {
            eprintln!("grf: unknown command '{other}'");
            usage();
            process::exit(2);
        }
    }
}

fn run(path: &Path, show_ast: bool) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("grf: cannot read {}: {error}", path.display());
            process::exit(1);
        }
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => {
            eprintln!("grf: {} is not valid UTF-8", path.display());
            process::exit(1);
        }
    };

    let source_name = path.display().to_string();
    let lexed = lex(&source_name, &source);
    if !lexed.diagnostics.is_empty() {
        for diagnostic in &lexed.diagnostics {
            eprintln!("{diagnostic}");
        }
        process::exit(1);
    }

    let parsed = parse(&source_name, &lexed.tokens);
    if !parsed.diagnostics.is_empty() {
        for diagnostic in &parsed.diagnostics {
            eprintln!("{diagnostic}");
        }
        process::exit(1);
    }

    let program = parsed
        .program
        .expect("a token stream with no parser diagnostics has a program");
    if show_ast {
        print!("{}", format_ast(&program));
    }
    eprintln!(
        "parsed {} import(s), {} function(s)",
        program.imports.len(),
        program.functions.len()
    );
    eprintln!("semantic analysis and code generation are not implemented yet");
    process::exit(2);
}

fn usage() {
    eprintln!("Usage: grf run <file.grf> [--ast]");
}
