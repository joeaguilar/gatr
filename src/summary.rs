use crate::storage::Meta;

/// The format-frozen first line of every `gatr run`. Everything after it is
/// human-oriented and may evolve; this line never changes shape.
pub fn contract_line(meta: &Meta) -> String {
    format!(
        "GATR exit={} dur={:.1}s errors={} warnings={} adapter={} tag={} log={}",
        meta.exit, meta.dur_s, meta.errors, meta.warnings, meta.adapter, meta.tag, meta.log
    )
}

/// Default human output: contract line, up to `max_blocks` error blocks, then
/// the tail.
pub fn render_human(meta: &Meta, max_blocks: usize) -> String {
    let mut out = contract_line(meta);
    let total = meta.error_blocks.len();
    for (i, block) in meta.error_blocks.iter().take(max_blocks).enumerate() {
        out.push_str(&format!("\n\n── error {}/{} ──\n{}", i + 1, total, block));
    }
    if total > max_blocks {
        out.push_str(&format!(
            "\n\n(+{} more error blocks — `gatr errors` prints them all)",
            total - max_blocks
        ));
    }
    if !meta.tail.is_empty() {
        out.push_str(&format!("\n\n── tail ({} lines) ──", meta.tail.len()));
        for line in &meta.tail {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}

/// Display form of the wrapped command: args with whitespace get quoted.
pub fn join_cmd(cmd: &[String]) -> String {
    cmd.iter()
        .map(|a| {
            if a.is_empty() || a.chars().any(char::is_whitespace) {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::META_SCHEMA;

    fn meta() -> Meta {
        Meta {
            gatr_meta: META_SCHEMA,
            cmd: "just ci".into(),
            tag: "ci".into(),
            adapter: "cargo".into(),
            exit: 1,
            dur_s: 42.34,
            errors: 3,
            warnings: 1,
            started: "2026-07-02T12-33-01Z".into(),
            project_path: "/Users/x/proj".into(),
            log: "/Users/x/.local/state/gatr/proj-abc/2026-07-02T12-33-01_ci.log".into(),
            error_blocks: vec!["error: one".into(), "error: two".into()],
            tail: vec!["test result: FAILED".into()],
        }
    }

    #[test]
    fn contract_line_shape_is_frozen() {
        assert_eq!(
            contract_line(&meta()),
            "GATR exit=1 dur=42.3s errors=3 warnings=1 adapter=cargo tag=ci \
             log=/Users/x/.local/state/gatr/proj-abc/2026-07-02T12-33-01_ci.log"
        );
    }

    #[test]
    fn human_output_caps_blocks() {
        let text = render_human(&meta(), 1);
        assert!(text.starts_with("GATR exit=1 "));
        assert!(text.contains("── error 1/2 ──"));
        assert!(!text.contains("error: two"));
        assert!(text.contains("(+1 more error blocks"));
        assert!(text.contains("── tail (1 lines) ──"));
    }

    #[test]
    fn cmd_join_quotes_whitespace() {
        assert_eq!(
            join_cmd(&["sh".into(), "-c".into(), "echo hi".into()]),
            "sh -c 'echo hi'"
        );
    }
}
