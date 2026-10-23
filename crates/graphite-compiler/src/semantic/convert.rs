//! One registry for `auto T` and explicit `T(value)` conversions.
//!
//! Identity conversions are not listed here. The checker leaves those
//! expressions unchanged. Every other approved pair maps to one operation
//! that code generation lowers through `graphite_core::convert`.

use super::CanonicalType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvOp {
    ToStr,
    StrToSigned,
    StrToUnsigned,
    StrToF64,
    StrToF32,
    StrToBool,
    StrToChar,
    SignedToSigned,
    SignedToUnsigned,
    UnsignedToSigned,
    UnsignedToUnsigned,
    SignedToF64,
    SignedToF32,
    UnsignedToF64,
    UnsignedToF32,
    F64ToF32,
    F32ToF64,
    F64ToSigned,
    F64ToUnsigned,
    F32ToSigned,
    F32ToUnsigned,
    SignedToBool,
    UnsignedToBool,
    F64ToBool,
    F32ToBool,
    BoolToSigned,
    BoolToUnsigned,
    BoolToF64,
    BoolToF32,
    SignedToChar,
    UnsignedToChar,
    F64ToChar,
    F32ToChar,
    BoolToChar,
    CharToSigned,
    CharToUnsigned,
    CharToF64,
    CharToF32,
    CharToBool,
    CharToStr,
    ArrayToStr,
    RecordToStr,
    MapToStr,
    TypeToStr,
    OptionalToStr,
    ArrayToChar,
    RecordToChar,
    MapToChar,
    TypeToChar,
    OptionalToChar,
}

pub fn conversion(from: CanonicalType, to: CanonicalType) -> Option<ConvOp> {
    use CanonicalType::*;
    if to == Str {
        return match from {
            Int16 | Int32 | Int64 | UInt16 | UInt32 | UInt64 | Flt32 | Flt64 | Bln => {
                Some(ConvOp::ToStr)
            }
            Chr => Some(ConvOp::CharToStr),
            Array { .. } => Some(ConvOp::ArrayToStr),
            Record(_) => Some(ConvOp::RecordToStr),
            Map(_) => Some(ConvOp::MapToStr),
            Type => Some(ConvOp::TypeToStr),
            _ => None,
        };
    }
    if from == Str {
        return match to {
            Int16 | Int32 | Int64 => Some(ConvOp::StrToSigned),
            UInt16 | UInt32 | UInt64 => Some(ConvOp::StrToUnsigned),
            Flt64 => Some(ConvOp::StrToF64),
            Flt32 => Some(ConvOp::StrToF32),
            Bln => Some(ConvOp::StrToBool),
            Chr => Some(ConvOp::StrToChar),
            _ => None,
        };
    }
    if from == Bln {
        return match to {
            Int16 | Int32 | Int64 => Some(ConvOp::BoolToSigned),
            UInt16 | UInt32 | UInt64 => Some(ConvOp::BoolToUnsigned),
            Flt64 => Some(ConvOp::BoolToF64),
            Flt32 => Some(ConvOp::BoolToF32),
            Chr => Some(ConvOp::BoolToChar),
            _ => None,
        };
    }
    if to == Bln {
        return match from {
            Int16 | Int32 | Int64 => Some(ConvOp::SignedToBool),
            UInt16 | UInt32 | UInt64 => Some(ConvOp::UnsignedToBool),
            Flt64 => Some(ConvOp::F64ToBool),
            Flt32 => Some(ConvOp::F32ToBool),
            Chr => Some(ConvOp::CharToBool),
            _ => None,
        };
    }
    if to == Chr {
        return match from {
            Int16 | Int32 | Int64 => Some(ConvOp::SignedToChar),
            UInt16 | UInt32 | UInt64 => Some(ConvOp::UnsignedToChar),
            Flt64 => Some(ConvOp::F64ToChar),
            Flt32 => Some(ConvOp::F32ToChar),
            Array { .. } => Some(ConvOp::ArrayToChar),
            Record(_) => Some(ConvOp::RecordToChar),
            Map(_) => Some(ConvOp::MapToChar),
            Type => Some(ConvOp::TypeToChar),
            _ => None,
        };
    }
    if from == Chr {
        return match to {
            Int16 | Int32 | Int64 => Some(ConvOp::CharToSigned),
            UInt16 | UInt32 | UInt64 => Some(ConvOp::CharToUnsigned),
            Flt64 => Some(ConvOp::CharToF64),
            Flt32 => Some(ConvOp::CharToF32),
            _ => None,
        };
    }
    match (integer_class(from), integer_class(to), from, to) {
        (Some(IntegerClass::Signed), Some(IntegerClass::Signed), _, _) => {
            Some(ConvOp::SignedToSigned)
        }
        (Some(IntegerClass::Signed), Some(IntegerClass::Unsigned), _, _) => {
            Some(ConvOp::SignedToUnsigned)
        }
        (Some(IntegerClass::Unsigned), Some(IntegerClass::Signed), _, _) => {
            Some(ConvOp::UnsignedToSigned)
        }
        (Some(IntegerClass::Unsigned), Some(IntegerClass::Unsigned), _, _) => {
            Some(ConvOp::UnsignedToUnsigned)
        }
        (Some(IntegerClass::Signed), None, _, Flt64) => Some(ConvOp::SignedToF64),
        (Some(IntegerClass::Signed), None, _, Flt32) => Some(ConvOp::SignedToF32),
        (Some(IntegerClass::Unsigned), None, _, Flt64) => Some(ConvOp::UnsignedToF64),
        (Some(IntegerClass::Unsigned), None, _, Flt32) => Some(ConvOp::UnsignedToF32),
        (None, Some(IntegerClass::Signed), Flt64, _) => Some(ConvOp::F64ToSigned),
        (None, Some(IntegerClass::Unsigned), Flt64, _) => Some(ConvOp::F64ToUnsigned),
        (None, Some(IntegerClass::Signed), Flt32, _) => Some(ConvOp::F32ToSigned),
        (None, Some(IntegerClass::Unsigned), Flt32, _) => Some(ConvOp::F32ToUnsigned),
        (None, None, Flt64, Flt32) => Some(ConvOp::F64ToF32),
        (None, None, Flt32, Flt64) => Some(ConvOp::F32ToF64),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IntegerClass {
    Signed,
    Unsigned,
}

fn integer_class(ty: CanonicalType) -> Option<IntegerClass> {
    match ty {
        CanonicalType::Int16 | CanonicalType::Int32 | CanonicalType::Int64 => {
            Some(IntegerClass::Signed)
        }
        CanonicalType::UInt16 | CanonicalType::UInt32 | CanonicalType::UInt64 => {
            Some(IntegerClass::Unsigned)
        }
        _ => None,
    }
}
