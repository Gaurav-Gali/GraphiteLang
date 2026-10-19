//! Numeric operator compatibility.
//!
//! Operator promotion is not the Stage 5 conversion registry. The result type
//! is chosen here, and any required operand adjustment is a separate typed
//! cast. Comparisons of different numeric types do not route through that cast.

use super::CanonicalType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericError {
    Incompatible,
    Remainder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareMode {
    Same,
    Integers,
    Floats,
    IntFloat,
}

pub fn arithmetic(
    left: CanonicalType,
    right: CanonicalType,
    remainder: bool,
) -> Result<CanonicalType, NumericError> {
    if !left.is_numeric() || !right.is_numeric() {
        return Err(NumericError::Incompatible);
    }
    if remainder && (!left.is_integer() || !right.is_integer()) {
        return Err(NumericError::Remainder);
    }
    Ok(result_type(left, right))
}

pub fn compare(left: CanonicalType, right: CanonicalType) -> Option<CompareMode> {
    if !left.is_numeric() || !right.is_numeric() {
        return None;
    }
    if left == right {
        Some(CompareMode::Same)
    } else if left.is_integer() && right.is_integer() {
        Some(CompareMode::Integers)
    } else if left.is_float() && right.is_float() {
        Some(CompareMode::Floats)
    } else {
        Some(CompareMode::IntFloat)
    }
}

pub fn needs_check(from: CanonicalType, to: CanonicalType) -> bool {
    from != to && !widens_exactly(from, to)
}

fn result_type(left: CanonicalType, right: CanonicalType) -> CanonicalType {
    if left.is_float() || right.is_float() {
        float_result(left, right)
    } else if left.is_signed() || right.is_signed() {
        signed_result(left, right)
    } else {
        unsigned_result(left, right)
    }
}

fn signed_result(left: CanonicalType, right: CanonicalType) -> CanonicalType {
    for candidate in [
        CanonicalType::Int16,
        CanonicalType::Int32,
        CanonicalType::Int64,
    ] {
        if covers(candidate, left) && covers(candidate, right) {
            return candidate;
        }
    }
    CanonicalType::Int64
}

fn unsigned_result(left: CanonicalType, right: CanonicalType) -> CanonicalType {
    match width(left).max(width(right)) {
        16 => CanonicalType::UInt16,
        32 => CanonicalType::UInt32,
        _ => CanonicalType::UInt64,
    }
}

fn float_result(left: CanonicalType, right: CanonicalType) -> CanonicalType {
    if left == CanonicalType::Flt64 || right == CanonicalType::Flt64 {
        return CanonicalType::Flt64;
    }
    let other = if left == CanonicalType::Flt32 {
        right
    } else {
        left
    };
    if other == CanonicalType::Flt32 || fits_f32(other) {
        CanonicalType::Flt32
    } else {
        CanonicalType::Flt64
    }
}

fn covers(target: CanonicalType, operand: CanonicalType) -> bool {
    let (min, max) = signed_bounds(target);
    if operand.is_signed() {
        let (low, high) = signed_bounds(operand);
        low >= min && high <= max
    } else if operand.is_unsigned() {
        unsigned_max(operand) <= max as u128
    } else {
        false
    }
}

fn fits_f32(ty: CanonicalType) -> bool {
    matches!(ty, CanonicalType::Int16 | CanonicalType::UInt16)
}

fn widens_exactly(from: CanonicalType, to: CanonicalType) -> bool {
    matches!(
        (from, to),
        (
            CanonicalType::Int16,
            CanonicalType::Int32 | CanonicalType::Int64
        ) | (CanonicalType::Int32, CanonicalType::Int64)
            | (
                CanonicalType::UInt16,
                CanonicalType::UInt32 | CanonicalType::UInt64
            )
            | (CanonicalType::UInt32, CanonicalType::UInt64)
            | (
                CanonicalType::UInt16,
                CanonicalType::Int32 | CanonicalType::Int64
            )
            | (CanonicalType::UInt32, CanonicalType::Int64)
            | (
                CanonicalType::Int16 | CanonicalType::UInt16,
                CanonicalType::Flt32 | CanonicalType::Flt64
            )
            | (
                CanonicalType::Int32 | CanonicalType::UInt32,
                CanonicalType::Flt64
            )
            | (CanonicalType::Flt32, CanonicalType::Flt64)
    )
}

fn width(ty: CanonicalType) -> u16 {
    match ty {
        CanonicalType::Int16 | CanonicalType::UInt16 => 16,
        CanonicalType::Int32 | CanonicalType::UInt32 | CanonicalType::Flt32 => 32,
        _ => 64,
    }
}

fn signed_bounds(ty: CanonicalType) -> (i128, i128) {
    match ty {
        CanonicalType::Int16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        CanonicalType::Int32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        _ => (i128::from(i64::MIN), i128::from(i64::MAX)),
    }
}

fn unsigned_max(ty: CanonicalType) -> u128 {
    match ty {
        CanonicalType::UInt16 => u128::from(u16::MAX),
        CanonicalType::UInt32 => u128::from(u32::MAX),
        _ => u128::from(u64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(name: &str) -> CanonicalType {
        match name {
            "i16" => CanonicalType::Int16,
            "i32" => CanonicalType::Int32,
            "i64" => CanonicalType::Int64,
            "u16" => CanonicalType::UInt16,
            "u32" => CanonicalType::UInt32,
            "u64" => CanonicalType::UInt64,
            "f32" => CanonicalType::Flt32,
            "f64" => CanonicalType::Flt64,
            _ => unreachable!(),
        }
    }

    #[test]
    fn arithmetic_matrix_follows_family_precedence() {
        let names = ["i16", "i32", "i64", "u16", "u32", "u64", "f32", "f64"];
        let expected = [
            ["i16", "i32", "i64", "i32", "i64", "i64", "f32", "f64"],
            ["i32", "i32", "i64", "i32", "i64", "i64", "f64", "f64"],
            ["i64", "i64", "i64", "i64", "i64", "i64", "f64", "f64"],
            ["i32", "i32", "i64", "u16", "u32", "u64", "f32", "f64"],
            ["i64", "i64", "i64", "u32", "u32", "u64", "f64", "f64"],
            ["i64", "i64", "i64", "u64", "u64", "u64", "f64", "f64"],
            ["f32", "f64", "f64", "f32", "f64", "f64", "f32", "f64"],
            ["f64", "f64", "f64", "f64", "f64", "f64", "f64", "f64"],
        ];
        for (row, left_name) in names.iter().enumerate() {
            for (column, right_name) in names.iter().enumerate() {
                let left = ty(left_name);
                let right = ty(right_name);
                let result = arithmetic(left, right, false).unwrap();
                assert_eq!(
                    result,
                    ty(expected[row][column]),
                    "{left_name} + {right_name}"
                );
                if left.is_integer() && right.is_integer() {
                    assert_eq!(arithmetic(left, right, true).unwrap(), result);
                } else {
                    assert_eq!(arithmetic(left, right, true), Err(NumericError::Remainder));
                }
            }
        }
    }

    #[test]
    fn comparisons_do_not_collapse_to_one_family() {
        assert_eq!(
            compare(CanonicalType::Int64, CanonicalType::UInt64),
            Some(CompareMode::Integers)
        );
        assert_eq!(
            compare(CanonicalType::Flt32, CanonicalType::Flt64),
            Some(CompareMode::Floats)
        );
        assert_eq!(
            compare(CanonicalType::Int32, CanonicalType::Flt32),
            Some(CompareMode::IntFloat)
        );
        assert_eq!(
            compare(CanonicalType::Int16, CanonicalType::Int16),
            Some(CompareMode::Same)
        );
        assert_eq!(compare(CanonicalType::Str, CanonicalType::Int64), None);
        assert!(!needs_check(CanonicalType::UInt16, CanonicalType::Int32));
        assert!(needs_check(CanonicalType::UInt64, CanonicalType::Int64));
        assert!(needs_check(CanonicalType::Int64, CanonicalType::Flt64));
        assert!(!needs_check(CanonicalType::Int32, CanonicalType::Flt64));
    }
}
