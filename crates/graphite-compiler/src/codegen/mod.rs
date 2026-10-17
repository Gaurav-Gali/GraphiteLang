//! Rust code generation.
//!
//! Generation reads a checked program. It does not rewrite Graphite source
//! text. Library calls use the Rust path stored on each call, which semantic
//! analysis copied from `grf.json`. Integer arithmetic is checked and aborts
//! on overflow instead of wrapping. That abort is a guard until the overflow
//! policy is frozen.

use std::path::{Path, PathBuf};

use crate::parser::{BinaryOperator, UnaryOperator};
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
    for function in &program.functions {
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
        let params = function
            .params
            .iter()
            .map(|(name, ty)| format!("mut {}: {}", rust_ident(name), ty.rust_name()))
            .collect::<Vec<_>>()
            .join(", ");
        if function.ret == CanonicalType::Void {
            self.line(&format!("fn {name}({params}) {{"));
        } else {
            self.line(&format!(
                "fn {name}({params}) -> {} {{",
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
            CheckedStmt::Let { name, ty, value } => {
                let value = self.emit_expr(value);
                self.line(&format!(
                    "let mut {}: {} = {value};",
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
            CheckedExpr::Int { value, ty } => format!("{value}{}", int_suffix(*ty)),
            CheckedExpr::Bool(value) => value.to_string(),
            CheckedExpr::Str(value) => format!("String::from({})", rust_string(value)),
            CheckedExpr::Load { name, ty } => {
                let name = rust_ident(name);
                if *ty == CanonicalType::Str {
                    format!("{name}.clone()")
                } else {
                    name
                }
            }
            CheckedExpr::Unary { op, expr, .. } => {
                let inner = self.emit_expr(expr);
                match op {
                    UnaryOperator::Not => format!("(!{inner})"),
                    UnaryOperator::Negate => format!("grf_neg!({inner})"),
                }
            }
            CheckedExpr::Binary {
                op, left, right, ..
            } => {
                let left = self.emit_expr(left);
                let right = self.emit_expr(right);
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
            CheckedExpr::Call { path, args, .. } => {
                let path = if path.contains("::") {
                    path.clone()
                } else {
                    rust_ident(path)
                };
                let args = args
                    .iter()
                    .map(|arg| self.emit_expr(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{path}({args})")
            }
            CheckedExpr::ConvertStr { from, expr } => {
                let inner = self.emit_expr(expr);
                match from {
                    CanonicalType::Int16 | CanonicalType::Int32 | CanonicalType::Int64 => {
                        format!("grf_signed_text(i128::from({inner}))")
                    }
                    CanonicalType::UInt16 | CanonicalType::UInt32 | CanonicalType::UInt64 => {
                        format!("grf_unsigned_text(u128::from({inner}))")
                    }
                    CanonicalType::Bln => format!("grf_bln_text({inner})"),
                    CanonicalType::Str => inner,
                    _ => "String::new()".to_string(),
                }
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

fn grf_overflow() -> ! {
    eprintln!("graphite: integer overflow");
    std::process::exit(1);
}

fn grf_div_zero() -> ! {
    eprintln!("graphite: division by zero");
    std::process::exit(1);
}

fn grf_signed_text(value: i128) -> String {
    let negative = value < 0;
    let mut digits = value.unsigned_abs();
    if digits == 0 {
        return "0".to_string();
    }
    let mut text = Vec::new();
    while digits > 0 {
        text.push(b'0' + (digits % 10) as u8);
        digits /= 10;
    }
    if negative {
        text.push(b'-');
    }
    text.reverse();
    String::from_utf8(text).unwrap()
}

fn grf_unsigned_text(value: u128) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut digits = value;
    let mut text = Vec::new();
    while digits > 0 {
        text.push(b'0' + (digits % 10) as u8);
        digits /= 10;
    }
    text.reverse();
    String::from_utf8(text).unwrap()
}

fn grf_bln_text(value: bool) -> String {
    if value {
        "true".to_string()
    } else {
        "false".to_string()
    }
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
