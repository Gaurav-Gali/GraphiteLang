//! Typed structured log fields. Distinct from standard metadata keys.

use std::collections::BTreeMap;
use std::fmt;

/// One structured field value. Numbers keep numeric JSON forms.
#[derive(Debug, Clone, PartialEq)]
pub enum LogValue {
    Str(String),
    Int(i64),
    UInt(u64),
    Flt(f64),
    Bln(bool),
    Null,
    Array(Vec<LogValue>),
    Object(LogObject),
}

/// User-defined fields for one log event. Insertion order is sorted by key for
/// stable JSON/CSV output.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogObject {
    pub fields: BTreeMap<String, LogValue>,
}

impl LogObject {
    pub fn new() -> Self {
        Self {
            fields: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: LogValue) {
        self.fields.insert(key.into(), value);
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, LogValue)>) -> Self {
        let mut object = Self::new();
        for (key, value) in pairs {
            object.insert(key, value);
        }
        object
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.fields.keys().map(String::as_str)
    }

    pub fn get(&self, key: &str) -> Option<&LogValue> {
        self.fields.get(key)
    }
}

impl LogValue {
    pub fn str(value: impl Into<String>) -> Self {
        Self::Str(value.into())
    }

    pub fn to_json(&self) -> String {
        match self {
            LogValue::Str(text) => json_string(text),
            LogValue::Int(value) => value.to_string(),
            LogValue::UInt(value) => value.to_string(),
            LogValue::Flt(value) => format_json_float(*value),
            LogValue::Bln(value) => {
                if *value {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            }
            LogValue::Null => "null".to_string(),
            LogValue::Array(items) => {
                let body = items
                    .iter()
                    .map(LogValue::to_json)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("[{body}]")
            }
            LogValue::Object(object) => object.to_json(),
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            LogValue::Str(text) => text.clone(),
            LogValue::Int(value) => value.to_string(),
            LogValue::UInt(value) => value.to_string(),
            LogValue::Flt(value) => format_json_float(*value),
            LogValue::Bln(value) => value.to_string(),
            LogValue::Null => "null".to_string(),
            LogValue::Array(items) => {
                let body = items
                    .iter()
                    .map(LogValue::to_text)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{body}]")
            }
            LogValue::Object(object) => object.to_text(),
        }
    }

    pub fn to_csv_cell(&self) -> Result<String, String> {
        match self {
            LogValue::Str(text) => Ok(csv_escape(text)),
            LogValue::Int(value) => Ok(value.to_string()),
            LogValue::UInt(value) => Ok(value.to_string()),
            LogValue::Flt(value) => Ok(format_json_float(*value)),
            LogValue::Bln(value) => Ok(value.to_string()),
            LogValue::Null => Ok(String::new()),
            LogValue::Array(_) | LogValue::Object(_) => Err(
                "CSV log fields cannot contain nested arrays or records; use .jsonl or .json"
                    .to_string(),
            ),
        }
    }
}

impl LogObject {
    pub fn to_json(&self) -> String {
        let body = self
            .fields
            .iter()
            .map(|(key, value)| format!("{}:{}", json_string(key), value.to_json()))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{body}}}")
    }

    pub fn to_text(&self) -> String {
        if self.fields.is_empty() {
            return String::new();
        }
        let body = self
            .fields
            .iter()
            .map(|(key, value)| format!("{key}={}", value.to_text()))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{{body}}}")
    }

    /// Pretty-printed JSON for console display (no ANSI).
    pub fn to_pretty_json(&self) -> String {
        self.to_pretty_json_indent(0, false)
    }

    /// Pretty-printed JSON with ANSI colors for TTY console output.
    pub fn to_colored_pretty_json(&self) -> String {
        self.to_pretty_json_indent(0, true)
    }

    fn to_pretty_json_indent(&self, depth: usize, color: bool) -> String {
        if self.fields.is_empty() {
            return "{}".to_string();
        }
        let reset = "\u{1b}[0m";
        let punct = "\u{1b}[90m"; // dim braces / commas / colons
        let key_c = "\u{1b}[96m"; // bright cyan keys
        let indent = "    ".repeat(depth);
        let inner = "    ".repeat(depth + 1);
        let mut out = String::new();
        if color {
            out.push_str(punct);
            out.push('{');
            out.push_str(reset);
        } else {
            out.push('{');
        }
        out.push('\n');
        let last = self.fields.len().saturating_sub(1);
        for (index, (key, value)) in self.fields.iter().enumerate() {
            out.push_str(&inner);
            if color {
                out.push_str(key_c);
                out.push_str(&json_string(key));
                out.push_str(reset);
                out.push_str(punct);
                out.push_str(": ");
                out.push_str(reset);
            } else {
                out.push_str(&json_string(key));
                out.push_str(": ");
            }
            out.push_str(&value.to_pretty_json_indent(depth + 1, color));
            if index != last {
                if color {
                    out.push_str(punct);
                    out.push(',');
                    out.push_str(reset);
                } else {
                    out.push(',');
                }
            }
            out.push('\n');
        }
        out.push_str(&indent);
        if color {
            out.push_str(punct);
            out.push('}');
            out.push_str(reset);
        } else {
            out.push('}');
        }
        out
    }
}

impl LogValue {
    fn to_pretty_json_indent(&self, depth: usize, color: bool) -> String {
        let reset = "\u{1b}[0m";
        let punct = "\u{1b}[90m";
        let str_c = "\u{1b}[92m"; // green strings
        let num_c = "\u{1b}[93m"; // yellow numbers
        let lit_c = "\u{1b}[95m"; // magenta true/false/null
        match self {
            LogValue::Str(text) => {
                let body = json_string(text);
                if color {
                    format!("{str_c}{body}{reset}")
                } else {
                    body
                }
            }
            LogValue::Int(value) => {
                let body = value.to_string();
                if color {
                    format!("{num_c}{body}{reset}")
                } else {
                    body
                }
            }
            LogValue::UInt(value) => {
                let body = value.to_string();
                if color {
                    format!("{num_c}{body}{reset}")
                } else {
                    body
                }
            }
            LogValue::Flt(value) => {
                let body = format_json_float(*value);
                if color {
                    format!("{num_c}{body}{reset}")
                } else {
                    body
                }
            }
            LogValue::Bln(value) => {
                let body = if *value { "true" } else { "false" };
                if color {
                    format!("{lit_c}{body}{reset}")
                } else {
                    body.to_string()
                }
            }
            LogValue::Null => {
                if color {
                    format!("{lit_c}null{reset}")
                } else {
                    "null".to_string()
                }
            }
            LogValue::Array(items) => {
                if items.is_empty() {
                    return "[]".to_string();
                }
                let indent = "    ".repeat(depth);
                let inner = "    ".repeat(depth + 1);
                let mut out = String::new();
                if color {
                    out.push_str(punct);
                    out.push('[');
                    out.push_str(reset);
                } else {
                    out.push('[');
                }
                out.push('\n');
                for (index, item) in items.iter().enumerate() {
                    out.push_str(&inner);
                    out.push_str(&item.to_pretty_json_indent(depth + 1, color));
                    if index + 1 != items.len() {
                        if color {
                            out.push_str(punct);
                            out.push(',');
                            out.push_str(reset);
                        } else {
                            out.push(',');
                        }
                    }
                    out.push('\n');
                }
                out.push_str(&indent);
                if color {
                    out.push_str(punct);
                    out.push(']');
                    out.push_str(reset);
                } else {
                    out.push(']');
                }
                out
            }
            LogValue::Object(object) => object.to_pretty_json_indent(depth, color),
        }
    }
}

impl fmt::Display for LogObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_text())
    }
}

pub fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn csv_escape(text: &str) -> String {
    if text.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn format_json_float(value: f64) -> String {
    if value.is_finite() {
        let text = value.to_string();
        if text.contains('.') || text.contains('e') || text.contains('E') {
            text
        } else {
            format!("{text}.0")
        }
    } else if value.is_nan() {
        "null".to_string()
    } else if value.is_sign_negative() {
        "null".to_string()
    } else {
        "null".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_preserves_types() {
        let mut object = LogObject::new();
        object.insert("name", LogValue::str("a\"b"));
        object.insert("n", LogValue::Int(3));
        object.insert("ok", LogValue::Bln(true));
        assert_eq!(
            object.to_json(),
            "{\"n\":3,\"name\":\"a\\\"b\",\"ok\":true}"
        );
    }
}
