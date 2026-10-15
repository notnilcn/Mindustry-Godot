#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# mp-local.sh — local SpacetimeDB harness for two-client multiplayer work
# (I08 T1). `up` starts the standalone server if nothing listens on 3000,
# publishes the `mindustry_godot` module with `server/build.sh` and prints the
# address a client connects to. `down` stops what `up` started. `status`
# reports whether the server answers.
#
# Usage:
#   tools/mp-local.sh up       start + publish; print the db address
#   tools/mp-local.sh down     stop the local server started by `up`
#   tools/mp-local.sh status   listening/version/provider report
#
# Environment:
#   STDB_PORT       server port (default 3000)
#   STDB_LOG        server log file (default /tmp/opencode/stdb.log)
#   STDB_PID        pidfile for `down` (default /tmp/opencode/stdb.pid)
#   STDB_DB         database name (default mindustry)
set -euo pipefail

export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PORT="${STDB_PORT:-3000}"
LOG="${STDB_LOG:-/tmp/opencode/stdb.log}"
PIDFILE="${STDB_PID:-/tmp/opencode/stdb.pid}"
DB="${STDB_DB:-mindustry}"
ADDR="127.0.0.1:${PORT}/${DB}"

usage() {
  sed -n '3,17p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

listening() {
  ss -ltn 2>/dev/null | grep -qE ":${PORT}[[:space:]]"
}

wait_for_port() {
  local tries="${1:-120}"
  for _ in $(seq 1 "$tries"); do
    listening && return 0
    sleep 0.5
  done
  return 1
}

cmd_up() {
  command -v spacetime >/dev/null 2>&1 || {
    echo "mp-local: spacetime CLI not found on PATH (expected 2.10.1)" >&2
    exit 1
  }
  mkdir -p "$(dirname "$LOG")"
  if listening; then
    echo "mp-local: SpacetimeDB already listening on :${PORT} (reusing)"
  else
    echo "mp-local: starting SpacetimeDB on :${PORT} (log: $LOG)"
    setsid nohup spacetime start >"$LOG" 2>&1 </dev/null &
    echo $! >"$PIDFILE"
    if ! wait_for_port 120; then
      echo "mp-local: server never listened on :${PORT}; see $LOG" >&2
      exit 1
    fi
    echo "mp-local: server up (pid $(cat "$PIDFILE" 2>/dev/null || echo '?'))"
  fi
  echo "mp-local: publishing $DB via server/build.sh"
  (cd "$REPO_ROOT" && bash server/build.sh)
  echo "mp-local: database address: $ADDR"
}

cmd_down() {
  if [ -f "$PIDFILE" ]; then
    local pid
    pid="$(cat "$PIDFILE" 2>/dev/null || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      echo "mp-local: stopped SpacetimeDB (pid $pid)"
    fi
    rm -f "$PIDFILE"
  else
    echo "mp-local: no pidfile at $PIDFILE (nothing started by mp-local.sh)"
  fi
  if pgrep -f "spacetime start" >/dev/null 2>&1; then
    echo "mp-local: note: a 'spacetime start' process is still running:"
    pgrep -af "spacetime start" || true
  fi
}

cmd_status() {
  if listening; then
    echo "mp-local: listening on :${PORT} (address: $ADDR)"
    spacetime version 2>/dev/null | head -n 1 || true
    spacetime server ping local 2>&1 | tail -n 2 || true
  else
    echo "mp-local: nothing listening on :${PORT}"
  fi
}

case "${1:-}" in
  up) cmd_up ;;
  down) cmd_down ;;
  status) cmd_status ;;
  -h | --help | "") usage ;;
  *)
    echo "mp-local: unknown command '$1' (see --help)" >&2
    exit 2
    ;;
esac
