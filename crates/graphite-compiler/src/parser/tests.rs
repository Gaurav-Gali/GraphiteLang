use std::path::PathBuf;

use crate::diagnostics::DiagnosticKind;
use crate::lexer::lex;
use crate::parser::{
    BinaryOperator, BraceEntry, ConversionMode, Expr, LiteralExpr, LoopKind, Stmt, TypeKind,
    UnaryOperator, UpdatePosition,
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
        ..
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
    let compound = parse_clean(
        "\
void main() {
    int index = 0;
    index += 1;
    index **= 2;
}
",
    );
    let Stmt::Expression(statement) = &function(&compound, "main").body.statements[1] else {
        panic!("expected compound assignment");
    };
    let Expr::Assignment(assignment) = &statement.expression else {
        panic!("expected assignment");
    };
    assert_eq!(assignment.op, Some(BinaryOperator::Add));
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
    let assigned = parse_clean(
        "\
void main() {
    int index = 0;
    loop (index = 0; index < 10; index++) {
    }
}
",
    );
    let Stmt::Loop(assigned_loop) = &function(&assigned, "main").body.statements[1] else {
        panic!("expected an assignment initializer");
    };
    assert!(matches!(
        assigned_loop.kind,
        LoopKind::CStyle {
            init_expr: Some(_),
            ..
        }
    ));
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
    let program = parse_clean(
        "\
void main() {
    int age;
}
",
    );
    let Stmt::VariableDecl(age) = &function(&program, "main").body.statements[0] else {
        panic!("expected a variable");
    };
    assert!(age.initializer.is_none());
    assert_rejected(
        "\
void main() {
    const int age;
}
",
        "a constant requires a value",
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
fn colon_is_a_slice_token_and_question_stays_lexical() {
    let colon = lex("test.grf", "void main() {\n    int a = 1;\n    a:b;\n}\n");
    assert!(colon.diagnostics.is_empty());
    let parsed = parse("test.grf", &colon.tokens);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("';'"))
    );
    let question = lex("test.grf", "void main() {\n    int? a = null;\n}\n");
    assert!(question.diagnostics.is_empty());
    let parsed = parse("test.grf", &question.tokens);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
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
        "void main() {\n    int[] values = [1, 2,];\n}\n",
        "trailing comma",
    );
    assert_rejected(
        "void main() {\n    int[1.5] values = [];\n}\n",
        "integer literal",
    );
    assert_rejected("void main() {\n    int[3 values = [];\n}\n", "expected ']'");
    assert_rejected(
        "void main() {\n    int a, int b, = [1, 2];\n}\n",
        "trailing comma",
    );
    assert_rejected(
        "void main() {\n    int a, auto int b = [1, 2];\n}\n",
        "auto is not allowed",
    );
    let class_only = parse_clean(
        "\
class Person {
}
",
    );
    assert_eq!(class_only.classes.len(), 1);
    assert_eq!(class_only.classes[0].name.name, "Person");
    assert_rejected("io.println(\"Hello\");\n", "function or class declaration");
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

#[test]
fn record_declarations_and_literals_parse() {
    let program = parse_clean(
        "\
type User {
    name: str,
    scores: int[],
}

User make() {
    return { name: \"Ada\", scores: [1, 2], };
}

void main() {
    User user = make();
    user.scores[0] = 3;
}
",
    );
    assert_eq!(program.types.len(), 1);
    assert_eq!(program.types[0].name.name, "User");
    assert_eq!(program.types[0].fields.len(), 2);
    assert_eq!(program.types[0].fields[1].ty.kind, TypeKind::Int);
    assert_eq!(program.types[0].fields[1].ty.bounds, vec![None]);
    assert_eq!(program.types[0].fields[1].conversion, ConversionMode::Exact);
    let function = function(&program, "make");
    let Stmt::Return(statement) = &function.body.statements[0] else {
        panic!("expected return");
    };
    assert!(matches!(statement.value.as_ref(), Some(Expr::Record(_))));
    assert_rejected(
        "\
void main() {
    type User {
        name: str,
    }
}
",
        "top level",
    );
    let auto_fields = parse_clean(
        "\
type rectangle {
    height: auto int,
    width: auto int,
}

void main() {
}
",
    );
    assert_eq!(
        auto_fields.types[0].fields[0].conversion,
        ConversionMode::Auto
    );
    assert_eq!(auto_fields.types[0].fields[0].ty.kind, TypeKind::Int);
    assert_eq!(
        auto_fields.types[0].fields[1].conversion,
        ConversionMode::Auto
    );
}

#[test]
fn map_is_a_keyword_and_brace_literals_keep_statement_blocks() {
    let program = parse_clean(
        "\
void main() {
    {}
    map<str, int> scores = { \"Ada\": 1, ...other, };
    map<str, map<int, str>> nested = {};
}
",
    );
    let function = function(&program, "main");
    assert!(matches!(function.body.statements[0], Stmt::Block(_)));
    let Stmt::VariableDecl(scores) = &function.body.statements[1] else {
        panic!("expected a map declaration");
    };
    assert_eq!(scores.ty.kind, TypeKind::Map);
    assert_eq!(scores.ty.arguments.len(), 2);
    assert_eq!(scores.ty.arguments[0].kind, TypeKind::Str);
    assert_eq!(scores.ty.arguments[1].kind, TypeKind::Int);
    let Expr::Record(literal) = scores.initializer.as_ref().expect("initializer") else {
        panic!("expected a brace literal");
    };
    assert!(matches!(literal.entries[1], BraceEntry::Spread { .. }));
    let Stmt::VariableDecl(nested) = &function.body.statements[2] else {
        panic!("expected a nested map declaration");
    };
    assert_eq!(nested.ty.arguments[1].kind, TypeKind::Map);

    assert_rejected("void map() {\n}\n", "'map' is a reserved keyword");
    assert_rejected(
        "\
void main() {
    int map = 1;
}
",
        "'map' is a reserved keyword",
    );
    assert_rejected(
        "\
type Box {
    map: int,
}

void main() {
}
",
        "'map' is a reserved keyword",
    );
    assert_rejected(
        "\
void main() {
    int[] values = [...scores];
}
",
        "spread is only allowed in a map literal",
    );
    assert_rejected(
        "map scores;\n\nvoid main() {\n}\n",
        "expected '<' after map",
    );
    assert_rejected(
        "map<int> scores;\n\nvoid main() {\n}\n",
        "map type requires a key type and a value type",
    );
}

#[test]
fn throw_is_reserved_and_not_a_statement() {
    let lexed = lex("test.grf", "void main() {\n    throw 1;\n}\n");
    let parsed = parse("test.grf", &lexed.tokens);
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code() == "GRF-EXC-007"),
        "{:?}",
        parsed.diagnostics
    );
    assert_rejected(
        "void main() {\n    throw 1;\n}\n",
        "explicit throw is not supported in this version",
    );
    assert_rejected(
        "void main() {\n    catch (RuntimeError error) {\n    }\n}\n",
        "catch must follow a try",
    );
    assert_rejected(
        "void main() {\n    try {\n    } finally {\n    } catch (RuntimeError error) {\n    }\n}\n",
        "catch must come before finally",
    );
}

#[test]
fn class_declarations_use_field_and_function_syntax() {
    let program = parse_clean(
        "\
class Counter {
    public int value = 0;
    private const str label = \"n\";
    int age;
    Counter(int start) {
        this.value = start;
    }
    public int getValue() {
        return this.value;
    }
    static int abs(int value) {
        return value;
    }
    private void hidden() {
    }
}

void main() {
}
",
    );
    assert_eq!(program.classes.len(), 1);
    let class = &program.classes[0];
    assert_eq!(class.name.name, "Counter");
    assert_eq!(class.fields.len(), 3);
    assert!(class.fields[0].public);
    assert!(!class.fields[0].const_field);
    assert!(class.fields[0].initializer.is_some());
    assert!(!class.fields[1].public);
    assert!(class.fields[1].const_field);
    assert!(class.fields[2].public);
    assert!(class.fields[2].initializer.is_none());
    assert_eq!(class.fields[2].name.name, "age");
    let init = class.init.as_ref().expect("constructor");
    assert!(init.public);
    assert_eq!(init.parameters.len(), 1);
    assert_eq!(init.parameters[0].name.name, "start");
    assert_eq!(class.methods.len(), 3);
    assert!(class.methods[0].public);
    assert!(class.methods[1].public);
    assert!(class.methods[1].is_static);
    assert!(!class.methods[2].public);
    assert_eq!(class.methods[0].name.name, "getValue");

    let omitted = parse_clean(
        "\
class Secret {
    Secret() {
    }
}

void main() {
}
",
    );
    assert!(omitted.classes[0].init.as_ref().unwrap().public);

    assert_rejected(
        "\
class Counter extends Other {
    value: int = 0;
}

void main() {
}
",
        "inheritance is not supported",
    );
    assert_rejected(
        "\
class Counter {
    value: int;
}

void main() {
}
",
        "class fields use Type name",
    );
    assert_rejected(
        "\
class Counter {
    init(start: int) {
    }
}

void main() {
}
",
        "the constructor is 'Counter'",
    );
    assert_rejected(
        "\
class Counter {
    Counter() {
    }
    Counter() {
    }
}

void main() {
}
",
        "duplicate constructor",
    );
}

#[test]
fn reserved_keywords_cannot_be_declaration_names() {
    let keywords = [
        "import", "auto", "const", "void", "int", "uint", "flt", "int16", "int32", "int64",
        "uint16", "uint32", "uint64", "flt32", "flt64", "bln", "chr", "str", "if", "else", "loop",
        "break", "continue", "return", "type", "map", "true", "false", "try", "catch", "finally",
        "throw", "class", "public", "private", "this", "init", "new", "static", "null",
    ];
    for keyword in keywords {
        let message = format!("'{keyword}' is a reserved keyword");
        assert_rejected(&format!("void {keyword}() {{\n}}\n"), &message);
        assert_rejected(
            &format!("void main() {{\n    int {keyword} = 1;\n}}\n"),
            &message,
        );
        assert_rejected(
            &format!("void hold(int {keyword}) {{\n}}\n\nvoid main() {{\n}}\n"),
            &message,
        );
        assert_rejected(
            &format!("type Box {{\n    {keyword}: int,\n}}\n\nvoid main() {{\n}}\n"),
            &message,
        );
    }
}
