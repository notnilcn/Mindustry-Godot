#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Show every loop launched by launch-parallel.sh: runner state, bridge, commits,
# scope, and the tail of its agent log.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars 1

printf '%-4s %-8s %-6s %-8s %-5s %-22s %s\n' LOOP STATE BRIDGE COMMITS DISPLAY SCOPE PID
found=0
for dir in "$PARITY_LOOPS_DIR"/run/loop-*; do
  [ -d "$dir" ] || continue
  [ -f "$dir/plan.json" ] || continue
  id="${dir##*loop-}"
  parity_loop_vars "$id"

  pid="$(cat "$dir/agent.pid" 2>/dev/null || true)"
  state="stopped"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    state="running"
  fi
  bridge="down"
  if ss -ltn 2>/dev/null | grep -qE ":${PARITY_BRIDGE_PORT}[[:space:]]"; then
    bridge="up"
  fi
  scope="$(python3 -c "import json;print(json.load(open('$dir/plan.json')).get('scope','?'))" 2>/dev/null || echo '?')"
  commits=0
  if [ "$id" -ge 2 ] && [ -d "$PARITY_WORKTREE/.git" -o -f "$PARITY_WORKTREE/.git" ]; then
    commits="$(git -C "$PARITY_WORKTREE" rev-list --count main..HEAD 2>/dev/null || echo 0)"
  fi
  printf '%-4s %-8s %-6s %-8s %-5s %-22s %s\n' \
    "$id" "$state" "$bridge" "$commits" "$PARITY_DISPLAY" "$scope" "${pid:--}"
  found=1
done
if [ "$found" -eq 0 ]; then
  echo "no loops launched by launch-parallel.sh"
  exit 0
fi

echo
echo "commits on loop branches:"
for dir in "$PARITY_LOOPS_DIR"/run/loop-*; do
  [ -f "$dir/plan.json" ] || continue
  id="${dir##*loop-}"
  [ "$id" -ge 2 ] || continue
  parity_loop_vars "$id"
  [ -d "$PARITY_WORKTREE/.git" -o -f "$PARITY_WORKTREE/.git" ] || continue
  branch_commits="$(git -C "$PARITY_WORKTREE" log --oneline main..HEAD 2>/dev/null | head -4)"
  if [ -n "$branch_commits" ]; then
    echo "  [$id] $branch_commits"
  fi
done

echo
echo "ledger (open/in-progress/regression):"
python3 "$PARITY_MAIN/.opencode/skills/parity-eval/scripts/record_finding.py" \
  --ledger "$PARITY_LEDGER" list 2>/dev/null | python3 -c '
import collections, json, sys
rows = json.load(sys.stdin)
counts = collections.Counter()
for f in rows:
    if f["status"] in ("open", "in-progress", "regression"):
        counts[(f.get("owner") or "unowned", f["status"])] += 1
if not counts:
    print("  (none)")
for key in sorted(counts):
    print(f"  {key[0]:<12} {key[1]:<12} {counts[key]}")
'

echo
echo "log tails:"
for dir in "$PARITY_LOOPS_DIR"/run/loop-*; do
  [ -f "$dir/plan.json" ] || continue
  id="${dir##*loop-}"
  echo "  [loop $id] $(tail -n 1 "$dir/logs/agent.log" 2>/dev/null | cut -c1-160)"
done
