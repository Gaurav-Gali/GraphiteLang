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

mod grf_fns {
    use super::*;
    pub(crate) async fn fetchCount() -> Result<i64, graphite_core::diagnostic::RuntimeError> {
        return Ok(21i64);
        unreachable!();
    }

    pub(crate) async fn calculate() -> Result<i64, graphite_core::diagnostic::RuntimeError> {
        let mut count: i64 = (graphite_core::async_rt::box_async(async move { match crate::grf_fns::fetchCount().await { Ok(value) => Ok(value), Err(error) => Err(error.enter_call("fetchCount", graphite_core::convert::Site { file: "examples/async/main.grf", line: 8, column: 24, end_column: 36, excerpt: "    await<int> count = fetchCount();" })) } })).await?;
        return Ok(grf_mul!(count, 2i64, graphite_core::convert::Site { file: "examples/async/main.grf", line: 9, column: 12, end_column: 21, excerpt: "    return count * 2;" }));
        unreachable!();
    }

    pub(crate) async fn save() -> Result<(), graphite_core::diagnostic::RuntimeError> {
        return Ok(());
        Ok(())
    }

}

async fn grf_user_main() -> Result<(), graphite_core::diagnostic::RuntimeError> {
    let mut result: i64 = (graphite_core::async_rt::box_async(async move { match crate::grf_fns::calculate().await { Ok(value) => Ok(value), Err(error) => Err(error.enter_call("calculate", graphite_core::convert::Site { file: "examples/async/main.grf", line: 13, column: 25, end_column: 36, excerpt: "    await<int> result = calculate();" })) } })).await?;
    match graphite_core::io::println(graphite_core::convert::signed_text(i128::from(result))) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "examples/async/main.grf", line: 14, column: 5, end_column: 23, excerpt: "    io.println(result);" })) };
    (graphite_core::async_rt::box_async(async move { match crate::grf_fns::save().await { Ok(value) => Ok(value), Err(error) => Err(error.enter_call("save", graphite_core::convert::Site { file: "examples/async/main.grf", line: 15, column: 11, end_column: 17, excerpt: "    await save();" })) } })).await?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = grf_user_main().await {
        graphite_core::diagnostic::report(&error);
        std::process::exit(1);
    }
}

