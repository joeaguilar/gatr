# gatr — GATe in Rust

**Status:** spec, v1 — ready to build (rev 2: `sitrep` cut from v1; gate state exposed as a JSON feed instead — see §3.1 and Deferred)
**Shape:** single Rust CLI binary (fleet convention: `itr`, `kgr`, `plr`, `ccq`), zero-config by default
**Origin:** `claude-reflection-notes.md` finding #4 (2026-07-02). Agents re-invent build-gate triage constantly: ~199 `cargo … 2>&1 | tail -N` / `grep -E '^error|-->'` pipelines and 20 `>/tmp/x.log 2>&1; echo EXIT=$?; tail` captures in rustglichur alone (sessions b6f68931×32, 27e8d372×28, 1bc1b853×26, 72934b6a×20), 84 `just ci | tail -20` runs, ~100 hand-rolled `GATE_EXIT=0 (green)/RED` markers in wisphive/TimelineClock (0565d9ad×18, c24ba5e7×5), 16 cargo-output mining commands in Panthexia-web/red, and 32 Darkroom commands carrying `grep -vi "NumPy version"` noise filters.

Finding #5 (the recurring `echo "=== git ===" && git status && echo "=== tracker ===" && itr stats` composite dashboard, ~260+ occurrences) is **not** absorbed here as a `sitrep` subcommand. Per the command-center investigation (`werkit/command-center-notes.md`), cross-project status rendering is wisphive's job — gatr's role in that picture is to **own gate state and expose it as a stable machine-readable feed** (§3.1) that wisphive federates. A single-project in-terminal `sitrep` may be revisited after the wisphive command center lands, if it still earns its keep.

## 1. Purpose

Run a verification command once, keep the **full** log on disk, and print a **compact, machine-stable summary** — so an agent (or a human) never pipes build output through ad-hoc `tail`/`grep` again, and never loses the full log when the summary wasn't enough. Secondary: be the system of record for "was the gate green?" — queryable after the fact (`gatr last`) and consumable by other tools (§3.1).

**The core contract** is the first output line of every `gatr run`, greppable and format-frozen:

```
GATR exit=1 dur=42.3s errors=3 warnings=1 adapter=cargo tag=test log=/Users/x/.local/state/gatr/BigGlichur/2026-07-02T12-33-01_test.log
```

Everything after that line is human-oriented and may evolve; the first line never changes shape. Agents parse it with a single grep.

## 2. CLI surface

### 2.1 `gatr run [opts] -- <cmd…>`
Executes `<cmd…>` in the current directory, streaming combined stdout+stderr to the log file while parsing it.

Output on completion (default):
1. The `GATR …` contract line.
2. Up to `--errors N` (default 3) extracted **error blocks** (an error line plus its continuation lines, e.g. a full rustc diagnostic with its `-->` and notes).
3. The last `--tail N` (default 10) raw lines (where cargo/pytest print their summaries).

| flag | meaning |
|---|---|
| `--tag <label>` | names the gate (`ci`, `test`, `clippy`); used in log naming, `last`, and `sitrep`. Default: first word of the command |
| `--adapter <auto\|cargo\|tsc\|pytest\|jest\|eslint\|generic>` | error-pattern set; `auto` (default) sniffs from argv[0] and log content |
| `--filter <regex>` | drop matching lines from *display* (never from the log). Repeatable. Kills the Darkroom `grep -vi "NumPy version"` class of noise |
| `--tail <n>` / `--errors <n>` | summary sizing |
| `--quiet` | contract line only |
| `--json` | machine summary: `{exit, dur_s, errors, warnings, error_blocks[], tail[], log, tag, adapter, cmd}` |
| `--timeout <dur>` | kill + report `exit=124`-style timeout after e.g. `10m` |

**Exit code passthrough:** `gatr run` exits with the wrapped command's code. `gatr run -- just ci && git commit …` behaves exactly like the bare command. This is non-negotiable — it is what makes `gatr` a drop-in wrapper.

### 2.2 `gatr last [--tag T] [--project <path>] [--json]`
Reprint the most recent summary for this project (optionally per tag) without rerunning. Answers "was the gate green?" after context loss — the exact situation the `GATE_EXIT` echo markers were invented for. `--project` resolves another repo's state without cd'ing (the consumer-facing query path of §3.1).

### 2.3 `gatr errors [--tag T] [--all]`
Print only the extracted error blocks from the most recent log (default: all of them, not just the summary's 3). `--all` dumps every match; still never the whole log.

### 2.4 `gatr log [--tag T] [--cat]`
Print the path of the most recent log (default), or its content (`--cat`). Composable: `rg 'E0308' "$(gatr log --tag clippy)"`.

## 3. Storage, retention & the feed contract

- Logs: `~/.local/state/gatr/<project-slug>/<timestamp>_<tag>.log` plus a sibling `.meta.json` (the `--json` summary). Project slug = basename of the git root (or cwd if not a repo) + short hash of the full path to disambiguate.
- Retention: keep the most recent **20 logs per tag** per project; prune older on each run. `gatr gc [--all]` for manual cleanup. Never touches anything outside its state dir.

### 3.1 Feed contract (public API — wisphive integration point)

Gate state is a first-class feed for external consumers (the wisphive command center's "last gate result" tile, per `werkit/command-center-notes.md`). Two access paths, both **stable and versioned**:

1. `gatr last [--tag T] [--project <path>] --json` — the blessed query path. `--project` lets a consumer ask about a repo without cd'ing into it.
2. The `.meta.json` files themselves, for daemon-style consumers that watch the state dir:

```json
{
  "gatr_meta": 1,
  "cmd": "just ci", "tag": "ci", "adapter": "cargo",
  "exit": 0, "dur_s": 241.3, "errors": 0, "warnings": 2,
  "started": "2026-07-02T12-33-01Z",
  "project_path": "/Users/x/AI_Projects/BigGlichur",
  "log": ".../2026-07-02T12-33-01_ci.log",
  "error_blocks": [], "tail": ["test result: ok. 214 passed"]
}
```

- `gatr_meta` is the schema version; additive changes only, bump on anything breaking.
- `project_path` is recorded so consumers never have to reverse the slug hash.
- Everything else about gatr's internals (log naming, dir layout beyond meta files) stays private.

## 4. Adapters (error extraction)

An adapter is a named set of regexes: `error_start`, `warning_start`, `continuation` (lines glued to the current block), `summary` (lines always included in tail view).

| adapter | error_start (illustrative) |
|---|---|
| `cargo` | `^error(\[E\d+\])?:` ; continuation `^\s+-->`, `^\s+=`, `^\s+\|` ; warnings `^warning:` |
| `tsc` | `error TS\d+:` |
| `pytest` | `^(FAILED\|ERROR) `, `^E\s`, section `^=+ (FAILURES\|ERRORS) =+` |
| `jest` | `^\s*●`, `^FAIL ` |
| `eslint` | `^\s+\d+:\d+\s+error` |
| `generic` | `(?i)\berror\b` on line start-ish; the fallback |

`auto`: match argv[0]/subcommand (`cargo`, `tsc`, `pytest`, `npx jest`, `just` → sniff log content). Adapters are compiled in; `.gatr.toml` can add project-local ones.

## 5. Config (optional, `.gatr.toml` at repo root)

```toml
[run]
filters = ["NumPy version", "scipy._lib"]   # always-on display filters (Darkroom case)

[tags.ci]
adapter = "cargo"
```

Zero-config must work well; config only tunes.

## 6. Non-goals & deferred

- Not a task runner — it wraps `just`/`cargo`/`npm`, never replaces them.
- No daemon, no watch mode, no CI integration, no notifications.
- No log shipping/parsing of *old* logs from other tools; only what `gatr run` produced.
- **No status rendering beyond its own gates.** Git state, itr stats, and cross-project composites are explicitly out of scope — that's the wisphive command center's surface, fed by §3.1. (**Deferred, not rejected:** a single-project `gatr sitrep` may return post-wisphive if a pull-based terminal view still proves useful; the earlier rev of this spec has the sketch.)

## 7. Implementation notes

- Rust: `clap`, `regex`, `serde_json`, `os_pipe`/`duct` (or std `Command` with piped merged output — must interleave stdout/stderr in arrival order), `jiff`/`chrono`.
- Stream-parse while teeing: the summary must not require a second pass over multi-hundred-MB logs; keep a bounded ring buffer for `--tail` and bounded vec for error blocks.
- ANSI: strip escapes for pattern-matching and the stored log gets raw bytes as received; summaries print stripped text.
- If the wrapped command is interactive/TTY-dependent, don't fight it: gatr provides no PTY in v1 (document: use for batch gates).

## 8. Acceptance (v1)

1. `gatr run --tag ci -- just ci` in rustglichur-style repo: contract line correct, exit passthrough verified for green and red runs, full log on disk.
2. `gatr run -- cargo test -p <crate>` on a red crate extracts the real rustc error block(s), not noise.
3. `gatr run --filter 'NumPy version' -- python3 develop.py …` hides the warning from display while the log retains it.
4. `gatr last`, `gatr errors`, `gatr log --cat` all work after the terminal session that ran the gate is gone; `gatr last --project <path> --json` works from an unrelated cwd.
5. Feed contract: `.meta.json` validates against the §3.1 shape (`gatr_meta: 1`, `project_path` present); a consumer reading only meta files can reconstruct the last result per tag without invoking gatr.
6. Retention: 25 runs with one tag leave exactly 20 logs (meta files pruned in lockstep with their logs).
