#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Drain the parity ledger: keep N loop runners busy until no finding is left
# in open/code-verified/regression (`wontfix` and `verified-fixed` are
# terminal). Every time a runner exits, its slot gets the next-highest-priority
# scope from the ledger, skipping areas an active runner already covers.
#
# Usage: drain-parallel.sh [--slots N] [--ids 2,3] [--iterations K]
#                          [--max-rounds R] [--dry-run]
#
# Intended to be started from an orchestrator session (`nohup ... &`) and
# monitored with parallel-status.sh; nothing is merged or pushed. A runner that
# fails to launch (dirty worktree, setup error) stops the drain.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars "${PARITY_LOOP:-1}"

record="$PARITY_MAIN/.opencode/skills/parity-eval/scripts/record_finding.py"
run_dir="$PARITY_LOOPS_DIR/run"

slots=2
ids="2,3"
iterations=2
max_rounds=40
dry=0
while [ $# -gt 0 ]; do
  case "$1" in
    --slots) slots="${2:?--slots needs a value}"; shift 2 ;;
    --slots=*) slots="${1#*=}"; shift ;;
    --ids) ids="${2:?--ids needs a value}"; shift 2 ;;
    --ids=*) ids="${1#*=}"; shift ;;
    --iterations) iterations="${2:?--iterations needs a value}"; shift 2 ;;
    --iterations=*) iterations="${1#*=}"; shift ;;
    --max-rounds) max_rounds="${2:?--max-rounds needs a value}"; shift 2 ;;
    --max-rounds=*) max_rounds="${1#*=}"; shift ;;
    --dry-run) dry=1; shift ;;
    -h | --help)
      sed -n '3,12p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "drain-parallel: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done
case "$slots" in '' | *[!0-9]*) echo "drain-parallel: --slots must be numeric" >&2; exit 2 ;; esac
[ "$slots" -ge 1 ] && [ "$slots" -le 3 ] || {
  echo "drain-parallel: --slots must be 1..3 (display budget)" >&2
  exit 2
}
IFS=, read -r -a id_arr <<<"$ids"
[ "${#id_arr[@]}" -ge "$slots" ] || {
  echo "drain-parallel: --ids needs at least $slots ids" >&2
  exit 2
}
active_ids=("${id_arr[@]:0:slots}")

next_scope() { # [exclude-csv]
  if [ -n "${1:-}" ]; then
    python3 "$record" --ledger "$PARITY_LEDGER" next-scope --exclude "$1" 2>/dev/null || true
  else
    python3 "$record" --ledger "$PARITY_LEDGER" next-scope 2>/dev/null || true
  fi
}

declare -A scope_of=()
declare -A pid_of=()

in_use() {
  local out="" id
  for id in "${active_ids[@]}"; do
    [ -n "${scope_of[$id]:-}" ] && out="${out:+$out,}${scope_of[$id]}"
  done
  printf '%s' "$out"
}

alive() { # id
  local pid="${pid_of[$1]:-}"
  [ -n "$pid" ] && [ "$pid" != "dry" ] && kill -0 "$pid" 2>/dev/null
}

launch_into() { # id
  local id="$1" scope
  scope="$(next_scope "$(in_use)")"
  if [ -z "$scope" ] || [ "$scope" = "null" ]; then
    return 3
  fi
  scope_of[$id]="$scope"
  echo "[drain] loop $id -> $scope"
  if [ "$dry" -eq 1 ]; then
    pid_of[$id]="dry"
    return 0
  fi
  if ! "$script_dir/launch-parallel.sh" "$id=$scope" --iterations "$iterations"; then
    echo "[drain] loop $id failed to launch (dirty worktree or setup error above)" >&2
    return 1
  fi
  pid_of[$id]="$(cat "$run_dir/loop-$id/agent.pid")"
  return 0
}

round=0
while :; do
  round=$((round + 1))
  if [ "$round" -gt "$max_rounds" ]; then
    echo "[drain] max rounds ($max_rounds) reached; next pending: $(next_scope)" >&2
    exit 1
  fi

  for id in "${active_ids[@]}"; do
    alive "$id" && continue
    launch_into "$id" || {
      rc=$?
      [ "$rc" -eq 3 ] || exit 1
    }
  done

  if [ "$dry" -eq 1 ]; then
    exit 0
  fi

  any=0
  for id in "${active_ids[@]}"; do
    alive "$id" && any=1
  done
  pending="$(next_scope)"
  if [ "$any" -eq 0 ] && { [ -z "$pending" ] || [ "$pending" = "null" ]; }; then
    echo "[drain] ledger drained (no open/code-verified/regression findings)"
    exit 0
  fi
  sleep 30
done
