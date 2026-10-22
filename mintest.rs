#![allow(dead_code, unused_macros, unreachable_code, unused_variables)]
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

fn grf_user_main() -> Result<(), graphite_core::diagnostic::RuntimeError> {
    match graphite_core::io::println(graphite_core::convert::array_text(&(graphite_core::array::slice_copy(&(vec![1i64, 2i64, 3i64, 4i64, 5i64]), None, None, Some(i128::from((-1i128 as i64))), graphite_core::convert::Site { file: "mintest.grf", line: 4, column: 14, end_column: 31, excerpt: "  io.println([1,2,3,4,5][::-1]);" })?), |grf_item| { graphite_core::convert::signed_text(i128::from(*grf_item)) })) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "mintest.grf", line: 4, column: 3, end_column: 32, excerpt: "  io.println([1,2,3,4,5][::-1]);" })) };
    Ok(())
}

fn main() {
    if let Err(error) = grf_user_main() {
        graphite_core::diagnostic::report(&error);
        std::process::exit(1);
    }
}

