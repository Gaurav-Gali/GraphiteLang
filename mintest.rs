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

pub(crate) fn grf_str_length(value: &str) -> u64 {
    value.chars().count() as u64
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct GrfType {
    pub(crate) name: &'static str,
    pub(crate) record: bool,
}

#[derive(Clone)]
struct GrfUser {
    name: String,
    age: i64,
}

#[derive(Clone)]
struct GrfDevice {
    device: String,
}

fn grf_user_main() -> Result<(), graphite_core::diagnostic::RuntimeError> {
    let mut d1: GrfDevice = { let grf_tmp_0 = String::from("laptop"); GrfDevice { device: grf_tmp_0 } };
    let mut d2: GrfDevice = { let grf_tmp_1 = String::from("mobile"); GrfDevice { device: grf_tmp_1 } };
    let mut u1: GrfUser = { let grf_tmp_2 = String::from("graphite"); let grf_tmp_3 = 21i64; GrfUser { name: grf_tmp_2, age: grf_tmp_3 } };
    let mut m: std::collections::HashMap<String, Vec<GrfDevice>> = { let mut grf_map = std::collections::HashMap::<String, Vec<GrfDevice>>::new();let grf_key = (u1.clone()).name.clone(); let grf_val = vec![d1.clone(), d2.clone()]; grf_map.insert(grf_key, grf_val); grf_map };
    match graphite_core::io::println({ let grf_tmp_4 = &(m.clone()); let mut grf_tmp_5 = String::from("{"); let mut grf_tmp_6 = true; for (grf_key, grf_val) in grf_tmp_4.iter() { if grf_tmp_6 { grf_tmp_5.push(' '); } else { grf_tmp_5.push_str(", "); } grf_tmp_6 = false; grf_tmp_5.push_str(&(graphite_core::convert::quoted_str(grf_key))); grf_tmp_5.push_str(": "); grf_tmp_5.push_str(&(graphite_core::convert::array_text(&(grf_val), |grf_item| { { let grf_tmp_7 = &(grf_item); let mut grf_tmp_8 = String::from("{ ");grf_tmp_8.push_str("device: ");grf_tmp_8.push_str(&(graphite_core::convert::quoted_str(&(grf_tmp_7.device))));grf_tmp_8.push_str(" }"); grf_tmp_8 } }))); } if !grf_tmp_6 { grf_tmp_5.push(' '); } grf_tmp_5.push('}'); grf_tmp_5 }) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "mintest.grf", line: 23, column: 3, end_column: 16, excerpt: "  io.println(m);" })) };
    let mut grf_pending_0: GrfExit<()> = GrfExit::Fall;
    let grf_body = (|| -> Result<(), graphite_core::diagnostic::RuntimeError> {
        let mut a: i64 = (graphite_core::convert::str_to_signed(&(match graphite_core::io::input(String::from(">> ")) { Ok(value) => value, Err(error) => return Err(error.enter_call("input", graphite_core::convert::Site { file: "mintest.grf", line: 26, column: 18, end_column: 33, excerpt: "    auto int a = io.input(\">> \");" })) }), -9223372036854775808, 9223372036854775807, "int64", graphite_core::convert::Site { file: "mintest.grf", line: 26, column: 18, end_column: 33, excerpt: "    auto int a = io.input(\">> \");" })? as i64);
        let mut b: i64 = grf_div!(a, 0i64, graphite_core::convert::Site { file: "mintest.grf", line: 27, column: 13, end_column: 18, excerpt: "    int b = a / 0;" });
        Ok(())
    })();
    if let Err(error) = grf_body { grf_pending_0 = GrfExit::Error(error); }
    if matches!(&grf_pending_0, GrfExit::Error(error) if error.kind_name() == "DivisionByZeroError") {
        let GrfExit::Error(err) = std::mem::replace(&mut grf_pending_0, GrfExit::Fall) else { unreachable!() };
        let grf_catch = (|| -> Result<(), graphite_core::diagnostic::RuntimeError> {
            match graphite_core::io::println(String::from(err.kind_name())) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "mintest.grf", line: 29, column: 5, end_column: 27, excerpt: "    io.println(err.kind());" })) };
            Ok(())
        })();
        if let Err(error) = grf_catch { grf_pending_0 = GrfExit::Error(error); }
    }
    else if matches!(&grf_pending_0, GrfExit::Error(_)) {
        let GrfExit::Error(err) = std::mem::replace(&mut grf_pending_0, GrfExit::Fall) else { unreachable!() };
        let grf_catch = (|| -> Result<(), graphite_core::diagnostic::RuntimeError> {
            match graphite_core::io::println(err.message_text()) { Ok(value) => value, Err(error) => return Err(error.enter_call("println", graphite_core::convert::Site { file: "mintest.grf", line: 31, column: 5, end_column: 30, excerpt: "    io.println(err.message());" })) };
            Ok(())
        })();
        if let Err(error) = grf_catch { grf_pending_0 = GrfExit::Error(error); }
    }
    match std::mem::replace(&mut grf_pending_0, GrfExit::Fall) {
        GrfExit::Fall => {}
        GrfExit::Return(_) => return Ok(()),
        GrfExit::Error(error) => return Err(error),
        GrfExit::Break(label) => {
            let _ = label;
        }
        GrfExit::Continue(label) => {
            let _ = label;
        }
        GrfExit::ContinueInner(label) => {
            let _ = label;
        }
    };
    Ok(())
}

fn main() {
    if let Err(error) = grf_user_main() {
        graphite_core::diagnostic::report(&error);
        std::process::exit(1);
    }
}

