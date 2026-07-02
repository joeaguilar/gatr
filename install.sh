#!/usr/bin/env bash
set -euo pipefail

# gatr installer — downloads a prebuilt binary from the latest GitHub Release,
# falling back to a source build when no release asset fits (or on request).
#
# Usage:
#   ./install.sh                # install or update
#   ./install.sh --update       # same thing, explicit
#   curl -fsSL https://raw.githubusercontent.com/joeaguilar/gatr/main/install.sh | bash
#
# Environment overrides:
#   GATR_VERSION      pin a release tag (default: latest)
#   GATR_INSTALL_DIR  install directory override
#   GATR_FROM_SOURCE  1 = build from source instead of downloading
#   GATR_REPO         GitHub repo (default: joeaguilar/gatr)

REPO="${GATR_REPO:-joeaguilar/gatr}"
BIN="gatr"

usage() {
    sed -n '4,15p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
}

case "${1:-}" in
    -h|--help) usage ;;
    --update|update|--install|install|"") ;;
    *) echo "unknown argument: $1 (try --help)"; exit 2 ;;
esac

log() { printf '%s\n' "$*" >&2; }
die() { log "ERROR: $*"; exit 1; }

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin)
            case "$arch" in
                arm64)  echo "aarch64-apple-darwin" ;;
                x86_64) echo "x86_64-apple-darwin" ;;
                *) die "unsupported macOS arch: $arch" ;;
            esac ;;
        Linux)
            case "$arch" in
                # Default to fully-static musl on x86_64 so it runs on any glibc.
                x86_64)          echo "x86_64-unknown-linux-musl" ;;
                aarch64|arm64)   echo "aarch64-unknown-linux-gnu" ;;
                *) die "unsupported Linux arch: $arch" ;;
            esac ;;
        MINGW*|MSYS*|CYGWIN*)
            die "use install.ps1 on Windows" ;;
        *) die "unsupported OS: $os" ;;
    esac
}

resolve_version() {
    if [ -n "${GATR_VERSION:-}" ]; then
        echo "$GATR_VERSION"
        return
    fi
    # Follow the /releases/latest redirect instead of hitting the API
    # (no rate limits, no token needed).
    local url
    if command -v curl >/dev/null 2>&1; then
        url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest")" || true
    elif command -v wget >/dev/null 2>&1; then
        url="$(wget -q --max-redirect=10 --server-response -O /dev/null "https://github.com/${REPO}/releases/latest" 2>&1 | awk '/Location:/{u=$2} END{print u}')" || true
    else
        die "need curl or wget"
    fi
    case "$url" in
        */tag/*) echo "${url##*/}" ;;
        *) echo "" ;;
    esac
}

fetch() { # fetch <url> <dest>
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$2" "$1"
    else
        wget -q -O "$2" "$1"
    fi
}

choose_install_dir() {
    if [ -n "${GATR_INSTALL_DIR:-}" ]; then
        echo "${GATR_INSTALL_DIR/#\~/$HOME}"
        return
    fi
    # Prefer replacing whatever the shell currently resolves.
    local existing
    existing="$(command -v "$BIN" 2>/dev/null || true)"
    if [ -n "$existing" ]; then
        dirname "$existing"
        return
    fi
    if [ -d "$HOME/.cargo/bin" ] && case ":$PATH:" in *":$HOME/.cargo/bin:"*) true ;; *) false ;; esac; then
        echo "$HOME/.cargo/bin"
        return
    fi
    echo "$HOME/.local/bin"
}

install_binary() { # install_binary <src>
    local dir
    dir="$(choose_install_dir)"
    mkdir -p "$dir" 2>/dev/null || true
    if [ -w "$dir" ]; then
        install -m 0755 "$1" "$dir/$BIN"
    else
        log "escalating with sudo to write $dir"
        sudo install -m 0755 "$1" "$dir/$BIN"
    fi
    log "installed $BIN to $dir/$BIN"
    case ":$PATH:" in
        *":$dir:"*) ;;
        *) log "WARNING: $dir is not on PATH — add: export PATH=\"$dir:\$PATH\"" ;;
    esac
    log "version: $("$dir/$BIN" --version)"
}

install_from_source() {
    command -v cargo >/dev/null 2>&1 || die "cargo not found; cannot build from source"
    [ -f Cargo.toml ] || die "run from the gatr repo root to build from source"
    log "building from source..."
    cargo build --release
    install_binary target/release/$BIN
}

install_from_release() {
    local target tag asset base tmp
    target="$(detect_target)"
    tag="$(resolve_version)"
    [ -n "$tag" ] || { log "could not resolve latest release; falling back to source build"; install_from_source; return; }
    base="${BIN}-${tag}-${target}"
    asset="https://github.com/${REPO}/releases/download/${tag}/${base}.tar.gz"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    log "downloading ${base}.tar.gz ..."
    if ! fetch "$asset" "$tmp/${base}.tar.gz"; then
        log "download failed; falling back to source build"
        install_from_source
        return
    fi
    if fetch "${asset}.sha256" "$tmp/${base}.tar.gz.sha256"; then
        (cd "$tmp" && { sha256sum -c "${base}.tar.gz.sha256" 2>/dev/null || shasum -a 256 -c "${base}.tar.gz.sha256"; }) \
            || die "checksum verification failed"
    else
        log "WARNING: no checksum file published for $tag; skipping verification"
    fi
    tar xzf "$tmp/${base}.tar.gz" -C "$tmp"
    [ -f "$tmp/$base/$BIN" ] || die "archive did not contain $BIN"
    install_binary "$tmp/$base/$BIN"
}

if [ "${GATR_FROM_SOURCE:-0}" = "1" ]; then
    install_from_source
else
    install_from_release
fi

log ""
log "quick start:"
log "  gatr run --tag ci -- just ci     # wrap any gate"
log "  gatr last                        # was the gate green?"
log "  gatr errors                      # just the error blocks"
log "  gatr --help"
