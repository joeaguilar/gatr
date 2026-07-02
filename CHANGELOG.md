# Changelog

All notable user-facing changes are recorded here.

## Versioning

- Tags are `vMAJOR.MINOR.PATCH`, created automatically by
  `.github/workflows/auto-version.yml` on pushes to `main`:
  `type!:` / `BREAKING CHANGE:` → major, `feat:` → minor, `fix:` → patch,
  anything else → no tag. Add `[skip version]` to the commit message to skip.
- `.github/workflows/release.yml` builds `gatr-<tag>-<target>` archives (+
  `.sha256`) for 7 targets from each tag; GitHub release notes are
  auto-generated.
- Binaries embed `git describe` via `build.rs` (`GATR_VERSION`), so
  `gatr --version` reports the real tag. `Cargo.toml` stays pinned at `0.1.0`
  on purpose — it is only the fallback for source builds without git metadata.

## Entry Format

Newest first. `### Release notes` for user-visible changes, `### Upgrade
notes` for compatibility/install/migration notes. Group bullets as Added /
Changed / Fixed / Docs / CI.

## Unreleased

### Release notes

- Added: `gatr run` — wrap any verification command; full log teed to
  `~/.local/state/gatr/<project>/`, compact summary with the format-frozen
  `GATR …` contract line, exit code passthrough, `--tag`, `--adapter`
  (cargo/tsc/pytest/jest/eslint/generic + auto sniffing), `--filter`,
  `--tail`, `--errors`, `--quiet`, `--json`, `--timeout`.
- Added: `gatr last` (`--tag`, `--project`, `--json`) — reprint the most
  recent summary; the blessed query path of the §3.1 feed contract
  (`gatr_meta: 1` `.meta.json` sidecars).
- Added: `gatr errors [--all]`, `gatr log [--cat]`, `gatr gc [--all]`;
  retention of 20 logs per tag per project, pruned on each run.
- Added: `.gatr.toml` project config — always-on display filters, per-tag
  adapters, project-local adapters.
- Added: `gatr upgrade` self-update (source checkouts).
