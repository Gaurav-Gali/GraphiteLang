//! Rust code generation.
//!
//! Milestone 5. Generation reads a validated AST. It does not rewrite Graphite
//! source text. Generated Cargo projects depend on `graphite-core` through the
//! workspace-relative path below. That path is not an absolute developer directory.
//! Finding the workspace root from an installed `grf` binary is milestone 7.

use std::path::{Path, PathBuf};

pub fn runtime_crate_relative_path() -> PathBuf {
    PathBuf::from(Path::new("crates/graphite-core"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_path_stays_workspace_relative() {
        let path = runtime_crate_relative_path();
        assert!(path.is_relative());
        assert_eq!(path, Path::new("crates/graphite-core"));
    }
}
