use std::env;
use std::fs;
use std::path::Path;
use std::process;

use graphite_compiler::lex;

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
            if args.next().is_some() {
                eprintln!("grf run: unexpected extra argument");
                process::exit(2);
            }
            run(Path::new(&path));
        }
        other => {
            eprintln!("grf: unknown command '{other}'");
            usage();
            process::exit(2);
        }
    }
}

fn run(path: &Path) {
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
    let result = lex(&source_name, &source);
    for diagnostic in &result.diagnostics {
        eprintln!("{diagnostic}");
    }
    for token in &result.tokens {
        let span = &token.span;
        println!(
            "{}:{}-{}:{} {token}",
            span.start.line, span.start.column, span.end.line, span.end.column
        );
    }

    if !result.diagnostics.is_empty() {
        process::exit(1);
    }

    eprintln!("lexer: {} tokens, 0 diagnostics", result.tokens.len());
    eprintln!("parsing, semantic analysis, and code generation are not implemented yet");
    process::exit(2);
}

fn usage() {
    eprintln!("Usage: grf run <file.grf>");
}
