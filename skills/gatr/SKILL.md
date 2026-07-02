---
name: gatr
description: Use gatr (gate runner CLI) to run build/test/lint verification commands instead of hand-rolling `cmd 2>&1 | tail -N`, `| grep -E '^error'`, `>/tmp/x.log 2>&1; echo EXIT=$?`, or `GATE_EXIT` marker pipelines. Trigger this skill ANY time you are about to run a verification gate (cargo build/test/clippy, just ci, pytest, tsc, jest, eslint, npm test, make check) and care about the errors or the exit code — even if the user doesn't say "gatr". Also trigger when the user asks "was the gate/build green", "what were the errors from the last build", "rerun the summary without rebuilding", "where's the full build log", or when you need a past gate result after context loss. Prefer `gatr run -- <cmd>` over piping build output through tail/grep, and `gatr last`/`gatr errors`/`gatr log` over rerunning a gate just to see its output again. Do NOT trigger for interactive/TTY commands, long-running servers/watch modes, or commands whose full output you genuinely need inline.
---

# gatr — gate runner skill

gatr wraps a verification command, tees the **full** combined stdout+stderr to
a log on disk, and prints a compact machine-stable summary. It is the system
of record for "was the gate green?".

## The one pattern

```sh
gatr run --tag <name> -- <command…>
```

Never `just ci 2>&1 | tail -20`. Never `cargo test >/tmp/x.log 2>&1; echo
EXIT=$?`. Just:

```sh
gatr run --tag ci -- just ci
gatr run --tag test -- cargo test -p mycrate
gatr run --tag clippy -- cargo clippy --all-targets -- -D warnings
```

The first output line is format-frozen and greppable:

```
GATR exit=1 dur=42.3s errors=3 warnings=1 adapter=cargo tag=ci log=/Users/x/.local/state/gatr/proj-ab12cd34/2026-07-02T12-33-01_ci.log
```

Then up to 3 extracted error blocks (full rustc diagnostics with their `-->`
lines) and the last 10 raw lines. If that's enough to fix the problem, you
never open the log; if it isn't, the `log=` path has everything.

**Exit passthrough:** `gatr run` exits with the wrapped command's code, so
`gatr run --tag ci -- just ci && git commit …` is safe — it behaves exactly
like the bare command.

## Question → command mapping

| you want | run |
|---|---|
| run a gate and see what broke | `gatr run --tag ci -- just ci` |
| was the last gate green? (after context loss) | `gatr last` / `gatr last --tag ci` |
| machine-readable result | `gatr last --json` (or `gatr run --json -- …`) |
| gate state of ANOTHER repo, without cd | `gatr last --project /path/to/repo --json` |
| just the error blocks, all of them | `gatr errors` (`--all` re-scans the full log) |
| the full log path (to rg/open) | `gatr log`, e.g. `rg 'E0308' "$(gatr log --tag clippy)"` |
| dump the full log | `gatr log --cat` |
| only the contract line (quiet gate in a script) | `gatr run --quiet -- …` |
| kill a hung gate | `gatr run --timeout 10m -- …` (reports `exit=124`) |
| hide known noise from the summary | `gatr run --filter 'NumPy version' -- …` (log keeps it) |

## Flags that matter

- `--tag <label>` — name the gate (`ci`, `test`, `clippy`). Everything
  (`last`, `errors`, `log`, retention) is per-tag. Default: first word of the
  command. Always tag your standard gates.
- `--adapter <auto|cargo|tsc|pytest|jest|eslint|generic>` — error-pattern
  set. `auto` (default) sniffs argv, then log content; only override when the
  sniff picks wrong (e.g. `just ci` mixing tools → `--adapter cargo`).
- `--errors N` / `--tail N` — summary sizing (defaults 3 / 10).
- `--filter <regex>` — display-only filter, repeatable; the stored log always
  keeps every line. Project-wide filters go in `.gatr.toml` `[run] filters`.

## Parsing the contract line

One grep, stable forever:

```sh
gatr run --quiet --tag ci -- just ci | grep -o 'exit=[0-9]*'
```

Or take the JSON: `{exit, dur_s, errors, warnings, error_blocks[], tail[],
log, tag, adapter, cmd, started, project_path, gatr_meta}`. `gatr_meta` is
the schema version (currently 1); the `.meta.json` sidecar files next to each
log carry the same shape for daemon-style consumers.

## Config (`.gatr.toml` at repo root, optional)

```toml
[run]
filters = ["NumPy version"]     # always-on display filters

[tags.ci]
adapter = "cargo"               # pin the adapter for a tag

[adapters.mytool]               # project-local adapter
error_start = "^BOOM:"
```

## Key gotchas

- **No live output.** The summary prints on completion; during a long build
  gatr is silent (the log is being written the whole time). For gates that
  can hang, add `--timeout`.
- **Batch gates only** — no PTY in v1. Don't wrap interactive commands,
  REPLs, watch modes, or servers.
- **State location:** `~/.local/state/gatr/<project-slug>/` (respects
  `XDG_STATE_HOME`; `GATR_STATE_DIR` overrides — useful in tests). Retention
  is 20 logs per tag per project, pruned automatically; `gatr gc [--all]` for
  manual cleanup.
- **Project identity** = git root of the cwd. Run gatr from anywhere inside
  the repo; `gatr last --project <path>` queries from outside.
- The wrapped command runs in the **current directory**, exactly as given —
  gatr never shell-interprets it. Pipes/globs need `sh -c '…'` (quoted, one
  arg).

## Updating gatr

- Source install: `gatr upgrade` (pulls the checkout it was built from,
  rebuilds, replaces the binary).
- Release install: re-run the installer —
  `curl -fsSL https://raw.githubusercontent.com/joeaguilar/gatr/main/install.sh | bash`.
- Check what you have: `gatr --version` (embeds `git describe`).

## When NOT to use gatr

- Interactive or TTY-dependent commands (git rebase -i, ssh, watch modes).
- Commands whose complete output you need inline anyway (short `ls`, `git
  status` — no gate, no benefit).
- As a task runner — it wraps `just`/`cargo`/`npm`, never replaces them.

## Reporting back to the user

Quote the contract line verbatim when reporting a gate result — it carries
exit, counts, duration, and the log path in one greppable line. For failures,
show the extracted error block(s), not raw log dumps.
