#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Sourced guard for resource-heavy entry points (tools/ci.sh, build.sh,
# pack.sh ...). It re-execs the calling script through the packaged resource
# queue at tools/ai-tools/resource-gate/run-resource-intensive-task.sh, so a
# direct `bash tools/build.sh` still queues behind whatever else is running.
#
# Requires the caller to have set REPO_ROOT. Nested invocations (a script call
# made by a task already holding the queue lock) inherit RESOURCE_QUEUE_HELD
# and run normally instead of queueing again.
#
# See docs/CI-WORKFLOW.md and tools/RESOURCE_INTENSIVE_TASKS_CATALOGUE.md.

# usage: mind_cpu_guard <task-name> <absolute-script-path> [script args...]
mind_cpu_guard() {
  local name="$1"
  local script="$2"
  shift 2

  if [[ -n "${RESOURCE_QUEUE_HELD:-}" || -n "${MIND_CPU_QUEUE_HELD:-}" || -n "${VN_CPU_QUEUE_HELD:-}" ]]; then
    return 0
  fi
  if [[ "${RESOURCE_QUEUE_DISABLE:-${MIND_CPU_QUEUE_DISABLE:-0}}" == "1" ]]; then
    echo "resource-queue: WARNING: RESOURCE_QUEUE_DISABLE=1, $name is running WITHOUT the lock" >&2
    return 0
  fi

  # Help must not block behind a long build.
  local arg
  for arg in "$@"; do
    case "$arg" in
      -h|--help) return 0 ;;
    esac
  done

  local wrapper="$REPO_ROOT/tools/ai-tools/resource-gate/run-resource-intensive-task.sh"
  if [[ ! -f "$wrapper" ]]; then
    echo "resource-queue: packaged queue not found: $wrapper" >&2
    echo "  Initialize the ai-tools submodule first: git submodule update --init tools/ai-tools" >&2
    return 1
  fi

  exec bash "$wrapper" --name "$name" -- bash "$script" "$@"
}
