//! Rust code generation.
//!
//! Generation reads a checked program. It does not rewrite Graphite source
//! text. Library calls use the Rust path stored on each call, which semantic
//! analysis copied from `grf.json`. Integer arithmetic is checked and aborts
//! on overflow instead of wrapping. That abort is a guard until the overflow
//! policy is frozen. Floating-point arithmetic uses Rust's operators.
//! Number, boolean, and character text uses `graphite_core::convert`, shared
//! by explicit `str(value)`, `auto str`, and string interpolation.
//! `grf_str_length` counts Unicode scalar values for the `lengthOf` built-in.

use std::path::{Path, PathBuf};

use crate::parser::{BinaryOperator, UnaryOperator};
use crate::semantic::convert::ConvOp;
use crate::semantic::{
    CanonicalType, CheckedExpr, CheckedFn, CheckedPart, CheckedProgram, CheckedStmt,
};

pub fn runtime_crate_relative_path() -> PathBuf {
    PathBuf::from(Path::new("crates/graphite-core"))
}

pub fn generate(program: &CheckedProgram) -> String {
    let mut emitter = Emitter {
        out: String::from(PRELUDE),
        indent: 0,
        loops: Vec::new(),
        tmps: 0,
        loop_ids: 0,
    };
    let mut others = Vec::new();
    let mut entry = Vec::new();
    for function in &program.functions {
        if function.name == "main" {
            entry.push(function);
        } else {
            others.push(function);
        }
    }
    if !others.is_empty() {
        emitter.line("mod grf_fns {");
        emitter.indent += 1;
        emitter.line("use super::*;");
        for function in others {
            emitter.emit_function(function);
        }
        emitter.indent -= 1;
        emitter.line("}");
        emitter.out.push('\n');
    }
    for function in entry {
        emitter.emit_function(function);
    }
    emitter.out
}

struct Emitter {
    out: String,
    indent: usize,
    loops: Vec<(String, String)>,
    tmps: u32,
    loop_ids: u32,
}

impl Emitter {
    fn emit_function(&mut self, function: &CheckedFn) {
        let name = rust_ident(&function.name);
        let visibility = if function.name == "main" {
            ""
        } else {
            "pub(crate) "
        };
        let params = function
            .params
            .iter()
            .map(|(name, ty)| format!("mut {}: {}", rust_ident(name), ty.rust_name()))
            .collect::<Vec<_>>()
            .join(", ");
        if function.ret == CanonicalType::Void {
            self.line(&format!("{visibility}fn {name}({params}) {{"));
        } else {
            self.line(&format!(
                "{visibility}fn {name}({params}) -> {} {{",
                function.ret.rust_name()
            ));
        }
        self.indent += 1;
        for statement in &function.body {
            self.emit_stmt(statement);
        }
        self.indent -= 1;
        self.line("}");
        self.out.push('\n');
    }

    fn emit_stmt(&mut self, stmt: &CheckedStmt) {
        match stmt {
            CheckedStmt::Let {
                name,
                ty,
                value,
                mutable,
            } => {
                let value = self.emit_expr(value);
                let binding = if *mutable { "let mut" } else { "let" };
                self.line(&format!(
                    "{binding} {}: {} = {value};",
                    rust_ident(name),
                    ty.rust_name()
                ));
            }
            CheckedStmt::Return(None) => self.line("return;"),
            CheckedStmt::Return(Some(value)) => {
                let value = self.emit_expr(value);
                self.line(&format!("return {value};"));
            }
            CheckedStmt::Expr(expr) => {
                let value = self.emit_expr(expr);
                if expr.ty() == CanonicalType::Void {
                    self.line(&format!("{value};"));
                } else {
                    self.line(&format!("let _ = {value};"));
                }
            }
            CheckedStmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = self.emit_expr(condition);
                self.line(&format!("if {condition} {{"));
                self.emit_body(then_body);
                if let Some(else_body) = else_body {
                    self.line("} else {");
                    self.emit_body(else_body);
                }
                self.line("}");
            }
            CheckedStmt::While { condition, body } => {
                let label = self.fresh_label();
                let condition = self.emit_expr(condition);
                self.loops.push((label.clone(), label.clone()));
                self.line(&format!("'{label}: while {condition} {{"));
                self.emit_body(body);
                self.line("}");
                self.loops.pop();
            }
            CheckedStmt::Loop { body } => {
                let label = self.fresh_label();
                self.loops.push((label.clone(), label.clone()));
                self.line(&format!("'{label}: loop {{"));
                self.emit_body(body);
                self.line("}");
                self.loops.pop();
            }
            CheckedStmt::CStyle {
                initializer,
                condition,
                update,
                body,
            } => {
                self.line("{");
                self.indent += 1;
                if let Some(initializer) = initializer {
                    self.emit_stmt(initializer);
                }
                let outer = self.fresh_label();
                let inner = self.fresh_label();
                let condition = self.emit_expr(condition);
                self.line(&format!("'{outer}: while {condition} {{"));
                self.indent += 1;
                self.line(&format!("'{inner}: loop {{"));
                self.loops.push((outer.clone(), inner.clone()));
                self.emit_body(body);
                self.loops.pop();
                self.line(&format!("break '{inner};"));
                self.line("}");
                if let Some(update) = update {
                    let ty = update.ty();
                    let update = self.emit_expr(update);
                    if ty == CanonicalType::Void {
                        self.line(&format!("{update};"));
                    } else {
                        self.line(&format!("let _ = {update};"));
                    }
                }
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            CheckedStmt::Break => {
                if let Some((label, _)) = self.loops.last() {
                    let label = label.clone();
                    self.line(&format!("break '{label};"));
                }
            }
            CheckedStmt::Continue => {
                if let Some((break_label, continue_label)) = self.loops.last() {
                    if break_label == continue_label {
                        let label = continue_label.clone();
                        self.line(&format!("continue '{label};"));
                    } else {
                        let label = continue_label.clone();
                        self.line(&format!("break '{label};"));
                    }
                }
            }
            CheckedStmt::Block { body } => {
                self.line("{");
                self.emit_body(body);
                self.line("}");
            }
        }
    }

    fn emit_body(&mut self, body: &[CheckedStmt]) {
        self.indent += 1;
        for statement in body {
            self.emit_stmt(statement);
        }
        self.indent -= 1;
    }

    fn emit_expr(&mut self, expr: &CheckedExpr) -> String {
        match expr {
            CheckedExpr::Int { value, ty } => emit_int(*value, *ty),
            CheckedExpr::Float { lexeme, ty } => emit_float(lexeme, *ty),
            CheckedExpr::Bool(value) => value.to_string(),
            CheckedExpr::Char(value) => format!("char::from_u32({}).unwrap()", u32::from(*value)),
            CheckedExpr::Str(value) => format!("String::from({})", rust_string(value)),
            CheckedExpr::Load { name, ty } => {
                let name = rust_ident(name);
                if *ty == CanonicalType::Str {
                    format!("{name}.clone()")
                } else {
                    name
                }
            }
            CheckedExpr::Unary { op, expr, ty } => {
                let inner = self.emit_expr(expr);
                match op {
                    UnaryOperator::Not => format!("(!{inner})"),
                    UnaryOperator::Negate if ty.is_float() => format!("(-({inner}))"),
                    UnaryOperator::Negate => format!("grf_neg!({inner})"),
                }
            }
            CheckedExpr::Binary {
                op, left, right, ..
            } => {
                let float_operand = left.ty().is_float();
                let left = self.emit_expr(left);
                let right = self.emit_expr(right);
                if float_operand {
                    return match op {
                        BinaryOperator::Add => format!("({left} + {right})"),
                        BinaryOperator::Subtract => format!("({left} - {right})"),
                        BinaryOperator::Multiply => format!("({left} * {right})"),
                        BinaryOperator::Divide => format!("({left} / {right})"),
                        BinaryOperator::Less => format!("({left} < {right})"),
                        BinaryOperator::LessEqual => format!("({left} <= {right})"),
                        BinaryOperator::Greater => format!("({left} > {right})"),
                        BinaryOperator::GreaterEqual => format!("({left} >= {right})"),
                        BinaryOperator::Equal => format!("({left} == {right})"),
                        BinaryOperator::NotEqual => format!("({left} != {right})"),
                        BinaryOperator::Remainder | BinaryOperator::And | BinaryOperator::Or => {
                            "false".to_string()
                        }
                    };
                }
                match op {
                    BinaryOperator::Add => format!("grf_add!({left}, {right})"),
                    BinaryOperator::Subtract => format!("grf_sub!({left}, {right})"),
                    BinaryOperator::Multiply => format!("grf_mul!({left}, {right})"),
                    BinaryOperator::Divide => format!("grf_div!({left}, {right})"),
                    BinaryOperator::Remainder => format!("grf_rem!({left}, {right})"),
                    BinaryOperator::Less => format!("({left} < {right})"),
                    BinaryOperator::LessEqual => format!("({left} <= {right})"),
                    BinaryOperator::Greater => format!("({left} > {right})"),
                    BinaryOperator::GreaterEqual => format!("({left} >= {right})"),
                    BinaryOperator::Equal => format!("({left} == {right})"),
                    BinaryOperator::NotEqual => format!("({left} != {right})"),
                    BinaryOperator::And | BinaryOperator::Or => "false".to_string(),
                }
            }
            CheckedExpr::Assign { name, value, ty } => {
                let name = rust_ident(name);
                let value = self.emit_expr(value);
                if *ty == CanonicalType::Str {
                    format!("{{ {name} = {value}; {name}.clone() }}")
                } else {
                    format!("{{ {name} = {value}; {name} }}")
                }
            }
            CheckedExpr::Update {
                prefix,
                increment,
                name,
                ..
            } => {
                let name = rust_ident(name);
                let op = if *increment { "grf_add" } else { "grf_sub" };
                if *prefix {
                    format!("{{ {name} = {op}!({name}, 1); {name} }}")
                } else {
                    let tmp = self.fresh_tmp();
                    format!("{{ let {tmp} = {name}; {name} = {op}!({name}, 1); {tmp} }}")
                }
            }
            CheckedExpr::LengthOf { expr } => {
                let inner = self.emit_expr(expr);
                format!("grf_str_length(&({inner}))")
            }
            CheckedExpr::Call { path, args, .. } => {
                let path = if path.contains("::") {
                    path.clone()
                } else if path == "main" {
                    "crate::main".to_string()
                } else {
                    format!("crate::grf_fns::{}", rust_ident(path))
                };
                let args = args
                    .iter()
                    .map(|arg| self.emit_expr(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{path}({args})")
            }
            CheckedExpr::Convert {
                from,
                to,
                op,
                expr,
                file,
                line,
                column,
            } => {
                let inner = self.emit_expr(expr);
                let site = format!(
                    "graphite_core::convert::Site {{ file: {}, line: {line}, column: {column} }}",
                    rust_string(file)
                );
                emit_conv(*op, *from, *to, &inner, &site)
            }
            CheckedExpr::Interpolated { parts } => {
                let mut lines = vec![
                    "{".to_string(),
                    "let mut grf_text = String::new();".to_string(),
                ];
                for part in parts {
                    match part {
                        CheckedPart::Text(text) => {
                            lines.push(format!("grf_text.push_str({});", rust_string(text)));
                        }
                        CheckedPart::Expr(expr) => {
                            let value = self.emit_expr(expr);
                            lines.push(format!("grf_text.push_str(&({value}));"));
                        }
                    }
                }
                lines.push("grf_text".to_string());
                lines.push("}".to_string());
                lines.join(" ")
            }
        }
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn fresh_label(&mut self) -> String {
        let id = self.loop_ids;
        self.loop_ids += 1;
        format!("grf_loop_{id}")
    }

    fn fresh_tmp(&mut self) -> String {
        let id = self.tmps;
        self.tmps += 1;
        format!("grf_tmp_{id}")
    }
}

fn int_suffix(ty: CanonicalType) -> &'static str {
    match ty {
        CanonicalType::Int16 => "i16",
        CanonicalType::Int32 => "i32",
        CanonicalType::Int64 => "i64",
        CanonicalType::UInt16 => "u16",
        CanonicalType::UInt32 => "u32",
        CanonicalType::UInt64 => "u64",
        _ => "",
    }
}

pub fn rust_ident(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

fn emit_int(value: i128, ty: CanonicalType) -> String {
    if value < 0 {
        format!("({value}i128 as {})", ty.rust_name())
    } else {
        format!("{value}{}", int_suffix(ty))
    }
}

fn emit_float(lexeme: &str, ty: CanonicalType) -> String {
    let suffix = if ty == CanonicalType::Flt32 {
        "f32"
    } else {
        "f64"
    };
    if let Some(rest) = lexeme.strip_prefix('-') {
        format!("(-{rest}{suffix})")
    } else {
        format!("{lexeme}{suffix}")
    }
}

fn emit_conv(
    op: ConvOp,
    from: CanonicalType,
    to: CanonicalType,
    inner: &str,
    site: &str,
) -> String {
    let from_name = rust_string(from.spelling());
    let to_name = rust_string(to.spelling());
    let rust = to.rust_name();
    let (signed_min, signed_max) = signed_bounds(to);
    let unsigned_max = unsigned_bound(to);
    match op {
        ConvOp::ToStr => match from {
            CanonicalType::Int16 | CanonicalType::Int32 | CanonicalType::Int64 => {
                format!("graphite_core::convert::signed_text(i128::from({inner}))")
            }
            CanonicalType::UInt16 | CanonicalType::UInt32 | CanonicalType::UInt64 => {
                format!("graphite_core::convert::unsigned_text(u128::from({inner}))")
            }
            CanonicalType::Flt32 => format!("graphite_core::convert::f32_text({inner})"),
            CanonicalType::Flt64 => format!("graphite_core::convert::f64_text({inner})"),
            CanonicalType::Bln => format!("graphite_core::convert::bln_text({inner})"),
            _ => "String::new()".to_string(),
        },
        ConvOp::CharToStr => format!("graphite_core::convert::char_to_str({inner})"),
        ConvOp::StrToSigned => format!(
            "(graphite_core::convert::str_to_signed(&({inner}), {signed_min}, {signed_max}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::StrToUnsigned => format!(
            "(graphite_core::convert::str_to_unsigned(&({inner}), {unsigned_max}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::StrToF64 => format!("graphite_core::convert::str_to_f64(&({inner}), {site})"),
        ConvOp::StrToF32 => format!("graphite_core::convert::str_to_f32(&({inner}), {site})"),
        ConvOp::StrToBool => format!("graphite_core::convert::str_to_bool(&({inner}), {site})"),
        ConvOp::StrToChar => format!("graphite_core::convert::str_to_char(&({inner}), {site})"),
        ConvOp::SignedToSigned => format!(
            "(graphite_core::convert::fit_signed(i128::from({inner}), {signed_min}, {signed_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::SignedToUnsigned => format!(
            "(graphite_core::convert::signed_to_unsigned(i128::from({inner}), {unsigned_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::UnsignedToSigned => format!(
            "(graphite_core::convert::unsigned_to_signed(u128::from({inner}), {signed_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::UnsignedToUnsigned => format!(
            "(graphite_core::convert::fit_unsigned(u128::from({inner}), {unsigned_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::SignedToF64 => {
            format!(
                "graphite_core::convert::signed_to_f64(i128::from({inner}), {from_name}, {site})"
            )
        }
        ConvOp::SignedToF32 => {
            format!(
                "graphite_core::convert::signed_to_f32(i128::from({inner}), {from_name}, {site})"
            )
        }
        ConvOp::UnsignedToF64 => format!(
            "graphite_core::convert::unsigned_to_f64(u128::from({inner}), {from_name}, {site})"
        ),
        ConvOp::UnsignedToF32 => format!(
            "graphite_core::convert::unsigned_to_f32(u128::from({inner}), {from_name}, {site})"
        ),
        ConvOp::F64ToF32 => {
            format!("graphite_core::convert::f64_to_f32({inner}, {from_name}, {to_name}, {site})")
        }
        ConvOp::F32ToF64 => format!("(f64::from({inner}))"),
        ConvOp::F64ToSigned => format!(
            "(graphite_core::convert::f64_to_signed({inner}, {signed_min}, {signed_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::F64ToUnsigned => format!(
            "(graphite_core::convert::f64_to_unsigned({inner}, {unsigned_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::F32ToSigned => format!(
            "(graphite_core::convert::f64_to_signed(f64::from({inner}), {signed_min}, {signed_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::F32ToUnsigned => format!(
            "(graphite_core::convert::f64_to_unsigned(f64::from({inner}), {unsigned_max}, {from_name}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::SignedToBool => {
            format!(
                "graphite_core::convert::signed_to_bool(i128::from({inner}), {from_name}, {site})"
            )
        }
        ConvOp::UnsignedToBool => format!(
            "graphite_core::convert::unsigned_to_bool(u128::from({inner}), {from_name}, {site})"
        ),
        ConvOp::F64ToBool => {
            format!("graphite_core::convert::f64_to_bool({inner}, {from_name}, {site})")
        }
        ConvOp::F32ToBool => {
            format!("graphite_core::convert::f64_to_bool(f64::from({inner}), {from_name}, {site})")
        }
        ConvOp::BoolToSigned | ConvOp::BoolToUnsigned => {
            format!(
                "(if {inner} {{ 1{rust_suffix} }} else {{ 0{rust_suffix} }})",
                rust_suffix = int_suffix(to)
            )
        }
        ConvOp::BoolToF64 => format!("(if {inner} {{ 1.0f64 }} else {{ 0.0f64 }})"),
        ConvOp::BoolToF32 => format!("(if {inner} {{ 1.0f32 }} else {{ 0.0f32 }})"),
        ConvOp::SignedToChar => {
            format!(
                "graphite_core::convert::code_to_char_signed(i128::from({inner}), {from_name}, {site})"
            )
        }
        ConvOp::UnsignedToChar => format!(
            "graphite_core::convert::code_to_char_unsigned(u128::from({inner}), {from_name}, {site})"
        ),
        ConvOp::F64ToChar => {
            format!("graphite_core::convert::code_to_char_f64({inner}, {from_name}, {site})")
        }
        ConvOp::F32ToChar => {
            format!(
                "graphite_core::convert::code_to_char_f64(f64::from({inner}), {from_name}, {site})"
            )
        }
        ConvOp::CharToSigned => format!(
            "(graphite_core::convert::char_to_signed({inner}, {signed_max}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::CharToUnsigned => format!(
            "(graphite_core::convert::char_to_unsigned({inner}, {unsigned_max}, {to_name}, {site}) as {rust})"
        ),
        ConvOp::CharToF64 => format!("graphite_core::convert::char_to_f64({inner})"),
        ConvOp::CharToF32 => format!("graphite_core::convert::char_to_f32({inner}, {site})"),
    }
}

fn signed_bounds(ty: CanonicalType) -> (i128, i128) {
    match ty {
        CanonicalType::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        CanonicalType::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        CanonicalType::UInt16 => (0, i128::from(u16::MAX)),
        CanonicalType::UInt32 => (0, i128::from(u32::MAX)),
        CanonicalType::UInt64 => (0, i128::from(u64::MAX)),
        _ => (i128::from(i64::MIN), i128::from(i64::MAX)),
    }
}

fn unsigned_bound(ty: CanonicalType) -> u128 {
    match ty {
        CanonicalType::UInt16 => u128::from(u16::MAX),
        CanonicalType::UInt32 => u128::from(u32::MAX),
        CanonicalType::Int16 => i16::MAX as u128,
        CanonicalType::Int32 => i32::MAX as u128,
        CanonicalType::Int64 => i64::MAX as u128,
        _ => u128::from(u64::MAX),
    }
}

fn rust_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            ch if ch.is_control() => out.push_str(&format!("\\u{{{:x}}}", u32::from(ch))),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

const PRELUDE: &str = r#"#![allow(dead_code, unused_macros)]

macro_rules! grf_add {
    ($a:expr, $b:expr) => {
        match $a.checked_add($b) {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}
macro_rules! grf_sub {
    ($a:expr, $b:expr) => {
        match $a.checked_sub($b) {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}
macro_rules! grf_mul {
    ($a:expr, $b:expr) => {
        match $a.checked_mul($b) {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}
macro_rules! grf_div {
    ($a:expr, $b:expr) => {
        match $a.checked_div($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    grf_div_zero()
                } else {
                    grf_overflow()
                }
            }
        }
    };
}
macro_rules! grf_rem {
    ($a:expr, $b:expr) => {
        match $a.checked_rem($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    grf_div_zero()
                } else {
                    grf_overflow()
                }
            }
        }
    };
}
macro_rules! grf_neg {
    ($a:expr) => {
        match $a.checked_neg() {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}

pub(crate) fn grf_overflow() -> ! {
    eprintln!("graphite: integer overflow");
    std::process::exit(1);
}

pub(crate) fn grf_div_zero() -> ! {
    eprintln!("graphite: division by zero");
    std::process::exit(1);
}

pub(crate) fn grf_str_length(value: &str) -> u64 {
    value.chars().count() as u64
}

"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_path_stays_workspace_relative() {
        let path = runtime_crate_relative_path();
        assert!(path.is_relative());
        assert_eq!(path, Path::new("crates/graphite-core"));
    }
}
