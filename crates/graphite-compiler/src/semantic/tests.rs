use crate::codegen::generate;
use crate::lexer::lex;
use crate::manifest::builtin_manifest;
use crate::parser::parse;
use crate::semantic::analyze;

fn analyze_source(source: &str) -> crate::semantic::Analysis {
    let lexed = lex("test.grf", source);
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    analyze(
        "test.grf",
        parsed.program.as_ref().unwrap(),
        &builtin_manifest(),
    )
}

fn messages(source: &str) -> String {
    analyze_source(source)
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn entry_point_diagnostics() {
    assert!(messages("void greet() {\n}\n").contains("missing the required void main()"));
    assert!(messages("void main() {\n}\nvoid main() {\n}\n").contains("duplicate entry point"));
    assert!(messages("int main() {\n    return 0;\n}\n").contains("'main' must return void"));
    assert!(messages("void main(int count) {\n}\n").contains("'main' cannot have parameters"));
}

#[test]
fn duplicate_exact_import_is_rejected() {
    let text = messages("import grf.io;\nimport grf.io;\n\nvoid main() {\n}\n");
    assert!(text.contains("imported more than once"), "{text}");
}

#[test]
fn conflicting_library_name_is_rejected() {
    let text = messages("import grf.io;\nimport other.io;\n\nvoid main() {\n}\n");
    assert!(text.contains("also binds the name 'io'"), "{text}");
}

#[test]
fn unknown_library_and_function() {
    assert!(messages("import grf.unknown;\n\nvoid main() {\n}\n").contains("was not found"));
    assert!(
        messages("import grf.io;\n\nvoid main() {\n    io.unknownFunction(\"Hello\");\n}\n")
            .contains("not exported")
    );
}

#[test]
fn integer_auto_str_is_accepted_and_float_is_not() {
    let accepted = analyze_source(
        "str greet(auto str msg) {\n    return f\"Hello {msg}\";\n}\n\nvoid main() {\n    greet(123);\n}\n",
    );
    assert!(
        accepted.diagnostics.is_empty(),
        "{:?}",
        accepted.diagnostics
    );

    let rejected = messages(
        "str greet(auto str msg) {\n    return msg;\n}\n\nvoid main() {\n    greet(1.5);\n}\n",
    );
    assert!(
        rejected.contains("not specified") || rejected.contains("floating-point"),
        "{rejected}"
    );
}

#[test]
fn short_circuit_is_not_emitted_as_valid() {
    let text = messages("void main() {\n    bln value = true && false;\n}\n");
    assert!(text.contains("short-circuit"), "{text}");
}

#[test]
fn sample_program_typechecks() {
    let source = include_str!("../../../../test.grf");
    let lexed = lex("test.grf", source);
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let analysis = analyze(
        "test.grf",
        parsed.program.as_ref().unwrap(),
        &builtin_manifest(),
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[test]
fn codegen_uses_the_manifest_rust_path() {
    let analysis = analyze_source("import grf.io;\n\nvoid main() {\n    io.print(\"Hi\");\n}\n");
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let rust = generate(analysis.program.as_ref().unwrap());
    let manifest = builtin_manifest();
    let path = &manifest.libraries["io"].functions["print"].rust_path;
    assert!(rust.contains(path), "{rust}");
    assert!(!rust.contains("graphite_core::io::println"));
}
