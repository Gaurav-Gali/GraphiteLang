//! Semantic analysis.
//!
//! Signatures are collected before function bodies so top-level functions may
//! call functions declared later in the same file. Alias normalization happens
//! here. The parser keeps source spellings.
//!
//! `auto T` and explicit `T(value)` share one conversion registry. Approved
//! pairs become typed conversion nodes. Identity pairs stay unchanged.
//! Formatting of numbers, booleans, and characters uses the same helpers as
//! string interpolation. `chr` values are produced by conversion, not by a
//! character literal.
//!
//! A numeric literal may initialize a narrower numeric type of the same family
//! when the value fits exactly. Other expressions do not convert across
//! canonical numeric types.
//!
//! `&&` and `||` are parsed, then rejected here because short-circuit behavior
//! is not frozen. Integer overflow is not wrapped: code generation uses checked
//! arithmetic and aborts if an operation overflows. That abort is a guard, not
//! the frozen overflow policy. Floating-point arithmetic uses Rust's IEEE-754
//! operators.
//!
//! `lengthOf` is a reserved built-in. A call takes one `str`, array, or record
//! and yields `uint64`. Strings count Unicode scalar values, arrays count
//! elements, and records count the fields declared directly by their named
//! type. A variable, parameter, or function named `lengthOf` is rejected.
//!
//! Named records are nominal. A record literal needs an expected record type,
//! and later assignments to an `auto` record do not convert another record
//! into it. Field slots use ordinary typed compatibility.

use std::collections::HashMap;

use crate::manifest::{Manifest, ManifestType};
use crate::parser::*;
use crate::span::Span;

pub(crate) mod convert;
pub(crate) mod numeric;

use convert::{ConvOp, conversion};
use numeric::{CompareMode, NumericError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalType {
    Void,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Flt32,
    Flt64,
    Bln,
    Chr,
    Str,
    Record(RecordId),
    Map(MapId),
    Array {
        leaf: Leaf,
        depth: u8,
        /// `limits[i]` is dimension `i`, innermost first. `None` is dynamic.
        limits: [Option<u64>; MAX_ARRAY_RANK],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leaf {
    Void,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
    Flt32,
    Flt64,
    Bln,
    Chr,
    Str,
    Record(RecordId),
    Map(MapId),
}

impl CanonicalType {
    pub fn rust_name(self) -> String {
        match self {
            CanonicalType::Void => "()".to_string(),
            CanonicalType::Int16 => "i16".to_string(),
            CanonicalType::Int32 => "i32".to_string(),
            CanonicalType::Int64 => "i64".to_string(),
            CanonicalType::UInt16 => "u16".to_string(),
            CanonicalType::UInt32 => "u32".to_string(),
            CanonicalType::UInt64 => "u64".to_string(),
            CanonicalType::Flt32 => "f32".to_string(),
            CanonicalType::Flt64 => "f64".to_string(),
            CanonicalType::Bln => "bool".to_string(),
            CanonicalType::Chr => "char".to_string(),
            CanonicalType::Str => "String".to_string(),
            CanonicalType::Record(_) => "GrfRecord".to_string(),
            CanonicalType::Map(_) => "HashMap<(), ()>".to_string(),
            CanonicalType::Array { leaf, depth, .. } => {
                let mut name = leaf.rust_name();
                for _ in 0..depth {
                    name = format!("Vec<{name}>");
                }
                name
            }
        }
    }

    pub fn spelling(self) -> String {
        match self {
            CanonicalType::Void => "void".to_string(),
            CanonicalType::Int16 => "int16".to_string(),
            CanonicalType::Int32 => "int32".to_string(),
            CanonicalType::Int64 => "int64".to_string(),
            CanonicalType::UInt16 => "uint16".to_string(),
            CanonicalType::UInt32 => "uint32".to_string(),
            CanonicalType::UInt64 => "uint64".to_string(),
            CanonicalType::Flt32 => "flt32".to_string(),
            CanonicalType::Flt64 => "flt64".to_string(),
            CanonicalType::Bln => "bln".to_string(),
            CanonicalType::Chr => "chr".to_string(),
            CanonicalType::Str => "str".to_string(),
            CanonicalType::Record(_) => "record".to_string(),
            CanonicalType::Map(_) => "map".to_string(),
            CanonicalType::Array {
                leaf,
                depth,
                limits,
            } => {
                let mut name = leaf.spelling().to_string();
                for bound in limits.iter().take(depth as usize) {
                    match bound {
                        None => name.push_str("[]"),
                        Some(limit) => name.push_str(&format!("[{limit}]")),
                    }
                }
                name
            }
        }
    }

    pub fn element(self) -> Option<CanonicalType> {
        match self {
            CanonicalType::Array { leaf, depth: 1, .. } => Some(leaf.canonical()),
            CanonicalType::Array {
                leaf,
                depth,
                mut limits,
            } if depth > 1 => {
                limits[(depth - 1) as usize] = None;
                Some(CanonicalType::Array {
                    leaf,
                    depth: depth - 1,
                    limits,
                })
            }
            _ => None,
        }
    }

    pub fn limit(self) -> Option<u64> {
        match self {
            CanonicalType::Array { depth, limits, .. } if depth > 0 => limits[(depth - 1) as usize],
            _ => None,
        }
    }

    pub fn depth(self) -> u8 {
        match self {
            CanonicalType::Array { depth, .. } => depth,
            _ => 0,
        }
    }

    pub fn is_array(self) -> bool {
        matches!(self, CanonicalType::Array { .. })
    }

    pub fn is_void_array(self) -> bool {
        matches!(
            self,
            CanonicalType::Array {
                leaf: Leaf::Void,
                ..
            }
        )
    }

    pub fn is_heap(self) -> bool {
        matches!(
            self,
            CanonicalType::Str
                | CanonicalType::Record(_)
                | CanonicalType::Map(_)
                | CanonicalType::Array { .. }
        )
    }

    pub fn is_record(self) -> bool {
        matches!(self, CanonicalType::Record(_))
    }

    pub fn is_map(self) -> bool {
        matches!(self, CanonicalType::Map(_))
    }

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            CanonicalType::Int16 | CanonicalType::Int32 | CanonicalType::Int64
        )
    }

    pub fn is_unsigned(self) -> bool {
        matches!(
            self,
            CanonicalType::UInt16 | CanonicalType::UInt32 | CanonicalType::UInt64
        )
    }

    pub fn nest(self) -> CanonicalType {
        match self {
            CanonicalType::Array {
                leaf,
                depth,
                mut limits,
            } => {
                if depth as usize >= MAX_ARRAY_RANK {
                    return CanonicalType::Array {
                        leaf,
                        depth,
                        limits,
                    };
                }
                limits[depth as usize] = None;
                CanonicalType::Array {
                    leaf,
                    depth: depth + 1,
                    limits,
                }
            }
            other => CanonicalType::Array {
                leaf: other.leaf(),
                depth: 1,
                limits: [None; MAX_ARRAY_RANK],
            },
        }
    }

    fn leaf(self) -> Leaf {
        match self {
            CanonicalType::Void => Leaf::Void,
            CanonicalType::Int16 => Leaf::Int16,
            CanonicalType::Int32 => Leaf::Int32,
            CanonicalType::Int64 => Leaf::Int64,
            CanonicalType::UInt16 => Leaf::UInt16,
            CanonicalType::UInt32 => Leaf::UInt32,
            CanonicalType::UInt64 => Leaf::UInt64,
            CanonicalType::Flt32 => Leaf::Flt32,
            CanonicalType::Flt64 => Leaf::Flt64,
            CanonicalType::Bln => Leaf::Bln,
            CanonicalType::Chr => Leaf::Chr,
            CanonicalType::Str => Leaf::Str,
            CanonicalType::Record(id) => Leaf::Record(id),
            CanonicalType::Map(id) => Leaf::Map(id),
            CanonicalType::Array { leaf, .. } => leaf,
        }
    }

    pub(crate) fn is_integer(self) -> bool {
        matches!(
            self,
            CanonicalType::Int16
                | CanonicalType::Int32
                | CanonicalType::Int64
                | CanonicalType::UInt16
                | CanonicalType::UInt32
                | CanonicalType::UInt64
        )
    }

    pub(crate) fn is_float(self) -> bool {
        matches!(self, CanonicalType::Flt32 | CanonicalType::Flt64)
    }

    fn is_numeric(self) -> bool {
        self.is_integer() || self.is_float()
    }
}

impl Leaf {
    fn canonical(self) -> CanonicalType {
        match self {
            Leaf::Void => CanonicalType::Void,
            Leaf::Int16 => CanonicalType::Int16,
            Leaf::Int32 => CanonicalType::Int32,
            Leaf::Int64 => CanonicalType::Int64,
            Leaf::UInt16 => CanonicalType::UInt16,
            Leaf::UInt32 => CanonicalType::UInt32,
            Leaf::UInt64 => CanonicalType::UInt64,
            Leaf::Flt32 => CanonicalType::Flt32,
            Leaf::Flt64 => CanonicalType::Flt64,
            Leaf::Bln => CanonicalType::Bln,
            Leaf::Chr => CanonicalType::Chr,
            Leaf::Str => CanonicalType::Str,
            Leaf::Record(id) => CanonicalType::Record(id),
            Leaf::Map(id) => CanonicalType::Map(id),
        }
    }

    fn spelling(self) -> &'static str {
        match self {
            Leaf::Void => "void",
            Leaf::Int16 => "int16",
            Leaf::Int32 => "int32",
            Leaf::Int64 => "int64",
            Leaf::UInt16 => "uint16",
            Leaf::UInt32 => "uint32",
            Leaf::UInt64 => "uint64",
            Leaf::Flt32 => "flt32",
            Leaf::Flt64 => "flt64",
            Leaf::Bln => "bln",
            Leaf::Chr => "chr",
            Leaf::Str => "str",
            Leaf::Record(_) => "record",
            Leaf::Map(_) => "map",
        }
    }

    fn rust_name(self) -> String {
        match self {
            Leaf::Void => "()".to_string(),
            other => other.canonical().rust_name(),
        }
    }
}

pub fn canonical_type(kind: TypeKind, bounds: &[Option<u64>]) -> CanonicalType {
    let scalar = match kind {
        TypeKind::Void => CanonicalType::Void,
        TypeKind::Int | TypeKind::Int64 => CanonicalType::Int64,
        TypeKind::UInt | TypeKind::UInt64 => CanonicalType::UInt64,
        TypeKind::Flt | TypeKind::Flt64 => CanonicalType::Flt64,
        TypeKind::Int16 => CanonicalType::Int16,
        TypeKind::Int32 => CanonicalType::Int32,
        TypeKind::UInt16 => CanonicalType::UInt16,
        TypeKind::UInt32 => CanonicalType::UInt32,
        TypeKind::Flt32 => CanonicalType::Flt32,
        TypeKind::Bln => CanonicalType::Bln,
        TypeKind::Chr => CanonicalType::Chr,
        TypeKind::Str => CanonicalType::Str,
        TypeKind::Named | TypeKind::Map => CanonicalType::Void,
    };
    if bounds.is_empty() {
        return scalar;
    }
    let mut limits = [None; MAX_ARRAY_RANK];
    for (index, bound) in bounds.iter().take(MAX_ARRAY_RANK).enumerate() {
        limits[index] = *bound;
    }
    CanonicalType::Array {
        leaf: scalar.leaf(),
        depth: bounds.len().min(MAX_ARRAY_RANK) as u8,
        limits,
    }
}

fn canonical_name(name: &str) -> Option<CanonicalType> {
    Some(match name {
        "void" => CanonicalType::Void,
        "int" | "int64" => CanonicalType::Int64,
        "uint" | "uint64" => CanonicalType::UInt64,
        "flt" | "flt64" => CanonicalType::Flt64,
        "int16" => CanonicalType::Int16,
        "int32" => CanonicalType::Int32,
        "uint16" => CanonicalType::UInt16,
        "uint32" => CanonicalType::UInt32,
        "flt32" => CanonicalType::Flt32,
        "bln" => CanonicalType::Bln,
        "chr" => CanonicalType::Chr,
        "str" => CanonicalType::Str,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    pub source_name: String,
    pub span: Span,
    pub message: String,
}

impl std::fmt::Display for SemanticDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.source_name, self.span.start.line, self.span.start.column, self.message
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub program: Option<CheckedProgram>,
    pub diagnostics: Vec<SemanticDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub records: Vec<CheckedRecord>,
    pub maps: Vec<MapShape>,
    pub functions: Vec<CheckedFn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapShape {
    pub key: CanonicalType,
    pub value: CanonicalType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedRecord {
    pub name: String,
    pub fields: Vec<(String, CanonicalType)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedFn {
    pub name: String,
    pub params: Vec<(String, CanonicalType)>,
    pub ret: CanonicalType,
    pub body: Vec<CheckedStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedStmt {
    Let {
        name: String,
        ty: CanonicalType,
        value: CheckedExpr,
        mutable: bool,
    },
    Return(Option<CheckedExpr>),
    Expr(CheckedExpr),
    If {
        condition: CheckedExpr,
        then_body: Vec<CheckedStmt>,
        else_body: Option<Vec<CheckedStmt>>,
    },
    While {
        condition: CheckedExpr,
        body: Vec<CheckedStmt>,
    },
    Loop {
        body: Vec<CheckedStmt>,
    },
    CStyle {
        initializer: Option<Box<CheckedStmt>>,
        init_expr: Option<CheckedExpr>,
        condition: CheckedExpr,
        update: Option<CheckedExpr>,
        body: Vec<CheckedStmt>,
    },
    Break,
    Continue,
    Block {
        body: Vec<CheckedStmt>,
    },
    Destructure {
        bindings: Vec<(String, CanonicalType, bool)>,
        value: CheckedExpr,
        file: String,
        line: u32,
        column: u32,
    },
    Iterate {
        bindings: Vec<(String, CanonicalType, bool)>,
        source: CheckedExpr,
        kind: LoopSourceKind,
        body: Vec<CheckedStmt>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopSourceKind {
    Values,
    Enumerate,
    Entries,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrayPlace {
    Var(String),
    Field {
        object: Box<ArrayPlace>,
        field: String,
        ty: CanonicalType,
    },
    Index {
        object: Box<ArrayPlace>,
        index: Box<CheckedExpr>,
        file: String,
        line: u32,
        column: u32,
    },
    MapIndex {
        object: Box<ArrayPlace>,
        key: Box<CheckedExpr>,
        value_ty: CanonicalType,
        type_name: String,
        file: String,
        line: u32,
        column: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayMethod {
    Get,
    Set,
    Push,
    Insert,
    Remove,
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapMethod {
    Get,
    GetOrDefault,
    ContainsKey,
    Set,
    Remove,
    Length,
    Keys,
    Values,
    Entries,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapEntry {
    Pair(CheckedExpr, CheckedExpr),
    Spread(CheckedExpr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedExpr {
    Int {
        value: i128,
        ty: CanonicalType,
    },
    Float {
        lexeme: String,
        ty: CanonicalType,
    },
    Bool(bool),
    Str(String),
    Load {
        name: String,
        ty: CanonicalType,
    },
    Unary {
        op: UnaryOperator,
        expr: Box<CheckedExpr>,
        ty: CanonicalType,
    },
    Binary {
        op: BinaryOperator,
        left: Box<CheckedExpr>,
        right: Box<CheckedExpr>,
        ty: CanonicalType,
        compare: Option<CompareMode>,
    },
    NumCast {
        from: CanonicalType,
        to: CanonicalType,
        check: bool,
        expr: Box<CheckedExpr>,
        file: String,
        line: u32,
        column: u32,
    },
    Assign {
        name: String,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
    },
    Update {
        prefix: bool,
        increment: bool,
        name: String,
        ty: CanonicalType,
    },
    Call {
        path: String,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
    },
    LengthOf {
        expr: Box<CheckedExpr>,
        count: Option<u64>,
    },
    Record {
        id: RecordId,
        fields: Vec<(usize, CheckedExpr)>,
        ty: CanonicalType,
    },
    FieldGet {
        object: Box<CheckedExpr>,
        record: RecordId,
        field: usize,
        ty: CanonicalType,
    },
    FieldSet {
        place: ArrayPlace,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
    },
    Convert {
        from: CanonicalType,
        to: CanonicalType,
        op: ConvOp,
        expr: Box<CheckedExpr>,
        file: String,
        line: u32,
        column: u32,
    },
    Char(char),
    Array {
        elements: Vec<CheckedExpr>,
        ty: CanonicalType,
    },
    IndexGet {
        object: Box<CheckedExpr>,
        index: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
    },
    SliceGet {
        object: Box<CheckedExpr>,
        start: Option<Box<CheckedExpr>>,
        stop: Option<Box<CheckedExpr>>,
        step: Option<Box<CheckedExpr>>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
    },
    IndexSet {
        place: Option<ArrayPlace>,
        object: Option<Box<CheckedExpr>>,
        index: Box<CheckedExpr>,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
    },
    SliceSet {
        place: Option<ArrayPlace>,
        object: Option<Box<CheckedExpr>>,
        start: Option<Box<CheckedExpr>>,
        stop: Option<Box<CheckedExpr>>,
        step: Option<Box<CheckedExpr>>,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
        limit: Option<u64>,
        file: String,
        line: u32,
        column: u32,
    },
    ArrayCall {
        place: Option<ArrayPlace>,
        object: Option<Box<CheckedExpr>>,
        method: ArrayMethod,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        limit: Option<u64>,
        file: String,
        line: u32,
        column: u32,
    },
    MapLiteral {
        entries: Vec<MapEntry>,
        ty: CanonicalType,
    },
    MapGet {
        object: Box<CheckedExpr>,
        key: Box<CheckedExpr>,
        ty: CanonicalType,
        type_name: String,
        file: String,
        line: u32,
        column: u32,
    },
    MapInsert {
        place: ArrayPlace,
        key: Box<CheckedExpr>,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
    },
    MapCall {
        place: Option<ArrayPlace>,
        object: Option<Box<CheckedExpr>>,
        method: MapMethod,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        key_ty: CanonicalType,
        value_ty: CanonicalType,
        type_name: String,
        file: String,
        line: u32,
        column: u32,
    },
    Interpolated {
        parts: Vec<CheckedPart>,
    },
}

impl CheckedExpr {
    pub fn ty(&self) -> CanonicalType {
        match self {
            CheckedExpr::Int { ty, .. }
            | CheckedExpr::Float { ty, .. }
            | CheckedExpr::Load { ty, .. }
            | CheckedExpr::Unary { ty, .. }
            | CheckedExpr::Binary { ty, .. }
            | CheckedExpr::Assign { ty, .. }
            | CheckedExpr::Update { ty, .. }
            | CheckedExpr::Call { ty, .. } => *ty,
            CheckedExpr::Convert { to, .. } | CheckedExpr::NumCast { to, .. } => *to,
            CheckedExpr::Array { ty, .. }
            | CheckedExpr::Record { ty, .. }
            | CheckedExpr::FieldGet { ty, .. }
            | CheckedExpr::FieldSet { ty, .. }
            | CheckedExpr::IndexGet { ty, .. }
            | CheckedExpr::SliceGet { ty, .. }
            | CheckedExpr::IndexSet { ty, .. }
            | CheckedExpr::SliceSet { ty, .. }
            | CheckedExpr::ArrayCall { ty, .. }
            | CheckedExpr::MapLiteral { ty, .. }
            | CheckedExpr::MapGet { ty, .. }
            | CheckedExpr::MapInsert { ty, .. }
            | CheckedExpr::MapCall { ty, .. } => *ty,
            CheckedExpr::LengthOf { .. } => CanonicalType::UInt64,
            CheckedExpr::Bool(_) => CanonicalType::Bln,
            CheckedExpr::Char(_) => CanonicalType::Chr,
            CheckedExpr::Str(_) | CheckedExpr::Interpolated { .. } => CanonicalType::Str,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckedPart {
    Text(String),
    Expr(CheckedExpr),
}

struct Signature {
    params: Vec<ParamInfo>,
    ret: CanonicalType,
    span: Span,
}

struct ParamInfo {
    name: String,
    ty: CanonicalType,
    auto: bool,
}

#[derive(Clone)]
struct ImportBinding {
    domain: String,
    library: String,
}

#[derive(Clone, Copy)]
struct Binding {
    ty: CanonicalType,
    mutable: bool,
    auto: bool,
}

pub fn analyze(source_name: &str, program: &Program, manifest: &Manifest) -> Analysis {
    let mut checker = Checker {
        source_name,
        manifest,
        diagnostics: Vec::new(),
        functions: HashMap::new(),
        imports: HashMap::new(),
        records: Vec::new(),
        record_names: HashMap::new(),
        maps: Vec::new(),
        resolve_stack: Vec::new(),
        scopes: Vec::new(),
        loop_depth: 0,
        current_return: CanonicalType::Void,
        iterating: Vec::new(),
    };
    checker.collect_imports(program);
    checker.collect_records(program);
    checker.collect_functions(program);
    checker.check_entry_point(program);
    let mut functions = Vec::new();
    if checker.diagnostics.is_empty() {
        for function in &program.functions {
            if let Some(checked) = checker.check_function(function) {
                functions.push(checked);
            }
        }
    } else {
        for function in &program.functions {
            let _ = checker.check_function(function);
        }
    }
    Analysis {
        program: if checker.diagnostics.is_empty() {
            Some(CheckedProgram {
                records: checker
                    .records
                    .iter()
                    .map(|record| CheckedRecord {
                        name: record.name.clone(),
                        fields: record
                            .fields
                            .iter()
                            .map(|field| (field.name.clone(), field.ty))
                            .collect(),
                    })
                    .collect(),
                maps: checker.maps,
                functions,
            })
        } else {
            None
        },
        diagnostics: checker.diagnostics,
    }
}

struct RecordInfo {
    name: String,
    fields: Vec<FieldInfo>,
    span: Span,
    resolving: bool,
    done: bool,
    reported_cycle: bool,
}

struct FieldInfo {
    name: String,
    syntax: TypeSyntax,
    ty: CanonicalType,
    span: Span,
}

struct Checker<'a> {
    source_name: &'a str,
    manifest: &'a Manifest,
    diagnostics: Vec<SemanticDiagnostic>,
    functions: HashMap<String, Signature>,
    imports: HashMap<String, ImportBinding>,
    records: Vec<RecordInfo>,
    record_names: HashMap<String, RecordId>,
    maps: Vec<MapShape>,
    resolve_stack: Vec<RecordId>,
    scopes: Vec<HashMap<String, Binding>>,
    loop_depth: u32,
    current_return: CanonicalType,
    iterating: Vec<IterLock>,
}

#[derive(Clone)]
struct IterLock {
    name: String,
    scope: usize,
}

impl<'a> Checker<'a> {
    fn collect_imports(&mut self, program: &Program) {
        let mut seen_paths = Vec::<(String, String, Span)>::new();
        for import in &program.imports {
            let path = (import.domain.name.clone(), import.library.name.clone());
            if let Some((_, _, first)) = seen_paths
                .iter()
                .find(|(domain, library, _)| *domain == path.0 && *library == path.1)
            {
                self.error(
                    import.span,
                    format!(
                        "'{}.{}' was imported more than once; the first import is at line {}",
                        path.0, path.1, first.start.line
                    ),
                );
                continue;
            }
            if self.imports.contains_key(&import.library.name) {
                self.error(
                    import.span,
                    format!(
                        "import '{}.{}' conflicts with an earlier import that also binds the name '{}'",
                        path.0, path.1, import.library.name
                    ),
                );
                continue;
            }
            seen_paths.push((path.0.clone(), path.1.clone(), import.span));
            if self.manifest.domain != import.domain.name {
                self.error(
                    import.domain.span,
                    format!("domain '{}' was not found", import.domain.name),
                );
                continue;
            }
            if !self.manifest.libraries.contains_key(&import.library.name) {
                self.error(
                    import.library.span,
                    format!(
                        "library '{}' was not found in domain '{}'",
                        import.library.name, import.domain.name
                    ),
                );
                continue;
            }
            self.imports.insert(
                import.library.name.clone(),
                ImportBinding {
                    domain: path.0,
                    library: path.1,
                },
            );
        }
    }

    fn collect_records(&mut self, program: &Program) {
        for declaration in &program.types {
            if declaration.name.name == "lengthOf" {
                self.error(
                    declaration.name.span,
                    "'lengthOf' is a reserved built-in name",
                );
            }
            if self.record_names.contains_key(&declaration.name.name) {
                let previous = self.record_names[&declaration.name.name];
                let line = self.records[previous.0 as usize].span.start.line;
                self.error(
                    declaration.name.span,
                    format!(
                        "record type '{}' is already declared at line {line}",
                        declaration.name.name
                    ),
                );
                continue;
            }
            if declaration.fields.is_empty() {
                self.error(
                    declaration.span,
                    "a record type must declare at least one field",
                );
            }
            let mut seen = HashMap::new();
            let mut fields = Vec::new();
            for field in &declaration.fields {
                if field.name.name == "lengthOf" {
                    self.error(field.name.span, "'lengthOf' is a reserved built-in name");
                }
                if seen.insert(field.name.name.clone(), field.span).is_some() {
                    self.error(
                        field.name.span,
                        format!("duplicate field '{}'", field.name.name),
                    );
                    continue;
                }
                fields.push(FieldInfo {
                    name: field.name.name.clone(),
                    syntax: field.ty.clone(),
                    ty: CanonicalType::Void,
                    span: field.span,
                });
            }
            let id = RecordId(self.records.len() as u32);
            self.record_names.insert(declaration.name.name.clone(), id);
            self.records.push(RecordInfo {
                name: declaration.name.name.clone(),
                fields,
                span: declaration.span,
                resolving: false,
                done: false,
                reported_cycle: false,
            });
        }
        let ids = (0..self.records.len())
            .map(|index| RecordId(index as u32))
            .collect::<Vec<_>>();
        for id in ids {
            self.finish_record(id);
        }
    }

    fn finish_record(&mut self, id: RecordId) {
        let index = id.0 as usize;
        if self.records[index].done || self.records[index].resolving {
            return;
        }
        self.records[index].resolving = true;
        self.resolve_stack.push(id);
        let syntaxes = self.records[index]
            .fields
            .iter()
            .map(|field| (field.syntax.clone(), field.span))
            .collect::<Vec<_>>();
        let mut types = Vec::new();
        for (syntax, span) in &syntaxes {
            let ty = self.resolve_type(syntax);
            if ty == CanonicalType::Void {
                self.error(*span, "void is not a valid field type");
            }
            if ty.is_void_array() {
                self.error(*span, "void is not a valid array element type");
            }
            types.push(ty);
        }
        for (field, ty) in self.records[index].fields.iter_mut().zip(types) {
            field.ty = ty;
        }
        self.resolve_stack.pop();
        self.records[index].resolving = false;
        self.records[index].done = true;
    }

    fn resolve_type(&mut self, syntax: &TypeSyntax) -> CanonicalType {
        if syntax.kind == TypeKind::Map {
            let key = syntax
                .arguments
                .first()
                .map(|key| self.resolve_type(key))
                .unwrap_or(CanonicalType::Void);
            let value = syntax
                .arguments
                .get(1)
                .map(|value| self.resolve_type(value))
                .unwrap_or(CanonicalType::Void);
            if syntax.arguments.len() != 2 {
                self.error(syntax.span, "map type requires a key type and a value type");
            }
            self.validate_map(key, value, syntax.span);
            let id = self.intern_map(key, value);
            return self.with_bounds(CanonicalType::Map(id), &syntax.bounds);
        }
        let scalar = if syntax.kind == TypeKind::Named {
            let Some(id) = self.record_names.get(&syntax.name).copied() else {
                self.error(syntax.span, format!("unknown type '{}'", syntax.name));
                return self.with_bounds(CanonicalType::Record(RecordId(u32::MAX)), &syntax.bounds);
            };
            if self.records[id.0 as usize].resolving {
                self.report_cycle(id, syntax.span);
                CanonicalType::Record(id)
            } else {
                self.finish_record(id);
                CanonicalType::Record(id)
            }
        } else {
            canonical_type(syntax.kind, &[])
        };
        self.with_bounds(scalar, &syntax.bounds)
    }

    fn with_bounds(&self, scalar: CanonicalType, bounds: &[Option<u64>]) -> CanonicalType {
        if bounds.is_empty() {
            return scalar;
        }
        let mut limits = [None; MAX_ARRAY_RANK];
        for (index, bound) in bounds.iter().take(MAX_ARRAY_RANK).enumerate() {
            limits[index] = *bound;
        }
        CanonicalType::Array {
            leaf: scalar.leaf(),
            depth: bounds.len().min(MAX_ARRAY_RANK) as u8,
            limits,
        }
    }

    fn report_cycle(&mut self, repeated: RecordId, span: Span) {
        if self.records[repeated.0 as usize].reported_cycle {
            return;
        }
        self.records[repeated.0 as usize].reported_cycle = true;
        let mut names = Vec::new();
        if let Some(start) = self.resolve_stack.iter().position(|id| *id == repeated) {
            for id in &self.resolve_stack[start..] {
                names.push(self.records[id.0 as usize].name.clone());
            }
        }
        names.push(self.records[repeated.0 as usize].name.clone());
        self.error(span, format!("record type cycle: {}", names.join(" -> ")));
    }

    fn spell(&self, ty: CanonicalType) -> String {
        match ty {
            CanonicalType::Record(id) => self.record_name(id),
            CanonicalType::Map(id) => {
                let shape = self.maps.get(id.0 as usize).cloned().unwrap_or(MapShape {
                    key: CanonicalType::Void,
                    value: CanonicalType::Void,
                });
                format!(
                    "map<{}, {}>",
                    self.spell(shape.key),
                    self.spell(shape.value)
                )
            }
            CanonicalType::Array {
                leaf: Leaf::Record(id),
                depth,
                limits,
            } => {
                let mut name = self.record_name(id);
                for bound in limits.iter().take(depth as usize) {
                    match bound {
                        None => name.push_str("[]"),
                        Some(limit) => name.push_str(&format!("[{limit}]")),
                    }
                }
                name
            }
            CanonicalType::Array {
                leaf: Leaf::Map(id),
                depth,
                limits,
            } => {
                let mut name = self.spell(CanonicalType::Map(id));
                for bound in limits.iter().take(depth as usize) {
                    match bound {
                        None => name.push_str("[]"),
                        Some(limit) => name.push_str(&format!("[{limit}]")),
                    }
                }
                name
            }
            other => other.spelling(),
        }
    }

    fn map_parts(&self, ty: CanonicalType) -> (CanonicalType, CanonicalType) {
        if let CanonicalType::Map(id) = ty {
            self.maps
                .get(id.0 as usize)
                .map(|shape| (shape.key, shape.value))
                .unwrap_or((CanonicalType::Int64, CanonicalType::Int64))
        } else {
            (CanonicalType::Int64, CanonicalType::Int64)
        }
    }

    fn intern_map(&mut self, key: CanonicalType, value: CanonicalType) -> MapId {
        if let Some(index) = self
            .maps
            .iter()
            .position(|shape| shape.key == key && shape.value == value)
        {
            return MapId(index as u32);
        }
        let id = MapId(self.maps.len() as u32);
        self.maps.push(MapShape { key, value });
        id
    }

    fn validate_map(&mut self, key: CanonicalType, value: CanonicalType, span: Span) {
        if value == CanonicalType::Void || value.is_void_array() {
            self.error(span, "void is not a valid map value type");
        }
        if !Self::hashable_key(key) {
            self.error(span, format!("'{}' cannot be a map key", self.spell(key)));
        }
    }

    fn hashable_key(key: CanonicalType) -> bool {
        matches!(
            key,
            CanonicalType::Str
                | CanonicalType::Bln
                | CanonicalType::Chr
                | CanonicalType::Int16
                | CanonicalType::Int32
                | CanonicalType::Int64
                | CanonicalType::UInt16
                | CanonicalType::UInt32
                | CanonicalType::UInt64
        )
    }

    fn accepts_brace(&self, ty: CanonicalType) -> bool {
        ty.is_record()
            || ty.is_map()
            || matches!(
                ty,
                CanonicalType::Array {
                    leaf: Leaf::Record(_) | Leaf::Map(_),
                    ..
                }
            )
    }

    fn record_name(&self, id: RecordId) -> String {
        self.records
            .get(id.0 as usize)
            .map(|record| record.name.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn field_index(&self, id: RecordId, name: &str) -> Option<(usize, CanonicalType)> {
        let record = self.records.get(id.0 as usize)?;
        record
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name)
            .map(|(index, field)| (index, field.ty))
    }

    fn collect_functions(&mut self, program: &Program) {
        for function in &program.functions {
            let ret = self.resolve_type(&function.return_type);
            let mut params = Vec::new();
            let mut names = HashMap::new();
            for parameter in &function.parameters {
                let ty = self.resolve_type(&parameter.ty);
                if ty == CanonicalType::Void {
                    self.error(parameter.ty.span, "a parameter cannot have type void");
                }
                if ty.is_void_array() {
                    self.error(parameter.ty.span, "void is not a valid array element type");
                }
                let _ = self.reject_unspecified_type(ty, parameter.ty.span);
                if names
                    .insert(parameter.name.name.clone(), parameter.span)
                    .is_some()
                {
                    self.error(
                        parameter.name.span,
                        format!("duplicate parameter '{}'", parameter.name.name),
                    );
                }
                if parameter.name.name == "lengthOf" {
                    self.error(
                        parameter.name.span,
                        "'lengthOf' is a reserved built-in name",
                    );
                }
                params.push(ParamInfo {
                    name: parameter.name.name.clone(),
                    ty,
                    auto: parameter.conversion == ConversionMode::Auto,
                });
            }
            self.reject_unspecified_type(ret, function.return_type.span);
            if ret.is_void_array() {
                self.error(
                    function.return_type.span,
                    "void is not a valid array element type",
                );
            }
            if function.name.name == "lengthOf" {
                self.error(function.name.span, "'lengthOf' is a reserved built-in name");
                continue;
            }
            if let Some(previous) = self.functions.get(&function.name.name) {
                if function.name.name == "main" {
                    self.error(
                        function.name.span,
                        format!(
                            "duplicate entry point 'main'; the first declaration is at line {}",
                            previous.span.start.line
                        ),
                    );
                } else {
                    self.error(
                        function.name.span,
                        format!(
                            "function '{}' is already declared at line {}",
                            function.name.name, previous.span.start.line
                        ),
                    );
                }
                continue;
            }
            self.functions.insert(
                function.name.name.clone(),
                Signature {
                    params,
                    ret,
                    span: function.name.span,
                },
            );
        }
    }

    fn check_entry_point(&mut self, program: &Program) {
        let mains: Vec<_> = program
            .functions
            .iter()
            .filter(|function| function.name.name == "main")
            .collect();
        if mains.is_empty() {
            self.error(
                program.span,
                "program is missing the required void main() entry point",
            );
            return;
        }
        for main in mains {
            let ret = self.resolve_type(&main.return_type);
            if ret != CanonicalType::Void {
                self.error(main.return_type.span, "'main' must return void");
            }
            if !main.parameters.is_empty() {
                self.error(main.name.span, "'main' cannot have parameters");
            }
        }
    }

    fn check_function(&mut self, function: &FunctionDecl) -> Option<CheckedFn> {
        let signature = self.functions.get(&function.name.name)?;
        let params = signature
            .params
            .iter()
            .map(|param| (param.name.clone(), param.ty, param.auto))
            .collect::<Vec<_>>();
        let ret = signature.ret;
        self.current_return = ret;
        self.scopes.push(HashMap::new());
        for (name, ty, auto) in &params {
            if name == "lengthOf" {
                continue;
            }
            if *ty != CanonicalType::Void {
                self.scopes.last_mut().unwrap().insert(
                    name.clone(),
                    Binding {
                        ty: *ty,
                        mutable: true,
                        auto: *auto,
                    },
                );
            }
        }
        let params = params
            .into_iter()
            .map(|(name, ty, _)| (name, ty))
            .collect::<Vec<_>>();
        let mut body = Vec::new();
        for statement in &function.body.statements {
            if let Some(checked) = self.check_stmt(statement) {
                body.push(checked);
            }
        }
        if ret != CanonicalType::Void && !guaranteed_return(&function.body.statements) {
            self.error(
                function.span,
                format!(
                    "function '{}' must return a value on every path",
                    function.name.name
                ),
            );
        }
        self.scopes.pop();
        Some(CheckedFn {
            name: function.name.name.clone(),
            params,
            ret,
            body,
        })
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Option<CheckedStmt> {
        match stmt {
            Stmt::Block(block) => Some(CheckedStmt::Block {
                body: self.check_block_stmts(block),
            }),
            Stmt::VariableDecl(decl) => self.check_variable(decl).map(|let_stmt| let_stmt),
            Stmt::Destructure(decl) => self.check_destructure(decl),
            Stmt::Return(stmt) => {
                if self.current_return == CanonicalType::Void {
                    if stmt.value.is_some() {
                        self.error(stmt.span, "a void function cannot return a value");
                    }
                    Some(CheckedStmt::Return(None))
                } else if let Some(value) = &stmt.value {
                    let checked = self.check_expr(value, Some(self.current_return), false);
                    let converted =
                        self.apply_conversion(checked, self.current_return, false, value.span());
                    Some(CheckedStmt::Return(Some(converted)))
                } else {
                    self.error(
                        stmt.span,
                        format!("expected a {} value", self.spell(self.current_return)),
                    );
                    Some(CheckedStmt::Return(None))
                }
            }
            Stmt::If(stmt) => {
                let condition = self.check_expr(&stmt.condition, Some(CanonicalType::Bln), false);
                if condition.ty() != CanonicalType::Bln {
                    self.error(stmt.condition.span(), "an if condition must have type bln");
                }
                let then_body = self.check_block_stmts(&stmt.then_branch);
                let else_body = stmt
                    .else_branch
                    .as_ref()
                    .map(|branch| self.check_else(branch));
                Some(CheckedStmt::If {
                    condition,
                    then_body,
                    else_body,
                })
            }
            Stmt::Loop(stmt) => self.check_loop(stmt),
            Stmt::Break(span) => {
                if self.loop_depth == 0 {
                    self.error(*span, "break is outside a loop");
                }
                Some(CheckedStmt::Break)
            }
            Stmt::Continue(span) => {
                if self.loop_depth == 0 {
                    self.error(*span, "continue is outside a loop");
                }
                Some(CheckedStmt::Continue)
            }
            Stmt::Expression(stmt) => Some(CheckedStmt::Expr(self.check_expr(
                &stmt.expression,
                None,
                true,
            ))),
        }
    }

    fn check_else(&mut self, stmt: &Stmt) -> Vec<CheckedStmt> {
        match stmt {
            Stmt::Block(block) => self.check_block_stmts(block),
            Stmt::If(_) => self.check_stmt(stmt).into_iter().collect(),
            other => self.check_stmt(other).into_iter().collect(),
        }
    }

    fn check_block_stmts(&mut self, block: &BlockStmt) -> Vec<CheckedStmt> {
        self.scopes.push(HashMap::new());
        let body = block
            .statements
            .iter()
            .filter_map(|statement| self.check_stmt(statement))
            .collect();
        self.scopes.pop();
        body
    }

    fn check_loop(&mut self, stmt: &LoopStmt) -> Option<CheckedStmt> {
        match &stmt.kind {
            LoopKind::Infinite => {
                self.loop_depth += 1;
                let body = self.check_block_stmts(&stmt.body);
                self.loop_depth -= 1;
                Some(CheckedStmt::Loop { body })
            }
            LoopKind::Condition { condition } => {
                let checked = self.check_expr(condition, Some(CanonicalType::Bln), false);
                if checked.ty() != CanonicalType::Bln {
                    self.error(condition.span(), "a loop condition must have type bln");
                }
                self.loop_depth += 1;
                let body = self.check_block_stmts(&stmt.body);
                self.loop_depth -= 1;
                Some(CheckedStmt::While {
                    condition: checked,
                    body,
                })
            }
            LoopKind::CStyle {
                initializer,
                init_expr,
                condition,
                update,
            } => {
                self.scopes.push(HashMap::new());
                let init = initializer
                    .as_ref()
                    .and_then(|decl| self.check_variable(decl))
                    .map(Box::new);
                let init_expr = init_expr
                    .as_ref()
                    .map(|expr| self.check_expr(expr, None, true));
                let checked_condition = if let Some(condition) = condition {
                    let checked = self.check_expr(condition, Some(CanonicalType::Bln), false);
                    if checked.ty() != CanonicalType::Bln {
                        self.error(condition.span(), "a loop condition must have type bln");
                    }
                    checked
                } else {
                    self.error(stmt.span, "a C-style loop requires a condition");
                    CheckedExpr::Bool(false)
                };
                let checked_update = update
                    .as_ref()
                    .map(|expr| self.check_expr(expr, None, true));
                self.loop_depth += 1;
                let body = self.check_block_stmts(&stmt.body);
                self.loop_depth -= 1;
                self.scopes.pop();
                Some(CheckedStmt::CStyle {
                    initializer: init,
                    init_expr,
                    condition: checked_condition,
                    update: checked_update,
                    body,
                })
            }
            LoopKind::Iteration { bindings, iterable } => {
                self.check_iteration(bindings, iterable, stmt)
            }
        }
    }

    fn check_variable(&mut self, decl: &VariableDecl) -> Option<CheckedStmt> {
        let ty = self.resolve_type(&decl.ty);
        if ty == CanonicalType::Void {
            self.error(decl.ty.span, "a variable cannot have type void");
            return None;
        }
        if ty.is_void_array() {
            self.error(decl.ty.span, "void is not a valid array element type");
            return None;
        }
        if self.reject_unspecified_type(ty, decl.ty.span) {
            return None;
        }
        let initializer = &decl.initializer;
        let value = self.check_expr(initializer, Some(ty), false);
        let value = self.apply_conversion(
            value,
            ty,
            decl.conversion == ConversionMode::Auto,
            initializer.span(),
        );
        if decl.name.name == "lengthOf" {
            self.error(decl.name.span, "'lengthOf' is a reserved built-in name");
            return None;
        }
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(&decl.name.name))
        {
            self.error(
                decl.name.span,
                format!("'{}' is already declared in this scope", decl.name.name),
            );
            return None;
        }
        self.scopes.last_mut().unwrap().insert(
            decl.name.name.clone(),
            Binding {
                ty,
                mutable: decl.mutable,
                auto: decl.conversion == ConversionMode::Auto,
            },
        );
        Some(CheckedStmt::Let {
            name: decl.name.name.clone(),
            ty,
            value,
            mutable: decl.mutable,
        })
    }

    fn check_expr(
        &mut self,
        expr: &Expr,
        expected: Option<CanonicalType>,
        allow_void: bool,
    ) -> CheckedExpr {
        let checked = match expr {
            Expr::Parenthesized { expression, .. } => {
                return self.check_expr(expression, expected, allow_void);
            }
            Expr::Literal(LiteralExpr::Integer { lexeme, span }) => {
                self.integer_literal(lexeme, expected, *span)
            }
            Expr::Literal(LiteralExpr::Float { lexeme, span }) => {
                self.float_literal(lexeme, expected, *span)
            }
            Expr::Literal(LiteralExpr::Boolean { value, .. }) => CheckedExpr::Bool(*value),
            Expr::Literal(LiteralExpr::String { value, .. }) => CheckedExpr::Str(value.clone()),
            Expr::Identifier(ident) => self.load(ident),
            Expr::Unary(expr) => self.check_unary(expr, expected),
            Expr::Binary(expr) => self.check_binary(expr, expected),
            Expr::Assignment(expr) => self.check_assignment(expr),
            Expr::Update(expr) => self.check_update(expr),
            Expr::Call(expr) => self.check_call(expr, allow_void),
            Expr::Conversion(expr) => self.check_conversion(expr),
            Expr::Array(expr) => self.check_array_literal(expr, expected),
            Expr::Record(expr) => self.check_record_literal(expr, expected),
            Expr::Index(expr) => self.check_index(expr),
            Expr::Slice(expr) => self.check_slice(expr),
            Expr::Member(expr) => self.check_member(expr),
            Expr::InterpolatedString(expr) => self.check_interpolation(expr),
        };
        if !allow_void && checked.ty() == CanonicalType::Void {
            self.error(expr.span(), "a void value cannot be used here");
        }
        checked
    }

    fn check_interpolation(&mut self, expr: &InterpolatedStringExpr) -> CheckedExpr {
        let mut parts = Vec::new();
        for part in &expr.parts {
            match part {
                StringPart::Text { value, .. } => parts.push(CheckedPart::Text(value.clone())),
                StringPart::Expression { expression, .. } => {
                    let checked = self.check_expr(expression, None, false);
                    let converted =
                        self.apply_conversion(checked, CanonicalType::Str, true, expression.span());
                    parts.push(CheckedPart::Expr(converted));
                }
            }
        }
        CheckedExpr::Interpolated { parts }
    }

    fn check_unary(&mut self, expr: &UnaryExpr, expected: Option<CanonicalType>) -> CheckedExpr {
        if expr.operator == UnaryOperator::Negate {
            if let Some(folded) = self.negated_literal(&expr.operand, expected, expr.span) {
                return folded;
            }
        }
        let operand_expected = match expr.operator {
            UnaryOperator::Not => Some(CanonicalType::Bln),
            UnaryOperator::Negate => expected.filter(|ty| ty.is_numeric()),
        };
        let operand = self.check_expr(&expr.operand, operand_expected, false);
        match expr.operator {
            UnaryOperator::Not => {
                if operand.ty() != CanonicalType::Bln {
                    self.error(expr.span, "operator '!' requires type bln");
                }
                CheckedExpr::Unary {
                    op: UnaryOperator::Not,
                    ty: CanonicalType::Bln,
                    expr: Box::new(operand),
                }
            }
            UnaryOperator::Negate => {
                if !operand.ty().is_numeric() {
                    self.error(expr.span, "unary '-' requires a numeric operand");
                }
                CheckedExpr::Unary {
                    op: UnaryOperator::Negate,
                    ty: operand.ty(),
                    expr: Box::new(operand),
                }
            }
        }
    }

    fn check_binary(&mut self, expr: &BinaryExpr, expected: Option<CanonicalType>) -> CheckedExpr {
        if matches!(expr.operator, BinaryOperator::And | BinaryOperator::Or) {
            let left = self.check_expr(&expr.left, Some(CanonicalType::Bln), false);
            let right = self.check_expr(&expr.right, Some(CanonicalType::Bln), false);
            self.error(
                expr.span,
                "the short-circuit behavior of '&&' and '||' is not specified yet",
            );
            return CheckedExpr::Binary {
                op: expr.operator,
                ty: CanonicalType::Bln,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
            };
        }
        let comparison = matches!(
            expr.operator,
            BinaryOperator::Less
                | BinaryOperator::LessEqual
                | BinaryOperator::Greater
                | BinaryOperator::GreaterEqual
                | BinaryOperator::Equal
                | BinaryOperator::NotEqual
        );
        let (left, right) = self.check_numeric_operands(&expr.left, &expr.right, expected);
        if matches!(
            expr.operator,
            BinaryOperator::Equal | BinaryOperator::NotEqual
        ) && (left.ty().is_record()
            || right.ty().is_record()
            || left.ty().is_map()
            || right.ty().is_map())
        {
            let kind = if left.ty().is_map() || right.ty().is_map() {
                "map"
            } else {
                "record"
            };
            self.error(
                expr.span,
                format!(
                    "operator '{}' cannot compare {kind} values",
                    operator_token(expr.operator)
                ),
            );
            return CheckedExpr::Binary {
                op: expr.operator,
                ty: CanonicalType::Bln,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
            };
        }
        if comparison {
            self.finish_comparison(expr, left, right)
        } else {
            self.finish_arithmetic(expr, left, right)
        }
    }

    fn check_numeric_operands(
        &mut self,
        left: &Expr,
        right: &Expr,
        expected: Option<CanonicalType>,
    ) -> (CheckedExpr, CheckedExpr) {
        if is_numeric_literal(left) && is_numeric_literal(right) {
            let literal_type = expected.filter(|ty| ty.is_numeric());
            let left = self.check_expr(left, literal_type, false);
            let right = self.check_expr(right, Some(left.ty()), false);
            (left, right)
        } else if is_numeric_literal(left) {
            let right = self.check_expr(right, None, false);
            let left = self.check_expr(left, Some(right.ty()), false);
            (left, right)
        } else {
            let left = self.check_expr(left, None, false);
            let right_type = if is_numeric_literal(right) {
                Some(left.ty())
            } else {
                None
            };
            let right = self.check_expr(right, right_type, false);
            (left, right)
        }
    }

    fn finish_comparison(
        &mut self,
        expr: &BinaryExpr,
        left: CheckedExpr,
        right: CheckedExpr,
    ) -> CheckedExpr {
        let compare = if left.ty() == right.ty()
            && (left.ty().is_numeric()
                || left.ty() == CanonicalType::Bln
                || left.ty() == CanonicalType::Str)
        {
            Some(CompareMode::Same)
        } else {
            numeric::compare(left.ty(), right.ty())
        };
        if compare.is_none() {
            self.error(
                expr.span,
                format!(
                    "operator '{}' requires two operands of the same comparable type",
                    operator_token(expr.operator)
                ),
            );
        }
        CheckedExpr::Binary {
            op: expr.operator,
            ty: CanonicalType::Bln,
            left: Box::new(left),
            right: Box::new(right),
            compare,
        }
    }

    fn finish_arithmetic(
        &mut self,
        expr: &BinaryExpr,
        left: CheckedExpr,
        right: CheckedExpr,
    ) -> CheckedExpr {
        let remainder = expr.operator == BinaryOperator::Remainder;
        match numeric::arithmetic(left.ty(), right.ty(), remainder) {
            Ok(ty) => {
                let left = self.promote(left, ty, expr.left.span());
                let right = self.promote(right, ty, expr.right.span());
                CheckedExpr::Binary {
                    op: expr.operator,
                    ty,
                    left: Box::new(left),
                    right: Box::new(right),
                    compare: None,
                }
            }
            Err(NumericError::Remainder) => {
                self.error(expr.span, "operator '%' requires integer operands");
                self.binary_error(expr, left, right)
            }
            Err(NumericError::Incompatible) => {
                self.error(
                    expr.span,
                    format!(
                        "operator '{}' requires two operands of the same numeric type",
                        operator_token(expr.operator)
                    ),
                );
                self.binary_error(expr, left, right)
            }
        }
    }

    fn binary_error(
        &self,
        expr: &BinaryExpr,
        left: CheckedExpr,
        right: CheckedExpr,
    ) -> CheckedExpr {
        CheckedExpr::Binary {
            op: expr.operator,
            ty: left.ty(),
            left: Box::new(left),
            right: Box::new(right),
            compare: None,
        }
    }

    fn promote(&mut self, expr: CheckedExpr, to: CanonicalType, span: Span) -> CheckedExpr {
        let from = expr.ty();
        if from == to {
            return expr;
        }
        let (file, line, column) = self.located(span);
        CheckedExpr::NumCast {
            from,
            to,
            check: numeric::needs_check(from, to),
            expr: Box::new(expr),
            file,
            line,
            column,
        }
    }

    fn check_assignment(&mut self, expr: &AssignmentExpr) -> CheckedExpr {
        match &*expr.target {
            Expr::Index(index) => return self.check_index_assignment(index, &expr.value),
            Expr::Slice(slice) => return self.check_slice_assignment(slice, &expr.value),
            Expr::Member(member) => return self.check_field_assignment(member, &expr.value),
            Expr::Identifier(_) => {}
            _ => {
                self.error(
                    expr.target.span(),
                    "the assignment target must be a variable, field, index, or slice",
                );
                return self.dummy(None);
            }
        }
        let Expr::Identifier(ident) = &*expr.target else {
            return self.dummy(None);
        };
        let Some(binding) = self.lookup(&ident.name) else {
            self.error(ident.span, format!("unknown name '{}'", ident.name));
            return self.dummy(None);
        };
        self.reject_binding_write(&ident.name, ident.span, false);
        let ty = binding.ty;
        let auto = binding.auto;
        let value = self.check_expr(&expr.value, Some(ty), false);
        let value = self.apply_conversion(value, ty, auto, expr.value.span());
        CheckedExpr::Assign {
            name: ident.name.clone(),
            ty,
            value: Box::new(value),
        }
    }

    fn check_update(&mut self, expr: &UpdateExpr) -> CheckedExpr {
        let Expr::Identifier(ident) = &*expr.operand else {
            self.error(
                expr.span,
                "increment and decrement require a mutable integer variable",
            );
            return self.dummy(None);
        };
        let Some(binding) = self.lookup(&ident.name) else {
            self.error(ident.span, format!("unknown name '{}'", ident.name));
            return self.dummy(None);
        };
        self.reject_binding_write(&ident.name, ident.span, true);
        if binding.mutable && !binding.ty.is_integer() {
            self.error(
                expr.span,
                "increment and decrement require a mutable integer variable",
            );
        }
        CheckedExpr::Update {
            prefix: expr.position == UpdatePosition::Prefix,
            increment: expr.operator == UpdateOperator::Increment,
            name: ident.name.clone(),
            ty: binding.ty,
        }
    }

    fn check_call(&mut self, expr: &CallExpr, allow_void: bool) -> CheckedExpr {
        if let Expr::Member(member) = &*expr.callee {
            if let Expr::Identifier(object) = &*member.object {
                if self.imports.contains_key(&object.name) {
                    return self.check_library_call(member, &expr.arguments, expr.span, allow_void);
                }
            }
            return self.check_array_method(member, &expr.arguments, expr.span);
        }
        if let Expr::Identifier(ident) = &*expr.callee {
            if ident.name == "lengthOf" {
                return self.check_length_of(&expr.arguments, expr.span);
            }
            return self.check_user_call(ident, &expr.arguments, expr.span);
        }
        self.error(expr.span, "unsupported call target");
        for argument in &expr.arguments {
            let _ = self.check_expr(argument, None, false);
        }
        self.dummy(None)
    }

    fn check_library_call(
        &mut self,
        member: &MemberExpr,
        arguments: &[Expr],
        span: Span,
        _allow_void: bool,
    ) -> CheckedExpr {
        let Expr::Identifier(object) = &*member.object else {
            self.error(
                member.span,
                "a library call must use the imported library name",
            );
            return self.dummy(None);
        };
        let Some(import) = self.imports.get(&object.name).cloned() else {
            if self.lookup(&object.name).is_some() {
                self.error(
                    member.span,
                    "member access is only supported for imported libraries in this milestone",
                );
            } else {
                self.error(object.span, format!("unknown name '{}'", object.name));
            }
            return self.dummy(None);
        };
        let Some(resolved) = self
            .manifest
            .libraries
            .get(&import.library)
            .and_then(|library| {
                library.functions.get(&member.member.name).map(|function| {
                    (
                        function.rust_path.clone(),
                        canonical_name(&function.returns.name),
                        function_params(function),
                    )
                })
            })
        else {
            self.error(
                member.member.span,
                format!(
                    "function '{}' is not exported by '{}.{}'",
                    member.member.name, import.domain, import.library
                ),
            );
            return self.dummy(None);
        };
        let (path, ret, params) = resolved;
        let Some(ret) = ret else {
            self.error(span, "the library function has an unknown return type");
            return self.dummy(None);
        };
        let args = self.check_arguments(arguments, &params, span);
        CheckedExpr::Call {
            path,
            args,
            ty: ret,
        }
    }

    fn check_length_of(&mut self, arguments: &[Expr], span: Span) -> CheckedExpr {
        if arguments.len() != 1 {
            self.error(
                span,
                format!("lengthOf expects 1 argument, found {}", arguments.len()),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return CheckedExpr::Int {
                value: 0,
                ty: CanonicalType::UInt64,
            };
        }
        let argument = &arguments[0];
        let value = self.check_expr(argument, None, false);
        if let CanonicalType::Record(id) = value.ty() {
            let count = self
                .records
                .get(id.0 as usize)
                .map(|record| record.fields.len() as u64)
                .unwrap_or(0);
            return CheckedExpr::LengthOf {
                expr: Box::new(value),
                count: Some(count),
            };
        }
        if value.ty() != CanonicalType::Str && !value.ty().is_array() && !value.ty().is_map() {
            self.error(
                argument.span(),
                format!(
                    "lengthOf requires a str, array, record, or map value; '{}' has no defined length",
                    self.spell(value.ty())
                ),
            );
            return CheckedExpr::Int {
                value: 0,
                ty: CanonicalType::UInt64,
            };
        }
        CheckedExpr::LengthOf {
            expr: Box::new(value),
            count: None,
        }
    }

    fn check_user_call(
        &mut self,
        ident: &Identifier,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let Some(resolved) = self.functions.get(&ident.name).map(|signature| {
            (
                signature.ret,
                signature
                    .params
                    .iter()
                    .map(|param| (param.ty, param.auto))
                    .collect::<Vec<_>>(),
            )
        }) else {
            self.error(ident.span, format!("unknown function '{}'", ident.name));
            return self.dummy(None);
        };
        let (ret, params) = resolved;
        let path = ident.name.clone();
        let args = self.check_arguments(arguments, &params, span);
        CheckedExpr::Call {
            path,
            args,
            ty: ret,
        }
    }

    fn check_arguments(
        &mut self,
        arguments: &[Expr],
        params: &[(CanonicalType, bool)],
        span: Span,
    ) -> Vec<CheckedExpr> {
        if arguments.len() != params.len() {
            self.error(
                span,
                format!(
                    "expected {} argument{}, found {}",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    arguments.len()
                ),
            );
        }
        let mut checked = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            if let Some((ty, auto)) = params.get(index) {
                let record_shape = self.accepts_brace(*ty);
                let expected = if *auto && !record_shape {
                    None
                } else {
                    Some(*ty)
                };
                let value = self.check_expr(argument, expected, false);
                checked.push(self.apply_conversion(value, *ty, *auto, argument.span()));
            } else {
                let _ = self.check_expr(argument, None, false);
            }
        }
        checked
    }

    fn check_conversion(&mut self, expr: &ConversionExpr) -> CheckedExpr {
        let target = self.resolve_type(&expr.target);
        if target == CanonicalType::Void {
            self.error(expr.target.span, "void is not a conversion target");
            return self.dummy(None);
        }
        if expr.arguments.len() != 1 {
            self.error(
                expr.span,
                format!(
                    "a conversion expects 1 argument, found {}",
                    expr.arguments.len()
                ),
            );
            for argument in &expr.arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(Some(target));
        }
        let argument = &expr.arguments[0];
        let value = self.check_expr(argument, None, false);
        self.apply_conversion(value, target, true, expr.span)
    }

    fn apply_conversion(
        &mut self,
        expr: CheckedExpr,
        target: CanonicalType,
        allow: bool,
        span: Span,
    ) -> CheckedExpr {
        let from = expr.ty();
        if from == target {
            return expr;
        }
        if allow {
            if let Some(op) = conversion(from, target) {
                return CheckedExpr::Convert {
                    from,
                    to: target,
                    op,
                    expr: Box::new(expr),
                    file: self.source_name.to_string(),
                    line: span.start.line,
                    column: span.start.column,
                };
            }
            self.error(
                span,
                format!(
                    "cannot convert '{}' to '{}' because that conversion is not supported",
                    self.spell(from),
                    self.spell(target)
                ),
            );
            return expr;
        }
        self.error(
            span,
            format!(
                "type mismatch: expected '{}', found '{}'",
                self.spell(target),
                self.spell(from)
            ),
        );
        expr
    }

    fn check_record_literal(
        &mut self,
        expr: &RecordExpr,
        expected: Option<CanonicalType>,
    ) -> CheckedExpr {
        match expected {
            Some(CanonicalType::Record(id)) => self.check_record_fields(expr, id),
            Some(ty) if ty.is_map() => self.check_map_literal(expr, ty),
            _ => {
                self.error(
                    expr.span,
                    "a brace literal needs an expected record or map type",
                );
                for entry in &expr.entries {
                    self.check_untyped_entry(entry);
                }
                self.dummy(expected)
            }
        }
    }

    fn check_untyped_entry(&mut self, entry: &BraceEntry) {
        match entry {
            BraceEntry::Pair { key, value, .. } => {
                let _ = self.check_expr(key, None, false);
                let _ = self.check_expr(value, None, false);
            }
            BraceEntry::Spread { value, .. } => {
                let _ = self.check_expr(value, None, false);
            }
        }
    }

    fn check_record_fields(&mut self, expr: &RecordExpr, id: RecordId) -> CheckedExpr {
        let declared = self
            .records
            .get(id.0 as usize)
            .map(|record| {
                record
                    .fields
                    .iter()
                    .map(|field| (field.name.clone(), field.ty))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut seen = HashMap::new();
        let mut initializers = Vec::new();
        for entry in &expr.entries {
            let BraceEntry::Pair { key, value, span } = entry else {
                if let BraceEntry::Spread { value, span } = entry {
                    self.error(*span, "spread is only allowed in a map literal");
                    let _ = self.check_expr(value, None, false);
                }
                continue;
            };
            let Expr::Identifier(name) = key else {
                self.error(key.span(), "a record field name must be an identifier");
                let _ = self.check_expr(key, None, false);
                let _ = self.check_expr(value, None, false);
                continue;
            };
            if let Some(previous) = seen.insert(name.name.clone(), *span) {
                self.error(
                    name.span,
                    format!(
                        "duplicate field '{}' in record literal; the first is at line {}",
                        name.name, previous.start.line
                    ),
                );
                let _ = self.check_expr(value, None, false);
                continue;
            }
            let Some(position) = declared.iter().position(|(field, _)| field == &name.name) else {
                self.error(
                    name.span,
                    format!("record literal has unknown field '{}'", name.name),
                );
                let _ = self.check_expr(value, None, false);
                continue;
            };
            let field_ty = declared[position].1;
            let checked = self.check_expr(value, Some(field_ty), false);
            let checked = self.apply_conversion(checked, field_ty, false, value.span());
            initializers.push((position, checked));
        }
        for (name, _) in &declared {
            if !seen.contains_key(name) {
                self.error(
                    expr.span,
                    format!("record literal is missing field '{name}'"),
                );
            }
        }
        CheckedExpr::Record {
            id,
            fields: initializers,
            ty: CanonicalType::Record(id),
        }
    }

    fn check_map_literal(&mut self, expr: &RecordExpr, map_ty: CanonicalType) -> CheckedExpr {
        let (key_ty, value_ty) = self.map_parts(map_ty);
        let mut entries = Vec::new();
        for entry in &expr.entries {
            match entry {
                BraceEntry::Pair { key, value, .. } => {
                    let key = self.check_map_key(key, key_ty);
                    let checked = self.check_expr(value, Some(value_ty), false);
                    let checked = self.apply_conversion(checked, value_ty, false, value.span());
                    entries.push(MapEntry::Pair(key, checked));
                }
                BraceEntry::Spread { value, .. } => {
                    let source = self.check_expr(value, Some(map_ty), false);
                    let source = self.apply_conversion(source, map_ty, false, value.span());
                    entries.push(MapEntry::Spread(source));
                }
            }
        }
        CheckedExpr::MapLiteral {
            entries,
            ty: map_ty,
        }
    }

    fn check_map_key(&mut self, expr: &Expr, key_ty: CanonicalType) -> CheckedExpr {
        let key = self.check_expr(expr, Some(key_ty), false);
        self.apply_conversion(key, key_ty, false, expr.span())
    }

    fn check_member(&mut self, expr: &MemberExpr) -> CheckedExpr {
        if let Expr::Identifier(object) = &*expr.object {
            if self.imports.contains_key(&object.name) {
                self.error(
                    expr.span,
                    "a library member can only be used as a function call",
                );
                return self.dummy(None);
            }
        }
        let object = self.check_expr(&expr.object, None, false);
        let CanonicalType::Record(id) = object.ty() else {
            self.error(
                expr.member.span,
                format!(
                    "'{}' has no field '{}'",
                    self.spell(object.ty()),
                    expr.member.name
                ),
            );
            return self.dummy(None);
        };
        let Some((index, ty)) = self.field_index(id, &expr.member.name) else {
            self.error(
                expr.member.span,
                format!("unknown field '{}'", expr.member.name),
            );
            return self.dummy(None);
        };
        CheckedExpr::FieldGet {
            object: Box::new(object),
            record: id,
            field: index,
            ty,
        }
    }

    fn check_field_assignment(&mut self, member: &MemberExpr, value: &Expr) -> CheckedExpr {
        let before = self.diagnostics.len();
        if let Some(place) = self.check_place(&Expr::Member(member.clone()), true) {
            let ty = self.place_type(&place);
            let checked = self.check_expr(value, Some(ty), false);
            let value = self.apply_conversion(checked, ty, false, value.span());
            return CheckedExpr::FieldSet {
                place,
                value: Box::new(value),
                ty,
            };
        }
        if self.diagnostics.len() == before {
            self.error(
                member.span,
                "a field assignment requires a mutable variable",
            );
        }
        let _ = self.check_expr(value, None, false);
        self.dummy(None)
    }

    fn check_array_literal(
        &mut self,
        expr: &ArrayExpr,
        expected: Option<CanonicalType>,
    ) -> CheckedExpr {
        let element_expected = expected.and_then(|ty| ty.element());
        if expr.elements.is_empty() {
            let Some(ty) = expected.filter(|ty| ty.is_array()) else {
                self.error(
                    expr.span,
                    "an empty array literal needs a known element type",
                );
                return CheckedExpr::Array {
                    elements: Vec::new(),
                    ty: expected.unwrap_or(CanonicalType::Int64.nest()),
                };
            };
            if ty.is_void_array() {
                self.error(expr.span, "void is not a valid array element type");
            }
            self.reject_over_limit(ty, 0, expr.span);
            return CheckedExpr::Array {
                elements: Vec::new(),
                ty,
            };
        }
        let mut elements: Vec<CheckedExpr> = Vec::new();
        for (index, element) in expr.elements.iter().enumerate() {
            let expected_element = if index == 0 {
                element_expected
            } else {
                Some(elements[0].ty())
            };
            let element_expected = if index == 0 {
                element_expected
            } else {
                expected_element
            };
            let checked = self.check_expr(element, element_expected, false);
            let target = element_expected.unwrap_or_else(|| checked.ty());
            if target == CanonicalType::Void || target.is_void_array() {
                self.error(element.span(), "void is not a valid array element type");
            }
            elements.push(self.apply_conversion(checked, target, false, element.span()));
        }
        let element_ty = element_expected.unwrap_or_else(|| elements[0].ty());
        let ty = match expected {
            Some(expected) if expected.element() == Some(element_ty) => expected,
            _ => element_ty.nest(),
        };
        self.reject_over_limit(ty, elements.len(), expr.span);
        CheckedExpr::Array { elements, ty }
    }

    fn reject_over_limit(&mut self, ty: CanonicalType, len: usize, span: Span) {
        let Some(limit) = ty.limit() else {
            return;
        };
        if len as u64 > limit {
            self.error(
                span,
                format!("array literal has {len} elements, which exceeds the maximum of {limit}"),
            );
        }
    }

    fn check_index(&mut self, expr: &IndexExpr) -> CheckedExpr {
        let object = self.check_expr(&expr.object, None, false);
        if object.ty().is_map() {
            return self.check_map_read(object, expr);
        }
        let index = self.check_index_operand(&expr.index);
        let ty = if object.ty().is_array() {
            object.ty().element().unwrap_or(CanonicalType::Int64)
        } else {
            self.error(
                expr.object.span(),
                format!("'{}' is not an array", self.spell(object.ty())),
            );
            CanonicalType::Int64
        };
        let (file, line, column) = self.located(expr.span);
        CheckedExpr::IndexGet {
            object: Box::new(object),
            index: Box::new(index),
            ty,
            file,
            line,
            column,
        }
    }

    fn check_slice(&mut self, expr: &SliceExpr) -> CheckedExpr {
        let object = self.check_expr(&expr.object, None, false);
        if object.ty().is_map() {
            self.error(expr.span, "maps do not support slicing");
        } else if !object.ty().is_array() {
            self.error(
                expr.object.span(),
                format!("'{}' is not an array", self.spell(object.ty())),
            );
        }
        let start = expr
            .start
            .as_ref()
            .map(|bound| Box::new(self.check_index_operand(bound)));
        let stop = expr
            .stop
            .as_ref()
            .map(|bound| Box::new(self.check_index_operand(bound)));
        let step = expr
            .step
            .as_ref()
            .map(|bound| Box::new(self.check_index_operand(bound)));
        let ty = object.ty();
        let (file, line, column) = self.located(expr.span);
        CheckedExpr::SliceGet {
            object: Box::new(object),
            start,
            stop,
            step,
            ty,
            file,
            line,
            column,
        }
    }

    fn check_index_assignment(&mut self, index: &IndexExpr, value: &Expr) -> CheckedExpr {
        if let Some(place) = self.check_place(&index.object, true) {
            let object_ty = self.place_type(&place);
            if object_ty.is_map() {
                return self.check_map_insert(place, object_ty, index, value);
            }
            let checked_index = self.check_index_operand(&index.index);
            let (file, line, column) = self.located(index.span);
            let array_ty = object_ty;
            if !array_ty.is_array() {
                self.error(
                    index.object.span(),
                    format!("'{}' is not an array", self.spell(array_ty)),
                );
                let _ = self.check_expr(value, None, false);
                return self.dummy(None);
            }
            let element = array_ty.element().unwrap_or(CanonicalType::Int64);
            let checked = self.check_expr(value, Some(element), false);
            let value = self.apply_conversion(checked, element, false, value.span());
            return CheckedExpr::IndexSet {
                place: Some(place),
                object: None,
                index: Box::new(checked_index),
                value: Box::new(value),
                ty: element,
                file,
                line,
                column,
            };
        }
        let object = self.check_expr(&index.object, None, false);
        if object.ty().is_map() {
            self.error(index.span, "a map assignment requires a mutable variable");
            let (key_ty, value_ty) = self.map_parts(object.ty());
            let _ = self.check_map_key(&index.index, key_ty);
            let _ = self.check_expr(value, Some(value_ty), false);
            return self.dummy(None);
        }
        let checked_index = self.check_index_operand(&index.index);
        let (file, line, column) = self.located(index.span);
        let element = if object.ty().is_array() {
            object.ty().element().unwrap_or(CanonicalType::Int64)
        } else {
            self.error(
                index.object.span(),
                format!("'{}' is not an array", self.spell(object.ty())),
            );
            CanonicalType::Int64
        };
        let checked = self.check_expr(value, Some(element), false);
        let value = self.apply_conversion(checked, element, false, value.span());
        CheckedExpr::IndexSet {
            place: None,
            object: Some(Box::new(object)),
            index: Box::new(checked_index),
            value: Box::new(value),
            ty: element,
            file,
            line,
            column,
        }
    }

    fn check_slice_assignment(&mut self, slice: &SliceExpr, value: &Expr) -> CheckedExpr {
        let start = slice
            .start
            .as_ref()
            .map(|bound| Box::new(self.check_index_operand(bound)));
        let stop = slice
            .stop
            .as_ref()
            .map(|bound| Box::new(self.check_index_operand(bound)));
        let step = slice
            .step
            .as_ref()
            .map(|bound| Box::new(self.check_index_operand(bound)));
        let (file, line, column) = self.located(slice.span);
        let (place, object, array_ty) = if let Some(place) = self.check_place(&slice.object, true) {
            let array_ty = self.place_type(&place);
            if array_ty.is_map() {
                self.error(slice.span, "maps do not support slicing");
                let _ = self.check_expr(value, None, false);
                return self.dummy(None);
            }
            if !array_ty.is_array() {
                self.error(
                    slice.object.span(),
                    format!("'{}' is not an array", self.spell(array_ty)),
                );
                let _ = self.check_expr(value, None, false);
                return self.dummy(None);
            }
            (Some(place), None, array_ty)
        } else {
            let object = self.check_expr(&slice.object, None, false);
            if object.ty().is_map() {
                self.error(slice.span, "maps do not support slicing");
                let _ = self.check_expr(value, None, false);
                return self.dummy(None);
            }
            if !object.ty().is_array() {
                self.error(
                    slice.object.span(),
                    format!("'{}' is not an array", self.spell(object.ty())),
                );
            }
            let array_ty = object.ty();
            (None, Some(Box::new(object)), array_ty)
        };
        let checked = self.check_expr(value, Some(array_ty), false);
        let value = self.apply_conversion(checked, array_ty, false, value.span());
        CheckedExpr::SliceSet {
            place,
            object,
            start,
            stop,
            step,
            value: Box::new(value),
            ty: array_ty,
            limit: array_ty.limit(),
            file,
            line,
            column,
        }
    }

    fn check_place(&mut self, expr: &Expr, mutating: bool) -> Option<ArrayPlace> {
        match expr {
            Expr::Parenthesized { expression, .. } => self.check_place(expression, mutating),
            Expr::Identifier(ident) => {
                let (scope, binding) = self.binding_at(&ident.name)?;
                if mutating {
                    self.reject_iterated_write(&ident.name, scope, ident.span);
                    if !binding.mutable {
                        self.error(
                            ident.span,
                            format!("cannot assign to constant '{}'", ident.name),
                        );
                    }
                }
                let _ = binding;
                Some(ArrayPlace::Var(ident.name.clone()))
            }
            Expr::Member(member) => {
                let object = self.check_place(&member.object, mutating)?;
                let ty = self.place_type(&object);
                let CanonicalType::Record(id) = ty else {
                    self.error(
                        member.member.span,
                        format!("'{}' has no field '{}'", self.spell(ty), member.member.name),
                    );
                    return None;
                };
                let Some((_, field_ty)) = self.field_index(id, &member.member.name) else {
                    self.error(
                        member.member.span,
                        format!("unknown field '{}'", member.member.name),
                    );
                    return None;
                };
                Some(ArrayPlace::Field {
                    object: Box::new(object),
                    field: member.member.name.clone(),
                    ty: field_ty,
                })
            }
            Expr::Index(index) => {
                let object = self.check_place(&index.object, mutating)?;
                let object_ty = self.place_type(&object);
                if object_ty.is_map() {
                    let (key_ty, value_ty) = self.map_parts(object_ty);
                    let key = self.check_map_key(&index.index, key_ty);
                    let (file, line, column) = self.located(index.span);
                    return Some(ArrayPlace::MapIndex {
                        object: Box::new(object),
                        key: Box::new(key),
                        value_ty,
                        type_name: self.spell(object_ty),
                        file,
                        line,
                        column,
                    });
                }
                if !object_ty.is_array() {
                    self.error(
                        index.object.span(),
                        format!("'{}' is not an array", self.spell(object_ty)),
                    );
                    return None;
                }
                let checked = self.check_index_operand(&index.index);
                let (file, line, column) = self.located(index.span);
                Some(ArrayPlace::Index {
                    object: Box::new(object),
                    index: Box::new(checked),
                    file,
                    line,
                    column,
                })
            }
            _ => None,
        }
    }

    fn place_type(&self, place: &ArrayPlace) -> CanonicalType {
        match place {
            ArrayPlace::Var(name) => self
                .lookup(name)
                .map(|binding| binding.ty)
                .unwrap_or(CanonicalType::Int64),
            ArrayPlace::Field { ty, .. } => *ty,
            ArrayPlace::MapIndex { value_ty, .. } => *value_ty,
            ArrayPlace::Index { object, .. } => self
                .place_type(object)
                .element()
                .unwrap_or(CanonicalType::Int64),
        }
    }

    fn check_array_method(
        &mut self,
        member: &MemberExpr,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let name = member.member.name.as_str();
        if matches!(name, "iter" | "enumerate" | "entries") {
            self.error(span, format!("{name}() can only be used as a loop source"));
            let _ = self.check_expr(&member.object, None, false);
            return self.dummy(None);
        }
        let mutating = matches!(name, "set" | "remove" | "push" | "insert");
        if let Some(place) = self.check_place(&member.object, mutating) {
            let ty = self.place_type(&place);
            if ty.is_map() {
                return self.finish_map_call(name, Some(place), None, arguments, span, member);
            }
            if ty.is_record() {
                return self.reject_record_call(member, arguments, ty);
            }
            return self.finish_array_call(name, Some(place), None, ty, arguments, span, member);
        }
        let object = self.check_expr(&member.object, None, false);
        let ty = object.ty();
        if ty.is_map() {
            if mutating {
                self.error(
                    span,
                    format!("map method '{name}' requires a mutable variable"),
                );
            }
            return self.finish_map_call(name, None, Some(object), arguments, span, member);
        }
        if ty.is_record() {
            return self.reject_record_call(member, arguments, ty);
        }
        self.finish_array_call(name, None, Some(object), ty, arguments, span, member)
    }

    fn finish_array_call(
        &mut self,
        name: &str,
        place: Option<ArrayPlace>,
        object: Option<CheckedExpr>,
        array_ty: CanonicalType,
        arguments: &[Expr],
        span: Span,
        member: &MemberExpr,
    ) -> CheckedExpr {
        let method = match name {
            "get" => ArrayMethod::Get,
            "set" => ArrayMethod::Set,
            "push" => ArrayMethod::Push,
            "insert" => ArrayMethod::Insert,
            "remove" => ArrayMethod::Remove,
            "length" => ArrayMethod::Length,
            _ => {
                self.error(member.member.span, format!("array has no method '{name}'"));
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            }
        };
        if !array_ty.is_array() {
            self.error(
                member.object.span(),
                format!("'{}' is not an array", self.spell(array_ty)),
            );
        }
        let expected_args = match method {
            ArrayMethod::Length => 0,
            ArrayMethod::Get | ArrayMethod::Push | ArrayMethod::Remove => 1,
            ArrayMethod::Set | ArrayMethod::Insert => 2,
        };
        if arguments.len() != expected_args {
            self.error(
                span,
                format!(
                    "method '{name}' expects {expected_args} argument{}, found {}",
                    if expected_args == 1 { "" } else { "s" },
                    arguments.len()
                ),
            );
        }
        let (file, line, column) = self.located(span);
        let args = self.array_method_args(method, array_ty, arguments);
        CheckedExpr::ArrayCall {
            place,
            object: object.map(Box::new),
            method,
            args,
            ty: self.array_method_type(method, array_ty),
            limit: array_ty.limit(),
            file,
            line,
            column,
        }
    }

    fn finish_map_call(
        &mut self,
        name: &str,
        place: Option<ArrayPlace>,
        object: Option<CheckedExpr>,
        arguments: &[Expr],
        span: Span,
        member: &MemberExpr,
    ) -> CheckedExpr {
        let map_ty = place
            .as_ref()
            .map(|place| self.place_type(place))
            .or_else(|| object.as_ref().map(|object| object.ty()))
            .unwrap_or(CanonicalType::Int64);
        let (key_ty, value_ty) = self.map_parts(map_ty);
        let method = match name {
            "get" => MapMethod::Get,
            "getOrDefault" => MapMethod::GetOrDefault,
            "containsKey" => MapMethod::ContainsKey,
            "set" => MapMethod::Set,
            "remove" => MapMethod::Remove,
            "length" => MapMethod::Length,
            "keys" => MapMethod::Keys,
            "values" => MapMethod::Values,
            _ => {
                self.error(member.member.span, format!("map has no method '{name}'"));
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            }
        };
        let expected = match method {
            MapMethod::Length | MapMethod::Keys | MapMethod::Values => 0,
            MapMethod::Get | MapMethod::ContainsKey | MapMethod::Remove => 1,
            MapMethod::GetOrDefault | MapMethod::Set => 2,
            MapMethod::Entries => 0,
        };
        if arguments.len() != expected {
            self.error(
                span,
                format!(
                    "method '{name}' expects {expected} argument{}, found {}",
                    if expected == 1 { "" } else { "s" },
                    arguments.len()
                ),
            );
        }
        let mut args = Vec::new();
        if expected >= 1 {
            if let Some(argument) = arguments.first() {
                args.push(self.check_map_key(argument, key_ty));
            }
        }
        if expected >= 2 {
            if let Some(argument) = arguments.get(1) {
                let checked = self.check_expr(argument, Some(value_ty), false);
                args.push(self.apply_conversion(checked, value_ty, false, argument.span()));
            }
        }
        for argument in arguments.iter().skip(expected) {
            let _ = self.check_expr(argument, None, false);
        }
        let ty = match method {
            MapMethod::Get | MapMethod::GetOrDefault => value_ty,
            MapMethod::ContainsKey | MapMethod::Remove => CanonicalType::Bln,
            MapMethod::Set => CanonicalType::Void,
            MapMethod::Length => CanonicalType::UInt64,
            MapMethod::Keys => key_ty.nest(),
            MapMethod::Values | MapMethod::Entries => value_ty.nest(),
        };
        let (file, line, column) = self.located(span);
        CheckedExpr::MapCall {
            place,
            object: object.map(Box::new),
            method,
            args,
            ty,
            key_ty,
            value_ty,
            type_name: self.spell(map_ty),
            file,
            line,
            column,
        }
    }

    fn check_map_read(&mut self, object: CheckedExpr, expr: &IndexExpr) -> CheckedExpr {
        let map_ty = object.ty();
        let (key_ty, value_ty) = self.map_parts(map_ty);
        let key = self.check_map_key(&expr.index, key_ty);
        let (file, line, column) = self.located(expr.span);
        CheckedExpr::MapGet {
            object: Box::new(object),
            key: Box::new(key),
            ty: value_ty,
            type_name: self.spell(map_ty),
            file,
            line,
            column,
        }
    }

    fn check_map_insert(
        &mut self,
        place: ArrayPlace,
        map_ty: CanonicalType,
        index: &IndexExpr,
        value: &Expr,
    ) -> CheckedExpr {
        let (key_ty, value_ty) = self.map_parts(map_ty);
        let key = self.check_map_key(&index.index, key_ty);
        let checked = self.check_expr(value, Some(value_ty), false);
        let checked = self.apply_conversion(checked, value_ty, false, value.span());
        let (file, line, column) = self.located(index.span);
        CheckedExpr::MapInsert {
            place,
            key: Box::new(key),
            value: Box::new(checked),
            ty: value_ty,
            file,
            line,
            column,
        }
    }

    fn reject_record_call(
        &mut self,
        member: &MemberExpr,
        arguments: &[Expr],
        ty: CanonicalType,
    ) -> CheckedExpr {
        if let CanonicalType::Record(id) = ty {
            if let Some((_, field_ty)) = self.field_index(id, &member.member.name) {
                self.error(
                    member.span,
                    format!("cannot call a value of type '{}'", self.spell(field_ty)),
                );
            } else {
                self.error(
                    member.member.span,
                    format!("unknown field '{}'", member.member.name),
                );
            }
        }
        for argument in arguments {
            let _ = self.check_expr(argument, None, false);
        }
        self.dummy(None)
    }

    fn array_method_type(&self, method: ArrayMethod, array_ty: CanonicalType) -> CanonicalType {
        match method {
            ArrayMethod::Get => array_ty.element().unwrap_or(CanonicalType::Int64),
            ArrayMethod::Length => CanonicalType::UInt64,
            ArrayMethod::Set | ArrayMethod::Push | ArrayMethod::Insert | ArrayMethod::Remove => {
                CanonicalType::Void
            }
        }
    }

    fn array_method_args(
        &mut self,
        method: ArrayMethod,
        array_ty: CanonicalType,
        arguments: &[Expr],
    ) -> Vec<CheckedExpr> {
        let element = array_ty.element().unwrap_or(CanonicalType::Int64);
        let mut checked = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let expected = match (method, index) {
                (ArrayMethod::Get | ArrayMethod::Remove, 0) => None,
                (ArrayMethod::Push, 0) => Some(element),
                (ArrayMethod::Set, 0) | (ArrayMethod::Insert, 0) => None,
                (ArrayMethod::Set, 1) | (ArrayMethod::Insert, 1) => Some(element),
                _ => None,
            };
            let value = self.check_expr(argument, expected, false);
            if expected.is_none() && matches!(index, 0) && method != ArrayMethod::Push {
                if !value.ty().is_integer() && arguments.len() > index {
                    self.error(
                        argument.span(),
                        format!(
                            "an index must be an integer, found '{}'",
                            self.spell(value.ty())
                        ),
                    );
                }
            }
            let value = if let Some(expected) = expected {
                self.apply_conversion(value, expected, false, argument.span())
            } else {
                value
            };
            checked.push(value);
        }
        checked
    }

    fn check_destructure(&mut self, decl: &DestructureDecl) -> Option<CheckedStmt> {
        if decl.bindings.len() < 2 {
            self.error(decl.span, "destructuring requires at least two bindings");
        }
        let value = self.check_expr(&decl.initializer, None, false);
        let Some(element) = value.ty().element() else {
            self.error(
                decl.initializer.span(),
                format!(
                    "destructuring requires an array, found '{}'",
                    self.spell(value.ty())
                ),
            );
            return None;
        };
        if let Expr::Array(array) = unwrap_parens(&decl.initializer) {
            if array.elements.len() != decl.bindings.len() {
                self.error(
                    decl.span,
                    format!(
                        "destructuring expects {} elements, found {}",
                        decl.bindings.len(),
                        array.elements.len()
                    ),
                );
            }
        }
        let mut names = Vec::new();
        let mut bindings = Vec::new();
        for binding in &decl.bindings {
            if binding.conversion == ConversionMode::Auto {
                self.error(
                    binding.span,
                    "auto is not allowed in a destructuring declaration",
                );
            }
            let ty = self.resolve_type(&binding.ty);
            if ty == CanonicalType::Void || ty.is_void_array() {
                self.error(binding.ty.span, "void is not a valid array element type");
            }
            if ty != element {
                self.error(
                    binding.ty.span,
                    format!(
                        "type mismatch: expected '{}', found '{}'",
                        self.spell(element),
                        self.spell(ty)
                    ),
                );
            }
            if names.contains(&binding.name.name) {
                self.error(
                    binding.name.span,
                    format!("duplicate binding '{}'", binding.name.name),
                );
            }
            names.push(binding.name.name.clone());
            if binding.name.name == "lengthOf" {
                self.error(binding.name.span, "'lengthOf' is a reserved built-in name");
            }
            if self
                .scopes
                .last()
                .is_some_and(|scope| scope.contains_key(&binding.name.name))
            {
                self.error(
                    binding.name.span,
                    format!("'{}' is already declared in this scope", binding.name.name),
                );
            }
            bindings.push((binding.name.name.clone(), ty, binding.mutable));
        }
        for (name, ty, mutable) in &bindings {
            self.scopes.last_mut().unwrap().insert(
                name.clone(),
                Binding {
                    ty: *ty,
                    mutable: *mutable,
                    auto: false,
                },
            );
        }
        let (file, line, column) = self.located(decl.span);
        Some(CheckedStmt::Destructure {
            bindings,
            value,
            file,
            line,
            column,
        })
    }

    fn check_iteration(
        &mut self,
        bindings: &[BindingHead],
        iterable: &Expr,
        stmt: &LoopStmt,
    ) -> Option<CheckedStmt> {
        let (source, kind, element, second) = self.expect_loop_source(iterable);
        let expected = match kind {
            LoopSourceKind::Values => 1,
            LoopSourceKind::Enumerate | LoopSourceKind::Entries => 2,
        };
        if bindings.len() != expected {
            self.error(
                stmt.span,
                format!(
                    "this loop expects {expected} binding{}, found {}",
                    if expected == 1 { "" } else { "s" },
                    bindings.len()
                ),
            );
        }
        if kind == LoopSourceKind::Enumerate {
            if let Some(index) = bindings.first() {
                let ty = self.resolve_type(&index.ty);
                if ty != CanonicalType::UInt64 {
                    self.error(
                        index.ty.span,
                        format!(
                            "an enumeration index must have type uint64, found '{}'",
                            self.spell(ty)
                        ),
                    );
                }
            }
            if let Some(value) = bindings.get(1) {
                let ty = self.resolve_type(&value.ty);
                if ty != element {
                    self.error(
                        value.ty.span,
                        format!(
                            "type mismatch: expected '{}', found '{}'",
                            self.spell(element),
                            self.spell(ty)
                        ),
                    );
                }
            }
        } else if kind == LoopSourceKind::Entries {
            if let Some(key) = bindings.first() {
                let ty = self.resolve_type(&key.ty);
                if ty != element {
                    self.error(
                        key.ty.span,
                        format!(
                            "type mismatch: expected '{}', found '{}'",
                            self.spell(element),
                            self.spell(ty)
                        ),
                    );
                }
            }
            if let Some(value) = bindings.get(1) {
                let ty = self.resolve_type(&value.ty);
                if ty != second {
                    self.error(
                        value.ty.span,
                        format!(
                            "type mismatch: expected '{}', found '{}'",
                            self.spell(second),
                            self.spell(ty)
                        ),
                    );
                }
            }
        } else if let Some(value) = bindings.first() {
            let ty = self.resolve_type(&value.ty);
            if ty != element {
                self.error(
                    value.ty.span,
                    format!(
                        "type mismatch: expected '{}', found '{}'",
                        self.spell(element),
                        self.spell(ty)
                    ),
                );
            }
        }
        let mut seen = Vec::new();
        let mut checked_bindings = Vec::new();
        for binding in bindings {
            if binding.conversion == ConversionMode::Auto {
                self.error(binding.span, "auto is not allowed in a loop binding");
            }
            if seen.contains(&binding.name.name) {
                self.error(
                    binding.name.span,
                    format!("duplicate binding '{}'", binding.name.name),
                );
            }
            seen.push(binding.name.name.clone());
            let ty = self.resolve_type(&binding.ty);
            checked_bindings.push((binding.name.name.clone(), ty, binding.mutable));
        }
        let lock = if let CheckedExpr::Load { name, .. } = &source {
            self.binding_at(name).map(|(scope, _)| IterLock {
                name: name.clone(),
                scope,
            })
        } else {
            None
        };
        if let Some(lock) = lock.clone() {
            self.iterating.push(lock);
        }
        self.scopes.push(HashMap::new());
        for (name, ty, mutable) in &checked_bindings {
            if name == "lengthOf" {
                self.error(stmt.span, "'lengthOf' is a reserved built-in name");
                continue;
            }
            self.scopes.last_mut().unwrap().insert(
                name.clone(),
                Binding {
                    ty: *ty,
                    mutable: *mutable,
                    auto: false,
                },
            );
        }
        self.loop_depth += 1;
        let body = self.check_block_stmts(&stmt.body);
        self.loop_depth -= 1;
        self.scopes.pop();
        if lock.is_some() {
            self.iterating.pop();
        }
        Some(CheckedStmt::Iterate {
            bindings: checked_bindings,
            source,
            kind,
            body,
        })
    }

    fn expect_loop_source(
        &mut self,
        expr: &Expr,
    ) -> (CheckedExpr, LoopSourceKind, CanonicalType, CanonicalType) {
        let fallback = (
            self.dummy(None),
            LoopSourceKind::Values,
            CanonicalType::Int64,
            CanonicalType::Int64,
        );
        let call = match unwrap_parens(expr) {
            Expr::Call(call) => call,
            _ => {
                self.error(
                    expr.span(),
                    "a loop iterable must be iter(), enumerate(), keys(), values(), or entries()",
                );
                return (
                    self.check_expr(expr, None, false),
                    LoopSourceKind::Values,
                    CanonicalType::Int64,
                    CanonicalType::Int64,
                );
            }
        };
        let Expr::Member(member) = &*call.callee else {
            self.error(
                expr.span(),
                "a loop iterable must be iter(), enumerate(), keys(), values(), or entries()",
            );
            return fallback;
        };
        let name = member.member.name.as_str();
        if !call.arguments.is_empty() {
            self.error(
                call.span,
                format!(
                    "method '{name}' expects 0 arguments, found {}",
                    call.arguments.len()
                ),
            );
        }
        if matches!(name, "keys" | "values" | "entries") {
            return self.map_loop_source(member, name, call.span);
        }
        let kind = match name {
            "iter" => LoopSourceKind::Values,
            "enumerate" => LoopSourceKind::Enumerate,
            _ => {
                self.error(
                    member.member.span,
                    "a loop iterable must be iter(), enumerate(), keys(), values(), or entries()",
                );
                LoopSourceKind::Values
            }
        };
        let object = self.check_expr(&member.object, None, false);
        if !object.ty().is_array() {
            self.error(
                member.object.span(),
                format!("'{}' is not an array", self.spell(object.ty())),
            );
        }
        let element = object.ty().element().unwrap_or(CanonicalType::Int64);
        (object, kind, element, element)
    }

    fn map_loop_source(
        &mut self,
        member: &MemberExpr,
        name: &str,
        span: Span,
    ) -> (CheckedExpr, LoopSourceKind, CanonicalType, CanonicalType) {
        let object = self.check_expr(&member.object, None, false);
        if !object.ty().is_map() {
            self.error(
                member.object.span(),
                format!("'{}' is not a map", self.spell(object.ty())),
            );
        }
        let map_ty = object.ty();
        let (key_ty, value_ty) = self.map_parts(map_ty);
        let method = match name {
            "keys" => MapMethod::Keys,
            "values" => MapMethod::Values,
            _ => MapMethod::Entries,
        };
        let ty = match method {
            MapMethod::Keys => key_ty.nest(),
            MapMethod::Values => value_ty.nest(),
            _ => value_ty,
        };
        let (file, line, column) = self.located(span);
        let source = CheckedExpr::MapCall {
            place: None,
            object: Some(Box::new(object)),
            method,
            args: Vec::new(),
            ty,
            key_ty,
            value_ty,
            type_name: self.spell(map_ty),
            file,
            line,
            column,
        };
        match method {
            MapMethod::Keys => (source, LoopSourceKind::Values, key_ty, key_ty),
            MapMethod::Values => (source, LoopSourceKind::Values, value_ty, value_ty),
            _ => (source, LoopSourceKind::Entries, key_ty, value_ty),
        }
    }

    fn check_index_operand(&mut self, expr: &Expr) -> CheckedExpr {
        let checked = self.check_expr(expr, None, false);
        if !checked.ty().is_integer() {
            self.error(
                expr.span(),
                format!(
                    "an index must be an integer, found '{}'",
                    self.spell(checked.ty())
                ),
            );
        }
        checked
    }

    fn reject_binding_write(&mut self, name: &str, span: Span, update: bool) {
        let Some((scope, binding)) = self.binding_at(name) else {
            return;
        };
        self.reject_iterated_write(name, scope, span);
        if !binding.mutable {
            let verb = if update { "modify" } else { "assign to" };
            self.error(span, format!("cannot {verb} constant '{name}'"));
        }
    }

    fn reject_iterated_write(&mut self, name: &str, scope: usize, span: Span) {
        if self
            .iterating
            .iter()
            .any(|lock| lock.name == name && lock.scope == scope)
        {
            self.error(
                span,
                format!("cannot modify array '{name}' while it is being iterated"),
            );
        }
    }

    fn binding_at(&self, name: &str) -> Option<(usize, Binding)> {
        for (scope, bindings) in self.scopes.iter().enumerate().rev() {
            if let Some(binding) = bindings.get(name) {
                return Some((scope, *binding));
            }
        }
        None
    }

    fn located(&self, span: Span) -> (String, u32, u32) {
        (
            self.source_name.to_string(),
            span.start.line,
            span.start.column,
        )
    }

    fn load(&mut self, ident: &Identifier) -> CheckedExpr {
        match self.lookup(&ident.name) {
            Some(binding) => CheckedExpr::Load {
                name: ident.name.clone(),
                ty: binding.ty,
            },
            None => {
                self.error(ident.span, format!("unknown name '{}'", ident.name));
                self.dummy(None)
            }
        }
    }

    fn integer_literal(
        &mut self,
        lexeme: &str,
        expected: Option<CanonicalType>,
        span: Span,
    ) -> CheckedExpr {
        let ty = expected
            .filter(|ty| ty.is_integer())
            .unwrap_or(CanonicalType::Int64);
        let Some(value) = parse_i128(lexeme) else {
            self.error(span, "integer literal is out of range");
            return CheckedExpr::Int { value: 0, ty };
        };
        self.finish_integer(value, ty, span)
    }

    fn finish_integer(&mut self, value: i128, ty: CanonicalType, span: Span) -> CheckedExpr {
        if let Some((min, max)) = int_bounds(ty) {
            if value < min || value > max {
                self.error(
                    span,
                    format!("integer literal does not fit in '{}'", self.spell(ty)),
                );
            }
        }
        CheckedExpr::Int { value, ty }
    }

    fn float_literal(
        &mut self,
        lexeme: &str,
        expected: Option<CanonicalType>,
        span: Span,
    ) -> CheckedExpr {
        let ty = expected
            .filter(|ty| ty.is_float())
            .unwrap_or(CanonicalType::Flt64);
        self.finish_float(lexeme, ty, span)
    }

    fn finish_float(&mut self, lexeme: &str, ty: CanonicalType, span: Span) -> CheckedExpr {
        let parsed = lexeme.parse::<f64>().ok().filter(|value| value.is_finite());
        match parsed {
            Some(value) => match float_problem(value, ty) {
                None => CheckedExpr::Float {
                    lexeme: lexeme.to_string(),
                    ty,
                },
                Some(problem) => {
                    self.error(
                        span,
                        format!("floating-point literal {problem} '{}'", self.spell(ty)),
                    );
                    CheckedExpr::Float {
                        lexeme: "0.0".to_string(),
                        ty,
                    }
                }
            },
            None => {
                self.error(span, "floating-point literal is out of range");
                CheckedExpr::Float {
                    lexeme: "0.0".to_string(),
                    ty,
                }
            }
        }
    }

    fn negated_literal(
        &mut self,
        operand: &Expr,
        expected: Option<CanonicalType>,
        span: Span,
    ) -> Option<CheckedExpr> {
        match unwrap_parens(operand) {
            Expr::Literal(LiteralExpr::Integer { lexeme, .. }) => {
                let ty = expected
                    .filter(|ty| ty.is_integer())
                    .unwrap_or(CanonicalType::Int64);
                let Some(magnitude) = parse_i128(lexeme) else {
                    self.error(span, "integer literal is out of range");
                    return Some(CheckedExpr::Int { value: 0, ty });
                };
                let Some(value) = magnitude.checked_neg() else {
                    self.error(span, "integer literal is out of range");
                    return Some(CheckedExpr::Int { value: 0, ty });
                };
                Some(self.finish_integer(value, ty, span))
            }
            Expr::Literal(LiteralExpr::Float { lexeme, .. }) => {
                let ty = expected
                    .filter(|ty| ty.is_float())
                    .unwrap_or(CanonicalType::Flt64);
                Some(self.finish_float(&format!("-{lexeme}"), ty, span))
            }
            _ => None,
        }
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(*binding);
            }
        }
        None
    }

    fn reject_unspecified_type(&mut self, _ty: CanonicalType, _span: Span) -> bool {
        false
    }

    fn dummy(&self, expected: Option<CanonicalType>) -> CheckedExpr {
        match expected.unwrap_or(CanonicalType::Int64) {
            CanonicalType::Str => CheckedExpr::Str(String::new()),
            CanonicalType::Bln => CheckedExpr::Bool(false),
            CanonicalType::Flt32 | CanonicalType::Flt64 => CheckedExpr::Float {
                lexeme: "0.0".to_string(),
                ty: expected.unwrap_or(CanonicalType::Flt64),
            },
            CanonicalType::Chr => CheckedExpr::Char('\0'),
            CanonicalType::Void => CheckedExpr::Int {
                value: 0,
                ty: CanonicalType::Int64,
            },
            ty => CheckedExpr::Int { value: 0, ty },
        }
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        let message = message.into();
        if self.diagnostics.last().is_some_and(|last| {
            last.span.start.offset == span.start.offset && last.message == message
        }) {
            return;
        }
        self.diagnostics.push(SemanticDiagnostic {
            source_name: self.source_name.to_string(),
            span,
            message,
        });
    }
}

fn function_params(function: &crate::manifest::Function) -> Vec<(CanonicalType, bool)> {
    function
        .params
        .iter()
        .filter_map(|param| manifest_type(&param.ty).map(|ty| (ty, param.ty.auto)))
        .collect()
}

fn manifest_type(ty: &ManifestType) -> Option<CanonicalType> {
    canonical_name(&ty.name)
}

fn is_numeric_literal(expr: &Expr) -> bool {
    match unwrap_parens(expr) {
        Expr::Literal(LiteralExpr::Integer { .. } | LiteralExpr::Float { .. }) => true,
        Expr::Unary(expr) if expr.operator == UnaryOperator::Negate => {
            is_numeric_literal(&expr.operand)
        }
        _ => false,
    }
}

fn unwrap_parens(expr: &Expr) -> &Expr {
    match expr {
        Expr::Parenthesized { expression, .. } => unwrap_parens(expression),
        other => other,
    }
}

fn float_problem(value: f64, ty: CanonicalType) -> Option<&'static str> {
    if ty != CanonicalType::Flt32 {
        return None;
    }
    let narrow = value as f32;
    if !narrow.is_finite() {
        return Some("does not fit in");
    }
    if narrow as f64 != value {
        return Some("is not exactly representable as");
    }
    None
}

fn operator_token(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
    }
}

fn parse_i128(lexeme: &str) -> Option<i128> {
    let mut value: i128 = 0;
    if lexeme.is_empty() {
        return None;
    }
    for ch in lexeme.chars() {
        let digit = ch.to_digit(10)?;
        value = value.checked_mul(10)?.checked_add(i128::from(digit))?;
    }
    Some(value)
}

fn int_bounds(ty: CanonicalType) -> Option<(i128, i128)> {
    Some(match ty {
        CanonicalType::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        CanonicalType::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        CanonicalType::Int64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        CanonicalType::UInt16 => (0, i128::from(u16::MAX)),
        CanonicalType::UInt32 => (0, i128::from(u32::MAX)),
        CanonicalType::UInt64 => (0, i128::from(u64::MAX)),
        _ => return None,
    })
}

fn guaranteed_return(statements: &[Stmt]) -> bool {
    statements.iter().any(stmt_returns)
}

#[cfg(test)]
mod tests;

fn stmt_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(_) => true,
        Stmt::Block(block) => guaranteed_return(&block.statements),
        Stmt::If(stmt) => {
            guaranteed_return(&stmt.then_branch.statements)
                && stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| stmt_returns(branch))
        }
        _ => false,
    }
}
