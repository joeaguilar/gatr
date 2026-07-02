# AGENTS.md — working in the gatr repo

gatr is the fleet's gate runner: `gatr run -- <cmd>` wraps a verification
command, keeps the full log, prints the format-frozen `GATR …` contract line.
Spec: [spec.md](spec.md). User docs: [README.md](README.md).

## Ground rules

- **The contract line is frozen.** The first output line of `gatr run`
  (`GATR exit=… dur=… errors=… warnings=… adapter=… tag=… log=…`) and the
  `.meta.json` schema (`gatr_meta`) are public API. Additive changes only;
  anything breaking bumps `gatr_meta` and needs a `feat!:` commit.
- **Exit passthrough is non-negotiable.** `gatr run` must exit with the
  wrapped command's code in every path you touch.
- **Dogfood.** Run gates through gatr itself: `just gate`, or
  `gatr run --tag ci -- just ci`.

## Gates

```sh
just ci        # what CI runs: fmt-check + clippy(-D warnings) + deny + unit + integration
just verify    # ci + release build
```

Both must be green before any commit. The integration suite
(`tests/integration.sh`) is the spec's acceptance criteria — extend it when
you extend the CLI.

## Versioning & commits

- Conventional Commits, enforced by consequence: `auto-version.yml` tags a
  release from your subject line (`feat:` → minor, `fix:` → patch, `!`/
  `BREAKING CHANGE` → major). A sloppy `feat:` on a refactor ships a release.
  Use `chore:`/`refactor:`/`docs:`/`test:`/`ci:` when nothing user-visible
  changed, or add `[skip version]`.
- `Cargo.toml` version stays `0.1.0`; real versions come from git tags via
  `build.rs` (`GATR_VERSION`).
- Update `CHANGELOG.md` (`## Unreleased`) with any user-visible change.

## Issue tracking

Durable work lives in `itr` (`.itr.db` in this repo): `itr ready`, `itr claim
<id>`, `itr close <id> "<reason>"`. Convenience recipes: `just next`,
`just issues`, `just close`.

## Layout

| file | owns |
|---|---|
| `src/main.rs` | CLI (clap), dispatch, run/last/errors/log/gc orchestration |
| `src/runner.rs` | spawn, merged-pipe tee, timeout kill (process group), exit codes |
| `src/parser.rs` | ANSI strip, stream parse, error blocks, tail ring, auto sniff |
| `src/adapters.rs` | built-in regex sets + argv sniffing + `.gatr.toml` adapters |
| `src/storage.rs` | state dir, project slug, meta files, retention |
| `src/config.rs` | `.gatr.toml` |
| `src/summary.rs` | contract line + human rendering (contract tests live here) |
| `src/upgrade.rs` | self-update |
| `src/version_shape.rs` | git-describe → SemVer shaping (shared with build.rs) |
