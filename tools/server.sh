#!/usr/bin/env bash
# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# Plan 22 M2 server wrapper: run `mind-headless server` or drive a running
# server's console socket (`tools/server.sh --socket "status,players"`).
#
# Usage:
#   tools/server.sh [server args...]                 # run the dedicated server
#   tools/server.sh --socket "status,exit" [--host H] [--port P]
#   tools/server.sh --socket "..." --serve --config-dir DIR  # start + drive

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${MIND_HEADLESS_BIN:-$REPO_ROOT/client/rust/target/debug/mind-headless}"
HOST="${MIND_SERVER_HOST:-127.0.0.1}"
PORT="${MIND_SERVER_PORT:-6859}"

SOCKET=""
SERVE=0
CONFIG_DIR="config"
SERVER_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --socket) SOCKET="$2"; shift 2 ;;
    --host) HOST="$2"; shift 2 ;;
    --port) PORT="$2"; shift 2 ;;
    --config-dir) CONFIG_DIR="$2"; SERVER_ARGS+=(--config-dir "$2"); shift 2 ;;
    --serve) SERVE=1; shift ;;
    *) SERVER_ARGS+=("$1"); shift ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "server.sh: building mind-headless..." >&2
  cargo build --manifest-path "$REPO_ROOT/client/rust/Cargo.toml" -p mind-headless >&2
fi

if [[ -z "$SOCKET" ]]; then
  exec "$BIN" server "${SERVER_ARGS[@]}"
fi

if (( SERVE )); then
  "$BIN" server --config-dir "$CONFIG_DIR" --socket-port "$PORT" &
  SERVER_PID=$!
  trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT
  # Wait for the socket to come up.
  for _ in $(seq 1 100); do
    if (exec 3<>"/dev/tcp/$HOST/$PORT") 2>/dev/null; then exec 3<&- 3>&-; break; fi
    sleep 0.1
  done
fi

exec 3<>"/dev/tcp/$HOST/$PORT"
IFS=',' read -ra COMMANDS <<< "$SOCKET"
for cmd in "${COMMANDS[@]}"; do
  printf '%s\n' "$cmd" >&3
done
cat <&3
exec 3<&- 3>&-
