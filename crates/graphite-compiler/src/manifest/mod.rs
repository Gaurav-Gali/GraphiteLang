//! Domain manifest loader.
//!
//! `grf.json` is JSON metadata. It is loaded here, not by the Graphite lexer
//! or parser. Stage 3 keeps the existing schema. Unknown fields are rejected so
//! a typo cannot silently change the contract. Type mappings and code-generation
//! strategies are not read from this file. They live in the compiler.

use std::collections::BTreeMap;

use serde::Deserialize;

pub const MANIFEST_FILE_NAME: &str = "grf.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub format_version: u32,
    pub domain: String,
    pub libraries: BTreeMap<String, Library>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub description: String,
    /// When true, importing the library also registers a built-in class with
    /// the same bind name whose static methods mirror these functions.
    pub exports_class: bool,
    pub functions: BTreeMap<String, Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub description: String,
    pub params: Vec<Parameter>,
    pub returns: ManifestType,
    pub rust_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub ty: ManifestType,
    /// Optional default when the Graphite argument is omitted. Only string
    /// literals are supported in the manifest today.
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestType {
    pub auto: bool,
    pub name: String,
}

/// Checks that manifest text is JSON. This does not validate the Graphite schema.
pub fn parse_json(text: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(text)
}

pub fn load_manifest(text: &str) -> Result<Manifest, String> {
    let file: ManifestFile =
        serde_json::from_str(text).map_err(|error| format!("invalid grf.json: {error}"))?;
    if file.format_version != 1 {
        return Err(format!(
            "unsupported grf.json formatVersion {}",
            file.format_version
        ));
    }
    if file.domain.name.is_empty() {
        return Err("grf.json domain name is empty".to_string());
    }
    let mut libraries = BTreeMap::new();
    for (name, library) in file.libraries {
        let mut functions = BTreeMap::new();
        for (function_name, function) in library.functions {
            if function.rust_path.is_empty() || !function.rust_path.contains("::") {
                return Err(format!("function '{function_name}' is missing a Rust path"));
            }
            let mut params = Vec::new();
            for param in function.params {
                params.push(Parameter {
                    name: param.name,
                    ty: parse_manifest_type(&param.ty)?,
                    default: param.default,
                });
            }
            functions.insert(
                function_name,
                Function {
                    description: function.description,
                    params,
                    returns: parse_manifest_type(&function.returns)?,
                    rust_path: function.rust_path,
                },
            );
        }
        libraries.insert(
            name,
            Library {
                description: library.description,
                exports_class: library.exports_class,
                functions,
            },
        );
    }
    Ok(Manifest {
        format_version: file.format_version,
        domain: file.domain.name,
        libraries,
    })
}

pub fn builtin_manifest() -> Manifest {
    load_manifest(include_str!("../../../../crates/graphite-core/grf.json"))
        .expect("bundled grf.json")
}

fn parse_manifest_type(text: &str) -> Result<ManifestType, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty type".to_string());
    }
    // `async void` or `async<void>` — library async returns (LLD-23).
    if let Some(inner) = text
        .strip_prefix("async<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        if !is_known_type(inner) {
            return Err(format!("unknown manifest type inside async<{inner}>"));
        }
        return Ok(ManifestType {
            auto: false,
            name: format!("async<{inner}>"),
        });
    }
    let mut parts = text.split_whitespace();
    let first = parts.next().ok_or_else(|| "empty type".to_string())?;
    if first == "async" {
        let inner = parts
            .next()
            .ok_or_else(|| "async requires a result type".to_string())?;
        if parts.next().is_some() {
            return Err(format!("invalid manifest type '{text}'"));
        }
        if !is_known_type(inner) {
            return Err(format!("unknown manifest type '{inner}'"));
        }
        return Ok(ManifestType {
            auto: false,
            name: format!("async<{inner}>"),
        });
    }
    let (auto, name) = if first == "auto" {
        let name = parts
            .next()
            .ok_or_else(|| "auto requires a target type".to_string())?;
        (true, name)
    } else {
        (false, first)
    };
    if parts.next().is_some() {
        return Err(format!("invalid manifest type '{text}'"));
    }
    if !is_known_type(name) {
        return Err(format!("unknown manifest type '{name}'"));
    }
    if auto && name == "void" {
        return Err("auto void is not a valid manifest type".to_string());
    }
    Ok(ManifestType {
        auto,
        name: name.to_string(),
    })
}

fn is_known_type(name: &str) -> bool {
    matches!(
        name,
        "void"
            | "int"
            | "uint"
            | "flt"
            | "int16"
            | "int32"
            | "int64"
            | "uint16"
            | "uint32"
            | "uint64"
            | "flt32"
            | "flt64"
            | "bln"
            | "chr"
            | "str"
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
struct ManifestFile {
    format_version: u32,
    domain: DomainFile,
    libraries: BTreeMap<String, LibraryFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct DomainFile {
    name: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
struct LibraryFile {
    description: String,
    #[serde(default)]
    exports_class: bool,
    functions: BTreeMap<String, FunctionFile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
struct FunctionFile {
    description: String,
    params: Vec<ParamFile>,
    returns: String,
    rust_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParamFile {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    default: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_file_name_is_grf_json() {
        assert_eq!(MANIFEST_FILE_NAME, "grf.json");
    }

    #[test]
    fn bundled_manifests_are_json() {
        let core = include_str!("../../../../crates/graphite-core/grf.json");
        let hello = include_str!("../../../../examples/hello/grf.json");
        let core_json = parse_json(core).expect("graphite-core grf.json");
        let hello_json = parse_json(hello).expect("examples/hello grf.json");
        assert_eq!(core_json, hello_json);
    }

    #[test]
    fn builtin_manifest_keeps_the_io_contract() {
        let manifest = builtin_manifest();
        assert_eq!(manifest.format_version, 1);
        assert_eq!(manifest.domain, "grf");
        let io = manifest.libraries.get("io").expect("io");
        let println = io.functions.get("println").expect("println");
        assert_eq!(println.rust_path, "graphite_core::io::println");
        assert_eq!(println.returns.name, "void");
        assert!(println.params[0].ty.auto);
        assert_eq!(println.params[0].ty.name, "str");
        let input = io.functions.get("input").expect("input");
        assert_eq!(input.returns.name, "str");
        assert!(!input.returns.auto);
    }

    #[test]
    fn builtin_manifest_keeps_the_io_log_contract() {
        let manifest = builtin_manifest();
        let log = manifest.libraries.get("io.log").expect("io.log");
        assert!(log.exports_class);
        let info = log.functions.get("info").expect("info");
        assert_eq!(info.rust_path, "graphite_core::io::log::info");
        assert_eq!(info.params.len(), 2);
        assert_eq!(info.params[1].default.as_deref(), Some(""));
        let custom = log.functions.get("custom").expect("custom");
        assert_eq!(custom.params[2].default.as_deref(), Some("#FFFFFF"));
        assert!(log.functions.contains_key("checkpoint"));
        assert!(log.functions.contains_key("fatal"));
    }

    #[test]
    fn builtin_manifest_keeps_the_time_contract() {
        let manifest = builtin_manifest();
        let time = manifest.libraries.get("time").expect("time");
        let today = time.functions.get("today").expect("today");
        assert_eq!(today.rust_path, "graphite_core::time::today");
        assert_eq!(today.returns.name, "str");
        assert!(today.params.is_empty());
        let secs = time.functions.get("secs").expect("secs");
        assert_eq!(secs.params[0].ty.name, "flt");
        assert!(secs.params[0].ty.auto);
        assert_eq!(secs.returns.name, "flt");
        let flt = time.functions.get("flt").expect("flt");
        assert_eq!(flt.rust_path, "graphite_core::time::current_flt");
        assert_eq!(flt.returns.name, "flt");
        let sleep = time.functions.get("sleep").expect("sleep");
        assert_eq!(sleep.returns.name, "async<void>");
        assert_eq!(sleep.rust_path, "graphite_core::time::sleep");
        assert!(time.functions.contains_key("addDays"));
        assert!(time.functions.contains_key("daysBetween"));
        assert!(time.functions.contains_key("toDay"));
    }

    #[test]
    fn async_void_manifest_types_parse() {
        let ty = parse_manifest_type("async void").expect("async void");
        assert_eq!(ty.name, "async<void>");
        let ty = parse_manifest_type("async<void>").expect("async<void>");
        assert_eq!(ty.name, "async<void>");
    }

    #[test]
    fn unknown_manifest_fields_are_rejected() {
        let mut value =
            parse_json(include_str!("../../../../crates/graphite-core/grf.json")).expect("json");
        value
            .as_object_mut()
            .unwrap()
            .insert("language".to_string(), serde_json::json!({}));
        let error = load_manifest(&value.to_string()).expect_err("unknown field");
        assert!(error.contains("unknown field"), "{error}");
    }
}
