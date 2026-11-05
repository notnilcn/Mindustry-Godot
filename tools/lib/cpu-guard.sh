#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Sourced guard for CPU-heavy entry points (tools/ci.sh, build.sh, pack.sh ...).
# It re-execs the calling script through tools/run-cpu-intensive-task.sh so a
# direct `bash tools/build.sh` still queues behind whatever else is running.
#
# Requires the caller to have set REPO_ROOT. Nested invocations (a script call
# made by a task already holding the queue lock) inherit MIND_CPU_QUEUE_HELD and
# run normally instead of queueing again.
#
# See docs/CI-WORKFLOW.md.

# usage: mind_cpu_guard <task-name> <absolute-script-path> [script args...]
mind_cpu_guard() {
  local name="$1"
  local script="$2"
  shift 2

  if [[ -n "${MIND_CPU_QUEUE_HELD:-}" ]]; then
    return 0
  fi
  if [[ "${MIND_CPU_QUEUE_DISABLE:-0}" == "1" ]]; then
    echo "cpu-queue: WARNING: MIND_CPU_QUEUE_DISABLE=1, $name is running WITHOUT the lock" >&2
    return 0
  fi

  # Help must not block behind a long build.
  local arg
  for arg in "$@"; do
    case "$arg" in
      -h|--help) return 0 ;;
    esac
  done

  exec bash "$REPO_ROOT/tools/run-cpu-intensive-task.sh" \
    --name "$name" -- bash "$script" "$@"
}
