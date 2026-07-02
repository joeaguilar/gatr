use std::collections::VecDeque;
use std::sync::LazyLock;

use regex::Regex;

use crate::adapters::Adapter;

/// Bounds keep the stream parse O(1) in memory even over multi-hundred-MB logs.
pub const MAX_BLOCKS: usize = 100;
pub const MAX_BLOCK_LINES: usize = 40;
const SUMMARY_KEEP: usize = 5;

static ANSI: LazyLock<Regex> = LazyLock::new(|| {
    // CSI sequences, OSC sequences, and stray non-CSI escapes.
    Regex::new(r"\x1b\[[0-9;:?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b.").unwrap()
});

/// Strip ANSI escapes and control chars (except tab) for pattern matching and
/// display. The stored log keeps raw bytes as received.
pub fn strip_ansi(line: &str) -> String {
    let no_esc = ANSI.replace_all(line, "");
    no_esc
        .chars()
        .filter(|c| *c == '\t' || !c.is_control())
        .collect()
}

/// Per-adapter accumulator: counts error/warning starts, glues continuation
/// lines into bounded error blocks, and keeps matched summary lines.
pub struct Collector {
    pub adapter: Adapter,
    pub errors: usize,
    pub warnings: usize,
    pub blocks: Vec<String>,
    pub summary_lines: Vec<String>,
    current: Vec<String>,
}

impl Collector {
    pub fn new(adapter: Adapter) -> Self {
        Self {
            adapter,
            errors: 0,
            warnings: 0,
            blocks: Vec::new(),
            summary_lines: Vec::new(),
            current: Vec::new(),
        }
    }

    pub fn feed(&mut self, line: &str) {
        if self.adapter.error_start.is_match(line) {
            self.flush();
            self.errors += 1;
            self.current.push(line.to_string());
        } else if !self.current.is_empty() && self.adapter.continuation.is_match(line) {
            if self.current.len() < MAX_BLOCK_LINES {
                self.current.push(line.to_string());
            }
        } else {
            self.flush();
            if self.adapter.warning_start.is_match(line) {
                self.warnings += 1;
            }
        }
        if self.adapter.summary.is_match(line) {
            if self.summary_lines.len() == SUMMARY_KEEP {
                self.summary_lines.remove(0);
            }
            self.summary_lines.push(line.to_string());
        }
    }

    pub fn finish(&mut self) {
        self.flush();
    }

    fn flush(&mut self) {
        if !self.current.is_empty() && self.blocks.len() < MAX_BLOCKS {
            self.blocks.push(self.current.join("\n"));
        }
        self.current.clear();
    }
}

pub struct ParseOutcome {
    pub adapter: String,
    pub errors: usize,
    pub warnings: usize,
    pub blocks: Vec<String>,
    /// Last N raw (stripped) lines, with adapter summary lines force-included.
    pub tail: Vec<String>,
}

/// Streams stripped lines into one collector (adapter known) or all candidate
/// collectors (auto content sniff), plus a bounded tail ring buffer.
pub struct StreamParser {
    collectors: Vec<Collector>,
    auto: bool,
    tail: VecDeque<String>,
    tail_keep: usize,
    filters: Vec<Regex>,
}

impl StreamParser {
    pub fn new(
        collectors: Vec<Collector>,
        auto: bool,
        tail_keep: usize,
        filters: Vec<Regex>,
    ) -> Self {
        Self {
            collectors,
            auto,
            tail: VecDeque::with_capacity(tail_keep + 1),
            tail_keep,
            filters,
        }
    }

    pub fn feed_line(&mut self, stripped: &str) {
        if self.filters.iter().any(|f| f.is_match(stripped)) {
            return; // display filter: line stays in the log, never in the summary
        }
        if self.tail_keep > 0 {
            if self.tail.len() == self.tail_keep {
                self.tail.pop_front();
            }
            self.tail.push_back(stripped.to_string());
        }
        for c in &mut self.collectors {
            c.feed(stripped);
        }
    }

    pub fn finish(mut self) -> ParseOutcome {
        for c in &mut self.collectors {
            c.finish();
        }
        let tail: Vec<String> = self.tail.into_iter().collect();
        let winner = if self.auto {
            pick_winner(self.collectors)
        } else {
            self.collectors.remove(0)
        };
        let mut merged_tail: Vec<String> = winner
            .summary_lines
            .iter()
            .filter(|s| !tail.contains(s))
            .cloned()
            .collect();
        merged_tail.extend(tail);
        ParseOutcome {
            adapter: winner.adapter.name.clone(),
            errors: winner.errors,
            warnings: winner.warnings,
            blocks: winner.blocks,
            tail: merged_tail,
        }
    }
}

/// Content sniff: prefer the specific adapter with the most error hits, then
/// the most warning hits; earlier candidates win ties. Fall back to the last
/// collector (generic).
fn pick_winner(mut collectors: Vec<Collector>) -> Collector {
    let specific_best = collectors
        .iter()
        .enumerate()
        .filter(|(_, c)| c.adapter.name != "generic")
        .max_by(|(ia, a), (ib, b)| {
            (a.errors, a.warnings, std::cmp::Reverse(*ia)).cmp(&(
                b.errors,
                b.warnings,
                std::cmp::Reverse(*ib),
            ))
        })
        .map(|(i, c)| (i, c.errors, c.warnings));
    match specific_best {
        Some((i, e, w)) if e > 0 || w > 0 => collectors.swap_remove(i),
        _ => collectors.pop().expect("at least one collector"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::built_in;

    const CARGO_RED: &str = "\
   Compiling gatr v0.1.0
error[E0308]: mismatched types
  --> src/main.rs:4:20
   |
 4 |     let x: u32 = \"hi\";
   |            ---   ^^^^ expected `u32`, found `&str`
   |            |
   |            expected due to this
warning: unused variable: `y`
 --> src/main.rs:5:9
error: could not compile `gatr` (bin \"gatr\") due to 1 previous error; 1 warning emitted
";

    fn feed_all(parser: &mut StreamParser, text: &str) {
        for line in text.lines() {
            parser.feed_line(&strip_ansi(line));
        }
    }

    #[test]
    fn cargo_block_extraction() {
        let mut p = StreamParser::new(
            vec![Collector::new(built_in("cargo").unwrap())],
            false,
            10,
            vec![],
        );
        feed_all(&mut p, CARGO_RED);
        let out = p.finish();
        assert_eq!(out.errors, 2); // E0308 + could-not-compile
        assert_eq!(out.warnings, 1);
        assert!(out.blocks[0].contains("mismatched types"));
        assert!(out.blocks[0].contains("--> src/main.rs:4:20"));
        assert!(out.blocks[0].contains("expected due to this"));
    }

    #[test]
    fn auto_sniff_picks_cargo_over_generic() {
        let collectors = crate::adapters::CANDIDATES
            .iter()
            .chain(["generic"].iter())
            .map(|n| Collector::new(built_in(n).unwrap()))
            .collect();
        let mut p = StreamParser::new(collectors, true, 10, vec![]);
        feed_all(&mut p, CARGO_RED);
        let out = p.finish();
        assert_eq!(out.adapter, "cargo");
    }

    #[test]
    fn auto_sniff_falls_back_to_generic() {
        let collectors = crate::adapters::CANDIDATES
            .iter()
            .chain(["generic"].iter())
            .map(|n| Collector::new(built_in(n).unwrap()))
            .collect();
        let mut p = StreamParser::new(collectors, true, 10, vec![]);
        feed_all(&mut p, "all good\nnothing to see\n");
        let out = p.finish();
        assert_eq!(out.adapter, "generic");
        assert_eq!(out.errors, 0);
    }

    #[test]
    fn filters_drop_lines_from_display() {
        let mut p = StreamParser::new(
            vec![Collector::new(built_in("generic").unwrap())],
            false,
            10,
            vec![Regex::new("NumPy version").unwrap()],
        );
        feed_all(&mut p, "warning: NumPy version mismatch\nreal output\n");
        let out = p.finish();
        assert_eq!(out.warnings, 0);
        assert_eq!(out.tail, vec!["real output"]);
    }

    #[test]
    fn tail_is_bounded_and_summary_merged() {
        let mut p = StreamParser::new(
            vec![Collector::new(built_in("cargo").unwrap())],
            false,
            3,
            vec![],
        );
        let mut text = String::from("test result: ok. 214 passed; 0 failed\n");
        for i in 0..50 {
            text.push_str(&format!("line {i}\n"));
        }
        feed_all(&mut p, &text);
        let out = p.finish();
        assert_eq!(out.tail.len(), 4); // 3 tail lines + force-included summary
        assert_eq!(out.tail[0], "test result: ok. 214 passed; 0 failed");
        assert_eq!(out.tail[3], "line 49");
    }

    #[test]
    fn strip_ansi_removes_color_and_osc() {
        assert_eq!(strip_ansi("\x1b[1;31merror\x1b[0m: boom"), "error: boom");
        assert_eq!(strip_ansi("\x1b]0;title\x07plain"), "plain");
        assert_eq!(strip_ansi("keep\ttabs"), "keep\ttabs");
    }

    #[test]
    fn block_count_is_bounded() {
        let mut c = Collector::new(built_in("cargo").unwrap());
        for i in 0..(MAX_BLOCKS + 50) {
            c.feed(&format!("error: boom {i}"));
        }
        c.finish();
        assert_eq!(c.blocks.len(), MAX_BLOCKS);
        assert_eq!(c.errors, MAX_BLOCKS + 50); // counting never stops
    }
}
