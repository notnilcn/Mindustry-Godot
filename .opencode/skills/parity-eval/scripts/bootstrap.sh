#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Bootstrap the host dependencies for the parity-eval skill:
#   - computer-mcp (PyPI) and open-godot-mcp (notnilcn/Open-Godot-MCP) as
#     uv tools, so the executables appear in ~/.local/bin (opencode's global
#     mcp config calls them by bare name)
#   - a JDK 17 for the Java Mindustry reference: uses an existing Java 17 if
#     one is on PATH / in JAVA_HOME, otherwise downloads Temurin into
#     $JAVA17_HOME
#
# Usage:
#   .opencode/skills/parity-eval/scripts/bootstrap.sh          # install + check
#   .opencode/skills/parity-eval/scripts/bootstrap.sh --check  # verify only
#
# Idempotent. No sudo required. If uv is absent, falls back to a venv under
# $MCP_VENV; optional host tools (xdotool, wmctrl, tesseract) are only
# reported, never installed.

set -euo pipefail

MCP_VENV="${MCP_VENV:-$HOME/.local/share/mcp-venv}"
OGM_REPO="${OGM_REPO:-https://github.com/notnilcn/Open-Godot-MCP}"
OGM_SRC="${OGM_SRC:-$HOME/.local/share/Open-Godot-MCP}"
JAVA17_HOME="${JAVA17_HOME:-$HOME/.local/share/jdks/temurin-17}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
COMPUTER_MCP_VERSION="${COMPUTER_MCP_VERSION:-0.0.7}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
ADDON_VERSION="$(sed -n 's/^version="\(.*\)"/\1/p' \
  "$REPO_ROOT/client/addons/open_godot_mcp/plugin.cfg" 2>/dev/null || true)"

CHECK_ONLY=0
if [ "${1:-}" = "--check" ]; then
  CHECK_ONLY=1
fi

log() { printf '[parity-eval] %s\n' "$*"; }
warn() { printf '[parity-eval] WARN: %s\n' "$*" >&2; }
die() { printf '[parity-eval] ERROR: %s\n' "$*" >&2; exit 1; }

is_java17() {
  [ -x "$1/bin/java" ] && "$1/bin/java" -version 2>&1 | grep -q '"17\.'
}

# Prefer an existing JDK 17 (PATH, JAVA_HOME, distro package, Temurin dir).
find_jdk17() {
  local candidate
  for candidate in "${JAVA_HOME:-}" "$JAVA17_HOME" \
      /usr/lib/jvm/java-17-openjdk-amd64 /usr/lib/jvm/java-17-openjdk \
      /usr/lib/jvm/temurin-17-jdk-amd64; do
    if [ -n "$candidate" ] && is_java17 "$candidate"; then
      JAVA17_HOME="$candidate"
      return 0
    fi
  done
  if command -v java >/dev/null 2>&1 && java -version 2>&1 | grep -q '"17\.'; then
    JAVA17_HOME="$(dirname "$(dirname "$(readlink -f "$(command -v java)")")")"
    return 0
  fi
  return 1
}

install_mcps() {
  if command -v uv >/dev/null 2>&1; then
    log "installing MCP servers with uv tool (binaries land in $BIN_DIR)"
    # computer-mcp 0.0.7 uses the MCP SDK 1.x Server API; 2.x removed it.
    uv tool install --force --quiet "computer-mcp==${COMPUTER_MCP_VERSION}" --with "mcp<2"
    uv tool install --force --quiet "git+${OGM_REPO}"
    return
  fi

  log "uv not found; falling back to venv $MCP_VENV"
  command -v python3 >/dev/null || die "python3 is required"
  [ -x "$MCP_VENV/bin/python" ] || python3 -m venv "$MCP_VENV"
  "$MCP_VENV/bin/pip" install --quiet --upgrade pip
  "$MCP_VENV/bin/pip" install --quiet "computer-mcp==${COMPUTER_MCP_VERSION}" "mcp<2"
  if [ ! -d "$OGM_SRC/.git" ]; then
    mkdir -p "$(dirname "$OGM_SRC")"
    git clone --depth 1 "$OGM_REPO" "$OGM_SRC"
  fi
  "$MCP_VENV/bin/pip" install --quiet "$OGM_SRC"
  mkdir -p "$BIN_DIR"
  ln -sf "$MCP_VENV/bin/computer-mcp" "$BIN_DIR/computer-mcp"
  ln -sf "$MCP_VENV/bin/open-godot-mcp" "$BIN_DIR/open-godot-mcp"
}

install_jdk() {
  is_java17 "$JAVA17_HOME" && return 0
  local arch api_arch tmp
  arch="$(uname -m)"
  case "$arch" in
    x86_64) api_arch=x64 ;;
    aarch64 | arm64) api_arch=aarch64 ;;
    *) die "unsupported arch for JDK download: $arch (install JDK 17 manually)" ;;
  esac
  log "downloading Temurin JDK 17 ($api_arch) into $JAVA17_HOME"
  mkdir -p "$JAVA17_HOME"
  tmp="$(mktemp -d)"
  curl -fL --retry 3 \
    "https://api.adoptium.net/v3/binary/latest/17/ga/linux/${api_arch}/jdk/hotspot/normal/eclipse" \
    -o "$tmp/jdk.tar.gz"
  tar -xzf "$tmp/jdk.tar.gz" -C "$JAVA17_HOME" --strip-components=1
  rm -rf "$tmp"
}

check_deps() {
  local failed=0
  command -v open-godot-mcp >/dev/null 2>&1 \
    || { warn "open-godot-mcp not on PATH (expected $BIN_DIR/open-godot-mcp)"; failed=1; }
  command -v computer-mcp >/dev/null 2>&1 \
    || { warn "computer-mcp not on PATH (expected $BIN_DIR/computer-mcp)"; failed=1; }
  command -v godot4 >/dev/null 2>&1 || command -v godot >/dev/null 2>&1 \
    || { warn "Godot binary not on PATH (set GODOT_BIN)"; failed=1; }

  if ! find_jdk17; then
    warn "no JDK 17 found on PATH, JAVA_HOME, /usr/lib/jvm, or $JAVA17_HOME"
    failed=1
  fi

  if command -v open-godot-mcp >/dev/null 2>&1 && [ -n "$ADDON_VERSION" ]; then
    local server_ver
    server_ver="$(open-godot-mcp --version 2>/dev/null | awk '{print $NF}')"
    if [ -n "$server_ver" ] && [ "$server_ver" != "$ADDON_VERSION" ]; then
      warn "open-godot-mcp $server_ver != vendored addon $ADDON_VERSION (bridge protocol drift)"
      failed=1
    fi
  fi

  command -v xdotool >/dev/null || log "optional: xdotool absent (window-geometry automation limited)"
  command -v wmctrl >/dev/null || log "optional: wmctrl absent"
  command -v tesseract >/dev/null || log "optional: tesseract absent (OCR comparison disabled)"

  if [ "$failed" -ne 0 ]; then
    die "preconditions not met; run bootstrap.sh without --check"
  fi
  log "all required preconditions present"
  log "JAVA_HOME=$JAVA17_HOME"
}

if [ "$CHECK_ONLY" -eq 0 ]; then
  install_mcps
  install_jdk
fi
check_deps

log "next: tools/build.sh && JAVA_HOME=$JAVA17_HOME bash ../Mindustry/gradlew -p ../Mindustry tools:pack desktop:dist"
