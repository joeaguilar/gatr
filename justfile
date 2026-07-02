# gatr development recipes

default:
    @just --list

# ── Build ──────────────────────────────────────────────

build:
    cargo build

release:
    cargo build --release

check:
    cargo check

install: release
    cargo install --path . --force

# ── Test ───────────────────────────────────────────────

test-unit:
    cargo test

test: release
    ./tests/integration.sh

test-debug: build
    ./tests/integration.sh target/debug/gatr

# ── Lint / Format ──────────────────────────────────────

lint:
    cargo clippy --all-targets -- -D warnings

deny:
    cargo deny check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# ── Aggregate gates ────────────────────────────────────

verify: release lint test-unit test fmt-check deny

ci: fmt-check lint deny test-unit test

clean:
    cargo clean

# ── Dogfood ────────────────────────────────────────────

# Run the ci gate through gatr itself
gate: release
    ./target/release/gatr run --tag ci -- just ci

# ── Issue tracker (itr) ────────────────────────────────

next:
    itr ready -f json

issues:
    itr list

issue title:
    itr add "{{title}}"

close id reason:
    itr close {{id}} "{{reason}}"

note id summary:
    itr note {{id}} "{{summary}}"

stats:
    itr stats

# ── Info ───────────────────────────────────────────────

deps:
    cargo tree --depth 1

size:
    ls -lh target/release/gatr

loc:
    wc -l src/*.rs
