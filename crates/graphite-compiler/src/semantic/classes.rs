//! Definite initialization, `this` escape, and method effects for classes.
//!
//! This walk uses the parsed class body. Semantic analysis still type-checks
//! that body; these checks add the class-specific rules.

use std::collections::HashSet;

use crate::parser::*;
use crate::span::Span;

#[derive(Clone, Copy)]
pub struct MethodFlag {
    pub mutating: bool,
    pub leaks: bool,
}

pub fn method_flags(class: &ClassDecl) -> Vec<MethodFlag> {
    let scans = class
        .methods
        .iter()
        .map(|method| scan_method(&method.body))
        .collect::<Vec<_>>();
    let mut flags = scans
        .iter()
        .map(|scan| MethodFlag {
            mutating: scan.mutated,
            leaks: scan.leaked,
        })
        .collect::<Vec<_>>();
    loop {
        let mut changed = false;
        for (index, scan) in scans.iter().enumerate() {
            for name in &scan.calls {
                let Some(callee) = class
                    .methods
                    .iter()
                    .position(|method| method.name.name == *name)
                else {
                    continue;
                };
                if flags[callee].mutating && !flags[index].mutating {
                    flags[index].mutating = true;
                    changed = true;
                }
                if flags[callee].leaks && !flags[index].leaks {
                    flags[index].leaks = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    flags
}

pub fn init_issues(class: &ClassDecl, leaks: &[bool], defaults: &[bool]) -> Vec<(Span, String)> {
    let ready = |index: usize, field: &ClassField| {
        field.initializer.is_some() || defaults.get(index).copied().unwrap_or(false)
    };
    let Some(init) = &class.init else {
        return class
            .fields
            .iter()
            .enumerate()
            .filter(|(index, field)| !ready(*index, field))
            .map(|(_, field)| {
                (
                    field.span,
                    format!("field '{}' is not initialized", field.name.name),
                )
            })
            .collect();
    };
    let mut flow = Flow {
        assigned: class
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| ready(index, field))
            .collect(),
        tainted_vars: HashSet::new(),
        tainted_fields: vec![false; class.fields.len()],
        diverges: false,
    };
    let mut issues = Vec::new();
    walk_block(&init.body, class, leaks, true, &mut flow, &mut issues);
    if !flow.diverges {
        for (index, field) in class.fields.iter().enumerate() {
            if !flow.assigned[index] {
                issues.push((
                    field.span,
                    format!("field '{}' is not initialized", field.name.name),
                ));
            }
        }
    }
    issues
}

struct Scan {
    mutated: bool,
    leaked: bool,
    calls: Vec<String>,
}

fn scan_method(body: &BlockStmt) -> Scan {
    let mut scan = Scan {
        mutated: false,
        leaked: false,
        calls: Vec::new(),
    };
    let mut tainted = HashSet::new();
    scan_block(body, &mut tainted, &mut scan);
    scan
}

fn scan_block(block: &BlockStmt, tainted: &mut HashSet<String>, scan: &mut Scan) {
    for statement in &block.statements {
        scan_stmt(statement, tainted, scan);
    }
}

fn scan_stmt(stmt: &Stmt, tainted: &mut HashSet<String>, scan: &mut Scan) {
    match stmt {
        Stmt::Block(block) => scan_block(block, tainted, scan),
        Stmt::VariableDecl(decl) => {
            if let Some(initializer) = &decl.initializer {
                if scan_expr(initializer, tainted, scan) {
                    tainted.insert(decl.name.name.clone());
                }
            }
        }
        Stmt::Destructure(decl) => {
            let _ = scan_expr(&decl.initializer, tainted, scan);
        }
        Stmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                if scan_expr(value, tainted, scan) {
                    scan.leaked = true;
                }
            }
        }
        Stmt::If(stmt) => {
            let _ = scan_expr(&stmt.condition, tainted, scan);
            let mut then_taint = tainted.clone();
            scan_block(&stmt.then_branch, &mut then_taint, scan);
            let mut else_taint = tainted.clone();
            if let Some(branch) = &stmt.else_branch {
                scan_stmt(branch, &mut else_taint, scan);
            }
            tainted.extend(then_taint);
            tainted.extend(else_taint);
        }
        Stmt::Loop(stmt) => {
            match &stmt.kind {
                LoopKind::Condition { condition } => {
                    let _ = scan_expr(condition, tainted, scan);
                }
                LoopKind::CStyle {
                    initializer,
                    init_expr,
                    condition,
                    update,
                } => {
                    if let Some(initializer) = initializer {
                        if let Some(value) = &initializer.initializer {
                            if scan_expr(value, tainted, scan) {
                                tainted.insert(initializer.name.name.clone());
                            }
                        }
                    }
                    if let Some(expr) = init_expr {
                        let _ = scan_expr(expr, tainted, scan);
                    }
                    if let Some(expr) = condition {
                        let _ = scan_expr(expr, tainted, scan);
                    }
                    if let Some(expr) = update {
                        let _ = scan_expr(expr, tainted, scan);
                    }
                }
                LoopKind::Iteration { iterable, .. } => {
                    let _ = scan_expr(iterable, tainted, scan);
                }
                LoopKind::Infinite => {}
            }
            scan_block(&stmt.body, tainted, scan);
        }
        Stmt::Try(stmt) => {
            scan_block(&stmt.body, tainted, scan);
            for clause in &stmt.catches {
                scan_block(&clause.block, tainted, scan);
            }
            if let Some(finally_block) = &stmt.finally_block {
                scan_block(finally_block, tainted, scan);
            }
        }
        Stmt::Expression(stmt) => {
            let _ = scan_expr(&stmt.expression, tainted, scan);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn scan_expr(expr: &Expr, tainted: &HashSet<String>, scan: &mut Scan) -> bool {
    match expr {
        Expr::Parenthesized { expression, .. } => scan_expr(expression, tainted, scan),
        Expr::Identifier(ident) => ident.name == "this" || tainted.contains(&ident.name),
        Expr::Assignment(assignment) => {
            let value_is_this = scan_expr(&assignment.value, tainted, scan);
            match assignment.target.as_ref() {
                Expr::Member(member) if receiver_is_this(&member.object, tainted) => {
                    scan.mutated = true;
                    let _ = scan_expr(&member.object, tainted, scan);
                    if value_is_this && !direct_receiver(&member.object, tainted) {
                        scan.leaked = true;
                    }
                    false
                }
                Expr::Identifier(_) => false,
                other => {
                    let _ = scan_expr(other, tainted, scan);
                    if value_is_this {
                        scan.leaked = true;
                    }
                    false
                }
            }
        }
        Expr::New(new_expr) => {
            for argument in &new_expr.arguments {
                if scan_expr(argument, tainted, scan) {
                    scan.leaked = true;
                }
            }
            false
        }
        Expr::Call(call) => {
            if let Expr::Member(member) = call.callee.as_ref() {
                let object_is_this = scan_expr(&member.object, tainted, scan);
                if object_is_this {
                    scan.calls.push(member.member.name.clone());
                }
                for argument in &call.arguments {
                    if scan_expr(argument, tainted, scan) {
                        scan.leaked = true;
                    }
                }
                return false;
            }
            let _ = scan_expr(&call.callee, tainted, scan);
            for argument in &call.arguments {
                if scan_expr(argument, tainted, scan) {
                    scan.leaked = true;
                }
            }
            false
        }
        Expr::Array(array) => {
            let mut leaked = false;
            for element in &array.elements {
                if scan_expr(element, tainted, scan) {
                    leaked = true;
                }
            }
            if leaked {
                scan.leaked = true;
            }
            false
        }
        Expr::Record(record) => {
            let mut leaked = false;
            for entry in &record.entries {
                let value = match entry {
                    BraceEntry::Pair { value, .. } | BraceEntry::Spread { value, .. } => value,
                };
                if scan_expr(value, tainted, scan) {
                    leaked = true;
                }
            }
            if leaked {
                scan.leaked = true;
            }
            false
        }
        Expr::Member(member) => {
            let _ = scan_expr(&member.object, tainted, scan);
            false
        }
        Expr::Unary(unary) => scan_expr(&unary.operand, tainted, scan),
        Expr::Binary(binary) => {
            let _ = scan_expr(&binary.left, tainted, scan);
            let _ = scan_expr(&binary.right, tainted, scan);
            false
        }
        Expr::Index(index) => {
            let _ = scan_expr(&index.object, tainted, scan);
            let _ = scan_expr(&index.index, tainted, scan);
            false
        }
        Expr::Slice(slice) => {
            let _ = scan_expr(&slice.object, tainted, scan);
            for bound in [&slice.start, &slice.stop, &slice.step] {
                if let Some(bound) = bound {
                    let _ = scan_expr(bound, tainted, scan);
                }
            }
            false
        }
        Expr::Conversion(conversion) => conversion
            .arguments
            .iter()
            .any(|argument| scan_expr(argument, tainted, scan)),
        Expr::Update(update) => scan_expr(&update.operand, tainted, scan),
        Expr::InterpolatedString(interpolated) => {
            for part in &interpolated.parts {
                if let StringPart::Expression { expression, .. } = part {
                    let _ = scan_expr(expression, tainted, scan);
                }
            }
            false
        }
        Expr::Literal(_) => false,
    }
}

fn receiver_is_this(expr: &Expr, tainted: &HashSet<String>) -> bool {
    match expr {
        Expr::Parenthesized { expression, .. } => receiver_is_this(expression, tainted),
        Expr::Identifier(ident) => ident.name == "this" || tainted.contains(&ident.name),
        Expr::Member(member) => receiver_is_this(&member.object, tainted),
        Expr::Index(index) => receiver_is_this(&index.object, tainted),
        _ => false,
    }
}

fn direct_receiver(expr: &Expr, tainted: &HashSet<String>) -> bool {
    match expr {
        Expr::Parenthesized { expression, .. } => direct_receiver(expression, tainted),
        Expr::Identifier(ident) => ident.name == "this" || tainted.contains(&ident.name),
        _ => false,
    }
}

struct Flow {
    assigned: Vec<bool>,
    tainted_vars: HashSet<String>,
    tainted_fields: Vec<bool>,
    diverges: bool,
}

fn walk_block(
    block: &BlockStmt,
    class: &ClassDecl,
    leaks: &[bool],
    checking: bool,
    flow: &mut Flow,
    issues: &mut Vec<(Span, String)>,
) {
    for statement in &block.statements {
        if flow.diverges {
            break;
        }
        walk_stmt(statement, class, leaks, checking, flow, issues);
    }
}

fn walk_stmt(
    stmt: &Stmt,
    class: &ClassDecl,
    leaks: &[bool],
    checking: bool,
    flow: &mut Flow,
    issues: &mut Vec<(Span, String)>,
) {
    match stmt {
        Stmt::Block(block) => walk_block(block, class, leaks, checking, flow, issues),
        Stmt::VariableDecl(decl) => {
            let carries_this = decl.initializer.as_ref().is_some_and(|initializer| {
                walk_expr(initializer, class, leaks, checking, flow, issues)
            });
            if carries_this {
                flow.tainted_vars.insert(decl.name.name.clone());
            } else {
                flow.tainted_vars.remove(&decl.name.name);
            }
        }
        Stmt::Destructure(decl) => {
            if walk_expr(&decl.initializer, class, leaks, checking, flow, issues) {
                escape(decl.span, issues);
            }
        }
        Stmt::Return(stmt) => {
            if let Some(value) = &stmt.value {
                if walk_expr(value, class, leaks, checking, flow, issues) {
                    escape(stmt.span, issues);
                }
            }
            if checking && flow.assigned.iter().any(|assigned| !assigned) {
                for (index, field) in class.fields.iter().enumerate() {
                    if !flow.assigned[index] {
                        issues.push((
                            stmt.span,
                            format!("field '{}' is not initialized", field.name.name),
                        ));
                    }
                }
            }
            flow.diverges = true;
        }
        Stmt::If(stmt) => {
            let _ = walk_expr(&stmt.condition, class, leaks, checking, flow, issues);
            let mut then_flow = clone_flow(flow);
            walk_block(
                &stmt.then_branch,
                class,
                leaks,
                checking,
                &mut then_flow,
                issues,
            );
            let else_stmt = stmt.else_branch.as_deref();
            let mut else_flow = clone_flow(flow);
            if let Some(branch) = else_stmt {
                walk_stmt(branch, class, leaks, checking, &mut else_flow, issues);
            }
            flow.tainted_vars
                .extend(then_flow.tainted_vars.iter().cloned());
            flow.tainted_vars
                .extend(else_flow.tainted_vars.iter().cloned());
            if checking {
                match (
                    then_flow.diverges,
                    else_stmt.is_some() && else_flow.diverges,
                ) {
                    (true, true) => flow.diverges = true,
                    (true, false) if else_stmt.is_some() => {
                        flow.assigned = else_flow.assigned;
                        flow.tainted_fields = else_flow.tainted_fields;
                    }
                    (false, true) => {
                        flow.assigned = then_flow.assigned;
                        flow.tainted_fields = then_flow.tainted_fields;
                    }
                    _ => {
                        for index in 0..flow.assigned.len() {
                            flow.assigned[index] =
                                then_flow.assigned[index] && else_flow.assigned[index];
                            flow.tainted_fields[index] =
                                then_flow.tainted_fields[index] || else_flow.tainted_fields[index];
                        }
                    }
                }
            }
        }
        Stmt::Loop(stmt) => {
            match &stmt.kind {
                LoopKind::Condition { condition } => {
                    let _ = walk_expr(condition, class, leaks, checking, flow, issues);
                }
                LoopKind::CStyle {
                    initializer,
                    init_expr,
                    condition,
                    update,
                } => {
                    if let Some(initializer) = initializer {
                        let carries_this = initializer.initializer.as_ref().is_some_and(|value| {
                            walk_expr(value, class, leaks, checking, flow, issues)
                        });
                        if carries_this {
                            flow.tainted_vars.insert(initializer.name.name.clone());
                        }
                    }
                    if let Some(expr) = init_expr {
                        let _ = walk_expr(expr, class, leaks, checking, flow, issues);
                    }
                    if let Some(expr) = condition {
                        let _ = walk_expr(expr, class, leaks, checking, flow, issues);
                    }
                    if let Some(expr) = update {
                        let _ = walk_expr(expr, class, leaks, checking, flow, issues);
                    }
                }
                LoopKind::Iteration { iterable, .. } => {
                    let _ = walk_expr(iterable, class, leaks, checking, flow, issues);
                }
                LoopKind::Infinite => {}
            }
            let saved = flow.assigned.clone();
            let saved_fields = flow.tainted_fields.clone();
            walk_block(&stmt.body, class, leaks, checking, flow, issues);
            if checking {
                flow.assigned = saved;
                flow.tainted_fields = saved_fields;
                flow.diverges = false;
            }
        }
        Stmt::Try(stmt) => {
            let saved = flow.assigned.clone();
            let saved_fields = flow.tainted_fields.clone();
            walk_block(&stmt.body, class, leaks, checking, flow, issues);
            for clause in &stmt.catches {
                walk_block(&clause.block, class, leaks, checking, flow, issues);
            }
            if let Some(finally_block) = &stmt.finally_block {
                walk_block(finally_block, class, leaks, checking, flow, issues);
            }
            if checking {
                flow.assigned = saved;
                flow.tainted_fields = saved_fields;
                flow.diverges = false;
            }
        }
        Stmt::Expression(stmt) => {
            let _ = walk_expr(&stmt.expression, class, leaks, checking, flow, issues);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn clone_flow(flow: &Flow) -> Flow {
    Flow {
        assigned: flow.assigned.clone(),
        tainted_vars: flow.tainted_vars.clone(),
        tainted_fields: flow.tainted_fields.clone(),
        diverges: false,
    }
}

fn walk_expr(
    expr: &Expr,
    class: &ClassDecl,
    leaks: &[bool],
    checking: bool,
    flow: &mut Flow,
    issues: &mut Vec<(Span, String)>,
) -> bool {
    match expr {
        Expr::Parenthesized { expression, .. } => {
            walk_expr(expression, class, leaks, checking, flow, issues)
        }
        Expr::Identifier(ident) => ident.name == "this" || flow.tainted_vars.contains(&ident.name),
        Expr::Assignment(assignment) => {
            let value_is_this = walk_expr(&assignment.value, class, leaks, checking, flow, issues);
            match assignment.target.as_ref() {
                Expr::Member(member) if is_this(&member.object, flow) => {
                    if let Some(index) = field_index(class, &member.member.name) {
                        if checking && class.fields[index].const_field && flow.assigned[index] {
                            issues.push((
                                member.span,
                                format!("cannot assign to constant field '{}'", member.member.name),
                            ));
                        }
                        flow.assigned[index] = true;
                        flow.tainted_fields[index] = value_is_this;
                    } else {
                        let _ = walk_expr(&member.object, class, leaks, checking, flow, issues);
                        if value_is_this {
                            escape(assignment.span, issues);
                        }
                    }
                    false
                }
                Expr::Identifier(ident) => {
                    if value_is_this {
                        flow.tainted_vars.insert(ident.name.clone());
                    } else {
                        flow.tainted_vars.remove(&ident.name);
                    }
                    false
                }
                other => {
                    let _ = walk_expr(other, class, leaks, checking, flow, issues);
                    if value_is_this {
                        escape(assignment.span, issues);
                    }
                    false
                }
            }
        }
        Expr::New(new_expr) => {
            let mut leaked = false;
            for argument in &new_expr.arguments {
                if walk_expr(argument, class, leaks, checking, flow, issues) {
                    leaked = true;
                }
            }
            if leaked {
                escape(new_expr.span, issues);
            }
            false
        }
        Expr::Call(call) => {
            if let Expr::Member(member) = call.callee.as_ref() {
                let object_is_this =
                    walk_expr(&member.object, class, leaks, checking, flow, issues);
                if object_is_this {
                    if let Some(index) = class
                        .methods
                        .iter()
                        .position(|method| method.name.name == member.member.name)
                    {
                        if checking && flow.assigned.iter().any(|assigned| !assigned) {
                            issues.push((
                                member.span,
                                format!(
                                    "cannot call method '{}' before every field is initialized",
                                    member.member.name
                                ),
                            ));
                        }
                        if leaks.get(index).copied().unwrap_or(false) {
                            escape(member.span, issues);
                        }
                    }
                }
                for argument in &call.arguments {
                    if walk_expr(argument, class, leaks, checking, flow, issues) {
                        escape(argument.span(), issues);
                    }
                }
                return false;
            }
            if walk_expr(&call.callee, class, leaks, checking, flow, issues) {
                escape(call.span, issues);
            }
            for argument in &call.arguments {
                if walk_expr(argument, class, leaks, checking, flow, issues) {
                    escape(argument.span(), issues);
                }
            }
            false
        }
        Expr::Array(array) => {
            let mut leaked = false;
            for element in &array.elements {
                if walk_expr(element, class, leaks, checking, flow, issues) {
                    leaked = true;
                }
            }
            if leaked {
                escape(array.span, issues);
            }
            false
        }
        Expr::Record(record) => {
            let mut leaked = false;
            for entry in &record.entries {
                let value = match entry {
                    BraceEntry::Pair { value, .. } | BraceEntry::Spread { value, .. } => value,
                };
                if walk_expr(value, class, leaks, checking, flow, issues) {
                    leaked = true;
                }
            }
            if leaked {
                escape(record.span, issues);
            }
            false
        }
        Expr::Member(member) => {
            let object_is_this = walk_expr(&member.object, class, leaks, checking, flow, issues);
            if object_is_this {
                if let Some(index) = field_index(class, &member.member.name) {
                    if checking && !flow.assigned[index] {
                        issues.push((
                            member.member.span,
                            format!(
                                "field '{}' is read before it is initialized",
                                member.member.name
                            ),
                        ));
                    }
                    return flow.tainted_fields.get(index).copied().unwrap_or(false);
                }
            }
            false
        }
        Expr::Unary(unary) => walk_expr(&unary.operand, class, leaks, checking, flow, issues),
        Expr::Binary(binary) => {
            let _ = walk_expr(&binary.left, class, leaks, checking, flow, issues);
            let _ = walk_expr(&binary.right, class, leaks, checking, flow, issues);
            false
        }
        Expr::Index(index) => {
            let _ = walk_expr(&index.object, class, leaks, checking, flow, issues);
            let _ = walk_expr(&index.index, class, leaks, checking, flow, issues);
            false
        }
        Expr::Slice(slice) => {
            let _ = walk_expr(&slice.object, class, leaks, checking, flow, issues);
            for bound in [&slice.start, &slice.stop, &slice.step] {
                if let Some(bound) = bound {
                    let _ = walk_expr(bound, class, leaks, checking, flow, issues);
                }
            }
            false
        }
        Expr::Conversion(conversion) => {
            let mut is_this = false;
            for argument in &conversion.arguments {
                if walk_expr(argument, class, leaks, checking, flow, issues) {
                    is_this = true;
                }
            }
            is_this
        }
        Expr::Update(update) => walk_expr(&update.operand, class, leaks, checking, flow, issues),
        Expr::InterpolatedString(interpolated) => {
            for part in &interpolated.parts {
                if let StringPart::Expression { expression, .. } = part {
                    let _ = walk_expr(expression, class, leaks, checking, flow, issues);
                }
            }
            false
        }
        Expr::Literal(_) => false,
    }
}

fn is_this(expr: &Expr, flow: &Flow) -> bool {
    match expr {
        Expr::Parenthesized { expression, .. } => is_this(expression, flow),
        Expr::Identifier(ident) => ident.name == "this" || flow.tainted_vars.contains(&ident.name),
        _ => false,
    }
}

fn field_index(class: &ClassDecl, name: &str) -> Option<usize> {
    class
        .fields
        .iter()
        .position(|field| field.name.name == name)
}

fn escape(span: Span, issues: &mut Vec<(Span, String)>) {
    let message = "this cannot escape before construction finishes";
    if issues
        .iter()
        .any(|(existing, text)| existing.start.offset == span.start.offset && text == message)
    {
        return;
    }
    issues.push((span, message.to_string()));
}
