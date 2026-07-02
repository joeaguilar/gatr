#!/usr/bin/env bash
set -euo pipefail

# Integration test suite for gatr
# Usage: ./tests/integration.sh [--smoke] [path-to-gatr-binary]
#
# If no path is provided, uses ./target/release/gatr
# --smoke runs the minimal cross-platform checks used by the release matrix.

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
SMOKE=0
if [ "${1:-}" = "--smoke" ]; then
    SMOKE=1
    shift
fi

if [ "$#" -gt 0 ]; then
    GATR="$1"
    case "$GATR" in
        /*) ;;
        *) GATR="$(pwd)/$GATR" ;;
    esac
else
    GATR="$SCRIPT_DIR/target/release/gatr"
fi

if [ ! -x "$GATR" ]; then
    echo "Binary not found at $GATR — run 'cargo build --release' first"
    exit 1
fi

PASS=0
FAIL=0
fail() { echo "FAIL: $1"; FAIL=$((FAIL + 1)); }
pass() { PASS=$((PASS + 1)); }
check() { # check <description> <command...>
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then pass; else fail "$desc"; fi
}

WORK="$(mktemp -d)"
export GATR_STATE_DIR="$WORK/state"
PROJ="$WORK/proj"
mkdir -p "$PROJ"
cd "$PROJ"
trap 'rm -rf "$WORK"' EXIT

echo "== gatr integration ($GATR) =="

# ── Smoke: version + green run + contract line + last ──
"$GATR" --version | grep -q "gatr" || fail "--version prints name"
pass

OUT="$("$GATR" run --tag smoke -- git --version)"
echo "$OUT" | head -1 | grep -qE '^GATR exit=0 dur=[0-9.]+s errors=0 warnings=[0-9]+ adapter=\S+ tag=smoke log=\S+' \
    || fail "contract line shape"
pass

check "gatr last works" "$GATR" last --tag smoke
"$GATR" last --tag smoke --json | grep -q '"gatr_meta":1' || fail "meta schema version"
pass

if [ "$SMOKE" = "1" ]; then
    echo "smoke OK ($PASS checks)"
    exit 0
fi

# ── Exit code passthrough ──
set +e
"$GATR" run --quiet --tag red -- sh -c 'echo "error: boom"; exit 7'
CODE=$?
set -e
[ "$CODE" = "7" ] || fail "exit passthrough (got $CODE)"
pass

# ── Error block extraction (cargo adapter) ──
set +e
OUT="$("$GATR" run --tag fake --adapter cargo -- sh -c 'printf "error[E0308]: mismatched types\n  --> src/main.rs:4:20\n   |\nmore\n"; exit 1')"
set -e
echo "$OUT" | grep -q "── error 1/1 ──" || fail "error block header"
echo "$OUT" | grep -q -- "--> src/main.rs:4:20" || fail "continuation glued to block"
pass

"$GATR" errors --tag fake | grep -q "E0308" || fail "gatr errors reprints blocks"
pass

# ── Display filter never touches the log ──
"$GATR" run --quiet --tag filt --filter 'NumPy version' -- sh -c 'echo "warning: NumPy version 1.2"; echo real' >/dev/null
# the filtered warning is neither counted nor displayed (cmd field aside)...
"$GATR" last --tag filt --json | grep -q '"warnings":0' || fail "filtered warning was counted"
"$GATR" last --tag filt | tail -n +2 | grep -q "NumPy" && fail "filter leaked into displayed summary"
# ...but the full log retains it
grep -q "NumPy" "$("$GATR" log --tag filt)" || fail "filtered line missing from log"
pass

# ── gatr log --cat ──
"$GATR" log --tag filt --cat | grep -q "real" || fail "log --cat streams content"
pass

# ── last --project from unrelated cwd ──
(cd "$WORK" && "$GATR" last --project "$PROJ" --tag red --json | grep -q '"exit":7') \
    || fail "last --project resolves other repo"
pass

# ── JSON summary shape (§3.1 feed contract) ──
META="$("$GATR" last --tag red --json)"
for field in gatr_meta cmd tag adapter exit dur_s errors warnings started project_path log error_blocks tail; do
    echo "$META" | grep -q "\"$field\"" || fail "meta field $field"
done
pass

# ── Retention: 25 runs leave exactly 20 logs ──
for i in $(seq 1 25); do
    "$GATR" run --quiet --tag ret -- sh -c "echo run $i" >/dev/null
done
LOGS=$(ls "$GATR_STATE_DIR"/*/ | grep -c "_ret.*\.log$" || true)
[ "$LOGS" = "20" ] || fail "retention kept $LOGS logs, expected 20"
METAS=$(ls "$GATR_STATE_DIR"/*/ | grep -c "_ret.*\.meta\.json$" || true)
[ "$METAS" = "20" ] || fail "retention kept $METAS metas, expected 20"
pass

# ── Timeout kills and reports 124 ──
set +e
"$GATR" run --quiet --tag slow --timeout 1s -- sh -c 'sleep 30; echo done'
CODE=$?
set -e
[ "$CODE" = "124" ] || fail "timeout exit (got $CODE)"
pass

# ── gc ──
check "gatr gc runs" "$GATR" gc

echo "== $PASS passed, $FAIL failed =="
[ "$FAIL" = "0" ]
