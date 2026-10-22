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
use crate::semantic::numeric::CompareMode;
use crate::semantic::{
    ArrayMethod, ArrayPlace, CanonicalType, CheckedClass, CheckedExpr, CheckedFn, CheckedPart,
    CheckedProgram, CheckedRecord, CheckedStmt, ClassId, Leaf, LoopSourceKind, MapEntry, MapMethod,
    MapShape, RecordId,
};

pub fn runtime_crate_relative_path() -> PathBuf {
    PathBuf::from(Path::new("crates/graphite-core"))
}

pub fn generate(program: &CheckedProgram) -> String {
    generate_with_source(program, "", "")
}

pub fn generate_with_source(program: &CheckedProgram, source_name: &str, source: &str) -> String {
    let mut emitter = Emitter {
        out: String::from(PRELUDE),
        indent: 0,
        loops: Vec::new(),
        tries: Vec::new(),
        fn_ret: "()".to_string(),
        pending_ids: 0,
        tmps: 0,
        loop_ids: 0,
        records: &program.records,
        classes: &program.classes,
        maps: &program.maps,
        source_name,
        source,
        constructor_return: false,
    };
    for index in record_order(&program.records) {
        emitter.emit_record(&program.records[index]);
        emitter.out.push('\n');
    }
    for class in &program.classes {
        emitter.emit_class_struct(class);
        emitter.out.push('\n');
    }
    for class in &program.classes {
        emitter.emit_class_behavior(class);
    }
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

struct TryFrame {
    pending: String,
    loop_depth: usize,
}

struct Emitter<'a> {
    out: String,
    indent: usize,
    loops: Vec<(String, String)>,
    tries: Vec<TryFrame>,
    fn_ret: String,
    pending_ids: u32,
    tmps: u32,
    loop_ids: u32,
    records: &'a [CheckedRecord],
    classes: &'a [CheckedClass],
    maps: &'a [MapShape],
    source_name: &'a str,
    source: &'a str,
    constructor_return: bool,
}

impl Emitter<'_> {
    fn emit_function(&mut self, function: &CheckedFn) {
        let name = rust_ident(&function.name);
        self.constructor_return = false;
        let visibility = if function.name == "main" {
            ""
        } else {
            "pub(crate) "
        };
        let params = function
            .params
            .iter()
            .map(|(name, ty)| format!("mut {}: {}", rust_ident(name), self.ty_name(*ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let rust_name = if function.name == "main" {
            "grf_user_main"
        } else {
            name.as_str()
        };
        let ret = if function.ret == CanonicalType::Void {
            "()".to_string()
        } else {
            self.ty_name(function.ret)
        };
        self.fn_ret = ret.clone();
        self.tries.clear();
        self.line(&format!(
            "{visibility}fn {rust_name}({params}) -> Result<{ret}, graphite_core::diagnostic::RuntimeError> {{"
        ));
        self.indent += 1;
        for statement in &function.body {
            self.emit_stmt(statement);
        }
        if function.ret == CanonicalType::Void {
            self.line("Ok(())");
        } else {
            self.line("unreachable!();");
        }
        self.indent -= 1;
        self.line("}");
        self.out.push('\n');
        if function.name == "main" {
            self.line("fn main() {");
            self.indent += 1;
            self.line("if let Err(error) = grf_user_main() {");
            self.indent += 1;
            self.line("graphite_core::diagnostic::report(&error);");
            self.line("std::process::exit(1);");
            self.indent -= 1;
            self.line("}");
            self.indent -= 1;
            self.line("}");
            self.out.push('\n');
        }
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
                if !*mutable && self.contains_class(*ty) {
                    let temp = self.fresh_tmp();
                    self.line(&format!("let {temp} = {value};"));
                    let freeze = self.freeze_block(*ty, &temp);
                    if !freeze.is_empty() {
                        self.line(&freeze);
                    }
                    self.line(&format!(
                        "{binding} {}: {} = {temp};",
                        rust_ident(name),
                        self.ty_name(*ty)
                    ));
                } else {
                    self.line(&format!(
                        "{binding} {}: {} = {value};",
                        rust_ident(name),
                        self.ty_name(*ty)
                    ));
                }
            }
            CheckedStmt::Return(value) => self.emit_return(value),
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
                init_expr,
                condition,
                update,
                body,
            } => {
                self.line("{");
                self.indent += 1;
                if let Some(initializer) = initializer {
                    self.emit_stmt(initializer);
                }
                if let Some(init_expr) = init_expr {
                    let init_expr = self.emit_expr(init_expr);
                    self.line(&format!("{init_expr};"));
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
            CheckedStmt::Break => self.emit_break(),
            CheckedStmt::Continue => self.emit_continue(),
            CheckedStmt::Block { body } => {
                self.line("{");
                self.emit_body(body);
                self.line("}");
            }
            CheckedStmt::Destructure {
                bindings,
                value,
                file,
                line,
                column,
                end_column,
            } => {
                let value = self.emit_expr(value);
                let site = self.locate(file, *line, *column, *end_column);
                let values = self.fresh_tmp();
                self.line(&format!("let {values} = {value};"));
                self.line(&format!(
                    "graphite_core::array::expect_len({values}.len(), {}, {site})?;",
                    bindings.len()
                ));
                for (index, (name, ty, mutable)) in bindings.iter().enumerate() {
                    let binding = if *mutable { "let mut" } else { "let" };
                    self.line(&format!(
                        "{binding} {}: {} = {values}[{index}].clone();",
                        rust_ident(name),
                        self.ty_name(*ty)
                    ));
                }
            }
            CheckedStmt::Iterate {
                bindings,
                source,
                kind,
                body,
            } => {
                let outer = self.fresh_label();
                let inner = self.fresh_label();
                self.line("{");
                self.indent += 1;
                if let CheckedExpr::Load { name, .. } = source {
                    self.line(&format!("let grf_src = &{};", rust_ident(name)));
                } else {
                    let source = self.emit_expr(source);
                    self.line(&format!("let grf_owned = {source};"));
                    self.line("let grf_src = &grf_owned;");
                }
                self.line("let mut grf_i: u64 = 0;");
                self.line(&format!(
                    "'{outer}: while (grf_i as usize) < grf_src.len() {{"
                ));
                self.indent += 1;
                self.line(&format!("'{inner}: loop {{"));
                self.indent += 1;
                self.loops.push((outer.clone(), inner.clone()));
                if *kind == LoopSourceKind::Enumerate {
                    if let Some((name, ty, mutable)) = bindings.first() {
                        let binding = if *mutable { "let mut" } else { "let" };
                        self.line(&format!(
                            "{binding} {}: {ty} = grf_i;",
                            rust_ident(name),
                            ty = self.ty_name(*ty)
                        ));
                    }
                    if let Some((name, ty, mutable)) = bindings.get(1) {
                        let binding = if *mutable { "let mut" } else { "let" };
                        self.line(&format!(
                            "{binding} {}: {} = grf_src[grf_i as usize].clone();",
                            rust_ident(name),
                            self.ty_name(*ty)
                        ));
                    }
                } else if *kind == LoopSourceKind::Entries {
                    let pair = self.fresh_tmp();
                    self.line(&format!("let {pair} = grf_src[grf_i as usize].clone();"));
                    if let Some((name, ty, mutable)) = bindings.first() {
                        let binding = if *mutable { "let mut" } else { "let" };
                        self.line(&format!(
                            "{binding} {}: {} = {pair}.0;",
                            rust_ident(name),
                            self.ty_name(*ty)
                        ));
                    }
                    if let Some((name, ty, mutable)) = bindings.get(1) {
                        let binding = if *mutable { "let mut" } else { "let" };
                        self.line(&format!(
                            "{binding} {}: {} = {pair}.1;",
                            rust_ident(name),
                            self.ty_name(*ty)
                        ));
                    }
                } else if let Some((name, ty, mutable)) = bindings.first() {
                    let binding = if *mutable { "let mut" } else { "let" };
                    self.line(&format!(
                        "{binding} {}: {} = grf_src[grf_i as usize].clone();",
                        rust_ident(name),
                        self.ty_name(*ty)
                    ));
                }
                self.emit_body(body);
                self.loops.pop();
                self.line(&format!("break '{inner};"));
                self.indent -= 1;
                self.line("}");
                self.line("grf_i += 1;");
                self.indent -= 1;
                self.line("}");
                self.indent -= 1;
                self.line("}");
            }
            CheckedStmt::Try {
                body,
                catches,
                finally_body,
            } => self.emit_try(body, catches, finally_body),
        }
    }

    fn emit_return(&mut self, value: &Option<CheckedExpr>) {
        let constructor = self.constructor_return && value.is_none();
        if let Some(pending) = self.tries.last().map(|frame| frame.pending.clone()) {
            if constructor {
                self.line(&format!("{pending} = GrfExit::Return(this.clone());"));
            } else if let Some(value) = value {
                let value = self.emit_expr(value);
                self.line(&format!("{pending} = GrfExit::Return({value});"));
            } else {
                self.line(&format!("{pending} = GrfExit::Return(());"));
            }
            self.line("return Ok(());");
            return;
        }
        if constructor {
            self.line("return Ok(this.clone());");
            return;
        }
        match value {
            None => self.line("return Ok(());"),
            Some(value) => {
                let value = self.emit_expr(value);
                self.line(&format!("return Ok({value});"));
            }
        }
    }

    fn emit_break(&mut self) {
        let Some((label, _)) = self.loops.last().cloned() else {
            return;
        };
        if self.exit_try() {
            let pending = self.tries.last().unwrap().pending.clone();
            self.line(&format!("{pending} = GrfExit::Break(\"{label}\");"));
            self.line("return Ok(());");
            return;
        }
        self.line(&format!("break '{label};"));
    }

    fn emit_continue(&mut self) {
        let Some((break_label, continue_label)) = self.loops.last().cloned() else {
            return;
        };
        let inner = break_label != continue_label;
        if self.exit_try() {
            let pending = self.tries.last().unwrap().pending.clone();
            if inner {
                self.line(&format!(
                    "{pending} = GrfExit::ContinueInner(\"{continue_label}\");"
                ));
            } else {
                self.line(&format!(
                    "{pending} = GrfExit::Continue(\"{continue_label}\");"
                ));
            }
            self.line("return Ok(());");
            return;
        }
        if inner {
            self.line(&format!("break '{continue_label};"));
        } else {
            self.line(&format!("continue '{continue_label};"));
        }
    }

    fn exit_try(&self) -> bool {
        self.tries
            .last()
            .is_some_and(|frame| self.loops.len() <= frame.loop_depth)
    }

    fn emit_try(
        &mut self,
        body: &[CheckedStmt],
        catches: &[crate::semantic::CheckedCatch],
        finally_body: &Option<Vec<CheckedStmt>>,
    ) {
        let id = self.pending_ids;
        self.pending_ids += 1;
        let pending = format!("grf_pending_{id}");
        let ret = self.fn_ret.clone();
        self.line(&format!(
            "let mut {pending}: GrfExit<{ret}> = GrfExit::Fall;"
        ));
        self.tries.push(TryFrame {
            pending: pending.clone(),
            loop_depth: self.loops.len(),
        });
        self.line("let grf_body = (|| -> Result<(), graphite_core::diagnostic::RuntimeError> {");
        self.indent += 1;
        for statement in body {
            self.emit_stmt(statement);
        }
        self.line("Ok(())");
        self.indent -= 1;
        self.line("})();");
        self.line(&format!(
            "if let Err(error) = grf_body {{ {pending} = GrfExit::Error(error); }}"
        ));
        for (index, clause) in catches.iter().enumerate() {
            let keyword = if index == 0 { "if" } else { "else if" };
            let condition = if clause.kind == "RuntimeError" || clause.kind == "Error" {
                format!("matches!(&{pending}, GrfExit::Error(_))")
            } else {
                format!(
                    "matches!(&{pending}, GrfExit::Error(error) if error.kind_name() == \"{}\")",
                    clause.kind
                )
            };
            self.line(&format!("{keyword} {condition} {{"));
            self.indent += 1;
            let binding = rust_ident(&clause.binding);
            self.line(&format!(
                "let GrfExit::Error({binding}) = std::mem::replace(&mut {pending}, GrfExit::Fall) else {{ unreachable!() }};"
            ));
            self.line(
                "let grf_catch = (|| -> Result<(), graphite_core::diagnostic::RuntimeError> {",
            );
            self.indent += 1;
            for statement in &clause.body {
                self.emit_stmt(statement);
            }
            self.line("Ok(())");
            self.indent -= 1;
            self.line("})();");
            self.line(&format!(
                "if let Err(error) = grf_catch {{ {pending} = GrfExit::Error(error); }}"
            ));
            self.indent -= 1;
            self.line("}");
        }
        if let Some(finally_body) = finally_body {
            self.line(
                "let grf_finally = (|| -> Result<(), graphite_core::diagnostic::RuntimeError> {",
            );
            self.indent += 1;
            for statement in finally_body {
                self.emit_stmt(statement);
            }
            self.line("Ok(())");
            self.indent -= 1;
            self.line("})();");
            self.line("if let Err(error) = grf_finally {");
            self.indent += 1;
            self.line(&format!(
                "{pending} = match std::mem::replace(&mut {pending}, GrfExit::Fall) {{"
            ));
            self.indent += 1;
            self.line("GrfExit::Error(old) => GrfExit::Error(error.with_cause(old)),");
            self.line("_ => GrfExit::Error(error),");
            self.indent -= 1;
            self.line("};");
            self.indent -= 1;
            self.line("}");
        }
        self.tries.pop();
        self.emit_resume(&pending);
    }

    fn emit_resume(&mut self, pending: &str) {
        if let Some(outer) = self.tries.last().map(|frame| frame.pending.clone()) {
            self.line(&format!(
                "match std::mem::replace(&mut {pending}, GrfExit::Fall) {{"
            ));
            self.indent += 1;
            self.line("GrfExit::Fall => {}");
            self.line(&format!("other => {{ {outer} = other; return Ok(()); }}"));
            self.indent -= 1;
            self.line("};");
            return;
        }
        let breaks = self
            .loops
            .iter()
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>();
        let continues = self
            .loops
            .iter()
            .filter(|(break_label, continue_label)| break_label == continue_label)
            .map(|(_, label)| label.clone())
            .collect::<Vec<_>>();
        let inners = self
            .loops
            .iter()
            .filter(|(break_label, continue_label)| break_label != continue_label)
            .map(|(_, label)| label.clone())
            .collect::<Vec<_>>();
        self.line(&format!(
            "match std::mem::replace(&mut {pending}, GrfExit::Fall) {{"
        ));
        self.indent += 1;
        self.line("GrfExit::Fall => {}");
        if self.fn_ret == "()" {
            self.line("GrfExit::Return(_) => return Ok(()),");
        } else {
            self.line("GrfExit::Return(value) => return Ok(value),");
        }
        self.line("GrfExit::Error(error) => return Err(error),");
        self.line("GrfExit::Break(label) => {");
        self.indent += 1;
        self.emit_label_dispatch("break", &breaks);
        self.indent -= 1;
        self.line("}");
        self.line("GrfExit::Continue(label) => {");
        self.indent += 1;
        self.emit_label_dispatch("continue", &continues);
        self.indent -= 1;
        self.line("}");
        self.line("GrfExit::ContinueInner(label) => {");
        self.indent += 1;
        self.emit_label_dispatch("break", &inners);
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("};");
    }

    fn emit_label_dispatch(&mut self, keyword: &str, labels: &[String]) {
        if labels.is_empty() {
            self.line("let _ = label;");
            return;
        }
        for (index, label) in labels.iter().enumerate() {
            let word = if index == 0 { "if" } else { "else if" };
            self.line(&format!(
                "{word} label == \"{label}\" {{ {keyword} '{label}; }}"
            ));
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
            CheckedExpr::Null { ty } => {
                let inner = ty.optional_inner().unwrap_or(*ty);
                format!("None::<{}>", self.ty_name(inner))
            }
            CheckedExpr::Wrap { expr, .. } => format!("Some({})", self.emit_expr(expr)),
            CheckedExpr::Unready { .. } => "Default::default()".to_string(),
            CheckedExpr::Unwrap {
                expr,
                file,
                line,
                column,
                end_column,
                ..
            } => {
                let value = self.emit_expr(expr);
                let site = self.locate(file, *line, *column, *end_column);
                format!(
                    "match {value} {{ Some(grf_value) => grf_value, None => return Err(graphite_core::diagnostic::null_access({site})) }}"
                )
            }
            CheckedExpr::Load { name, ty } => {
                let name = rust_ident(name);
                if ty.is_heap() {
                    format!("{name}.clone()")
                } else {
                    name
                }
            }
            CheckedExpr::Unary { op, expr, ty, span } => {
                let inner = self.emit_expr(expr);
                match op {
                    UnaryOperator::Not => format!("(!{inner})"),
                    UnaryOperator::Negate if ty.is_float() => format!("(-({inner}))"),
                    UnaryOperator::Negate if ty.is_unsigned() => {
                        let zero = match ty {
                            CanonicalType::UInt16 => "0u16",
                            CanonicalType::UInt32 => "0u32",
                            _ => "0u64",
                        };
                        let site = self.locate_span(*span);
                        format!(
                            "{{ if u128::from({inner}) != 0 {{ return Err(graphite_core::diagnostic::integer_overflow({site})); }} else {{ {zero} }} }}"
                        )
                    }
                    UnaryOperator::Negate => {
                        let site = self.locate_span(*span);
                        format!("grf_neg!({inner}, {site})")
                    }
                }
            }
            CheckedExpr::Binary {
                op,
                left,
                right,
                compare,
                span,
                ..
            } => {
                if let Some(mode) = compare {
                    return self.emit_compare(*op, left, right, *mode);
                }
                let float_operand = left.ty().is_float();
                let left = self.emit_expr(left);
                let right = self.emit_expr(right);
                let site = self.locate_span(*span);
                if float_operand {
                    return match op {
                        BinaryOperator::Add => format!("({left} + {right})"),
                        BinaryOperator::Subtract => format!("({left} - {right})"),
                        BinaryOperator::Multiply => format!("({left} * {right})"),
                        BinaryOperator::Divide => format!("({left} / {right})"),
                        BinaryOperator::Power => format!("({left}.powf({right}))"),
                        BinaryOperator::FloorDiv => format!("(({left} / {right}).floor())"),
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
                    BinaryOperator::Add => format!("grf_add!({left}, {right}, {site})"),
                    BinaryOperator::Subtract => format!("grf_sub!({left}, {right}, {site})"),
                    BinaryOperator::Multiply => format!("grf_mul!({left}, {right}, {site})"),
                    BinaryOperator::Divide => format!("grf_div!({left}, {right}, {site})"),
                    BinaryOperator::Power => format!("grf_pow!({left}, {right}, {site})"),
                    BinaryOperator::FloorDiv => format!("grf_floordiv!({left}, {right}, {site})"),
                    BinaryOperator::Remainder => format!("grf_rem!({left}, {right}, {site})"),
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
                if ty.is_heap() {
                    format!("{{ {name} = {value}; {name}.clone() }}")
                } else {
                    format!("{{ {name} = {value}; {name} }}")
                }
            }
            CheckedExpr::Update {
                prefix,
                increment,
                name,
                span,
                ..
            } => {
                let name = rust_ident(name);
                let op = if *increment { "grf_add" } else { "grf_sub" };
                let site = self.locate_span(*span);
                if *prefix {
                    format!("{{ {name} = {op}!({name}, 1, {site}); {name} }}")
                } else {
                    let tmp = self.fresh_tmp();
                    format!("{{ let {tmp} = {name}; {name} = {op}!({name}, 1, {site}); {tmp} }}")
                }
            }
            CheckedExpr::TypeOf { expr, name, record } => {
                let inner = self.emit_expr(expr);
                format!(
                    "{{ let _ = {inner}; GrfType {{ name: {}, record: {record} }} }}",
                    rust_string(name)
                )
            }
            CheckedExpr::TypeName { object } => {
                let inner = self.emit_expr(object);
                format!("String::from(({inner}).name)")
            }
            CheckedExpr::LengthOf { expr, count } => {
                if let Some(count) = count {
                    let inner = self.emit_expr(expr);
                    format!("{{ let _ = {inner}; {count}u64 }}")
                } else if expr.ty().is_array() {
                    let inner = self.emit_array_ref(expr);
                    format!("(({inner}).len() as u64)")
                } else if expr.ty().is_map() {
                    let inner = self.emit_map_ref(expr);
                    format!("(({inner}).len() as u64)")
                } else {
                    let inner = self.emit_expr(expr);
                    format!("grf_str_length(&({inner}))")
                }
            }
            CheckedExpr::Record { id, fields, .. } => self.emit_record_literal(*id, fields),
            CheckedExpr::MapLiteral { entries, ty } => self.emit_map_literal(entries, *ty),
            CheckedExpr::MapGet {
                object,
                key,
                type_name,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_map_get(object, key, type_name, file, *line, *column, *end_column),
            CheckedExpr::MapInsert {
                place,
                key,
                value,
                file,
                line,
                column,
                ..
            } => self.emit_map_insert(place, key, value, file, *line, *column),
            CheckedExpr::MapCall {
                place,
                object,
                method,
                args,
                key_ty,
                value_ty,
                type_name,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_map_call(
                place,
                object,
                *method,
                args,
                *key_ty,
                *value_ty,
                type_name,
                file,
                *line,
                *column,
                *end_column,
            ),
            CheckedExpr::FieldGet {
                object,
                record,
                field,
                ty,
            } => {
                let object = self.emit_expr(object);
                let field = self.field_name(*record, *field);
                if ty.is_heap() {
                    format!("({object}).{field}.clone()")
                } else {
                    format!("({object}).{field}")
                }
            }
            CheckedExpr::FieldSet { place, value, ty } => {
                let value = self.emit_expr(value);
                let (setup, container, commit) = self.emit_container(place);
                if ty.is_heap() {
                    format!(
                        "{{ let grf_stored = {value}; {setup} {container} = grf_stored.clone(); {commit} grf_stored }}"
                    )
                } else {
                    format!(
                        "{{ let grf_stored = {value}; {setup} {container} = grf_stored; {commit} grf_stored }}"
                    )
                }
            }
            CheckedExpr::Array { elements, ty } => {
                if elements.is_empty() {
                    format!("{}::new()", vec_turbofish(&self.ty_name(*ty)))
                } else {
                    let elements = elements
                        .iter()
                        .map(|element| self.emit_expr(element))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("vec![{elements}]")
                }
            }
            CheckedExpr::IndexGet {
                object,
                index,
                file,
                line,
                column,
                end_column,
                ..
            } => {
                let object = self.emit_array_ref(object);
                let site = self.locate(file, *line, *column, *end_column);
                self.emit_read(&object, index, &site)
            }
            CheckedExpr::SliceGet {
                object,
                start,
                stop,
                step,
                file,
                line,
                column,
                end_column,
                ..
            } => {
                let object = self.emit_array_ref(object);
                let site = self.locate(file, *line, *column, *end_column);
                format!(
                    "graphite_core::array::slice_copy({object}, {}, {}, {}, {site})?",
                    self.emit_bound(start),
                    self.emit_bound(stop),
                    self.emit_bound(step)
                )
            }
            CheckedExpr::IndexSet {
                place,
                object,
                index,
                value,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_index_set(
                place,
                object,
                index,
                value,
                file,
                *line,
                *column,
                *end_column,
            ),
            CheckedExpr::SliceSet {
                place,
                object,
                start,
                stop,
                step,
                value,
                limit,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_slice_set(
                place,
                object,
                start,
                stop,
                step,
                value,
                *limit,
                file,
                *line,
                *column,
                *end_column,
            ),
            CheckedExpr::ArrayCall {
                place,
                object,
                method,
                args,
                limit,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_array_call(
                place,
                object,
                *method,
                args,
                *limit,
                file,
                *line,
                *column,
                *end_column,
            ),
            CheckedExpr::Call {
                path,
                args,
                callee,
                file,
                line,
                column,
                end_column,
                ..
            } => {
                let path = if path.contains("::") {
                    path.clone()
                } else if path == "main" {
                    "crate::grf_user_main".to_string()
                } else {
                    format!("crate::grf_fns::{}", rust_ident(path))
                };
                let args = args
                    .iter()
                    .map(|arg| self.emit_expr(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                let site = self.locate(file, *line, *column, *end_column);
                format!(
                    "match {path}({args}) {{ Ok(value) => value, Err(error) => return Err(error.enter_call(\"{callee}\", {site})) }}"
                )
            }
            CheckedExpr::HandleCall { name, method, .. } => {
                let name = rust_ident(name);
                match method.as_str() {
                    "message" => format!("{name}.message_text()"),
                    "code" => format!("String::from({name}.code)"),
                    "kind" => format!("String::from({name}.kind_name())"),
                    "sourceFile" => format!("{name}.source_file()"),
                    "line" => format!("{name}.line_number()"),
                    "column" => format!("{name}.column_number()"),
                    "causeMessage" => format!("{name}.cause_message()"),
                    "stackTrace" => format!("{name}.stack_trace()"),
                    _ => "String::new()".to_string(),
                }
            }
            CheckedExpr::NumCast {
                from,
                to,
                check,
                expr,
                file,
                line,
                column,
                end_column,
            } => {
                let inner = self.emit_expr(expr);
                let site = self.locate(file, *line, *column, *end_column);
                emit_num_cast(*from, *to, *check, &inner, &site)
            }
            CheckedExpr::Convert {
                from,
                to,
                op,
                expr,
                file,
                line,
                column,
                end_column,
            } => {
                let inner = self.emit_expr(expr);
                let site = self.locate(file, *line, *column, *end_column);
                match op {
                    ConvOp::RecordToStr => self.record_to_str(*from, &inner),
                    ConvOp::ArrayToStr => self.array_to_str(*from, &inner),
                    ConvOp::MapToStr => self.map_to_str(*from, &inner),
                    ConvOp::TypeToStr => type_to_str(&inner),
                    ConvOp::OptionalToStr => {
                        let inner_ty = from.optional_inner().unwrap_or(*from);
                        let rendered = if inner_ty == CanonicalType::Str {
                            "grf_inner.clone()".to_string()
                        } else if inner_ty.is_record() {
                            self.record_to_str(inner_ty, "grf_inner")
                        } else if inner_ty.is_array() {
                            self.array_to_str(inner_ty, "grf_inner")
                        } else if inner_ty.is_map() {
                            self.map_to_str(inner_ty, "grf_inner")
                        } else if inner_ty == CanonicalType::Type {
                            type_to_str("grf_inner")
                        } else if inner_ty == CanonicalType::Chr {
                            emit_conv(
                                ConvOp::CharToStr,
                                inner_ty,
                                CanonicalType::Str,
                                "grf_inner",
                                &site,
                            )
                        } else {
                            emit_conv(
                                ConvOp::ToStr,
                                inner_ty,
                                CanonicalType::Str,
                                "grf_inner",
                                &site,
                            )
                        };
                        format!(
                            "{{ let grf_opt = {inner}; match grf_opt {{ None => String::from(\"null\"), Some(grf_inner) => {rendered} }} }}"
                        )
                    }
                    other => emit_conv(*other, *from, *to, &inner, &site),
                }
            }
            CheckedExpr::ClassNew {
                class,
                args,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_class_new(*class, args, file, *line, *column, *end_column),
            CheckedExpr::ClassGet {
                object,
                class,
                field,
                ..
            } => self.emit_class_get(object, *class, *field),
            CheckedExpr::ClassSet {
                object,
                class,
                field,
                value,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_class_set(
                object,
                *class,
                *field,
                value,
                file,
                *line,
                *column,
                *end_column,
            ),
            CheckedExpr::ClassCall {
                class,
                method,
                object,
                args,
                file,
                line,
                column,
                end_column,
                ..
            } => self.emit_class_call(
                *class,
                *method,
                object,
                args,
                file,
                *line,
                *column,
                *end_column,
            ),
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

    fn emit_compare(
        &mut self,
        op: BinaryOperator,
        left: &CheckedExpr,
        right: &CheckedExpr,
        mode: CompareMode,
    ) -> String {
        if mode == CompareMode::Null {
            let value = if matches!(left, CheckedExpr::Null { .. }) {
                right
            } else {
                left
            };
            let value = self.emit_expr(value);
            let test = format!("{value}.is_none()");
            return if op == BinaryOperator::NotEqual {
                format!("(!{test})")
            } else {
                test
            };
        }
        if mode == CompareMode::Optional {
            let left = self.emit_expr(left);
            let right = self.emit_expr(right);
            let equal = format!(
                "{{ let grf_left = {left}; let grf_right = {right}; match (&grf_left, &grf_right) {{ (None, None) => true, (Some(grf_a), Some(grf_b)) => grf_a == grf_b, _ => false }} }}"
            );
            return if op == BinaryOperator::NotEqual {
                format!("(!{equal})")
            } else {
                equal
            };
        }
        let (left_text, right_text) = match mode {
            CompareMode::Same => (self.emit_expr(left), self.emit_expr(right)),
            CompareMode::Integers => (self.emit_i128(left), self.emit_i128(right)),
            CompareMode::Floats => (self.emit_f64(left), self.emit_f64(right)),
            CompareMode::Null | CompareMode::Optional => unreachable!("handled above"),
            CompareMode::IntFloat => {
                let int_on_left = !left.ty().is_float();
                let (integer, float) = if int_on_left {
                    (left, right)
                } else {
                    (right, left)
                };
                let order = format!(
                    "graphite_core::numeric::cmp_int_f64({}, {})",
                    self.emit_i128(integer),
                    self.emit_f64(float)
                );
                return int_float_op(op, &order, int_on_left);
            }
        };
        match op {
            BinaryOperator::Less => format!("({left_text} < {right_text})"),
            BinaryOperator::LessEqual => format!("({left_text} <= {right_text})"),
            BinaryOperator::Greater => format!("({left_text} > {right_text})"),
            BinaryOperator::GreaterEqual => format!("({left_text} >= {right_text})"),
            BinaryOperator::Equal => format!("({left_text} == {right_text})"),
            BinaryOperator::NotEqual => format!("({left_text} != {right_text})"),
            _ => "false".to_string(),
        }
    }

    fn emit_f64(&mut self, expr: &CheckedExpr) -> String {
        let inner = self.emit_expr(expr);
        if expr.ty() == CanonicalType::Flt32 {
            format!("f64::from({inner})")
        } else {
            inner
        }
    }

    fn emit_array_ref(&mut self, expr: &CheckedExpr) -> String {
        if let CheckedExpr::Load { name, ty } = expr {
            if ty.is_array() {
                return format!("&{}", rust_ident(name));
            }
        }
        format!("&({})", self.emit_expr(expr))
    }

    fn emit_map_ref(&mut self, expr: &CheckedExpr) -> String {
        if let CheckedExpr::Load { name, ty } = expr {
            if ty.is_map() {
                return format!("&{}", rust_ident(name));
            }
        }
        format!("&({})", self.emit_expr(expr))
    }

    fn emit_map_literal(&mut self, entries: &[MapEntry], ty: CanonicalType) -> String {
        let CanonicalType::Map(id) = ty else {
            return "std::collections::HashMap::new()".to_string();
        };
        let shape = self.maps.get(id.0 as usize).cloned().unwrap_or(MapShape {
            key: CanonicalType::Void,
            value: CanonicalType::Void,
        });
        let map_ty = format!(
            "std::collections::HashMap::<{}, {}>",
            self.ty_name(shape.key),
            self.ty_name(shape.value)
        );
        let mut body = format!("let mut grf_map = {map_ty}::new();");
        for entry in entries {
            match entry {
                MapEntry::Pair(key, value) => {
                    let key = self.emit_expr(key);
                    let value = self.emit_expr(value);
                    body.push_str(&format!(
                        "let grf_key = {key}; let grf_val = {value}; grf_map.insert(grf_key, grf_val);"
                    ));
                }
                MapEntry::Spread(source) => {
                    let source = self.emit_expr(source);
                    let source_tmp = self.fresh_tmp();
                    body.push_str(&format!(
                        "let {source_tmp} = {source}; for (grf_key, grf_val) in {source_tmp}.iter() {{ grf_map.insert(grf_key.clone(), grf_val.clone()); }}"
                    ));
                }
            }
        }
        format!("{{ {body} grf_map }}")
    }

    fn emit_map_get(
        &mut self,
        object: &CheckedExpr,
        key: &CheckedExpr,
        type_name: &str,
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let map_ref = self.emit_map_ref(object);
        let key = self.emit_expr(key);
        let site = self.locate(file, line, column, end_column);
        let map_type = rust_string(type_name);
        format!(
            "{{ let grf_map = {map_ref}; let grf_key = {key}; graphite_core::map::get(grf_map, &grf_key, {site}, {map_type})? }}"
        )
    }

    fn emit_map_insert(
        &mut self,
        place: &ArrayPlace,
        key: &CheckedExpr,
        value: &CheckedExpr,
        file: &str,
        line: u32,
        column: u32,
    ) -> String {
        let (setup, container, commit) = self.emit_container(place);
        let key = self.emit_expr(key);
        let value = self.emit_expr(value);
        let _ = (file, line, column);
        format!(
            "{{ {setup} let grf_key = {key}; let grf_val = {value}; let grf_result = graphite_core::map::assign(&mut {container}, grf_key, grf_val.clone()); {commit} grf_result }}"
        )
    }

    fn emit_map_call(
        &mut self,
        place: &Option<ArrayPlace>,
        object: &Option<Box<CheckedExpr>>,
        method: MapMethod,
        args: &[CheckedExpr],
        key_ty: CanonicalType,
        value_ty: CanonicalType,
        type_name: &str,
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let site = self.locate(file, line, column, end_column);
        let map_type = rust_string(type_name);
        let (setup, container, commit, borrowed) = if let Some(place) = place {
            let (setup, container, commit) = self.emit_container(place);
            (setup, container, commit, false)
        } else if let Some(object) = object {
            let map_ref = self.emit_map_ref(object);
            (String::new(), map_ref, String::new(), true)
        } else {
            (
                String::new(),
                "&std::collections::HashMap::new()".to_string(),
                String::new(),
                true,
            )
        };
        let arg = |index: usize, this: &mut Self| {
            args.get(index)
                .map(|arg| this.emit_expr(arg))
                .unwrap_or_else(|| "Default::default()".to_string())
        };
        let map_expr = if borrowed {
            format!("({container})")
        } else {
            format!("(&{container})")
        };
        let map_mut = if borrowed {
            format!("&mut ({container}).clone()")
        } else {
            format!("&mut {container}")
        };
        let key_name = self.ty_name(key_ty);
        let value_name = self.ty_name(value_ty);
        let call = match method {
            MapMethod::Get => {
                let key = arg(0, self);
                format!(
                    "{{ let grf_map = {map_expr}; let grf_key = {key}; graphite_core::map::get(grf_map, &grf_key, {site}, {map_type})? }}"
                )
            }
            MapMethod::GetOrDefault => {
                let key = arg(0, self);
                let default = arg(1, self);
                format!(
                    "{{ let grf_map = {map_expr}; let grf_key = {key}; let grf_default = {default}; graphite_core::map::get_or_default(grf_map, &grf_key, grf_default) }}"
                )
            }
            MapMethod::ContainsKey => {
                let key = arg(0, self);
                format!(
                    "{{ let grf_map = {map_expr}; let grf_key = {key}; graphite_core::map::contains(grf_map, &grf_key) }}"
                )
            }
            MapMethod::Set => {
                let key = arg(0, self);
                let value = arg(1, self);
                format!(
                    "{{ {setup} let grf_key = {key}; let grf_val = {value}; graphite_core::map::insert({map_mut}, grf_key, grf_val); {commit} }}"
                )
            }
            MapMethod::Remove => {
                let key = arg(0, self);
                format!(
                    "{{ {setup} let grf_key = {key}; let grf_result = graphite_core::map::remove({map_mut}, &grf_key); {commit} grf_result }}"
                )
            }
            MapMethod::Length => format!("({map_expr}.len() as u64)"),
            MapMethod::Keys => format!(
                "{{ let grf_map = {map_expr}; let mut grf_out: Vec<{key_name}> = Vec::new(); for grf_key in grf_map.keys() {{ grf_out.push(grf_key.clone()); }} grf_out }}"
            ),
            MapMethod::Values => format!(
                "{{ let grf_map = {map_expr}; let mut grf_out: Vec<{value_name}> = Vec::new(); for grf_val in grf_map.values() {{ grf_out.push(grf_val.clone()); }} grf_out }}"
            ),
            MapMethod::Entries => format!(
                "{{ let grf_map = {map_expr}; let mut grf_out: Vec<({key_name}, {value_name})> = Vec::new(); for (grf_key, grf_val) in grf_map.iter() {{ grf_out.push((grf_key.clone(), grf_val.clone())); }} grf_out }}"
            ),
        };
        if method == MapMethod::Set || method == MapMethod::Remove {
            return call;
        }
        if setup.is_empty() {
            call
        } else {
            format!("{{ {setup} {call} }}")
        }
    }

    fn map_to_str(&mut self, ty: CanonicalType, inner: &str) -> String {
        let CanonicalType::Map(id) = ty else {
            return "String::new()".to_string();
        };
        let shape = self.maps.get(id.0 as usize).cloned().unwrap_or(MapShape {
            key: CanonicalType::Void,
            value: CanonicalType::Void,
        });
        let binding = self.fresh_tmp();
        let output = self.fresh_tmp();
        let first = self.fresh_tmp();
        let key_text = self.display_value(shape.key, "grf_key", true);
        let value_text = self.display_value(shape.value, "grf_val", true);
        format!(
            "{{ let {binding} = &({inner}); let mut {output} = String::from(\"{{\"); let mut {first} = true; for (grf_key, grf_val) in {binding}.iter() {{ if {first} {{ {output}.push(' '); }} else {{ {output}.push_str(\", \"); }} {first} = false; {output}.push_str(&({key_text})); {output}.push_str(\": \"); {output}.push_str(&({value_text})); }} if !{first} {{ {output}.push(' '); }} {output}.push('}}'); {output} }}"
        )
    }

    fn emit_bound(&mut self, bound: &Option<Box<CheckedExpr>>) -> String {
        match bound {
            None => "None".to_string(),
            Some(bound) => format!("Some({})", self.emit_i128(bound)),
        }
    }

    fn emit_i128(&mut self, index: &CheckedExpr) -> String {
        let inner = self.emit_expr(index);
        if index.ty().is_unsigned() {
            format!("(u128::from({inner}) as i128)")
        } else {
            format!("i128::from({inner})")
        }
    }

    fn emit_read(&mut self, object: &str, index: &CheckedExpr, site: &str) -> String {
        let inner = self.emit_expr(index);
        if index.ty().is_unsigned() {
            format!("graphite_core::array::read_unsigned({object}, u128::from({inner}), {site})?")
        } else {
            format!("graphite_core::array::read_signed({object}, i128::from({inner}), {site})?")
        }
    }

    fn emit_index_set(
        &mut self,
        place: &Option<ArrayPlace>,
        object: &Option<Box<CheckedExpr>>,
        index: &CheckedExpr,
        value: &CheckedExpr,
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let value_expr = self.emit_expr(value);
        let site = self.locate(file, line, column, end_column);
        let index_expr = self.emit_expr(index);
        let unsigned = index.ty().is_unsigned();
        let (setup, container, commit) = if let Some(place) = place {
            self.emit_container(place)
        } else {
            let object = object
                .as_ref()
                .map(|object| self.emit_expr(object))
                .unwrap_or_else(|| "Vec::new()".to_string());
            (
                format!("let mut grf_recv = {object};"),
                "grf_recv".to_string(),
                String::new(),
            )
        };
        let write = if unsigned {
            format!(
                "graphite_core::array::write_unsigned(&mut {container}, u128::from({index_expr}), grf_stored.clone(), {site})?;"
            )
        } else {
            format!(
                "graphite_core::array::write_signed(&mut {container}, i128::from({index_expr}), grf_stored.clone(), {site})?;"
            )
        };
        format!("{{ let grf_stored = {value_expr}; {setup} {write} {commit} grf_stored }}")
    }

    fn emit_slice_set(
        &mut self,
        place: &Option<ArrayPlace>,
        object: &Option<Box<CheckedExpr>>,
        start: &Option<Box<CheckedExpr>>,
        stop: &Option<Box<CheckedExpr>>,
        step: &Option<Box<CheckedExpr>>,
        value: &CheckedExpr,
        limit: Option<u64>,
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let value_expr = self.emit_expr(value);
        let site = self.locate(file, line, column, end_column);
        let (setup, container, commit) = if let Some(place) = place {
            self.emit_container(place)
        } else {
            let object = object
                .as_ref()
                .map(|object| self.emit_expr(object))
                .unwrap_or_else(|| "Vec::new()".to_string());
            (
                format!("let mut grf_recv = {object};"),
                "grf_recv".to_string(),
                String::new(),
            )
        };
        let limit = limit_expr(limit);
        format!(
            "{{ let grf_repl = {value_expr}; {setup} graphite_core::array::assign_slice(&mut {container}, {}, {}, {}, &grf_repl, {limit}, {site})?; {commit} grf_repl }}",
            self.emit_bound(start),
            self.emit_bound(stop),
            self.emit_bound(step)
        )
    }

    fn emit_container(&mut self, place: &ArrayPlace) -> (String, String, String) {
        match place {
            ArrayPlace::Var(name) => (String::new(), rust_ident(name), String::new()),
            ArrayPlace::Field { object, field, .. } => {
                let (setup, parent, commit) = self.emit_container(object);
                (setup, format!("{parent}.{}", rust_ident(field)), commit)
            }
            ArrayPlace::ClassField {
                object,
                class,
                field,
                ty,
                file,
                line,
                column,
                end_column,
            } => {
                let struct_name = class_struct(&self.classes[class.0 as usize].name);
                let field_name = rust_ident(&self.classes[class.0 as usize].fields[*field].name);
                let object = self.emit_expr(object);
                let obj = self.fresh_tmp();
                let slot = self.fresh_tmp();
                let read = self.read_inside(*ty, &format!("obj.{field_name}"));
                let unpack = self.unpack_expr(*ty, "grf_raw");
                let setup = format!(
                    "let {obj} = {object}; let mut {slot} = {{ let grf_raw = graphite_core::heap::with::<{struct_name}, _>({obj}.handle(), |obj| {read})?; {unpack} }};"
                );
                let packed = self.pack_expr(*ty, &slot);
                let site = self.locate(file, *line, *column, *end_column);
                let commit = format!(
                    "let grf_packed = {packed}; graphite_core::heap::with_mut::<{struct_name}, ()>({obj}.handle(), {site}, |obj| {{ obj.{field_name} = grf_packed; }})?;"
                );
                (setup, slot, commit)
            }
            ArrayPlace::MapIndex {
                object,
                key,
                type_name,
                file,
                line,
                column,
                end_column,
                ..
            } => {
                let (setup, parent, commit) = self.emit_container(object);
                let key_expr = self.emit_expr(key);
                let tmp = self.fresh_tmp();
                let site = self.locate(file, *line, *column, *end_column);
                let map_type = rust_string(type_name);
                (
                    format!("{setup} let {tmp} = {key_expr};"),
                    format!(
                        "(*graphite_core::map::get_mut(&mut {parent}, &{tmp}, {site}, {map_type})?)"
                    ),
                    commit,
                )
            }
            ArrayPlace::Index {
                object,
                index,
                file,
                line,
                column,
                end_column,
            } => {
                let (setup, parent, commit) = self.emit_container(object);
                let site = self.locate(file, *line, *column, *end_column);
                let tmp = self.fresh_tmp();
                let index_expr = self.emit_expr(index);
                let read = if index.ty().is_unsigned() {
                    format!(
                        "graphite_core::array::index_unsigned({parent}.len() as u64, u128::from({index_expr}), {site})? as usize"
                    )
                } else {
                    format!(
                        "graphite_core::array::index_signed({parent}.len() as u64, i128::from({index_expr}), {site})? as usize"
                    )
                };
                (
                    format!("{setup} let {tmp} = {read};"),
                    format!("{parent}[{tmp}]"),
                    commit,
                )
            }
        }
    }

    fn emit_array_call(
        &mut self,
        place: &Option<ArrayPlace>,
        object: &Option<Box<CheckedExpr>>,
        method: ArrayMethod,
        args: &[CheckedExpr],
        limit: Option<u64>,
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let site = self.locate(file, line, column, end_column);
        let limit = limit_expr(limit);
        let (setup, container, commit, mutable) = if let Some(place) = place {
            let (setup, container, commit) = self.emit_container(place);
            let commit = if matches!(
                method,
                ArrayMethod::Push | ArrayMethod::Set | ArrayMethod::Insert | ArrayMethod::Remove
            ) {
                commit
            } else {
                String::new()
            };
            (setup, container, commit, true)
        } else {
            let object = object
                .as_ref()
                .map(|object| self.emit_expr(object))
                .unwrap_or_else(|| "Vec::new()".to_string());
            (
                format!("let grf_recv = {object};"),
                "grf_recv".to_string(),
                String::new(),
                false,
            )
        };
        let arg = |index: usize, this: &mut Self| {
            args.get(index)
                .map(|arg| this.emit_expr(arg))
                .unwrap_or_else(|| "Default::default()".to_string())
        };
        let call = match method {
            ArrayMethod::Length => format!("({container}.len() as u64)"),
            ArrayMethod::Get => {
                let index = args.first();
                if index.is_some_and(|index| index.ty().is_unsigned()) {
                    let index = arg(0, self);
                    format!(
                        "graphite_core::array::read_unsigned(&{container}, u128::from({index}), {site})?"
                    )
                } else {
                    let index = arg(0, self);
                    format!(
                        "graphite_core::array::read_signed(&{container}, i128::from({index}), {site})?"
                    )
                }
            }
            ArrayMethod::Push => {
                let value = arg(0, self);
                let value_tmp = self.fresh_tmp();
                if mutable {
                    format!(
                        "let {value_tmp} = {value}; graphite_core::array::push(&mut {container}, {value_tmp}, {limit}, {site})?"
                    )
                } else {
                    format!(
                        "let {value_tmp} = {value}; {{ let mut grf_tmp = {container}.clone(); graphite_core::array::push(&mut grf_tmp, {value_tmp}, {limit}, {site})?; }}"
                    )
                }
            }
            ArrayMethod::Set => {
                let index = arg(0, self);
                let value = arg(1, self);
                let index_tmp = self.fresh_tmp();
                let value_tmp = self.fresh_tmp();
                let receiver = if mutable {
                    format!("&mut {container}")
                } else {
                    format!("&mut {container}.clone()")
                };
                let call = if args.first().is_some_and(|index| index.ty().is_unsigned()) {
                    format!(
                        "graphite_core::array::write_unsigned({receiver}, u128::from({index_tmp}), {value_tmp}, {site})?"
                    )
                } else {
                    format!(
                        "graphite_core::array::write_signed({receiver}, i128::from({index_tmp}), {value_tmp}, {site})?"
                    )
                };
                format!("let {index_tmp} = {index}; let {value_tmp} = {value}; {call}")
            }
            ArrayMethod::Insert => {
                let index = arg(0, self);
                let value = arg(1, self);
                let index_tmp = self.fresh_tmp();
                let value_tmp = self.fresh_tmp();
                let receiver = if mutable {
                    format!("&mut {container}")
                } else {
                    format!("&mut {{ let mut grf_tmp = {container}.clone(); grf_tmp }}")
                };
                let call = if args.first().is_some_and(|index| index.ty().is_unsigned()) {
                    format!(
                        "graphite_core::array::insert_unsigned({receiver}, u128::from({index_tmp}), {value_tmp}, {limit}, {site})?"
                    )
                } else {
                    format!(
                        "graphite_core::array::insert_signed({receiver}, i128::from({index_tmp}), {value_tmp}, {limit}, {site})?"
                    )
                };
                format!("let {index_tmp} = {index}; let {value_tmp} = {value}; {call}")
            }
            ArrayMethod::Remove => {
                let index = arg(0, self);
                let index_tmp = self.fresh_tmp();
                let receiver = if mutable {
                    format!("&mut {container}")
                } else {
                    format!("&mut {{ let mut grf_tmp = {container}.clone(); grf_tmp }}")
                };
                let call = if args.first().is_some_and(|index| index.ty().is_unsigned()) {
                    format!(
                        "graphite_core::array::remove_unsigned({receiver}, u128::from({index_tmp}), {site})?"
                    )
                } else {
                    format!(
                        "graphite_core::array::remove_signed({receiver}, i128::from({index_tmp}), {site})?"
                    )
                };
                format!("let {index_tmp} = {index}; {call}")
            }
        };
        if setup.is_empty() || method == ArrayMethod::Length || method == ArrayMethod::Get {
            if method == ArrayMethod::Length || method == ArrayMethod::Get {
                if let CheckedExpr::Load { name, .. } =
                    object.as_deref().unwrap_or(&CheckedExpr::Bool(false))
                {
                    let _ = name;
                }
                if setup.is_empty() {
                    return call;
                }
            }
        }
        if commit.is_empty() {
            format!("{{ {setup} {call} }}")
        } else {
            format!("{{ {setup} let grf_result = {{ {call} }}; {commit} grf_result }}")
        }
    }

    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn emit_record(&mut self, record: &CheckedRecord) {
        self.line("#[derive(Clone, Default)]");
        self.line(&format!("struct {} {{", record_struct(&record.name)));
        self.indent += 1;
        for (name, ty) in &record.fields {
            self.line(&format!("{}: {},", rust_ident(name), self.ty_name(*ty)));
        }
        self.indent -= 1;
        self.line("}");
        let struct_name = record_struct(&record.name);
        self.line(&format!("impl {struct_name} {{"));
        self.indent += 1;
        self.line("fn grf_trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {");
        self.indent += 1;
        for (name, ty) in &record.fields {
            self.emit_trace_stack(*ty, &format!("self.{}", rust_ident(name)));
        }
        self.indent -= 1;
        self.line("}");
        self.line("fn grf_disarm(&mut self) {");
        self.indent += 1;
        for (name, ty) in &record.fields {
            self.emit_disarm(*ty, &format!("self.{}", rust_ident(name)));
        }
        self.indent -= 1;
        self.line("}");
        self.line(&format!("fn grf_copy_raw(&self) -> {struct_name} {{"));
        self.indent += 1;
        self.line(&format!("{struct_name} {{"));
        self.indent += 1;
        for (name, ty) in &record.fields {
            let copied = self.copy_field_expr(*ty, &format!("self.{}", rust_ident(name)));
            self.line(&format!("{}: {copied},", rust_ident(name)));
        }
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.line(&format!("fn grf_promote(&self) -> {struct_name} {{"));
        self.indent += 1;
        self.line(&format!("{struct_name} {{"));
        self.indent += 1;
        for (name, ty) in &record.fields {
            let promoted = self.promote_field_expr(*ty, &format!("self.{}", rust_ident(name)));
            self.line(&format!("{}: {promoted},", rust_ident(name)));
        }
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.line("fn grf_freeze(&self) {");
        self.indent += 1;
        for (name, ty) in &record.fields {
            let freeze = self.freeze_block(*ty, &format!("self.{}", rust_ident(name)));
            if !freeze.is_empty() {
                self.line(&freeze);
            }
        }
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
    }

    fn emit_class_struct(&mut self, class: &CheckedClass) {
        let struct_name = class_struct(&class.name);
        self.line(&format!("struct {struct_name} {{"));
        self.indent += 1;
        for field in &class.fields {
            self.line(&format!(
                "{}: {},",
                rust_ident(&field.name),
                self.heap_ty(field.ty)
            ));
        }
        self.indent -= 1;
        self.line("}");
        self.line(&format!(
            "impl graphite_core::heap::GcObject for {struct_name} {{"
        ));
        self.indent += 1;
        self.line("fn trace(&self, visit: &mut dyn FnMut(graphite_core::heap::Handle)) {");
        self.indent += 1;
        for field in &class.fields {
            self.emit_trace_heap(field.ty, &format!("self.{}", rust_ident(&field.name)));
        }
        self.indent -= 1;
        self.line("}");
        self.line("fn as_any(&self) -> &dyn std::any::Any { self }");
        self.line("fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }");
        self.indent -= 1;
        self.line("}");
    }

    fn emit_class_behavior(&mut self, class: &CheckedClass) {
        let struct_name = class_struct(&class.name);
        let params = class.init.as_ref().map(|init| &init.params);
        let param_list = params
            .map(|params| {
                params
                    .iter()
                    .map(|(name, ty)| format!("mut {}: {}", rust_ident(name), self.ty_name(*ty)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        self.constructor_return = false;
        self.fn_ret = "graphite_core::heap::Rooted".to_string();
        self.tries.clear();
        self.line(&format!(
            "fn {}({param_list}) -> Result<graphite_core::heap::Rooted, graphite_core::diagnostic::RuntimeError> {{",
            class_new_fn(&class.name)
        ));
        self.indent += 1;
        self.line(&format!(
            "let this = graphite_core::heap::alloc({struct_name} {{"
        ));
        self.indent += 1;
        for field in &class.fields {
            self.line(&format!(
                "{}: {},",
                rust_ident(&field.name),
                self.heap_default(field.ty)
            ));
        }
        self.indent -= 1;
        self.line("})?;");
        for field in &class.fields {
            if let Some(initializer) = &field.initializer {
                let value = self.emit_expr(initializer);
                self.emit_field_store(class, field, &value, &self.empty_site(), field.const_field);
            }
        }
        if let Some(init) = &class.init {
            self.constructor_return = true;
            for statement in &init.body {
                self.emit_stmt(statement);
            }
            self.constructor_return = false;
        }
        self.line("Ok(this)");
        self.indent -= 1;
        self.line("}");
        self.out.push('\n');

        for method in &class.methods {
            self.emit_class_method(class, method);
        }
    }

    fn emit_class_method(&mut self, class: &CheckedClass, method: &crate::semantic::CheckedMethod) {
        self.constructor_return = false;
        let params = method
            .params
            .iter()
            .map(|(name, ty)| format!("mut {}: {}", rust_ident(name), self.ty_name(*ty)))
            .collect::<Vec<_>>()
            .join(", ");
        let this_param = "this: graphite_core::heap::Rooted";
        let all_params = match (method.is_static, params.is_empty()) {
            (true, true) => String::new(),
            (true, false) => params,
            (false, true) => this_param.to_string(),
            (false, false) => format!("{this_param}, {params}"),
        };
        let ret = if method.ret == CanonicalType::Void {
            "()".to_string()
        } else {
            self.ty_name(method.ret)
        };
        self.fn_ret = ret.clone();
        self.tries.clear();
        self.line(&format!(
            "fn {}({all_params}) -> Result<{ret}, graphite_core::diagnostic::RuntimeError> {{",
            class_method_fn(&class.name, &method.name)
        ));
        self.indent += 1;
        for statement in &method.body {
            self.emit_stmt(statement);
        }
        if method.ret == CanonicalType::Void {
            self.line("Ok(())");
        } else {
            self.line("unreachable!();");
        }
        self.indent -= 1;
        self.line("}");
        self.out.push('\n');
    }

    fn emit_field_store(
        &mut self,
        class: &CheckedClass,
        field: &crate::semantic::CheckedClassField,
        value: &str,
        site: &str,
        freeze: bool,
    ) {
        let struct_name = class_struct(&class.name);
        let field_name = rust_ident(&field.name);
        let packed = self.pack_expr(field.ty, "grf_stored");
        self.line(&format!("let grf_stored = {value};"));
        self.line(&format!("let grf_packed = {packed};"));
        self.line(&format!(
            "graphite_core::heap::with_mut::<{struct_name}, ()>(this.handle(), {site}, |obj| {{ obj.{field_name} = grf_packed; }})?;"
        ));
        if freeze && self.contains_class(field.ty) {
            let text = self.freeze_block(field.ty, "grf_stored");
            if !text.is_empty() {
                self.line(&text);
            }
        }
    }

    fn emit_class_new(
        &mut self,
        class: ClassId,
        args: &[CheckedExpr],
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let name = self.classes[class.0 as usize].name.clone();
        let args = args
            .iter()
            .map(|arg| self.emit_expr(arg))
            .collect::<Vec<_>>()
            .join(", ");
        let site = self.locate(file, line, column, end_column);
        let path = format!("crate::{}", class_new_fn(&name));
        format!(
            "match {path}({args}) {{ Ok(value) => value, Err(error) => return Err(error.enter_call(\"{name}\", {site})) }}"
        )
    }

    fn emit_class_get(&mut self, object: &CheckedExpr, class: ClassId, field: usize) -> String {
        let info = &self.classes[class.0 as usize];
        let struct_name = class_struct(&info.name);
        let field_name = rust_ident(&info.fields[field].name);
        let field_ty = info.fields[field].ty;
        let object = self.emit_expr(object);
        let read = self.read_inside(field_ty, &format!("obj.{field_name}"));
        let unpack = self.unpack_expr(field_ty, "grf_raw");
        format!(
            "{{ let grf_obj = {object}; let grf_raw = graphite_core::heap::with::<{struct_name}, _>(grf_obj.handle(), |obj| {read})?; {unpack} }}"
        )
    }

    fn emit_class_set(
        &mut self,
        object: &CheckedExpr,
        class: ClassId,
        field: usize,
        value: &CheckedExpr,
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let info = &self.classes[class.0 as usize];
        let struct_name = class_struct(&info.name);
        let field_name = rust_ident(&info.fields[field].name);
        let field_ty = info.fields[field].ty;
        let const_field = info.fields[field].const_field;
        let object = self.emit_expr(object);
        let value = self.emit_expr(value);
        let packed = self.pack_expr(field_ty, "grf_stored");
        let site = self.locate(file, line, column, end_column);
        let freeze = if const_field && self.contains_class(field_ty) {
            self.freeze_block(field_ty, "grf_stored")
        } else {
            String::new()
        };
        format!(
            "{{ let grf_obj = {object}; let grf_stored = {value}; let grf_packed = {packed}; graphite_core::heap::with_mut::<{struct_name}, ()>(grf_obj.handle(), {site}, |obj| {{ obj.{field_name} = grf_packed; }})?; {freeze} grf_stored }}"
        )
    }

    fn emit_class_call(
        &mut self,
        class: ClassId,
        method: usize,
        object: &Option<Box<CheckedExpr>>,
        args: &[CheckedExpr],
        file: &str,
        line: u32,
        column: u32,
        end_column: u32,
    ) -> String {
        let info = &self.classes[class.0 as usize];
        let is_static = info.methods[method].is_static;
        let method_name = info.methods[method].name.clone();
        let path = format!("crate::{}", class_method_fn(&info.name, &method_name));
        let args = args
            .iter()
            .map(|arg| self.emit_expr(arg))
            .collect::<Vec<_>>()
            .join(", ");
        let site = self.locate(file, line, column, end_column);
        if is_static {
            let invoke = format!(
                "match {path}({args}) {{ Ok(value) => value, Err(error) => return Err(error.enter_call(\"{method_name}\", {site})) }}"
            );
            if let Some(object) = object {
                let object = self.emit_expr(object);
                format!("{{ let _grf_recv = {object}; {invoke} }}")
            } else {
                invoke
            }
        } else {
            let object = object
                .as_ref()
                .map(|object| self.emit_expr(object))
                .unwrap_or_else(|| "graphite_core::heap::alloc(())".to_string());
            let call_args = if args.is_empty() {
                object
            } else {
                format!("{object}, {args}")
            };
            format!(
                "match {path}({call_args}) {{ Ok(value) => value, Err(error) => return Err(error.enter_call(\"{method_name}\", {site})) }}"
            )
        }
    }

    fn empty_site(&self) -> String {
        site_expr("", 0, 1, 1, "")
    }

    fn contains_class(&self, ty: CanonicalType) -> bool {
        self.contains_class_seen(ty, &mut Vec::new())
    }

    fn contains_class_seen(&self, ty: CanonicalType, seen: &mut Vec<u32>) -> bool {
        match ty {
            CanonicalType::Class(_) => true,
            CanonicalType::Optional(kind) => self.contains_class_seen(kind.canonical(), seen),
            CanonicalType::Record(id) => {
                if !seen.contains(&id.0) {
                    seen.push(id.0);
                    let fields = self
                        .records
                        .get(id.0 as usize)
                        .map(|record| record.fields.clone())
                        .unwrap_or_default();
                    if fields
                        .iter()
                        .any(|(_, field)| self.contains_class_seen(*field, seen))
                    {
                        return true;
                    }
                }
                false
            }
            CanonicalType::Array { .. } => ty
                .element()
                .is_some_and(|element| self.contains_class_seen(element, seen)),
            CanonicalType::Map(id) => self
                .maps
                .get(id.0 as usize)
                .is_some_and(|shape| self.contains_class_seen(shape.value, seen)),
            _ => false,
        }
    }

    fn heap_ty(&self, ty: CanonicalType) -> String {
        match ty {
            CanonicalType::Optional(kind) => {
                format!("Option<{}>", self.heap_ty(kind.canonical()))
            }
            CanonicalType::Class(_) => "graphite_core::heap::Handle".to_string(),
            CanonicalType::Array { .. } => {
                format!(
                    "Vec<{}>",
                    self.heap_ty(ty.element().expect("array element"))
                )
            }
            CanonicalType::Map(id) => {
                let shape = &self.maps[id.0 as usize];
                format!(
                    "std::collections::HashMap<{}, {}>",
                    self.ty_name(shape.key),
                    self.heap_ty(shape.value)
                )
            }
            other => self.ty_name(other),
        }
    }

    fn heap_default(&self, ty: CanonicalType) -> String {
        match ty {
            CanonicalType::Optional(_) => "None".to_string(),
            CanonicalType::Class(_) => "graphite_core::heap::Handle::invalid()".to_string(),
            CanonicalType::Array { .. } => "Vec::new()".to_string(),
            CanonicalType::Map(_) => "std::collections::HashMap::new()".to_string(),
            CanonicalType::Record(_) => format!("{}::default()", self.ty_name(ty)),
            CanonicalType::Str => "String::new()".to_string(),
            _ => "Default::default()".to_string(),
        }
    }

    fn heap_contains_rooted(&self, ty: CanonicalType) -> bool {
        match ty {
            CanonicalType::Record(_) => self.contains_class(ty),
            CanonicalType::Array { .. } => ty
                .element()
                .is_some_and(|element| self.heap_contains_rooted(element)),
            CanonicalType::Map(id) => self
                .maps
                .get(id.0 as usize)
                .is_some_and(|shape| self.heap_contains_rooted(shape.value)),
            _ => false,
        }
    }

    fn read_inside(&self, ty: CanonicalType, access: &str) -> String {
        if self.heap_contains_rooted(ty) {
            self.heap_copy_raw(ty, access)
        } else {
            format!("{access}.clone()")
        }
    }

    fn heap_copy_raw(&self, ty: CanonicalType, access: &str) -> String {
        match ty {
            CanonicalType::Record(_) => format!("{access}.grf_copy_raw()"),
            CanonicalType::Array { .. } => {
                let inner = self.heap_copy_raw(ty.element().expect("array element"), "grf_item");
                format!("{access}.iter().map(|grf_item| {inner}).collect::<Vec<_>>()")
            }
            CanonicalType::Map(id) => {
                let inner = self.heap_copy_raw(self.maps[id.0 as usize].value, "grf_val");
                format!(
                    "{{ let mut grf_out = std::collections::HashMap::new(); for (grf_key, grf_val) in {access}.iter() {{ grf_out.insert(grf_key.clone(), {inner}); }} grf_out }}"
                )
            }
            _ => format!("{access}.clone()"),
        }
    }

    fn pack_expr(&self, ty: CanonicalType, access: &str) -> String {
        match ty {
            CanonicalType::Optional(kind) => {
                let inner = kind.canonical();
                let packed = self.pack_expr(inner, "grf_item");
                format!("{access}.as_ref().map(|grf_item| {packed})")
            }
            CanonicalType::Class(_) => format!("{access}.handle()"),
            CanonicalType::Record(_) if self.contains_class(ty) => {
                format!("{access}.grf_copy_raw()")
            }
            CanonicalType::Array { .. } => {
                let element = ty.element().expect("array element");
                if !self.contains_class(element) {
                    format!("{access}.clone()")
                } else {
                    let inner = self.pack_expr(element, "grf_item");
                    format!("{access}.iter().map(|grf_item| {inner}).collect::<Vec<_>>()")
                }
            }
            CanonicalType::Map(id) => {
                let value = self.maps[id.0 as usize].value;
                if !self.contains_class(value) {
                    format!("{access}.clone()")
                } else {
                    let inner = self.pack_expr(value, "grf_val");
                    format!(
                        "{{ let mut grf_out = std::collections::HashMap::new(); for (grf_key, grf_val) in {access}.iter() {{ grf_out.insert(grf_key.clone(), {inner}); }} grf_out }}"
                    )
                }
            }
            _ => format!("{access}.clone()"),
        }
    }

    fn unpack_expr(&self, ty: CanonicalType, access: &str) -> String {
        if !self.contains_class(ty) {
            return access.to_string();
        }
        format!("({})?", self.unpack_result(ty, access))
    }

    fn unpack_result(&self, ty: CanonicalType, access: &str) -> String {
        match ty {
            CanonicalType::Class(_) => format!("graphite_core::heap::root({access})"),
            CanonicalType::Record(_) if self.contains_class(ty) => {
                format!("Ok({access}.grf_promote())")
            }
            CanonicalType::Array { .. } => {
                let element = ty.element().expect("array element");
                if !self.contains_class(element) {
                    format!("Ok({access})")
                } else {
                    let inner = self.unpack_result(element, "grf_item");
                    format!(
                        "{access}.into_iter().map(|grf_item| {inner}).collect::<Result<Vec<_>, graphite_core::diagnostic::RuntimeError>>()"
                    )
                }
            }
            CanonicalType::Map(id) => {
                let value = self.maps[id.0 as usize].value;
                if !self.contains_class(value) {
                    format!("Ok({access})")
                } else {
                    let inner = self.unpack_result(value, "grf_val");
                    format!(
                        "{{ let mut grf_out = std::collections::HashMap::new(); for (grf_key, grf_val) in {access}.into_iter() {{ grf_out.insert(grf_key, ({inner})?); }} Ok(grf_out) }}"
                    )
                }
            }
            CanonicalType::Optional(kind) => {
                let inner = kind.canonical();
                if inner.is_class() {
                    format!(
                        "Ok(match {access} {{ None => None, Some(grf_item) => Some(graphite_core::heap::root(grf_item)?) }})"
                    )
                } else if self.contains_class(inner) {
                    let unpacked = self.unpack_result(inner, "grf_item");
                    format!(
                        "Ok(match {access} {{ None => None, Some(grf_item) => Some(({unpacked})?) }})"
                    )
                } else {
                    format!("Ok({access})")
                }
            }
            _ => format!("Ok({access})"),
        }
    }

    fn freeze_block(&self, ty: CanonicalType, access: &str) -> String {
        if !self.contains_class(ty) {
            return String::new();
        }
        match ty {
            CanonicalType::Class(_) => {
                format!("graphite_core::heap::freeze({access}.handle());")
            }
            CanonicalType::Record(_) => format!("{access}.grf_freeze();"),
            CanonicalType::Array { .. } => {
                let inner = self.freeze_block(ty.element().expect("array element"), "grf_item");
                format!("for grf_item in {access}.iter() {{ {inner} }}")
            }
            CanonicalType::Map(id) => {
                let inner = self.freeze_block(self.maps[id.0 as usize].value, "grf_item");
                format!("for grf_item in {access}.values() {{ {inner} }}")
            }
            CanonicalType::Optional(kind) => {
                let inner = self.freeze_block(kind.canonical(), "grf_item");
                format!("if let Some(grf_item) = {access}.as_ref() {{ {inner} }}")
            }
            _ => String::new(),
        }
    }

    fn emit_trace_heap(&mut self, ty: CanonicalType, access: &str) {
        if !self.contains_class(ty) {
            return;
        }
        match ty {
            CanonicalType::Optional(kind) => {
                let inner = kind.canonical();
                self.line(&format!("if let Some(grf_edge) = {access}.as_ref() {{"));
                self.indent += 1;
                if inner.is_class() {
                    self.line("visit(*grf_edge);");
                } else {
                    self.emit_trace_heap(inner, "grf_edge");
                }
                self.indent -= 1;
                self.line("}");
            }
            CanonicalType::Class(_) => self.line(&format!("visit({access});")),
            CanonicalType::Record(_) => self.line(&format!("{access}.grf_trace(visit);")),
            CanonicalType::Array { .. } => {
                let element = ty.element().expect("array element");
                self.line(&format!("for grf_edge in {access}.iter() {{"));
                self.indent += 1;
                if element.is_class() {
                    self.line("visit(*grf_edge);");
                } else {
                    self.emit_trace_heap(element, "grf_edge");
                }
                self.indent -= 1;
                self.line("}");
            }
            CanonicalType::Map(id) => {
                let value = self.maps[id.0 as usize].value;
                self.line(&format!("for grf_edge in {access}.values() {{"));
                self.indent += 1;
                if value.is_class() {
                    self.line("visit(*grf_edge);");
                } else {
                    self.emit_trace_heap(value, "grf_edge");
                }
                self.indent -= 1;
                self.line("}");
            }
            _ => {}
        }
    }

    fn emit_trace_stack(&mut self, ty: CanonicalType, access: &str) {
        if !self.contains_class(ty) {
            return;
        }
        match ty {
            CanonicalType::Optional(kind) => {
                let inner = kind.canonical();
                self.line(&format!("if let Some(grf_edge) = {access}.as_ref() {{"));
                self.indent += 1;
                if inner.is_class() {
                    self.line("visit(grf_edge.handle());");
                } else {
                    self.emit_trace_stack(inner, "grf_edge");
                }
                self.indent -= 1;
                self.line("}");
            }
            CanonicalType::Class(_) => self.line(&format!("visit({access}.handle());")),
            CanonicalType::Record(_) => self.line(&format!("{access}.grf_trace(visit);")),
            CanonicalType::Array { .. } => {
                let element = ty.element().expect("array element");
                self.line(&format!("for grf_edge in {access}.iter() {{"));
                self.indent += 1;
                if element.is_class() {
                    self.line("visit(grf_edge.handle());");
                } else {
                    self.emit_trace_stack(element, "grf_edge");
                }
                self.indent -= 1;
                self.line("}");
            }
            CanonicalType::Map(id) => {
                let value = self.maps[id.0 as usize].value;
                self.line(&format!("for grf_edge in {access}.values() {{"));
                self.indent += 1;
                if value.is_class() {
                    self.line("visit(grf_edge.handle());");
                } else {
                    self.emit_trace_stack(value, "grf_edge");
                }
                self.indent -= 1;
                self.line("}");
            }
            _ => {}
        }
    }

    fn emit_disarm(&mut self, ty: CanonicalType, access: &str) {
        if !self.contains_class(ty) {
            return;
        }
        match ty {
            CanonicalType::Optional(kind) => {
                let inner = kind.canonical();
                self.line(&format!("if let Some(grf_edge) = {access}.as_mut() {{"));
                self.indent += 1;
                self.emit_disarm(inner, "grf_edge");
                self.indent -= 1;
                self.line("}");
            }
            CanonicalType::Class(_) => self.line(&format!("{access}.disarm();")),
            CanonicalType::Record(_) => self.line(&format!("{access}.grf_disarm();")),
            CanonicalType::Array { .. } => {
                let element = ty.element().expect("array element");
                self.line(&format!("for grf_edge in {access}.iter_mut() {{"));
                self.indent += 1;
                self.emit_disarm(element, "grf_edge");
                self.indent -= 1;
                self.line("}");
            }
            CanonicalType::Map(id) => {
                let value = self.maps[id.0 as usize].value;
                self.line(&format!("for grf_edge in {access}.values_mut() {{"));
                self.indent += 1;
                self.emit_disarm(value, "grf_edge");
                self.indent -= 1;
                self.line("}");
            }
            _ => {}
        }
    }

    fn copy_field_expr(&self, ty: CanonicalType, access: &str) -> String {
        if !self.contains_class(ty) {
            return format!("{access}.clone()");
        }
        match ty {
            CanonicalType::Optional(kind) => {
                let copied = self.copy_field_expr(kind.canonical(), "grf_item");
                format!("{access}.as_ref().map(|grf_item| {copied})")
            }
            CanonicalType::Class(_) => format!("{access}.duplicate_unrooted()"),
            CanonicalType::Record(_) => format!("{access}.grf_copy_raw()"),
            CanonicalType::Array { .. } => {
                let inner = self.copy_field_expr(ty.element().expect("array element"), "grf_item");
                format!("{access}.iter().map(|grf_item| {inner}).collect::<Vec<_>>()")
            }
            CanonicalType::Map(id) => {
                let inner = self.copy_field_expr(self.maps[id.0 as usize].value, "grf_val");
                format!(
                    "{{ let mut grf_out = std::collections::HashMap::new(); for (grf_key, grf_val) in {access}.iter() {{ grf_out.insert(grf_key.clone(), {inner}); }} grf_out }}"
                )
            }
            _ => format!("{access}.clone()"),
        }
    }

    fn promote_field_expr(&self, ty: CanonicalType, access: &str) -> String {
        if !self.contains_class(ty) {
            return format!("{access}.clone()");
        }
        match ty {
            CanonicalType::Optional(kind) => {
                let promoted = self.promote_field_expr(kind.canonical(), "grf_item");
                format!("{access}.as_ref().map(|grf_item| {promoted})")
            }
            CanonicalType::Class(_) => format!("{access}.promote()"),
            CanonicalType::Record(_) => format!("{access}.grf_promote()"),
            CanonicalType::Array { .. } => {
                let inner =
                    self.promote_field_expr(ty.element().expect("array element"), "grf_item");
                format!("{access}.iter().map(|grf_item| {inner}).collect::<Vec<_>>()")
            }
            CanonicalType::Map(id) => {
                let inner = self.promote_field_expr(self.maps[id.0 as usize].value, "grf_val");
                format!(
                    "{{ let mut grf_out = std::collections::HashMap::new(); for (grf_key, grf_val) in {access}.iter() {{ grf_out.insert(grf_key.clone(), {inner}); }} grf_out }}"
                )
            }
            _ => format!("{access}.clone()"),
        }
    }

    fn emit_record_literal(&mut self, id: RecordId, fields: &[(usize, CheckedExpr)]) -> String {
        let record = self
            .records
            .get(id.0 as usize)
            .expect("record id is resolved before code generation");
        let mut temps = vec![String::new(); record.fields.len()];
        let mut setup = String::new();
        for (field_index, value) in fields {
            let temp = self.fresh_tmp();
            let value = self.emit_expr(value);
            setup.push_str(&format!("let {temp} = {value}; "));
            if let Some(slot) = temps.get_mut(*field_index) {
                *slot = temp;
            }
        }
        let inits = record
            .fields
            .iter()
            .enumerate()
            .map(|(index, (name, _))| {
                format!(
                    "{}: {}",
                    rust_ident(name),
                    temps
                        .get(index)
                        .map(String::as_str)
                        .unwrap_or("Default::default()")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {setup}{} {{ {inits} }} }}", record_struct(&record.name))
    }

    fn array_to_str(&mut self, ty: CanonicalType, inner: &str) -> String {
        let Some(element) = ty.element() else {
            return "String::new()".to_string();
        };
        let body = self.display_value(element, "grf_item", true);
        format!("graphite_core::convert::array_text(&({inner}), |grf_item| {{ {body} }})")
    }

    fn record_to_str(&mut self, ty: CanonicalType, inner: &str) -> String {
        let CanonicalType::Record(id) = ty else {
            return "String::new()".to_string();
        };
        let fields = self
            .records
            .get(id.0 as usize)
            .map(|record| record.fields.clone())
            .unwrap_or_default();
        let binding = self.fresh_tmp();
        let output = self.fresh_tmp();
        let mut body =
            format!("let {binding} = &({inner}); let mut {output} = String::from(\"{{ \");");
        for (index, (name, field_ty)) in fields.iter().enumerate() {
            if index > 0 {
                body.push_str(&format!("{output}.push_str(\", \");"));
            }
            body.push_str(&format!(
                "{output}.push_str({});",
                rust_string(&format!("{name}: "))
            ));
            let access = format!("{binding}.{}", rust_ident(name));
            let text = self.display_value(*field_ty, &access, false);
            body.push_str(&format!("{output}.push_str(&({text}));"));
        }
        body.push_str(&format!("{output}.push_str(\" }}\"); {output}"));
        format!("{{ {body} }}")
    }

    fn display_value(&mut self, ty: CanonicalType, expr: &str, referenced: bool) -> String {
        if ty.is_array() {
            return self.array_to_str(ty, expr);
        }
        if ty.is_record() {
            return self.record_to_str(ty, expr);
        }
        if ty.is_map() {
            return self.map_to_str(ty, expr);
        }
        if ty == CanonicalType::Type {
            let value = if referenced {
                format!("*{expr}")
            } else {
                expr.to_string()
            };
            return type_to_str(&value);
        }
        let value = if referenced {
            format!("*{expr}")
        } else {
            expr.to_string()
        };
        let text = if referenced {
            expr.to_string()
        } else {
            format!("&({expr})")
        };
        match ty {
            CanonicalType::Int16 | CanonicalType::Int32 | CanonicalType::Int64 => {
                format!("graphite_core::convert::signed_text(i128::from({value}))")
            }
            CanonicalType::UInt16 | CanonicalType::UInt32 | CanonicalType::UInt64 => {
                format!("graphite_core::convert::unsigned_text(u128::from({value}))")
            }
            CanonicalType::Flt32 => format!("graphite_core::convert::f32_text({value})"),
            CanonicalType::Flt64 => format!("graphite_core::convert::f64_text({value})"),
            CanonicalType::Bln => format!("graphite_core::convert::bln_text({value})"),
            CanonicalType::Chr => {
                format!(
                    "graphite_core::convert::quoted_str(&graphite_core::convert::char_to_str({value}))"
                )
            }
            CanonicalType::Str => format!("graphite_core::convert::quoted_str({text})"),
            _ => "String::new()".to_string(),
        }
    }

    fn field_name(&self, id: RecordId, field: usize) -> String {
        self.records
            .get(id.0 as usize)
            .and_then(|record| record.fields.get(field))
            .map(|(name, _)| rust_ident(name))
            .unwrap_or_else(|| "missing".to_string())
    }

    fn ty_name(&self, ty: CanonicalType) -> String {
        match ty {
            CanonicalType::Record(id) => self
                .records
                .get(id.0 as usize)
                .map(|record| record_struct(&record.name))
                .unwrap_or_else(|| "GrfUnknown".to_string()),
            CanonicalType::Map(id) => {
                let shape = self.maps.get(id.0 as usize).cloned().unwrap_or(MapShape {
                    key: CanonicalType::Void,
                    value: CanonicalType::Void,
                });
                format!(
                    "std::collections::HashMap<{}, {}>",
                    self.ty_name(shape.key),
                    self.ty_name(shape.value)
                )
            }
            CanonicalType::Array {
                leaf: Leaf::Map(id),
                depth,
                ..
            } => {
                let mut name = self.ty_name(CanonicalType::Map(id));
                for _ in 0..depth {
                    name = format!("Vec<{name}>");
                }
                name
            }
            CanonicalType::Array {
                leaf: Leaf::Record(id),
                depth,
                ..
            } => {
                let mut name = self.ty_name(CanonicalType::Record(id));
                for _ in 0..depth {
                    name = format!("Vec<{name}>");
                }
                name
            }
            CanonicalType::Optional(kind) => format!("Option<{}>", self.ty_name(kind.canonical())),
            other => other.rust_name(),
        }
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

fn vec_turbofish(ty_name: &str) -> String {
    ty_name
        .strip_prefix("Vec<")
        .and_then(|rest| rest.strip_suffix('>'))
        .map(|inner| format!("Vec::<{inner}>"))
        .unwrap_or_else(|| ty_name.to_string())
}

fn record_struct(name: &str) -> String {
    format!("Grf{name}")
}

fn class_struct(name: &str) -> String {
    format!("GrfClass{name}")
}

fn class_new_fn(name: &str) -> String {
    format!("grf_new_{name}")
}

fn class_method_fn(class: &str, method: &str) -> String {
    format!("grf_method_{class}_{method}")
}

fn record_order(records: &[CheckedRecord]) -> Vec<usize> {
    fn dependencies(ty: CanonicalType, found: &mut Vec<usize>) {
        match ty {
            CanonicalType::Record(id) => found.push(id.0 as usize),
            CanonicalType::Array {
                leaf: Leaf::Record(id),
                ..
            } => found.push(id.0 as usize),
            _ => {}
        }
    }
    fn visit(index: usize, records: &[CheckedRecord], seen: &mut [bool], order: &mut Vec<usize>) {
        if index >= records.len() || seen[index] {
            return;
        }
        seen[index] = true;
        for (_, ty) in &records[index].fields {
            let mut deps = Vec::new();
            dependencies(*ty, &mut deps);
            for dep in deps {
                visit(dep, records, seen, order);
            }
        }
        order.push(index);
    }
    let mut seen = vec![false; records.len()];
    let mut order = Vec::new();
    for index in 0..records.len() {
        visit(index, records, &mut seen, &mut order);
    }
    order
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

fn type_to_str(inner: &str) -> String {
    format!(
        "{{ let grf_ty = {inner}; if grf_ty.record {{ format!(\"<type '{{}}'>\", grf_ty.name) }} else {{ format!(\"<class '{{}}'>\", grf_ty.name) }} }}"
    )
}

fn emit_conv(
    op: ConvOp,
    from: CanonicalType,
    to: CanonicalType,
    inner: &str,
    site: &str,
) -> String {
    let from_name = rust_string(&from.spelling());
    let to_name = rust_string(&to.spelling());
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
        ConvOp::ArrayToStr
        | ConvOp::RecordToStr
        | ConvOp::MapToStr
        | ConvOp::TypeToStr
        | ConvOp::OptionalToStr => String::new(),
        ConvOp::StrToSigned => format!(
            "(graphite_core::convert::str_to_signed(&({inner}), {signed_min}, {signed_max}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::StrToUnsigned => format!(
            "(graphite_core::convert::str_to_unsigned(&({inner}), {unsigned_max}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::StrToF64 => format!("graphite_core::convert::str_to_f64(&({inner}), {site})?"),
        ConvOp::StrToF32 => format!("graphite_core::convert::str_to_f32(&({inner}), {site})?"),
        ConvOp::StrToBool => format!("graphite_core::convert::str_to_bool(&({inner}), {site})?"),
        ConvOp::StrToChar => format!("graphite_core::convert::str_to_char(&({inner}), {site})?"),
        ConvOp::SignedToSigned => format!(
            "(graphite_core::convert::fit_signed(i128::from({inner}), {signed_min}, {signed_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::SignedToUnsigned => format!(
            "(graphite_core::convert::signed_to_unsigned(i128::from({inner}), {unsigned_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::UnsignedToSigned => format!(
            "(graphite_core::convert::unsigned_to_signed(u128::from({inner}), {signed_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::UnsignedToUnsigned => format!(
            "(graphite_core::convert::fit_unsigned(u128::from({inner}), {unsigned_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::SignedToF64 => {
            format!(
                "graphite_core::convert::signed_to_f64(i128::from({inner}), {from_name}, {site})?"
            )
        }
        ConvOp::SignedToF32 => {
            format!(
                "graphite_core::convert::signed_to_f32(i128::from({inner}), {from_name}, {site})?"
            )
        }
        ConvOp::UnsignedToF64 => format!(
            "graphite_core::convert::unsigned_to_f64(u128::from({inner}), {from_name}, {site})?"
        ),
        ConvOp::UnsignedToF32 => format!(
            "graphite_core::convert::unsigned_to_f32(u128::from({inner}), {from_name}, {site})?"
        ),
        ConvOp::F64ToF32 => {
            format!("graphite_core::convert::f64_to_f32({inner}, {from_name}, {to_name}, {site})?")
        }
        ConvOp::F32ToF64 => format!("(f64::from({inner}))"),
        ConvOp::F64ToSigned => format!(
            "(graphite_core::convert::f64_to_signed({inner}, {signed_min}, {signed_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::F64ToUnsigned => format!(
            "(graphite_core::convert::f64_to_unsigned({inner}, {unsigned_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::F32ToSigned => format!(
            "(graphite_core::convert::f64_to_signed(f64::from({inner}), {signed_min}, {signed_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::F32ToUnsigned => format!(
            "(graphite_core::convert::f64_to_unsigned(f64::from({inner}), {unsigned_max}, {from_name}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::SignedToBool => {
            format!(
                "graphite_core::convert::signed_to_bool(i128::from({inner}), {from_name}, {site})?"
            )
        }
        ConvOp::UnsignedToBool => format!(
            "graphite_core::convert::unsigned_to_bool(u128::from({inner}), {from_name}, {site})?"
        ),
        ConvOp::F64ToBool => {
            format!("graphite_core::convert::f64_to_bool({inner}, {from_name}, {site})?")
        }
        ConvOp::F32ToBool => {
            format!("graphite_core::convert::f64_to_bool(f64::from({inner}), {from_name}, {site})?")
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
                "graphite_core::convert::code_to_char_signed(i128::from({inner}), {from_name}, {site})?"
            )
        }
        ConvOp::UnsignedToChar => format!(
            "graphite_core::convert::code_to_char_unsigned(u128::from({inner}), {from_name}, {site})?"
        ),
        ConvOp::F64ToChar => {
            format!("graphite_core::convert::code_to_char_f64({inner}, {from_name}, {site})?")
        }
        ConvOp::F32ToChar => {
            format!(
                "graphite_core::convert::code_to_char_f64(f64::from({inner}), {from_name}, {site})?"
            )
        }
        ConvOp::CharToSigned => format!(
            "(graphite_core::convert::char_to_signed({inner}, {signed_max}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::CharToUnsigned => format!(
            "(graphite_core::convert::char_to_unsigned({inner}, {unsigned_max}, {to_name}, {site})? as {rust})"
        ),
        ConvOp::CharToF64 => format!("graphite_core::convert::char_to_f64({inner})"),
        ConvOp::CharToF32 => format!("graphite_core::convert::char_to_f32({inner}, {site})?"),
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

fn int_float_op(op: BinaryOperator, order: &str, int_on_left: bool) -> String {
    let less = if int_on_left {
        format!("{order} < 0")
    } else {
        format!("{order} > 0")
    };
    let greater = if int_on_left {
        format!("{order} > 0")
    } else {
        format!("{order} < 0")
    };
    match op {
        BinaryOperator::Less => format!("({less})"),
        BinaryOperator::Greater => format!("({greater})"),
        BinaryOperator::LessEqual => format!("({less} || {order} == 0)"),
        BinaryOperator::GreaterEqual => format!("({greater} || {order} == 0)"),
        BinaryOperator::Equal => format!("({order} == 0)"),
        BinaryOperator::NotEqual => format!("({order} != 0)"),
        _ => "false".to_string(),
    }
}

fn emit_num_cast(
    from: CanonicalType,
    to: CanonicalType,
    check: bool,
    inner: &str,
    site: &str,
) -> String {
    let rust = to.rust_name();
    if !check {
        return match (from, to) {
            (CanonicalType::Int16, CanonicalType::Int32) => format!("i32::from({inner})"),
            (CanonicalType::Int16 | CanonicalType::Int32, CanonicalType::Int64) => {
                format!("i64::from({inner})")
            }
            (CanonicalType::UInt16, CanonicalType::UInt32) => format!("u32::from({inner})"),
            (CanonicalType::UInt16 | CanonicalType::UInt32, CanonicalType::UInt64) => {
                format!("u64::from({inner})")
            }
            (CanonicalType::UInt16, CanonicalType::Int32) => format!("i32::from({inner})"),
            (CanonicalType::UInt16 | CanonicalType::UInt32, CanonicalType::Int64) => {
                format!("i64::from({inner})")
            }
            (CanonicalType::Int16 | CanonicalType::UInt16, CanonicalType::Flt32) => {
                format!("f32::from({inner})")
            }
            (
                CanonicalType::Int16
                | CanonicalType::Int32
                | CanonicalType::UInt16
                | CanonicalType::UInt32,
                CanonicalType::Flt64,
            ) => format!("f64::from({inner})"),
            (CanonicalType::Flt32, CanonicalType::Flt64) => format!("f64::from({inner})"),
            _ => format!("({inner} as {rust})"),
        };
    }
    let to_name = rust_string(&to.spelling());
    let max = match to {
        CanonicalType::Int16 => i128::from(i16::MAX),
        CanonicalType::Int32 => i128::from(i32::MAX),
        _ => i128::from(i64::MAX),
    };
    match (
        from.is_unsigned(),
        to.is_float(),
        to == CanonicalType::Flt32,
    ) {
        (true, false, _) => format!(
            "(graphite_core::numeric::unsigned_to_signed(u128::from({inner}), {max}, {to_name}, {site})? as {rust})"
        ),
        (false, true, true) => {
            format!(
                "graphite_core::numeric::signed_to_f32(i128::from({inner}), {to_name}, {site})?"
            )
        }
        (true, true, true) => format!(
            "graphite_core::numeric::unsigned_to_f32(u128::from({inner}), {to_name}, {site})?"
        ),
        (false, true, false) => {
            format!(
                "graphite_core::numeric::signed_to_f64(i128::from({inner}), {to_name}, {site})?"
            )
        }
        (true, true, false) => format!(
            "graphite_core::numeric::unsigned_to_f64(u128::from({inner}), {to_name}, {site})?"
        ),
        (false, false, _) => format!("({inner} as {rust})"),
    }
}

impl Emitter<'_> {
    fn locate(&self, file: &str, line: u32, column: u32, end_column: u32) -> String {
        let excerpt = if self.source.is_empty() {
            String::new()
        } else {
            graphite_core::diagnostic::source_line(self.source, line)
        };
        site_expr(file, line, column, end_column, &excerpt)
    }

    fn locate_span(&self, span: crate::span::Span) -> String {
        self.locate(
            self.source_name,
            span.start.line,
            span.start.column,
            span.end.column,
        )
    }
}

fn limit_expr(limit: Option<u64>) -> String {
    match limit {
        Some(limit) => format!("Some({limit}u64)"),
        None => "None".to_string(),
    }
}

fn site_expr(file: &str, line: u32, column: u32, end_column: u32, excerpt: &str) -> String {
    format!(
        "graphite_core::convert::Site {{ file: {}, line: {line}, column: {column}, end_column: {end_column}, excerpt: {} }}",
        rust_string(file),
        rust_string(excerpt)
    )
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

const PRELUDE: &str = r#"#![allow(dead_code, unused_macros, unreachable_code, unused_variables)]
enum GrfExit<T> {
    Fall,
    Return(T),
    Break(&'static str),
    Continue(&'static str),
    ContinueInner(&'static str),
    Error(graphite_core::diagnostic::RuntimeError),
}

macro_rules! grf_add {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_add($b) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_sub {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_sub($b) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_mul {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_mul($b) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_div {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_div($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    return Err(graphite_core::diagnostic::division_by_zero($site));
                } else {
                    return Err(graphite_core::diagnostic::integer_overflow($site));
                }
            }
        }
    };
}
macro_rules! grf_rem {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_rem($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    return Err(graphite_core::diagnostic::remainder_by_zero($site));
                } else {
                    return Err(graphite_core::diagnostic::integer_overflow($site));
                }
            }
        }
    };
}
macro_rules! grf_neg {
    ($a:expr, $site:expr) => {
        match $a.checked_neg() {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    };
}
macro_rules! grf_pow {
    ($a:expr, $b:expr, $site:expr) => {{
        let grf_exp = match u32::try_from($b) {
            Ok(exp) => exp,
            Err(_) => return Err(graphite_core::diagnostic::integer_overflow($site)),
        };
        match $a.checked_pow(grf_exp) {
            Some(value) => value,
            None => return Err(graphite_core::diagnostic::integer_overflow($site)),
        }
    }};
}
macro_rules! grf_floordiv {
    ($a:expr, $b:expr, $site:expr) => {
        match $a.checked_div_euclid($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    return Err(graphite_core::diagnostic::division_by_zero($site));
                } else {
                    return Err(graphite_core::diagnostic::integer_overflow($site));
                }
            }
        }
    };
}

pub(crate) fn grf_str_length(value: &str) -> u64 {
    value.chars().count() as u64
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GrfType {
    pub(crate) name: &'static str,
    pub(crate) record: bool,
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
