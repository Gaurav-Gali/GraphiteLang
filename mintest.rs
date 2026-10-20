#![allow(dead_code, unused_macros)]

macro_rules! grf_add {
    ($a:expr, $b:expr) => {
        match $a.checked_add($b) {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}
macro_rules! grf_sub {
    ($a:expr, $b:expr) => {
        match $a.checked_sub($b) {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}
macro_rules! grf_mul {
    ($a:expr, $b:expr) => {
        match $a.checked_mul($b) {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}
macro_rules! grf_div {
    ($a:expr, $b:expr) => {
        match $a.checked_div($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    grf_div_zero()
                } else {
                    grf_overflow()
                }
            }
        }
    };
}
macro_rules! grf_rem {
    ($a:expr, $b:expr) => {
        match $a.checked_rem($b) {
            Some(value) => value,
            None => {
                if $b == 0 {
                    grf_div_zero()
                } else {
                    grf_overflow()
                }
            }
        }
    };
}
macro_rules! grf_neg {
    ($a:expr) => {
        match $a.checked_neg() {
            Some(value) => value,
            None => grf_overflow(),
        }
    };
}

pub(crate) fn grf_overflow() -> ! {
    eprintln!("graphite: integer overflow");
    std::process::exit(1);
}

pub(crate) fn grf_div_zero() -> ! {
    eprintln!("graphite: division by zero");
    std::process::exit(1);
}

pub(crate) fn grf_str_length(value: &str) -> u64 {
    value.chars().count() as u64
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct GrfType {
    pub(crate) name: &'static str,
    pub(crate) record: bool,
}

fn main() {
    let mut m: std::collections::HashMap<String, i64> = { let mut grf_map = std::collections::HashMap::<String, i64>::new();let grf_key = String::from("a"); let grf_val = 1i64; grf_map.insert(grf_key, grf_val);let grf_key = String::from("b"); let grf_val = 2i64; grf_map.insert(grf_key, grf_val); grf_map };
    graphite_core::io::println({ let grf_tmp_0 = &(m.clone()); let mut grf_tmp_1 = String::from("{"); let mut grf_tmp_2 = true; for (grf_key, grf_val) in grf_tmp_0.iter() { if grf_tmp_2 { grf_tmp_1.push(' '); } else { grf_tmp_1.push_str(", "); } grf_tmp_2 = false; grf_tmp_1.push_str(&(graphite_core::convert::quoted_str(grf_key))); grf_tmp_1.push_str(": "); grf_tmp_1.push_str(&(graphite_core::convert::signed_text(i128::from(*grf_val)))); } if !grf_tmp_2 { grf_tmp_1.push(' '); } grf_tmp_1.push('}'); grf_tmp_1 });
}

