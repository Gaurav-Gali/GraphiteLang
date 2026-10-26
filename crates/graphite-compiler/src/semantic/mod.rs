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
//! `&&`/`and` and `||`/`or` are short-circuit boolean operators. Bitwise
//! `&`, `|`, `^`, `~`, `<<`, and `>>` apply to integers. Integer overflow is
//! not wrapped: code generation uses checked
//! arithmetic and aborts if an operation overflows. That abort is a guard, not
//! the frozen overflow policy. Floating-point arithmetic uses Rust's IEEE-754
//! operators.
//!
//! `lengthOf`, `typeOf`, and `range` are reserved built-ins. `lengthOf` takes one `str`,
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
pub(crate) mod generics;
pub(crate) mod numeric;

use convert::{ConvOp, conversion};
use numeric::{CompareMode, NumericError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IterId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AsyncIterId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallableId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeParamId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericInstance {
    pub class: ClassId,
    pub args: Vec<CanonicalType>,
}

/// Parameter types and return type of a callable value.
///
/// `autos` records which parameters convert at the call. Compatibility
/// compares `params` and `ret` and ignores `autos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableSignature {
    pub params: Vec<CanonicalType>,
    pub autos: Vec<bool>,
    pub ret: CanonicalType,
}

#[derive(Debug, Clone, Copy, Eq)]
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
    /// A function value. The signature lives in the program's callable table.
    Callable(CallableId),
    /// A reference whose static type is an interface.
    Interface(InterfaceId),
    /// A type parameter declared on a function, method, or class.
    Param(TypeParamId),
    /// A constructed generic class. The arguments live in the program table.
    Generic(GenericId),
    /// Built-in `iter<T>`. The element type lives in the program table.
    Iter(IterId),
    /// Built-in `asyncIter<T>` — async pull stream of `T`.
    AsyncIter(AsyncIterId),
    /// Built-in `async<T>`. The eventual result type lives in the program table.
    Async(AsyncId),
}

/// The type inside `T?`. One level of nullability.
#[derive(Debug, Clone, Copy, Eq)]
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
    Interface(InterfaceId),
    Map(MapId),
    Array {
        leaf: Leaf,
        depth: u8,
        limits: [Option<u64>; MAX_ARRAY_RANK],
    },
    Param(TypeParamId),
    Generic(GenericId),
    Iter(IterId),
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
            OptionalKind::Interface(id) => CanonicalType::Interface(id),
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
            OptionalKind::Param(id) => CanonicalType::Param(id),
            OptionalKind::Generic(id) => CanonicalType::Generic(id),
            OptionalKind::Iter(id) => CanonicalType::Iter(id),
        }
    }
}

impl PartialEq for OptionalKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                OptionalKind::Array { leaf, depth, .. },
                OptionalKind::Array {
                    leaf: other_leaf,
                    depth: other_depth,
                    ..
                },
            ) => leaf == other_leaf && depth == other_depth,
            _ => {
                std::mem::discriminant(self) == std::mem::discriminant(other)
                    && match (self, other) {
                        (OptionalKind::Record(left), OptionalKind::Record(right)) => left == right,
                        (OptionalKind::Class(left), OptionalKind::Class(right)) => left == right,
                        (OptionalKind::Interface(left), OptionalKind::Interface(right)) => {
                            left == right
                        }
                        (OptionalKind::Map(left), OptionalKind::Map(right)) => left == right,
                        (OptionalKind::Param(left), OptionalKind::Param(right)) => left == right,
                        (OptionalKind::Generic(left), OptionalKind::Generic(right)) => {
                            left == right
                        }
                        (OptionalKind::Iter(left), OptionalKind::Iter(right)) => left == right,
                        (OptionalKind::Array { .. }, _) | (_, OptionalKind::Array { .. }) => false,
                        _ => true,
                    }
            }
        }
    }
}

fn limits_need_check(source: CanonicalType, target: CanonicalType) -> bool {
    match (source, target) {
        (CanonicalType::Optional(source), CanonicalType::Optional(target)) => {
            limits_need_check(source.canonical(), target.canonical())
        }
        (
            CanonicalType::Array {
                leaf,
                depth,
                limits: source_limits,
            },
            CanonicalType::Array {
                leaf: target_leaf,
                depth: target_depth,
                limits: target_limits,
            },
        ) if leaf == target_leaf && depth == target_depth => {
            (0..depth as usize).any(|index| match (source_limits[index], target_limits[index]) {
                (_, None) => false,
                (Some(source_limit), Some(target_limit)) => source_limit > target_limit,
                (None, Some(_)) => true,
            })
        }
        _ => false,
    }
}

impl PartialEq for CanonicalType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                CanonicalType::Array { leaf, depth, .. },
                CanonicalType::Array {
                    leaf: other_leaf,
                    depth: other_depth,
                    ..
                },
            ) => leaf == other_leaf && depth == other_depth,
            (CanonicalType::Optional(left), CanonicalType::Optional(right)) => left == right,
            (CanonicalType::Record(left), CanonicalType::Record(right)) => left == right,
            (CanonicalType::Class(left), CanonicalType::Class(right)) => left == right,
            (CanonicalType::Interface(left), CanonicalType::Interface(right)) => left == right,
            (CanonicalType::Map(left), CanonicalType::Map(right)) => left == right,
            (CanonicalType::Callable(left), CanonicalType::Callable(right)) => left == right,
            (CanonicalType::Param(left), CanonicalType::Param(right)) => left == right,
            (CanonicalType::Generic(left), CanonicalType::Generic(right)) => left == right,
            (CanonicalType::Iter(left), CanonicalType::Iter(right)) => left == right,
            (CanonicalType::AsyncIter(left), CanonicalType::AsyncIter(right)) => left == right,
            (CanonicalType::Async(left), CanonicalType::Async(right)) => left == right,
            _ => {
                std::mem::discriminant(self) == std::mem::discriminant(other)
                    && !matches!(
                        self,
                        CanonicalType::Array { .. }
                            | CanonicalType::Optional(_)
                            | CanonicalType::Record(_)
                            | CanonicalType::Class(_)
                            | CanonicalType::Interface(_)
                            | CanonicalType::Map(_)
                            | CanonicalType::Callable(_)
                            | CanonicalType::Param(_)
                            | CanonicalType::Generic(_)
                            | CanonicalType::Iter(_)
                            | CanonicalType::AsyncIter(_)
                            | CanonicalType::Async(_)
                    )
            }
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
    Interface(InterfaceId),
    Map(MapId),
    Param(TypeParamId),
    Generic(GenericId),
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
            CanonicalType::Class(_) | CanonicalType::Interface(_) | CanonicalType::Generic(_) => {
                "graphite_core::heap::Rooted".to_string()
            }
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
            CanonicalType::Callable(_) => "fn()".to_string(),
            CanonicalType::Param(_) => "T".to_string(),
            CanonicalType::Iter(_) => "graphite_core::iter::Iter<()>".to_string(),
            CanonicalType::AsyncIter(_) => {
                "graphite_core::async_iter::AsyncIter<()>".to_string()
            }
            CanonicalType::Async(_) => "graphite_core::async_rt::Async<()>".to_string(),
        }
    }

    pub fn optional(self) -> Option<CanonicalType> {
        let kind = match self {
            CanonicalType::Optional(_)
            | CanonicalType::Void
            | CanonicalType::ErrorHandle
            | CanonicalType::Callable(_)
            | CanonicalType::AsyncIter(_)
            | CanonicalType::Async(_) => {
                // V1: async / asyncIter values are not nullable.
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
            CanonicalType::Interface(id) => OptionalKind::Interface(id),
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
            CanonicalType::Param(id) => OptionalKind::Param(id),
            CanonicalType::Generic(id) => OptionalKind::Generic(id),
            CanonicalType::Iter(id) => OptionalKind::Iter(id),
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
            CanonicalType::Interface(_) => "interface".to_string(),
            CanonicalType::Record(_) => "record".to_string(),
            CanonicalType::Map(_) => "map".to_string(),
            CanonicalType::Array { leaf, depth, .. } => {
                let mut name = leaf.spelling().to_string();
                name.push_str(&"[]".repeat(depth as usize));
                name
            }
            CanonicalType::Optional(kind) => format!("{}?", kind.canonical().spelling()),
            CanonicalType::Callable(_) => "callable".to_string(),
            CanonicalType::Param(_) => "param".to_string(),
            CanonicalType::Generic(_) => "generic".to_string(),
            CanonicalType::Iter(_) => "iter".to_string(),
            CanonicalType::AsyncIter(_) => "asyncIter".to_string(),
            CanonicalType::Async(_) => "async".to_string(),
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
                | CanonicalType::Interface(_)
                | CanonicalType::Generic(_)
                | CanonicalType::Map(_)
                | CanonicalType::Array { .. }
                | CanonicalType::Optional(_)
                | CanonicalType::Iter(_)
                | CanonicalType::AsyncIter(_)
        )
    }

    pub fn is_iter(self) -> bool {
        matches!(self, CanonicalType::Iter(_))
    }

    pub fn is_async_iter(self) -> bool {
        matches!(self, CanonicalType::AsyncIter(_))
    }

    pub fn is_async(self) -> bool {
        matches!(self, CanonicalType::Async(_))
    }

    pub fn is_record(self) -> bool {
        matches!(self, CanonicalType::Record(_))
    }

    pub fn is_class(self) -> bool {
        matches!(self, CanonicalType::Class(_))
    }

    pub fn is_interface(self) -> bool {
        matches!(self, CanonicalType::Interface(_))
    }

    pub fn is_generic(self) -> bool {
        matches!(self, CanonicalType::Generic(_))
    }

    pub fn is_param(self) -> bool {
        matches!(self, CanonicalType::Param(_))
    }

    pub fn is_object(self) -> bool {
        self.is_class() || self.is_interface() || self.is_generic()
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

    pub(crate) fn leaf(self) -> Leaf {
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
            CanonicalType::Interface(id) => Leaf::Interface(id),
            CanonicalType::Map(id) => Leaf::Map(id),
            CanonicalType::Array { leaf, .. } => leaf,
            CanonicalType::Param(id) => Leaf::Param(id),
            CanonicalType::Generic(id) => Leaf::Generic(id),
            CanonicalType::Optional(_)
            | CanonicalType::Callable(_)
            | CanonicalType::Iter(_)
            | CanonicalType::AsyncIter(_)
            | CanonicalType::Async(_) => Leaf::Void,
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
            Leaf::Interface(id) => CanonicalType::Interface(id),
            Leaf::Map(id) => CanonicalType::Map(id),
            Leaf::Param(id) => CanonicalType::Param(id),
            Leaf::Generic(id) => CanonicalType::Generic(id),
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
            Leaf::Interface(_) => "interface",
            Leaf::Map(_) => "map",
            Leaf::Param(_) => "param",
            Leaf::Generic(_) => "generic",
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
    pub warnings: Vec<SemanticDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedProgram {
    pub records: Vec<CheckedRecord>,
    pub classes: Vec<CheckedClass>,
    pub interfaces: Vec<CheckedInterface>,
    pub maps: Vec<MapShape>,
    pub iters: Vec<IterShape>,
    pub async_iters: Vec<IterShape>,
    pub asyncs: Vec<AsyncShape>,
    pub callables: Vec<CallableSignature>,
    pub functions: Vec<CheckedFn>,
    /// Concrete copies of generic functions, keyed by call-site type arguments.
    pub specializations: Vec<CheckedFn>,
    pub generics: Vec<GenericInstance>,
    pub type_param_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedInterface {
    pub name: String,
    pub methods: Vec<CheckedIfaceMethod>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedIfaceMethod {
    pub name: String,
    pub params: Vec<CanonicalType>,
    pub ret: CanonicalType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedClass {
    pub name: String,
    pub parent: Option<ClassId>,
    pub implements: Vec<InterfaceId>,
    pub abstract_class: bool,
    pub fields: Vec<CheckedClassField>,
    pub init: Option<CheckedInit>,
    pub methods: Vec<CheckedMethod>,
    pub type_params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedClassField {
    pub name: String,
    pub ty: CanonicalType,
    pub const_field: bool,
    pub is_static: bool,
    pub initializer: Option<CheckedExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedInit {
    pub params: Vec<(String, CanonicalType)>,
    pub body: Vec<CheckedStmt>,
    pub implicit_super: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedMethod {
    pub name: String,
    pub params: Vec<(String, CanonicalType)>,
    pub ret: CanonicalType,
    pub body: Vec<CheckedStmt>,
    pub mutating: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_virtual: bool,
    pub type_params: Vec<String>,
    pub is_generator: bool,
    pub is_async_iter: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapShape {
    pub key: CanonicalType,
    pub value: CanonicalType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IterShape {
    pub element: CanonicalType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncShape {
    pub result: CanonicalType,
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
    /// Eventual return type. For async functions this is `T` from `async<T>`,
    /// not the `async<T>` wrapper.
    pub ret: CanonicalType,
    pub body: Vec<CheckedStmt>,
    pub type_params: Vec<String>,
    /// True when the function returns `iter<T>` and uses `emit`.
    pub is_generator: bool,
    /// True when the declaration used `async<T>` as its result type.
    pub is_async: bool,
    /// True when the declaration used `asyncIter<T>` (emit + await allowed).
    pub is_async_iter: bool,
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
    /// `await<T> name = value` or bare `await value` (void).
    Await {
        name: Option<String>,
        value: CheckedExpr,
        result_ty: CanonicalType,
    },
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
    /// `loop (await<T> name : asyncIter)` — awaits each item.
    AsyncIterate {
        name: String,
        ty: CanonicalType,
        source: CheckedExpr,
        body: Vec<CheckedStmt>,
    },
    Try {
        body: Vec<CheckedStmt>,
        catches: Vec<CheckedCatch>,
        finally_body: Option<Vec<CheckedStmt>>,
    },
    Super {
        class: ClassId,
        args: Vec<CheckedExpr>,
    },
    Emit {
        value: CheckedExpr,
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
    Append,
    Extend,
    Insert,
    Remove,
    Pop,
    Clear,
    IndexOf,
    Count,
    Reverse,
    Sort,
    Copy,
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMethod {
    Length,
    Get,
    Set,
    Push,
    Append,
    Extend,
    Insert,
    Remove,
    Pop,
    Clear,
    Reverse,
    Copy,
    Split,
    RSplit,
    Strip,
    LStrip,
    RStrip,
    Lower,
    Upper,
    Replace,
    Find,
    Index,
    Count,
    StartsWith,
    EndsWith,
    Join,
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
    Ternary {
        condition: Box<CheckedExpr>,
        then_branch: Box<CheckedExpr>,
        else_branch: Box<CheckedExpr>,
        ty: CanonicalType,
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
        type_args: Vec<CanonicalType>,
        ty: CanonicalType,
        callee: String,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    /// A non-capturing lambda. Its type is `Callable`.
    Lambda {
        params: Vec<(String, CanonicalType)>,
        body: Box<Vec<CheckedStmt>>,
        ret: CanonicalType,
        ty: CanonicalType,
    },
    /// A named function used as a value.
    FunctionRef {
        name: String,
        ty: CanonicalType,
    },
    /// A call through a callable value.
    Invoke {
        callee: Box<CheckedExpr>,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        callee_name: String,
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
        text: bool,
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
        text: bool,
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
        text: bool,
        limit: Option<u64>,
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
        text: bool,
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
        element: CanonicalType,
        limit: Option<u64>,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    TextCall {
        place: Option<ArrayPlace>,
        object: Option<Box<CheckedExpr>>,
        method: TextMethod,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
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
        type_args: Vec<CanonicalType>,
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
        type_args: Vec<CanonicalType>,
        ty: CanonicalType,
        direct: bool,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    IfaceCall {
        interface: InterfaceId,
        method: usize,
        object: Box<CheckedExpr>,
        args: Vec<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    Upcast {
        expr: Box<CheckedExpr>,
        ty: CanonicalType,
    },
    /// Same array type, with the destination's maximum lengths checked.
    EnsureLimit {
        expr: Box<CheckedExpr>,
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
    /// `array.iter()` / `iter.iter()` producing or aliasing an `iter<T>`.
    MakeIter {
        object: Box<CheckedExpr>,
        ty: CanonicalType,
    },
    IterCall {
        object: Box<CheckedExpr>,
        method: IterMethod,
        /// True for `asyncIter` protocol calls that suspend (`hasNext`/`next`/`value`).
        suspend: bool,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
    /// Built-in `range(end)`, `range(start, end)`, or `range(start, end, step)`.
    Range {
        start: Box<CheckedExpr>,
        end: Box<CheckedExpr>,
        step: Box<CheckedExpr>,
        ty: CanonicalType,
        file: String,
        line: u32,
        column: u32,
        end_column: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterMethod {
    HasNext,
    Next,
    /// Current element without consuming (prefetches like `hasNext`).
    Value,
    Iter,
}

impl CheckedExpr {
    pub fn ty(&self) -> CanonicalType {
        match self {
            CheckedExpr::Int { ty, .. }
            | CheckedExpr::Float { ty, .. }
            | CheckedExpr::Load { ty, .. }
            | CheckedExpr::Unary { ty, .. }
            | CheckedExpr::Binary { ty, .. }
            | CheckedExpr::Ternary { ty, .. }
            | CheckedExpr::Assign { ty, .. }
            | CheckedExpr::Update { ty, .. }
            | CheckedExpr::Call { ty, .. }
            | CheckedExpr::Lambda { ty, .. }
            | CheckedExpr::FunctionRef { ty, .. }
            | CheckedExpr::Invoke { ty, .. }
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
            | CheckedExpr::TextCall { ty, .. }
            | CheckedExpr::MapLiteral { ty, .. }
            | CheckedExpr::MapGet { ty, .. }
            | CheckedExpr::MapInsert { ty, .. }
            | CheckedExpr::MapCall { ty, .. }
            | CheckedExpr::ClassNew { ty, .. }
            | CheckedExpr::ClassGet { ty, .. }
            | CheckedExpr::ClassSet { ty, .. }
            | CheckedExpr::ClassCall { ty, .. }
            | CheckedExpr::IfaceCall { ty, .. }
            | CheckedExpr::Upcast { ty, .. }
            | CheckedExpr::EnsureLimit { ty, .. }
            | CheckedExpr::MakeIter { ty, .. }
            | CheckedExpr::IterCall { ty, .. }
            | CheckedExpr::Range { ty, .. }
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

#[derive(Clone)]
struct Signature {
    params: Vec<ParamInfo>,
    ret: CanonicalType,
    span: Span,
    type_params: Vec<TypeParamId>,
}

#[derive(Clone)]
struct ParamInfo {
    name: String,
    ty: CanonicalType,
    auto: bool,
    default: Option<Expr>,
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
    /// Async operations are move-only; true after move or await.
    moved: bool,
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
        interfaces: Vec::new(),
        interface_names: HashMap::new(),
        warnings: Vec::new(),
        at_super_slot: false,
        maps: Vec::new(),
        iters: Vec::new(),
        async_iters: Vec::new(),
        asyncs: Vec::new(),
        generator_element: None,
        in_async: false,
        in_async_iter: false,
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
        allow_convert: false,
        callables: Vec::new(),
        lambda_floor: None,
        lambda_var: None,
        type_params: Vec::new(),
        type_param_scopes: Vec::new(),
        generics: Vec::new(),
        function_asts: HashMap::new(),
        specializations: Vec::new(),
        specialized_keys: HashMap::new(),
        instantiation: None,
        param_substitution: None,
    };
    checker.collect_imports(program);
    checker.collect_interfaces(program);
    checker.collect_classes(program);
    checker.link_classes();
    checker.collect_records(program);
    checker.finish_class_types();
    checker.check_inheritance();
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
                interfaces: checker
                    .interfaces
                    .iter()
                    .map(|interface| CheckedInterface {
                        name: interface.name.clone(),
                        methods: interface
                            .methods
                            .iter()
                            .map(|method| CheckedIfaceMethod {
                                name: method.name.clone(),
                                params: method.params.iter().map(|param| param.ty).collect(),
                                ret: method.ret,
                            })
                            .collect(),
                    })
                    .collect(),
                maps: checker.maps,
                iters: checker.iters,
                async_iters: checker.async_iters,
                asyncs: checker.asyncs,
                callables: checker.callables,
                functions,
                specializations: checker.specializations,
                generics: checker.generics,
                type_param_names: checker
                    .type_params
                    .iter()
                    .map(|param| param.name.clone())
                    .collect(),
            })
        } else {
            None
        },
        diagnostics: checker.diagnostics,
        warnings: checker.warnings,
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
    protected: bool,
    is_static: bool,
    deprecated: bool,
    span: Span,
}

struct ClassMethodInfo {
    name: String,
    params: Vec<ParamInfo>,
    param_syntax: Vec<RawParam>,
    ret: CanonicalType,
    ret_syntax: TypeSyntax,
    public: bool,
    protected: bool,
    is_static: bool,
    is_override: bool,
    is_abstract: bool,
    is_final: bool,
    is_virtual: bool,
    deprecated: bool,
    mutating: bool,
    leaks: bool,
    span: Span,
    type_params: Vec<TypeParamId>,
}

struct ClassInitInfo {
    params: Vec<ParamInfo>,
    param_syntax: Vec<RawParam>,
    public: bool,
    protected: bool,
    deprecated: bool,
}

struct InterfaceInfo {
    name: String,
    methods: Vec<IfaceMethodInfo>,
    #[allow(dead_code)]
    span: Span,
}

struct IfaceMethodInfo {
    name: String,
    params: Vec<ParamInfo>,
    param_syntax: Vec<RawParam>,
    ret: CanonicalType,
    ret_syntax: TypeSyntax,
    deprecated: bool,
    span: Span,
}

#[derive(Clone)]
struct RawParam {
    name: String,
    syntax: TypeSyntax,
    auto: bool,
    span: Span,
    name_span: Span,
    callable: Option<Vec<Parameter>>,
    default: Option<Expr>,
}

struct ClassInfo {
    name: String,
    span: Span,
    parent: Option<ClassId>,
    parent_name: Option<(String, Span)>,
    interface_names: Vec<(String, Span)>,
    implements: Vec<InterfaceId>,
    abstract_class: bool,
    final_class: bool,
    fields: Vec<ClassFieldInfo>,
    methods: Vec<ClassMethodInfo>,
    init: Option<ClassInitInfo>,
    type_params: Vec<TypeParamId>,
}

struct TypeParamInfo {
    name: String,
    #[allow(dead_code)]
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
    classes: Vec<ClassInfo>,
    class_names: HashMap<String, ClassId>,
    interfaces: Vec<InterfaceInfo>,
    interface_names: HashMap<String, InterfaceId>,
    warnings: Vec<SemanticDiagnostic>,
    at_super_slot: bool,
    maps: Vec<MapShape>,
    iters: Vec<IterShape>,
    async_iters: Vec<IterShape>,
    asyncs: Vec<AsyncShape>,
    /// When checking a generator / asyncIter, the element type for `emit`.
    generator_element: Option<CanonicalType>,
    /// True while checking an `async<T>` function body.
    in_async: bool,
    /// True while checking an `asyncIter<T>` function body.
    in_async_iter: bool,
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
    allow_convert: bool,
    callables: Vec<CallableSignature>,
    /// Scope index of the innermost lambda. Bindings below it are captures.
    lambda_floor: Option<usize>,
    /// Name of the callable variable currently being initialized.
    lambda_var: Option<String>,
    type_params: Vec<TypeParamInfo>,
    /// Active type-parameter scopes, innermost last.
    type_param_scopes: Vec<Vec<TypeParamId>>,
    generics: Vec<GenericInstance>,
    /// Original generic function declarations for instantiation.
    function_asts: HashMap<String, FunctionDecl>,
    /// Monomorphized generic functions produced at call sites.
    specializations: Vec<CheckedFn>,
    /// Keys already specialized: `"name<arg, arg>"`.
    specialized_keys: HashMap<String, String>,
    /// When set, diagnostics are attributed to a generic instantiation.
    instantiation: Option<String>,
    /// Active type-parameter → concrete type map while monomorphizing a
    /// generic function. Named type arguments like `one<T>` resolve through
    /// this map so nested generic calls see concrete types.
    param_substitution: Option<Vec<(TypeParamId, CanonicalType)>>,
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
        if name == "lengthOf" || name == "typeOf" || name == "range" {
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
        if syntax.kind == TypeKind::Named && syntax.name == "iter" {
            if syntax.arguments.len() != 1 {
                self.error(syntax.span, "iter type requires exactly one type argument");
            }
            let element = syntax
                .arguments
                .first()
                .map(|argument| self.resolve_type(argument))
                .unwrap_or(CanonicalType::Int64);
            if !syntax.bounds.is_empty() {
                self.error(syntax.span, "iter types cannot be arrays");
            }
            let id = self.intern_iter(element);
            return self.finish_optional(CanonicalType::Iter(id), syntax);
        }
        if syntax.kind == TypeKind::Named && syntax.name == "async" {
            if syntax.arguments.len() != 1 {
                self.error(syntax.span, "async type requires exactly one type argument");
            }
            let result = syntax
                .arguments
                .first()
                .map(|argument| self.resolve_type(argument))
                .unwrap_or(CanonicalType::Void);
            if !syntax.bounds.is_empty() {
                self.error(syntax.span, "async types cannot be arrays");
            }
            if syntax.optional {
                self.error(
                    syntax.span,
                    "async operations cannot be nullable in this version",
                );
            }
            let id = self.intern_async(result);
            return CanonicalType::Async(id);
        }
        if syntax.kind == TypeKind::Named && syntax.name == "asyncIter" {
            if syntax.arguments.len() != 1 {
                self.error(
                    syntax.span,
                    "asyncIter type requires exactly one type argument",
                );
            }
            let element = syntax
                .arguments
                .first()
                .map(|argument| self.resolve_type(argument))
                .unwrap_or(CanonicalType::Int64);
            if !syntax.bounds.is_empty() {
                self.error(syntax.span, "asyncIter types cannot be arrays");
            }
            if syntax.optional {
                self.error(
                    syntax.span,
                    "asyncIter values cannot be nullable in this version",
                );
            }
            let id = self.intern_async_iter(element);
            return CanonicalType::AsyncIter(id);
        }
        if syntax.kind == TypeKind::Named {
            if let Some(param) = self.lookup_type_param(&syntax.name) {
                if !syntax.arguments.is_empty() {
                    self.error(
                        syntax.span,
                        format!(
                            "type parameter '{}' cannot take type arguments",
                            syntax.name
                        ),
                    );
                }
                let resolved = self
                    .param_substitution
                    .as_ref()
                    .and_then(|map| {
                        map.iter()
                            .find(|(id, _)| *id == param)
                            .map(|(_, concrete)| *concrete)
                    })
                    .unwrap_or(CanonicalType::Param(param));
                return self.finish_optional(
                    self.with_bounds(resolved, &syntax.bounds),
                    syntax,
                );
            }
            if let Some(id) = self.class_names.get(&syntax.name).copied() {
                let ty = if !syntax.arguments.is_empty()
                    || !self.classes[id.0 as usize].type_params.is_empty()
                {
                    self.resolve_constructed_class(id, &syntax.arguments, syntax.span)
                } else {
                    CanonicalType::Class(id)
                };
                return self.finish_optional(self.with_bounds(ty, &syntax.bounds), syntax);
            }
            if let Some(id) = self.interface_names.get(&syntax.name).copied() {
                if !syntax.arguments.is_empty() {
                    self.error(syntax.span, "generic interfaces are not supported");
                }
                return self.finish_optional(
                    self.with_bounds(CanonicalType::Interface(id), &syntax.bounds),
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
            CanonicalType::Interface(id) => self.interface_name(id),
            CanonicalType::Param(id) => self
                .type_params
                .get(id.0 as usize)
                .map(|param| param.name.clone())
                .unwrap_or_else(|| "T".to_string()),
            CanonicalType::Generic(id) => {
                let Some(instance) = self.generics.get(id.0 as usize) else {
                    return "generic".to_string();
                };
                let args = instance
                    .args
                    .iter()
                    .map(|arg| self.spell(*arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}<{args}>", self.class_name(instance.class))
            }
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
            CanonicalType::Iter(id) => {
                let element = self
                    .iters
                    .get(id.0 as usize)
                    .map(|shape| shape.element)
                    .unwrap_or(CanonicalType::Int64);
                format!("iter<{}>", self.spell(element))
            }
            CanonicalType::AsyncIter(id) => {
                let element = self
                    .async_iters
                    .get(id.0 as usize)
                    .map(|shape| shape.element)
                    .unwrap_or(CanonicalType::Int64);
                format!("asyncIter<{}>", self.spell(element))
            }
            CanonicalType::Async(id) => {
                let result = self
                    .asyncs
                    .get(id.0 as usize)
                    .map(|shape| shape.result)
                    .unwrap_or(CanonicalType::Void);
                format!("async<{}>", self.spell(result))
            }
            CanonicalType::Array {
                leaf: Leaf::Class(id),
                depth,
                ..
            } => format!("{}{}", self.class_name(id), "[]".repeat(depth as usize)),
            CanonicalType::Array {
                leaf: Leaf::Generic(id),
                depth,
                ..
            } => format!(
                "{}{}",
                self.spell(CanonicalType::Generic(id)),
                "[]".repeat(depth as usize)
            ),
            CanonicalType::Array {
                leaf: Leaf::Param(id),
                depth,
                ..
            } => format!(
                "{}{}",
                self.spell(CanonicalType::Param(id)),
                "[]".repeat(depth as usize)
            ),
            CanonicalType::Array {
                leaf: Leaf::Record(id),
                depth,
                ..
            } => format!("{}{}", self.record_name(id), "[]".repeat(depth as usize)),
            CanonicalType::Array {
                leaf: Leaf::Map(id),
                depth,
                ..
            } => format!(
                "{}{}",
                self.spell(CanonicalType::Map(id)),
                "[]".repeat(depth as usize)
            ),
            CanonicalType::Optional(kind) => format!("{}?", self.spell(kind.canonical())),
            CanonicalType::Callable(id) => self.spell_callable(id),
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

    fn intern_iter(&mut self, element: CanonicalType) -> IterId {
        if let Some(index) = self.iters.iter().position(|shape| shape.element == element) {
            return IterId(index as u32);
        }
        let id = IterId(self.iters.len() as u32);
        self.iters.push(IterShape { element });
        id
    }

    fn iter_element(&self, ty: CanonicalType) -> Option<CanonicalType> {
        match ty {
            CanonicalType::Iter(id) => self.iters.get(id.0 as usize).map(|shape| shape.element),
            _ => None,
        }
    }

    fn intern_async(&mut self, result: CanonicalType) -> AsyncId {
        if let Some(index) = self.asyncs.iter().position(|shape| shape.result == result) {
            return AsyncId(index as u32);
        }
        let id = AsyncId(self.asyncs.len() as u32);
        self.asyncs.push(AsyncShape { result });
        id
    }

    fn async_result(&self, ty: CanonicalType) -> Option<CanonicalType> {
        match ty {
            CanonicalType::Async(id) => self.asyncs.get(id.0 as usize).map(|shape| shape.result),
            _ => None,
        }
    }

    fn intern_async_iter(&mut self, element: CanonicalType) -> AsyncIterId {
        if let Some(index) = self
            .async_iters
            .iter()
            .position(|shape| shape.element == element)
        {
            return AsyncIterId(index as u32);
        }
        let id = AsyncIterId(self.async_iters.len() as u32);
        self.async_iters.push(IterShape { element });
        id
    }

    fn async_iter_element(&self, ty: CanonicalType) -> Option<CanonicalType> {
        match ty {
            CanonicalType::AsyncIter(id) => self
                .async_iters
                .get(id.0 as usize)
                .map(|shape| shape.element),
            _ => None,
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
            let type_params = self.register_type_params(&function.type_params);
            self.push_type_params(&type_params);
            let ret = self.resolve_type(&function.return_type);
            let params = self.resolve_parameters(&function.parameters);
            self.pop_type_params();
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
            if function.name.name == "main" && !type_params.is_empty() {
                self.error(function.name.span, "'main' cannot be generic");
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
                    type_params,
                },
            );
            if !function.type_params.is_empty() {
                self.function_asts
                    .insert(function.name.name.clone(), function.clone());
            }
        }
    }

    fn register_type_params(&mut self, names: &[Identifier]) -> Vec<TypeParamId> {
        let mut ids = Vec::new();
        let mut seen = HashMap::new();
        for name in names {
            if !generics::is_upper_camel(&name.name) {
                self.error(
                    name.span,
                    format!("type parameter '{}' must be UpperCamelCase", name.name),
                );
            }
            if seen.insert(name.name.clone(), name.span).is_some() {
                self.error(
                    name.span,
                    format!("duplicate type parameter '{}'", name.name),
                );
                continue;
            }
            let id = TypeParamId(self.type_params.len() as u32);
            self.type_params.push(TypeParamInfo {
                name: name.name.clone(),
                span: name.span,
            });
            ids.push(id);
        }
        ids
    }

    fn push_type_params(&mut self, params: &[TypeParamId]) {
        self.type_param_scopes.push(params.to_vec());
    }

    fn pop_type_params(&mut self) {
        self.type_param_scopes.pop();
    }

    fn lookup_type_param(&self, name: &str) -> Option<TypeParamId> {
        for scope in self.type_param_scopes.iter().rev() {
            for id in scope {
                if self.type_params[id.0 as usize].name == name {
                    return Some(*id);
                }
            }
        }
        None
    }

    fn type_param_names(&self, ids: &[TypeParamId]) -> Vec<String> {
        ids.iter()
            .map(|id| self.type_params[id.0 as usize].name.clone())
            .collect()
    }

    fn type_mentions_param(&self, ty: CanonicalType) -> bool {
        match ty {
            CanonicalType::Param(_) => true,
            CanonicalType::Optional(kind) => self.type_mentions_param(kind.canonical()),
            CanonicalType::Array { leaf, .. } => match leaf {
                Leaf::Param(_) => true,
                Leaf::Generic(id) => self.generics.get(id.0 as usize).is_some_and(|instance| {
                    instance
                        .args
                        .iter()
                        .any(|arg| self.type_mentions_param(*arg))
                }),
                Leaf::Map(id) => self.maps.get(id.0 as usize).is_some_and(|shape| {
                    self.type_mentions_param(shape.key) || self.type_mentions_param(shape.value)
                }),
                _ => false,
            },
            CanonicalType::Generic(id) => {
                self.generics.get(id.0 as usize).is_some_and(|instance| {
                    instance
                        .args
                        .iter()
                        .any(|arg| self.type_mentions_param(*arg))
                })
            }
            CanonicalType::Map(id) => self.maps.get(id.0 as usize).is_some_and(|shape| {
                self.type_mentions_param(shape.key) || self.type_mentions_param(shape.value)
            }),
            CanonicalType::Iter(id) => self
                .iters
                .get(id.0 as usize)
                .is_some_and(|shape| self.type_mentions_param(shape.element)),
            CanonicalType::Async(id) => self
                .asyncs
                .get(id.0 as usize)
                .is_some_and(|shape| self.type_mentions_param(shape.result)),
            CanonicalType::Callable(id) => {
                self.callables.get(id.0 as usize).is_some_and(|signature| {
                    signature
                        .params
                        .iter()
                        .any(|param| self.type_mentions_param(*param))
                        || self.type_mentions_param(signature.ret)
                })
            }
            _ => false,
        }
    }

    fn uses_class_params(&self, ty: CanonicalType, class_params: &[TypeParamId]) -> bool {
        match ty {
            CanonicalType::Param(id) => class_params.contains(&id),
            CanonicalType::Optional(kind) => self.uses_class_params(kind.canonical(), class_params),
            CanonicalType::Array { leaf, .. } => match leaf {
                Leaf::Param(id) => class_params.contains(&id),
                Leaf::Generic(id) => self.generics.get(id.0 as usize).is_some_and(|instance| {
                    instance
                        .args
                        .iter()
                        .any(|arg| self.uses_class_params(*arg, class_params))
                }),
                _ => false,
            },
            CanonicalType::Generic(id) => {
                self.generics.get(id.0 as usize).is_some_and(|instance| {
                    instance
                        .args
                        .iter()
                        .any(|arg| self.uses_class_params(*arg, class_params))
                })
            }
            CanonicalType::Map(id) => self.maps.get(id.0 as usize).is_some_and(|shape| {
                self.uses_class_params(shape.key, class_params)
                    || self.uses_class_params(shape.value, class_params)
            }),
            CanonicalType::Iter(id) => self.iters.get(id.0 as usize).is_some_and(|shape| {
                self.uses_class_params(shape.element, class_params)
            }),
            CanonicalType::Async(id) => self.asyncs.get(id.0 as usize).is_some_and(|shape| {
                self.uses_class_params(shape.result, class_params)
            }),
            CanonicalType::Callable(id) => {
                self.callables.get(id.0 as usize).is_some_and(|signature| {
                    signature
                        .params
                        .iter()
                        .any(|param| self.uses_class_params(*param, class_params))
                        || self.uses_class_params(signature.ret, class_params)
                })
            }
            _ => false,
        }
    }

    fn resolve_constructed_class(
        &mut self,
        id: ClassId,
        args: &[TypeSyntax],
        span: Span,
    ) -> CanonicalType {
        let params = self.classes[id.0 as usize].type_params.clone();
        if params.is_empty() {
            self.error(
                span,
                format!("class '{}' is not generic", self.class_name(id)),
            );
            return CanonicalType::Class(id);
        }
        if args.is_empty() {
            self.error(
                span,
                format!(
                    "generic class '{}' requires type arguments",
                    self.class_name(id)
                ),
            );
            return CanonicalType::Class(id);
        }
        let Some(map) = self.substitution_map(&params, args, span) else {
            return CanonicalType::Class(id);
        };
        let concrete: Vec<_> = map.iter().map(|(_, ty)| *ty).collect();
        self.intern_generic(id, concrete)
    }

    fn intern_generic(&mut self, class: ClassId, args: Vec<CanonicalType>) -> CanonicalType {
        if let Some(index) = self
            .generics
            .iter()
            .position(|instance| instance.class == class && instance.args == args)
        {
            return CanonicalType::Generic(GenericId(index as u32));
        }
        let id = GenericId(self.generics.len() as u32);
        self.generics.push(GenericInstance { class, args });
        CanonicalType::Generic(id)
    }

    fn generic_map(&self, id: GenericId) -> Vec<(TypeParamId, CanonicalType)> {
        let Some(instance) = self.generics.get(id.0 as usize) else {
            return Vec::new();
        };
        self.classes[instance.class.0 as usize]
            .type_params
            .iter()
            .copied()
            .zip(instance.args.iter().copied())
            .collect()
    }

    fn generic_class(
        &self,
        ty: CanonicalType,
    ) -> Option<(ClassId, Vec<(TypeParamId, CanonicalType)>)> {
        match ty {
            CanonicalType::Class(id) => Some((id, Vec::new())),
            CanonicalType::Generic(id) => {
                let instance = self.generics.get(id.0 as usize)?;
                Some((instance.class, self.generic_map(id)))
            }
            _ => None,
        }
    }

    fn substitute_type(
        &mut self,
        ty: CanonicalType,
        map: &[(TypeParamId, CanonicalType)],
    ) -> CanonicalType {
        if map.is_empty() {
            return ty;
        }
        match ty {
            CanonicalType::Param(id) => map
                .iter()
                .find(|(param, _)| *param == id)
                .map(|(_, concrete)| *concrete)
                .unwrap_or(ty),
            CanonicalType::Optional(kind) => {
                let inner = self.substitute_type(kind.canonical(), map);
                inner.optional().unwrap_or(inner)
            }
            CanonicalType::Array {
                leaf,
                depth,
                limits,
            } => CanonicalType::Array {
                leaf: self.substitute_leaf(leaf, map),
                depth,
                limits,
            },
            CanonicalType::Generic(id) => {
                let Some(instance) = self.generics.get(id.0 as usize).cloned() else {
                    return ty;
                };
                let args = instance
                    .args
                    .iter()
                    .map(|arg| self.substitute_type(*arg, map))
                    .collect();
                self.intern_generic(instance.class, args)
            }
            CanonicalType::Map(id) => {
                let Some(shape) = self.maps.get(id.0 as usize).cloned() else {
                    return ty;
                };
                let key = self.substitute_type(shape.key, map);
                let value = self.substitute_type(shape.value, map);
                CanonicalType::Map(self.intern_map(key, value))
            }
            CanonicalType::Iter(id) => {
                let Some(shape) = self.iters.get(id.0 as usize).cloned() else {
                    return ty;
                };
                let element = self.substitute_type(shape.element, map);
                CanonicalType::Iter(self.intern_iter(element))
            }
            CanonicalType::AsyncIter(id) => {
                let Some(shape) = self.async_iters.get(id.0 as usize).cloned() else {
                    return ty;
                };
                let element = self.substitute_type(shape.element, map);
                CanonicalType::AsyncIter(self.intern_async_iter(element))
            }
            CanonicalType::Async(id) => {
                let Some(shape) = self.asyncs.get(id.0 as usize).cloned() else {
                    return ty;
                };
                let result = self.substitute_type(shape.result, map);
                CanonicalType::Async(self.intern_async(result))
            }
            CanonicalType::Callable(id) => {
                let Some(signature) = self.callables.get(id.0 as usize).cloned() else {
                    return ty;
                };
                let params: Vec<ParamInfo> = signature
                    .params
                    .iter()
                    .zip(signature.autos.iter())
                    .map(|(param, auto)| ParamInfo {
                        name: String::new(),
                        ty: self.substitute_type(*param, map),
                        auto: *auto,
                        default: None,
                    })
                    .collect();
                let ret = self.substitute_type(signature.ret, map);
                CanonicalType::Callable(self.intern_callable(&params, ret))
            }
            other => other,
        }
    }

    fn substitute_leaf(&mut self, leaf: Leaf, map: &[(TypeParamId, CanonicalType)]) -> Leaf {
        match leaf {
            Leaf::Param(id) => self.substitute_type(CanonicalType::Param(id), map).leaf(),
            Leaf::Generic(id) => self.substitute_type(CanonicalType::Generic(id), map).leaf(),
            other => other,
        }
    }

    fn deferred_param_pair(left: CanonicalType, right: CanonicalType) -> Option<CanonicalType> {
        match (left, right) {
            (CanonicalType::Param(a), CanonicalType::Param(b)) if a == b => Some(left),
            _ => None,
        }
    }

    fn specialization_key(name: &str, args: &[CanonicalType]) -> String {
        let rendered = args
            .iter()
            .map(|ty| ty.rust_name())
            .collect::<Vec<_>>()
            .join(", ");
        format!("{name}<{rendered}>")
    }

    fn mangle_specialization(name: &str, args: &[CanonicalType]) -> String {
        let mut mangled = rust_ident_simple(name);
        for arg in args {
            mangled.push('_');
            mangled.push_str(&mangle_type_name(*arg));
        }
        mangled
    }

    fn instantiate_function(
        &mut self,
        name: &str,
        map: &[(TypeParamId, CanonicalType)],
        type_args: &[CanonicalType],
        call_span: Span,
    ) -> String {
        let key = Self::specialization_key(name, type_args);
        if let Some(existing) = self.specialized_keys.get(&key) {
            return existing.clone();
        }
        let mangled = Self::mangle_specialization(name, type_args);
        // While typechecking a generic template, nested calls may still mention
        // type parameters (`one<T>`). Those are only for signature checking —
        // do not emit a monomorphized body until the args are concrete.
        if type_args.iter().any(|ty| self.type_mentions_param(*ty)) {
            return mangled;
        }
        let Some(decl) = self.function_asts.get(name).cloned() else {
            return name.to_string();
        };
        let signature = self.functions.get(name).cloned();
        let Some(signature) = signature else {
            return name.to_string();
        };
        let display = {
            let args = type_args
                .iter()
                .map(|ty| self.spell(*ty))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name}<{args}>")
        };
        let saved_return = self.current_return;
        let saved_instantiation = self.instantiation.clone();
        let saved_substitution = self.param_substitution.clone();
        self.instantiation = Some(display);
        self.param_substitution = Some(map.to_vec());
        self.push_type_params(&signature.type_params);
        let params: Vec<(String, CanonicalType, bool)> = signature
            .params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    self.substitute_type(param.ty, map),
                    param.auto,
                )
            })
            .collect();
        let declared_ret = self.substitute_type(signature.ret, map);
        let has_emit = contains_emit(&decl.body.statements);
        let has_await = contains_await(&decl.body.statements);
        let is_async = declared_ret.is_async();
        let is_async_iter = declared_ret.is_async_iter();
        let is_generator = declared_ret.is_iter() && has_emit;
        if has_emit && is_async {
            self.error(decl.span, "emit is not allowed inside an async function");
        } else if has_emit && !declared_ret.is_iter() && !is_async_iter {
            self.error(
                decl.span,
                "emit is only allowed in functions that return iter<T> or asyncIter<T>",
            );
        }
        if has_await && is_generator {
            self.error(decl.span, "await is not allowed inside a generator");
        }
        let body_ret = if is_async {
            self.async_result(declared_ret).unwrap_or(CanonicalType::Void)
        } else {
            declared_ret
        };
        let path_ret = if is_generator || is_async_iter {
            CanonicalType::Void
        } else {
            body_ret
        };
        let saved_in_async = self.in_async;
        let saved_in_async_iter = self.in_async_iter;
        self.current_return = path_ret;
        self.in_async = is_async;
        self.in_async_iter = is_async_iter;
        self.scopes.push(HashMap::new());
        for (param_name, ty, auto) in &params {
            if *ty != CanonicalType::Void {
                self.scopes.last_mut().unwrap().insert(
                    param_name.clone(),
                    Binding {
                        ty: *ty,
                        mutable: true,
                        auto: *auto,
                        ready: true,
                        moved: false,
                    },
                );
            }
        }
        let saved_generator = self.generator_element;
        self.generator_element = if is_generator {
            self.iter_element(declared_ret)
        } else if is_async_iter {
            self.async_iter_element(declared_ret)
        } else {
            None
        };
        let mut body = Vec::new();
        for statement in &decl.body.statements {
            if let Some(checked) = self.check_stmt(statement) {
                body.push(checked);
            }
        }
        if !is_generator
            && !is_async_iter
            && path_ret != CanonicalType::Void
            && !guaranteed_return(&decl.body.statements)
        {
            self.error(
                call_span,
                format!("function '{name}' must return a value on every path"),
            );
        }
        self.scopes.pop();
        self.generator_element = saved_generator;
        self.in_async = saved_in_async;
        self.in_async_iter = saved_in_async_iter;
        self.pop_type_params();
        self.param_substitution = saved_substitution;
        self.current_return = saved_return;
        self.instantiation = saved_instantiation;
        self.specializations.push(CheckedFn {
            name: mangled.clone(),
            params: params.into_iter().map(|(n, ty, _)| (n, ty)).collect(),
            ret: if is_async_iter || is_generator {
                declared_ret
            } else {
                body_ret
            },
            body,
            type_params: Vec::new(),
            is_generator,
            is_async,
            is_async_iter,
        });
        self.specialized_keys.insert(key, mangled.clone());
        mangled
    }

    fn substitution_map(
        &mut self,
        params: &[TypeParamId],
        args: &[TypeSyntax],
        span: Span,
    ) -> Option<Vec<(TypeParamId, CanonicalType)>> {
        if params.len() != args.len() {
            self.error(
                span,
                format!(
                    "expected {} type argument{}, found {}",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    args.len()
                ),
            );
            return None;
        }
        let mut map = Vec::new();
        for (param, arg) in params.iter().zip(args) {
            let ty = self.resolve_type(arg);
            if ty == CanonicalType::Void || ty.is_void_array() {
                self.error(arg.span, "void is not a valid type argument");
            }
            map.push((*param, ty));
        }
        Some(map)
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
            let ok = ret == CanonicalType::Void
                || self.async_result(ret) == Some(CanonicalType::Void);
            if !ok {
                self.error(
                    main.return_type.span,
                    "'main' must return void or async<void>",
                );
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
        let declared_ret = signature.ret;
        let type_params = signature.type_params.clone();
        let has_emit = contains_emit(&function.body.statements);
        let has_await = contains_await(&function.body.statements);
        let is_async = declared_ret.is_async();
        let is_async_iter = declared_ret.is_async_iter();
        let is_generator = declared_ret.is_iter() && has_emit;
        if has_emit && is_async {
            self.error(function.span, "emit is not allowed inside an async function");
        } else if has_emit && !declared_ret.is_iter() && !is_async_iter {
            self.error(
                function.span,
                "emit is only allowed in functions that return iter<T> or asyncIter<T>",
            );
        }
        if has_await && is_generator {
            self.error(function.span, "await is not allowed inside a generator");
        } else if has_await && !is_async && !is_async_iter {
            self.error(
                function.span,
                "await can only be used inside an async or asyncIter function",
            );
        }
        let body_ret = if is_async {
            self.async_result(declared_ret).unwrap_or(CanonicalType::Void)
        } else {
            declared_ret
        };
        // Generators / asyncIters use emit; body "return type" for path checking is void.
        let path_ret = if is_generator || is_async_iter {
            CanonicalType::Void
        } else {
            body_ret
        };
        self.push_type_params(&type_params);
        self.current_return = path_ret;
        self.in_async = is_async;
        self.in_async_iter = is_async_iter;
        self.generator_element = if is_generator {
            self.iter_element(declared_ret)
        } else if is_async_iter {
            self.async_iter_element(declared_ret)
        } else {
            None
        };
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
                        moved: false,
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
        // Generators / asyncIters / async void finish by falling off the end or bare `return;`.
        if !is_generator
            && !is_async_iter
            && path_ret != CanonicalType::Void
            && !guaranteed_return(&function.body.statements)
        {
            self.error(
                function.span,
                format!(
                    "function '{}' must return a value on every path",
                    function.name.name
                ),
            );
        }
        self.scopes.pop();
        self.generator_element = None;
        self.in_async = false;
        self.in_async_iter = false;
        let type_param_names = self.type_param_names(&type_params);
        self.pop_type_params();
        Some(CheckedFn {
            name: function.name.name.clone(),
            params,
            ret: if is_async_iter || is_generator {
                declared_ret
            } else {
                body_ret
            },
            body,
            type_params: type_param_names,
            is_generator,
            is_async,
            is_async_iter,
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
                if self.generator_element.is_some() {
                    if stmt.value.is_some() {
                        self.error(
                            stmt.span,
                            "a generator cannot return a value; use emit for elements",
                        );
                    }
                    return Some(CheckedStmt::Return(None));
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
            Stmt::Emit(stmt) => {
                if self.in_async {
                    self.error(stmt.span, "emit is not allowed inside an async function");
                }
                let Some(element) = self.generator_element else {
                    self.error(stmt.span, "emit is only allowed inside a generator");
                    let _ = self.check_expr(&stmt.value, None, false);
                    return Some(CheckedStmt::Emit {
                        value: self.dummy(None),
                    });
                };
                let checked = self.check_expr(&stmt.value, Some(element), false);
                let converted = self.apply_conversion(checked, element, false, stmt.value.span());
                Some(CheckedStmt::Emit { value: converted })
            }
            Stmt::Await(stmt) => Some(self.check_await(stmt)),
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
            Stmt::Conditional(stmt) => {
                let condition = self.check_expr(&stmt.condition, Some(CanonicalType::Bln), false);
                if condition.ty() != CanonicalType::Bln {
                    self.error(
                        stmt.condition.span(),
                        "a ternary condition must have type bln",
                    );
                }
                let (then_narrow, _) = self.null_split(&stmt.condition);
                self.push_narrow(&then_narrow);
                let action = self.check_stmt(&stmt.action);
                self.pop_narrow();
                Some(CheckedStmt::If {
                    condition,
                    then_body: action.into_iter().collect(),
                    else_body: None,
                })
            }
            Stmt::Expression(stmt) => {
                let expr = self.check_expr(&stmt.expression, None, true);
                if expr.ty() == CanonicalType::ErrorHandle {
                    self.error(
                        stmt.span,
                        "a runtime error handle cannot be stored, passed, or returned",
                    );
                }
                if expr.ty().is_async() {
                    self.warn(
                        stmt.span,
                        "async operation result is discarded without await",
                    );
                }
                Some(CheckedStmt::Expr(expr))
            }
            Stmt::Super(stmt) => self.check_super(stmt),
        }
    }

    fn check_super(&mut self, stmt: &SuperStmt) -> Option<CheckedStmt> {
        if !self.in_init {
            self.error(stmt.span, "super can only be used in a constructor");
            return None;
        }
        if !self.at_super_slot {
            self.error(
                stmt.span,
                "super must be the first statement in the constructor",
            );
        }
        let Some(class) = self.current_class else {
            self.error(stmt.span, "super requires a parent class");
            return None;
        };
        let Some(parent) = self.classes[class.0 as usize].parent else {
            self.error(stmt.span, "super requires a parent class");
            for argument in &stmt.arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return None;
        };
        let saved = self.current_class;
        self.current_class = Some(class);
        let constructed = self.check_construct(
            parent,
            &stmt.arguments,
            stmt.span,
            CanonicalType::Class(parent),
        );
        self.current_class = saved;
        let CheckedExpr::ClassNew { args, .. } = constructed else {
            return None;
        };
        Some(CheckedStmt::Super { class, args })
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
                moved: false,
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
            LoopKind::AwaitIteration { binding, iterable } => {
                let before = self.ready_snapshot();
                let result = self.check_await_iteration(binding, iterable, stmt);
                self.restore_ready(&before);
                result
            }
        }
    }

    fn check_await_iteration(
        &mut self,
        binding: &BindingHead,
        iterable: &Expr,
        stmt: &LoopStmt,
    ) -> Option<CheckedStmt> {
        if !self.in_async && !self.in_async_iter {
            self.error(
                stmt.span,
                "async iteration (loop await) is only allowed inside an async or asyncIter function",
            );
        }
        let source = self.check_expr(iterable, None, false);
        let Some(element) = self.async_iter_element(source.ty()) else {
            self.error(
                iterable.span(),
                format!(
                    "async iteration requires asyncIter<T>, found '{}'",
                    self.spell(source.ty())
                ),
            );
            return None;
        };
        let declared = self.resolve_type(&binding.ty);
        if declared != element {
            self.error(
                binding.ty.span,
                format!(
                    "type mismatch: expected '{}', found '{}'",
                    self.spell(element),
                    self.spell(declared)
                ),
            );
        }
        self.scopes.push(HashMap::new());
        if !self.reject_reserved_name(&binding.name.name, binding.name.span) {
            self.scopes.last_mut().unwrap().insert(
                binding.name.name.clone(),
                Binding {
                    ty: declared,
                    mutable: true,
                    auto: false,
                    ready: true,
                    moved: false,
                },
            );
        }
        self.loop_depth += 1;
        let body = self.check_block_stmts(&stmt.body);
        self.loop_depth -= 1;
        self.scopes.pop();
        Some(CheckedStmt::AsyncIterate {
            name: binding.name.name.clone(),
            ty: declared,
            source,
            body,
        })
    }

    fn check_await(&mut self, stmt: &crate::parser::AwaitStmt) -> CheckedStmt {
        if self.generator_element.is_some() && !self.in_async_iter {
            self.error(stmt.span, "await is not allowed inside a generator");
        }
        if !self.in_async && !self.in_async_iter {
            self.error(
                stmt.span,
                "await can only be used inside an async or asyncIter function",
            );
        }
        let value = self.check_expr(&stmt.value, None, false);
        let value = self.consume_async_value(value, stmt.value.span());
        let Some(result) = self.async_result(value.ty()) else {
            self.error(
                stmt.value.span(),
                format!(
                    "await requires an async<T> operation, found '{}'",
                    self.spell(value.ty())
                ),
            );
            return CheckedStmt::Await {
                name: None,
                value,
                result_ty: CanonicalType::Void,
            };
        };
        match &stmt.binding {
            None => {
                if result != CanonicalType::Void {
                    self.error(
                        stmt.span,
                        "await without a binding is only allowed for async<void> in this version",
                    );
                }
                CheckedStmt::Await {
                    name: None,
                    value,
                    result_ty: result,
                }
            }
            Some(binding) => {
                let declared = self.resolve_type(&binding.ty);
                if declared != result {
                    self.error(
                        binding.ty.span,
                        format!(
                            "await result type '{}' does not match declared type '{}'",
                            self.spell(result),
                            self.spell(declared)
                        ),
                    );
                }
                if self.reject_reserved_name(&binding.name.name, binding.name.span) {
                    return CheckedStmt::Await {
                        name: Some(binding.name.name.clone()),
                        value,
                        result_ty: declared,
                    };
                }
                if self
                    .scopes
                    .last()
                    .is_some_and(|scope| scope.contains_key(&binding.name.name))
                {
                    self.error(
                        binding.name.span,
                        format!(
                            "'{}' is already declared in this scope",
                            binding.name.name
                        ),
                    );
                } else {
                    self.scopes.last_mut().unwrap().insert(
                        binding.name.name.clone(),
                        Binding {
                            ty: declared,
                            mutable: true,
                            auto: false,
                            ready: true,
                            moved: false,
                        },
                    );
                }
                CheckedStmt::Await {
                    name: Some(binding.name.name.clone()),
                    value,
                    result_ty: declared,
                }
            }
        }
    }

    /// Mark an async local as moved when consumed by await/assignment/argument.
    fn consume_async_value(&mut self, value: CheckedExpr, span: Span) -> CheckedExpr {
        if !value.ty().is_async() {
            return value;
        }
        if let CheckedExpr::Load { name, .. } = &value {
            if let Some(binding) = self.lookup_mut(name) {
                if binding.moved {
                    self.error(span, format!("use of moved async operation '{name}'"));
                } else {
                    binding.moved = true;
                }
            }
        }
        value
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Binding> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                return Some(binding);
            }
        }
        None
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
            | CanonicalType::Interface(_)
            | CanonicalType::Generic(_)
            | CanonicalType::Param(_)
            | CanonicalType::Type
            | CanonicalType::Void
            | CanonicalType::ErrorHandle
            | CanonicalType::Callable(_)
            | CanonicalType::Iter(_)
            | CanonicalType::AsyncIter(_)
            | CanonicalType::Async(_) => None,
        }
    }

    fn can_delay(&self, ty: CanonicalType) -> bool {
        matches!(
            ty,
            CanonicalType::Class(_)
                | CanonicalType::Interface(_)
                | CanonicalType::Generic(_)
                | CanonicalType::Record(_)
        )
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
            if let Some(lambda) = as_lambda(initializer) {
                return self.bind_lambda_variable(decl, ty, lambda);
            }
            let saved = self.allow_convert;
            self.allow_convert = decl.conversion == ConversionMode::Auto;
            let value = self.check_expr(initializer, Some(ty), false);
            self.allow_convert = saved;
            let converted = self.apply_conversion(
                value,
                ty,
                decl.conversion == ConversionMode::Auto,
                initializer.span(),
            );
            let converted = if ty.is_async() {
                self.consume_async_value(converted, initializer.span())
            } else {
                converted
            };
            (converted, true)
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
                moved: false,
            },
        );
        Some(CheckedStmt::Let {
            name: decl.name.name.clone(),
            ty,
            value,
            mutable: decl.mutable,
        })
    }

    fn bind_lambda_variable(
        &mut self,
        decl: &VariableDecl,
        declared: CanonicalType,
        lambda: &LambdaExpr,
    ) -> Option<CheckedStmt> {
        let saved_name = self.lambda_var.clone();
        self.lambda_var = Some(decl.name.name.clone());
        let value = self.check_lambda(lambda);
        self.lambda_var = saved_name;
        if decl.conversion != ConversionMode::Auto {
            self.error(
                decl.ty.span,
                format!("cannot convert a lambda to '{}'", self.spell(declared)),
            );
            return None;
        }
        let CanonicalType::Callable(id) = value.ty() else {
            return None;
        };
        let ret = self
            .callables
            .get(id.0 as usize)
            .map(|sig| sig.ret)
            .unwrap_or(CanonicalType::Void);
        if ret != declared {
            self.error(
                decl.ty.span,
                format!(
                    "type mismatch: expected a lambda returning '{}', found '{}'",
                    self.spell(declared),
                    self.spell(ret)
                ),
            );
        }
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
        let ty = value.ty();
        self.scopes.last_mut().unwrap().insert(
            decl.name.name.clone(),
            Binding {
                ty,
                mutable: decl.mutable,
                auto: false,
                ready: true,
            moved: false,
            },
        );
        Some(CheckedStmt::Let {
            name: decl.name.name.clone(),
            ty,
            value,
            mutable: decl.mutable,
        })
    }

    fn check_lambda(&mut self, lambda: &LambdaExpr) -> CheckedExpr {
        let ret = self.resolve_type(&lambda.ret);
        if ret.is_void_array() {
            self.error(lambda.ret.span, "void is not a valid array element type");
        }
        let _ = self.reject_unspecified_type(ret, lambda.ret.span);
        let params = self.resolve_parameters(&lambda.params);
        let id = self.intern_callable(&params, ret);
        let saved_return = self.current_return;
        let saved_loop = self.loop_depth;
        let saved_finally = self.finally_depth;
        let saved_floor = self.lambda_floor;
        self.current_return = ret;
        self.loop_depth = 0;
        self.finally_depth = 0;
        self.scopes.push(HashMap::new());
        self.lambda_floor = Some(self.scopes.len() - 1);
        for param in &params {
            self.scopes.last_mut().unwrap().insert(
                param.name.clone(),
                Binding {
                    ty: param.ty,
                    mutable: true,
                    auto: param.auto,
                    ready: true,
                moved: false,
                },
            );
        }
        let body = self.check_block_stmts(&lambda.body);
        if ret != CanonicalType::Void && !guaranteed_return(&lambda.body.statements) {
            self.error(lambda.span, "lambda must return a value on every path");
        }
        self.scopes.pop();
        self.current_return = saved_return;
        self.loop_depth = saved_loop;
        self.finally_depth = saved_finally;
        self.lambda_floor = saved_floor;
        CheckedExpr::Lambda {
            params: params
                .iter()
                .map(|param| (param.name.clone(), param.ty))
                .collect(),
            body: Box::new(body),
            ret,
            ty: CanonicalType::Callable(id),
        }
    }

    fn finish_invoke(
        &mut self,
        callee: CheckedExpr,
        id: CallableId,
        arguments: &[Expr],
        span: Span,
        callee_name: &str,
    ) -> CheckedExpr {
        let sig = self
            .callables
            .get(id.0 as usize)
            .cloned()
            .unwrap_or(CallableSignature {
                params: Vec::new(),
                autos: Vec::new(),
                ret: CanonicalType::Void,
            });
        let params = sig
            .params
            .iter()
            .zip(sig.autos.iter())
            .map(|(ty, auto)| (*ty, *auto))
            .collect::<Vec<_>>();
        let args = self.check_arguments(arguments, &params, span);
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::Invoke {
            callee: Box::new(callee),
            args,
            ty: sig.ret,
            callee_name: callee_name.to_string(),
            file,
            line,
            column,
            end_column,
        }
    }

    fn binding_scope(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rposition(|scope| scope.contains_key(name))
    }

    fn reject_capture(&mut self, name: &str, scope: usize, span: Span) -> bool {
        if self.lambda_floor.is_some_and(|floor| scope < floor) {
            self.error(span, format!("lambda cannot capture '{name}'"));
            return true;
        }
        false
    }

    fn check_expr(
        &mut self,
        expr: &Expr,
        expected: Option<CanonicalType>,
        allow_void: bool,
    ) -> CheckedExpr {
        let saved_convert = self.allow_convert;
        if !matches!(expr, Expr::Ternary(_) | Expr::Parenthesized { .. }) {
            self.allow_convert = false;
        }
        let checked = match expr {
            Expr::Parenthesized { expression, .. } => {
                self.check_expr(expression, expected, allow_void)
            }
            Expr::Ternary(ternary) => self.check_ternary(ternary, expected, allow_void),
            Expr::Lambda(lambda) => self.check_lambda(lambda),
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
            Expr::Super { span } => {
                self.error(*span, "super must name a parent member");
                self.dummy(None)
            }
            Expr::InterpolatedString(expr) => self.check_interpolation(expr),
        };
        self.allow_convert = saved_convert;
        if !allow_void && checked.ty() == CanonicalType::Void {
            self.error(expr.span(), "a void value cannot be used here");
        }
        checked
    }

    fn check_ternary(
        &mut self,
        expr: &TernaryExpr,
        expected: Option<CanonicalType>,
        allow_void: bool,
    ) -> CheckedExpr {
        let allow = self.allow_convert;
        self.allow_convert = false;
        let condition = self.check_expr(&expr.condition, Some(CanonicalType::Bln), false);
        if condition.ty() != CanonicalType::Bln {
            self.error(
                expr.condition.span(),
                "a ternary condition must have type bln",
            );
        }
        let (then_narrow, else_narrow) = self.null_split(&expr.condition);
        self.push_narrow(&then_narrow);
        self.allow_convert = allow;
        let then_branch = self.check_expr(&expr.then_branch, expected, allow_void);
        self.pop_narrow();
        self.push_narrow(&else_narrow);
        self.allow_convert = allow;
        let else_branch = self.check_expr(&expr.else_branch, expected, allow_void);
        self.pop_narrow();
        self.allow_convert = allow;
        let (then_branch, else_branch, ty) = if let Some(target) = expected {
            let then_branch =
                self.apply_conversion(then_branch, target, allow, expr.then_branch.span());
            let else_branch =
                self.apply_conversion(else_branch, target, allow, expr.else_branch.span());
            (then_branch, else_branch, target)
        } else if then_branch.ty() == else_branch.ty() {
            let ty = then_branch.ty();
            (then_branch, else_branch, ty)
        } else {
            self.error(
                expr.span,
                format!(
                    "type mismatch: ternary branches have types '{}' and '{}'",
                    self.spell(then_branch.ty()),
                    self.spell(else_branch.ty())
                ),
            );
            let ty = then_branch.ty();
            (then_branch, else_branch, ty)
        };
        CheckedExpr::Ternary {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch: Box::new(else_branch),
            ty,
        }
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
            UnaryOperator::BitNot => expected.filter(|ty| ty.is_integer()),
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
                if !operand.ty().is_numeric() && !operand.ty().is_param() {
                    self.error(expr.span, "unary '-' requires a numeric operand");
                }
                CheckedExpr::Unary {
                    op: UnaryOperator::Negate,
                    ty: operand.ty(),
                    expr: Box::new(operand),
                    span: expr.span,
                }
            }
            UnaryOperator::BitNot => {
                if !operand.ty().is_integer() && !operand.ty().is_param() {
                    self.error(expr.span, "operator '~' requires an integer operand");
                }
                CheckedExpr::Unary {
                    op: UnaryOperator::BitNot,
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
            if left.ty() != CanonicalType::Bln || right.ty() != CanonicalType::Bln {
                self.error(expr.span, "a logical operand must have type bln");
            }
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
            BinaryOperator::BitAnd
                | BinaryOperator::BitOr
                | BinaryOperator::BitXor
                | BinaryOperator::Shl
                | BinaryOperator::Shr
        ) {
            let (left, right) = self.check_numeric_operands(&expr.left, &expr.right, expected);
            return if matches!(expr.operator, BinaryOperator::Shl | BinaryOperator::Shr) {
                self.finish_shift(expr, left, right)
            } else {
                self.finish_bitwise(expr, left, right)
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
        if left.ty().is_object() || right.ty().is_object() {
            let equality = matches!(
                expr.operator,
                BinaryOperator::Equal | BinaryOperator::NotEqual
            );
            if equality
                && (left.ty() == right.ty()
                    || self.assignable(left.ty(), right.ty())
                    || self.assignable(right.ty(), left.ty()))
            {
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
        if let Some(_) = Self::deferred_param_pair(left.ty(), right.ty()) {
            return CheckedExpr::Binary {
                op: expr.operator,
                ty: CanonicalType::Bln,
                left: Box::new(left),
                right: Box::new(right),
                compare: Some(CompareMode::Same),
                span: expr.span,
            };
        }
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

    fn finish_bitwise(
        &mut self,
        expr: &BinaryExpr,
        left: CheckedExpr,
        right: CheckedExpr,
    ) -> CheckedExpr {
        self.finish_integer_op(expr.operator, left, right, expr.span, false)
    }

    fn finish_shift(
        &mut self,
        expr: &BinaryExpr,
        left: CheckedExpr,
        right: CheckedExpr,
    ) -> CheckedExpr {
        self.finish_integer_op(expr.operator, left, right, expr.span, true)
    }

    fn finish_integer_op(
        &mut self,
        op: BinaryOperator,
        left: CheckedExpr,
        right: CheckedExpr,
        span: Span,
        shift: bool,
    ) -> CheckedExpr {
        if let Some(ty) = Self::deferred_param_pair(left.ty(), right.ty()) {
            return CheckedExpr::Binary {
                op,
                ty,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span,
            };
        }
        if !left.ty().is_integer() || !right.ty().is_integer() {
            self.error(
                span,
                format!(
                    "operator '{}' requires integer operands",
                    operator_token(op)
                ),
            );
            return CheckedExpr::Binary {
                op,
                ty: left.ty(),
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span,
            };
        }
        if shift {
            return CheckedExpr::Binary {
                op,
                ty: left.ty(),
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span,
            };
        }
        match numeric::arithmetic(left.ty(), right.ty(), false) {
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
            Err(_) => {
                self.error(
                    span,
                    format!(
                        "operator '{}' requires integer operands",
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

    fn finish_arithmetic(
        &mut self,
        expr: &BinaryExpr,
        left: CheckedExpr,
        right: CheckedExpr,
    ) -> CheckedExpr {
        if let Some(ty) = Self::deferred_param_pair(left.ty(), right.ty()) {
            return CheckedExpr::Binary {
                op: expr.operator,
                ty,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span: expr.span,
            };
        }
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
            if self.lambda_var.as_deref() == Some(ident.name.as_str()) {
                self.error(
                    ident.span,
                    format!("lambda cannot capture '{}'", ident.name),
                );
            } else {
                self.error(ident.span, format!("unknown name '{}'", ident.name));
            }
            let _ = self.check_expr(&expr.value, None, false);
            return self.dummy(None);
        };
        if let Some(scope) = self.binding_scope(&ident.name) {
            if self.reject_capture(&ident.name, scope, ident.span) {
                let _ = self.check_expr(&expr.value, None, false);
                return self.dummy(Some(binding.ty));
            }
        }
        self.reject_binding_write(&ident.name, ident.span, false);
        let ty = binding.ty;
        let auto = binding.auto;
        let current = CheckedExpr::Load {
            name: ident.name.clone(),
            ty,
        };
        let value = self.store_value(expr.op, current, &expr.value, ty, auto, expr.value.span());
        self.mark_ready(&ident.name);
        if ty.is_async() {
            if let Some(binding) = self.binding_mut(&ident.name) {
                binding.moved = false;
            }
        }
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
            let saved = self.allow_convert;
            self.allow_convert = auto;
            let checked = self.check_expr(value, Some(ty), false);
            self.allow_convert = saved;
            let converted = self.apply_conversion(checked, ty, auto, span);
            return if ty.is_async() {
                self.consume_async_value(converted, span)
            } else {
                converted
            };
        };
        let rhs = if is_numeric_literal(value) {
            self.check_expr(value, Some(current.ty()), false)
        } else {
            self.check_expr(value, None, false)
        };
        let combined = if matches!(
            op,
            BinaryOperator::BitAnd
                | BinaryOperator::BitOr
                | BinaryOperator::BitXor
                | BinaryOperator::Shl
                | BinaryOperator::Shr
        ) {
            self.finish_integer_op(
                op,
                current,
                rhs,
                span,
                matches!(op, BinaryOperator::Shl | BinaryOperator::Shr),
            )
        } else {
            self.finish_op(op, current, rhs, span)
        };
        self.apply_conversion(combined, ty, auto, span)
    }

    fn finish_op(
        &mut self,
        op: BinaryOperator,
        left: CheckedExpr,
        right: CheckedExpr,
        span: Span,
    ) -> CheckedExpr {
        if let Some(ty) = Self::deferred_param_pair(left.ty(), right.ty()) {
            return CheckedExpr::Binary {
                op,
                ty,
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span,
            };
        }
        if left.ty().is_param() || right.ty().is_param() {
            self.error(
                span,
                format!(
                    "operator '{}' requires matching type-parameter operands",
                    operator_token(op)
                ),
            );
            return CheckedExpr::Binary {
                op,
                ty: left.ty(),
                left: Box::new(left),
                right: Box::new(right),
                compare: None,
                span,
            };
        }
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
            if self.lambda_var.as_deref() == Some(ident.name.as_str()) {
                self.error(
                    ident.span,
                    format!("lambda cannot capture '{}'", ident.name),
                );
            } else {
                self.error(ident.span, format!("unknown name '{}'", ident.name));
            }
            return self.dummy(None);
        };
        if let Some(scope) = self.binding_scope(&ident.name) {
            if self.reject_capture(&ident.name, scope, ident.span) {
                return self.dummy(Some(binding.ty));
            }
        }
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
            if matches!(&*member.object, Expr::Super { .. }) {
                if !expr.type_arguments.is_empty() {
                    self.error(expr.span, "super calls cannot take type arguments");
                }
                return self.check_super_call(member, &expr.arguments, expr.span);
            }
            if let Expr::Identifier(object) = &*member.object {
                if self.imports.contains_key(&object.name) {
                    if !expr.type_arguments.is_empty() {
                        self.error(expr.span, "library calls cannot take type arguments");
                    }
                    return self.check_library_call(member, &expr.arguments, expr.span, allow_void);
                }
                if self.lookup(&object.name).is_none()
                    && self.class_names.contains_key(&object.name)
                {
                    return self.check_static_call(
                        member,
                        &expr.type_arguments,
                        &expr.arguments,
                        expr.span,
                    );
                }
            }
            return self.check_array_method(
                member,
                &expr.type_arguments,
                &expr.arguments,
                expr.span,
            );
        }
        if let Expr::Identifier(ident) = &*expr.callee {
            if ident.name == "lengthOf" {
                return self.check_length_of(&expr.arguments, expr.span);
            }
            if ident.name == "typeOf" {
                return self.check_type_of(&expr.arguments, expr.span);
            }
            if ident.name == "range" {
                return self.check_range(&expr.arguments, expr.span);
            }
            return self.check_user_call(ident, &expr.type_arguments, &expr.arguments, expr.span);
        }
        if !expr.type_arguments.is_empty() {
            self.error(
                expr.span,
                "type arguments require a generic function or class",
            );
        }
        let callee = self.check_expr(&expr.callee, None, false);
        if let CanonicalType::Callable(id) = callee.ty() {
            return self.finish_invoke(callee, id, &expr.arguments, expr.span, "lambda");
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
        let Some(function) = self
            .manifest
            .libraries
            .get(&import.library)
            .and_then(|library| library.functions.get(&member.member.name))
            .cloned()
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
        let Some(ret) = self.manifest_return_type(&function.returns) else {
            self.error(span, "the library function has an unknown return type");
            return self.dummy(None);
        };
        let params = function_params(&function);
        let args = self.check_arguments(arguments, &params, span);
        CheckedExpr::Call {
            path: function.rust_path,
            callee: member.member.name.clone(),
            args,
            type_args: Vec::new(),
            ty: ret,
            file: self.source_name.to_string(),
            line: span.start.line,
            column: span.start.column,
            end_column: span.end.column,
        }
    }

    fn manifest_return_type(&mut self, ty: &ManifestType) -> Option<CanonicalType> {
        if let Some(inner) = ty
            .name
            .strip_prefix("async<")
            .and_then(|rest| rest.strip_suffix('>'))
        {
            let result = canonical_name(inner)?;
            return Some(CanonicalType::Async(self.intern_async(result)));
        }
        canonical_name(&ty.name)
    }

    fn check_range(&mut self, arguments: &[Expr], span: Span) -> CheckedExpr {
        let ty = CanonicalType::Iter(self.intern_iter(CanonicalType::Int64));
        let (file, line, column, end_column) = self.located(span);
        let zero = || CheckedExpr::Int {
            value: 0,
            ty: CanonicalType::Int64,
        };
        let one = || CheckedExpr::Int {
            value: 1,
            ty: CanonicalType::Int64,
        };
        let check_int = |checker: &mut Self, expr: &Expr| {
            let checked = checker.check_expr(expr, Some(CanonicalType::Int64), false);
            checker.apply_conversion(checked, CanonicalType::Int64, false, expr.span())
        };
        match arguments.len() {
            1 => {
                let end = check_int(self, &arguments[0]);
                CheckedExpr::Range {
                    start: Box::new(zero()),
                    end: Box::new(end),
                    step: Box::new(one()),
                    ty,
                    file,
                    line,
                    column,
                    end_column,
                }
            }
            2 => {
                let start = check_int(self, &arguments[0]);
                let end = check_int(self, &arguments[1]);
                CheckedExpr::Range {
                    start: Box::new(start),
                    end: Box::new(end),
                    step: Box::new(one()),
                    ty,
                    file,
                    line,
                    column,
                    end_column,
                }
            }
            3 => {
                let start = check_int(self, &arguments[0]);
                let end = check_int(self, &arguments[1]);
                let step = check_int(self, &arguments[2]);
                if let CheckedExpr::Int { value: 0, .. } = step {
                    self.error(arguments[2].span(), "range step cannot be zero");
                }
                CheckedExpr::Range {
                    start: Box::new(start),
                    end: Box::new(end),
                    step: Box::new(step),
                    ty,
                    file,
                    line,
                    column,
                    end_column,
                }
            }
            count => {
                self.error(
                    span,
                    format!("range expects 1, 2, or 3 arguments, found {count}"),
                );
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                self.dummy(Some(ty))
            }
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
        if described.is_generic()
            || matches!(
                described,
                CanonicalType::Array {
                    leaf: Leaf::Generic(_),
                    ..
                }
            )
            || described
                .optional_inner()
                .is_some_and(|inner| inner.is_generic())
        {
            self.error(
                argument.span(),
                "typeOf is not supported for generic class values",
            );
            return self.dummy(Some(CanonicalType::Type));
        }
        CheckedExpr::TypeOf {
            name: self.spell(described),
            record: described.is_record(),
            expr: Box::new(value),
        }
    }

    fn check_user_call(
        &mut self,
        ident: &Identifier,
        type_arguments: &[TypeSyntax],
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        if let Some(scope) = self.binding_scope(&ident.name) {
            if self.reject_capture(&ident.name, scope, ident.span) {
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            }
            if let Some(binding) = self.lookup(&ident.name) {
                if let CanonicalType::Callable(id) = binding.ty {
                    if !type_arguments.is_empty() {
                        self.error(span, "a function value cannot take type arguments");
                    }
                    let callee = CheckedExpr::Load {
                        name: ident.name.clone(),
                        ty: binding.ty,
                    };
                    return self.finish_invoke(callee, id, arguments, span, &ident.name);
                }
                if !self.functions.contains_key(&ident.name) {
                    self.error(
                        ident.span,
                        format!("cannot call a value of type '{}'", self.spell(binding.ty)),
                    );
                    for argument in arguments {
                        let _ = self.check_expr(argument, None, false);
                    }
                    return self.dummy(None);
                }
            }
        }
        if self.lambda_var.as_deref() == Some(ident.name.as_str()) {
            self.error(
                ident.span,
                format!("lambda cannot capture '{}'", ident.name),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        }
        if self.class_names.contains_key(&ident.name) {
            let origin = self.class_names[&ident.name];
            let is_generic = !self.classes[origin.0 as usize].type_params.is_empty();
            if is_generic || !type_arguments.is_empty() {
                let constructed =
                    self.resolve_constructed_class(origin, type_arguments, ident.span);
                if self.classes[origin.0 as usize].abstract_class {
                    self.error(
                        ident.span,
                        format!("cannot construct abstract class '{}'", ident.name),
                    );
                }
                return self.check_construct(origin, arguments, span, constructed);
            }
            if !self.functions.contains_key(&ident.name) {
                if self.classes[origin.0 as usize].abstract_class {
                    self.error(
                        ident.span,
                        format!("cannot construct abstract class '{}'", ident.name),
                    );
                }
                let deprecated = self.classes[origin.0 as usize]
                    .init
                    .as_ref()
                    .is_some_and(|init| init.deprecated);
                self.note_deprecated(deprecated, ident.span, &ident.name);
                return self.check_construct(origin, arguments, span, CanonicalType::Class(origin));
            }
        }
        if self.interface_names.contains_key(&ident.name)
            && !self.functions.contains_key(&ident.name)
        {
            self.error(
                ident.span,
                format!("cannot construct interface '{}'", ident.name),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        }
        let Some(signature) = self.functions.get(&ident.name).cloned() else {
            self.error(ident.span, format!("unknown function '{}'", ident.name));
            return self.dummy(None);
        };
        if !signature.type_params.is_empty() && type_arguments.is_empty() {
            self.error(
                span,
                format!("generic function '{}' requires type arguments", ident.name),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(Some(signature.ret));
        }
        if signature.type_params.is_empty() && !type_arguments.is_empty() {
            self.error(span, format!("function '{}' is not generic", ident.name));
        }
        let (ret, params, path) = if signature.type_params.is_empty() {
            (signature.ret, signature.params.clone(), ident.name.clone())
        } else {
            let Some(map) = self.substitution_map(&signature.type_params, type_arguments, span)
            else {
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(Some(signature.ret));
            };
            let type_args: Vec<_> = map.iter().map(|(_, ty)| *ty).collect();
            let path = self.instantiate_function(&ident.name, &map, &type_args, span);
            (
                self.substitute_type(signature.ret, &map),
                signature
                    .params
                    .iter()
                    .map(|param| ParamInfo {
                        name: param.name.clone(),
                        ty: self.substitute_type(param.ty, &map),
                        auto: param.auto,
                        default: param.default.clone(),
                    })
                    .collect::<Vec<_>>(),
                path,
            )
        };
        let args = self.check_call_args(arguments, &params, span);
        CheckedExpr::Call {
            path,
            callee: ident.name.clone(),
            args,
            type_args: Vec::new(),
            ty: ret,
            file: self.source_name.to_string(),
            line: span.start.line,
            column: span.start.column,
            end_column: span.end.column,
        }
    }

    fn check_call_args(
        &mut self,
        arguments: &[Expr],
        params: &[ParamInfo],
        span: Span,
    ) -> Vec<CheckedExpr> {
        let min_args = params
            .iter()
            .position(|param| param.default.is_some())
            .unwrap_or(params.len());
        let max_args = params.len();
        if arguments.len() < min_args || arguments.len() > max_args {
            let expected = if min_args == max_args {
                format!("{max_args}")
            } else {
                format!("{min_args}..={max_args}")
            };
            self.error(
                span,
                format!(
                    "expected {expected} argument{}, found {}",
                    if max_args == 1 { "" } else { "s" },
                    arguments.len()
                ),
            );
        }
        let mut checked = Vec::new();
        for (index, param) in params.iter().enumerate() {
            if let Some(argument) = arguments.get(index) {
                let record_shape = self.accepts_brace(param.ty);
                let expected = if param.auto && !record_shape && !is_ternary_expr(argument) {
                    None
                } else {
                    Some(param.ty)
                };
                let saved = self.allow_convert;
                self.allow_convert = param.auto;
                let value = self.check_expr(argument, expected, false);
                self.allow_convert = saved;
                let converted =
                    self.apply_conversion(value, param.ty, param.auto, argument.span());
                let converted = if param.ty.is_async() || converted.ty().is_async() {
                    self.consume_async_value(converted, argument.span())
                } else {
                    converted
                };
                checked.push(converted);
            } else if let Some(default) = &param.default {
                let value = self.check_expr(default, Some(param.ty), false);
                let converted =
                    self.apply_conversion(value, param.ty, param.auto, default.span());
                let converted = if param.ty.is_async() || converted.ty().is_async() {
                    self.consume_async_value(converted, default.span())
                } else {
                    converted
                };
                checked.push(converted);
            } else {
                // Missing required argument — already diagnosed above.
            }
        }
        for argument in arguments.iter().skip(params.len()) {
            let _ = self.check_expr(argument, None, false);
        }
        checked
    }

    fn check_arguments(
        &mut self,
        arguments: &[Expr],
        params: &[(CanonicalType, bool)],
        span: Span,
    ) -> Vec<CheckedExpr> {
        let infos: Vec<ParamInfo> = params
            .iter()
            .map(|(ty, auto)| ParamInfo {
                name: String::new(),
                ty: *ty,
                auto: *auto,
                default: None,
            })
            .collect();
        self.check_call_args(arguments, &infos, span)
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
        if limits_need_check(from, target) {
            let (file, line, column, end_column) = self.located(span);
            return CheckedExpr::EnsureLimit {
                expr: Box::new(expr),
                ty: target,
                file,
                line,
                column,
                end_column,
            };
        }
        if from == target || self.assignable(from, target) {
            return if from == target {
                expr
            } else {
                CheckedExpr::Upcast {
                    expr: Box::new(expr),
                    ty: target,
                }
            };
        }
        if let (CanonicalType::Callable(from_id), CanonicalType::Callable(to_id)) = (from, target) {
            if self.callable_compatible(from_id, to_id) {
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
            return expr;
        }
        if let Some(inner) = target.optional_inner() {
            if matches!(expr, CheckedExpr::Null { .. }) {
                return CheckedExpr::Null { ty: target };
            }
            if from == inner || self.assignable(from, inner) {
                let expr = if limits_need_check(from, inner) {
                    let (file, line, column, end_column) = self.located(span);
                    CheckedExpr::EnsureLimit {
                        expr: Box::new(expr),
                        ty: inner,
                        file,
                        line,
                        column,
                        end_column,
                    }
                } else if from == inner {
                    expr
                } else {
                    CheckedExpr::Upcast {
                        expr: Box::new(expr),
                        ty: inner,
                    }
                };
                return CheckedExpr::Wrap {
                    expr: Box::new(expr),
                    ty: target,
                };
            }
            if allow {
                if let Some(op) = conversion(from, inner) {
                    let (file, line, column, end_column) = self.located(span);
                    let converted = CheckedExpr::Convert {
                        from,
                        to: inner,
                        op,
                        expr: Box::new(expr),
                        file,
                        line,
                        column,
                        end_column,
                    };
                    return CheckedExpr::Wrap {
                        expr: Box::new(converted),
                        ty: target,
                    };
                }
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
        if target == CanonicalType::Chr && allow && from.is_optional() {
            let inner = from.optional_inner().unwrap_or(from);
            if inner == CanonicalType::Str
                || conversion(inner, CanonicalType::Str).is_some()
                || conversion(inner, CanonicalType::Chr).is_some()
            {
                let (file, line, column, end_column) = self.located(span);
                return CheckedExpr::Convert {
                    from,
                    to: target,
                    op: ConvOp::OptionalToChar,
                    expr: Box::new(expr),
                    file,
                    line,
                    column,
                    end_column,
                };
            }
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
            let saved = self.allow_convert;
            self.allow_convert = auto;
            let checked = self.check_expr(value, Some(field_ty), false);
            self.allow_convert = saved;
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
        if let Expr::Super { .. } = &*expr.object {
            return self.finish_super_field(expr);
        }
        if let Expr::Identifier(object) = &*expr.object {
            if self.lookup(&object.name).is_none() && self.class_names.contains_key(&object.name) {
                let id = self.class_names[&object.name];
                return self.finish_named_static_field(id, expr);
            }
        }
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
        if let Some((id, _)) = self.generic_class(object.ty()) {
            return self.finish_class_get(object, id, expr);
        }
        if let CanonicalType::Interface(id) = object.ty() {
            return self.finish_interface_get(object, id, expr);
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
        if let Expr::Identifier(object) = &*member.object {
            if self.lookup(&object.name).is_none() && self.class_names.contains_key(&object.name) {
                let id = self.class_names[&object.name];
                if self
                    .locate_field(id, &member.member.name)
                    .is_some_and(|(owner, index)| {
                        !self.classes[owner.0 as usize].fields[index].is_static
                    })
                {
                    self.error(
                        member.member.span,
                        format!("field '{}' requires an instance", member.member.name),
                    );
                    let _ = self.check_expr(value, None, false);
                    return self.dummy(None);
                }
                let dummy = CheckedExpr::Bool(false);
                return self.finish_class_set(dummy, id, member, value, op);
            }
        }
        if self.peek_type(&member.object).is_some_and(|ty| {
            ty.is_object() || ty.optional_inner().is_some_and(|inner| inner.is_object())
        }) {
            let object = self.check_expr(&member.object, None, false);
            let object = self.adopt_object(object, member.object.span());
            if let Some((id, _)) = self.generic_class(object.ty()) {
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
        let text = object.ty() == CanonicalType::Str;
        let ty = if object.ty().is_array() {
            object.ty().element().unwrap_or(CanonicalType::Int64)
        } else if text {
            CanonicalType::Chr
        } else {
            self.error(
                expr.object.span(),
                format!("'{}' is not an array or a str", self.spell(object.ty())),
            );
            CanonicalType::Int64
        };
        let (file, line, column, end_column) = self.located(expr.span);
        CheckedExpr::IndexGet {
            object: Box::new(object),
            index: Box::new(index),
            ty,
            text,
            file,
            line,
            column,
            end_column,
        }
    }

    fn check_slice(&mut self, expr: &SliceExpr) -> CheckedExpr {
        let object = self.check_expr(&expr.object, None, false);
        let text = object.ty() == CanonicalType::Str;
        if object.ty().is_map() {
            self.error(expr.span, "maps do not support slicing");
        } else if !object.ty().is_array() && !text {
            self.error(
                expr.object.span(),
                format!("'{}' is not an array or a str", self.spell(object.ty())),
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
            text,
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
            let text = array_ty == CanonicalType::Str;
            if !array_ty.is_array() && !text {
                self.error(
                    index.object.span(),
                    format!("'{}' is not an array or a str", self.spell(array_ty)),
                );
                let _ = self.check_expr(value, None, false);
                return self.dummy(None);
            }
            let element = if text {
                CanonicalType::Chr
            } else {
                array_ty.element().unwrap_or(CanonicalType::Int64)
            };
            let current = CheckedExpr::IndexGet {
                object: Box::new(self.place_load(&place)),
                index: Box::new(checked_index.clone()),
                ty: element,
                text,
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
                text,
                limit: array_ty.limit(),
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
        let text = object.ty() == CanonicalType::Str;
        let element = if object.ty().is_array() {
            object.ty().element().unwrap_or(CanonicalType::Int64)
        } else if text {
            CanonicalType::Chr
        } else {
            self.error(
                index.object.span(),
                format!("'{}' is not an array or a str", self.spell(object.ty())),
            );
            CanonicalType::Int64
        };
        let current = CheckedExpr::IndexGet {
            object: Box::new(object.clone()),
            index: Box::new(checked_index.clone()),
            ty: element,
            text,
            file: file.clone(),
            line,
            column,
            end_column,
        };
        let value = self.store_value(op, current, value, element, false, value.span());
        let limit = if text { None } else { object.ty().limit() };
        CheckedExpr::IndexSet {
            place: None,
            object: Some(Box::new(object)),
            index: Box::new(checked_index),
            value: Box::new(value),
            ty: element,
            text,
            limit,
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
            if !array_ty.is_array() && array_ty != CanonicalType::Str {
                self.error(
                    slice.object.span(),
                    format!("'{}' is not an array or a str", self.spell(array_ty)),
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
            if !object.ty().is_array() && object.ty() != CanonicalType::Str {
                self.error(
                    slice.object.span(),
                    format!("'{}' is not an array or a str", self.spell(object.ty())),
                );
            }
            let array_ty = object.ty();
            (None, Some(Box::new(object)), array_ty)
        };
        let text = array_ty == CanonicalType::Str;
        let replacement = if text { CanonicalType::Str } else { array_ty };
        let checked = self.check_expr(value, Some(replacement), false);
        let value = self.apply_conversion(checked, replacement, false, value.span());
        CheckedExpr::SliceSet {
            place,
            object,
            start,
            stop,
            step,
            value: Box::new(value),
            ty: array_ty,
            text,
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
                    text: false,
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
        let (id, map) = self.generic_class(object.ty())?;
        let Some((owner, index)) = self.locate_field(id, &member.member.name) else {
            if self.has_method_name(id, &member.member.name) {
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
        let public = self.classes[owner.0 as usize].fields[index].public;
        let protected = self.classes[owner.0 as usize].fields[index].protected;
        let const_field = self.classes[owner.0 as usize].fields[index].const_field;
        let ty = self.substitute_type(self.classes[owner.0 as usize].fields[index].ty, &map);
        let name = self.classes[owner.0 as usize].fields[index].name.clone();
        if !self.member_visible(public, protected, owner) {
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
            class: owner,
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
                    .is_some_and(|ty| ty.is_object())
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

    fn check_array_iter(
        &mut self,
        member: &MemberExpr,
        type_arguments: &[TypeSyntax],
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        if !type_arguments.is_empty() {
            self.error(span, "iter() cannot take type arguments");
        }
        if !arguments.is_empty() {
            self.error(
                span,
                format!(
                    "method 'iter' expects 0 arguments, found {}",
                    arguments.len()
                ),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
        }
        let object = self.check_expr(&member.object, None, false);
        let element = if object.ty() == CanonicalType::Str {
            CanonicalType::Chr
        } else {
            object.ty().element().unwrap_or(CanonicalType::Int64)
        };
        let ty = CanonicalType::Iter(self.intern_iter(element));
        CheckedExpr::MakeIter {
            object: Box::new(object),
            ty,
        }
    }

    fn check_iter_method(
        &mut self,
        member: &MemberExpr,
        type_arguments: &[TypeSyntax],
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        if !type_arguments.is_empty() {
            self.error(span, "iterator methods cannot take type arguments");
        }
        let name = member.member.name.as_str();
        let method = match name {
            "hasNext" => IterMethod::HasNext,
            "next" => IterMethod::Next,
            "value" => IterMethod::Value,
            "iter" => IterMethod::Iter,
            other => {
                self.error(
                    member.member.span,
                    format!("unknown iterator method '{other}'"),
                );
                let _ = self.check_expr(&member.object, None, false);
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            }
        };
        if !arguments.is_empty() {
            self.error(
                span,
                format!(
                    "method '{name}' expects 0 arguments, found {}",
                    arguments.len()
                ),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
        }
        let object = self.check_expr(&member.object, None, false);
        let async_iter = object.ty().is_async_iter();
        let element = if async_iter {
            self.async_iter_element(object.ty())
                .unwrap_or(CanonicalType::Int64)
        } else {
            self.iter_element(object.ty())
                .unwrap_or(CanonicalType::Int64)
        };
        let suspend = async_iter && !matches!(method, IterMethod::Iter);
        if suspend && !self.in_async && !self.in_async_iter {
            self.error(
                span,
                format!(
                    "asyncIter method '{name}' can only be used inside an async or asyncIter function"
                ),
            );
        }
        let ty = match method {
            IterMethod::HasNext => CanonicalType::Bln,
            IterMethod::Next | IterMethod::Value => element,
            IterMethod::Iter => object.ty(),
        };
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::IterCall {
            object: Box::new(object),
            method,
            suspend,
            ty,
            file,
            line,
            column,
            end_column,
        }
    }

    fn check_array_method(
        &mut self,
        member: &MemberExpr,
        type_arguments: &[TypeSyntax],
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let name = member.member.name.as_str();
        if let Expr::Identifier(ident) = &*member.object {
            if self
                .lookup(&ident.name)
                .is_some_and(|binding| binding.ty == CanonicalType::ErrorHandle)
            {
                if !type_arguments.is_empty() {
                    self.error(span, "runtime error methods cannot take type arguments");
                }
                return self.finish_handle_call(&ident.name, name, arguments, span);
            }
        }
        if matches!(name, "enumerate" | "entries") {
            self.error(span, format!("{name}() can only be used as a loop source"));
            let _ = self.check_expr(&member.object, None, false);
            return self.dummy(None);
        }
        if let Some(object_ty) = self.peek_type(&member.object) {
            if object_ty.is_iter() || object_ty.is_async_iter() {
                return self.check_iter_method(member, type_arguments, arguments, span);
            }
            if name == "iter" && (object_ty.is_array() || object_ty == CanonicalType::Str) {
                return self.check_array_iter(member, type_arguments, arguments, span);
            }
        }
        let mutating = matches!(
            name,
            "set"
                | "remove"
                | "push"
                | "append"
                | "extend"
                | "insert"
                | "pop"
                | "clear"
                | "reverse"
                | "sort"
        );
        if self.peek_type(&member.object).is_some_and(|ty| {
            ty.is_object() || ty.optional_inner().is_some_and(|inner| inner.is_object())
        }) {
            let object = self.check_expr(&member.object, None, false);
            let object = self.adopt_object(object, member.object.span());
            if let Some((id, _)) = self.generic_class(object.ty()) {
                return self.finish_class_call(object, id, member, type_arguments, arguments, span);
            }
            if let CanonicalType::Interface(id) = object.ty() {
                if !type_arguments.is_empty() {
                    self.error(span, "interface methods cannot take type arguments");
                }
                return self.finish_interface_call(object, id, member, arguments, span);
            }
        }
        if let Some(place) = self.check_place(&member.object, mutating) {
            let ty = self.place_type(&place);
            if ty.is_object() {
                let object = self.check_expr(&member.object, None, false);
                if let Some((id, _)) = self.generic_class(object.ty()) {
                    return self.finish_class_call(
                        object,
                        id,
                        member,
                        type_arguments,
                        arguments,
                        span,
                    );
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
            if ty == CanonicalType::Str {
                return self.finish_text_call(name, Some(place), None, arguments, span, member);
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
        if let Some((id, _)) = self.generic_class(ty) {
            return self.finish_class_call(object, id, member, type_arguments, arguments, span);
        }
        if !type_arguments.is_empty() {
            self.error(span, "type arguments require a generic method");
        }
        if ty == CanonicalType::Str {
            return self.finish_text_call(name, None, Some(object), arguments, span, member);
        }
        self.finish_array_call(name, None, Some(object), ty, arguments, span, member)
    }

    fn finish_text_call(
        &mut self,
        name: &str,
        place: Option<ArrayPlace>,
        object: Option<CheckedExpr>,
        arguments: &[Expr],
        span: Span,
        member: &MemberExpr,
    ) -> CheckedExpr {
        let method = match name {
            "length" => TextMethod::Length,
            "get" => TextMethod::Get,
            "set" => TextMethod::Set,
            "push" => TextMethod::Push,
            "append" => TextMethod::Append,
            "extend" => TextMethod::Extend,
            "insert" => TextMethod::Insert,
            "remove" => TextMethod::Remove,
            "pop" => TextMethod::Pop,
            "clear" => TextMethod::Clear,
            "reverse" => TextMethod::Reverse,
            "copy" => TextMethod::Copy,
            "split" => TextMethod::Split,
            "rsplit" => TextMethod::RSplit,
            "strip" => TextMethod::Strip,
            "lstrip" => TextMethod::LStrip,
            "rstrip" => TextMethod::RStrip,
            "lower" => TextMethod::Lower,
            "upper" => TextMethod::Upper,
            "replace" => TextMethod::Replace,
            "find" => TextMethod::Find,
            "index" => TextMethod::Index,
            "count" => TextMethod::Count,
            "startswith" => TextMethod::StartsWith,
            "endswith" => TextMethod::EndsWith,
            "join" => TextMethod::Join,
            _ => {
                self.error(member.member.span, format!("str has no method '{name}'"));
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            }
        };
        let flexible = matches!(
            method,
            TextMethod::Pop
                | TextMethod::Split
                | TextMethod::RSplit
                | TextMethod::Strip
                | TextMethod::LStrip
                | TextMethod::RStrip
        );
        let expected = match method {
            TextMethod::Length
            | TextMethod::Clear
            | TextMethod::Reverse
            | TextMethod::Copy
            | TextMethod::Lower
            | TextMethod::Upper => 0,
            TextMethod::Get
            | TextMethod::Push
            | TextMethod::Append
            | TextMethod::Remove
            | TextMethod::Extend
            | TextMethod::Find
            | TextMethod::Index
            | TextMethod::Count
            | TextMethod::StartsWith
            | TextMethod::EndsWith
            | TextMethod::Join => 1,
            TextMethod::Set | TextMethod::Insert | TextMethod::Replace => 2,
            TextMethod::Pop => arguments.len().min(1),
            TextMethod::Split | TextMethod::RSplit => arguments.len().min(2),
            TextMethod::Strip | TextMethod::LStrip | TextMethod::RStrip => arguments.len().min(1),
        };
        if flexible {
            let ok = match method {
                TextMethod::Pop => arguments.len() <= 1,
                TextMethod::Strip | TextMethod::LStrip | TextMethod::RStrip => arguments.len() <= 1,
                TextMethod::Split | TextMethod::RSplit => arguments.len() <= 2,
                _ => true,
            };
            if !ok {
                self.error(
                    span,
                    format!(
                        "method '{name}' expects fewer arguments, found {}",
                        arguments.len()
                    ),
                );
            }
        } else if arguments.len() != expected {
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
        for (index, argument) in arguments.iter().enumerate() {
            let wanted = match (method, index) {
                (TextMethod::Get | TextMethod::Remove | TextMethod::Pop, 0) => None,
                (TextMethod::Set | TextMethod::Insert, 0) => None,
                (TextMethod::Push | TextMethod::Append, 0)
                | (TextMethod::Set | TextMethod::Insert, 1) => Some(CanonicalType::Chr),
                (
                    TextMethod::Extend
                    | TextMethod::Find
                    | TextMethod::Index
                    | TextMethod::Count
                    | TextMethod::StartsWith
                    | TextMethod::EndsWith
                    | TextMethod::Replace,
                    _,
                ) => Some(CanonicalType::Str),
                (TextMethod::Split | TextMethod::RSplit, 0)
                | (TextMethod::Strip | TextMethod::LStrip | TextMethod::RStrip, 0) => {
                    Some(CanonicalType::Str)
                }
                (TextMethod::Split | TextMethod::RSplit, 1) => None,
                (TextMethod::Join, 0) => Some(CanonicalType::Str.nest()),
                _ => None,
            };
            let value = self.check_expr(argument, wanted, false);
            let index_argument = wanted.is_none()
                && index == 0
                && matches!(
                    method,
                    TextMethod::Get
                        | TextMethod::Set
                        | TextMethod::Insert
                        | TextMethod::Remove
                        | TextMethod::Pop
                );
            let maxsplit = matches!(method, TextMethod::Split | TextMethod::RSplit) && index == 1;
            if (index_argument || maxsplit) && !value.ty().is_integer() {
                self.error(
                    argument.span(),
                    format!(
                        "an index must be an integer, found '{}'",
                        self.spell(value.ty())
                    ),
                );
            }
            let value = if let Some(wanted) = wanted {
                self.apply_conversion(value, wanted, false, argument.span())
            } else {
                value
            };
            args.push(value);
        }
        let ty = match method {
            TextMethod::Length => CanonicalType::UInt64,
            TextMethod::Get | TextMethod::Pop => CanonicalType::Chr,
            TextMethod::Find | TextMethod::Index | TextMethod::Count => CanonicalType::Int64,
            TextMethod::StartsWith | TextMethod::EndsWith => CanonicalType::Bln,
            TextMethod::Split | TextMethod::RSplit => CanonicalType::Str.nest(),
            TextMethod::Join
            | TextMethod::Strip
            | TextMethod::LStrip
            | TextMethod::RStrip
            | TextMethod::Lower
            | TextMethod::Upper
            | TextMethod::Replace
            | TextMethod::Copy => CanonicalType::Str,
            TextMethod::Set
            | TextMethod::Push
            | TextMethod::Append
            | TextMethod::Extend
            | TextMethod::Insert
            | TextMethod::Remove
            | TextMethod::Clear
            | TextMethod::Reverse => CanonicalType::Void,
        };
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::TextCall {
            place,
            object: object.map(Box::new),
            method,
            args,
            ty,
            file,
            line,
            column,
            end_column,
        }
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
            "append" => ArrayMethod::Append,
            "extend" => ArrayMethod::Extend,
            "insert" => ArrayMethod::Insert,
            "remove" => ArrayMethod::Remove,
            "pop" => ArrayMethod::Pop,
            "clear" => ArrayMethod::Clear,
            "index" => ArrayMethod::IndexOf,
            "count" => ArrayMethod::Count,
            "reverse" => ArrayMethod::Reverse,
            "sort" => ArrayMethod::Sort,
            "copy" => ArrayMethod::Copy,
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
        let element = array_ty.element().unwrap_or(CanonicalType::Int64);
        if method == ArrayMethod::Sort && !self.can_order(element) {
            self.error(
                span,
                format!("cannot compare '{}' values", self.spell(element)),
            );
        }
        if matches!(method, ArrayMethod::IndexOf | ArrayMethod::Count) && !self.can_equate(element)
        {
            self.error(
                span,
                format!("cannot compare '{}' values", self.spell(element)),
            );
        }
        let expected_args = match method {
            ArrayMethod::Length
            | ArrayMethod::Clear
            | ArrayMethod::Reverse
            | ArrayMethod::Sort
            | ArrayMethod::Copy => 0,
            ArrayMethod::Get
            | ArrayMethod::Push
            | ArrayMethod::Append
            | ArrayMethod::Extend
            | ArrayMethod::Remove
            | ArrayMethod::IndexOf
            | ArrayMethod::Count => 1,
            ArrayMethod::Set | ArrayMethod::Insert => 2,
            ArrayMethod::Pop => arguments.len().min(1),
        };
        if method == ArrayMethod::Pop && arguments.len() > 1 {
            self.error(
                span,
                format!(
                    "method 'pop' expects 0 or 1 argument, found {}",
                    arguments.len()
                ),
            );
        } else if arguments.len() != expected_args {
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
            element,
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
            ArrayMethod::Get | ArrayMethod::Pop => {
                array_ty.element().unwrap_or(CanonicalType::Int64)
            }
            ArrayMethod::Length => CanonicalType::UInt64,
            ArrayMethod::IndexOf | ArrayMethod::Count => CanonicalType::Int64,
            ArrayMethod::Copy => array_ty,
            ArrayMethod::Set
            | ArrayMethod::Push
            | ArrayMethod::Append
            | ArrayMethod::Extend
            | ArrayMethod::Insert
            | ArrayMethod::Remove
            | ArrayMethod::Clear
            | ArrayMethod::Reverse
            | ArrayMethod::Sort => CanonicalType::Void,
        }
    }

    fn can_equate(&self, ty: CanonicalType) -> bool {
        self.can_order(ty) || ty.is_class()
    }

    fn can_order(&self, ty: CanonicalType) -> bool {
        ty.is_numeric()
            || ty == CanonicalType::Bln
            || ty == CanonicalType::Chr
            || ty == CanonicalType::Str
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
                (ArrayMethod::Get | ArrayMethod::Remove | ArrayMethod::Pop, 0) => None,
                (ArrayMethod::Push | ArrayMethod::Append, 0) => Some(element),
                (ArrayMethod::Extend, 0) => Some(array_ty),
                (ArrayMethod::IndexOf | ArrayMethod::Count, 0) => Some(element),
                (ArrayMethod::Set, 0) | (ArrayMethod::Insert, 0) => None,
                (ArrayMethod::Set, 1) | (ArrayMethod::Insert, 1) => Some(element),
                _ => None,
            };
            let value = self.check_expr(argument, expected, false);
            let index_argument = matches!(
                method,
                ArrayMethod::Get
                    | ArrayMethod::Remove
                    | ArrayMethod::Pop
                    | ArrayMethod::Set
                    | ArrayMethod::Insert
            ) && index == 0;
            if index_argument {
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
            let auto = binding.conversion == ConversionMode::Auto;
            let ty = self.resolve_type(&binding.ty);
            if ty == CanonicalType::Void || ty.is_void_array() {
                self.error(binding.ty.span, "void is not a valid array element type");
            }
            if ty != element && !(auto && self.auto_converts(element, ty)) {
                if auto {
                    self.error(
                        binding.ty.span,
                        format!(
                            "cannot convert '{}' to '{}'",
                            self.spell(element),
                            self.spell(ty)
                        ),
                    );
                } else {
                    self.error(
                        binding.ty.span,
                        format!(
                            "type mismatch: expected '{}', found '{}'",
                            self.spell(element),
                            self.spell(ty)
                        ),
                    );
                }
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
            bindings.push((binding.name.name.clone(), ty, binding.mutable, auto));
        }
        for (name, ty, mutable, auto) in &bindings {
            self.scopes.last_mut().unwrap().insert(
                name.clone(),
                Binding {
                    ty: *ty,
                    mutable: *mutable,
                    auto: *auto,
                    ready: true,
                moved: false,
                },
            );
        }
        let (file, line, column, end_column) = self.located(decl.span);
        Some(CheckedStmt::Destructure {
            bindings: bindings
                .into_iter()
                .map(|(name, ty, mutable, _)| (name, ty, mutable))
                .collect(),
            value,
            file,
            line,
            column,
            end_column,
        })
    }

    fn auto_converts(&self, from: CanonicalType, to: CanonicalType) -> bool {
        if from == to {
            return true;
        }
        if let Some(inner) = to.optional_inner() {
            if from == inner || conversion(from, inner).is_some() {
                return true;
            }
        }
        conversion(from, to).is_some()
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
        let lock_target = match &source {
            CheckedExpr::MakeIter { object, .. } => object.as_ref(),
            other => other,
        };
        let lock = if let CheckedExpr::Load { name, .. } = lock_target {
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
                moved: false,
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
        // Specialized multi-binding helpers remain available via method calls.
        if let Expr::Call(call) = unwrap_parens(expr) {
            if let Expr::Member(member) = &*call.callee {
                let name = member.member.name.as_str();
                if matches!(name, "keys" | "values" | "entries") {
                    if !call.arguments.is_empty() {
                        self.error(
                            call.span,
                            format!(
                                "method '{name}' expects 0 arguments, found {}",
                                call.arguments.len()
                            ),
                        );
                    }
                    return self.map_loop_source(member, name, call.span);
                }
                if name == "enumerate" {
                    if !call.arguments.is_empty() {
                        self.error(
                            call.span,
                            format!(
                                "method 'enumerate' expects 0 arguments, found {}",
                                call.arguments.len()
                            ),
                        );
                    }
                    let object = self.check_expr(&member.object, None, false);
                    if !object.ty().is_array() {
                        self.error(
                            member.object.span(),
                            format!("'{}' is not an array", self.spell(object.ty())),
                        );
                    }
                    let element = object.ty().element().unwrap_or(CanonicalType::Int64);
                    return (object, LoopSourceKind::Enumerate, element, element);
                }
            }
        }
        let source = self.check_expr(expr, None, false);
        if let Some(element) = self.iter_element(source.ty()) {
            // Already an iterator — do not call .iter() again.
            return (source, LoopSourceKind::Values, element, element);
        }
        if source.ty().is_array() {
            let element = source.ty().element().unwrap_or(CanonicalType::Int64);
            let ty = CanonicalType::Iter(self.intern_iter(element));
            return (
                CheckedExpr::MakeIter {
                    object: Box::new(source),
                    ty,
                },
                LoopSourceKind::Values,
                element,
                element,
            );
        }
        if source.ty() == CanonicalType::Str {
            let element = CanonicalType::Chr;
            let ty = CanonicalType::Iter(self.intern_iter(element));
            return (
                CheckedExpr::MakeIter {
                    object: Box::new(source),
                    ty,
                },
                LoopSourceKind::Values,
                element,
                element,
            );
        }
        if let Some((id, _)) = self.generic_class(source.ty()) {
            if let Some(method) = self.classes[id.0 as usize]
                .methods
                .iter()
                .position(|method| method.name == "iter" && !method.is_static)
            {
                let ret = self.classes[id.0 as usize].methods[method].ret;
                if let Some(element) = self.iter_element(ret) {
                    let (file, line, column, end_column) = self.located(expr.span());
                    return (
                        CheckedExpr::ClassCall {
                            class: id,
                            method,
                            object: Some(Box::new(source)),
                            args: Vec::new(),
                            type_args: Vec::new(),
                            ty: ret,
                            direct: false,
                            file,
                            line,
                            column,
                            end_column,
                        },
                        LoopSourceKind::Values,
                        element,
                        element,
                    );
                }
                self.error(
                    expr.span(),
                    format!("iter() must return iter<T>, found '{}'", self.spell(ret)),
                );
                return (
                    source,
                    LoopSourceKind::Values,
                    CanonicalType::Int64,
                    CanonicalType::Int64,
                );
            }
        }
        self.error(
            expr.span(),
            format!(
                "'{}' is not iterable; expected an array, str, iter<T>, generator, or type with iter()",
                self.spell(source.ty())
            ),
        );
        fallback
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
        if let Some(scope) = self.binding_scope(&ident.name) {
            if self.reject_capture(&ident.name, scope, ident.span) {
                return self.dummy(None);
            }
        } else if self.lambda_var.as_deref() == Some(ident.name.as_str()) {
            self.error(
                ident.span,
                format!("lambda cannot capture '{}'", ident.name),
            );
            return self.dummy(None);
        } else if let Some(signature) = self.functions.get(&ident.name).cloned() {
            if !signature.type_params.is_empty() {
                self.error(
                    ident.span,
                    format!(
                        "generic function '{}' cannot be used as a value",
                        ident.name
                    ),
                );
                return self.dummy(None);
            }
            let id = self.intern_callable(&signature.params, signature.ret);
            return CheckedExpr::FunctionRef {
                name: ident.name.clone(),
                ty: CanonicalType::Callable(id),
            };
        }
        match self.lookup(&ident.name) {
            Some(binding) => {
                if !binding.ready {
                    self.error(
                        ident.span,
                        format!("variable '{}' is used before it is assigned", ident.name),
                    );
                }
                if binding.moved && binding.ty.is_async() {
                    self.error(
                        ident.span,
                        format!("use of moved async operation '{}'", ident.name),
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
                } else if self.class_names.contains_key(&ident.name)
                    || self.interface_names.contains_key(&ident.name)
                {
                    self.error(
                        ident.span,
                        format!("construct '{}' with new or a call", ident.name),
                    );
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

    fn interface_name(&self, id: InterfaceId) -> String {
        self.interfaces
            .get(id.0 as usize)
            .map(|interface| interface.name.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn inherits(&self, child: ClassId, ancestor: ClassId) -> bool {
        let mut current = self
            .classes
            .get(child.0 as usize)
            .and_then(|class| class.parent);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self
                .classes
                .get(id.0 as usize)
                .and_then(|class| class.parent);
        }
        false
    }

    fn class_implements(&self, class: ClassId, iface: InterfaceId) -> bool {
        let mut current = Some(class);
        while let Some(id) = current {
            if self
                .classes
                .get(id.0 as usize)
                .is_some_and(|info| info.implements.contains(&iface))
            {
                return true;
            }
            current = self.classes.get(id.0 as usize).and_then(|info| info.parent);
        }
        false
    }

    fn assignable(&self, from: CanonicalType, to: CanonicalType) -> bool {
        match (from, to) {
            (CanonicalType::Class(child), CanonicalType::Class(parent)) => {
                child == parent || self.inherits(child, parent)
            }
            (CanonicalType::Class(class), CanonicalType::Interface(iface)) => {
                self.class_implements(class, iface)
            }
            _ => false,
        }
    }

    fn member_visible(&self, public: bool, protected: bool, owner: ClassId) -> bool {
        if public {
            return true;
        }
        let Some(current) = self.current_class else {
            return false;
        };
        current == owner || (protected && self.inherits(current, owner))
    }

    fn locate_field(&self, start: ClassId, name: &str) -> Option<(ClassId, usize)> {
        let mut current = Some(start);
        while let Some(id) = current {
            if let Some(index) = self.classes[id.0 as usize]
                .fields
                .iter()
                .position(|field| field.name == name)
            {
                return Some((id, index));
            }
            current = self.classes[id.0 as usize].parent;
        }
        None
    }

    fn has_method_name(&self, start: ClassId, name: &str) -> bool {
        let mut current = Some(start);
        while let Some(id) = current {
            if self.classes[id.0 as usize]
                .methods
                .iter()
                .any(|method| method.name == name)
            {
                return true;
            }
            current = self.classes[id.0 as usize].parent;
        }
        false
    }

    fn note_deprecated(&mut self, deprecated: bool, span: Span, name: &str) {
        if deprecated {
            self.warn(span, format!("'{name}' is deprecated"));
        }
    }

    fn collect_methods(
        &self,
        start: ClassId,
        name: &str,
        want_static: bool,
    ) -> Vec<(ClassId, usize)> {
        let mut found = Vec::new();
        let mut current = Some(start);
        while let Some(id) = current {
            for (index, method) in self.classes[id.0 as usize].methods.iter().enumerate() {
                if method.name != name || method.is_static != want_static {
                    continue;
                }
                let hidden = found.iter().any(|(owner, other): &(ClassId, usize)| {
                    self.same_params(
                        &self.classes[owner.0 as usize].methods[*other].params,
                        &method.params,
                    )
                });
                if !hidden {
                    found.push((id, index));
                }
            }
            current = self.classes[id.0 as usize].parent;
        }
        found
    }

    fn adaptable(&self, from: CanonicalType, to: CanonicalType, auto: bool) -> bool {
        if from == to || self.assignable(from, to) {
            return true;
        }
        if let Some(inner) = to.optional_inner() {
            if from == inner || self.assignable(from, inner) {
                return true;
            }
            if auto && crate::semantic::convert::conversion(from, inner).is_some() {
                return true;
            }
        }
        auto && crate::semantic::convert::conversion(from, to).is_some()
    }

    fn resolve_methods(
        &mut self,
        start: ClassId,
        name: &str,
        want_static: bool,
        arguments: &[Expr],
        span: Span,
    ) -> Option<(ClassId, usize, Vec<CheckedExpr>)> {
        let found = self.collect_methods(start, name, want_static);
        if found.is_empty() {
            return None;
        }
        if found.len() == 1 {
            let (class, index) = found[0];
            let params = self.classes[class.0 as usize].methods[index].params.clone();
            let deprecated = self.classes[class.0 as usize].methods[index].deprecated;
            self.note_deprecated(deprecated, span, name);
            let args = self.check_call_args(arguments, &params, span);
            return Some((class, index, args));
        }
        let prelim = arguments
            .iter()
            .map(|argument| self.check_expr(argument, None, false))
            .collect::<Vec<_>>();
        let matches = found
            .iter()
            .copied()
            .filter(|(class, index)| {
                let params = &self.classes[class.0 as usize].methods[*index].params;
                let min_args = params
                    .iter()
                    .position(|param| param.default.is_some())
                    .unwrap_or(params.len());
                prelim.len() >= min_args
                    && prelim.len() <= params.len()
                    && params.iter().zip(&prelim).all(|(param, argument)| {
                        self.adaptable(argument.ty(), param.ty, param.auto)
                    })
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            self.error(
                span,
                format!("no overload of '{name}' matches the arguments"),
            );
            return None;
        }
        if matches.len() > 1 {
            self.error(span, format!("ambiguous call to '{name}'"));
            return None;
        }
        let (class, index) = matches[0];
        let params = self.classes[class.0 as usize].methods[index].params.clone();
        let deprecated = self.classes[class.0 as usize].methods[index].deprecated;
        self.note_deprecated(deprecated, span, name);
        let mut args: Vec<CheckedExpr> = prelim
            .into_iter()
            .zip(arguments.iter().zip(params.iter()))
            .map(|(argument, (source, param))| {
                self.apply_conversion(argument, param.ty, param.auto, source.span())
            })
            .collect();
        for param in params.iter().skip(args.len()) {
            if let Some(default) = &param.default {
                let value = self.check_expr(default, Some(param.ty), false);
                args.push(self.apply_conversion(value, param.ty, param.auto, default.span()));
            }
        }
        Some((class, index, args))
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
            if self.interface_names.contains_key(&declaration.name.name) {
                self.error(
                    declaration.name.span,
                    format!("'{}' is already declared", declaration.name.name),
                );
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
                    protected: field.protected,
                    is_static: field.is_static,
                    deprecated: field.deprecated,
                    span: field.span,
                });
            }
            let type_params = self.register_type_params(&declaration.type_params);
            let mut methods = Vec::new();
            for method in &declaration.methods {
                let _ = self.reject_reserved_name(&method.name.name, method.name.span);
                if seen.contains_key(&method.name.name)
                    && fields.iter().any(|field| field.name == method.name.name)
                {
                    self.error(
                        method.name.span,
                        format!("duplicate member '{}'", method.name.name),
                    );
                }
                seen.insert(method.name.name.clone(), method.name.span);
                let method_params = self.register_type_params(&method.type_params);
                for method_param in &method_params {
                    let method_name = self.type_params[method_param.0 as usize].name.clone();
                    if type_params.iter().any(|class_param| {
                        self.type_params[class_param.0 as usize].name == method_name
                    }) {
                        self.error(
                            method.name.span,
                            format!(
                                "method type parameter '{method_name}' shadows a class type parameter"
                            ),
                        );
                    }
                }
                methods.push(ClassMethodInfo {
                    name: method.name.name.clone(),
                    params: Vec::new(),
                    param_syntax: self.raw_params(&method.parameters),
                    ret: CanonicalType::Void,
                    ret_syntax: method.return_type.clone(),
                    public: method.public,
                    protected: method.protected,
                    is_static: method.is_static,
                    is_override: method.is_override,
                    is_abstract: method.is_abstract,
                    is_final: method.is_final,
                    is_virtual: false,
                    deprecated: method.deprecated,
                    mutating: false,
                    leaks: false,
                    span: method.span,
                    type_params: method_params,
                });
            }
            let init = declaration.init.as_ref().map(|init| ClassInitInfo {
                params: Vec::new(),
                param_syntax: self.raw_params(&init.parameters),
                public: init.public,
                protected: init.protected,
                deprecated: init.deprecated,
            });
            if !type_params.is_empty() {
                if declaration.parent.is_some() {
                    self.error(declaration.span, "a generic class cannot use extends");
                }
                if !declaration.interfaces.is_empty() {
                    self.error(declaration.span, "a generic class cannot use implements");
                }
            }
            let id = ClassId(self.classes.len() as u32);
            self.class_names.insert(declaration.name.name.clone(), id);
            self.classes.push(ClassInfo {
                name: declaration.name.name.clone(),
                span: declaration.span,
                parent: None,
                parent_name: declaration
                    .parent
                    .as_ref()
                    .map(|name| (name.name.clone(), name.span)),
                interface_names: declaration
                    .interfaces
                    .iter()
                    .map(|name| (name.name.clone(), name.span))
                    .collect(),
                implements: Vec::new(),
                abstract_class: declaration.abstract_class,
                final_class: declaration.final_class,
                fields,
                methods,
                init,
                type_params,
            });
        }
    }

    fn collect_interfaces(&mut self, program: &Program) {
        for declaration in &program.interfaces {
            if self.class_names.contains_key(&declaration.name.name)
                || self.interface_names.contains_key(&declaration.name.name)
            {
                self.error(
                    declaration.name.span,
                    format!("'{}' is already declared", declaration.name.name),
                );
                continue;
            }
            let methods = declaration
                .methods
                .iter()
                .map(|method| IfaceMethodInfo {
                    name: method.name.name.clone(),
                    params: Vec::new(),
                    param_syntax: self.raw_params(&method.parameters),
                    ret: CanonicalType::Void,
                    ret_syntax: method.return_type.clone(),
                    deprecated: method.deprecated,
                    span: method.span,
                })
                .collect();
            let id = InterfaceId(self.interfaces.len() as u32);
            self.interface_names
                .insert(declaration.name.name.clone(), id);
            self.interfaces.push(InterfaceInfo {
                name: declaration.name.name.clone(),
                methods,
                span: declaration.span,
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
                name_span: parameter.name.span,
                callable: parameter.callable.clone(),
                default: parameter.default.clone(),
            })
            .collect()
    }

    fn resolve_parameters(&mut self, parameters: &[Parameter]) -> Vec<ParamInfo> {
        let raw = self.raw_params(parameters);
        self.resolve_raw_params(&raw)
    }

    fn resolve_raw_params(&mut self, raw: &[RawParam]) -> Vec<ParamInfo> {
        let mut names = HashMap::new();
        let mut params = Vec::new();
        let mut saw_default = false;
        for parameter in raw {
            let ty = if let Some(nested) = &parameter.callable {
                let nested_params = self.resolve_parameters(nested);
                let ret = self.resolve_type(&parameter.syntax);
                if ret.is_void_array() {
                    self.error(
                        parameter.syntax.span,
                        "void is not a valid array element type",
                    );
                }
                let _ = self.reject_unspecified_type(ret, parameter.syntax.span);
                CanonicalType::Callable(self.intern_callable(&nested_params, ret))
            } else {
                let ty = self.resolve_type(&parameter.syntax);
                if ty == CanonicalType::Void {
                    self.error(parameter.syntax.span, "a parameter cannot have type void");
                }
                if ty.is_void_array() {
                    self.error(
                        parameter.syntax.span,
                        "void is not a valid array element type",
                    );
                }
                let _ = self.reject_unspecified_type(ty, parameter.syntax.span);
                ty
            };
            if names
                .insert(parameter.name.clone(), parameter.span)
                .is_some()
            {
                self.error(
                    parameter.name_span,
                    format!("duplicate parameter '{}'", parameter.name),
                );
            }
            let _ = self.reject_reserved_name(&parameter.name, parameter.name_span);
            if parameter.default.is_some() {
                saw_default = true;
                if Self::expr_is_null(parameter.default.as_ref()) && !ty.is_optional() {
                    self.error(
                        parameter.span,
                        format!(
                            "null defaults require a nullable parameter type, found '{}'",
                            self.spell(ty)
                        ),
                    );
                }
            } else if saw_default {
                self.error(
                    parameter.name_span,
                    "a required parameter cannot follow a parameter with a default",
                );
            }
            params.push(ParamInfo {
                name: parameter.name.clone(),
                ty,
                auto: parameter.auto,
                default: parameter.default.clone(),
            });
        }
        params
    }

    fn expr_is_null(expr: Option<&Expr>) -> bool {
        matches!(
            expr,
            Some(Expr::Literal(crate::parser::LiteralExpr::Null { .. }))
        )
    }

    fn intern_callable(&mut self, params: &[ParamInfo], ret: CanonicalType) -> CallableId {
        let shape_params: Vec<CanonicalType> = params.iter().map(|param| param.ty).collect();
        let shape_autos: Vec<bool> = params.iter().map(|param| param.auto).collect();
        if let Some(index) = self.callables.iter().position(|sig| {
            sig.params == shape_params && sig.autos == shape_autos && sig.ret == ret
        }) {
            return CallableId(index as u32);
        }
        let id = CallableId(self.callables.len() as u32);
        self.callables.push(CallableSignature {
            params: shape_params,
            autos: shape_autos,
            ret,
        });
        id
    }

    fn callable_compatible(&self, from: CallableId, to: CallableId) -> bool {
        let Some(left) = self.callables.get(from.0 as usize) else {
            return false;
        };
        let Some(right) = self.callables.get(to.0 as usize) else {
            return false;
        };
        left.params == right.params && left.ret == right.ret
    }

    fn spell_callable(&self, id: CallableId) -> String {
        let Some(sig) = self.callables.get(id.0 as usize) else {
            return "callable".to_string();
        };
        let params = sig
            .params
            .iter()
            .map(|ty| self.spell(*ty))
            .collect::<Vec<_>>()
            .join(", ");
        format!("({}) -> {}", params, self.spell(sig.ret))
    }

    fn finish_class_types(&mut self) {
        let count = self.classes.len();
        for index in 0..count {
            let class_params = self.classes[index].type_params.clone();
            self.push_type_params(&class_params);
            let fields = self.classes[index]
                .fields
                .iter()
                .map(|field| {
                    (
                        field.syntax.clone(),
                        field.span,
                        field.is_static,
                        field.name.clone(),
                    )
                })
                .collect::<Vec<_>>();
            for (field_index, (syntax, span, is_static, name)) in fields.into_iter().enumerate() {
                let ty = self.resolve_type(&syntax);
                if ty == CanonicalType::Void {
                    self.error(span, "a field cannot have type void");
                }
                if ty.is_void_array() {
                    self.error(span, "void is not a valid array element type");
                }
                if is_static && self.uses_class_params(ty, &class_params) {
                    self.error(
                        span,
                        format!("static field '{name}' cannot use a class type parameter"),
                    );
                }
                self.classes[index].fields[field_index].ty = ty;
            }
            let method_count = self.classes[index].methods.len();
            for method_index in 0..method_count {
                let method_params = self.classes[index].methods[method_index]
                    .type_params
                    .clone();
                self.push_type_params(&method_params);
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
                self.pop_type_params();
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
            self.pop_type_params();
        }
        let interface_count = self.interfaces.len();
        for index in 0..interface_count {
            let method_count = self.interfaces[index].methods.len();
            for method_index in 0..method_count {
                let syntax = self.interfaces[index].methods[method_index]
                    .ret_syntax
                    .clone();
                let span = self.interfaces[index].methods[method_index].span;
                let ret = self.resolve_type(&syntax);
                let raw = self.interfaces[index].methods[method_index]
                    .param_syntax
                    .clone();
                let params = self.resolve_raw_params(&raw);
                self.interfaces[index].methods[method_index].ret = ret;
                self.interfaces[index].methods[method_index].params = params;
                let _ = span;
            }
        }
    }

    fn link_classes(&mut self) {
        let count = self.classes.len();
        for index in 0..count {
            let Some((name, span)) = self.classes[index].parent_name.clone() else {
                continue;
            };
            let Some(parent) = self.class_names.get(&name).copied() else {
                self.error(span, format!("unknown class '{name}'"));
                continue;
            };
            if parent.0 as usize == index {
                self.error(span, format!("class '{name}' cannot extend itself"));
                continue;
            }
            self.classes[index].parent = Some(parent);
        }
        for index in 0..count {
            let mut seen = Vec::new();
            let mut current = Some(ClassId(index as u32));
            while let Some(id) = current {
                if seen.contains(&id) {
                    self.error(
                        self.classes[index].span,
                        format!("inheritance cycle involving '{}'", self.classes[index].name),
                    );
                    self.classes[index].parent = None;
                    break;
                }
                seen.push(id);
                current = self.classes[id.0 as usize].parent;
            }
        }
        for index in 0..count {
            let Some(parent) = self.classes[index].parent else {
                continue;
            };
            if self.classes[parent.0 as usize].final_class {
                let parent_name = self.classes[parent.0 as usize].name.clone();
                self.error(
                    self.classes[index].span,
                    format!("cannot extend final class '{parent_name}'"),
                );
            }
            let own = self.classes[index]
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.span))
                .collect::<Vec<_>>();
            let mut ancestor = Some(parent);
            while let Some(id) = ancestor {
                for (name, span) in &own {
                    if self.classes[id.0 as usize]
                        .fields
                        .iter()
                        .any(|field| field.name == *name)
                    {
                        self.error(span.clone(), format!("duplicate member '{name}'"));
                    }
                }
                ancestor = self.classes[id.0 as usize].parent;
            }
        }
        for index in 0..count {
            let names = self.classes[index].interface_names.clone();
            let mut implements = Vec::new();
            for (name, span) in names {
                let Some(id) = self.interface_names.get(&name).copied() else {
                    self.error(span, format!("unknown interface '{name}'"));
                    continue;
                };
                if implements.contains(&id) {
                    self.error(span, format!("interface '{name}' is implemented twice"));
                    continue;
                }
                implements.push(id);
            }
            self.classes[index].implements = implements;
        }
    }

    fn check_inheritance(&mut self) {
        let count = self.classes.len();
        for index in 0..count {
            let method_count = self.classes[index].methods.len();
            for left in 0..method_count {
                for right in (left + 1)..method_count {
                    let same_name = self.classes[index].methods[left].name
                        == self.classes[index].methods[right].name;
                    let same_sig = self.same_params(
                        &self.classes[index].methods[left].params,
                        &self.classes[index].methods[right].params,
                    );
                    if same_name && same_sig {
                        let name = self.classes[index].methods[right].name.clone();
                        let span = self.classes[index].methods[right].span;
                        self.error(span, format!("duplicate method '{name}'"));
                    }
                }
            }
        }
        for index in 0..count {
            let id = ClassId(index as u32);
            let method_count = self.classes[index].methods.len();
            for method_index in 0..method_count {
                let is_static = self.classes[index].methods[method_index].is_static;
                let is_override = self.classes[index].methods[method_index].is_override;
                let is_abstract = self.classes[index].methods[method_index].is_abstract;
                let is_final = self.classes[index].methods[method_index].is_final;
                let public = self.classes[index].methods[method_index].public;
                let protected = self.classes[index].methods[method_index].protected;
                let name = self.classes[index].methods[method_index].name.clone();
                let span = self.classes[index].methods[method_index].span;
                let ret = self.classes[index].methods[method_index].ret;
                let params = self.classes[index].methods[method_index].params.clone();
                if is_static && is_override {
                    self.error(span, format!("@static and @override cannot be combined"));
                }
                if is_abstract && is_static {
                    self.error(span, "a static method cannot be abstract");
                }
                if is_abstract && is_final {
                    self.error(span, "an abstract method cannot be final");
                }
                if is_abstract && !public && !protected {
                    self.error(span, format!("abstract method '{name}' cannot be private"));
                }
                if is_abstract {
                    self.classes[index].methods[method_index].is_virtual = true;
                }
                if is_abstract && !self.classes[index].abstract_class {
                    let class_name = self.classes[index].name.clone();
                    self.error(
                        span,
                        format!(
                            "concrete class '{class_name}' cannot declare abstract method '{name}'"
                        ),
                    );
                }
                let parent_method = self.classes[index]
                    .parent
                    .and_then(|parent| self.find_method(parent, &name, &params, is_static));
                let interface_method = !is_static
                    && self.matches_interface_method(ClassId(index as u32), &name, &params, ret);
                if is_override {
                    if let Some((owner, other)) = parent_method {
                        if self.classes[owner.0 as usize].methods[other].is_final {
                            self.error(span, format!("final method '{name}' cannot be overridden"));
                        }
                        let parent_ret = self.classes[owner.0 as usize].methods[other].ret;
                        if parent_ret != ret {
                            self.error(
                                span,
                                format!(
                                    "override of '{name}' must return '{}'",
                                    self.spell(parent_ret)
                                ),
                            );
                        }
                        self.classes[index].methods[method_index].is_virtual = true;
                        self.classes[owner.0 as usize].methods[other].is_virtual = true;
                        self.mark_override_chain(owner, other);
                    } else if !interface_method {
                        self.error(
                            span,
                            format!("method '{name}' does not override a parent method"),
                        );
                    }
                } else if !is_static && (parent_method.is_some() || interface_method) {
                    self.error(
                        span,
                        format!("method '{name}' overrides a parent method and needs @override"),
                    );
                }
            }
            if !self.classes[index].abstract_class {
                self.require_implementations(id);
            }
            self.require_interfaces(id);
        }
    }

    fn matches_interface_method(
        &self,
        class: ClassId,
        name: &str,
        params: &[ParamInfo],
        ret: CanonicalType,
    ) -> bool {
        let mut current = Some(class);
        while let Some(id) = current {
            for iface in &self.classes[id.0 as usize].implements {
                if self.interfaces[iface.0 as usize]
                    .methods
                    .iter()
                    .any(|method| {
                        method.name == name
                            && method.ret == ret
                            && method.params.len() == params.len()
                            && method
                                .params
                                .iter()
                                .zip(params)
                                .all(|(left, right)| left.ty == right.ty)
                    })
                {
                    return true;
                }
            }
            current = self.classes[id.0 as usize].parent;
        }
        false
    }

    fn same_params(&self, left: &[ParamInfo], right: &[ParamInfo]) -> bool {
        left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left, right)| left.ty == right.ty)
    }

    fn find_method(
        &self,
        start: ClassId,
        name: &str,
        params: &[ParamInfo],
        is_static: bool,
    ) -> Option<(ClassId, usize)> {
        let mut current = Some(start);
        while let Some(id) = current {
            if let Some(index) = self.classes[id.0 as usize]
                .methods
                .iter()
                .position(|method| {
                    method.name == name
                        && method.is_static == is_static
                        && self.same_params(&method.params, params)
                })
            {
                return Some((id, index));
            }
            current = self.classes[id.0 as usize].parent;
        }
        None
    }

    fn mark_override_chain(&mut self, mut class: ClassId, mut index: usize) {
        while let Some(parent) = self.classes[class.0 as usize].parent {
            let name = self.classes[class.0 as usize].methods[index].name.clone();
            let params = self.classes[class.0 as usize].methods[index].params.clone();
            let is_static = self.classes[class.0 as usize].methods[index].is_static;
            let Some((owner, other)) = self.find_method(parent, &name, &params, is_static) else {
                break;
            };
            self.classes[owner.0 as usize].methods[other].is_virtual = true;
            if !self.classes[owner.0 as usize].methods[other].is_override
                && !self.classes[owner.0 as usize].methods[other].is_abstract
            {
                break;
            }
            class = owner;
            index = other;
        }
    }

    fn require_implementations(&mut self, class: ClassId) {
        let mut current = Some(class);
        while let Some(id) = current {
            let abstracts = self.classes[id.0 as usize]
                .methods
                .iter()
                .filter(|method| method.is_abstract)
                .map(|method| (method.name.clone(), method.params.clone(), method.span))
                .collect::<Vec<_>>();
            for (name, params, span) in abstracts {
                if !self.has_concrete(class, &name, &params) {
                    let class_name = self.classes[class.0 as usize].name.clone();
                    self.error(
                        self.classes[class.0 as usize].span,
                        format!("class '{class_name}' must implement '{name}'"),
                    );
                    let _ = span;
                }
            }
            current = self.classes[id.0 as usize].parent;
        }
    }

    fn has_concrete(&self, class: ClassId, name: &str, params: &[ParamInfo]) -> bool {
        let mut current = Some(class);
        while let Some(id) = current {
            if self.classes[id.0 as usize].methods.iter().any(|method| {
                !method.is_static
                    && !method.is_abstract
                    && method.name == name
                    && self.same_params(&method.params, params)
            }) {
                return true;
            }
            current = self.classes[id.0 as usize].parent;
        }
        false
    }

    fn require_interfaces(&mut self, class: ClassId) {
        let concrete = !self.classes[class.0 as usize].abstract_class;
        let implements = self.classes[class.0 as usize].implements.clone();
        for iface in implements {
            let methods = self.interfaces[iface.0 as usize]
                .methods
                .iter()
                .map(|method| {
                    (
                        method.name.clone(),
                        method.params.clone(),
                        method.ret,
                        method.span,
                    )
                })
                .collect::<Vec<_>>();
            let iface_name = self.interfaces[iface.0 as usize].name.clone();
            for (name, params, ret, span) in methods {
                let found = self.find_method(class, &name, &params, false);
                let ok = found.is_some_and(|(owner, index)| {
                    let method = &self.classes[owner.0 as usize].methods[index];
                    method.ret == ret && method.public && (!concrete || !method.is_abstract)
                });
                if !ok {
                    let class_name = self.classes[class.0 as usize].name.clone();
                    self.error(
                        self.classes[class.0 as usize].span,
                        format!("class '{class_name}' must implement '{iface_name}.{name}'"),
                    );
                    let _ = span;
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
        let class_params = self.classes[id.0 as usize].type_params.clone();
        self.push_type_params(&class_params);
        let mut checked_fields = Vec::new();
        for (index, field) in declaration.fields.iter().enumerate() {
            let ty = self.classes[id.0 as usize].fields[index].ty;
            let auto = self.classes[id.0 as usize].fields[index].auto;
            let name = self.classes[id.0 as usize].fields[index].name.clone();
            let const_field = self.classes[id.0 as usize].fields[index].const_field;
            let initializer = if let Some(expr) = &field.initializer {
                let saved = self.allow_convert;
                self.allow_convert = auto;
                let value = self.check_expr(expr, Some(ty), false);
                self.allow_convert = saved;
                Some(self.apply_conversion(value, ty, auto, expr.span()))
            } else if !const_field {
                self.default_value(ty, &mut Vec::new())
            } else {
                None
            };
            let is_static = self.classes[id.0 as usize].fields[index].is_static;
            checked_fields.push(CheckedClassField {
                name,
                ty,
                const_field,
                is_static,
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
            self.current_class = Some(id);
            let (body, _, _) = self.check_member_body(
                id,
                &params,
                CanonicalType::Void,
                &init.body,
                init.span,
                "",
                false,
                false,
            );
            self.current_class = None;
            self.in_init = false;
            let saw_super = init
                .body
                .statements
                .iter()
                .any(|statement| matches!(statement, Stmt::Super(_)));
            let implicit_super = self.finish_super_requirement(id, init.span, saw_super);
            Some(CheckedInit {
                params: params
                    .into_iter()
                    .map(|param| (param.name, param.ty))
                    .collect(),
                body,
                implicit_super,
            })
        } else {
            if self.classes[id.0 as usize].parent.is_some() {
                let _ = self.finish_super_requirement(id, declaration.span, false);
            }
            None
        };
        let mut methods = Vec::new();
        for (index, method) in declaration.methods.iter().enumerate() {
            let params = self.classes[id.0 as usize].methods[index].params.clone();
            let ret = self.classes[id.0 as usize].methods[index].ret;
            let mutating = self.classes[id.0 as usize].methods[index].mutating;
            let is_static = self.classes[id.0 as usize].methods[index].is_static;
            let is_abstract = self.classes[id.0 as usize].methods[index].is_abstract;
            let is_virtual = self.classes[id.0 as usize].methods[index].is_virtual;
            let name = self.classes[id.0 as usize].methods[index].name.clone();
            let method_params = self.classes[id.0 as usize].methods[index]
                .type_params
                .clone();
            let type_param_names = self.type_param_names(&method_params);
            let (body, is_generator, is_async_iter) = if is_abstract {
                (Vec::new(), false, false)
            } else {
                self.push_type_params(&method_params);
                self.current_class = Some(id);
                let result = self.check_member_body(
                    id,
                    &params,
                    ret,
                    &method.body,
                    method.span,
                    &name,
                    true,
                    is_static,
                );
                self.current_class = None;
                self.pop_type_params();
                result
            };
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
                is_abstract,
                is_virtual,
                type_params: type_param_names,
                is_generator,
                is_async_iter,
            });
        }
        self.pop_type_params();
        Some(CheckedClass {
            name: declaration.name.name.clone(),
            parent: self.classes[id.0 as usize].parent,
            implements: self.classes[id.0 as usize].implements.clone(),
            abstract_class: self.classes[id.0 as usize].abstract_class,
            fields: checked_fields,
            init,
            methods,
            type_params: self.type_param_names(&class_params),
        })
    }

    fn finish_super_requirement(&mut self, id: ClassId, span: Span, saw_super: bool) -> bool {
        let Some(parent) = self.classes[id.0 as usize].parent else {
            if saw_super {
                self.error(span, "super requires a parent class");
            }
            return false;
        };
        if saw_super {
            return false;
        }
        let needs_args = self.classes[parent.0 as usize]
            .init
            .as_ref()
            .is_some_and(|init| !init.params.is_empty());
        if needs_args {
            let parent_name = self.classes[parent.0 as usize].name.clone();
            self.error(
                span,
                format!("constructor must call super for '{parent_name}'"),
            );
            false
        } else {
            let public = self.classes[parent.0 as usize]
                .init
                .as_ref()
                .is_none_or(|init| init.public);
            let protected = self.classes[parent.0 as usize]
                .init
                .as_ref()
                .is_some_and(|init| init.protected);
            self.current_class = Some(id);
            if self.classes[parent.0 as usize].init.is_some()
                && !self.member_visible(public, protected, parent)
            {
                let parent_name = self.classes[parent.0 as usize].name.clone();
                self.error(
                    span,
                    format!("constructor of '{parent_name}' is not accessible"),
                );
            }
            self.current_class = None;
            true
        }
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
    ) -> (Vec<CheckedStmt>, bool, bool) {
        let saved_return = self.current_return;
        let saved_class = self.current_class;
        let saved_static = self.current_static;
        let saved_generator = self.generator_element;
        let saved_in_async = self.in_async;
        let saved_in_async_iter = self.in_async_iter;
        let has_emit = contains_emit(&body.statements);
        let has_await = contains_await(&body.statements);
        let is_async = ret.is_async();
        let is_async_iter = ret.is_async_iter();
        let is_generator = ret.is_iter() && has_emit;
        if has_emit && is_async {
            self.error(span, "emit is not allowed inside an async function");
        } else if has_emit && !ret.is_iter() && !is_async_iter {
            self.error(
                span,
                "emit is only allowed in functions that return iter<T> or asyncIter<T>",
            );
        }
        if has_await && is_generator {
            self.error(span, "await is not allowed inside a generator");
        } else if has_await && !is_async && !is_async_iter {
            self.error(
                span,
                "await can only be used inside an async or asyncIter function",
            );
        }
        let body_ret = if is_async {
            self.async_result(ret).unwrap_or(CanonicalType::Void)
        } else {
            ret
        };
        let path_ret = if is_generator || is_async_iter {
            CanonicalType::Void
        } else {
            body_ret
        };
        self.current_return = path_ret;
        self.current_class = Some(id);
        self.current_static = is_static;
        self.in_async = is_async;
        self.in_async_iter = is_async_iter;
        self.generator_element = if is_generator {
            self.iter_element(ret)
        } else if is_async_iter {
            self.async_iter_element(ret)
        } else {
            None
        };
        self.scopes.push(HashMap::new());
        if !is_static {
            self.scopes.last_mut().unwrap().insert(
                "this".to_string(),
                Binding {
                    ty: CanonicalType::Class(id),
                    mutable: false,
                    auto: false,
                    ready: true,
                    moved: false,
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
                        moved: false,
                    },
                );
            }
        }
        let mut checked = Vec::new();
        let mut super_slot = self.in_init;
        for statement in &body.statements {
            self.at_super_slot = super_slot;
            if let Some(stmt) = self.check_stmt(statement) {
                checked.push(stmt);
            }
            super_slot = false;
        }
        self.at_super_slot = false;
        if require_return
            && !is_generator
            && !is_async_iter
            && path_ret != CanonicalType::Void
            && !guaranteed_return(&body.statements)
        {
            self.error(
                span,
                format!("function '{name}' must return a value on every path"),
            );
        }
        self.scopes.pop();
        self.current_return = saved_return;
        self.current_class = saved_class;
        self.current_static = saved_static;
        self.generator_element = saved_generator;
        self.in_async = saved_in_async;
        self.in_async_iter = saved_in_async_iter;
        (checked, is_generator, is_async_iter)
    }

    fn check_new(&mut self, expr: &NewExpr) -> CheckedExpr {
        let Some(origin) = self.class_names.get(&expr.name.name).copied() else {
            self.error(
                expr.name.span,
                format!("unknown class '{}'", expr.name.name),
            );
            for argument in &expr.arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        };
        let result_ty = if !self.classes[origin.0 as usize].type_params.is_empty()
            || !expr.type_arguments.is_empty()
        {
            self.resolve_constructed_class(origin, &expr.type_arguments, expr.name.span)
        } else {
            CanonicalType::Class(origin)
        };
        if self.classes[origin.0 as usize].abstract_class {
            self.error(
                expr.name.span,
                format!("cannot construct abstract class '{}'", expr.name.name),
            );
        }
        let deprecated = self.classes[origin.0 as usize]
            .init
            .as_ref()
            .is_some_and(|init| init.deprecated);
        self.note_deprecated(deprecated, expr.name.span, &expr.name.name);
        self.check_construct(origin, &expr.arguments, expr.span, result_ty)
    }

    fn check_static_call(
        &mut self,
        member: &MemberExpr,
        type_arguments: &[TypeSyntax],
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
        let dummy = CheckedExpr::Null {
            ty: CanonicalType::Class(id),
        };
        let call = self.finish_class_call(dummy, id, member, type_arguments, arguments, span);
        match call {
            CheckedExpr::ClassCall {
                class,
                method,
                args,
                type_args,
                ty,
                direct,
                file,
                line,
                column,
                end_column,
                ..
            } => {
                if !self.classes[class.0 as usize].methods[method].is_static {
                    let name = member.member.name.clone();
                    let class_name = self.class_name(id);
                    self.error(
                        member.member.span,
                        format!("method '{name}' requires an instance of '{class_name}'"),
                    );
                    return self.dummy(None);
                }
                CheckedExpr::ClassCall {
                    class,
                    method,
                    object: None,
                    args,
                    type_args,
                    ty,
                    direct,
                    file,
                    line,
                    column,
                    end_column,
                }
            }
            other => other,
        }
    }

    fn check_construct(
        &mut self,
        id: ClassId,
        arguments: &[Expr],
        span: Span,
        result_ty: CanonicalType,
    ) -> CheckedExpr {
        let name = self.class_name(id);
        let map = match result_ty {
            CanonicalType::Generic(generic) => self.generic_map(generic),
            _ => Vec::new(),
        };
        let init = self.classes[id.0 as usize]
            .init
            .as_ref()
            .map(|init| (init.public, init.protected, init.params.clone()));
        let (file, line, column, end_column) = self.located(span);
        let args = if let Some((public, protected, params)) = init {
            if !self.member_visible(public, protected, id) {
                self.error(span, format!("constructor of '{name}' is not accessible"));
            }
            let infos = params
                .iter()
                .map(|param| ParamInfo {
                    name: param.name.clone(),
                    ty: self.substitute_type(param.ty, &map),
                    auto: param.auto,
                    default: param.default.clone(),
                })
                .collect::<Vec<_>>();
            self.check_call_args(arguments, &infos, span)
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
        let type_args = match result_ty {
            CanonicalType::Generic(generic) => self
                .generics
                .get(generic.0 as usize)
                .map(|instance| instance.args.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        CheckedExpr::ClassNew {
            class: id,
            args,
            type_args,
            ty: result_ty,
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
        let map = match object.ty() {
            CanonicalType::Generic(generic) => self.generic_map(generic),
            _ => Vec::new(),
        };
        if let Some((owner, index)) = self.locate_field(id, &expr.member.name) {
            let public = self.classes[owner.0 as usize].fields[index].public;
            let protected = self.classes[owner.0 as usize].fields[index].protected;
            let ty = self.substitute_type(self.classes[owner.0 as usize].fields[index].ty, &map);
            let name = self.classes[owner.0 as usize].fields[index].name.clone();
            let deprecated = self.classes[owner.0 as usize].fields[index].deprecated;
            if !self.member_visible(public, protected, owner) {
                self.error(
                    expr.member.span,
                    format!("field '{name}' is not accessible"),
                );
            }
            self.note_deprecated(deprecated, expr.member.span, &name);
            return CheckedExpr::ClassGet {
                object: Box::new(object),
                class: owner,
                field: index,
                ty,
            };
        }
        if self.has_method_name(id, &expr.member.name) {
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
        let map = match object.ty() {
            CanonicalType::Generic(generic) => self.generic_map(generic),
            _ => Vec::new(),
        };
        let Some((owner, index)) = self.locate_field(id, &member.member.name) else {
            if self.has_method_name(id, &member.member.name) {
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
        let public = self.classes[owner.0 as usize].fields[index].public;
        let protected = self.classes[owner.0 as usize].fields[index].protected;
        let const_field = self.classes[owner.0 as usize].fields[index].const_field;
        let is_static = self.classes[owner.0 as usize].fields[index].is_static;
        let ty = self.substitute_type(self.classes[owner.0 as usize].fields[index].ty, &map);
        let auto = self.classes[owner.0 as usize].fields[index].auto;
        let name = self.classes[owner.0 as usize].fields[index].name.clone();
        let deprecated = self.classes[owner.0 as usize].fields[index].deprecated;
        self.note_deprecated(deprecated, member.member.span, &name);
        if !self.member_visible(public, protected, owner) {
            self.error(
                member.member.span,
                format!("field '{name}' is not accessible"),
            );
        }
        if const_field && (is_static || !self.in_init) {
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
            class: owner,
            field: index,
            ty,
        };
        let value = self.store_value(op, current, value, ty, auto, value.span());
        let (file, line, column, end_column) = self.located(member.span);
        CheckedExpr::ClassSet {
            object: Box::new(object),
            class: owner,
            field: index,
            value: Box::new(value),
            ty,
            file,
            line,
            column,
            end_column,
        }
    }

    fn infer_type_args(
        &mut self,
        type_params: &[TypeParamId],
        params: &[ParamInfo],
        arguments: &[Expr],
        span: Span,
    ) -> Option<Vec<(TypeParamId, CanonicalType)>> {
        if type_params.is_empty() {
            return Some(Vec::new());
        }
        let mut map = Vec::new();
        for (param, argument) in params.iter().zip(arguments.iter()) {
            if !self.uses_class_params(param.ty, type_params) {
                continue;
            }
            let concrete = if let Some(ty) = self.peek_type(argument) {
                ty
            } else {
                self.check_expr(argument, None, false).ty()
            };
            if !self.unify_type(param.ty, concrete, &mut map) {
                self.error(
                    argument.span(),
                    format!(
                        "type mismatch: expected '{}', found '{}'",
                        self.spell(param.ty),
                        self.spell(concrete)
                    ),
                );
                return None;
            }
        }
        for param in type_params {
            if !map.iter().any(|(id, _)| id == param) {
                self.error(
                    span,
                    format!(
                        "cannot infer type parameter '{}'",
                        self.type_params[param.0 as usize].name
                    ),
                );
                return None;
            }
        }
        let ordered = type_params
            .iter()
            .filter_map(|param| {
                map.iter()
                    .find(|(id, _)| id == param)
                    .map(|(_, ty)| (*param, *ty))
            })
            .collect::<Vec<_>>();
        if ordered.len() == type_params.len() {
            Some(ordered)
        } else {
            None
        }
    }

    fn unify_type(
        &self,
        pattern: CanonicalType,
        concrete: CanonicalType,
        map: &mut Vec<(TypeParamId, CanonicalType)>,
    ) -> bool {
        match (pattern, concrete) {
            (CanonicalType::Param(id), concrete) => {
                if let Some((_, existing)) = map.iter().find(|(param, _)| *param == id) {
                    return *existing == concrete || self.assignable(concrete, *existing);
                }
                map.push((id, concrete));
                true
            }
            (CanonicalType::Optional(pattern), CanonicalType::Optional(concrete)) => {
                self.unify_type(pattern.canonical(), concrete.canonical(), map)
            }
            (
                CanonicalType::Array {
                    leaf: pattern_leaf,
                    depth: pattern_depth,
                    ..
                },
                CanonicalType::Array {
                    leaf: concrete_leaf,
                    depth: concrete_depth,
                    ..
                },
            ) if pattern_depth == concrete_depth => {
                self.unify_type(pattern_leaf.canonical(), concrete_leaf.canonical(), map)
            }
            (CanonicalType::Callable(pattern_id), CanonicalType::Callable(concrete_id)) => {
                let Some(pattern_sig) = self.callables.get(pattern_id.0 as usize) else {
                    return false;
                };
                let Some(concrete_sig) = self.callables.get(concrete_id.0 as usize) else {
                    return false;
                };
                if pattern_sig.params.len() != concrete_sig.params.len() {
                    return false;
                }
                pattern_sig
                    .params
                    .iter()
                    .zip(concrete_sig.params.iter())
                    .all(|(pattern_param, concrete_param)| {
                        self.unify_type(*pattern_param, *concrete_param, map)
                    })
                    && self.unify_type(pattern_sig.ret, concrete_sig.ret, map)
            }
            (pattern, concrete) => {
                pattern == concrete
                    || self.assignable(concrete, pattern)
                    || (pattern.is_numeric() && concrete.is_numeric() && pattern == concrete)
            }
        }
    }

    fn finish_class_call(
        &mut self,
        object: CheckedExpr,
        id: ClassId,
        member: &MemberExpr,
        type_arguments: &[TypeSyntax],
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let instance = self.collect_methods(id, &member.member.name, false);
        let statics = self.collect_methods(id, &member.member.name, true);
        if instance.is_empty() && statics.is_empty() {
            if let Some((owner, index)) = self.locate_field(id, &member.member.name) {
                let spelled = self.spell(self.classes[owner.0 as usize].fields[index].ty);
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
        }
        let want_static = instance.is_empty();
        let found = if want_static { statics } else { instance };
        let (owner, index) = found[0];
        if found.len() > 1 {
            // Fall back to unsubstituted overload resolution for non-generic overloads.
            let Some((owner, index, args)) =
                self.resolve_methods(id, &member.member.name, want_static, arguments, span)
            else {
                return self.dummy(None);
            };
            let ret = self.classes[owner.0 as usize].methods[index].ret;
            let (file, line, column, end_column) = self.located(span);
            return CheckedExpr::ClassCall {
                class: owner,
                method: index,
                object: Some(Box::new(object)),
                args,
                type_args: Vec::new(),
                ty: ret,
                direct: false,
                file,
                line,
                column,
                end_column,
            };
        }
        let mut map = match object.ty() {
            CanonicalType::Generic(generic) => self.generic_map(generic),
            _ => Vec::new(),
        };
        let class_params = self.classes[owner.0 as usize].type_params.clone();
        let method_params = self.classes[owner.0 as usize].methods[index]
            .type_params
            .clone();
        let is_static = self.classes[owner.0 as usize].methods[index].is_static;
        let raw_params = self.classes[owner.0 as usize].methods[index].params.clone();
        let raw_ret = self.classes[owner.0 as usize].methods[index].ret;
        let mut type_args = Vec::new();
        if !method_params.is_empty() {
            if type_arguments.is_empty() {
                self.error(
                    span,
                    format!(
                        "generic method '{}' requires type arguments",
                        member.member.name
                    ),
                );
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            }
            let Some(method_map) = self.substitution_map(&method_params, type_arguments, span)
            else {
                for argument in arguments {
                    let _ = self.check_expr(argument, None, false);
                }
                return self.dummy(None);
            };
            type_args = method_map.iter().map(|(_, ty)| *ty).collect();
            map.extend(method_map);
        } else if is_static && !class_params.is_empty() && map.is_empty() {
            if !type_arguments.is_empty() {
                let Some(class_map) = self.substitution_map(&class_params, type_arguments, span)
                else {
                    for argument in arguments {
                        let _ = self.check_expr(argument, None, false);
                    }
                    return self.dummy(None);
                };
                type_args = class_map.iter().map(|(_, ty)| *ty).collect();
                map.extend(class_map);
            } else {
                let before = self.diagnostics.len();
                if let Some(inferred) =
                    self.infer_type_args(&class_params, &raw_params, arguments, span)
                {
                    type_args = inferred.iter().map(|(_, ty)| *ty).collect();
                    map.extend(inferred);
                } else {
                    if self.diagnostics.len() == before {
                        self.error(
                            span,
                            format!(
                                "generic class '{}' requires type arguments for static method '{}'",
                                self.class_name(owner),
                                member.member.name
                            ),
                        );
                    }
                    for argument in arguments {
                        let _ = self.check_expr(argument, None, false);
                    }
                    return self.dummy(None);
                }
            }
        } else if !type_arguments.is_empty() {
            self.error(
                span,
                format!("method '{}' is not generic", member.member.name),
            );
        }
        let params = raw_params
            .iter()
            .map(|param| ParamInfo {
                name: param.name.clone(),
                ty: self.substitute_type(param.ty, &map),
                auto: param.auto,
                default: param.default.clone(),
            })
            .collect::<Vec<_>>();
        let ret = self.substitute_type(raw_ret, &map);
        let public = self.classes[owner.0 as usize].methods[index].public;
        let protected = self.classes[owner.0 as usize].methods[index].protected;
        let mutating = self.classes[owner.0 as usize].methods[index].mutating;
        let name = self.classes[owner.0 as usize].methods[index].name.clone();
        let deprecated = self.classes[owner.0 as usize].methods[index].deprecated;
        self.note_deprecated(deprecated, span, &name);
        if !self.member_visible(public, protected, owner) {
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
        let args = self.check_call_args(arguments, &params, span);
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::ClassCall {
            class: owner,
            method: index,
            object: Some(Box::new(object)),
            args,
            type_args,
            ty: ret,
            direct: false,
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
            Expr::Identifier(ident) if ident.name == "range" => {
                // Element type is always int64; id 0 may be wrong before interning,
                // so callers that need a concrete IterId go through check_range.
                Some(CanonicalType::Iter(IterId(0)))
            }
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
        let mut message = message.into();
        if let Some(instantiation) = &self.instantiation {
            message = format!("{message} (in {instantiation})");
        }
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

    fn warn(&mut self, span: Span, message: impl Into<String>) {
        let message = message.into();
        self.warnings.push(SemanticDiagnostic {
            source_name: self.source_name.to_string(),
            span,
            message,
            code: "GRF-SEM-035",
            help: None,
        });
    }

    fn finish_named_static_field(&mut self, id: ClassId, expr: &MemberExpr) -> CheckedExpr {
        let Some((owner, index)) = self.locate_field(id, &expr.member.name) else {
            if self.has_method_name(id, &expr.member.name) {
                self.error(
                    expr.span,
                    format!("method '{}' must be called", expr.member.name),
                );
            } else {
                self.error(
                    expr.member.span,
                    format!("unknown field '{}'", expr.member.name),
                );
            }
            return self.dummy(None);
        };
        let (is_static, public, protected, ty, deprecated, name) = {
            let field = &self.classes[owner.0 as usize].fields[index];
            (
                field.is_static,
                field.public,
                field.protected,
                field.ty,
                field.deprecated,
                field.name.clone(),
            )
        };
        if !is_static {
            self.error(
                expr.member.span,
                format!("field '{name}' requires an instance"),
            );
        }
        if !self.member_visible(public, protected, owner) {
            self.error(
                expr.member.span,
                format!("field '{name}' is not accessible"),
            );
        }
        self.note_deprecated(deprecated, expr.member.span, &name);
        CheckedExpr::ClassGet {
            object: Box::new(CheckedExpr::Bool(false)),
            class: owner,
            field: index,
            ty,
        }
    }

    fn finish_super_field(&mut self, expr: &MemberExpr) -> CheckedExpr {
        let Some(class) = self.current_class else {
            self.error(expr.span, "super requires a parent class");
            return self.dummy(None);
        };
        if self.current_static {
            self.error(expr.span, "static method cannot use 'super'");
            return self.dummy(None);
        }
        let Some(parent) = self.classes[class.0 as usize].parent else {
            self.error(expr.span, "super requires a parent class");
            return self.dummy(None);
        };
        let this = CheckedExpr::Load {
            name: "this".to_string(),
            ty: CanonicalType::Class(class),
        };
        self.finish_class_get(this, parent, expr)
    }

    fn check_super_call(
        &mut self,
        member: &MemberExpr,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let Some(class) = self.current_class else {
            self.error(span, "super requires a parent class");
            return self.dummy(None);
        };
        if self.current_static {
            self.error(span, "static method cannot use 'super'");
            return self.dummy(None);
        }
        let Some(parent) = self.classes[class.0 as usize].parent else {
            self.error(span, "super requires a parent class");
            return self.dummy(None);
        };
        let before = self.diagnostics.len();
        let Some((owner, index, args)) =
            self.resolve_methods(parent, &member.member.name, false, arguments, span)
        else {
            if self.diagnostics.len() == before {
                self.error(
                    member.member.span,
                    format!("unknown method '{}'", member.member.name),
                );
            }
            return self.dummy(None);
        };
        if self.classes[owner.0 as usize].methods[index].is_abstract {
            self.error(
                member.member.span,
                format!("super cannot call abstract method '{}'", member.member.name),
            );
        }
        let ret = self.classes[owner.0 as usize].methods[index].ret;
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::ClassCall {
            class: owner,
            method: index,
            object: Some(Box::new(CheckedExpr::Load {
                name: "this".to_string(),
                ty: CanonicalType::Class(class),
            })),
            args,
            type_args: Vec::new(),
            ty: ret,
            direct: true,
            file,
            line,
            column,
            end_column,
        }
    }

    fn finish_interface_get(
        &mut self,
        _object: CheckedExpr,
        id: InterfaceId,
        expr: &MemberExpr,
    ) -> CheckedExpr {
        if self.interfaces[id.0 as usize]
            .methods
            .iter()
            .any(|method| method.name == expr.member.name)
        {
            self.error(
                expr.span,
                format!("method '{}' must be called", expr.member.name),
            );
        } else {
            self.error(
                expr.member.span,
                format!(
                    "interface '{}' has no field '{}'",
                    self.interface_name(id),
                    expr.member.name
                ),
            );
        }
        self.dummy(None)
    }

    fn finish_interface_call(
        &mut self,
        object: CheckedExpr,
        id: InterfaceId,
        member: &MemberExpr,
        arguments: &[Expr],
        span: Span,
    ) -> CheckedExpr {
        let found = self.interfaces[id.0 as usize]
            .methods
            .iter()
            .enumerate()
            .filter(|(_, method)| method.name == member.member.name)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if found.is_empty() {
            self.error(
                member.member.span,
                format!("unknown method '{}'", member.member.name),
            );
            for argument in arguments {
                let _ = self.check_expr(argument, None, false);
            }
            return self.dummy(None);
        }
        let chosen = if found.len() == 1 {
            let index = found[0];
            let params = self.interfaces[id.0 as usize].methods[index]
                .params
                .iter()
                .map(|param| (param.ty, param.auto))
                .collect::<Vec<_>>();
            let args = self.check_arguments(arguments, &params, span);
            Some((index, args))
        } else {
            let prelim = arguments
                .iter()
                .map(|argument| self.check_expr(argument, None, false))
                .collect::<Vec<_>>();
            let matches = found
                .iter()
                .copied()
                .filter(|index| {
                    let params = &self.interfaces[id.0 as usize].methods[*index].params;
                    params.len() == prelim.len()
                        && params.iter().zip(&prelim).all(|(param, argument)| {
                            self.adaptable(argument.ty(), param.ty, param.auto)
                        })
                })
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                self.error(span, format!("ambiguous call to '{}'", member.member.name));
                None
            } else {
                let index = matches[0];
                let params = self.interfaces[id.0 as usize].methods[index].params.clone();
                let args = prelim
                    .into_iter()
                    .zip(arguments.iter().zip(params))
                    .map(|(argument, (source, param))| {
                        self.apply_conversion(argument, param.ty, param.auto, source.span())
                    })
                    .collect();
                Some((index, args))
            }
        };
        let Some((index, args)) = chosen else {
            return self.dummy(None);
        };
        let method = &self.interfaces[id.0 as usize].methods[index];
        let ret = method.ret;
        let deprecated = method.deprecated;
        let name = method.name.clone();
        self.note_deprecated(deprecated, span, &name);
        let (file, line, column, end_column) = self.located(span);
        CheckedExpr::IfaceCall {
            interface: id,
            method: index,
            object: Box::new(object),
            args,
            ty: ret,
            file,
            line,
            column,
            end_column,
        }
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

fn is_ternary_expr(expr: &Expr) -> bool {
    matches!(unwrap_parens(expr), Expr::Ternary(_))
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
        BinaryOperator::BitAnd => "&",
        BinaryOperator::BitOr => "|",
        BinaryOperator::BitXor => "^",
        BinaryOperator::Shl => "<<",
        BinaryOperator::Shr => ">>",
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

fn as_lambda(expr: &Expr) -> Option<&LambdaExpr> {
    match expr {
        Expr::Lambda(lambda) => Some(lambda),
        Expr::Parenthesized { expression, .. } => as_lambda(expression),
        _ => None,
    }
}

fn guaranteed_return(statements: &[Stmt]) -> bool {
    statements.iter().any(stmt_returns)
}

fn rust_ident_simple(name: &str) -> String {
    if name.is_empty() {
        return "grf_".to_string();
    }
    let mut ident = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if index == 0 && ch.is_ascii_digit() {
                ident.push_str("grf_");
            }
            ident.push(ch);
        } else {
            ident.push('_');
        }
    }
    if ident.is_empty() {
        "grf_".to_string()
    } else {
        ident
    }
}

fn mangle_type_name(ty: CanonicalType) -> String {
    match ty {
        CanonicalType::Void => "void".to_string(),
        CanonicalType::Int16 => "i16".to_string(),
        CanonicalType::Int32 => "i32".to_string(),
        CanonicalType::Int64 => "i64".to_string(),
        CanonicalType::UInt16 => "u16".to_string(),
        CanonicalType::UInt32 => "u32".to_string(),
        CanonicalType::UInt64 => "u64".to_string(),
        CanonicalType::Flt32 => "f32".to_string(),
        CanonicalType::Flt64 => "f64".to_string(),
        CanonicalType::Bln => "bln".to_string(),
        CanonicalType::Chr => "chr".to_string(),
        CanonicalType::Str => "str".to_string(),
        CanonicalType::Type => "Type".to_string(),
        CanonicalType::Class(id) => format!("C{}", id.0),
        CanonicalType::Interface(id) => format!("I{}", id.0),
        CanonicalType::Record(id) => format!("R{}", id.0),
        CanonicalType::Map(id) => format!("M{}", id.0),
        CanonicalType::Iter(id) => format!("It{}", id.0),
        CanonicalType::AsyncIter(id) => format!("AIt{}", id.0),
        CanonicalType::Async(id) => format!("As{}", id.0),
        CanonicalType::Generic(id) => format!("G{}", id.0),
        CanonicalType::Param(id) => format!("P{}", id.0),
        CanonicalType::Callable(id) => format!("Fn{}", id.0),
        CanonicalType::ErrorHandle => "err".to_string(),
        CanonicalType::Optional(kind) => format!("opt_{}", mangle_type_name(kind.canonical())),
        CanonicalType::Array { leaf, depth, .. } => {
            format!("arr{depth}_{}", mangle_type_name(leaf.canonical()))
        }
    }
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

fn contains_emit(statements: &[Stmt]) -> bool {
    statements.iter().any(stmt_contains_emit)
}

fn stmt_contains_emit(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Emit(_) => true,
        Stmt::Block(block) => contains_emit(&block.statements),
        Stmt::If(stmt) => {
            contains_emit(&stmt.then_branch.statements)
                || stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| stmt_contains_emit(branch))
        }
        Stmt::Loop(stmt) => contains_emit(&stmt.body.statements),
        Stmt::Try(stmt) => {
            contains_emit(&stmt.body.statements)
                || stmt
                    .catches
                    .iter()
                    .any(|clause| contains_emit(&clause.block.statements))
                || stmt
                    .finally_block
                    .as_ref()
                    .is_some_and(|block| contains_emit(&block.statements))
        }
        Stmt::Conditional(stmt) => stmt_contains_emit(&stmt.action),
        _ => false,
    }
}

fn contains_await(statements: &[Stmt]) -> bool {
    statements.iter().any(stmt_contains_await)
}

fn stmt_contains_await(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Await(_) => true,
        Stmt::Block(block) => contains_await(&block.statements),
        Stmt::If(stmt) => {
            contains_await(&stmt.then_branch.statements)
                || stmt
                    .else_branch
                    .as_ref()
                    .is_some_and(|branch| stmt_contains_await(branch))
        }
        Stmt::Loop(stmt) => contains_await(&stmt.body.statements),
        Stmt::Try(stmt) => {
            contains_await(&stmt.body.statements)
                || stmt
                    .catches
                    .iter()
                    .any(|clause| contains_await(&clause.block.statements))
                || stmt
                    .finally_block
                    .as_ref()
                    .is_some_and(|block| contains_await(&block.statements))
        }
        Stmt::Conditional(stmt) => stmt_contains_await(&stmt.action),
        _ => false,
    }
}
