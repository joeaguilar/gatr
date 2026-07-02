use std::collections::BTreeMap;

use anyhow::{Context, Result};
use regex::Regex;

use crate::config::AdapterSection;

/// A regex that matches no line at all.
const NEVER: &str = r"[^\s\S]";

/// Candidate order doubles as the tie-break priority when content-sniffing.
pub const CANDIDATES: [&str; 5] = ["cargo", "tsc", "pytest", "jest", "eslint"];

pub struct Adapter {
    pub name: String,
    pub error_start: Regex,
    pub warning_start: Regex,
    pub continuation: Regex,
    pub summary: Regex,
}

impl Adapter {
    fn built_in(name: &str, error: &str, warning: &str, cont: &str, summary: &str) -> Self {
        let rx = |p: &str| Regex::new(p).expect("built-in adapter regex must compile");
        Self {
            name: name.to_string(),
            error_start: rx(error),
            warning_start: rx(warning),
            continuation: rx(cont),
            summary: rx(summary),
        }
    }

    pub fn from_config(name: &str, section: &AdapterSection) -> Result<Self> {
        let rx = |p: &str, field: &str| {
            Regex::new(p)
                .with_context(|| format!(".gatr.toml adapter '{name}': invalid {field} regex"))
        };
        Ok(Self {
            name: name.to_string(),
            error_start: rx(&section.error_start, "error_start")?,
            warning_start: rx(
                section.warning_start.as_deref().unwrap_or(NEVER),
                "warning_start",
            )?,
            continuation: rx(
                section.continuation.as_deref().unwrap_or(r"^\s+"),
                "continuation",
            )?,
            summary: rx(section.summary.as_deref().unwrap_or(NEVER), "summary")?,
        })
    }
}

pub fn built_in(name: &str) -> Option<Adapter> {
    let a = match name {
        "cargo" => Adapter::built_in(
            "cargo",
            r"^error(\[E\d+\])?:",
            r"^warning:",
            r"^\s+-->|^\s*\d*\s*\||^\s+=|^\s+(note|help)[:\s]|^note[:\s]|^help[:\s]|^\s+\^",
            r"^test result:|^error: could not compile|^error: aborting",
        ),
        "tsc" => Adapter::built_in("tsc", r"error TS\d+:", NEVER, r"^\s+", r"Found \d+ error"),
        "pytest" => Adapter::built_in(
            "pytest",
            r"^(FAILED|ERROR)\b|^E\s+|^=+ (FAILURES|ERRORS) =+",
            r"\b[A-Z]\w*Warning\b",
            r"^\s+|^>",
            r"^=+ .+ =+$",
        ),
        "jest" => Adapter::built_in(
            "jest",
            r"^\s*●|^FAIL\b",
            NEVER,
            r"^\s+",
            r"^(Tests|Test Suites|Snapshots|Time):",
        ),
        "eslint" => Adapter::built_in(
            "eslint",
            r"^\s+\d+:\d+\s+error\b",
            r"^\s+\d+:\d+\s+warning\b",
            NEVER,
            r"^✖|\d+ problems?",
        ),
        "generic" => Adapter::built_in(
            "generic",
            r"(?i)^.{0,32}\berror\b",
            r"(?i)^.{0,32}\bwarning\b",
            r"^\s+",
            NEVER,
        ),
        _ => return None,
    };
    Some(a)
}

/// Resolve an adapter by name: built-ins first, then project-local `.gatr.toml` adapters.
pub fn resolve(name: &str, custom: &BTreeMap<String, AdapterSection>) -> Result<Adapter> {
    if let Some(a) = built_in(name) {
        return Ok(a);
    }
    if let Some(section) = custom.get(name) {
        return Adapter::from_config(name, section);
    }
    anyhow::bail!(
        "unknown adapter '{name}' (built-ins: auto, cargo, tsc, pytest, jest, eslint, generic; \
         project-local adapters come from [adapters.*] in .gatr.toml)"
    );
}

/// Sniff an adapter from the command's argv; `None` means fall back to content sniffing.
pub fn sniff_from_argv(cmd: &[String]) -> Option<&'static str> {
    let base = |s: &str| {
        std::path::Path::new(s)
            .file_stem()
            .map_or_else(|| s.to_lowercase(), |f| f.to_string_lossy().to_lowercase())
    };
    let argv0 = base(cmd.first()?);
    let arg1 = cmd.get(1).map(|s| base(s)).unwrap_or_default();
    match argv0.as_str() {
        "cargo" | "rustc" => Some("cargo"),
        "tsc" => Some("tsc"),
        "pytest" | "py.test" => Some("pytest"),
        "jest" => Some("jest"),
        "eslint" => Some("eslint"),
        "python" | "python2" | "python3" | "uv" | "uvx" => {
            cmd.iter().any(|a| a.contains("pytest")).then_some("pytest")
        }
        "npx" | "pnpx" | "bunx" => match arg1.as_str() {
            "jest" => Some("jest"),
            "eslint" => Some("eslint"),
            "tsc" => Some("tsc"),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn sniffs_common_tools() {
        assert_eq!(sniff_from_argv(&v(&["cargo", "test"])), Some("cargo"));
        assert_eq!(
            sniff_from_argv(&v(&["/usr/bin/pytest", "-x"])),
            Some("pytest")
        );
        assert_eq!(
            sniff_from_argv(&v(&["python3", "-m", "pytest"])),
            Some("pytest")
        );
        assert_eq!(sniff_from_argv(&v(&["npx", "jest"])), Some("jest"));
        assert_eq!(sniff_from_argv(&v(&["just", "ci"])), None);
    }

    #[test]
    fn all_built_ins_compile() {
        for name in CANDIDATES.iter().chain(["generic"].iter()) {
            assert!(built_in(name).is_some(), "missing built-in {name}");
        }
    }

    #[test]
    fn cargo_patterns() {
        let a = built_in("cargo").unwrap();
        assert!(a.error_start.is_match("error[E0308]: mismatched types"));
        assert!(a.error_start.is_match("error: could not compile `gatr`"));
        assert!(!a.error_start.is_match("2 errors emitted"));
        assert!(a.warning_start.is_match("warning: unused variable: `x`"));
        assert!(a.continuation.is_match("  --> src/main.rs:4:5"));
        assert!(a.continuation.is_match("   |"));
        assert!(a.continuation.is_match("   = note: expected `u32`"));
        assert!(a.summary.is_match("test result: ok. 214 passed; 0 failed"));
    }

    #[test]
    fn generic_matches_start_ish_only() {
        let a = built_in("generic").unwrap();
        assert!(a.error_start.is_match("error: something broke"));
        assert!(a.error_start.is_match("[14:02:11] ERROR something broke"));
        assert!(!a.error_start.is_match(
            "this long line eventually mentions that there was an error deep inside the text"
        ));
    }
}
