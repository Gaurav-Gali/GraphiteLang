use std::env;
use std::path::Path;
use std::process;

use graphite_compiler::driver::run_path;

fn main() {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        process::exit(2);
    };

    match command.as_str() {
        "run" => {
            let mut path = None;
            let mut print_ast = false;
            for arg in args {
                if arg == "--ast" {
                    print_ast = true;
                } else if path.is_none() {
                    path = Some(arg);
                } else {
                    eprintln!("grf run: unexpected extra argument");
                    process::exit(2);
                }
            }
            let Some(path) = path else {
                eprintln!("grf run: missing source file");
                usage();
                process::exit(2);
            };
            process::exit(run_path(Path::new(&path), print_ast));
        }
        other => {
            eprintln!("grf: unknown command '{other}'");
            usage();
            process::exit(2);
        }
    }
}

fn usage() {
    eprintln!("Usage: grf run <file.grf> [--ast]");
}
