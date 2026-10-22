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
//! `lengthOf` and `typeOf` are reserved built-ins. `lengthOf` takes one `str`,
//! array, record, or map and yields `uint64`. Strings count Unicode scalar
//! values, arrays and maps count elements, and records count the fields
//! declared directly by their named type. `typeOf` takes one value and yields
//! a `Type` descriptor for that value's static type. A variable, parameter,
//! function, field, or type named `lengthOf` or `typeOf` is rejected. `Type`
//! itself is reserved only as a type name.
//!
//! Named records are nominal. A record literal needs an expected record type,
//! and later assignments to an `auto` record do not convert another record
//! into it. A field written `auto T` stores `T` and converts writes through
//! the same registry as an `auto T` variable. A field written `T` stays an
//! ordinary typed slot.

use std::collections::HashMap;

use crate::manifest::{Manifest, ManifestType};
use crate::parser::*;
use crate::span::Span;

pub(crate) mod classes;
pub(crate) mod convert;
pub(crate) mod numeric;

use convert::{ConvOp, conversion};
use numeric::{CompareMode, NumericError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

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
    Type,
    ErrorHandle,
    Record(RecordId),
    Class(ClassId),
    Map(MapId),
    Array {
        leaf: Leaf,
        depth: u8,
        /// `limits[i]` is dimension `i`, innermost first. `None` is dynamic.
        limits: [Option<u64>; MAX_ARRAY_RANK],
    },
    /// `T?`. The inner type is never itself optional.
    Optional(OptionalKind),
}

/// The type inside `T?`. One level of nullability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalKind {
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
    Type,
    Record(RecordId),
    Class(ClassId),
    Map(MapId),
    Array {
        leaf: Leaf,
        depth: u8,
        limits: [Option<u64>; MAX_ARRAY_RANK],
    },
}

impl OptionalKind {
    pub(crate) fn canonical(self) -> CanonicalType {
        match self {
            OptionalKind::Int16 => CanonicalType::Int16,
            OptionalKind::Int32 => CanonicalType::Int32,
            OptionalKind::Int64 => CanonicalType::Int64,
            OptionalKind::UInt16 => CanonicalType::UInt16,
            OptionalKind::UInt32 => CanonicalType::UInt32,
            OptionalKind::UInt64 => CanonicalType::UInt64,
            OptionalKind::Flt32 => CanonicalType::Flt32,
            OptionalKind::Flt64 => CanonicalType::Flt64,
            OptionalKind::Bln => CanonicalType::Bln,
            OptionalKind::Chr => CanonicalType::Chr,
            OptionalKind::Str => CanonicalType::Str,
            OptionalKind::Type => CanonicalType::Type,
            OptionalKind::Record(id) => CanonicalType::Record(id),
            OptionalKind::Class(id) => CanonicalType::Class(id),
            OptionalKind::Map(id) => CanonicalType::Map(id),
            OptionalKind::Array {
                leaf,
                depth,
                limits,
            } => CanonicalType::Array {
                leaf,
                depth,
                limits,
            },
        }
    }
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
    Type,
    ErrorHandle,
    Record(RecordId),
    Class(ClassId),
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
            CanonicalType::Type => "GrfType".to_string(),
            CanonicalType::ErrorHandle => "graphite_core::diagnostic::RuntimeError".to_string(),
            CanonicalType::Class(_) => "graphite_core::heap::Rooted".to_string(),
            CanonicalType::Record(_) => "GrfRecord".to_string(),
            CanonicalType::Map(_) => "HashMap<(), ()>".to_string(),
            CanonicalType::Array { leaf, depth, .. } => {
                let mut name = leaf.rust_name();
                for _ in 0..depth {
                    name = format!("Vec<{name}>");
                }
                name
            }
            CanonicalType::Optional(kind) => format!("Option<{}>", kind.canonical().rust_name()),
        }
    }

    pub fn optional(self) -> Option<CanonicalType> {
        let kind = match self {
            CanonicalType::Optional(_) | CanonicalType::Void | CanonicalType::ErrorHandle => {
                return None;
            }
            CanonicalType::Int16 => OptionalKind::Int16,
            CanonicalType::Int32 => OptionalKind::Int32,
            CanonicalType::Int64 => OptionalKind::Int64,
            CanonicalType::UInt16 => OptionalKind::UInt16,
            CanonicalType::UInt32 => OptionalKind::UInt32,
            CanonicalType::UInt64 => OptionalKind::UInt64,
            CanonicalType::Flt32 => OptionalKind::Flt32,
            CanonicalType::Flt64 => OptionalKind::Flt64,
            CanonicalType::Bln => OptionalKind::Bln,
            CanonicalType::Chr => OptionalKind::Chr,
            CanonicalType::Str => OptionalKind::Str,
            CanonicalType::Type => OptionalKind::Type,
            CanonicalType::Record(id) => OptionalKind::Record(id),
            CanonicalType::Class(id) => OptionalKind::Class(id),
            CanonicalType::Map(id) => OptionalKind::Map(id),
            CanonicalType::Array {
                leaf,
                depth,
                limits,
            } => OptionalKind::Array {
                leaf,
                depth,
                limits,
            },
        };
        Some(CanonicalType::Optional(kind))
    }

    pub fn optional_inner(self) -> Option<CanonicalType> {
        match self {
            CanonicalType::Optional(kind) => Some(kind.canonical()),
            _ => None,
        }
    }

    pub fn is_optional(self) -> bool {
        matches!(self, CanonicalType::Optional(_))
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
            CanonicalType::Type => "Type".to_string(),
            CanonicalType::ErrorHandle => "RuntimeError".to_string(),
            CanonicalType::Class(_) => "class".to_string(),
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
            CanonicalType::Optional(kind) => format!("{}?", kind.canonical().spelling()),
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
                | CanonicalType::ErrorHandle
                | CanonicalType::Record(_)
                | CanonicalType::Class(_)
                | CanonicalType::Map(_)
                | CanonicalType::Array { .. }
                | CanonicalType::Optional(_)
        )
    }

    pub fn is_record(self) -> bool {
        matches!(self, CanonicalType::Record(_))
    }

    pub fn is_class(self) -> bool {
        matches!(self, CanonicalType::Class(_))
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
            CanonicalType::Type => Leaf::Type,
            CanonicalType::ErrorHandle => Leaf::ErrorHandle,
            CanonicalType::Record(id) => Leaf::Record(id),
            CanonicalType::Class(id) => Leaf::Class(id),
            CanonicalType::Map(id) => Leaf::Map(id),
            CanonicalType::Array { leaf, .. } => leaf,
            CanonicalType::Optional(_) => Leaf::Void,
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
            Leaf::Type => CanonicalType::Type,
            Leaf::ErrorHandle => CanonicalType::ErrorHandle,
            Leaf::Record(id) => CanonicalType::Record(id),
            Leaf::Class(id) => CanonicalType::Class(id),
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
            Leaf::Type => "Type",
            Leaf::ErrorHandle => "RuntimeError",
            Leaf::Record(_) => "record",
            Leaf::Class(_) => "class",
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
    pub code: &'static str,
    pub help: Option<String>,
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
    pub classes: Vec<CheckedClass>,
    pub maps: Vec<MapShape>,
    pub functions: Vec<CheckedFn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedClass {
    pub name: String,
    pub fields: Vec<CheckedClassField>,
    pub init: Option<CheckedInit>,
    pub methods: Vec<CheckedMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedClassField {
    pub name: String,
    pub ty: CanonicalType,
    pub const_field: bool,
    pub initializer: Option<CheckedExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedInit {
    pub params: Vec<(String, CanonicalType)>,
    pub body: Vec<CheckedStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedMethod {
    pub name: String,
    pub params: Vec<(String, CanonicalType)>,
    pub ret: CanonicalType,
    pub body: Vec<CheckedStmt>,
    pub mutating: bool,
    pub is_static: bool,
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
        end_column: u32,
    },
    Iterate {
        bindings: Vec<(String, CanonicalType, bool)>,
        source: CheckedExpr,
        kind: LoopSourceKind,
        body: Vec<CheckedStmt>,
    },
    Try {
        body: Vec<CheckedStmt>,
        catches: Vec<CheckedCatch>,
        finally_body: Option<Vec<CheckedStmt>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedCatch {
    pub kind: String,
    pub binding: String,
    pub body: Vec<CheckedStmt>,
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
        end_column: u32,
    },
    MapIndex {
        object: Box<ArrayPlace>,
        key: Box<CheckedExpr>,
        value_ty: CanonicalType,
        type_name: String,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    /// A field stored on a class instance. Nested record, array, and map
    /// updates copy this value out, change it, and write it back.
    ClassField {
        object: Box<CheckedExpr>,
        class: ClassId,
        field: usize,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
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
        span: Span,
    },
    Binary {
        op: BinaryOperator,
        left: Box<CheckedExpr>,
        right: Box<CheckedExpr>,
        ty: CanonicalType,
        compare: Option<CompareMode>,
        span: Span,
    },
    NumCast {
        from: CanonicalType,
        to: CanonicalType,
        check: bool,
        expr: Box<CheckedExpr>,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
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
        span: Span,
    },
    Call {
        path: String,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        callee: String,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    HandleCall {
        name: String,
        method: String,
        ty: CanonicalType,
    },
    LengthOf {
        expr: Box<CheckedExpr>,
        count: Option<u64>,
    },
    TypeOf {
        expr: Box<CheckedExpr>,
        name: String,
        record: bool,
    },
    TypeName {
        object: Box<CheckedExpr>,
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
        end_column: u32,
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
        end_column: u32,
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
        end_column: u32,
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
        end_column: u32,
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
        end_column: u32,
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
        end_column: u32,
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
        end_column: u32,
    },
    MapInsert {
        place: ArrayPlace,
        key: Box<CheckedExpr>,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
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
        end_column: u32,
    },
    ClassNew {
        class: ClassId,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    ClassGet {
        object: Box<CheckedExpr>,
        class: ClassId,
        field: usize,
        ty: CanonicalType,
    },
    ClassSet {
        object: Box<CheckedExpr>,
        class: ClassId,
        field: usize,
        value: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    ClassCall {
        class: ClassId,
        method: usize,
        object: Option<Box<CheckedExpr>>,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    Interpolated {
        parts: Vec<CheckedPart>,
    },
    Null {
        ty: CanonicalType,
    },
    Wrap {
        expr: Box<CheckedExpr>,
        ty: CanonicalType,
    },
    Unwrap {
        expr: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    Unready {
        ty: CanonicalType,
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
            | CheckedExpr::Call { ty, .. }
            | CheckedExpr::HandleCall { ty, .. } => *ty,
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
            | CheckedExpr::MapCall { ty, .. }
            | CheckedExpr::ClassNew { ty, .. }
            | CheckedExpr::ClassGet { ty, .. }
            | CheckedExpr::ClassSet { ty, .. }
            | CheckedExpr::ClassCall { ty, .. }
            | CheckedExpr::Null { ty }
            | CheckedExpr::Wrap { ty, .. }
            | CheckedExpr::Unwrap { ty, .. }
            | CheckedExpr::Unready { ty } => *ty,
            CheckedExpr::LengthOf { .. } => CanonicalType::UInt64,
            CheckedExpr::TypeOf { .. } => CanonicalType::Type,
            CheckedExpr::TypeName { .. } => CanonicalType::Str,
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

#[derive(Clone)]
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
    ready: bool,
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
        classes: Vec::new(),
        class_names: HashMap::new(),
        maps: Vec::new(),
        resolve_stack: Vec::new(),
        scopes: Vec::new(),
        loop_depth: 0,
        finally_depth: 0,
        current_return: CanonicalType::Void,
        current_class: None,
        current_static: false,
        in_init: false,
        iterating: Vec::new(),
        narrowed: Vec::new(),
    };
    checker.collect_imports(program);
    checker.collect_classes(program);
    checker.collect_records(program);
    checker.finish_class_types();
    checker.prepare_class_effects(program);
    checker.collect_functions(program);
    checker.check_entry_point(program);
    let mut checked_classes = Vec::new();
    for declaration in &program.classes {
        if let Some(checked) = checker.check_class(declaration) {
            checked_classes.push(checked);
        }
    }
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
                classes: checked_classes,
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
    auto: bool,
    span: Span,
}

struct ClassFieldInfo {
    name: String,
    syntax: TypeSyntax,
    ty: CanonicalType,
    auto: bool,
    const_field: bool,
    public: bool,
    span: Span,
}

struct ClassMethodInfo {
    name: String,
    params: Vec<ParamInfo>,
    param_syntax: Vec<RawParam>,
    ret: CanonicalType,
    ret_syntax: TypeSyntax,
    public: bool,
    is_static: bool,
    mutating: bool,
    leaks: bool,
    span: Span,
}

struct ClassInitInfo {
    params: Vec<ParamInfo>,
    param_syntax: Vec<RawParam>,
    public: bool,
}

#[derive(Clone)]
struct RawParam {
    name: String,
    syntax: TypeSyntax,
    auto: bool,
    span: Span,
}

struct ClassInfo {
    name: String,
    span: Span,
    fields: Vec<ClassFieldInfo>,
    methods: Vec<ClassMethodInfo>,
    init: Option<ClassInitInfo>,
}

struct Checker<'a> {
    source_name: &'a str,
    manifest: &'a Manifest,
    diagnostics: Vec<SemanticDiagnostic>,
    functions: HashMap<String, Signature>,
    imports: HashMap<String, ImportBinding>,
    records: Vec<RecordInfo>,
    record_names: HashMap<String, RecordId>,
    classes: Vec<ClassInfo>,
    class_names: HashMap<String, ClassId>,
    maps: Vec<MapShape>,
    resolve_stack: Vec<RecordId>,
    scopes: Vec<HashMap<String, Binding>>,
    loop_depth: u32,
    finally_depth: u32,
    current_return: CanonicalType,
    current_class: Option<ClassId>,
    current_static: bool,
    in_init: bool,
    iterating: Vec<IterLock>,
    narrowed: Vec<HashMap<String, CanonicalType>>,
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

    fn reject_reserved_name(&mut self, name: &str, span: Span) -> bool {
        if name == "lengthOf" || name == "typeOf" {
            self.error(span, format!("'{name}' is a reserved built-in name"));
            return true;
        }
        if matches!(name, "self" | "super" | "crate" | "Self") {
            self.error(span, format!("'{name}' is a reserved name"));
            return true;
        }
        false
    }

    fn collect_records(&mut self, program: &Program) {
        for declaration in &program.types {
            if declaration.name.name == "Type" {
                self.error(declaration.name.span, "'Type' is a built-in type");
                continue;
            }
            if self.reject_reserved_name(&declaration.name.name, declaration.name.span) {
                continue;
            }
            if self.class_names.contains_key(&declaration.name.name) {
                let id = self.class_names[&declaration.name.name];
                let line = self.classes[id.0 as usize].span.start.line;
                self.error(
                    declaration.name.span,
                    format!(
                        "record type '{}' is already declared at line {line}",
                        declaration.name.name
                    ),
                );
                continue;
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
                let _ = self.reject_reserved_name(&field.name.name, field.name.span);
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
                    auto: field.conversion == ConversionMode::Auto,
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
            return self.finish_optional(
                self.with_bounds(CanonicalType::Map(id), &syntax.bounds),
                syntax,
            );
        }
        if syntax.kind == TypeKind::Named && syntax.name == "Type" {
            return self.finish_optional(
                self.with_bounds(CanonicalType::Type, &syntax.bounds),
                syntax,
            );
        }
        if syntax.kind == TypeKind::Named {
            if let Some(id) = self.class_names.get(&syntax.name).copied() {
                return self.finish_optional(
                    self.with_bounds(CanonicalType::Class(id), &syntax.bounds),
                    syntax,
                );
            }
        }
        let scalar = if syntax.kind == TypeKind::Named {
            let Some(id) = self.record_names.get(&syntax.name).copied() else {
                self.error(syntax.span, format!("unknown type '{}'", syntax.name));
                return self.finish_optional(
                    self.with_bounds(CanonicalType::Record(RecordId(u32::MAX)), &syntax.bounds),
                    syntax,
                );
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
        let ty = self.with_bounds(scalar, &syntax.bounds);
        self.finish_optional(ty, syntax)
    }

    fn finish_optional(&mut self, ty: CanonicalType, syntax: &TypeSyntax) -> CanonicalType {
        if !syntax.optional {
            return ty;
        }
        match ty.optional() {
            Some(optional) => optional,
            None => {
                self.error(
                    syntax.span,
                    format!("'{}' cannot be nullable", self.spell(ty)),
                );
                ty
            }
        }
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
            CanonicalType::Class(id) => self.class_name(id),
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
                leaf: Leaf::Class(id),
                depth,
                limits,
            } => {
                let mut name = self.class_name(id);
                for bound in limits.iter().take(depth as usize) {
                    match bound {
                        None => name.push_str("[]"),
                        Some(limit) => name.push_str(&format!("[{limit}]")),
                    }
                }
                name
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
            CanonicalType::Optional(kind) => format!("{}?", self.spell(kind.canonical())),
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

    fn field_auto(&self, place: &ArrayPlace) -> bool {
        let ArrayPlace::Field { object, field, .. } = place else {
            return false;
        };
        let CanonicalType::Record(id) = self.place_type(object) else {
            return false;
        };
        self.records
            .get(id.0 as usize)
            .and_then(|record| record.fields.iter().find(|info| info.name == *field))
            .is_some_and(|info| info.auto)
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
                let _ = self.reject_reserved_name(&parameter.name.name, parameter.name.span);
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
            if self.reject_reserved_name(&function.name.name, function.name.span) {
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
            if name == "lengthOf" || name == "typeOf" {
                continue;
            }
            if *ty != CanonicalType::Void {
                self.scopes.last_mut().unwrap().insert(
                    name.clone(),
                    Binding {
                        ty: *ty,
                        mutable: true,
                        auto: *auto,
                        ready: true,
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
                if self.finally_depth > 0 {
                    self.error(stmt.span, "return is not allowed inside finally");
                }
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
                let (then_narrow, else_narrow) = self.null_split(&stmt.condition);
                let before = self.ready_snapshot();
                self.push_narrow(&then_narrow);
                let then_body = self.check_block_stmts(&stmt.then_branch);
                self.pop_narrow();
                let after_then = self.ready_snapshot();
                self.restore_ready(&before);
                let else_body = if let Some(branch) = &stmt.else_branch {
                    self.push_narrow(&else_narrow);
                    let body = self.check_else(branch);
                    self.pop_narrow();
                    Some(body)
                } else {
                    None
                };
                let after_else = if stmt.else_branch.is_some() {
                    self.ready_snapshot()
                } else {
                    before
                };
                self.merge_ready(&after_then, &after_else);
                Some(CheckedStmt::If {
                    condition,
                    then_body,
                    else_body,
                })
            }
            Stmt::Loop(stmt) => self.check_loop(stmt),
            Stmt::Break(span) => {
                if self.finally_depth > 0 {
                    self.error(*span, "break is not allowed inside finally");
                } else if self.loop_depth == 0 {
                    self.error(*span, "break is outside a loop");
                }
                Some(CheckedStmt::Break)
            }
            Stmt::Continue(span) => {
                if self.finally_depth > 0 {
                    self.error(*span, "continue is not allowed inside finally");
                } else if self.loop_depth == 0 {
                    self.error(*span, "continue is outside a loop");
                }
                Some(CheckedStmt::Continue)
            }
            Stmt::Try(stmt) => Some(self.check_try(stmt)),
            Stmt::Expression(stmt) => {
                let expr = self.check_expr(&stmt.expression, None, true);
                if expr.ty() == CanonicalType::ErrorHandle {
                    self.error(
                        stmt.span,
                        "a runtime error handle cannot be stored, passed, or returned",
                    );
                }
                Some(CheckedStmt::Expr(expr))
            }
        }
    }

    fn check_try(&mut self, stmt: &TryStmt) -> CheckedStmt {
        if stmt.catches.is_empty() && stmt.finally_block.is_none() {
            self.error(stmt.span, "a try statement needs a catch or a finally");
        }
        let body = self.check_block_stmts(&stmt.body);
        let mut seen = Vec::new();
        let mut caught_all = false;
        let mut catches = Vec::new();
        for clause in &stmt.catches {
            let kind = catch_type(&clause.error_type.name);
            if let Some(kind) = kind {
                if caught_all || seen.iter().any(|existing| *existing == kind) {
                    self.error(
                        clause.error_type.span,
                        format!("unreachable catch '{kind}'"),
                    );
                } else {
                    seen.push(kind);
                    if catches_all(kind) {
                        caught_all = true;
                    }
                }
            } else {
                self.error(
                    clause.error_type.span,
                    format!("unknown catch type '{}'", clause.error_type.name),
                );
            }
            self.scopes.push(HashMap::new());
            self.scopes.last_mut().unwrap().insert(
                clause.binding.name.clone(),
                Binding {
                    ty: CanonicalType::ErrorHandle,
                    mutable: false,
                    auto: false,
                    ready: true,
                },
            );
            let catch_body = clause
                .block
                .statements
                .iter()
                .filter_map(|statement| self.check_stmt(statement))
                .collect();
            self.scopes.pop();
            if let Some(kind) = kind {
                catches.push(CheckedCatch {
                    kind: kind.to_string(),
                    binding: clause.binding.name.clone(),
                    body: catch_body,
                });
            }
        }
        self.finally_depth += 1;
        let finally_body = stmt
            .finally_block
            .as_ref()
            .map(|block| self.check_block_stmts(block));
        self.finally_depth -= 1;
        CheckedStmt::Try {
            body,
            catches,
            finally_body,
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
                let before = self.ready_snapshot();
                self.loop_depth += 1;
                let body = self.check_block_stmts(&stmt.body);
                self.loop_depth -= 1;
                self.restore_ready(&before);
                Some(CheckedStmt::Loop { body })
            }
            LoopKind::Condition { condition } => {
                let checked = self.check_expr(condition, Some(CanonicalType::Bln), false);
                if checked.ty() != CanonicalType::Bln {
                    self.error(condition.span(), "a loop condition must have type bln");
                }
                let (then_narrow, _) = self.null_split(condition);
                let before = self.ready_snapshot();
                self.loop_depth += 1;
                self.push_narrow(&then_narrow);
                let body = self.check_block_stmts(&stmt.body);
                self.pop_narrow();
                self.loop_depth -= 1;
                self.restore_ready(&before);
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
                let before = self.ready_snapshot();
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
                self.restore_ready(&before);
                Some(CheckedStmt::CStyle {
                    initializer: init,
                    init_expr,
                    condition: checked_condition,
                    update: checked_update,
                    body,
                })
            }
            LoopKind::Iteration { bindings, iterable } => {
                let before = self.ready_snapshot();
                let result = self.check_iteration(bindings, iterable, stmt);
                self.restore_ready(&before);
                result
            }
        }
    }

    fn default_value(&self, ty: CanonicalType, stack: &mut Vec<RecordId>) -> Option<CheckedExpr> {
        match ty {
            CanonicalType::Int16
            | CanonicalType::Int32
            | CanonicalType::Int64
            | CanonicalType::UInt16
            | CanonicalType::UInt32
            | CanonicalType::UInt64 => Some(CheckedExpr::Int { value: 0, ty }),
            CanonicalType::Flt32 | CanonicalType::Flt64 => Some(CheckedExpr::Float {
                lexeme: "0.0".to_string(),
                ty,
            }),
            CanonicalType::Bln => Some(CheckedExpr::Bool(false)),
            CanonicalType::Chr => Some(CheckedExpr::Char('\0')),
            CanonicalType::Str => Some(CheckedExpr::Str(String::new())),
            CanonicalType::Array { .. } => {
                let element = ty.element()?;
                if let Some(limit) = ty.limit() {
                    let item = self.default_value(element, stack)?;
                    let count = usize::try_from(limit).ok()?;
                    Some(CheckedExpr::Array {
                        elements: vec![item; count],
                        ty,
                    })
                } else {
                    Some(CheckedExpr::Array {
                        elements: Vec::new(),
                        ty,
                    })
                }
            }
            CanonicalType::Map(_) => Some(CheckedExpr::MapLiteral {
                entries: Vec::new(),
                ty,
            }),
            CanonicalType::Record(id) => {
                if stack.contains(&id) {
                    return None;
                }
                stack.push(id);
                let fields = self
                    .records
                    .get(id.0 as usize)
                    .map(|record| {
                        record
                            .fields
                            .iter()
                            .map(|field| field.ty)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut values = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    let Some(value) = self.default_value(*field, stack) else {
                        stack.pop();
                        return None;
                    };
                    values.push((index, value));
                }
                stack.pop();
                Some(CheckedExpr::Record {
                    id,
                    fields: values,
                    ty,
                })
            }
            CanonicalType::Optional(_) => Some(CheckedExpr::Null { ty }),
            CanonicalType::Class(_)
            | CanonicalType::Type
            | CanonicalType::Void
            | CanonicalType::ErrorHandle => None,
        }
    }

    fn can_delay(&self, ty: CanonicalType) -> bool {
        matches!(ty, CanonicalType::Class(_) | CanonicalType::Record(_))
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
        let (value, ready) = if let Some(initializer) = &decl.initializer {
            let value = self.check_expr(initializer, Some(ty), false);
            (
                self.apply_conversion(
                    value,
                    ty,
                    decl.conversion == ConversionMode::Auto,
                    initializer.span(),
                ),
                true,
            )
        } else if let Some(value) = self.default_value(ty, &mut Vec::new()) {
            (value, true)
        } else if self.can_delay(ty) {
            (CheckedExpr::Unready { ty }, false)
        } else {
            self.error(
                decl.name.span,
                format!("type '{}' has no default value", self.spell(ty)),
            );
            return None;
        };
        if self.reject_reserved_name(&decl.name.name, decl.name.span) {
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
                ready,
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
            Expr::Literal(LiteralExpr::Null { span }) => self.null_literal(*span, expected),
            Expr::Identifier(ident) => self.load(ident),
            Expr::Unary(expr) => self.check_unary(expr, expected),
            Expr::Binary(expr) => self.check_binary(expr, expected),
            Expr::Assignment(expr) => self.check_assignment(expr),
            Expr::Update(expr) => self.check_update(expr),
            Expr::New(expr) => self.check_new(expr),
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
                    span: expr.span,
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
                    span: expr.span,
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
                span: expr.span,
            };
        }
        if matches!(
            expr.operator,
            BinaryOperator::Equal | BinaryOperator::NotEqual
        ) && (expr_is_null(&expr.left) || expr_is_null(&expr.right))
        {
            return self.check_null_compare(expr);
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
        ) && left.ty().is_optional()
            && left.ty() == right.ty()
        {
            let inner = left.ty().optional_inner().unwrap_or(left.ty());
            if inner.is_record() || inner.is_map() {
                self.error(
                    expr.span,
                    format!(
                        "operator '{}' cannot compare {} values",
                        operator_token(expr.operator),
                        if inner.is_map() { "map" } else { "record" }
                    ),
                );
            }
            return CheckedExpr::Binary {
                op: expr.operator,
                ty: CanonicalType::Bln,
                left: Box::new(left),
                right: Box::new(right),
                compare: Some(CompareMode::Optional),
                span: expr.span,
            };
        }
        if comparison && (left.ty() == CanonicalType::Type || right.ty() == CanonicalType::Type) {
            let equality = matches!(
                expr.operator,
                BinaryOperator::Equal | BinaryOperator::NotEqual
            );
            if equality && left.ty() == right.ty() {
                return CheckedExpr::Binary {
                    op: expr.operator,
                    ty: CanonicalType::Bln,
                    left: Box::new(left),
                    right: Box::new(right),
                    compare: Some(CompareMode::Same),
                    span: expr.span,
                };
            }
            let message = if equality {
                format!(
                    "operator '{}' cannot compare '{}' with '{}'",
                    operator_token(expr.operator),
                    self.spell(left.ty()),
                    self.spell(right.ty())
                )
            } else {
                format!(
                    "operator '{}' cannot compare type descriptors",
                    operator_token(expr.operator)
                )
            };
            self.error(expr.span, message);
            return CheckedExpr::Binary {
                op: expr.operator,
                ty: CanonicalType::Bln,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span: expr.span,
            };
        }
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
                span: expr.span,
            };
        }
        if left.ty().is_class() || right.ty().is_class() {
            let equality = matches!(
                expr.operator,
                BinaryOperator::Equal | BinaryOperator::NotEqual
            );
            if equality && left.ty() == right.ty() {
                return CheckedExpr::Binary {
                    op: expr.operator,
                    ty: CanonicalType::Bln,
                    left: Box::new(left),
                    right: Box::new(right),
                    compare: Some(CompareMode::Same),
                    span: expr.span,
                };
            }
            let message = if equality {
                format!(
                    "operator '{}' cannot compare '{}' with '{}'",
                    operator_token(expr.operator),
                    self.spell(left.ty()),
                    self.spell(right.ty())
                )
            } else {
                format!(
                    "operator '{}' cannot compare class values",
                    operator_token(expr.operator)
                )
            };
            self.error(expr.span, message);
            return CheckedExpr::Binary {
                op: expr.operator,
                ty: CanonicalType::Bln,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span: expr.span,
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
            span: expr.span,
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
                    span: expr.span,
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
            span: expr.span,
        }
    }

    fn promote(&mut self, expr: CheckedExpr, to: CanonicalType, span: Span) -> CheckedExpr {
        let from = expr.ty();
        if from == to {
            return expr;
        }
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::NumCast {
            from,
            to,
            check: numeric::needs_check(from, to),
            expr: Box::new(expr),
            file,
            line,
            column,
            end_column,
        }
    }

    fn check_assignment(&mut self, expr: &AssignmentExpr) -> CheckedExpr {
        match &*expr.target {
            Expr::Index(index) => {
                return self.check_index_assignment(index, &expr.value, expr.op);
            }
            Expr::Slice(slice) => {
                return self.check_slice_assignment(slice, &expr.value, expr.op);
            }
            Expr::Member(member) => {
                return self.check_field_assignment(member, &expr.value, expr.op);
            }
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
        let current = CheckedExpr::Load {
            name: ident.name.clone(),
            ty,
        };
        let value = self.store_value(expr.op, current, &expr.value, ty, auto, expr.value.span());
        self.mark_ready(&ident.name);
        CheckedExpr::Assign {
            name: ident.name.clone(),
            ty,
            value: Box::new(value),
        }
    }

    fn store_value(
        &mut self,
        op: Option<BinaryOperator>,
        current: CheckedExpr,
        value: &Expr,
        ty: CanonicalType,
        auto: bool,
        span: Span,
    ) -> CheckedExpr {
        let Some(op) = op else {
            let checked = self.check_expr(value, Some(ty), false);
            return self.apply_conversion(checked, ty, auto, span);
        };
        let rhs = if is_numeric_literal(value) {
            self.check_expr(value, Some(current.ty()), false)
        } else {
            self.check_expr(value, None, false)
        };
        let combined = self.finish_op(op, current, rhs, span);
        self.apply_conversion(combined, ty, auto, span)
    }

    fn finish_op(
        &mut self,
        op: BinaryOperator,
        left: CheckedExpr,
        right: CheckedExpr,
        span: Span,
    ) -> CheckedExpr {
        let remainder = op == BinaryOperator::Remainder;
        match numeric::arithmetic(left.ty(), right.ty(), remainder) {
            Ok(ty) => {
                let left = self.promote(left, ty, span);
                let right = self.promote(right, ty, span);
                CheckedExpr::Binary {
                    op,
                    ty,
                    left: Box::new(left),
                    right: Box::new(right),
                    compare: None,
                    span,
                }
            }
            Err(NumericError::Remainder) => {
                self.error(span, "operator '%' requires integer operands");
                CheckedExpr::Binary {
                    op,
                    ty: left.ty(),
                    left: Box::new(left),
                    right: Box::new(right),
                    compare: None,
                    span,
                }
            }
            Err(NumericError::Incompatible) => {
                self.error(
                    span,
                    format!(
                        "operator '{}' requires two operands of the same numeric type",
                        operator_token(op)
                    ),
                );
                CheckedExpr::Binary {
                    op,
                    ty: left.ty(),
                    left: Box::new(left),
                    right: Box::new(right),
                    compare: None,
                    span,
                }
            }
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
            span: expr.span,
        }
    }

    fn check_call(&mut self, expr: &CallExpr, allow_void: bool) -> CheckedExpr {
        if let Expr::Member(member) = &*expr.callee {
            if let Expr::Identifier(object) = &*member.object {
                if self.imports.contains_key(&object.name) {
                    return self.check_library_call(member, &expr.arguments, expr.span, allow_void);
                }
                if self.lookup(&object.name).is_none()
                    && self.class_names.contains_key(&object.name)
                {
                    return self.check_static_call(member, &expr.arguments, expr.span);
                }
            }
            return self.check_array_method(member, &expr.arguments, expr.span);
        }
        if let Expr::Identifier(ident) = &*expr.callee {
            if ident.name == "lengthOf" {
                return self.check_length_of(&expr.arguments, expr.span);
            }
            if ident.name == "typeOf" {
                return self.check_type_of(&expr.arguments, expr.span);
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
            callee: member.member.name.clone(),
            args,
            ty: ret,
            file: self.source_name.to_string(),
            line: span.start.line,
            column: span.start.column,
            end_column: span.end.column,
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

    fn check_type_of(&mut self, arguments: &[Expr], span: Span) -> CheckedExpr {
        if arguments.len() != 1 {
            self.error(
                span,
                format!("typeOf expects 1 argument, found {}", arguments.len()),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(Some(CanonicalType::Type));
        }
        let argument = &arguments[0];
        let value = self.check_expr(argument, None, true);
        if value.ty() == CanonicalType::Void || value.ty().is_void_array() {
            self.error(argument.span(), "typeOf requires a value");
            return self.dummy(Some(CanonicalType::Type));
        }
        let described = value.ty();
        CheckedExpr::TypeOf {
            name: self.spell(described),
            record: described.is_record(),
            expr: Box::new(value),
        }
    }

    fn check_user_call(
        &mut self,
        ident: &Identifier,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        if self.class_names.contains_key(&ident.name) && !self.functions.contains_key(&ident.name) {
            self.error(ident.span, format!("construct '{}' with new", ident.name));
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            let ty = self
                .class_names
                .get(&ident.name)
                .copied()
                .map(CanonicalType::Class);
            return self.dummy(ty);
        }
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
            callee: ident.name.clone(),
            args,
            ty: ret,
            file: self.source_name.to_string(),
            line: span.start.line,
            column: span.start.column,
            end_column: span.end.column,
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
        if from == CanonicalType::ErrorHandle && target != CanonicalType::ErrorHandle {
            self.error(
                span,
                "a runtime error handle cannot be stored, passed, or returned",
            );
            return expr;
        }
        if from == target {
            return expr;
        }
        if let Some(inner) = target.optional_inner() {
            if matches!(expr, CheckedExpr::Null { .. }) {
                return CheckedExpr::Null { ty: target };
            }
            if from == inner {
                return CheckedExpr::Wrap {
                    expr: Box::new(expr),
                    ty: target,
                };
            }
        }
        if matches!(expr, CheckedExpr::Null { .. }) {
            self.error(span, "null requires a nullable type");
            return expr;
        }
        if from.is_optional() && from.optional_inner() == Some(target) {
            self.error(span, "a nullable value must be checked before it is used");
            return expr;
        }
        if target == CanonicalType::Str && allow && from.is_optional() {
            let inner = from.optional_inner().unwrap_or(from);
            if inner == CanonicalType::Str || conversion(inner, CanonicalType::Str).is_some() {
                let (file, line, column, end_column) = self.located(span);
                return CheckedExpr::Convert {
                    from,
                    to: target,
                    op: ConvOp::OptionalToStr,
                    expr: Box::new(expr),
                    file,
                    line,
                    column,
                    end_column,
                };
            }
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
                    end_column: span.end.column,
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
        let help = self.conversion_help(&expr, from, target);
        self.error_help(
            span,
            format!(
                "type mismatch: expected '{}', found '{}'",
                self.spell(target),
                self.spell(from)
            ),
            help,
        );
        expr
    }

    fn conversion_help(
        &self,
        expr: &CheckedExpr,
        from: CanonicalType,
        target: CanonicalType,
    ) -> Option<String> {
        let op = conversion(from, target)?;
        if matches!(
            op,
            ConvOp::F64ToSigned
                | ConvOp::F64ToUnsigned
                | ConvOp::F32ToSigned
                | ConvOp::F32ToUnsigned
                | ConvOp::F64ToF32
        ) {
            return None;
        }
        if let CheckedExpr::Str(text) = expr {
            if !string_literal_converts(text, target) {
                return None;
            }
            return Some(format!(
                "convert the value explicitly, for example `{}(\"{text}\")`",
                self.spell(target)
            ));
        }
        Some(format!(
            "an explicit conversion to '{}' is allowed; it is checked when the program runs",
            self.spell(target)
        ))
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
                    .map(|field| (field.name.clone(), field.ty, field.auto))
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
            let Some(position) = declared
                .iter()
                .position(|(field, _, _)| field == &name.name)
            else {
                self.error(
                    name.span,
                    format!("record literal has unknown field '{}'", name.name),
                );
                let _ = self.check_expr(value, None, false);
                continue;
            };
            let field_ty = declared[position].1;
            let auto = declared[position].2;
            let checked = self.check_expr(value, Some(field_ty), false);
            let checked = self.apply_conversion(checked, field_ty, auto, value.span());
            initializers.push((position, checked));
        }
        for (name, _, _) in &declared {
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
        let object = self.adopt_object(object, expr.object.span());
        if object.ty() == CanonicalType::ErrorHandle {
            self.error(
                expr.span,
                "call a runtime error method, for example error.message()",
            );
            return self.dummy(None);
        }
        if let CanonicalType::Class(id) = object.ty() {
            return self.finish_class_get(object, id, expr);
        }
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

    fn check_field_assignment(
        &mut self,
        member: &MemberExpr,
        value: &Expr,
        op: Option<BinaryOperator>,
    ) -> CheckedExpr {
        if self.peek_type(&member.object).is_some_and(|ty| {
            ty.is_class() || ty.optional_inner().is_some_and(|inner| inner.is_class())
        }) {
            let object = self.check_expr(&member.object, None, false);
            let object = self.adopt_object(object, member.object.span());
            if let CanonicalType::Class(id) = object.ty() {
                return self.finish_class_set(object, id, member, value, op);
            }
        }
        let before = self.diagnostics.len();
        if let Some(place) = self.check_place(&Expr::Member(member.clone()), true) {
            let ty = self.place_type(&place);
            let auto = self.field_auto(&place);
            let current = self.check_expr(&Expr::Member(member.clone()), None, false);
            let value = self.store_value(op, current, value, ty, auto, value.span());
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
        let (file, line, column, end_column) = self.located(expr.span);
        CheckedExpr::IndexGet {
            object: Box::new(object),
            index: Box::new(index),
            ty,
            file,
            line,
            column,
            end_column,
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
        let (file, line, column, end_column) = self.located(expr.span);
        CheckedExpr::SliceGet {
            object: Box::new(object),
            start,
            stop,
            step,
            ty,
            file,
            line,
            column,
            end_column,
        }
    }

    fn check_index_assignment(
        &mut self,
        index: &IndexExpr,
        value: &Expr,
        op: Option<BinaryOperator>,
    ) -> CheckedExpr {
        if op.is_some() && self.peek_type(&index.object).is_some_and(|ty| ty.is_map()) {
            self.error(index.span, "compound assignment is not supported for maps");
            let _ = self.check_expr(value, None, false);
            let _ = self.check_expr(&index.index, None, false);
            return self.dummy(None);
        }
        if let Some(place) = self.check_place(&index.object, true) {
            let object_ty = self.place_type(&place);
            if object_ty.is_map() {
                return self.check_map_insert(place, object_ty, index, value);
            }
            let checked_index = self.check_index_operand(&index.index);
            let (file, line, column, end_column) = self.located(index.span);
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
            let current = CheckedExpr::IndexGet {
                object: Box::new(self.place_load(&place)),
                index: Box::new(checked_index.clone()),
                ty: element,
                file: file.clone(),
                line,
                column,
                end_column,
            };
            let value = self.store_value(op, current, value, element, false, value.span());
            return CheckedExpr::IndexSet {
                place: Some(place),
                object: None,
                index: Box::new(checked_index),
                value: Box::new(value),
                ty: element,
                file,
                line,
                column,
                end_column,
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
        let (file, line, column, end_column) = self.located(index.span);
        let element = if object.ty().is_array() {
            object.ty().element().unwrap_or(CanonicalType::Int64)
        } else {
            self.error(
                index.object.span(),
                format!("'{}' is not an array", self.spell(object.ty())),
            );
            CanonicalType::Int64
        };
        let current = CheckedExpr::IndexGet {
            object: Box::new(object.clone()),
            index: Box::new(checked_index.clone()),
            ty: element,
            file: file.clone(),
            line,
            column,
            end_column,
        };
        let value = self.store_value(op, current, value, element, false, value.span());
        CheckedExpr::IndexSet {
            place: None,
            object: Some(Box::new(object)),
            index: Box::new(checked_index),
            value: Box::new(value),
            ty: element,
            file,
            line,
            column,
            end_column,
        }
    }

    fn check_slice_assignment(
        &mut self,
        slice: &SliceExpr,
        value: &Expr,
        op: Option<BinaryOperator>,
    ) -> CheckedExpr {
        if op.is_some() {
            self.error(
                slice.span,
                "compound assignment is not supported for slices",
            );
            let _ = self.check_expr(value, None, false);
            return self.dummy(None);
        }
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
        let (file, line, column, end_column) = self.located(slice.span);
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
            end_column,
        }
    }

    fn place_load(&mut self, place: &ArrayPlace) -> CheckedExpr {
        match place {
            ArrayPlace::Var(name) => {
                let ty = self
                    .lookup(name)
                    .map(|binding| binding.ty)
                    .unwrap_or(CanonicalType::Int64);
                CheckedExpr::Load {
                    name: name.clone(),
                    ty,
                }
            }
            ArrayPlace::Field { object, field, ty } => {
                let object_expr = self.place_load(object);
                if let CanonicalType::Record(id) = object_expr.ty() {
                    if let Some((index, _)) = self.field_index(id, field) {
                        return CheckedExpr::FieldGet {
                            object: Box::new(object_expr),
                            record: id,
                            field: index,
                            ty: *ty,
                        };
                    }
                }
                self.dummy(Some(*ty))
            }
            ArrayPlace::Index {
                object,
                index,
                file,
                line,
                column,
                end_column,
            } => {
                let object_expr = self.place_load(object);
                let element = object_expr.ty().element().unwrap_or(CanonicalType::Int64);
                CheckedExpr::IndexGet {
                    object: Box::new(object_expr),
                    index: index.clone(),
                    ty: element,
                    file: file.clone(),
                    line: *line,
                    column: *column,
                    end_column: *end_column,
                }
            }
            ArrayPlace::MapIndex { value_ty, .. } => self.dummy(Some(*value_ty)),
            ArrayPlace::ClassField {
                object,
                class,
                field,
                ty,
                ..
            } => CheckedExpr::ClassGet {
                object: object.clone(),
                class: *class,
                field: *field,
                ty: *ty,
            },
        }
    }

    fn class_field_place(&mut self, member: &MemberExpr, mutating: bool) -> Option<ArrayPlace> {
        let object = self.check_expr(&member.object, None, false);
        let CanonicalType::Class(id) = object.ty() else {
            return None;
        };
        let Some(index) = self.classes[id.0 as usize]
            .fields
            .iter()
            .position(|field| field.name == member.member.name)
        else {
            if self.classes[id.0 as usize]
                .methods
                .iter()
                .any(|method| method.name == member.member.name)
            {
                self.error(
                    member.span,
                    format!("method '{}' must be called", member.member.name),
                );
            } else {
                self.error(
                    member.member.span,
                    format!("unknown field '{}'", member.member.name),
                );
            }
            return None;
        };
        let public = self.classes[id.0 as usize].fields[index].public;
        let const_field = self.classes[id.0 as usize].fields[index].const_field;
        let ty = self.classes[id.0 as usize].fields[index].ty;
        let name = self.classes[id.0 as usize].fields[index].name.clone();
        if !public && self.current_class != Some(id) {
            self.error(
                member.member.span,
                format!("field '{name}' is not accessible"),
            );
        }
        if mutating && const_field {
            self.error(
                member.span,
                format!("cannot assign to constant field '{name}'"),
            );
        }
        if mutating {
            if let Some(root) = self.frozen_path(&member.object) {
                self.error(member.span, format!("cannot assign to constant '{root}'"));
            }
        }
        let (file, line, column, end_column) = self.located(member.span);
        Some(ArrayPlace::ClassField {
            object: Box::new(object),
            class: id,
            field: index,
            ty,
            file,
            line,
            column,
            end_column,
        })
    }

    fn check_place(&mut self, expr: &Expr, mutating: bool) -> Option<ArrayPlace> {
        match expr {
            Expr::Parenthesized { expression, .. } => self.check_place(expression, mutating),
            Expr::Identifier(ident) => {
                let (scope, binding) = self.binding_at(&ident.name)?;
                if !binding.ready {
                    self.error(
                        ident.span,
                        format!("variable '{}' is used before it is assigned", ident.name),
                    );
                }
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
                if self
                    .peek_type(&member.object)
                    .is_some_and(|ty| ty.is_class())
                {
                    return self.class_field_place(member, mutating);
                }
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
                    let (file, line, column, end_column) = self.located(index.span);
                    return Some(ArrayPlace::MapIndex {
                        object: Box::new(object),
                        key: Box::new(key),
                        value_ty,
                        type_name: self.spell(object_ty),
                        file,
                        line,
                        column,
                        end_column,
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
                let (file, line, column, end_column) = self.located(index.span);
                Some(ArrayPlace::Index {
                    object: Box::new(object),
                    index: Box::new(checked),
                    file,
                    line,
                    column,
                    end_column,
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
            ArrayPlace::ClassField { ty, .. } => *ty,
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
        if let Expr::Identifier(ident) = &*member.object {
            if self
                .lookup(&ident.name)
                .is_some_and(|binding| binding.ty == CanonicalType::ErrorHandle)
            {
                return self.finish_handle_call(&ident.name, name, arguments, span);
            }
        }
        if matches!(name, "iter" | "enumerate" | "entries") {
            self.error(span, format!("{name}() can only be used as a loop source"));
            let _ = self.check_expr(&member.object, None, false);
            return self.dummy(None);
        }
        let mutating = matches!(name, "set" | "remove" | "push" | "insert");
        if self.peek_type(&member.object).is_some_and(|ty| {
            ty.is_class() || ty.optional_inner().is_some_and(|inner| inner.is_class())
        }) {
            let object = self.check_expr(&member.object, None, false);
            let object = self.adopt_object(object, member.object.span());
            if let CanonicalType::Class(id) = object.ty() {
                return self.finish_class_call(object, id, member, arguments, span);
            }
        }
        if let Some(place) = self.check_place(&member.object, mutating) {
            let ty = self.place_type(&place);
            if ty.is_class() {
                let object = self.check_expr(&member.object, None, false);
                if let CanonicalType::Class(id) = object.ty() {
                    return self.finish_class_call(object, id, member, arguments, span);
                }
            }
            if ty.is_map() {
                return self.finish_map_call(name, Some(place), None, arguments, span, member);
            }
            if ty == CanonicalType::Type {
                return self.finish_type_call(name, Some(place), None, arguments, span, member);
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
        if ty == CanonicalType::Type {
            return self.finish_type_call(name, None, Some(object), arguments, span, member);
        }
        if ty.is_record() {
            return self.reject_record_call(member, arguments, ty);
        }
        if let CanonicalType::Class(id) = ty {
            return self.finish_class_call(object, id, member, arguments, span);
        }
        self.finish_array_call(name, None, Some(object), ty, arguments, span, member)
    }

    fn finish_handle_call(
        &mut self,
        name: &str,
        method: &str,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        for argument in arguments {
            let _ = self.check_expr(argument, None, false);
        }
        if !arguments.is_empty() {
            self.error(
                span,
                format!("runtime error method '{method}' expects no arguments"),
            );
        }
        let ty = match method {
            "message" | "code" | "kind" | "sourceFile" | "causeMessage" | "stackTrace" => {
                CanonicalType::Str
            }
            "line" | "column" => CanonicalType::UInt64,
            _ => {
                self.error(
                    span,
                    format!("runtime error handle has no method '{method}'"),
                );
                return self.dummy(None);
            }
        };
        CheckedExpr::HandleCall {
            name: name.to_string(),
            method: method.to_string(),
            ty,
        }
    }

    fn finish_type_call(
        &mut self,
        name: &str,
        place: Option<ArrayPlace>,
        object: Option<CheckedExpr>,
        arguments: &[Expr],
        span: Span,
        member: &MemberExpr,
    ) -> CheckedExpr {
        let object = object.unwrap_or_else(|| self.check_expr(&member.object, None, false));
        let _ = place;
        if name != "name" {
            self.error(member.member.span, format!("Type has no method '{name}'"));
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(Some(CanonicalType::Str));
        }
        if !arguments.is_empty() {
            self.error(
                span,
                format!("name expects 0 arguments, found {}", arguments.len()),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
        }
        CheckedExpr::TypeName {
            object: Box::new(object),
        }
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
        let (file, line, column, end_column) = self.located(span);
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
            end_column,
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
        let (file, line, column, end_column) = self.located(span);
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
            end_column,
        }
    }

    fn check_map_read(&mut self, object: CheckedExpr, expr: &IndexExpr) -> CheckedExpr {
        let map_ty = object.ty();
        let (key_ty, value_ty) = self.map_parts(map_ty);
        let key = self.check_map_key(&expr.index, key_ty);
        let (file, line, column, end_column) = self.located(expr.span);
        CheckedExpr::MapGet {
            object: Box::new(object),
            key: Box::new(key),
            ty: value_ty,
            type_name: self.spell(map_ty),
            file,
            line,
            column,
            end_column,
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
        let (file, line, column, end_column) = self.located(index.span);
        CheckedExpr::MapInsert {
            place,
            key: Box::new(key),
            value: Box::new(checked),
            ty: value_ty,
            file,
            line,
            column,
            end_column,
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
            let _ = self.reject_reserved_name(&binding.name.name, binding.name.span);
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
                    ready: true,
                },
            );
        }
        let (file, line, column, end_column) = self.located(decl.span);
        Some(CheckedStmt::Destructure {
            bindings,
            value,
            file,
            line,
            column,
            end_column,
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
            if self.reject_reserved_name(name, stmt.span) {
                continue;
            }
            self.scopes.last_mut().unwrap().insert(
                name.clone(),
                Binding {
                    ty: *ty,
                    mutable: *mutable,
                    auto: false,
                    ready: true,
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
        let (file, line, column, end_column) = self.located(span);
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
            end_column,
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
        if binding.ty == CanonicalType::ErrorHandle {
            self.error(span, "a runtime error handle cannot be assigned");
            return;
        }
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

    fn located(&self, span: Span) -> (String, u32, u32, u32) {
        (
            self.source_name.to_string(),
            span.start.line,
            span.start.column,
            span.end.column.max(span.start.column),
        )
    }

    fn adopt_object(&mut self, object: CheckedExpr, span: Span) -> CheckedExpr {
        let Some(inner) = object.ty().optional_inner() else {
            return object;
        };
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::Unwrap {
            expr: Box::new(object),
            ty: inner,
            file,
            line,
            column,
            end_column,
        }
    }

    fn load(&mut self, ident: &Identifier) -> CheckedExpr {
        match self.lookup(&ident.name) {
            Some(binding) => {
                if !binding.ready {
                    self.error(
                        ident.span,
                        format!("variable '{}' is used before it is assigned", ident.name),
                    );
                }
                let narrowed = self.narrowed_type(&ident.name);
                if let Some(inner) = narrowed.filter(|ty| *ty != binding.ty) {
                    let (file, line, column, end_column) = self.located(ident.span);
                    CheckedExpr::Unwrap {
                        expr: Box::new(CheckedExpr::Load {
                            name: ident.name.clone(),
                            ty: binding.ty,
                        }),
                        ty: inner,
                        file,
                        line,
                        column,
                        end_column,
                    }
                } else {
                    CheckedExpr::Load {
                        name: ident.name.clone(),
                        ty: binding.ty,
                    }
                }
            }
            None => {
                if ident.name == "this" && self.current_static {
                    self.error(ident.span, "static method cannot use 'this'");
                } else if self.class_names.contains_key(&ident.name) {
                    self.error(ident.span, format!("construct '{}' with new", ident.name));
                } else {
                    self.error(ident.span, format!("unknown name '{}'", ident.name));
                }
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

    fn class_name(&self, id: ClassId) -> String {
        self.classes
            .get(id.0 as usize)
            .map(|class| class.name.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn collect_classes(&mut self, program: &Program) {
        for declaration in &program.classes {
            if declaration.name.name == "Type" {
                self.error(declaration.name.span, "'Type' is a built-in type");
                continue;
            }
            if self.reject_reserved_name(&declaration.name.name, declaration.name.span) {
                continue;
            }
            if let Some(previous) = self.class_names.get(&declaration.name.name).copied() {
                let line = self.classes[previous.0 as usize].span.start.line;
                self.error(
                    declaration.name.span,
                    format!(
                        "class '{}' is already declared at line {line}",
                        declaration.name.name
                    ),
                );
                continue;
            }
            let mut seen = HashMap::new();
            let mut fields = Vec::new();
            for field in &declaration.fields {
                let _ = self.reject_reserved_name(&field.name.name, field.name.span);
                if seen.insert(field.name.name.clone(), field.span).is_some() {
                    self.error(
                        field.name.span,
                        format!("duplicate member '{}'", field.name.name),
                    );
                }
                fields.push(ClassFieldInfo {
                    name: field.name.name.clone(),
                    syntax: field.ty.clone(),
                    ty: CanonicalType::Void,
                    auto: field.conversion == ConversionMode::Auto,
                    const_field: field.const_field,
                    public: field.public,
                    span: field.span,
                });
            }
            let mut methods = Vec::new();
            for method in &declaration.methods {
                let _ = self.reject_reserved_name(&method.name.name, method.name.span);
                if seen
                    .insert(method.name.name.clone(), method.name.span)
                    .is_some()
                {
                    self.error(
                        method.name.span,
                        format!("duplicate member '{}'", method.name.name),
                    );
                }
                methods.push(ClassMethodInfo {
                    name: method.name.name.clone(),
                    params: Vec::new(),
                    param_syntax: self.raw_params(&method.parameters),
                    ret: CanonicalType::Void,
                    ret_syntax: method.return_type.clone(),
                    public: method.public,
                    is_static: method.is_static,
                    mutating: false,
                    leaks: false,
                    span: method.span,
                });
            }
            let init = declaration.init.as_ref().map(|init| ClassInitInfo {
                params: Vec::new(),
                param_syntax: self.raw_params(&init.parameters),
                public: init.public,
            });
            let id = ClassId(self.classes.len() as u32);
            self.class_names.insert(declaration.name.name.clone(), id);
            self.classes.push(ClassInfo {
                name: declaration.name.name.clone(),
                span: declaration.span,
                fields,
                methods,
                init,
            });
        }
    }

    fn raw_params(&self, parameters: &[Parameter]) -> Vec<RawParam> {
        parameters
            .iter()
            .map(|parameter| RawParam {
                name: parameter.name.name.clone(),
                syntax: parameter.ty.clone(),
                auto: parameter.conversion == ConversionMode::Auto,
                span: parameter.span,
            })
            .collect()
    }

    fn resolve_raw_params(&mut self, raw: &[RawParam]) -> Vec<ParamInfo> {
        let mut names = HashMap::new();
        let mut params = Vec::new();
        for parameter in raw {
            let ty = self.resolve_type(&parameter.syntax);
            if ty == CanonicalType::Void {
                self.error(parameter.span, "a parameter cannot have type void");
            }
            if ty.is_void_array() {
                self.error(parameter.span, "void is not a valid array element type");
            }
            if names
                .insert(parameter.name.clone(), parameter.span)
                .is_some()
            {
                self.error(
                    parameter.span,
                    format!("duplicate parameter '{}'", parameter.name),
                );
            }
            let _ = self.reject_reserved_name(&parameter.name, parameter.span);
            params.push(ParamInfo {
                name: parameter.name.clone(),
                ty,
                auto: parameter.auto,
            });
        }
        params
    }

    fn finish_class_types(&mut self) {
        let count = self.classes.len();
        for index in 0..count {
            let fields = self.classes[index]
                .fields
                .iter()
                .map(|field| (field.syntax.clone(), field.span))
                .collect::<Vec<_>>();
            for (field_index, (syntax, span)) in fields.into_iter().enumerate() {
                let ty = self.resolve_type(&syntax);
                if ty == CanonicalType::Void {
                    self.error(span, "a field cannot have type void");
                }
                if ty.is_void_array() {
                    self.error(span, "void is not a valid array element type");
                }
                self.classes[index].fields[field_index].ty = ty;
            }
            let method_count = self.classes[index].methods.len();
            for method_index in 0..method_count {
                let syntax = self.classes[index].methods[method_index].ret_syntax.clone();
                let span = self.classes[index].methods[method_index].span;
                let ret = self.resolve_type(&syntax);
                if ret.is_void_array() {
                    self.error(span, "void is not a valid array element type");
                }
                let raw = self.classes[index].methods[method_index]
                    .param_syntax
                    .clone();
                let params = self.resolve_raw_params(&raw);
                self.classes[index].methods[method_index].ret = ret;
                self.classes[index].methods[method_index].params = params;
            }
            if self.classes[index].init.is_some() {
                let raw = self.classes[index]
                    .init
                    .as_ref()
                    .map(|init| init.param_syntax.clone())
                    .unwrap_or_default();
                let params = self.resolve_raw_params(&raw);
                if let Some(init) = self.classes[index].init.as_mut() {
                    init.params = params;
                }
            }
        }
    }

    fn prepare_class_effects(&mut self, program: &Program) {
        for declaration in &program.classes {
            let Some(id) = self.class_names.get(&declaration.name.name).copied() else {
                continue;
            };
            if self.classes[id.0 as usize].span != declaration.span {
                continue;
            }
            let flags = classes::method_flags(declaration);
            for (method, flag) in self.classes[id.0 as usize].methods.iter_mut().zip(flags) {
                if method.is_static {
                    method.mutating = false;
                    method.leaks = false;
                } else {
                    method.mutating = flag.mutating;
                    method.leaks = flag.leaks;
                }
            }
        }
    }

    fn check_class(&mut self, declaration: &ClassDecl) -> Option<CheckedClass> {
        let id = self.class_names.get(&declaration.name.name).copied()?;
        if self.classes[id.0 as usize].span != declaration.span {
            return None;
        }
        let leaks = self.classes[id.0 as usize]
            .methods
            .iter()
            .map(|method| method.leaks)
            .collect::<Vec<_>>();
        let defaults = declaration
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let ty = self.classes[id.0 as usize].fields[index].ty;
                !field.const_field && self.default_value(ty, &mut Vec::new()).is_some()
            })
            .collect::<Vec<_>>();
        for (span, message) in classes::init_issues(declaration, &leaks, &defaults) {
            self.error(span, message);
        }
        self.current_class = Some(id);
        let mut checked_fields = Vec::new();
        for (index, field) in declaration.fields.iter().enumerate() {
            let ty = self.classes[id.0 as usize].fields[index].ty;
            let auto = self.classes[id.0 as usize].fields[index].auto;
            let name = self.classes[id.0 as usize].fields[index].name.clone();
            let const_field = self.classes[id.0 as usize].fields[index].const_field;
            let initializer = if let Some(expr) = &field.initializer {
                let value = self.check_expr(expr, Some(ty), false);
                Some(self.apply_conversion(value, ty, auto, expr.span()))
            } else if !const_field {
                self.default_value(ty, &mut Vec::new())
            } else {
                None
            };
            checked_fields.push(CheckedClassField {
                name,
                ty,
                const_field,
                initializer,
            });
        }
        self.current_class = None;
        let init = if let Some(init) = &declaration.init {
            let params = self.classes[id.0 as usize]
                .init
                .as_ref()
                .map(|info| info.params.clone())
                .unwrap_or_default();
            self.in_init = true;
            let body = self.check_member_body(
                id,
                &params,
                CanonicalType::Void,
                &init.body,
                init.span,
                "",
                false,
                false,
            );
            self.in_init = false;
            Some(CheckedInit {
                params: params
                    .into_iter()
                    .map(|param| (param.name, param.ty))
                    .collect(),
                body,
            })
        } else {
            None
        };
        let mut methods = Vec::new();
        for (index, method) in declaration.methods.iter().enumerate() {
            let params = self.classes[id.0 as usize].methods[index].params.clone();
            let ret = self.classes[id.0 as usize].methods[index].ret;
            let mutating = self.classes[id.0 as usize].methods[index].mutating;
            let is_static = self.classes[id.0 as usize].methods[index].is_static;
            let name = self.classes[id.0 as usize].methods[index].name.clone();
            let body = self.check_member_body(
                id,
                &params,
                ret,
                &method.body,
                method.span,
                &name,
                true,
                is_static,
            );
            methods.push(CheckedMethod {
                name,
                params: params
                    .into_iter()
                    .map(|param| (param.name, param.ty))
                    .collect(),
                ret,
                body,
                mutating,
                is_static,
            });
        }
        Some(CheckedClass {
            name: declaration.name.name.clone(),
            fields: checked_fields,
            init,
            methods,
        })
    }

    fn check_member_body(
        &mut self,
        id: ClassId,
        params: &[ParamInfo],
        ret: CanonicalType,
        body: &BlockStmt,
        span: Span,
        name: &str,
        require_return: bool,
        is_static: bool,
    ) -> Vec<CheckedStmt> {
        let saved_return = self.current_return;
        let saved_class = self.current_class;
        let saved_static = self.current_static;
        self.current_return = ret;
        self.current_class = Some(id);
        self.current_static = is_static;
        self.scopes.push(HashMap::new());
        if !is_static {
            self.scopes.last_mut().unwrap().insert(
                "this".to_string(),
                Binding {
                    ty: CanonicalType::Class(id),
                    mutable: false,
                    auto: false,
                    ready: true,
                },
            );
        }
        for param in params {
            if param.ty != CanonicalType::Void {
                self.scopes.last_mut().unwrap().insert(
                    param.name.clone(),
                    Binding {
                        ty: param.ty,
                        mutable: true,
                        auto: param.auto,
                        ready: true,
                    },
                );
            }
        }
        let mut checked = Vec::new();
        for statement in &body.statements {
            if let Some(stmt) = self.check_stmt(statement) {
                checked.push(stmt);
            }
        }
        if require_return && ret != CanonicalType::Void && !guaranteed_return(&body.statements) {
            self.error(
                span,
                format!("function '{name}' must return a value on every path"),
            );
        }
        self.scopes.pop();
        self.current_return = saved_return;
        self.current_class = saved_class;
        self.current_static = saved_static;
        checked
    }

    fn check_new(&mut self, expr: &NewExpr) -> CheckedExpr {
        let Some(id) = self.class_names.get(&expr.name.name).copied() else {
            self.error(
                expr.name.span,
                format!("unknown class '{}'", expr.name.name),
            );
            for argument in &expr.arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        };
        self.check_construct(id, &expr.arguments, expr.span)
    }

    fn check_static_call(
        &mut self,
        member: &MemberExpr,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let Expr::Identifier(object) = &*member.object else {
            return self.dummy(None);
        };
        let Some(id) = self.class_names.get(&object.name).copied() else {
            self.error(object.span, format!("unknown class '{}'", object.name));
            return self.dummy(None);
        };
        let Some(index) = self.classes[id.0 as usize]
            .methods
            .iter()
            .position(|method| method.name == member.member.name)
        else {
            self.error(
                member.member.span,
                format!("unknown method '{}'", member.member.name),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        };
        let is_static = self.classes[id.0 as usize].methods[index].is_static;
        let public = self.classes[id.0 as usize].methods[index].public;
        let ret = self.classes[id.0 as usize].methods[index].ret;
        let params = self.classes[id.0 as usize].methods[index]
            .params
            .iter()
            .map(|param| (param.ty, param.auto))
            .collect::<Vec<_>>();
        let name = self.classes[id.0 as usize].methods[index].name.clone();
        let class_name = self.class_name(id);
        if !is_static {
            self.error(
                member.member.span,
                format!("method '{name}' requires an instance of '{class_name}'"),
            );
        }
        if !public && self.current_class != Some(id) {
            self.error(
                member.member.span,
                format!("method '{name}' is not accessible"),
            );
        }
        let args = self.check_arguments(arguments, &params, span);
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::ClassCall {
            class: id,
            method: index,
            object: None,
            args,
            ty: ret,
            file,
            line,
            column,
            end_column,
        }
    }

    fn check_construct(&mut self, id: ClassId, arguments: &[Expr], span: Span) -> CheckedExpr {
        let name = self.class_name(id);
        let init = self.classes[id.0 as usize]
            .init
            .as_ref()
            .map(|init| (init.public, init.params.clone()));
        let (file, line, column, end_column) = self.located(span);
        let args = if let Some((public, params)) = init {
            if !public && self.current_class != Some(id) {
                self.error(span, format!("constructor of '{name}' is not accessible"));
            }
            let pairs = params
                .iter()
                .map(|param| (param.ty, param.auto))
                .collect::<Vec<_>>();
            self.check_arguments(arguments, &pairs, span)
        } else if arguments.is_empty() {
            Vec::new()
        } else {
            self.error(
                span,
                format!("expected 0 arguments, found {}", arguments.len()),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            Vec::new()
        };
        CheckedExpr::ClassNew {
            class: id,
            args,
            ty: CanonicalType::Class(id),
            file,
            line,
            column,
            end_column,
        }
    }

    fn finish_class_get(
        &mut self,
        object: CheckedExpr,
        id: ClassId,
        expr: &MemberExpr,
    ) -> CheckedExpr {
        if let Some(index) = self.classes[id.0 as usize]
            .fields
            .iter()
            .position(|field| field.name == expr.member.name)
        {
            let public = self.classes[id.0 as usize].fields[index].public;
            let ty = self.classes[id.0 as usize].fields[index].ty;
            let name = self.classes[id.0 as usize].fields[index].name.clone();
            if !public && self.current_class != Some(id) {
                self.error(
                    expr.member.span,
                    format!("field '{name}' is not accessible"),
                );
            }
            return CheckedExpr::ClassGet {
                object: Box::new(object),
                class: id,
                field: index,
                ty,
            };
        }
        if self.classes[id.0 as usize]
            .methods
            .iter()
            .any(|method| method.name == expr.member.name)
        {
            self.error(
                expr.span,
                format!("method '{}' must be called", expr.member.name),
            );
            return self.dummy(None);
        }
        self.error(
            expr.member.span,
            format!("unknown field '{}'", expr.member.name),
        );
        self.dummy(None)
    }

    fn finish_class_set(
        &mut self,
        object: CheckedExpr,
        id: ClassId,
        member: &MemberExpr,
        value: &Expr,
        op: Option<BinaryOperator>,
    ) -> CheckedExpr {
        let Some(index) = self.classes[id.0 as usize]
            .fields
            .iter()
            .position(|field| field.name == member.member.name)
        else {
            if self.classes[id.0 as usize]
                .methods
                .iter()
                .any(|method| method.name == member.member.name)
            {
                self.error(
                    member.span,
                    format!("method '{}' must be called", member.member.name),
                );
            } else {
                self.error(
                    member.member.span,
                    format!("unknown field '{}'", member.member.name),
                );
            }
            let _ = self.check_expr(value, None, false);
            return self.dummy(None);
        };
        let public = self.classes[id.0 as usize].fields[index].public;
        let const_field = self.classes[id.0 as usize].fields[index].const_field;
        let ty = self.classes[id.0 as usize].fields[index].ty;
        let auto = self.classes[id.0 as usize].fields[index].auto;
        let name = self.classes[id.0 as usize].fields[index].name.clone();
        if !public && self.current_class != Some(id) {
            self.error(
                member.member.span,
                format!("field '{name}' is not accessible"),
            );
        }
        if const_field && !self.in_init {
            self.error(
                member.span,
                format!("cannot assign to constant field '{name}'"),
            );
        }
        if let Some(root) = self.frozen_path(&member.object) {
            self.error(member.span, format!("cannot assign to constant '{root}'"));
        }
        let current = CheckedExpr::ClassGet {
            object: Box::new(object.clone()),
            class: id,
            field: index,
            ty,
        };
        let value = self.store_value(op, current, value, ty, auto, value.span());
        let (file, line, column, end_column) = self.located(member.span);
        CheckedExpr::ClassSet {
            object: Box::new(object),
            class: id,
            field: index,
            value: Box::new(value),
            ty,
            file,
            line,
            column,
            end_column,
        }
    }

    fn finish_class_call(
        &mut self,
        object: CheckedExpr,
        id: ClassId,
        member: &MemberExpr,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let Some(index) = self.classes[id.0 as usize]
            .methods
            .iter()
            .position(|method| method.name == member.member.name)
        else {
            if let Some(field) = self.classes[id.0 as usize]
                .fields
                .iter()
                .find(|field| field.name == member.member.name)
            {
                let spelled = self.spell(field.ty);
                self.error(
                    member.span,
                    format!("cannot call a value of type '{spelled}'"),
                );
            } else {
                self.error(
                    member.member.span,
                    format!("unknown method '{}'", member.member.name),
                );
            }
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        };
        let public = self.classes[id.0 as usize].methods[index].public;
        let is_static = self.classes[id.0 as usize].methods[index].is_static;
        let mutating = self.classes[id.0 as usize].methods[index].mutating;
        let ret = self.classes[id.0 as usize].methods[index].ret;
        let params = self.classes[id.0 as usize].methods[index]
            .params
            .iter()
            .map(|param| (param.ty, param.auto))
            .collect::<Vec<_>>();
        let name = self.classes[id.0 as usize].methods[index].name.clone();
        if !public && self.current_class != Some(id) {
            self.error(
                member.member.span,
                format!("method '{name}' is not accessible"),
            );
        }
        if mutating && !is_static {
            if let Some(root) = self.frozen_path(&member.object) {
                self.error(
                    span,
                    format!("cannot call mutating method '{name}' on constant '{root}'"),
                );
            }
        }
        let args = self.check_arguments(arguments, &params, span);
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::ClassCall {
            class: id,
            method: index,
            object: Some(Box::new(object)),
            args,
            ty: ret,
            file,
            line,
            column,
            end_column,
        }
    }

    fn peek_type(&self, expr: &Expr) -> Option<CanonicalType> {
        match unwrap_parens(expr) {
            Expr::Identifier(ident) => {
                if let Some(ty) = self.narrowed_type(&ident.name) {
                    return Some(ty);
                }
                self.lookup(&ident.name)
                    .map(|binding| binding.ty)
                    .or_else(|| {
                        if ident.name == "this" {
                            self.current_class.map(CanonicalType::Class)
                        } else {
                            None
                        }
                    })
            }
            Expr::Call(call) => self.peek_call(call),
            Expr::Member(member) => {
                let object = self.peek_type(&member.object)?;
                let object = object.optional_inner().unwrap_or(object);
                if let CanonicalType::Class(id) = object {
                    let class = self.classes.get(id.0 as usize)?;
                    if let Some(field) = class
                        .fields
                        .iter()
                        .find(|field| field.name == member.member.name)
                    {
                        return Some(field.ty);
                    }
                    if let Some(method) = class
                        .methods
                        .iter()
                        .find(|method| method.name == member.member.name)
                    {
                        return Some(method.ret);
                    }
                    return None;
                }
                if let CanonicalType::Record(id) = object {
                    return self.field_index(id, &member.member.name).map(|(_, ty)| ty);
                }
                object.element()
            }
            Expr::Index(index) => self.peek_type(&index.object).and_then(|ty| ty.element()),
            Expr::New(new_expr) => self
                .class_names
                .get(&new_expr.name.name)
                .copied()
                .map(CanonicalType::Class),
            _ => None,
        }
    }

    fn peek_call(&self, call: &CallExpr) -> Option<CanonicalType> {
        match call.callee.as_ref() {
            Expr::Identifier(ident) => self
                .functions
                .get(&ident.name)
                .map(|signature| signature.ret),
            Expr::New(new_expr) => self
                .class_names
                .get(&new_expr.name.name)
                .copied()
                .map(CanonicalType::Class),
            Expr::Member(member) => self.peek_type(&Expr::Member(member.clone())),
            _ => None,
        }
    }

    fn frozen_path(&self, expr: &Expr) -> Option<String> {
        match unwrap_parens(expr) {
            Expr::Identifier(ident) => {
                if ident.name == "this" {
                    return None;
                }
                let binding = self.lookup(&ident.name)?;
                if !binding.mutable {
                    Some(ident.name.clone())
                } else {
                    None
                }
            }
            Expr::Member(member) => {
                if let Some(name) = self.frozen_path(&member.object) {
                    return Some(name);
                }
                if let Some(CanonicalType::Class(id)) = self.peek_type(&member.object) {
                    let const_field = self.classes.get(id.0 as usize).is_some_and(|class| {
                        class
                            .fields
                            .iter()
                            .any(|field| field.name == member.member.name && field.const_field)
                    });
                    if const_field {
                        return Some(member.member.name.clone());
                    }
                }
                None
            }
            Expr::Index(index) => self.frozen_path(&index.object),
            _ => None,
        }
    }

    fn null_literal(&mut self, span: Span, expected: Option<CanonicalType>) -> CheckedExpr {
        if let Some(target) = expected.filter(|ty| ty.is_optional()) {
            return CheckedExpr::Null { ty: target };
        }
        if let Some(target) = expected {
            self.error(
                span,
                format!(
                    "null requires a nullable type, found '{}'",
                    self.spell(target)
                ),
            );
            return self.dummy(Some(target));
        }
        self.error(span, "null requires a nullable type");
        self.dummy(None)
    }

    fn check_null_compare(&mut self, expr: &BinaryExpr) -> CheckedExpr {
        let left_null = expr_is_null(&expr.left);
        let right_null = expr_is_null(&expr.right);
        if left_null && right_null {
            self.error(expr.span, "null requires a nullable type");
            return CheckedExpr::Bool(false);
        }
        let (value_expr, null_expr, value_on_left) = if right_null {
            (&expr.left, &expr.right, true)
        } else {
            (&expr.right, &expr.left, false)
        };
        let value = self.check_expr(value_expr, None, false);
        if !value.ty().is_optional() {
            self.error(
                expr.span,
                format!("null cannot be compared with '{}'", self.spell(value.ty())),
            );
        }
        let null = self.check_expr(null_expr, Some(value.ty()), false);
        let (left, right) = if value_on_left {
            (value, null)
        } else {
            (null, value)
        };
        CheckedExpr::Binary {
            op: expr.operator,
            ty: CanonicalType::Bln,
            left: Box::new(left),
            right: Box::new(right),
            compare: Some(CompareMode::Null),
            span: expr.span,
        }
    }

    fn null_split(
        &self,
        expr: &Expr,
    ) -> (Vec<(String, CanonicalType)>, Vec<(String, CanonicalType)>) {
        let Expr::Binary(binary) = unwrap_parens(expr) else {
            return (Vec::new(), Vec::new());
        };
        let (ident, positive) = match (
            binary.operator,
            unwrap_parens(&binary.left),
            unwrap_parens(&binary.right),
        ) {
            (
                BinaryOperator::NotEqual,
                Expr::Identifier(ident),
                Expr::Literal(LiteralExpr::Null { .. }),
            )
            | (
                BinaryOperator::NotEqual,
                Expr::Literal(LiteralExpr::Null { .. }),
                Expr::Identifier(ident),
            ) => (ident, true),
            (
                BinaryOperator::Equal,
                Expr::Identifier(ident),
                Expr::Literal(LiteralExpr::Null { .. }),
            )
            | (
                BinaryOperator::Equal,
                Expr::Literal(LiteralExpr::Null { .. }),
                Expr::Identifier(ident),
            ) => (ident, false),
            _ => return (Vec::new(), Vec::new()),
        };
        let Some(inner) = self
            .lookup(&ident.name)
            .and_then(|binding| binding.ty.optional_inner())
        else {
            return (Vec::new(), Vec::new());
        };
        let pair = vec![(ident.name.clone(), inner)];
        if positive {
            (pair, Vec::new())
        } else {
            (Vec::new(), pair)
        }
    }

    fn push_narrow(&mut self, pairs: &[(String, CanonicalType)]) {
        self.narrowed.push(pairs.iter().cloned().collect());
    }

    fn pop_narrow(&mut self) {
        self.narrowed.pop();
    }

    fn narrowed_type(&self, name: &str) -> Option<CanonicalType> {
        for frame in self.narrowed.iter().rev() {
            if let Some(ty) = frame.get(name) {
                return Some(*ty);
            }
        }
        None
    }

    fn clear_narrow(&mut self, name: &str) {
        for frame in &mut self.narrowed {
            frame.remove(name);
        }
    }

    fn mark_ready(&mut self, name: &str) {
        if let Some(binding) = self.binding_mut(name) {
            binding.ready = true;
        }
        self.clear_narrow(name);
    }

    fn binding_mut(&mut self, name: &str) -> Option<&mut Binding> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                return Some(binding);
            }
        }
        None
    }

    fn ready_snapshot(&self) -> HashMap<String, bool> {
        let mut map = HashMap::new();
        for scope in &self.scopes {
            for (name, binding) in scope {
                map.insert(name.clone(), binding.ready);
            }
        }
        map
    }

    fn restore_ready(&mut self, map: &HashMap<String, bool>) {
        for (name, ready) in map {
            if let Some(binding) = self.binding_mut(name) {
                binding.ready = *ready;
            }
        }
    }

    fn merge_ready(
        &mut self,
        then_ready: &HashMap<String, bool>,
        else_ready: &HashMap<String, bool>,
    ) {
        for (name, then_flag) in then_ready {
            let else_flag = else_ready.get(name).copied().unwrap_or(false);
            if let Some(binding) = self.binding_mut(name) {
                binding.ready = *then_flag && else_flag;
            }
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
        self.error_help(span, message, None);
    }

    fn error_help(&mut self, span: Span, message: impl Into<String>, help: Option<String>) {
        let message = message.into();
        if self.diagnostics.last().is_some_and(|last| {
            last.span.start.offset == span.start.offset && last.message == message
        }) {
            return;
        }
        let code = crate::diagnostics::semantic_code(&message);
        debug_assert_ne!(code, "GRF-SEM-900", "unclassified diagnostic: {message}");
        self.diagnostics.push(SemanticDiagnostic {
            source_name: self.source_name.to_string(),
            span,
            message,
            code,
            help,
        });
    }
}

fn string_literal_converts(text: &str, target: CanonicalType) -> bool {
    let site = graphite_core::convert::Site {
        file: "",
        line: 0,
        column: 1,
        end_column: 1,
        excerpt: "",
    };
    match target {
        CanonicalType::Int16 => graphite_core::convert::str_to_signed(
            text,
            i16::MIN as i128,
            i16::MAX as i128,
            "int16",
            site,
        )
        .is_ok(),
        CanonicalType::Int32 => graphite_core::convert::str_to_signed(
            text,
            i32::MIN as i128,
            i32::MAX as i128,
            "int32",
            site,
        )
        .is_ok(),
        CanonicalType::Int64 => graphite_core::convert::str_to_signed(
            text,
            i64::MIN as i128,
            i64::MAX as i128,
            "int64",
            site,
        )
        .is_ok(),
        CanonicalType::UInt16 => {
            graphite_core::convert::str_to_unsigned(text, u16::MAX as u128, "uint16", site).is_ok()
        }
        CanonicalType::UInt32 => {
            graphite_core::convert::str_to_unsigned(text, u32::MAX as u128, "uint32", site).is_ok()
        }
        CanonicalType::UInt64 => {
            graphite_core::convert::str_to_unsigned(text, u64::MAX as u128, "uint64", site).is_ok()
        }
        CanonicalType::Flt32 => graphite_core::convert::str_to_f32(text, site).is_ok(),
        CanonicalType::Flt64 => graphite_core::convert::str_to_f64(text, site).is_ok(),
        CanonicalType::Bln => graphite_core::convert::str_to_bool(text, site).is_ok(),
        CanonicalType::Chr => graphite_core::convert::str_to_char(text, site).is_ok(),
        _ => false,
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

fn expr_is_null(expr: &Expr) -> bool {
    matches!(unwrap_parens(expr), Expr::Literal(LiteralExpr::Null { .. }))
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
        BinaryOperator::Power => "**",
        BinaryOperator::FloorDiv => "//",
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

fn catches_all(kind: &str) -> bool {
    kind == "RuntimeError" || kind == "Error"
}

fn catch_type(name: &str) -> Option<&'static str> {
    Some(match name {
        "RuntimeError" => "RuntimeError",
        "Error" => "Error",
        "ArrayIndexOutOfBoundsError" => "ArrayIndexOutOfBoundsError",
        "MissingMapKeyError" => "MissingMapKeyError",
        "InvalidSliceError" => "InvalidSliceError",
        "IntegerOverflowError" => "IntegerOverflowError",
        "DivisionByZeroError" => "DivisionByZeroError",
        "NumericConversionError" => "NumericConversionError",
        "IoError" => "IoError",
        "ExternalLibraryError" => "ExternalLibraryError",
        "RuntimeInternalError" => "RuntimeInternalError",
        "ConstViolationError" => "ConstViolationError",
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
        Stmt::Try(stmt) => {
            guaranteed_return(&stmt.body.statements)
                && stmt
                    .catches
                    .iter()
                    .all(|clause| guaranteed_return(&clause.block.statements))
        }
        _ => false,
    }
}
