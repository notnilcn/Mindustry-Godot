#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Prepare one parity loop and optionally start an opencode session in it.
#
# Usage:
#   start-loop.sh <loop-id>            # set up display/worktree/dirs, print env
#   start-loop.sh <loop-id> --run      # set up, then exec opencode in the worktree
#
# Every loop runs on its own X display, started as Xvfb when down (loop 1 uses
# an inherited display or :10; loop N >= 2 uses :(9+N)), with its own git
# worktree (branch parity/loop-N) and bridge port (6970, 6980, 6990, ...).
#
# Options:
#   --run             exec opencode in the loop worktree when setup succeeds
#   --no-auto         launch opencode without --auto (approval prompts enabled)
#   --reuse-bridge    do not fail when the loop's bridge port is already in
#                     use (launch-parallel.sh reuses a running editor)
#   --no-seed-builds  do not copy client/bin/rust and client/.godot into a new
#                     worktree (a from-scratch Rust/Godot build is expensive)
#   --allow-drift     start even when the worktree's workflow files
#                     (.opencode, AGENTS.md) differ from the main checkout
#
# Loop sessions launch with `opencode --auto` by default: permission asks are
# auto-approved inside the loop, while explicit deny rules (the evaluator's
# game-code/commit/push blocks) still hold. Set PARITY_OPENCODE_AUTO=0 or pass
# --no-auto to keep prompting.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"

usage() {
  sed -n '3,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

run_opencode=0
seed_builds=1
auto_approve="${PARITY_OPENCODE_AUTO:-1}"
reuse_bridge=0
allow_drift=0
id=""
for arg in "$@"; do
  case "$arg" in
    --run) run_opencode=1 ;;
    --auto) auto_approve=1 ;;
    --no-auto) auto_approve=0 ;;
    --reuse-bridge) reuse_bridge=1 ;;
    --no-seed-builds) seed_builds=0 ;;
    --allow-drift) allow_drift=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    '' | *[!0-9]*)
      echo "start-loop: unknown argument: $arg" >&2
      usage
      exit 2
      ;;
    *) id="$arg" ;;
  esac
done
parity_loop_vars "${id:-${PARITY_LOOP:-1}}"

mkdir -p "$PARITY_LOOP_DIR"/{logs,xdg-data,xdg-config,xdg-cache,home}

# 1) dedicated X display: every loop gets one, started when missing
parity_ensure_display

# 2) git worktree for loops >= 2
if [ "$PARITY_LOOP" -ge 2 ]; then
  if git -C "$PARITY_MAIN" worktree list --porcelain | grep -qxF "worktree $PARITY_WORKTREE"; then
    echo "[loop $PARITY_LOOP] worktree present: $PARITY_WORKTREE"
  elif [ -e "$PARITY_WORKTREE" ]; then
    echo "[loop $PARITY_LOOP] ERROR: $PARITY_WORKTREE exists but is not a registered worktree" >&2
    exit 1
  elif git -C "$PARITY_MAIN" show-ref --verify --quiet "refs/heads/parity/loop-$PARITY_LOOP"; then
    git -C "$PARITY_MAIN" worktree add "$PARITY_WORKTREE" "parity/loop-$PARITY_LOOP"
  else
    git -C "$PARITY_MAIN" worktree add -b "parity/loop-$PARITY_LOOP" "$PARITY_WORKTREE" HEAD
  fi

  if [ "$seed_builds" -eq 1 ]; then
    for rel in client/bin/rust client/.godot; do
      if [ ! -e "$PARITY_WORKTREE/$rel" ] && [ -e "$PARITY_MAIN/$rel" ]; then
        echo "[loop $PARITY_LOOP] seeding $rel from the main checkout (first run only)"
        mkdir -p "$(dirname "$PARITY_WORKTREE/$rel")"
        cp -a "$PARITY_MAIN/$rel" "$PARITY_WORKTREE/$rel"
      fi
    done
  fi

  # The session loads commands, agents and skills from this worktree, but
  # record_finding.py and the shared ledger resolve to $PARITY_MAIN. Refuse to
  # start when the two disagree: a stale loop branch or uncommitted workflow
  # edits on main would invoke a helper whose CLI no longer matches the
  # command that called it.
  workflow_drift=()
  for rel in .opencode/agent .opencode/command .opencode/skills \
    .opencode/plugin .opencode/loops AGENTS.md; do
    if [ ! -e "$PARITY_MAIN/$rel" ] && [ ! -e "$PARITY_WORKTREE/$rel" ]; then
      continue
    fi
    if ! diff -rq -x run -x __pycache__ "$PARITY_MAIN/$rel" "$PARITY_WORKTREE/$rel" \
      >/dev/null 2>&1; then
      workflow_drift+=("$rel")
    fi
  done
  if [ "${#workflow_drift[@]}" -gt 0 ]; then
    echo "[loop $PARITY_LOOP] workflow files differ from the main checkout:" >&2
    printf '  - %s\n' "${workflow_drift[@]}" >&2
    echo "  This session loads commands/agents from the worktree, but the ledger helper" >&2
    echo "  and shared ledger resolve to main; the mismatch can call a helper with a" >&2
    echo "  different CLI (unknown --for/--status values)." >&2
    echo "  Refresh the worktree from main (git -C $PARITY_WORKTREE merge main), then" >&2
    echo "  restart. Pass --allow-drift to start anyway." >&2
    if [ "$allow_drift" -ne 1 ]; then
      exit 1
    fi
    echo "[loop $PARITY_LOOP] WARNING: continuing despite workflow drift (--allow-drift)" >&2
  fi
fi

# 3) bridge-port ownership for loops >= 2 (loop 1 may already hold 6970)
if [ "$PARITY_LOOP" -ge 2 ] && command -v ss >/dev/null 2>&1; then
  if ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
    if [ "$reuse_bridge" -eq 1 ]; then
      echo "[loop $PARITY_LOOP] bridge port $PARITY_BRIDGE_PORT already in use; reusing the running editor"
    else
      echo "[loop $PARITY_LOOP] ERROR: bridge port $PARITY_BRIDGE_PORT is already in use." >&2
      echo "  A stale editor may be holding it; stop it or choose another loop id" >&2
      echo "  (or pass --reuse-bridge to use the editor that is already listening)." >&2
      exit 1
    fi
  fi
fi

# 4) session environment
export PATH="$PARITY_MCP_BIN:$PATH"
export DISPLAY="$PARITY_DISPLAY"

cat <<EOF
[parity-loop $PARITY_LOOP] ready
  display:      $PARITY_DISPLAY
  bridge port:  $PARITY_BRIDGE_PORT  (editor addon listens; MCP shim connects)
  worktree:     $PARITY_WORKTREE
  evals dir:    $PARITY_EVALS_DIR
  ledger:       $PARITY_LEDGER
  run prefix:   '${PARITY_RUN_PREFIX}'  (prefix run dirs with this)
  runtime dir:  $PARITY_LOOP_DIR
  MCP shims:    $PARITY_MCP_BIN  (prepended to PATH)
  auto-approve: $([ "$auto_approve" -eq 1 ] && echo "--auto (asks approved; explicit denies stay)" || echo "off (permission prompts enabled)")

  editor:  $script_dir/run-godot-editor.sh &
  java:    $script_dir/run-java.sh
EOF

if [ "$run_opencode" -eq 1 ]; then
  if [ "$auto_approve" -eq 1 ]; then
    echo "[parity-loop $PARITY_LOOP] starting opencode --auto in $PARITY_WORKTREE"
    cd "$PARITY_WORKTREE"
    exec opencode --auto
  fi
  echo "[parity-loop $PARITY_LOOP] starting opencode in $PARITY_WORKTREE"
  cd "$PARITY_WORKTREE"
  exec opencode
fi
