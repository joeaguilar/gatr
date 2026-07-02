// Shapes `git describe --tags --always --dirty` output into a SemVer-ish
// display version. Shared between build.rs (via include!) and unit tests.
//
//   v1.2.3             -> 1.2.3
//   v1.2.3-4-gabcdef   -> 1.2.3-4-gabcdef
//   abcdef1 (no tags)  -> <fallback>+gabcdef1
//   abcdef1-dirty      -> <fallback>+gabcdef1-dirty
//   "" / whitespace    -> <fallback>
pub fn shape_version(describe: &str, fallback: &str) -> String {
    let d = describe.trim();
    if d.is_empty() {
        return fallback.to_string();
    }
    if let Some(rest) = d.strip_prefix('v') {
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            return rest.to_string();
        }
    }
    if d.starts_with(|c: char| c.is_ascii_digit()) && d.contains('.') {
        return d.to_string();
    }
    // Bare commit hash (possibly -dirty): repo has no version tags yet.
    format!("{fallback}+g{d}")
}

#[cfg(test)]
mod version_shape_tests {
    use super::shape_version;

    #[test]
    fn tagged() {
        assert_eq!(shape_version("v1.2.3", "0.1.0"), "1.2.3");
    }

    #[test]
    fn tagged_with_commits() {
        assert_eq!(
            shape_version("v1.2.3-4-gabcdef", "0.1.0"),
            "1.2.3-4-gabcdef"
        );
    }

    #[test]
    fn bare_hash() {
        assert_eq!(shape_version("abcdef1", "0.1.0"), "0.1.0+gabcdef1");
    }

    #[test]
    fn bare_hash_dirty() {
        assert_eq!(
            shape_version("abcdef1-dirty", "0.1.0"),
            "0.1.0+gabcdef1-dirty"
        );
    }

    #[test]
    fn empty_falls_back() {
        assert_eq!(shape_version("  ", "0.1.0"), "0.1.0");
    }

    #[test]
    fn unprefixed_semver() {
        assert_eq!(shape_version("1.2.3", "0.1.0"), "1.2.3");
    }
}
