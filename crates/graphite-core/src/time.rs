//! `grf.time` — current time, parsing, formatting, duration helpers, and sleep.
//!
//! Graphite sees only `str` / `int` / `flt` / `bln` / `async<void>`. Chrono
//! types stay private. Rust paths for keyword Graphite names (`int`, `flt`,
//! `str`) use aliases (`current_int`, `current_flt`, `current_str`).

use std::time::Duration;

use chrono::{Datelike, Days, Local, Months, NaiveDate, NaiveTime, Timelike, Utc, Weekday};

use crate::diagnostic::{self, RuntimeError};

const DATE_FMT: &str = "%Y-%m-%d";
const CLOCK_FMT: &str = "%H:%M:%S";
const CLOCK_MS_FMT: &str = "%H:%M:%S%.3f";

// --- current values ---------------------------------------------------------

/// Graphite `time.flt()` — Unix epoch milliseconds with fractional precision.
pub fn current_flt() -> Result<f64, RuntimeError> {
    Ok(system_epoch_ms_f64())
}

/// Graphite `time.int()` — Unix epoch milliseconds, truncated toward zero.
pub fn current_int() -> Result<i64, RuntimeError> {
    Ok(system_epoch_ms_f64().trunc() as i64)
}

/// Graphite `time.str()` — human-readable local date and time.
pub fn current_str() -> Result<String, RuntimeError> {
    Ok(Local::now().format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Graphite `time.iso()` — current UTC instant as ISO 8601 with `Z`.
pub fn iso_now() -> Result<String, RuntimeError> {
    Ok(format_iso_utc(Utc::now()))
}

/// Graphite `time.today()` — local calendar date `yyyy-MM-dd`.
pub fn today() -> Result<String, RuntimeError> {
    Ok(Local::now().date_naive().format(DATE_FMT).to_string())
}

/// Graphite `time.now()` — local clock time `HH:mm:ss`.
pub fn now() -> Result<String, RuntimeError> {
    Ok(Local::now().time().format(CLOCK_FMT).to_string())
}

// --- parsing ----------------------------------------------------------------

pub fn parse_date(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    Ok(date.format(DATE_FMT).to_string())
}

pub fn parse_clock_time(time_text: String) -> Result<String, RuntimeError> {
    let (time, has_ms) = parse_clock(&time_text)?;
    Ok(format_clock(time, has_ms))
}

pub fn parse_iso(iso_text: String) -> Result<String, RuntimeError> {
    let dt = parse_absolute_iso(&iso_text)?;
    Ok(format_iso_utc(dt))
}

pub fn parse_int(iso_text: String) -> Result<i64, RuntimeError> {
    let dt = parse_absolute_iso(&iso_text)?;
    Ok(datetime_to_epoch_ms_f64(dt).trunc() as i64)
}

pub fn parse_flt(iso_text: String) -> Result<f64, RuntimeError> {
    let dt = parse_absolute_iso(&iso_text)?;
    Ok(datetime_to_epoch_ms_f64(dt))
}

// --- formatting / conversion ------------------------------------------------

pub fn format(value: String, pattern: String) -> Result<String, RuntimeError> {
    format_value(&value, &pattern)
}

pub fn format_date(date_text: String) -> Result<String, RuntimeError> {
    parse_date(date_text)
}

pub fn format_clock_time(time_text: String) -> Result<String, RuntimeError> {
    parse_clock_time(time_text)
}

/// Graphite `time.fromMs` / `isoFromMs`.
pub fn iso_from_ms(ms: i64) -> Result<String, RuntimeError> {
    from_flt(ms as f64)
}

/// Graphite `time.fromFlt` / `isoFromFlt`.
pub fn from_flt(ms: f64) -> Result<String, RuntimeError> {
    if !ms.is_finite() {
        return Err(fail("timestamp must be a finite number"));
    }
    let dt = epoch_ms_to_utc(ms)?;
    Ok(format_iso_utc(dt))
}

pub fn iso_from_flt(ms: f64) -> Result<String, RuntimeError> {
    from_flt(ms)
}

/// Graphite `time.toMs` — same as `parseFlt` (timestamp → fractional Unix ms).
pub fn to_ms(iso_text: String) -> Result<f64, RuntimeError> {
    parse_flt(iso_text)
}

// --- clock components -------------------------------------------------------

pub fn hour(time_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_clock(&time_text)?.0.hour() as i64)
}

pub fn minute(time_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_clock(&time_text)?.0.minute() as i64)
}

pub fn second(time_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_clock(&time_text)?.0.second() as i64)
}

pub fn millisecond(time_text: String) -> Result<i64, RuntimeError> {
    Ok((parse_clock(&time_text)?.0.nanosecond() / 1_000_000) as i64)
}

// --- date components / predicates -------------------------------------------

pub fn year(date_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_iso_date(&date_text)?.year() as i64)
}

pub fn month(date_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_iso_date(&date_text)?.month() as i64)
}

pub fn day(date_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_iso_date(&date_text)?.day() as i64)
}

pub fn day_of_week(date_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_iso_date(&date_text)?.weekday().number_from_monday() as i64)
}

pub fn day_of_year(date_text: String) -> Result<i64, RuntimeError> {
    Ok(parse_iso_date(&date_text)?.ordinal() as i64)
}

pub fn days_in_month(date_text: String) -> Result<i64, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    Ok(days_in_month_of(date.year(), date.month()) as i64)
}

pub fn is_leap_year(year: i64) -> Result<bool, RuntimeError> {
    let year = i32::try_from(year).map_err(|_| fail("year out of range"))?;
    Ok(NaiveDate::from_ymd_opt(year, 2, 29).is_some())
}

pub fn is_weekday(date_text: String) -> Result<bool, RuntimeError> {
    let wd = parse_iso_date(&date_text)?.weekday().number_from_monday();
    Ok((1..=5).contains(&wd))
}

pub fn is_weekend(date_text: String) -> Result<bool, RuntimeError> {
    Ok(!is_weekday(date_text)?)
}

pub fn to_day(date_text: String) -> Result<String, RuntimeError> {
    Ok(weekday_full(parse_iso_date(&date_text)?))
}

pub fn to_day_short(date_text: String) -> Result<String, RuntimeError> {
    Ok(weekday_short(parse_iso_date(&date_text)?))
}

pub fn to_month(date_text: String) -> Result<String, RuntimeError> {
    Ok(month_full(parse_iso_date(&date_text)?.month()))
}

pub fn to_month_short(date_text: String) -> Result<String, RuntimeError> {
    Ok(month_short(parse_iso_date(&date_text)?.month()))
}

pub fn is_today(date_text: String) -> Result<bool, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    Ok(date == Local::now().date_naive())
}

pub fn is_first_day_of_month(date_text: String) -> Result<bool, RuntimeError> {
    Ok(parse_iso_date(&date_text)?.day() == 1)
}

pub fn is_last_day_of_month(date_text: String) -> Result<bool, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    Ok(date.day() == days_in_month_of(date.year(), date.month()))
}

pub fn is_same_day(a: String, b: String) -> Result<bool, RuntimeError> {
    Ok(parse_iso_date(&a)? == parse_iso_date(&b)?)
}

pub fn is_same_month(a: String, b: String) -> Result<bool, RuntimeError> {
    let left = parse_iso_date(&a)?;
    let right = parse_iso_date(&b)?;
    Ok(left.year() == right.year() && left.month() == right.month())
}

pub fn is_same_year(a: String, b: String) -> Result<bool, RuntimeError> {
    Ok(parse_iso_date(&a)?.year() == parse_iso_date(&b)?.year())
}

// --- calendar boundaries / relative / arithmetic ----------------------------

pub fn start_of_week(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let offset = (date.weekday().number_from_monday() - 1) as u64;
    let start = date
        .checked_sub_days(Days::new(offset))
        .ok_or_else(|| fail("date out of range"))?;
    Ok(start.format(DATE_FMT).to_string())
}

pub fn end_of_week(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let offset = (7 - date.weekday().number_from_monday()) as u64;
    let end = date
        .checked_add_days(Days::new(offset))
        .ok_or_else(|| fail("date out of range"))?;
    Ok(end.format(DATE_FMT).to_string())
}

pub fn start_of_month(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
        .ok_or_else(|| fail("date out of range"))?;
    Ok(start.format(DATE_FMT).to_string())
}

pub fn end_of_month(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let last = days_in_month_of(date.year(), date.month());
    let end = NaiveDate::from_ymd_opt(date.year(), date.month(), last)
        .ok_or_else(|| fail("date out of range"))?;
    Ok(end.format(DATE_FMT).to_string())
}

pub fn start_of_year(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let start =
        NaiveDate::from_ymd_opt(date.year(), 1, 1).ok_or_else(|| fail("date out of range"))?;
    Ok(start.format(DATE_FMT).to_string())
}

pub fn end_of_year(date_text: String) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let end =
        NaiveDate::from_ymd_opt(date.year(), 12, 31).ok_or_else(|| fail("date out of range"))?;
    Ok(end.format(DATE_FMT).to_string())
}

pub fn tomorrow() -> Result<String, RuntimeError> {
    add_days(today()?, 1)
}

pub fn yesterday() -> Result<String, RuntimeError> {
    add_days(today()?, -1)
}

pub fn next_monday(date_text: String) -> Result<String, RuntimeError> {
    next_weekday(date_text, Weekday::Mon)
}

pub fn previous_monday(date_text: String) -> Result<String, RuntimeError> {
    previous_weekday(date_text, Weekday::Mon)
}

pub fn add_days(date_text: String, amount: i64) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let result = if amount >= 0 {
        date.checked_add_days(Days::new(amount as u64))
    } else {
        date.checked_sub_days(Days::new((-amount) as u64))
    }
    .ok_or_else(|| fail("date out of range"))?;
    Ok(result.format(DATE_FMT).to_string())
}

pub fn add_weeks(date_text: String, weeks: i64) -> Result<String, RuntimeError> {
    add_days(date_text, weeks.checked_mul(7).ok_or_else(|| fail("week overflow"))?)
}

pub fn add_months(date_text: String, months: i64) -> Result<String, RuntimeError> {
    let date = parse_iso_date(&date_text)?;
    let result = if months >= 0 {
        let n = u32::try_from(months).map_err(|_| fail("month count out of range"))?;
        date.checked_add_months(Months::new(n))
    } else {
        let n = u32::try_from(-months).map_err(|_| fail("month count out of range"))?;
        date.checked_sub_months(Months::new(n))
    }
    .ok_or_else(|| fail("date out of range"))?;
    Ok(result.format(DATE_FMT).to_string())
}

pub fn add_years(date_text: String, years: i64) -> Result<String, RuntimeError> {
    let months = years
        .checked_mul(12)
        .ok_or_else(|| fail("year overflow"))?;
    add_months(date_text, months)
}

pub fn days_between(start: String, end: String) -> Result<i64, RuntimeError> {
    let start = parse_iso_date(&start)?;
    let end = parse_iso_date(&end)?;
    Ok((end - start).num_days())
}

// --- duration helpers (milliseconds as flt) ---------------------------------

fn require_finite(n: f64, label: &str) -> Result<f64, RuntimeError> {
    if !n.is_finite() {
        return Err(fail(format!("{label} must be a finite number")));
    }
    Ok(n)
}

/// Graphite `time.ms` — identity on millisecond durations.
pub fn ms(n: f64) -> Result<f64, RuntimeError> {
    require_finite(n, "duration")
}

/// Alias kept for earlier V1 spelling.
pub fn millis(n: f64) -> Result<f64, RuntimeError> {
    ms(n)
}

pub fn secs(n: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(n, "seconds")? * 1_000.0)
}

pub fn mins(n: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(n, "minutes")? * 60_000.0)
}

pub fn hrs(n: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(n, "hours")? * 3_600_000.0)
}

/// Fixed 24-hour duration in milliseconds — not calendar-day arithmetic.
pub fn days(n: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(n, "days")? * 86_400_000.0)
}

pub fn to_secs(ms: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(ms, "duration")? / 1_000.0)
}

pub fn to_mins(ms: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(ms, "duration")? / 60_000.0)
}

pub fn to_hrs(ms: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(ms, "duration")? / 3_600_000.0)
}

pub fn to_days(ms: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(ms, "duration")? / 86_400_000.0)
}

pub fn elapsed_ms(start_ms: f64, end_ms: f64) -> Result<f64, RuntimeError> {
    Ok(require_finite(end_ms, "end")? - require_finite(start_ms, "start")?)
}

pub fn elapsed_secs(start_ms: f64, end_ms: f64) -> Result<f64, RuntimeError> {
    to_secs(elapsed_ms(start_ms, end_ms)?)
}

pub fn elapsed_mins(start_ms: f64, end_ms: f64) -> Result<f64, RuntimeError> {
    to_mins(elapsed_ms(start_ms, end_ms)?)
}

pub fn elapsed_hrs(start_ms: f64, end_ms: f64) -> Result<f64, RuntimeError> {
    to_hrs(elapsed_ms(start_ms, end_ms)?)
}

// --- async sleep ------------------------------------------------------------

/// Graphite `time.sleep(ms)` — non-blocking sleep for `ms` milliseconds.
pub async fn sleep(ms: f64) -> Result<(), RuntimeError> {
    sleep_ms(ms).await
}

/// Graphite `time.sleepMs(ms)` — convenience alias for millisecond sleeps.
pub async fn sleep_ms(ms: f64) -> Result<(), RuntimeError> {
    let ms = require_finite(ms, "sleep duration")?;
    if ms < 0.0 {
        return Err(fail("sleep duration cannot be negative"));
    }
    tokio::time::sleep(Duration::from_secs_f64(ms / 1_000.0)).await;
    Ok(())
}

// --- internals --------------------------------------------------------------

fn fail(detail: impl Into<String>) -> RuntimeError {
    diagnostic::time_invalid(detail)
}

fn days_in_month_of(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid month");
    let last = first_next
        .pred_opt()
        .expect("day before first of next month");
    last.day()
}

fn next_weekday(date_text: String, target: Weekday) -> Result<String, RuntimeError> {
    let mut date = parse_iso_date(&date_text)?
        .checked_add_days(Days::new(1))
        .ok_or_else(|| fail("date out of range"))?;
    while date.weekday() != target {
        date = date
            .checked_add_days(Days::new(1))
            .ok_or_else(|| fail("date out of range"))?;
    }
    Ok(date.format(DATE_FMT).to_string())
}

fn previous_weekday(date_text: String, target: Weekday) -> Result<String, RuntimeError> {
    let mut date = parse_iso_date(&date_text)?
        .checked_sub_days(Days::new(1))
        .ok_or_else(|| fail("date out of range"))?;
    while date.weekday() != target {
        date = date
            .checked_sub_days(Days::new(1))
            .ok_or_else(|| fail("date out of range"))?;
    }
    Ok(date.format(DATE_FMT).to_string())
}

fn system_epoch_ms_f64() -> f64 {
    datetime_to_epoch_ms_f64(Utc::now())
}

fn datetime_to_epoch_ms_f64(dt: chrono::DateTime<Utc>) -> f64 {
    let secs = dt.timestamp() as f64;
    let nanos = dt.timestamp_subsec_nanos() as f64;
    secs * 1_000.0 + nanos / 1_000_000.0
}

fn epoch_ms_to_utc(ms: f64) -> Result<chrono::DateTime<Utc>, RuntimeError> {
    if !ms.is_finite() {
        return Err(fail("timestamp must be a finite number"));
    }
    let secs = (ms / 1_000.0).trunc() as i64;
    let rem_ms = ms - (secs as f64) * 1_000.0;
    let nanos = (rem_ms * 1_000_000.0).round() as u32;
    chrono::DateTime::from_timestamp(secs, nanos.min(999_999_999))
        .ok_or_else(|| fail("timestamp out of range"))
}

fn format_iso_utc(dt: chrono::DateTime<Utc>) -> String {
    let ms = dt.timestamp_subsec_millis();
    if ms == 0 {
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    } else {
        dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }
}

fn parse_iso_date(text: &str) -> Result<NaiveDate, RuntimeError> {
    NaiveDate::parse_from_str(text.trim(), DATE_FMT)
        .map_err(|_| fail(format!("invalid date '{text}', expected yyyy-MM-dd")))
}

fn parse_clock(text: &str) -> Result<(NaiveTime, bool), RuntimeError> {
    let text = text.trim();
    if let Ok(time) = NaiveTime::parse_from_str(text, CLOCK_MS_FMT) {
        return Ok((time, true));
    }
    if let Ok(time) = NaiveTime::parse_from_str(text, "%H:%M:%S%.f") {
        return Ok((time, time.nanosecond() != 0));
    }
    if let Ok(time) = NaiveTime::parse_from_str(text, CLOCK_FMT) {
        return Ok((time, false));
    }
    Err(fail(format!(
        "invalid clock time '{text}', expected HH:mm:ss or HH:mm:ss.SSS"
    )))
}

fn format_clock(time: NaiveTime, has_ms: bool) -> String {
    if has_ms || time.nanosecond() != 0 {
        time.format(CLOCK_MS_FMT).to_string()
    } else {
        time.format(CLOCK_FMT).to_string()
    }
}

fn parse_absolute_iso(text: &str) -> Result<chrono::DateTime<Utc>, RuntimeError> {
    let text = text.trim();
    // Offset-less timestamps must not silently become UTC (LLD §7.1).
    if !has_explicit_offset(text) {
        return Err(fail(format!(
            "invalid ISO timestamp '{text}'; an explicit timezone or offset is required"
        )));
    }
    chrono::DateTime::parse_from_rfc3339(text)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f%z")
                .or_else(|_| chrono::DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%z"))
                .map(|dt| dt.with_timezone(&Utc))
        })
        .map_err(|_| {
            fail(format!(
                "invalid ISO timestamp '{text}'; an explicit timezone or offset is required"
            ))
        })
}

fn has_explicit_offset(text: &str) -> bool {
    if text.ends_with('Z') || text.ends_with('z') {
        return true;
    }
    let Some(t_pos) = text.find('T').or_else(|| text.find('t')) else {
        return false;
    };
    let after = &text[t_pos + 1..];
    after.contains('+') || after.rfind('-').is_some_and(|i| i > 0)
}

fn format_value(value: &str, pattern: &str) -> Result<String, RuntimeError> {
    let value = value.trim();
    if let Ok(date) = parse_iso_date(value) {
        return apply_pattern(pattern, Some(date), None, None);
    }
    if let Ok((time, _)) = parse_clock(value) {
        return apply_pattern(pattern, None, Some(time), None);
    }
    if let Ok(dt) = parse_absolute_iso(value) {
        let local_date = dt.date_naive();
        let local_time = dt.time();
        return apply_pattern(pattern, Some(local_date), Some(local_time), Some(dt));
    }
    Err(fail(format!(
        "cannot format '{value}'; expected a date, clock time, or ISO timestamp"
    )))
}

fn apply_pattern(
    pattern: &str,
    date: Option<NaiveDate>,
    time: Option<NaiveTime>,
    utc: Option<chrono::DateTime<Utc>>,
) -> Result<String, RuntimeError> {
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if i + 1 < chars.len() && chars[i + 1] == '\'' {
                        out.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }

        let start = i;
        let ch = chars[i];
        while i < chars.len() && chars[i] == ch && is_pattern_letter(ch) {
            i += 1;
        }
        let len = i - start;
        if len == 0 {
            out.push(ch);
            i += 1;
            continue;
        }
        let token: String = chars[start..i].iter().collect();
        out.push_str(&expand_token(&token, date, time, utc)?);
    }
    Ok(out)
}

fn is_pattern_letter(ch: char) -> bool {
    matches!(ch, 'y' | 'M' | 'd' | 'E' | 'H' | 'h' | 'm' | 's' | 'S' | 'a')
}

fn expand_token(
    token: &str,
    date: Option<NaiveDate>,
    time: Option<NaiveTime>,
    _utc: Option<chrono::DateTime<Utc>>,
) -> Result<String, RuntimeError> {
    let need_date = matches!(
        token,
        "yyyy" | "MM" | "MMM" | "MMMM" | "dd" | "EEEE" | "EEE"
    );
    let need_time = matches!(token, "HH" | "hh" | "mm" | "ss" | "SSS" | "a");
    if need_date && date.is_none() {
        return Err(fail(format!(
            "format token '{token}' requires a date component"
        )));
    }
    if need_time && time.is_none() {
        return Err(fail(format!(
            "format token '{token}' requires a clock-time component"
        )));
    }
    let date = date.unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
    let time = time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    Ok(match token {
        "yyyy" => format!("{:04}", date.year()),
        "MM" => format!("{:02}", date.month()),
        "MMM" => month_short(date.month()),
        "MMMM" => month_full(date.month()),
        "dd" => format!("{:02}", date.day()),
        "EEEE" => weekday_full(date),
        "EEE" => weekday_short(date),
        "HH" => format!("{:02}", time.hour()),
        "hh" => {
            let h = time.hour12();
            format!("{:02}", if h.1 == 0 { 12 } else { h.1 })
        }
        "mm" => format!("{:02}", time.minute()),
        "ss" => format!("{:02}", time.second()),
        "SSS" => format!("{:03}", time.nanosecond() / 1_000_000),
        "a" => {
            if time.hour12().0 {
                "PM".to_string()
            } else {
                "AM".to_string()
            }
        }
        other => {
            return Err(fail(format!("unsupported format token '{other}'")));
        }
    })
}

fn month_full(month: u32) -> String {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
    .to_string()
}

fn month_short(month: u32) -> String {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "Unk",
    }
    .to_string()
}

fn weekday_full(date: NaiveDate) -> String {
    use chrono::Weekday::*;
    match date.weekday() {
        Mon => "Monday",
        Tue => "Tuesday",
        Wed => "Wednesday",
        Thu => "Thursday",
        Fri => "Friday",
        Sat => "Saturday",
        Sun => "Sunday",
    }
    .to_string()
}

fn weekday_short(date: NaiveDate) -> String {
    use chrono::Weekday::*;
    match date.weekday() {
        Mon => "Mon",
        Tue => "Tue",
        Wed => "Wed",
        Thu => "Thu",
        Fri => "Fri",
        Sat => "Sat",
        Sun => "Sun",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_helpers_use_milliseconds() {
        assert_eq!(secs(1.0).unwrap(), 1_000.0);
        assert_eq!(mins(1.0).unwrap(), 60_000.0);
        assert_eq!(hrs(1.0).unwrap(), 3_600_000.0);
        assert_eq!(days(1.0).unwrap(), 86_400_000.0);
        assert_eq!(secs(1.5).unwrap(), 1_500.0);
        assert_eq!(to_secs(500.0).unwrap(), 0.5);
        assert_eq!(to_secs(1500.5).unwrap(), 1.5005);
        assert_eq!(elapsed_ms(100.0, 250.5).unwrap(), 150.5);
    }

    #[test]
    fn days_is_fixed_24_hours_not_calendar() {
        assert_eq!(days(1.0).unwrap(), hrs(24.0).unwrap());
    }

    #[test]
    fn calendar_components_and_arithmetic() {
        assert_eq!(year("2026-10-26".into()).unwrap(), 2026);
        assert_eq!(month("2026-10-26".into()).unwrap(), 10);
        assert_eq!(day("2026-10-26".into()).unwrap(), 26);
        assert_eq!(day_of_week("2026-10-26".into()).unwrap(), 1);
        assert_eq!(to_day("2026-10-26".into()).unwrap(), "Monday");
        assert_eq!(to_month("2026-10-26".into()).unwrap(), "October");
        assert!(is_weekday("2026-10-26".into()).unwrap());
        assert!(!is_weekend("2026-10-26".into()).unwrap());
        assert_eq!(add_days("2026-10-26".into(), 1).unwrap(), "2026-10-27");
        assert_eq!(add_months("2026-01-31".into(), 1).unwrap(), "2026-02-28");
        assert_eq!(add_months("2024-01-31".into(), 1).unwrap(), "2024-02-29");
        assert_eq!(days_between("2026-10-01".into(), "2026-10-26".into()).unwrap(), 25);
        assert_eq!(start_of_week("2026-10-26".into()).unwrap(), "2026-10-26");
        assert_eq!(end_of_week("2026-10-26".into()).unwrap(), "2026-11-01");
        assert_eq!(next_monday("2026-10-26".into()).unwrap(), "2026-11-02");
    }

    #[test]
    fn parse_and_format_round_trip_date() {
        let parsed = parse_date("2026-10-26".into()).unwrap();
        assert_eq!(parsed, "2026-10-26");
        assert_eq!(
            format(parsed, "EEEE, dd MMMM yyyy".into()).unwrap(),
            "Monday, 26 October 2026"
        );
    }

    #[test]
    fn iso_requires_timezone() {
        let err = parse_iso("2026-10-26T14:30:00".into()).unwrap_err();
        assert!(err.label.contains("timezone") || err.label.contains("offset"), "{err:?}");
    }

    #[test]
    fn iso_z_parses() {
        let ms = parse_int("2026-10-26T14:30:00Z".into()).unwrap();
        let back = iso_from_ms(ms).unwrap();
        assert!(back.starts_with("2026-10-26T14:30:00"), "{back}");
        assert!(back.ends_with('Z'), "{back}");
    }

    #[test]
    fn invalid_date_is_rejected() {
        assert!(parse_date("2026-02-30".into()).is_err());
    }

    #[test]
    fn clock_components() {
        assert_eq!(hour("14:30:00".into()).unwrap(), 14);
        assert_eq!(minute("14:30:00".into()).unwrap(), 30);
        assert_eq!(second("14:30:05".into()).unwrap(), 5);
        assert_eq!(millisecond("14:30:00.250".into()).unwrap(), 250);
        assert_eq!(millisecond("14:30:00".into()).unwrap(), 0);
    }
}
