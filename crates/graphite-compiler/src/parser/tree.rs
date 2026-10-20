use std::fmt::Write;

use super::ast::*;

/// Writes a readable program tree. Spans are omitted so the structure stays visible.
pub fn format_ast(program: &Program) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Program");
    let import_count = program.imports.len();
    let type_count = program.types.len();
    let function_count = program.functions.len();
    for (index, import) in program.imports.iter().enumerate() {
        let last = index + 1 == import_count && type_count == 0 && function_count == 0;
        write_line(
            &mut out,
            "",
            last,
            &format!("Import {}.{}", import.domain.name, import.library.name),
        );
    }
    for (index, declaration) in program.types.iter().enumerate() {
        let last = index + 1 == type_count && function_count == 0;
        let fields = declaration
            .fields
            .iter()
            .map(|field| {
                let ty = match field.conversion {
                    ConversionMode::Auto => format!("auto {}", format_type(&field.ty)),
                    ConversionMode::Exact => format_type(&field.ty),
                };
                format!("{}: {ty}", field.name.name)
            })
            .collect::<Vec<_>>()
            .join(", ");
        write_line(
            &mut out,
            "",
            last,
            &format!("Type {} {{ {fields} }}", declaration.name.name),
        );
    }
    for (index, function) in program.functions.iter().enumerate() {
        let last = index + 1 == function_count;
        write_function(&mut out, "", last, function);
    }
    out
}

fn write_function(out: &mut String, prefix: &str, last: bool, function: &FunctionDecl) {
    let params = function
        .parameters
        .iter()
        .map(format_parameter)
        .collect::<Vec<_>>()
        .join(", ");
    write_line(
        out,
        prefix,
        last,
        &format!(
            "Function {} {}({params})",
            format_type(&function.return_type),
            function.name.name
        ),
    );
    let child = child_prefix(prefix, last);
    write_block(out, &child, true, &function.body);
}

fn write_block(out: &mut String, prefix: &str, last: bool, block: &BlockStmt) {
    write_line(out, prefix, last, "Block");
    let child = child_prefix(prefix, last);
    let count = block.statements.len();
    for (index, statement) in block.statements.iter().enumerate() {
        write_statement(out, &child, index + 1 == count, statement);
    }
}

fn write_statement(out: &mut String, prefix: &str, last: bool, statement: &Stmt) {
    match statement {
        Stmt::Block(block) => write_block(out, prefix, last, block),
        Stmt::VariableDecl(declaration) => {
            write_line(
                out,
                prefix,
                last,
                &format!(
                    "{} {} {}",
                    if declaration.mutable {
                        "Variable"
                    } else {
                        "Constant"
                    },
                    format_typed_name(
                        declaration.conversion,
                        &declaration.ty,
                        &declaration.name.name
                    ),
                    ""
                )
                .trim_end(),
            );
            let child = child_prefix(prefix, last);
            write_line(out, &child, true, "Initializer");
            write_expression(
                out,
                &child_prefix(&child, true),
                true,
                &declaration.initializer,
            );
        }
        Stmt::Return(return_statement) => {
            write_line(out, prefix, last, "Return");
            if let Some(value) = &return_statement.value {
                write_expression(out, &child_prefix(prefix, last), true, value);
            }
        }
        Stmt::If(if_statement) => {
            write_line(out, prefix, last, "If");
            let child = child_prefix(prefix, last);
            let has_else = if_statement.else_branch.is_some();
            write_line(out, &child, false, "Condition");
            write_expression(
                out,
                &child_prefix(&child, false),
                true,
                &if_statement.condition,
            );
            write_block(out, &child, !has_else, &if_statement.then_branch);
            if let Some(else_branch) = &if_statement.else_branch {
                write_line(out, &child, true, "Else");
                write_statement(out, &child_prefix(&child, true), true, else_branch);
            }
        }
        Stmt::Loop(loop_statement) => {
            write_line(out, prefix, last, "Loop");
            let child = child_prefix(prefix, last);
            match &loop_statement.kind {
                LoopKind::Infinite => write_line(out, &child, false, "Infinite"),
                LoopKind::Condition { condition } => {
                    write_line(out, &child, false, "Condition");
                    write_expression(out, &child_prefix(&child, false), true, condition);
                }
                LoopKind::Iteration { bindings, iterable } => {
                    write_line(out, &child, false, "Iteration");
                    let header = child_prefix(&child, false);
                    for (index, binding) in bindings.iter().enumerate() {
                        let binding_last = index + 1 == bindings.len();
                        write_line(
                            out,
                            &header,
                            binding_last && false,
                            &format_typed_name(binding.conversion, &binding.ty, &binding.name.name),
                        );
                    }
                    write_line(out, &header, true, "Iterable");
                    write_expression(out, &child_prefix(&header, true), true, iterable);
                }
                LoopKind::CStyle {
                    initializer,
                    init_expr,
                    condition,
                    update,
                } => {
                    write_line(out, &child, false, "CStyle");
                    let header = child_prefix(&child, false);
                    write_line(out, &header, false, "Initializer");
                    if let Some(expression) = init_expr {
                        write_expression(out, &child_prefix(&header, false), true, expression);
                    }
                    if let Some(declaration) = initializer {
                        write_line(
                            out,
                            &child_prefix(&header, false),
                            true,
                            &format_typed_name(
                                declaration.conversion,
                                &declaration.ty,
                                &declaration.name.name,
                            ),
                        );
                    }
                    write_line(out, &header, false, "Condition");
                    if let Some(condition) = condition {
                        write_expression(out, &child_prefix(&header, false), true, condition);
                    }
                    write_line(out, &header, true, "Update");
                    if let Some(update) = update {
                        write_expression(out, &child_prefix(&header, true), true, update);
                    }
                }
            }
            write_block(out, &child, true, &loop_statement.body);
        }
        Stmt::Break(_) => write_line(out, prefix, last, "Break"),
        Stmt::Continue(_) => write_line(out, prefix, last, "Continue"),
        Stmt::Destructure(declaration) => {
            write_line(out, prefix, last, "Destructure");
            let child = child_prefix(prefix, last);
            for (index, binding) in declaration.bindings.iter().enumerate() {
                write_line(
                    out,
                    &child,
                    false,
                    &format_typed_name(binding.conversion, &binding.ty, &binding.name.name),
                );
                let _ = index;
            }
            write_line(out, &child, true, "Initializer");
            write_expression(
                out,
                &child_prefix(&child, true),
                true,
                &declaration.initializer,
            );
        }
        Stmt::Expression(expression) => {
            write_line(out, prefix, last, "Expression");
            write_expression(
                out,
                &child_prefix(prefix, last),
                true,
                &expression.expression,
            );
        }
    }
}

fn write_expression(out: &mut String, prefix: &str, last: bool, expression: &Expr) {
    match expression {
        Expr::Identifier(identifier) => {
            write_line(
                out,
                prefix,
                last,
                &format!("Identifier {}", identifier.name),
            );
        }
        Expr::Literal(literal) => write_line(out, prefix, last, &format_literal(literal)),
        Expr::Unary(unary) => {
            let symbol = match unary.operator {
                UnaryOperator::Negate => "-",
                UnaryOperator::Not => "!",
            };
            write_line(out, prefix, last, &format!("Unary {symbol}"));
            write_expression(out, &child_prefix(prefix, last), true, &unary.operand);
        }
        Expr::Binary(binary) => {
            write_line(
                out,
                prefix,
                last,
                &format!("Binary {}", binary_symbol(binary.operator)),
            );
            let child = child_prefix(prefix, last);
            write_expression(out, &child, false, &binary.left);
            write_expression(out, &child, true, &binary.right);
        }
        Expr::Assignment(assignment) => {
            write_line(out, prefix, last, "Assignment");
            let child = child_prefix(prefix, last);
            write_expression(out, &child, false, &assignment.target);
            write_expression(out, &child, true, &assignment.value);
        }
        Expr::Conversion(conversion) => {
            write_line(
                out,
                prefix,
                last,
                &format!("Conversion {}", format_type(&conversion.target)),
            );
            let child = child_prefix(prefix, last);
            for (index, argument) in conversion.arguments.iter().enumerate() {
                let argument_last = index + 1 == conversion.arguments.len();
                write_expression(out, &child, argument_last, argument);
            }
            if conversion.arguments.is_empty() {
                write_line(out, &child, true, "Arguments");
            }
        }
        Expr::Call(call) => {
            write_line(out, prefix, last, "Call");
            let child = child_prefix(prefix, last);
            let arg_count = call.arguments.len();
            write_line(out, &child, arg_count == 0, "Callee");
            write_expression(
                out,
                &child_prefix(&child, arg_count == 0),
                true,
                &call.callee,
            );
            for (index, argument) in call.arguments.iter().enumerate() {
                let arg_last = index + 1 == arg_count;
                write_line(out, &child, arg_last, "Argument");
                write_expression(out, &child_prefix(&child, arg_last), true, argument);
            }
        }
        Expr::Member(member) => {
            write_line(out, prefix, last, &format!("Member {}", member.member.name));
            write_expression(out, &child_prefix(prefix, last), true, &member.object);
        }
        Expr::Update(update) => {
            let symbol = match update.operator {
                UpdateOperator::Increment => "++",
                UpdateOperator::Decrement => "--",
            };
            let position = match update.position {
                UpdatePosition::Prefix => "prefix",
                UpdatePosition::Postfix => "postfix",
            };
            write_line(out, prefix, last, &format!("Update {position} {symbol}"));
            write_expression(out, &child_prefix(prefix, last), true, &update.operand);
        }
        Expr::InterpolatedString(interpolated) => {
            write_line(out, prefix, last, "InterpolatedString");
            let child = child_prefix(prefix, last);
            let count = interpolated.parts.len();
            for (index, part) in interpolated.parts.iter().enumerate() {
                let part_last = index + 1 == count;
                match part {
                    StringPart::Text { value, .. } => {
                        write_line(out, &child, part_last, &format!("Text {value:?}"));
                    }
                    StringPart::Expression { expression, .. } => {
                        write_line(out, &child, part_last, "Expression");
                        write_expression(out, &child_prefix(&child, part_last), true, expression);
                    }
                }
            }
        }
        Expr::Record(record) => {
            write_line(out, prefix, last, "Brace");
            let child = child_prefix(prefix, last);
            let count = record.entries.len();
            if count == 0 {
                write_line(out, &child, true, "Empty");
            }
            for (index, entry) in record.entries.iter().enumerate() {
                let entry_last = index + 1 == count;
                match entry {
                    BraceEntry::Pair { key, value, .. } => {
                        write_line(out, &child, entry_last, "Entry");
                        let entry_child = child_prefix(&child, entry_last);
                        write_expression(out, &entry_child, false, key);
                        write_expression(out, &entry_child, true, value);
                    }
                    BraceEntry::Spread { value, .. } => {
                        write_line(out, &child, entry_last, "Spread");
                        write_expression(out, &child_prefix(&child, entry_last), true, value);
                    }
                }
            }
        }
        Expr::Array(array) => {
            write_line(out, prefix, last, "Array");
            let child = child_prefix(prefix, last);
            let count = array.elements.len();
            if count == 0 {
                write_line(out, &child, true, "Empty");
            }
            for (index, element) in array.elements.iter().enumerate() {
                write_expression(out, &child, index + 1 == count, element);
            }
        }
        Expr::Index(index) => {
            write_line(out, prefix, last, "Index");
            let child = child_prefix(prefix, last);
            write_expression(out, &child, false, &index.object);
            write_expression(out, &child, true, &index.index);
        }
        Expr::Slice(slice) => {
            write_line(out, prefix, last, "Slice");
            let child = child_prefix(prefix, last);
            write_optional_expr(out, &child, false, "Start", slice.start.as_deref());
            write_optional_expr(out, &child, false, "Stop", slice.stop.as_deref());
            write_optional_expr(out, &child, true, "Step", slice.step.as_deref());
            write_expression(out, &child, false, &slice.object);
        }
        Expr::Parenthesized { expression, .. } => {
            write_line(out, prefix, last, "Parenthesized");
            write_expression(out, &child_prefix(prefix, last), true, expression);
        }
    }
}

fn format_parameter(parameter: &Parameter) -> String {
    format_typed_name(parameter.conversion, &parameter.ty, &parameter.name.name)
}

fn format_typed_name(conversion: ConversionMode, ty: &TypeSyntax, name: &str) -> String {
    match conversion {
        ConversionMode::Auto => format!("auto {} {name}", format_type(ty)),
        ConversionMode::Exact => format!("{} {name}", format_type(ty)),
    }
}

fn write_optional_expr(
    out: &mut String,
    prefix: &str,
    last: bool,
    label: &str,
    expression: Option<&Expr>,
) {
    write_line(
        out,
        prefix,
        last && expression.is_none(),
        &format!(
            "{label}{}",
            if expression.is_none() { " omitted" } else { "" }
        ),
    );
    if let Some(expression) = expression {
        write_expression(out, &child_prefix(prefix, last), true, expression);
    }
}

fn format_type(ty: &TypeSyntax) -> String {
    let mut name = if ty.kind == TypeKind::Map {
        let key = ty
            .arguments
            .first()
            .map(format_type)
            .unwrap_or_else(|| "unknown".to_string());
        let value = ty
            .arguments
            .get(1)
            .map(format_type)
            .unwrap_or_else(|| "unknown".to_string());
        format!("map<{key}, {value}>")
    } else {
        match ty.kind {
            TypeKind::Void => "void",
            TypeKind::Int => "int",
            TypeKind::UInt => "uint",
            TypeKind::Flt => "flt",
            TypeKind::Int16 => "int16",
            TypeKind::Int32 => "int32",
            TypeKind::Int64 => "int64",
            TypeKind::UInt16 => "uint16",
            TypeKind::UInt32 => "uint32",
            TypeKind::UInt64 => "uint64",
            TypeKind::Flt32 => "flt32",
            TypeKind::Flt64 => "flt64",
            TypeKind::Bln => "bln",
            TypeKind::Chr => "chr",
            TypeKind::Str => "str",
            TypeKind::Named => ty.name.as_str(),
            TypeKind::Map => "map",
        }
        .to_string()
    };
    for bound in &ty.bounds {
        match bound {
            None => name.push_str("[]"),
            Some(limit) => name.push_str(&format!("[{limit}]")),
        }
    }
    name
}

fn format_literal(literal: &LiteralExpr) -> String {
    match literal {
        LiteralExpr::Integer { lexeme, .. } => format!("Integer {lexeme}"),
        LiteralExpr::Float { lexeme, .. } => format!("Float {lexeme}"),
        LiteralExpr::Boolean { value, .. } => format!("Boolean {value}"),
        LiteralExpr::String { value, .. } => format!("String {value:?}"),
    }
}

fn binary_symbol(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
    }
}

fn write_line(out: &mut String, prefix: &str, last: bool, text: &str) {
    let branch = if prefix.is_empty() {
        if last { "└── " } else { "├── " }
    } else if last {
        "└── "
    } else {
        "├── "
    };
    let _ = writeln!(out, "{prefix}{branch}{text}");
}

fn child_prefix(prefix: &str, last: bool) -> String {
    if prefix.is_empty() {
        if last {
            "    ".to_string()
        } else {
            "│   ".to_string()
        }
    } else if last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::lex;
    use crate::parser::parse;

    use super::format_ast;

    #[test]
    fn tree_shows_import_and_function() {
        let source = "import grf.io;\n\nvoid main() {\n    io.println(\"Hello\");\n}\n";
        let lexed = lex("tree.grf", source);
        let parsed = parse("tree.grf", &lexed.tokens);
        let tree = format_ast(parsed.program.as_ref().unwrap());
        assert!(tree.contains("Import grf.io"));
        assert!(tree.contains("Function void main()"));
        assert!(tree.contains("Member println"));
        assert!(tree.contains("String \"Hello\""));
    }
}
