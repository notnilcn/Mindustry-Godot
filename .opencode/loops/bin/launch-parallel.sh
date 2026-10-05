#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Spawn N autonomous parity loop runners from one orchestrator session.
# Each loop gets its own Xvfb display, editor bridge port, worktree, editor,
# MCP shims, and a headless `opencode run --agent loop-runner` child process
# that runs up to K iterations of .opencode/command/parity-loop.md and commits
# on its own parity/loop-N branch. Nothing is merged or pushed.
#
# Usage:
#   launch-parallel.sh <loop>=<scope> [<loop>=<scope> ...] [--iterations K]
#   launch-parallel.sh --plan FILE [--iterations K]
#   launch-parallel.sh --dry-run <loop>=<scope> ...   # print, do not spawn
#
# Examples:
#   launch-parallel.sh 1=ui/menu 2=input --iterations 2
#   launch-parallel.sh --plan .opencode/loops/plans/current.plan
#
# Plan file format (blank lines and # comments ignored):
#   1=ui/menu
#   2=input,ui/hud
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars "${PARITY_LOOP:-1}"

usage() { sed -n '3,21p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

iterations=2
plan_file=""
dry_run=0
specs=()
while [ $# -gt 0 ]; do
  case "$1" in
    --iterations) iterations="${2:?--iterations needs a value}"; shift 2 ;;
    --iterations=*) iterations="${1#*=}"; shift ;;
    --plan) plan_file="${2:?--plan needs a path}"; shift 2 ;;
    --plan=*) plan_file="${1#*=}"; shift ;;
    --dry-run) dry_run=1; shift ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      specs+=("$1")
      shift
      ;;
  esac
done
case "$iterations" in
  '' | *[!0-9]*) echo "launch-parallel: invalid --iterations: $iterations" >&2; exit 2 ;;
esac
[ "$iterations" -ge 1 ] || { echo "launch-parallel: --iterations must be >= 1" >&2; exit 2; }

if [ -n "$plan_file" ]; then
  [ -f "$plan_file" ] || { echo "launch-parallel: plan not found: $plan_file" >&2; exit 2; }
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%%#*}"
    line="$(printf '%s' "$line" | tr -d '[:space:]')"
    [ -n "$line" ] || continue
    specs+=("$line")
  done <"$plan_file"
fi
[ "${#specs[@]}" -gt 0 ] || { echo "launch-parallel: no loop specs given" >&2; usage; exit 2; }

# The loop-runner agent must be available even in older worktrees, so install
# the repo copy into the global agents dir when it is missing or outdated.
global_agent="$HOME/.config/opencode/agents/loop-runner.md"
repo_agent="$PARITY_MAIN/.opencode/agent/loop-runner.md"
if [ "$dry_run" -eq 0 ] && [ -f "$repo_agent" ]; then
  mkdir -p "$(dirname "$global_agent")"
  if ! cmp -s "$repo_agent" "$global_agent"; then
    cp "$repo_agent" "$global_agent"
    echo "[parallel] installed loop-runner agent at $global_agent"
  fi
fi

failures=0
launched=()
for spec in "${specs[@]}"; do
  id="${spec%%=*}"
  scope="${spec#*=}"
  case "$id" in
    '' | *[!0-9]*) echo "launch-parallel: bad spec '$spec' (want N=scope)" >&2; exit 2 ;;
  esac
  [ "$scope" != "$spec" ] && [ -n "$scope" ] || {
    echo "launch-parallel: spec '$spec' must be N=scope" >&2
    exit 2
  }

  echo "[parallel] preparing loop $id (scope: $scope, iterations: $iterations)"
  parity_loop_vars "$id"

  if [ "$dry_run" -eq 0 ]; then
    if ! "$script_dir/start-loop.sh" "$id" --reuse-bridge >/dev/null; then
      echo "[parallel] loop $id: setup failed; skipping" >&2
      failures=$((failures + 1))
      continue
    fi
    parity_loop_vars "$id" # start-loop may have created the worktree

    # Fail closed on a dirty parallel worktree: an in-flight edit is a real
    # conflict, and a runner could commit or overwrite it.
    if [ "$id" -ge 2 ] && [ -n "$(git -C "$PARITY_WORKTREE" status --porcelain)" ]; then
      echo "[parallel] loop $id: worktree has uncommitted changes; refusing to run (commit or stash first)" >&2
      git -C "$PARITY_WORKTREE" status --short >&2
      failures=$((failures + 1))
      continue
    fi

    # Fast-forward a clean worktree that has no loop commits yet, so each run
    # starts from current main (and gets the latest tooling).
    if [ "$id" -ge 2 ] && [ -d "$PARITY_WORKTREE/.git" -o -f "$PARITY_WORKTREE/.git" ] \
      && [ "$(git -C "$PARITY_MAIN" rev-list --count "main..parity/loop-$id")" = "0" ] \
      && ! git -C "$PARITY_MAIN" merge-base --is-ancestor main "parity/loop-$id"; then
      git -C "$PARITY_WORKTREE" merge --ff-only main >/dev/null
      echo "[parallel] loop $id worktree synced to main"
    fi

    mkdir -p "$PARITY_LOOP_DIR/logs"
    if ! ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
      echo "[parallel] loop $id editor starting on $PARITY_DISPLAY (bridge $PARITY_BRIDGE_PORT)"
      (PARITY_LOOP="$id" nohup "$script_dir/run-godot-editor.sh" \
        >>"$PARITY_LOOP_DIR/logs/editor.log" 2>&1 &)
      bound=0
      for _ in $(seq 1 180); do
        if ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
          bound=1
          break
        fi
        sleep 1
      done
      if [ "$bound" -ne 1 ]; then
        echo "[parallel] loop $id: editor did not bind $PARITY_BRIDGE_PORT; see $PARITY_LOOP_DIR/logs/editor.log" >&2
        failures=$((failures + 1))
        continue
      fi
    fi
  fi

  prompt="$(cat <<EOF
You are parity loop $id in an autonomous parallel batch.

Scope: $scope
Iteration budget: $iterations (stop earlier when no open finding in scope remains)

Loop resources (already exported to your environment): PARITY_LOOP=$id, display
$PARITY_DISPLAY, editor bridge port $PARITY_BRIDGE_PORT, worktree
$PARITY_WORKTREE, shared ledger $PARITY_LEDGER, run-dir prefix
${PARITY_RUN_PREFIX:-<none>}, runtime dir $PARITY_LOOP_DIR.

Protocol for each iteration:
1. Read \`.opencode/command/parity-loop.md\` and follow it.
2. The evaluator subagent does evaluation and re-verification; spawn it with the
   Task tool exactly as that command describes. If the ledger has no open finding
   in scope, have it evaluate the next uncovered scenario in scope.
3. You own the fix: claim atomically with
   \`record_finding.py claim --area "$scope" --owner loop-$id\`, fix it, run the
   narrow checks, then commit on this branch with \`Fixes EV-XXXX\` (pre-approved;
   never merge, push, rebase, or switch branches).
4. Never ask the user questions; on a blocker, stop and report it exactly.

Final report: loop id; iterations run; findings fixed and verified-fixed with
evidence paths; commit shas and subjects; blockers; uncovered scenarios.
EOF
)"

  if [ "$dry_run" -eq 1 ]; then
    echo "[parallel] dry-run loop $id"
    echo "  display=$PARITY_DISPLAY bridge=$PARITY_BRIDGE_PORT worktree=$PARITY_WORKTREE"
    echo "  prompt saved to: $PARITY_LOOP_DIR/agent-prompt.txt (dry-run does not write)"
    echo "  child: opencode run --auto --agent loop-runner --dir $PARITY_WORKTREE ..."
    continue
  fi

  mkdir -p "$PARITY_LOOP_DIR/logs"
  printf '%s' "$prompt" >"$PARITY_LOOP_DIR/agent-prompt.txt"
  (
    PARITY_LOOP="$id" DISPLAY="$PARITY_DISPLAY" PATH="$PARITY_MCP_BIN:$PATH" \
      nohup opencode run --auto --agent loop-runner \
      --dir "$PARITY_WORKTREE" --title "parity loop $id: $scope" "$prompt" \
      >"$PARITY_LOOP_DIR/logs/agent.log" 2>&1 &
    echo $! >"$PARITY_LOOP_DIR/agent.pid"
  )
  pid="$(cat "$PARITY_LOOP_DIR/agent.pid")"
  printf '{"loop": %s, "scope": "%s", "iterations": %s, "started": "%s", "pid": %s}\n' \
    "$id" "$scope" "$iterations" "$(date -Is)" "$pid" >"$PARITY_LOOP_DIR/plan.json"
  echo "[parallel] loop $id runner pid $pid -> $PARITY_LOOP_DIR/logs/agent.log"
  launched+=("$id")
done

echo
if [ "$dry_run" -eq 1 ]; then
  echo "[parallel] dry run complete; nothing was launched"
  exit 0
fi
echo "[parallel] started ${#launched[@]} runner(s): ${launched[*]:-none}"
[ "$failures" -eq 0 ] || echo "[parallel] $failures loop(s) failed setup" >&2
"$script_dir/parallel-status.sh"
[ "$failures" -eq 0 ] || exit 1
