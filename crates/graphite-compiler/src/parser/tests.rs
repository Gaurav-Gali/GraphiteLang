use std::path::PathBuf;

use crate::diagnostics::DiagnosticKind;
use crate::lexer::lex;
use crate::parser::{
    BinaryOperator, ConversionMode, Expr, LiteralExpr, LoopKind, Stmt, TypeKind, UnaryOperator,
    UpdatePosition,
};

use super::{Program, parse};

fn parse_clean(source: &str) -> Program {
    let lexed = lex("test.grf", source);
    assert!(
        lexed.diagnostics.is_empty(),
        "lexer diagnostics: {:?}",
        lexed.diagnostics
    );
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(
        parsed.diagnostics.is_empty(),
        "parser diagnostics: {:?}",
        parsed.diagnostics
    );
    parsed.program.expect("program")
}

fn parser_messages(source: &str) -> Vec<String> {
    let lexed = lex("test.grf", source);
    parse("test.grf", &lexed.tokens)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

fn assert_rejected(source: &str, snippet: &str) {
    let messages = parser_messages(source);
    assert!(
        messages.iter().any(|message| message.contains(snippet)),
        "expected a diagnostic containing {snippet:?}, got {messages:?}"
    );
}

fn function<'a>(program: &'a Program, name: &str) -> &'a super::FunctionDecl {
    program
        .functions
        .iter()
        .find(|function| function.name.name == name)
        .unwrap_or_else(|| panic!("missing function {name}"))
}

#[test]
fn minimal_entry_point() {
    let program = parse_clean("void main() {\n}\n");
    assert!(program.imports.is_empty());
    assert_eq!(program.functions.len(), 1);
    let main = function(&program, "main");
    assert_eq!(main.return_type.kind, TypeKind::Void);
    assert!(main.parameters.is_empty());
    assert!(main.body.statements.is_empty());
}

#[test]
fn import_and_library_call() {
    let program = parse_clean(
        "\
import grf.io;

void main() {
    io.println(\"Hello\");
}
",
    );
    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.imports[0].domain.name, "grf");
    assert_eq!(program.imports[0].library.name, "io");
    let main = function(&program, "main");
    let Stmt::Expression(statement) = &main.body.statements[0] else {
        panic!("expected expression statement");
    };
    let Expr::Call(call) = &statement.expression else {
        panic!("expected call");
    };
    let Expr::Member(member) = call.callee.as_ref() else {
        panic!("expected member callee");
    };
    let Expr::Identifier(object) = member.object.as_ref() else {
        panic!("expected identifier object");
    };
    assert_eq!(object.name, "io");
    assert_eq!(member.member.name, "println");
    let Expr::Literal(LiteralExpr::String { value, .. }) = &call.arguments[0] else {
        panic!("expected string argument");
    };
    assert_eq!(value, "Hello");
}

#[test]
fn nested_calls_and_multiplication() {
    let program = parse_clean(
        "\
import grf.io;

void main() {
    io.println(square(5));
}

int square(int value) {
    return value * value;
}
",
    );
    let main = function(&program, "main");
    let Stmt::Expression(statement) = &main.body.statements[0] else {
        panic!("expected call statement");
    };
    let Expr::Call(outer) = &statement.expression else {
        panic!("expected outer call");
    };
    let Expr::Call(inner) = &outer.arguments[0] else {
        panic!("expected nested call");
    };
    let Expr::Identifier(name) = inner.callee.as_ref() else {
        panic!("expected square");
    };
    assert_eq!(name.name, "square");

    let square = function(&program, "square");
    let Stmt::Return(return_statement) = &square.body.statements[0] else {
        panic!("expected return");
    };
    let Expr::Binary(binary) = return_statement.value.as_ref().unwrap() else {
        panic!("expected multiplication");
    };
    assert_eq!(binary.operator, BinaryOperator::Multiply);
}

#[test]
fn forward_function_call_parses() {
    let program = parse_clean(
        "\
void main() {
    later();
}

void later() {
}
",
    );
    assert_eq!(program.functions.len(), 2);
    assert_eq!(program.functions[0].name.name, "main");
    assert_eq!(program.functions[1].name.name, "later");
}

#[test]
fn auto_parameter_and_interpolation() {
    let program = parse_clean(
        "\
str greet(auto str msg) {
    return f\"Hello {msg}\";
}
",
    );
    let greet = function(&program, "greet");
    assert_eq!(greet.parameters.len(), 1);
    assert_eq!(greet.parameters[0].conversion, ConversionMode::Auto);
    assert_eq!(greet.parameters[0].ty.kind, TypeKind::Str);
    assert_eq!(greet.parameters[0].name.name, "msg");
    let Stmt::Return(return_statement) = &greet.body.statements[0] else {
        panic!("expected return");
    };
    let Expr::InterpolatedString(interpolated) = return_statement.value.as_ref().unwrap() else {
        panic!("expected interpolation");
    };
    assert_eq!(interpolated.parts.len(), 2);
}

#[test]
fn arithmetic_precedence_and_parentheses() {
    let source = "\
int calculate(int a, int b, int c) {
    return a + b * c;
}

int grouped() {
    return (2 + 3) * 4;
}
";
    let program = parse_clean(source);
    let calculate = function(&program, "calculate");
    let Stmt::Return(return_statement) = &calculate.body.statements[0] else {
        panic!("expected return");
    };
    let Expr::Binary(addition) = return_statement.value.as_ref().unwrap() else {
        panic!("expected addition");
    };
    assert_eq!(addition.operator, BinaryOperator::Add);
    let Expr::Binary(product) = addition.right.as_ref() else {
        panic!("expected product on the right");
    };
    assert_eq!(product.operator, BinaryOperator::Multiply);

    let grouped = function(&program, "grouped");
    let Stmt::Return(return_statement) = &grouped.body.statements[0] else {
        panic!("expected return");
    };
    let Expr::Binary(product) = return_statement.value.as_ref().unwrap() else {
        panic!("expected product");
    };
    let Expr::Parenthesized { span, expression } = product.left.as_ref() else {
        panic!("expected parentheses to remain in the AST");
    };
    let open = source.find("(2 + 3)").unwrap();
    assert_eq!(span.start.offset, open);
    assert_eq!(span.end.offset, open + "(2 + 3)".len());
    let Expr::Binary(addition) = expression.as_ref() else {
        panic!("expected addition inside parentheses");
    };
    assert_eq!(addition.operator, BinaryOperator::Add);
}

#[test]
fn assignment_is_right_associative() {
    let program = parse_clean(
        "\
void main() {
    int a = 1;
    int b = 2;
    a = b = 5;
}
",
    );
    let main = function(&program, "main");
    let Stmt::Expression(statement) = &main.body.statements[2] else {
        panic!("expected assignment statement");
    };
    let Expr::Assignment(outer) = &statement.expression else {
        panic!("expected assignment");
    };
    let Expr::Assignment(_) = outer.value.as_ref() else {
        panic!("expected nested assignment");
    };
}

#[test]
fn boolean_literals_and_unary_minus() {
    let program = parse_clean(
        "\
bln flag(int value) {
    return !true && -value < 0;
}
",
    );
    let flag = function(&program, "flag");
    let Stmt::Return(return_statement) = &flag.body.statements[0] else {
        panic!("expected return");
    };
    let Expr::Binary(and) = return_statement.value.as_ref().unwrap() else {
        panic!("expected &&");
    };
    assert_eq!(and.operator, BinaryOperator::And);
    let Expr::Unary(not) = and.left.as_ref() else {
        panic!("expected logical not");
    };
    assert_eq!(not.operator, UnaryOperator::Not);
    let Expr::Literal(LiteralExpr::Boolean { value: true, .. }) = not.operand.as_ref() else {
        panic!("expected true");
    };
}

#[test]
fn conditions_loops_and_updates() {
    let program = parse_clean(
        "\
void main() {
    int age = 18;
    if (age >= 18) {
        io.println(\"Adult\");
    } else if (age >= 13) {
        io.println(\"Teenager\");
    } else {
        io.println(\"Child\");
    }
    int index = 0;
    loop (index < 10) {
        index++;
    }
    loop (int offset = 0; offset < 10; offset++) {
        ++offset;
    }
    loop (; index < 3; index++) {
    }
    loop () {
        break;
    }
}
",
    );
    let main = function(&program, "main");
    let Stmt::If(if_statement) = &main.body.statements[1] else {
        panic!("expected if");
    };
    let Some(Stmt::If(else_if)) = if_statement.else_branch.as_deref() else {
        panic!("expected else if");
    };
    assert!(matches!(
        else_if.else_branch.as_deref(),
        Some(Stmt::Block(_))
    ));

    let Stmt::Loop(condition_loop) = &main.body.statements[3] else {
        panic!("expected condition loop");
    };
    assert!(matches!(condition_loop.kind, LoopKind::Condition { .. }));

    let Stmt::Loop(c_style) = &main.body.statements[4] else {
        panic!("expected c-style loop");
    };
    let LoopKind::CStyle {
        initializer: Some(initializer),
        condition: Some(_),
        update: Some(update),
    } = &c_style.kind
    else {
        panic!("expected a declaration initializer");
    };
    assert_eq!(initializer.name.name, "offset");
    let Expr::Update(update) = update else {
        panic!("expected update");
    };
    assert_eq!(update.position, UpdatePosition::Postfix);

    let Stmt::Expression(statement) = &c_style.body.statements[0] else {
        panic!("expected prefix update statement");
    };
    let Expr::Update(update) = &statement.expression else {
        panic!("expected prefix update");
    };
    assert_eq!(update.position, UpdatePosition::Prefix);

    let Stmt::Loop(empty_init) = &main.body.statements[5] else {
        panic!("expected empty-initializer loop");
    };
    let LoopKind::CStyle {
        initializer: None, ..
    } = &empty_init.kind
    else {
        panic!("expected an empty initializer");
    };

    let Stmt::Loop(infinite) = &main.body.statements[6] else {
        panic!("expected infinite loop");
    };
    assert!(matches!(infinite.kind, LoopKind::Infinite));
    assert!(matches!(infinite.body.statements[0], Stmt::Break(_)));
}

#[test]
fn void_is_accepted_in_every_type_position() {
    let program = parse_clean(
        "\
void f(void value) {
    void x = 1;
}
",
    );
    let function = function(&program, "f");
    assert_eq!(function.parameters[0].ty.kind, TypeKind::Void);
    let Stmt::VariableDecl(declaration) = &function.body.statements[0] else {
        panic!("expected declaration");
    };
    assert_eq!(declaration.ty.kind, TypeKind::Void);
}

#[test]
fn current_programs_parse() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative in ["test.grf", "examples/hello/main.grf"] {
        let path = root.join(relative);
        let source = std::fs::read_to_string(&path).expect("sample");
        let lexed = lex(relative, &source);
        assert!(lexed.diagnostics.is_empty(), "{relative}");
        let parsed = parse(relative, &lexed.tokens);
        assert!(parsed.is_ok(), "{relative}: {:?}", parsed.diagnostics);
    }
    let source = std::fs::read_to_string(root.join("test.grf")).unwrap();
    let lexed = lex("test.grf", &source);
    let program = parse("test.grf", &lexed.tokens).program.unwrap();
    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.functions.len(), 3);
    assert_eq!(
        function(&program, "greet").parameters[0].conversion,
        ConversionMode::Auto
    );
}

#[test]
fn local_const_parses_and_top_level_const_is_rejected() {
    let lexed = lex(
        "test.grf",
        "\
void main() {
    const int value = 1;
    const auto str label = 42;
    loop (const int step = 0; step < 1; step++) {
    }
}
",
    );
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
    let Stmt::VariableDecl(declaration) = &parsed.program.as_ref().unwrap().functions[0]
        .body
        .statements[0]
    else {
        panic!("expected a constant declaration");
    };
    assert!(!declaration.mutable);

    let lexed = lex(
        "test.grf",
        "\
const int value = 1;

void main() {
}
",
    );
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("inside a function"))
    );
    assert!(
        parsed
            .program
            .unwrap()
            .functions
            .iter()
            .any(|function| function.name.name == "main")
    );
}

#[test]
fn rejects_unary_plus_and_compound_sequences() {
    assert_rejected(
        "\
void main() {
    int index = 0;
    +index;
}
",
        "unary '+'",
    );
    assert_rejected(
        "\
void main() {
    int index = 0;
    index += 1;
}
",
        "'+='",
    );
    assert_rejected(
        "\
void main() {
    int a = 0;
    int b = a->b;
}
",
        "'->'",
    );
}

#[test]
fn rejects_bad_loop_headers() {
    assert_rejected(
        "\
void main() {
    int index = 0;
    loop (index = 0; index < 10; index++) {
    }
}
",
        "variable declaration",
    );
    let source = "\
void main() {
    loop (;;) {
    }
}
";
    let lexed = lex("test.grf", source);
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("loop (;;)"))
    );
    let Stmt::Loop(loop_statement) = &parsed.program.unwrap().functions[0].body.statements[0]
    else {
        panic!("expected a loop node");
    };
    assert!(!matches!(loop_statement.kind, LoopKind::Infinite));
}

#[test]
fn rejects_missing_initializer_and_character_syntax() {
    assert_rejected(
        "\
void main() {
    int age;
}
",
        "'='",
    );

    let source = "\
void main() {
    chr initial = 'a';
}
";
    let lexed = lex("test.grf", source);
    assert!(
        lexed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::UnexpectedCharacter)
    );
    let parsed = parse("test.grf", &lexed.tokens);
    let rendered = format!("{:?}", parsed.program);
    assert!(!rendered.contains("Character"));
}

#[test]
fn colon_and_question_remain_lexer_errors() {
    for source in [
        "void main() {\n    int a = 1;\n    a:b;\n}\n",
        "void main() {\n    int a = 1;\n    a?b;\n}\n",
    ] {
        let lexed = lex("test.grf", source);
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == DiagnosticKind::UnexpectedCharacter)
        );
    }
}

#[test]
fn rejects_invalid_syntax() {
    assert_rejected("import grf.io\n", "';'");
    assert_rejected("import grf.io.print;\n", "domain.library");
    assert_rejected(
        "\
void main() {
}

import grf.io;
",
        "before declarations",
    );
    assert_rejected("void main( {\n}\n", "')'");
    assert_rejected("void main() {\n", "'}'");
    assert_rejected("void main() {\n    return\n}\n", "';'");
    assert_rejected(
        "int add(int value) {\n    return value +\n}\n",
        "expression",
    );
    assert_rejected("void main() {\n    io.println(\"Hello\";\n}\n", "')'");
    assert_rejected(
        "int add(int value,) {\n    return value;\n}\n",
        "trailing comma",
    );
    assert_rejected(
        "\
class Person {
}
",
        "function declaration",
    );
    assert_rejected("io.println(\"Hello\");\n", "function declaration");
}

#[test]
fn interpolation_errors_are_reported() {
    let missing_brace = parser_messages(
        "\
void main() {
    str message = f\"Hello {name;
}
",
    );
    assert!(
        missing_brace
            .iter()
            .any(|message| message.contains("interpolation") || message.contains("'}'")),
        "{missing_brace:?}"
    );

    let unterminated = parser_messages(
        "\
void main() {
    str message = f\"Hello;
}
",
    );
    assert!(
        unterminated
            .iter()
            .any(|message| message.contains("interpolated string")),
        "{unterminated:?}"
    );
}

#[test]
fn recovery_keeps_the_following_function() {
    let lexed = lex(
        "test.grf",
        "\
int broken(int value {
    return value;
}

void main() {
}
",
    );
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(!parsed.diagnostics.is_empty());
    let program = parsed.program.expect("partial program");
    assert!(
        program
            .functions
            .iter()
            .any(|function| function.name.name == "main")
    );
}
