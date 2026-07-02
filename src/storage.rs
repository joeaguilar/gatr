use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Retention: most recent N logs per tag, per project; pruned on each run.
pub const RETAIN_PER_TAG: usize = 20;

/// Schema version for `.meta.json` (§3.1 feed contract). Additive changes
/// only; bump on anything breaking.
pub const META_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)] // gatr_meta is the §3.1 wire format
pub struct Meta {
    pub gatr_meta: u32,
    pub cmd: String,
    pub tag: String,
    pub adapter: String,
    pub exit: i32,
    pub dur_s: f64,
    pub errors: usize,
    pub warnings: usize,
    pub started: String,
    pub project_path: String,
    pub log: String,
    pub error_blocks: Vec<String>,
    pub tail: Vec<String>,
}

/// State root: `$GATR_STATE_DIR` override (mostly for tests), else
/// `$XDG_STATE_HOME/gatr`, else `~/.local/state/gatr`.
pub fn state_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("GATR_STATE_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(|| home_dir().join(".local").join("state"), PathBuf::from);
    base.join("gatr")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Walk up from `start` to the enclosing git root; fall back to `start`.
pub fn project_root(start: &Path) -> PathBuf {
    let start = std::fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    let mut dir = start.as_path();
    loop {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return start.clone(),
        }
    }
}

/// Project slug: basename of the project root plus a short hash of the full
/// path, to disambiguate same-named checkouts.
pub fn slug(root: &Path) -> String {
    let canon = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let name = canon
        .file_name()
        .map_or_else(|| "root".to_string(), |n| n.to_string_lossy().to_string());
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!(
        "{sanitized}-{:08x}",
        fnv1a32(canon.to_string_lossy().as_bytes())
    )
}

fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for b in bytes {
        hash ^= u32::from(*b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// State dir for the project containing `path` (resolves the git root first).
pub fn project_state_dir(path: &Path) -> PathBuf {
    state_root().join(slug(&project_root(path)))
}

pub fn sanitize_tag(tag: &str) -> String {
    let s: String = tag
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if s.is_empty() {
        "run".to_string()
    } else {
        s
    }
}

/// Allocate a fresh `<timestamp>_<tag>.log` path (suffixing on collision).
pub fn new_log_path(dir: &Path, started: &str, tag: &str) -> PathBuf {
    let stem_base = format!("{}_{}", started.trim_end_matches('Z'), sanitize_tag(tag));
    let mut candidate = dir.join(format!("{stem_base}.log"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem_base}-{n}.log"));
        n += 1;
    }
    candidate
}

pub fn meta_path_for(log_path: &Path) -> PathBuf {
    log_path.with_extension("meta.json")
}

pub fn write_meta(meta: &Meta, log_path: &Path) -> Result<PathBuf> {
    let path = meta_path_for(log_path);
    let json = serde_json::to_string_pretty(meta).context("failed to serialize meta")?;
    std::fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

/// All metas in a project state dir, newest first. Ordered by meta-file
/// mtime (the moment the run completed), falling back to the timestamp
/// filename prefix — plain lexicographic order would mis-sort same-second
/// collision suffixes (`_ci-2` sorts before `_ci.`).
pub fn list_metas(dir: &Path) -> Vec<(PathBuf, Meta)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut metas: Vec<(std::time::SystemTime, PathBuf, Meta)> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".meta.json"))
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let meta: Meta = serde_json::from_str(&text).ok()?;
            let mtime = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((mtime, p, meta))
        })
        .collect();
    metas.sort_by(|(ta, pa, _), (tb, pb, _)| (tb, pb.file_name()).cmp(&(ta, pa.file_name())));
    metas.into_iter().map(|(_, p, m)| (p, m)).collect()
}

pub fn latest(dir: &Path, tag: Option<&str>) -> Option<(PathBuf, Meta)> {
    list_metas(dir)
        .into_iter()
        .find(|(_, m)| tag.is_none_or(|t| m.tag == t))
}

/// Prune logs beyond the retention window; meta files go in lockstep with
/// their logs. Never touches anything outside `dir`.
pub fn prune(dir: &Path) -> usize {
    let metas = list_metas(dir);
    let mut per_tag: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut removed = 0;
    for (meta_path, meta) in &metas {
        let seen = per_tag.entry(meta.tag.as_str()).or_insert(0);
        *seen += 1;
        if *seen > RETAIN_PER_TAG {
            let log_path = meta_path.with_extension("").with_extension("log");
            let _ = std::fs::remove_file(&log_path);
            let _ = std::fs::remove_file(meta_path);
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta_stub(tag: &str, started: &str) -> Meta {
        Meta {
            gatr_meta: META_SCHEMA,
            cmd: "true".into(),
            tag: tag.into(),
            adapter: "generic".into(),
            exit: 0,
            dur_s: 0.1,
            errors: 0,
            warnings: 0,
            started: started.into(),
            project_path: "/tmp/x".into(),
            log: String::new(),
            error_blocks: vec![],
            tail: vec![],
        }
    }

    fn write_pair(dir: &Path, started: &str, tag: &str) {
        let log = new_log_path(dir, started, tag);
        std::fs::write(&log, "log").unwrap();
        let mut m = meta_stub(tag, started);
        m.log = log.to_string_lossy().to_string();
        write_meta(&m, &log).unwrap();
    }

    #[test]
    fn slug_is_stable_and_disambiguated() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("proj");
        std::fs::create_dir(&a).unwrap();
        let s1 = slug(&a);
        let s2 = slug(&a);
        assert_eq!(s1, s2);
        assert!(s1.starts_with("proj-"));
    }

    #[test]
    fn meta_path_derivation() {
        assert_eq!(
            meta_path_for(Path::new("/x/2026-07-02T12-33-01_ci.log")),
            Path::new("/x/2026-07-02T12-33-01_ci.meta.json")
        );
    }

    #[test]
    fn latest_respects_tag_filter() {
        let dir = tempfile::tempdir().unwrap();
        write_pair(dir.path(), "2026-07-02T10-00-00", "ci");
        write_pair(dir.path(), "2026-07-02T11-00-00", "test");
        write_pair(dir.path(), "2026-07-02T12-00-00", "ci");
        let (_, newest) = latest(dir.path(), None).unwrap();
        assert_eq!(newest.started, "2026-07-02T12-00-00");
        let (_, newest_test) = latest(dir.path(), Some("test")).unwrap();
        assert_eq!(newest_test.started, "2026-07-02T11-00-00");
        assert!(latest(dir.path(), Some("nope")).is_none());
    }

    #[test]
    fn retention_keeps_twenty_per_tag() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..25 {
            write_pair(dir.path(), &format!("2026-07-02T10-00-{i:02}"), "ci");
        }
        write_pair(dir.path(), "2026-07-02T09-00-00", "other");
        let removed = prune(dir.path());
        assert_eq!(removed, 5);
        let remaining = list_metas(dir.path());
        assert_eq!(remaining.iter().filter(|(_, m)| m.tag == "ci").count(), 20);
        assert_eq!(
            remaining.iter().filter(|(_, m)| m.tag == "other").count(),
            1
        );
        // logs pruned in lockstep with metas
        let logs = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "log")
            })
            .count();
        assert_eq!(logs, 21);
    }

    #[test]
    fn collision_suffixes() {
        let dir = tempfile::tempdir().unwrap();
        let first = new_log_path(dir.path(), "2026-07-02T10-00-00", "ci");
        std::fs::write(&first, "x").unwrap();
        let second = new_log_path(dir.path(), "2026-07-02T10-00-00", "ci");
        assert_ne!(first, second);
        assert!(second.to_string_lossy().ends_with("_ci-2.log"));
    }
}
