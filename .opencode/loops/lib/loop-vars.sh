#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Shared resource map for parallel parity loops (see ../README.md).
#
# Usage:
#   . "/path/to/.opencode/loops/lib/loop-vars.sh"
#   parity_loop_vars [loop-id]     # defaults to $PARITY_LOOP or 1
#
# Exported per loop:
#   PARITY_LOOP        numeric loop id (1-based)
#   PARITY_DISPLAY     X display for this loop (:10 or an inherited display for
#                      loop 1, :(9+N) for loop N; started as Xvfb when down)
#   PARITY_BRIDGE_PORT open-godot-mcp bridge port (editor addon listens)
#   PARITY_DAP_PORT    documented DAP port for this loop (inert, Godot-owned)
#   PARITY_LSP_PORT    documented LSP port for this loop (inert, Godot-owned)
#   PARITY_WORKTREE    git worktree / checkout this loop runs in
#   PARITY_EVALS_DIR   shared evals directory (main checkout)
#   PARITY_LEDGER      shared findings.json (flock-protected)
#   PARITY_LOOP_DIR    per-loop runtime dir (pid/log/xdg/home)
#   PARITY_RUN_PREFIX  run-dir prefix ("" for loop 1, "l<N>-" otherwise)
#   PARITY_MCP_BIN     PATH shim dir (pins DISPLAY/bridge port for MCP)

_parity_loops_dir() {
  local src="${BASH_SOURCE[0]}"
  (cd "$(dirname "$src")/.." && pwd)
}

parity_loop_vars() {
  local id="${1:-${PARITY_LOOP:-1}}"
  case "$id" in
    '' | *[!0-9]*) echo "invalid loop id: '$id'" >&2; return 2 ;;
  esac
  [ "$id" -ge 1 ] || { echo "loop id must be >= 1" >&2; return 2; }

  local loops_dir main inherited default_display
  loops_dir="$(_parity_loops_dir)"
  main="$(cd "$loops_dir/../.." && pwd)"
  inherited="${DISPLAY:-:10}"
  inherited="${inherited%%.*}"

  PARITY_LOOP="$id"
  PARITY_LOOPS_DIR="$loops_dir"
  PARITY_MAIN="$main"
  if [ "$id" -eq 1 ]; then
    default_display="$inherited"
    PARITY_WORKTREE="$main"
    PARITY_RUN_PREFIX=""
  else
    default_display=":$((9 + id))"
    PARITY_WORKTREE="$(dirname "$main")/Mindustry-Godot-loop$id"
    PARITY_RUN_PREFIX="l${id}-"
  fi
  # Idempotent across repeated calls in one shell: only an explicit
  # PARITY_DISPLAY_OVERRIDE beats the per-loop default.
  PARITY_DISPLAY="${PARITY_DISPLAY_OVERRIDE:-$default_display}"
  PARITY_BRIDGE_PORT=$((6970 + 10 * (id - 1)))
  PARITY_DAP_PORT=$((6006 + 10 * (id - 1)))
  PARITY_LSP_PORT=$((6005 + 10 * (id - 1)))
  PARITY_EVALS_DIR="$main/.opencode/evals"
  PARITY_LEDGER="$PARITY_EVALS_DIR/findings.json"
  PARITY_LOOP_DIR="$loops_dir/run/loop-$id"
  PARITY_MCP_BIN="$loops_dir/mcp-bin"

  export PARITY_LOOP PARITY_LOOPS_DIR PARITY_MAIN PARITY_DISPLAY \
    PARITY_WORKTREE PARITY_RUN_PREFIX PARITY_BRIDGE_PORT PARITY_DAP_PORT \
    PARITY_LSP_PORT PARITY_EVALS_DIR PARITY_LEDGER PARITY_LOOP_DIR PARITY_MCP_BIN
}

parity_ensure_display() {
  # Ensure this loop's X display exists; start a managed Xvfb when it is down.
  # No-op when a server already owns the display. Only local `:N` displays can
  # be created here; a host-qualified display is reported as unusable.
  local display="${PARITY_DISPLAY:-}"
  if [ -z "$display" ]; then
    echo "parity_ensure_display: PARITY_DISPLAY is not set" >&2
    return 2
  fi
  case "$display" in
    :[0-9]*) ;;
    *)
      echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: display $display is not a local :N display; cannot start Xvfb" >&2
      return 1
      ;;
  esac
  local num="${display#:}"
  case "$num" in
    '' | *[!0-9]*)
      echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: display $display is not a local :N display; cannot start Xvfb" >&2
      return 1
      ;;
  esac

  local sock="/tmp/.X11-unix/X$num"
  [ -S "$sock" ] && return 0
  if ! command -v Xvfb >/dev/null 2>&1; then
    echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: $display is down and Xvfb is not installed" >&2
    return 1
  fi

  mkdir -p "$PARITY_LOOP_DIR"
  echo "[parity-loop ${PARITY_LOOP:-?}] starting Xvfb on $display (1280x720x24)" >&2
  nohup Xvfb "$display" -screen 0 1280x720x24 -ac -nolisten tcp \
    >"$PARITY_LOOP_DIR/xvfb.log" 2>&1 &
  echo "$!" >"$PARITY_LOOP_DIR/xvfb.pid"
  local _
  for _ in $(seq 1 100); do
    [ -S "$sock" ] && return 0
    sleep 0.1
  done
  echo "[parity-loop ${PARITY_LOOP:-?}] ERROR: Xvfb failed on $display; see $PARITY_LOOP_DIR/xvfb.log" >&2
  return 1
}

parity_loop_java() {
  # Echo the path to a usable JDK 17 java binary, or return 1.
  local candidate
  for candidate in "${JAVA17_HOME:-}" "${JAVA_HOME:-}" \
    /usr/lib/jvm/java-17-openjdk-amd64 /usr/lib/jvm/java-17-openjdk \
    /usr/lib/jvm/temurin-17-jdk-amd64; do
    if [ -n "$candidate" ] && [ -x "$candidate/bin/java" ] \
      && "$candidate/bin/java" -version 2>&1 | grep -q '"17\.'; then
      printf '%s\n' "$candidate/bin/java"
      return 0
    fi
  done
  if command -v java >/dev/null 2>&1 && java -version 2>&1 | grep -q '"17\.'; then
    readlink -f "$(command -v java)"
    return 0
  fi
  return 1
}
