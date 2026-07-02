# gatr — GATe Runner

[![CI](https://github.com/joeaguilar/gatr/actions/workflows/ci.yml/badge.svg)](https://github.com/joeaguilar/gatr/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/joeaguilar/gatr)](https://github.com/joeaguilar/gatr/releases/latest)

Run a verification command once, keep the **full** log on disk, and print a
**compact, machine-stable summary** — so an agent (or a human) never pipes
build output through ad-hoc `tail`/`grep` again, and never loses the full log
when the summary wasn't enough.

```
$ gatr run --tag ci -- just ci
GATR exit=1 dur=42.3s errors=3 warnings=1 adapter=cargo tag=ci log=/Users/x/.local/state/gatr/proj-ab12cd34/2026-07-02T12-33-01_ci.log

── error 1/3 ──
error[E0308]: mismatched types
  --> src/main.rs:4:20
  ...

── tail (10 lines) ──
test result: FAILED. 213 passed; 1 failed
error: could not compile `proj` due to 1 previous error
```

The first line is **the contract**: greppable, format-frozen, never changes
shape. Everything after it is human-oriented and may evolve.

`gatr` is also the system of record for "was the gate green?" — queryable
after the fact (`gatr last`) and consumable by other tools via versioned
`.meta.json` feed files.

## Install

```sh
# from a release (recommended)
curl -fsSL https://raw.githubusercontent.com/joeaguilar/gatr/main/install.sh | bash

# from source
git clone git@github.com:joeaguilar/gatr.git && cd gatr && ./install.sh
# (or GATR_FROM_SOURCE=1 ./install.sh, or cargo install --path .)

# Windows
irm https://raw.githubusercontent.com/joeaguilar/gatr/main/install.ps1 | iex
```

Update later with `gatr upgrade` (source installs) or by re-running the
installer (release installs).

## CLI surface

### `gatr run [opts] -- <cmd…>`

Executes `<cmd…>` in the current directory, streaming combined stdout+stderr
(in arrival order) to the log file while parsing it.

| flag | meaning |
|---|---|
| `--tag <label>` | names the gate (`ci`, `test`, `clippy`); default: first word of the command |
| `--adapter <auto\|cargo\|tsc\|pytest\|jest\|eslint\|generic>` | error-pattern set; `auto` (default) sniffs argv and log content |
| `--filter <regex>` | drop matching lines from *display* (never from the log); repeatable |
| `--tail <n>` / `--errors <n>` | summary sizing (defaults 10 / 3) |
| `--quiet` | contract line only |
| `--json` | machine summary as JSON |
| `--timeout <dur>` | kill and report `exit=124` after e.g. `10m` |

**Exit code passthrough:** `gatr run` exits with the wrapped command's code,
so `gatr run -- just ci && git commit …` behaves exactly like the bare
command. gatr is a drop-in wrapper.

### `gatr last [--tag T] [--project <path>] [--json]`

Reprint the most recent summary without rerunning — answers "was the gate
green?" after context loss. `--project` resolves another repo's state without
cd'ing into it.

### `gatr errors [--tag T] [--all]`

Print only the extracted error blocks from the most recent log. `--all`
re-scans the full log for every match.

### `gatr log [--tag T] [--cat]`

Print the path of the most recent log (default) or its content (`--cat`).
Composable: `rg 'E0308' "$(gatr log --tag clippy)"`.

### `gatr gc [--all]`

Prune logs beyond the retention window (20 per tag per project). Runs
automatically after each `gatr run`.

### `gatr upgrade`

Self-update: pull the source checkout, rebuild, replace the running binary.

## Storage & the feed contract

Logs live under `~/.local/state/gatr/<project-slug>/<timestamp>_<tag>.log`
(respecting `XDG_STATE_HOME`; override with `GATR_STATE_DIR`), each with a
sibling `.meta.json`:

```json
{
  "gatr_meta": 1,
  "cmd": "just ci", "tag": "ci", "adapter": "cargo",
  "exit": 0, "dur_s": 241.3, "errors": 0, "warnings": 2,
  "started": "2026-07-02T12-33-01Z",
  "project_path": "/Users/x/AI_Projects/proj",
  "log": ".../2026-07-02T12-33-01_ci.log",
  "error_blocks": [], "tail": ["test result: ok. 214 passed"]
}
```

`gatr_meta` is the schema version (additive changes only). The two stable
access paths for external consumers are `gatr last --json` and the meta files
themselves; everything else about the state dir layout is private.

## Config (optional, `.gatr.toml` at repo root)

```toml
[run]
filters = ["NumPy version", "scipy._lib"]   # always-on display filters

[tags.ci]
adapter = "cargo"

[adapters.mytool]                            # project-local adapter
error_start = "^BOOM:"
```

Zero-config works well; config only tunes.

## Non-goals

Not a task runner (it wraps `just`/`cargo`/`npm`, never replaces them). No
daemon, no watch mode, no notifications, no PTY (use it for batch gates, not
interactive commands).

## Development

```sh
just ci        # fmt-check + clippy + deny + unit + integration
just verify    # the ci gate plus release build
just gate      # dogfood: run the ci gate through gatr itself
```

Versioning is automatic: conventional commits on `main` drive
[auto-version.yml](.github/workflows/auto-version.yml) (`feat:` → minor,
`fix:` → patch, `type!:`/`BREAKING CHANGE` → major), which tags `vX.Y.Z` and
dispatches [release.yml](.github/workflows/release.yml) to build and publish
binaries for 7 targets. Binaries embed `git describe` via `build.rs`;
`Cargo.toml` stays pinned at 0.1.0 on purpose. Add `[skip version]` to a
commit message to suppress tagging.

## License

MIT
