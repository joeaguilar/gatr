#!/usr/bin/env bash
set -euo pipefail

# gatr uninstaller — removes the binary and (optionally) recorded gate state.
#
# Usage:
#   ./uninstall.sh            # remove the binary only
#   ./uninstall.sh --purge    # also remove ~/.local/state/gatr (all logs/meta)

PURGE=0
case "${1:-}" in
    --purge) PURGE=1 ;;
    "") ;;
    *) echo "unknown argument: $1"; exit 2 ;;
esac

BIN_PATH="$(command -v gatr 2>/dev/null || true)"
if [ -n "$BIN_PATH" ]; then
    if [ -w "$(dirname "$BIN_PATH")" ]; then
        rm -f "$BIN_PATH"
    else
        sudo rm -f "$BIN_PATH"
    fi
    echo "removed $BIN_PATH"
else
    echo "gatr not found on PATH; nothing to remove"
fi

if [ "$PURGE" = "1" ]; then
    STATE="${GATR_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/gatr}"
    if [ -d "$STATE" ]; then
        rm -rf "$STATE"
        echo "removed state dir $STATE"
    fi
fi
