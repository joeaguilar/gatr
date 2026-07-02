use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Project-local config, `.gatr.toml` at the repo root. Zero-config must work
/// well; config only tunes.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub run: RunSection,
    pub tags: BTreeMap<String, TagSection>,
    pub adapters: BTreeMap<String, AdapterSection>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunSection {
    /// Always-on display filters (never applied to the stored log).
    pub filters: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TagSection {
    pub adapter: Option<String>,
    pub filters: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AdapterSection {
    pub error_start: String,
    pub warning_start: Option<String>,
    pub continuation: Option<String>,
    pub summary: Option<String>,
}

pub const CONFIG_FILE: &str = ".gatr.toml";

pub fn load(project_root: &Path) -> Result<Config> {
    let path = project_root.join(CONFIG_FILE);
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_example() {
        let cfg: Config = toml::from_str(
            r#"
            [run]
            filters = ["NumPy version", "scipy._lib"]

            [tags.ci]
            adapter = "cargo"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.run.filters.len(), 2);
        assert_eq!(cfg.tags["ci"].adapter.as_deref(), Some("cargo"));
    }

    #[test]
    fn parses_custom_adapter() {
        let cfg: Config = toml::from_str(
            r#"
            [adapters.mytool]
            error_start = "^BOOM:"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.adapters["mytool"].error_start, "^BOOM:");
    }

    #[test]
    fn missing_file_is_default() {
        let cfg = load(Path::new("/nonexistent-dir-xyz")).unwrap();
        assert!(cfg.run.filters.is_empty());
    }
}
