//! Format detection and writers for structured log events.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::value::{LogObject, csv_escape, json_string};
use super::{LogEvent, fail, fail_io};
use crate::diagnostic::RuntimeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    Log,
    Txt,
    Json,
    Jsonl,
    Csv,
}

pub fn detect_format(path: &str) -> Result<FileFormat, RuntimeError> {
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "log" => Ok(FileFormat::Log),
        "txt" => Ok(FileFormat::Txt),
        "json" => Ok(FileFormat::Json),
        "jsonl" => Ok(FileFormat::Jsonl),
        "csv" => Ok(FileFormat::Csv),
        "" => Err(fail(format!(
            "log destination '{path}' has no file extension; use .log, .txt, .json, .jsonl, or .csv"
        ))),
        other => Err(fail(format!(
            "unsupported log file extension '.{other}'; use .log, .txt, .json, .jsonl, or .csv"
        ))),
    }
}

pub fn write_event(path: &str, event: &LogEvent) -> Result<(), RuntimeError> {
    let format = detect_format(path)?;
    ensure_parent(path)?;
    match format {
        FileFormat::Log | FileFormat::Txt => append_line(path, &event.to_text_record()),
        FileFormat::Jsonl => append_line(path, &event.to_json_object()),
        FileFormat::Json => append_json_array(path, &event.to_json_object()),
        FileFormat::Csv => append_csv(path, event),
    }
}

fn ensure_parent(path: &str) -> Result<(), RuntimeError> {
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                fail_io(std::io::Error::new(
                    error.kind(),
                    format!(
                        "cannot create log directory '{}': {error}",
                        parent.display()
                    ),
                ))
            })?;
        }
    }
    Ok(())
}

fn append_line(path: &str, line: &str) -> Result<(), RuntimeError> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| {
            fail_io(std::io::Error::new(
                error.kind(),
                format!("cannot open log file '{path}': {error}"),
            ))
        })?;
    writeln!(file, "{line}").map_err(|error| {
        fail_io(std::io::Error::new(
            error.kind(),
            format!("cannot write log file '{path}': {error}"),
        ))
    })?;
    file.flush().map_err(fail_io)
}

fn append_json_array(path: &str, object_json: &str) -> Result<(), RuntimeError> {
    let path_buf = Path::new(path);
    if !path_buf.exists() || fs::metadata(path_buf).map(|m| m.len()).unwrap_or(0) == 0 {
        let mut file = File::create(path).map_err(fail_io)?;
        write!(file, "[\n  {object_json}\n]\n").map_err(fail_io)?;
        return file.flush().map_err(fail_io);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(fail_io)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(fail_io)?;
    let trimmed = contents.trim_end();
    if !trimmed.ends_with(']') {
        return Err(fail(format!(
            "log file '{path}' is not a JSON array; cannot append a structured event"
        )));
    }
    let without_bracket = trimmed[..trimmed.len() - 1].trim_end();
    let body = if without_bracket.ends_with('[') {
        format!("{without_bracket}\n  {object_json}\n]\n")
    } else {
        format!("{without_bracket},\n  {object_json}\n]\n")
    };
    file.set_len(0).map_err(fail_io)?;
    file.seek(SeekFrom::Start(0)).map_err(fail_io)?;
    file.write_all(body.as_bytes()).map_err(fail_io)?;
    file.flush().map_err(fail_io)
}

fn append_csv(path: &str, event: &LogEvent) -> Result<(), RuntimeError> {
    let path_buf = Path::new(path);
    let header = csv_header(&event.fields);
    if !path_buf.exists() || fs::metadata(path_buf).map(|m| m.len()).unwrap_or(0) == 0 {
        append_line(path, &header)?;
        return append_line(path, &csv_row(event)?);
    }
    let existing = fs::read_to_string(path).map_err(fail_io)?;
    let first = existing.lines().next().unwrap_or("");
    if first != header {
        return Err(fail(format!(
            "CSV log schema mismatch for '{path}': existing header does not match this event's fields"
        )));
    }
    append_line(path, &csv_row(event)?)
}

fn csv_header(fields: &LogObject) -> String {
    let mut cols = vec![
        "timestamp".to_string(),
        "level".to_string(),
        "message".to_string(),
    ];
    cols.extend(fields.keys().map(str::to_string));
    cols.join(",")
}

fn csv_row(event: &LogEvent) -> Result<String, RuntimeError> {
    let mut cols = vec![
        csv_escape(&event.timestamp),
        csv_escape(&event.level),
        csv_escape(&event.message),
    ];
    for key in event.fields.keys() {
        let cell = match event.fields.get(key) {
            Some(value) => value.to_csv_cell().map_err(fail)?,
            None => String::new(),
        };
        cols.push(cell);
    }
    Ok(cols.join(","))
}

impl LogEvent {
    pub fn to_text_record(&self) -> String {
        let fields = self.fields.to_text();
        if fields.is_empty() {
            format!(
                "{} [{}] {}",
                self.timestamp, self.level, self.message
            )
        } else {
            format!(
                "{} [{}] {} {}",
                self.timestamp, self.level, self.message, fields
            )
        }
    }

    pub fn to_json_object(&self) -> String {
        let mut body = format!(
            "\"timestamp\":{},\"level\":{},\"message\":{}",
            json_string(&self.timestamp),
            json_string(&self.level),
            json_string(&self.message)
        );
        for (key, value) in &self.fields.fields {
            body.push(',');
            body.push_str(&json_string(key));
            body.push(':');
            body.push_str(&value.to_json());
        }
        format!("{{{body}}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::value::LogValue;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample(fields: LogObject) -> LogEvent {
        LogEvent {
            timestamp: "2026-10-28 00:15:01.125".into(),
            level: "INFO".into(),
            message: "Training progress".into(),
            fields,
        }
    }

    #[test]
    fn detect_supported_and_reject_unknown() {
        assert_eq!(detect_format("a.jsonl").unwrap(), FileFormat::Jsonl);
        assert!(detect_format("a.parquet").is_err());
    }

    #[test]
    fn jsonl_and_json_append() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("grf_log_fmt_{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        let mut fields = LogObject::new();
        fields.insert("epoch", LogValue::Int(50));
        fields.insert("loss", LogValue::Flt(0.024));
        let event = sample(fields);

        let jsonl = dir.join("a.jsonl");
        write_event(jsonl.to_str().unwrap(), &event).unwrap();
        write_event(jsonl.to_str().unwrap(), &event).unwrap();
        let text = fs::read_to_string(&jsonl).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().all(|line| line.starts_with('{')));

        let json = dir.join("a.json");
        write_event(json.to_str().unwrap(), &event).unwrap();
        write_event(json.to_str().unwrap(), &event).unwrap();
        let text = fs::read_to_string(&json).unwrap();
        assert!(text.trim_start().starts_with('['));
        assert!(text.trim_end().ends_with(']'));
        assert_eq!(text.matches("\"epoch\"").count(), 2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn csv_enforces_schema() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("grf_log_csv_{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.csv");
        let path_str = path.to_str().unwrap();

        let mut fields = LogObject::new();
        fields.insert("epoch", LogValue::Int(1));
        write_event(path_str, &sample(fields)).unwrap();

        let mut other = LogObject::new();
        other.insert("loss", LogValue::Flt(0.1));
        assert!(write_event(path_str, &sample(other)).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
