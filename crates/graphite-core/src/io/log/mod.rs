//! `grf.io.log` — extensible logging with optional typed structured fields.

mod format;
mod value;

use std::io::{self, IsTerminal, Write};
use std::sync::{Mutex, OnceLock};

use chrono::Local;

use crate::diagnostic::{self, RuntimeError};

pub use value::{LogObject, LogValue};

use format::write_event;

static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One logical log record shared by console and file writers.
#[derive(Debug, Clone)]
pub struct LogEvent {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub fields: LogObject,
}

pub fn info(message: String, save_to: String) -> Result<(), RuntimeError> {
    emit("INFO", &message, Color::Named(NamedColor::Info), LogObject::new(), &save_to)
}

pub fn debug(message: String, save_to: String) -> Result<(), RuntimeError> {
    emit("DEBUG", &message, Color::Named(NamedColor::Debug), LogObject::new(), &save_to)
}

pub fn warn(message: String, save_to: String) -> Result<(), RuntimeError> {
    emit("WARN", &message, Color::Named(NamedColor::Warn), LogObject::new(), &save_to)
}

pub fn error(message: String, save_to: String) -> Result<(), RuntimeError> {
    emit("ERROR", &message, Color::Named(NamedColor::Error), LogObject::new(), &save_to)
}

pub fn fatal(message: String, save_to: String) -> Result<(), RuntimeError> {
    emit("FATAL", &message, Color::Named(NamedColor::Fatal), LogObject::new(), &save_to)
}

pub fn checkpoint(message: String, save_to: String) -> Result<(), RuntimeError> {
    emit(
        "CHECKPOINT",
        &message,
        Color::Named(NamedColor::Checkpoint),
        LogObject::new(),
        &save_to,
    )
}

pub fn custom(
    message: String,
    level: String,
    color: String,
    save_to: String,
) -> Result<(), RuntimeError> {
    let level = normalize_level(&level)?;
    let rgb = parse_rgb(&color)?;
    emit(&level, &message, Color::Rgb(rgb), LogObject::new(), &save_to)
}

pub fn info_structured(
    message: String,
    fields: LogObject,
    save_to: String,
) -> Result<(), RuntimeError> {
    emit("INFO", &message, Color::Named(NamedColor::Info), fields, &save_to)
}

pub fn debug_structured(
    message: String,
    fields: LogObject,
    save_to: String,
) -> Result<(), RuntimeError> {
    emit("DEBUG", &message, Color::Named(NamedColor::Debug), fields, &save_to)
}

pub fn warn_structured(
    message: String,
    fields: LogObject,
    save_to: String,
) -> Result<(), RuntimeError> {
    emit("WARN", &message, Color::Named(NamedColor::Warn), fields, &save_to)
}

pub fn error_structured(
    message: String,
    fields: LogObject,
    save_to: String,
) -> Result<(), RuntimeError> {
    emit("ERROR", &message, Color::Named(NamedColor::Error), fields, &save_to)
}

pub fn fatal_structured(
    message: String,
    fields: LogObject,
    save_to: String,
) -> Result<(), RuntimeError> {
    emit("FATAL", &message, Color::Named(NamedColor::Fatal), fields, &save_to)
}

pub fn checkpoint_structured(
    message: String,
    fields: LogObject,
    save_to: String,
) -> Result<(), RuntimeError> {
    emit(
        "CHECKPOINT",
        &message,
        Color::Named(NamedColor::Checkpoint),
        fields,
        &save_to,
    )
}

pub fn custom_structured(
    message: String,
    level: String,
    fields: LogObject,
    color: String,
    save_to: String,
) -> Result<(), RuntimeError> {
    let level = normalize_level(&level)?;
    let rgb = parse_rgb(&color)?;
    emit(&level, &message, Color::Rgb(rgb), fields, &save_to)
}

fn emit(
    level: &str,
    message: &str,
    color: Color,
    fields: LogObject,
    save_to: &str,
) -> Result<(), RuntimeError> {
    reject_reserved_fields(&fields)?;
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let safe_message = sanitize_message(message);
    let event = LogEvent {
        timestamp: timestamp.clone(),
        level: level.to_string(),
        message: safe_message.clone(),
        fields,
    };
    let _guard = lock();

    let colorize = io::stdout().is_terminal();
    let console = format_console(&event, color, colorize);
    write_console(&console).map_err(fail_io)?;

    if !save_to.is_empty() {
        write_event(save_to, &event)?;
    }
    Ok(())
}

fn reject_reserved_fields(fields: &LogObject) -> Result<(), RuntimeError> {
    for key in fields.keys() {
        if matches!(key, "timestamp" | "level" | "message") {
            return Err(fail(format!(
                "structured log field '{key}' conflicts with standard log metadata"
            )));
        }
    }
    Ok(())
}

fn format_console(event: &LogEvent, color: Color, colorize: bool) -> String {
    let header = if colorize {
        let ts = "\u{1b}[96m";
        let reset = "\u{1b}[0m";
        let msg = "\u{1b}[97m";
        let level_ansi = color.ansi();
        format!(
            "{ts}{}{reset} {level_ansi}[{}]{reset} {msg}{}{reset}",
            event.timestamp, event.level, event.message
        )
    } else {
        format!(
            "{} [{}] {}",
            event.timestamp, event.level, event.message
        )
    };
    if event.fields.fields.is_empty() {
        return header;
    }
    // Pretty JSON on the next line; ANSI colors only on a TTY. File writers stay compact.
    let fields = if colorize {
        event.fields.to_colored_pretty_json()
    } else {
        event.fields.to_pretty_json()
    };
    format!("{header}\n{fields}")
}

fn sanitize_message(message: &str) -> String {
    message.replace('\r', "\\r").replace('\n', "\\n")
}

fn normalize_level(level: &str) -> Result<String, RuntimeError> {
    if level.is_empty() {
        return Err(fail("custom log level must not be empty"));
    }
    if level.chars().any(|ch| ch == '\n' || ch == '\r') {
        return Err(fail("custom log level must not contain line breaks"));
    }
    if level.chars().any(|ch| ch.is_control()) {
        return Err(fail("custom log level must not contain control characters"));
    }
    Ok(level.to_ascii_uppercase())
}

fn parse_rgb(color: &str) -> Result<(u8, u8, u8), RuntimeError> {
    let rest = color
        .strip_prefix('#')
        .ok_or_else(|| fail(format!("invalid log color '{color}'; expected #RRGGBB")))?;
    if rest.len() != 6 || !rest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(fail(format!(
            "invalid log color '{color}'; expected #RRGGBB"
        )));
    }
    let r = u8::from_str_radix(&rest[0..2], 16).map_err(|_| fail("invalid log color"))?;
    let g = u8::from_str_radix(&rest[2..4], 16).map_err(|_| fail("invalid log color"))?;
    let b = u8::from_str_radix(&rest[4..6], 16).map_err(|_| fail("invalid log color"))?;
    Ok((r, g, b))
}

fn write_console(line: &str) -> io::Result<()> {
    let mut out = io::stdout().lock();
    writeln!(out, "{line}")?;
    out.flush()
}

pub(crate) fn fail(detail: impl Into<String>) -> RuntimeError {
    diagnostic::external_library(detail)
}

pub(crate) fn fail_io(error: io::Error) -> RuntimeError {
    diagnostic::input_output(error.to_string())
}

#[derive(Clone, Copy)]
enum Color {
    Named(NamedColor),
    Rgb((u8, u8, u8)),
}

#[derive(Clone, Copy)]
enum NamedColor {
    Info,
    Debug,
    Warn,
    Error,
    Fatal,
    Checkpoint,
}

impl Color {
    fn ansi(self) -> String {
        match self {
            Color::Named(NamedColor::Info) => "\u{1b}[1;94m".to_string(),
            Color::Named(NamedColor::Debug) => "\u{1b}[1;90m".to_string(),
            Color::Named(NamedColor::Warn) => "\u{1b}[1;93m".to_string(),
            Color::Named(NamedColor::Error) => "\u{1b}[1;91m".to_string(),
            Color::Named(NamedColor::Fatal) => "\u{1b}[1;97;41m".to_string(),
            Color::Named(NamedColor::Checkpoint) => "\u{1b}[1;96m".to_string(),
            Color::Rgb((r, g, b)) => format!("\u{1b}[1;38;2;{r};{g};{b}m"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn message_only_still_works() {
        info("console only".into(), String::new()).unwrap();
    }

    #[test]
    fn structured_jsonl_roundtrip_shape() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("grf_log_struct_{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.jsonl");
        let mut fields = LogObject::new();
        fields.insert("duration", LogValue::Flt(12.33));
        fields.insert("error", LogValue::str("log failed"));
        warn_structured(
            "Operation failed".into(),
            fields,
            path.to_string_lossy().into(),
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"duration\":12.33"));
        assert!(text.contains("\"error\":\"log failed\""));
        assert!(!text.contains('\u{1b}'));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reserved_field_names_are_rejected() {
        let mut fields = LogObject::new();
        fields.insert("message", LogValue::str("nope"));
        assert!(info_structured("x".into(), fields, String::new()).is_err());
    }
}
