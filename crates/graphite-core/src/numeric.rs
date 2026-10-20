//! Operator promotion helpers.
//!
//! These checks are separate from explicit `T(value)` conversions. An operand
//! that does not fit in the operator's result type stops the program. Integer
//! and float comparisons stay exact: a value is never rounded or reinterpreted
//! as unsigned just to make the operands match.

use crate::convert::{self, Site};
use crate::diagnostic::{self, RuntimeError};

pub fn unsigned_to_signed(
    value: u128,
    max: i128,
    to: &str,
    site: Site<'_>,
) -> Result<i128, RuntimeError> {
    if max < 0 || value > max as u128 {
        return Err(diagnostic::numeric_failure(
            site,
            format!("value does not fit in '{to}'"),
        ));
    }
    Ok(value as i128)
}

pub fn signed_to_f64(value: i128, to: &str, site: Site<'_>) -> Result<f64, RuntimeError> {
    convert::exact_signed_f64(value).ok_or_else(|| {
        diagnostic::numeric_failure(
            site,
            format!("value is not exactly representable as '{to}'"),
        )
    })
}

pub fn unsigned_to_f64(value: u128, to: &str, site: Site<'_>) -> Result<f64, RuntimeError> {
    convert::exact_unsigned_f64(value).ok_or_else(|| {
        diagnostic::numeric_failure(
            site,
            format!("value is not exactly representable as '{to}'"),
        )
    })
}

pub fn signed_to_f32(value: i128, to: &str, site: Site<'_>) -> Result<f32, RuntimeError> {
    let wide = signed_to_f64(value, to, site)?;
    narrow_f64(wide, to, site)
}

pub fn unsigned_to_f32(value: u128, to: &str, site: Site<'_>) -> Result<f32, RuntimeError> {
    let wide = unsigned_to_f64(value, to, site)?;
    narrow_f64(wide, to, site)
}

fn narrow_f64(value: f64, to: &str, site: Site<'_>) -> Result<f32, RuntimeError> {
    let narrow = value as f32;
    if narrow as f64 == value {
        Ok(narrow)
    } else {
        Err(diagnostic::numeric_failure(
            site,
            format!("value is not exactly representable as '{to}'"),
        ))
    }
}

/// Compares an integer with the exact real value of a float.
///
/// Returns `-1` when the integer is less, `0` when equal, `1` when greater,
/// and `2` when the float is `NaN`.
pub fn cmp_int_f64(value: i128, float: f64) -> i8 {
    if float.is_nan() {
        return 2;
    }
    if float.is_infinite() {
        return if float.is_sign_positive() { -1 } else { 1 };
    }
    if float.fract() != 0.0 {
        let boundary = convert::integral_i128(float.trunc()).unwrap_or(0);
        return if value < boundary {
            -1
        } else if value > boundary {
            1
        } else if float > boundary as f64 {
            -1
        } else {
            1
        };
    }
    match convert::integral_i128(float) {
        Some(bits) => {
            if value < bits {
                -1
            } else if value > bits {
                1
            } else {
                0
            }
        }
        None => {
            if float.is_sign_positive() {
                -1
            } else {
                1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_float_comparisons_stay_exact() {
        assert_eq!(cmp_int_f64(-10, 5.0), -1);
        assert_eq!(cmp_int_f64(10, 10.5), -1);
        assert_eq!(cmp_int_f64(10, 10.0), 0);
        assert_eq!(cmp_int_f64(0, -0.0), 0);
        assert_eq!(cmp_int_f64(2, 1.5), 1);
        assert_eq!(cmp_int_f64(-2, -1.5), -1);
        assert_eq!(cmp_int_f64(-1, -1.5), 1);
        assert_eq!(
            cmp_int_f64(9_007_199_254_740_993, 9_007_199_254_740_992.0),
            1
        );
        assert_eq!(
            cmp_int_f64(i128::MIN, -170141183460469231731687303715884105728.0),
            0
        );
        assert_eq!(cmp_int_f64(1, f64::INFINITY), -1);
        assert_eq!(cmp_int_f64(1, f64::NEG_INFINITY), 1);
        assert_eq!(cmp_int_f64(1, f64::NAN), 2);
        assert_eq!(cmp_int_f64(1, f64::from_bits(0x7ff0000000000001)), 2);
    }
}
