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
#   PARITY_DISPLAY     X display for this loop (:10 for loop 1, Xvfb :11+)
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
