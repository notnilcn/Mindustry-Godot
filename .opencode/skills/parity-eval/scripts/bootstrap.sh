#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Bootstrap the host dependencies for the parity-eval skill:
#   - open-godot-mcp (notnilcn/Open-Godot-MCP) as a uv tool, so the
#     executable appears in ~/.local/bin (opencode's global mcp config calls
#     it by bare name)
#   - the parity-eval helper venv for the Java leg shell helpers
#     (java_input.py, window_ctl.py, capture_screen.py, ...): mss, Pillow,
#     python-xlib, pynput; the venv's python is $PARITY_PY
#   - a JDK 17 for the Java Mindustry reference: uses an existing Java 17 if
#     one is on PATH / in JAVA_HOME, otherwise downloads Temurin into
#     $JAVA17_HOME
#
# Usage:
#   .opencode/skills/parity-eval/scripts/bootstrap.sh          # install + check
#   .opencode/skills/parity-eval/scripts/bootstrap.sh --check  # verify only
#
# Idempotent. No sudo required. If uv is absent, falls back to a venv under
# $MCP_VENV for open-godot-mcp; the Java-leg venv is plain python3 -m venv.
# Optional host tools (xdotool, wmctrl, tesseract) are only reported, never
# installed.

set -euo pipefail

PARITY_VENV="${PARITY_VENV:-$HOME/.local/share/parity-eval/venv}"
PARITY_PY="${PARITY_PY:-$PARITY_VENV/bin/python}"
MCP_VENV="${MCP_VENV:-$HOME/.local/share/mcp-venv}"
OGM_REPO="${OGM_REPO:-https://github.com/notnilcn/Open-Godot-MCP}"
OGM_SRC="${OGM_SRC:-$HOME/.local/share/Open-Godot-MCP}"
JAVA17_HOME="${JAVA17_HOME:-$HOME/.local/share/jdks/temurin-17}"
BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
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

install_ogm() {
  if command -v uv >/dev/null 2>&1; then
    log "installing open-godot-mcp with uv tool (binary lands in $BIN_DIR)"
    uv tool install --force --quiet "git+${OGM_REPO}"
    return
  fi

  log "uv not found; falling back to venv $MCP_VENV"
  command -v python3 >/dev/null || die "python3 is required"
  [ -x "$MCP_VENV/bin/python" ] || python3 -m venv "$MCP_VENV"
  "$MCP_VENV/bin/pip" install --quiet --upgrade pip
  if [ ! -d "$OGM_SRC/.git" ]; then
    mkdir -p "$(dirname "$OGM_SRC")"
    git clone --depth 1 "$OGM_REPO" "$OGM_SRC"
  fi
  "$MCP_VENV/bin/pip" install --quiet "$OGM_SRC"
  mkdir -p "$BIN_DIR"
  ln -sf "$MCP_VENV/bin/open-godot-mcp" "$BIN_DIR/open-godot-mcp"
}

# The Java-leg helpers run on their own venv so no MCP server package is
# involved; pins match the versions the workflow was verified against.
install_helpers() {
  log "installing the parity-eval helper venv at $PARITY_VENV"
  if command -v uv >/dev/null 2>&1; then
    [ -x "$PARITY_VENV/bin/python" ] || uv venv --quiet "$PARITY_VENV"
    uv pip install --quiet --python "$PARITY_PY" \
      mss==10.2.0 pillow==12.3.0 python-xlib==0.33 pynput==1.8.2
    return
  fi

  command -v python3 >/dev/null || die "python3 is required"
  [ -x "$PARITY_VENV/bin/python" ] || python3 -m venv "$PARITY_VENV"
  "$PARITY_VENV/bin/pip" install --quiet --upgrade pip
  "$PARITY_VENV/bin/pip" install --quiet \
    mss==10.2.0 pillow==12.3.0 python-xlib==0.33 pynput==1.8.2
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
  command -v godot4 >/dev/null 2>&1 || command -v godot >/dev/null 2>&1 \
    || { warn "Godot binary not on PATH (set GODOT_BIN)"; failed=1; }

  if [ ! -x "$PARITY_PY" ]; then
    warn "parity-eval helper venv python missing at $PARITY_PY (run bootstrap.sh)"
    failed=1
  elif ! "$PARITY_PY" -c 'import importlib.util, sys; missing = [m for m in ("mss", "PIL", "Xlib", "pynput") if importlib.util.find_spec(m) is None]; sys.exit("missing modules: " + ", ".join(missing) if missing else 0)'; then
    warn "parity-eval helper venv is missing modules (run bootstrap.sh to repair)"
    failed=1
  fi

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
  log "PARITY_PY=$PARITY_PY"
}

if [ "$CHECK_ONLY" -eq 0 ]; then
  install_ogm
  install_helpers
  install_jdk
fi
check_deps

log "next: tools/build.sh; Java reference jar: ../Mindustry/desktop/build/libs/Mindustry.jar (rebuild: JAVA_HOME=$JAVA17_HOME bash ../Mindustry/gradlew -p ../Mindustry tools:pack desktop:dist)"
