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
    pub(crate) fn getNumbers() -> Result<graphite_core::async_iter::AsyncIter<i64>, graphite_core::diagnostic::RuntimeError> {
        Ok(graphite_core::async_iter::AsyncIter::spawn(move |__grf_tx| {
            Box::pin(async move {
                let __grf_err_tx = __grf_tx.clone();
                let __grf_body = async move {
                    let mut i: i64 = 0i64;
                    'grf_loop_0: while (i < 5i64) {
                        (graphite_core::async_rt::box_async(async move { match graphite_core::time::sleep(match graphite_core::time::secs(graphite_core::convert::signed_to_f64(i128::from(1i64), "int64", graphite_core::convert::Site { file: "mintest.grf", line: 7, column: 36, end_column: 37, excerpt: "        await time.sleep(time.secs(1));" })?) { Ok(value) => value, Err(error) => return Err(error.enter_call("secs", graphite_core::convert::Site { file: "mintest.grf", line: 7, column: 26, end_column: 38, excerpt: "        await time.sleep(time.secs(1));" })) }).await { Ok(value) => Ok(value), Err(error) => Err(error.enter_call("sleep", graphite_core::convert::Site { file: "mintest.grf", line: 7, column: 15, end_column: 39, excerpt: "        await time.sleep(time.secs(1));" })) } })).await?;
                        __grf_tx.send(Ok(i)).await.map_err(|_| graphite_core::diagnostic::runtime_internal("asyncIter closed"))?;
                        let _ = { i = grf_add!(i, 1i64, graphite_core::convert::Site { file: "mintest.grf", line: 9, column: 13, end_column: 18, excerpt: "        i = i + 1;" }); i };
                    }
                    Ok::<(), graphite_core::diagnostic::RuntimeError>(())
                };
                if let Err(__grf_error) = __grf_body.await {
                    let _ = __grf_err_tx.send(Err(__grf_error)).await;
                }
            })
        }))
    }

}

async fn grf_user_main() -> Result<(), graphite_core::diagnostic::RuntimeError> {
    {
        let grf_async_it = match crate::grf_fns::getNumbers() { Ok(value) => value, Err(error) => return Err(error.enter_call("getNumbers", graphite_core::convert::Site { file: "mintest.grf", line: 14, column: 26, end_column: 38, excerpt: "    loop (await<int> n : getNumbers()) {" })) };
        'grf_loop_1: loop {
            'grf_loop_2: loop {
                let mut n: i64 = match grf_async_it.try_next().await? {
                    Some(__grf_item) => __grf_item,
                    None => break 'grf_loop_1,
                };
                    match graphite_core::io::println(graphite_core::convert::signed_text(i128::from(n))) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "mintest.grf", line: 15, column: 9, end_column: 22, excerpt: "        io.println(n);" })) };
                break 'grf_loop_2;
            }
        }
    }
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = grf_user_main().await {
        graphite_core::diagnostic::report(&error);
        std::process::exit(1);
    }
}

