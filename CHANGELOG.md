# Changelog

All notable user-facing changes are recorded here.

## Versioning

- Tags are `vMAJOR.MINOR.PATCH`, created automatically by
  `.github/workflows/auto-version.yml` on pushes to `main`:
  `type!:` / `BREAKING CHANGE:` → major, `feat:` → minor, `fix:` → patch,
  anything else → no tag. Add `[skip version]` to the commit message to skip.
- Before tagging, the same workflow syncs `Cargo.toml` and `Cargo.lock` to the
  new version in a `chore(release): … [skip version]` commit, and the tag
  points at that commit.
- `.github/workflows/release.yml` builds `gatr-<tag>-<target>` archives (+
  `.sha256`) for 7 targets from each tag into a draft release, and publishes
  it only once all 14 assets are uploaded, so `/releases/latest` never points
  at a release that is missing downloads. GitHub release notes are
  auto-generated.
- Binaries embed `git describe` via `build.rs` (`GATR_VERSION`), so
  `gatr --version` reports the real tag; the synced `Cargo.toml` version is the
  fallback for source builds without git metadata.

## Entry Format

Newest first. `### Release notes` for user-visible changes, `### Upgrade
notes` for compatibility/install/migration notes. Group bullets as Added /
Changed / Fixed / Docs / CI.

## v0.3.0 — 2026-09-26

### Release notes

- Fixed: `install.ps1` runs on Windows PowerShell 5.1. The BOM-less script
  decoded as CP1252 there, turning an em-dash into a curly quote that ended a
  string early, so the whole script failed to parse. TLS 1.2 is now forced and
  downloads use `-UseBasicParsing`.
- Fixed: on PowerShell 7, a missing `.sha256` no longer hard-fails the install;
  a real checksum mismatch still does.
- Fixed: downloads on Windows PowerShell 5.1 are no longer throttled by the
  progress bar, and the caller's `$ProgressPreference` is restored afterwards
  (the documented `iex` install runs in the caller's session).
- Changed: `install.ps1` adds the install directory to the User PATH and the
  current session instead of only printing a warning. Re-running is a no-op.
- CI: installer smoke test on Windows PowerShell 5.1 and 7 whenever the
  installer changes; draft-first release publishing; manifest sync on every
  auto-version tag.

### Upgrade notes

- New Windows installs default to `%LOCALAPPDATA%\Programs\gatr` (was
  `%LOCALAPPDATA%\gatr\bin`). An existing `gatr` on PATH is updated in place,
  and `%USERPROFILE%\.cargo\bin` still wins when present.

## v0.2.0

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
