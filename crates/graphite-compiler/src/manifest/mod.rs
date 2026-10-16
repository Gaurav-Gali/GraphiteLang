//! Domain manifest loader.
//!
//! `grf.json` is JSON metadata. It must be loaded here, not by the Graphite lexer
//! or parser. Schema validation, unknown-field policy, and domain lookup are
//! milestone 4. Those decisions are still open in LLD-03.

pub const MANIFEST_FILE_NAME: &str = "grf.json";

/// Checks that manifest text is JSON. This does not validate the Graphite schema
/// and does not choose whether unknown fields are rejected.
pub fn parse_json(text: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(text)
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
}
