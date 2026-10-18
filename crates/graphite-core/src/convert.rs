//! Shared primitive conversions.
//!
//! String formatting for numbers, booleans, and characters lives here so
//! explicit `str(value)`, `auto str`, and interpolation use one policy.
//! Fallible conversions print one diagnostic and terminate the process.

#[derive(Clone, Copy)]
pub struct Site<'a> {
    pub file: &'a str,
    pub line: u32,
    pub column: u32,
}

pub fn fail(site: Site<'_>, from: &str, to: &str, category: &str) -> ! {
    eprintln!(
        "Runtime conversion error at {}:{}:{}:\ncannot convert {from} to {to}: {category}",
        site.file, site.line, site.column
    );
    std::process::exit(1);
}

pub fn signed_text(value: i128) -> String {
    let negative = value < 0;
    let mut digits = value.unsigned_abs();
    if digits == 0 {
        return "0".to_string();
    }
    let mut text = Vec::new();
    while digits > 0 {
        text.push(b'0' + (digits % 10) as u8);
        digits /= 10;
    }
    if negative {
        text.push(b'-');
    }
    text.reverse();
    String::from_utf8(text).unwrap()
}

pub fn unsigned_text(value: u128) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut digits = value;
    let mut text = Vec::new();
    while digits > 0 {
        text.push(b'0' + (digits % 10) as u8);
        digits /= 10;
    }
    text.reverse();
    String::from_utf8(text).unwrap()
}

pub fn bln_text(value: bool) -> String {
    if value {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

pub fn f64_text(value: f64) -> String {
    value.to_string()
}

pub fn f32_text(value: f32) -> String {
    value.to_string()
}

pub fn char_to_str(value: char) -> String {
    value.to_string()
}

pub fn str_to_signed(text: &str, min: i128, max: i128, to: &str, site: Site<'_>) -> i128 {
    let Some(value) = parse_signed(text) else {
        let category = if signed_shape(text) {
            "out of range"
        } else {
            "invalid decimal integer"
        };
        fail(site, "str", to, category);
    };
    if value < min || value > max {
        fail(site, "str", to, "out of range");
    }
    value
}

pub fn str_to_unsigned(text: &str, max: u128, to: &str, site: Site<'_>) -> u128 {
    let Some(value) = parse_unsigned(text) else {
        let category = if unsigned_shape(text) {
            "out of range"
        } else {
            "invalid decimal integer"
        };
        fail(site, "str", to, category);
    };
    if value > max {
        fail(site, "str", to, "out of range");
    }
    value
}

pub fn str_to_f64(text: &str, site: Site<'_>) -> f64 {
    parse_float(text, site, "flt64", |body| body.parse::<f64>().ok())
}

pub fn str_to_f32(text: &str, site: Site<'_>) -> f32 {
    parse_float(text, site, "flt32", |body| body.parse::<f32>().ok())
}

pub fn str_to_bool(text: &str, site: Site<'_>) -> bool {
    let body = trim_ascii_ws(text);
    if body.eq_ignore_ascii_case("true") {
        true
    } else if body.eq_ignore_ascii_case("false") {
        false
    } else {
        fail(site, "str", "bln", "invalid boolean text");
    }
}

pub fn str_to_char(text: &str, site: Site<'_>) -> char {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => ch,
        _ => fail(site, "str", "chr", "invalid character string"),
    }
}

pub fn fit_signed(value: i128, min: i128, max: i128, from: &str, to: &str, site: Site<'_>) -> i128 {
    if value < min || value > max {
        fail(site, from, to, "out of range");
    }
    value
}

pub fn fit_unsigned(value: u128, max: u128, from: &str, to: &str, site: Site<'_>) -> u128 {
    if value > max {
        fail(site, from, to, "out of range");
    }
    value
}

pub fn signed_to_unsigned(value: i128, max: u128, from: &str, to: &str, site: Site<'_>) -> u128 {
    if value < 0 {
        fail(site, from, to, "out of range");
    }
    fit_unsigned(value as u128, max, from, to, site)
}

pub fn unsigned_to_signed(value: u128, max: i128, from: &str, to: &str, site: Site<'_>) -> i128 {
    if max < 0 || value > max as u128 {
        fail(site, from, to, "out of range");
    }
    value as i128
}

pub fn signed_to_f64(value: i128, from: &str, site: Site<'_>) -> f64 {
    exact_signed_f64(value)
        .unwrap_or_else(|| fail(site, from, "flt64", "not exactly representable"))
}

pub fn signed_to_f32(value: i128, from: &str, site: Site<'_>) -> f32 {
    let wide = exact_signed_f64(value)
        .unwrap_or_else(|| fail(site, from, "flt32", "not exactly representable"));
    f64_to_f32(wide, from, "flt32", site)
}

pub fn unsigned_to_f64(value: u128, from: &str, site: Site<'_>) -> f64 {
    exact_unsigned_f64(value)
        .unwrap_or_else(|| fail(site, from, "flt64", "not exactly representable"))
}

pub fn unsigned_to_f32(value: u128, from: &str, site: Site<'_>) -> f32 {
    let wide = exact_unsigned_f64(value)
        .unwrap_or_else(|| fail(site, from, "flt32", "not exactly representable"));
    f64_to_f32(wide, from, "flt32", site)
}

pub fn f64_to_f32(value: f64, from: &str, to: &str, site: Site<'_>) -> f32 {
    if value.is_nan() {
        return f32::NAN;
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    let narrow = value as f32;
    if !narrow.is_finite() {
        fail(site, from, to, "out of range");
    }
    if narrow == 0.0 && value != 0.0 {
        fail(site, from, to, "underflow");
    }
    if narrow as f64 != value {
        fail(site, from, to, "not exactly representable");
    }
    narrow
}

pub fn f64_to_signed(
    value: f64,
    min: i128,
    max: i128,
    from: &str,
    to: &str,
    site: Site<'_>,
) -> i128 {
    let Some(converted) = integral_i128(value) else {
        let category = if value.is_finite() {
            "not an integer"
        } else {
            "non-finite value"
        };
        fail(site, from, to, category);
    };
    fit_signed(converted, min, max, from, to, site)
}

pub fn f64_to_unsigned(value: f64, max: u128, from: &str, to: &str, site: Site<'_>) -> u128 {
    if value < 0.0 {
        fail(site, from, to, "out of range");
    }
    let signed = f64_to_signed(value, 0, i128::MAX, from, to, site);
    fit_unsigned(signed as u128, max, from, to, site)
}

pub fn signed_to_bool(value: i128, from: &str, site: Site<'_>) -> bool {
    match value {
        0 => false,
        1 => true,
        _ => fail(site, from, "bln", "invalid boolean"),
    }
}

pub fn unsigned_to_bool(value: u128, from: &str, site: Site<'_>) -> bool {
    match value {
        0 => false,
        1 => true,
        _ => fail(site, from, "bln", "invalid boolean"),
    }
}

pub fn f64_to_bool(value: f64, from: &str, site: Site<'_>) -> bool {
    if value == 0.0 {
        false
    } else if value == 1.0 {
        true
    } else {
        fail(site, from, "bln", "invalid boolean");
    }
}

pub fn code_to_char_signed(value: i128, from: &str, site: Site<'_>) -> char {
    if value < 0 || value > i128::from(0x10FFFF) {
        fail(site, from, "chr", "invalid Unicode scalar value");
    }
    char::from_u32(value as u32)
        .unwrap_or_else(|| fail(site, from, "chr", "invalid Unicode scalar value"))
}

pub fn code_to_char_unsigned(value: u128, from: &str, site: Site<'_>) -> char {
    if value > u128::from(0x10FFFFu32) {
        fail(site, from, "chr", "invalid Unicode scalar value");
    }
    code_to_char_signed(value as i128, from, site)
}

pub fn code_to_char_f64(value: f64, from: &str, site: Site<'_>) -> char {
    let code = f64_to_signed(value, 0, i128::from(0x10FFFF), from, "chr", site);
    code_to_char_signed(code, from, site)
}

pub fn char_to_code(value: char) -> u32 {
    u32::from(value)
}

pub fn char_to_signed(value: char, max: i128, to: &str, site: Site<'_>) -> i128 {
    unsigned_to_signed(u128::from(char_to_code(value)), max, "chr", to, site)
}

pub fn char_to_unsigned(value: char, max: u128, to: &str, site: Site<'_>) -> u128 {
    fit_unsigned(u128::from(char_to_code(value)), max, "chr", to, site)
}

pub fn char_to_f64(value: char) -> f64 {
    f64::from(char_to_code(value))
}

pub fn char_to_f32(value: char, site: Site<'_>) -> f32 {
    f64_to_f32(char_to_f64(value), "chr", "flt32", site)
}

fn parse_float<T: PartialEq + Copy>(
    text: &str,
    site: Site<'_>,
    to: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> T
where
    T: FloatParsed,
{
    let body = trim_ascii_ws(text);
    if !float_grammar(body) {
        fail(site, "str", to, "invalid decimal number");
    }
    let Some(value) = parse(body) else {
        fail(site, "str", to, "invalid decimal number");
    };
    if !value.is_finite_number() {
        fail(site, "str", to, "out of range");
    }
    if value.is_zero() && !significand_is_zero(body) {
        fail(site, "str", to, "underflow");
    }
    value
}

trait FloatParsed: Copy {
    fn is_finite_number(self) -> bool;
    fn is_zero(self) -> bool;
}

impl FloatParsed for f64 {
    fn is_finite_number(self) -> bool {
        self.is_finite()
    }
    fn is_zero(self) -> bool {
        self == 0.0
    }
}

impl FloatParsed for f32 {
    fn is_finite_number(self) -> bool {
        self.is_finite()
    }
    fn is_zero(self) -> bool {
        self == 0.0
    }
}

fn trim_ascii_ws(text: &str) -> &str {
    text.trim_matches(|ch: char| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{000B}' | '\u{000C}'))
}

fn signed_shape(text: &str) -> bool {
    let body = trim_ascii_ws(text);
    let rest = body.strip_prefix(['+', '-']).unwrap_or(body);
    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit())
}

fn unsigned_shape(text: &str) -> bool {
    let body = trim_ascii_ws(text);
    let rest = body.strip_prefix('+').unwrap_or(body);
    !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()) && !body.starts_with('-')
}

fn parse_signed(text: &str) -> Option<i128> {
    if !signed_shape(text) {
        return None;
    }
    let body = trim_ascii_ws(text);
    let (negative, digits) = if let Some(rest) = body.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = body.strip_prefix('+') {
        (false, rest)
    } else {
        (false, body)
    };
    let magnitude = parse_i128_digits(digits)?;
    if negative {
        magnitude.checked_neg()
    } else {
        Some(magnitude)
    }
}

fn parse_unsigned(text: &str) -> Option<u128> {
    if !unsigned_shape(text) {
        return None;
    }
    let body = trim_ascii_ws(text);
    let digits = body.strip_prefix('+').unwrap_or(body);
    parse_u128_digits(digits)
}

fn parse_i128_digits(digits: &str) -> Option<i128> {
    let mut value: i128 = 0;
    for byte in digits.bytes() {
        value = value
            .checked_mul(10)?
            .checked_add(i128::from(byte - b'0'))?;
    }
    Some(value)
}

fn parse_u128_digits(digits: &str) -> Option<u128> {
    let mut value: u128 = 0;
    for byte in digits.bytes() {
        value = value
            .checked_mul(10)?
            .checked_add(u128::from(byte - b'0'))?;
    }
    Some(value)
}

fn float_grammar(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }
    let start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == start {
        return false;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let fraction = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == fraction {
            return false;
        }
    }
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        index += 1;
        if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
            index += 1;
        }
        let exponent = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == exponent {
            return false;
        }
    }
    index == bytes.len()
}

fn significand_is_zero(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    if bytes
        .first()
        .is_some_and(|byte| *byte == b'+' || *byte == b'-')
    {
        index = 1;
    }
    let mut saw_digit = false;
    while index < bytes.len() && bytes[index] != b'e' && bytes[index] != b'E' {
        if bytes[index] != b'.' && bytes[index] != b'0' {
            return false;
        }
        if bytes[index] != b'.' {
            saw_digit = true;
        }
        index += 1;
    }
    saw_digit
}

fn exact_signed_f64(value: i128) -> Option<f64> {
    let converted = value as f64;
    if !converted.is_finite() {
        return None;
    }
    if converted as i128 == value {
        Some(converted)
    } else {
        None
    }
}

fn exact_unsigned_f64(value: u128) -> Option<f64> {
    let converted = value as f64;
    if !converted.is_finite() {
        return None;
    }
    if converted as u128 == value {
        Some(converted)
    } else {
        None
    }
}

fn integral_i128(value: f64) -> Option<i128> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    let converted = value as i128;
    if converted as f64 == value {
        Some(converted)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Site<'static> {
        Site {
            file: "test.grf",
            line: 1,
            column: 1,
        }
    }

    #[test]
    fn integer_text_matches_the_existing_decimal_form() {
        assert_eq!(signed_text(0), "0");
        assert_eq!(signed_text(-7), "-7");
        assert_eq!(signed_text(i64::MIN as i128), "-9223372036854775808");
        assert_eq!(unsigned_text(0), "0");
        assert_eq!(unsigned_text(u64::MAX as u128), "18446744073709551615");
    }

    #[test]
    fn parses_signed_and_unsigned_boundaries() {
        let here = site();
        assert_eq!(
            str_to_signed(" -42 ", i64::MIN as i128, i64::MAX as i128, "int64", here),
            -42
        );
        assert_eq!(
            str_to_signed(
                "-9223372036854775808",
                i64::MIN as i128,
                i64::MAX as i128,
                "int64",
                here
            ),
            i64::MIN as i128
        );
        assert_eq!(
            str_to_signed(
                "9223372036854775807",
                i64::MIN as i128,
                i64::MAX as i128,
                "int64",
                here
            ),
            i64::MAX as i128
        );
        assert_eq!(
            str_to_unsigned("18446744073709551615", u64::MAX as u128, "uint64", here),
            u64::MAX as u128
        );
        assert_eq!(
            str_to_signed("123", i32::MIN as i128, i32::MAX as i128, "int32", here),
            123
        );
        assert_eq!(
            str_to_signed(
                "-2147483648",
                i32::MIN as i128,
                i32::MAX as i128,
                "int32",
                here
            ),
            i32::MIN as i128
        );
        assert_eq!(
            str_to_signed(
                "2147483647",
                i32::MIN as i128,
                i32::MAX as i128,
                "int32",
                here
            ),
            i32::MAX as i128
        );
        assert_eq!(
            str_to_signed("-32768", i16::MIN as i128, i16::MAX as i128, "int16", here),
            i16::MIN as i128
        );
        assert_eq!(
            str_to_signed("32767", i16::MIN as i128, i16::MAX as i128, "int16", here),
            i16::MAX as i128
        );
        assert_eq!(
            str_to_unsigned("65535", u16::MAX as u128, "uint16", here),
            u16::MAX as u128
        );
        assert_eq!(
            str_to_unsigned("4294967295", u32::MAX as u128, "uint32", here),
            u32::MAX as u128
        );
        assert!(parse_signed("40000").is_some());
        assert!(parse_signed("40000").unwrap() <= i32::MAX as i128);
        assert!(parse_signed("40000").unwrap() > i16::MAX as i128);
        assert!(parse_signed("2147483648").unwrap() > i32::MAX as i128);
        assert!(parse_unsigned("-0").is_none());
        assert!(parse_unsigned("-1").is_none());
        assert!(parse_signed("").is_none());
        assert!(parse_signed("+").is_none());
        assert!(parse_signed("1.0").is_none());
        assert!(parse_signed("12abc").is_none());
        assert!(parse_signed("1 2").is_none());
    }

    #[test]
    fn trims_only_the_six_ascii_whitespace_characters() {
        for pad in [' ', '\t', '\n', '\r', '\u{000B}', '\u{000C}'] {
            let text = format!("{pad}12{pad}");
            assert_eq!(
                str_to_signed(&text, i16::MIN as i128, i16::MAX as i128, "int16", site()),
                12
            );
        }
        assert!(parse_signed("\u{00A0}12").is_none());
        assert!(parse_signed("12\u{00A0}").is_none());
    }

    #[test]
    fn float_text_follows_the_graphite_literal_shapes() {
        let here = site();
        assert_eq!(str_to_f64("12", here), 12.0);
        assert_eq!(str_to_f64("12.5", here), 12.5);
        assert_eq!(str_to_f64("1e-2", here), 0.01);
        assert_eq!(str_to_f64("+1.5", here), 1.5);
        assert_eq!(str_to_f64("-0.5", here), -0.5);
        assert_eq!(str_to_f32("1.5", here), 1.5);
        for rejected in [".5", "5.", "1e", "12abc", "NaN", "Infinity", "+inf", ""] {
            assert!(!float_grammar(rejected), "{rejected}");
        }
        assert!(float_grammar("0e-9999"));
        assert_eq!(str_to_f64("0e-9999", here), 0.0);
    }

    #[test]
    fn boolean_text_is_case_insensitive() {
        assert!(str_to_bool(" true ", site()));
        assert!(!str_to_bool("FALSE", site()));
        assert!(parse_signed("yes").is_none());
    }

    #[test]
    fn character_text_counts_unicode_scalars() {
        assert_eq!(str_to_char("A", site()), 'A');
        assert_eq!(str_to_char("é", site()), 'é');
        assert_eq!(str_to_char("𐍈", site()), '\u{10348}');
        assert_eq!("AB".chars().count(), 2);
        assert_eq!("é".chars().count(), 1);
    }

    #[test]
    fn numeric_boundaries_keep_exact_values() {
        let here = site();
        assert_eq!(
            signed_to_f64(1i128 << 53, "int64", here),
            (1i64 << 53) as f64
        );
        assert!(exact_signed_f64((1i128 << 53) + 1).is_none());
        assert_eq!(
            signed_to_f32(1i128 << 24, "int32", here),
            (1i32 << 24) as f32
        );
        assert!(exact_signed_f64(1i128 << 24).is_some());
        assert!(f64_to_f32((1i64 << 24) as f64, "flt64", "flt32", here) == (1i32 << 24) as f32);
        let too_wide = ((1i64 << 24) as f64) + 1.0;
        assert!(too_wide as f32 as f64 != too_wide);
        assert_eq!(
            f64_to_signed(
                12.0,
                i32::MIN as i128,
                i32::MAX as i128,
                "flt64",
                "int32",
                here
            ),
            12
        );
        assert!(integral_i128(12.5).is_none());
        assert!(integral_i128(f64::NAN).is_none());
        assert!(integral_i128(f64::INFINITY).is_none());
        assert!(exact_signed_f64(i64::MAX as i128).is_none());
        assert_eq!(signed_to_bool(0, "int64", here), false);
        assert!(signed_to_bool(1, "int64", here));
        assert_eq!(code_to_char_signed(0x41, "int64", here), 'A');
        assert_eq!(char_to_code('é'), 'é' as u32);
    }
}
