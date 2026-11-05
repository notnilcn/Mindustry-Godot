#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# run-cpu-intensive-task.sh — one-at-a-time queue for builds, tests and gates.
#
# Several agents sharing this checkout must never run `tools/ci.sh`, `cargo
# build`, an asset pack or a parity gate concurrently: the linker and rustc
# will happily consume every core until the machine falls over. Every heavy
# task therefore runs under one exclusive `flock`; a second invocation waits in
# line and then runs with the full machine available.
#
# Usage:
#   tools/run-cpu-intensive-task.sh [queue-options] <task> [task-arguments...]
#
# Tasks (the remaining arguments are forwarded to the underlying command):
#   --ci                  full local gate (tools/ci.sh); the CI fixer's job
#   --build               tools/build.sh (mind-gdext + mind-headless)
#   --cargo-build         cargo build --manifest-path client/rust/Cargo.toml
#   --cargo-test          cargo test  --manifest-path client/rust/Cargo.toml
#   --cargo-check         cargo check --manifest-path client/rust/Cargo.toml
#   --cargo-fmt           cargo fmt --all (in place)
#   --cargo-clippy        cargo clippy --workspace --all-targets
#   --cargo-run           cargo run --manifest-path client/rust/Cargo.toml
#   --godot               tools/godot.sh (headless one-shots, NOT an editor session)
#   --pack                tools/pack.sh (defaults to `pack`)
#   --parity              tools/parity.sh
#   --pcam-parity         tools/pcam_parity.sh
#   --export              tools/export.sh
#   --goldens             tools/regen_goldens.sh
#   --stdb                server/build.sh
#   --bench               release bench_baseline (mind-headless)
#   --cmd -- CMD...       arbitrary command under the lock
#
# Queue options:
#   --status              show the holder, waiters and last history event
#   --list                list the named tasks
#   --history [N]         last N queue events (default 10)
#   --no-wait             do not queue: exit 75 when the lock is busy
#   --timeout SECS        stop waiting after SECS (0 = forever, default)
#   --quiet               suppress the queue start/finish lines
#   --dry-run             print the resolved command and exit without locking
#   -h, --help            this help
#
# Exit codes: the task's own status; 75 when busy/timeout (EX_TEMPFAIL); 2 usage.
#
# Environment:
#   MIND_CPU_QUEUE_DIR     state directory (default: <git-common-dir>/
#                          mindustry-cpu-queue, shared by every worktree)
#   MIND_CPU_QUEUE_TIMEOUT default for --timeout
#   MIND_CPU_QUEUE_QUIET   default for --quiet
#   MIND_CPU_QUEUE_DISABLE=1  escape hatch for debugging the queue only; it
#                             prints a warning and runs without the lock
#   MIND_CPU_QUEUE_HELD    internal: set for commands run under the lock, so
#                          nested heavy scripts do not deadlock on it
#
# See docs/CI-WORKFLOW.md and tools/AGENTS.md.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# Resolve cargo/godot even from non-login shells (CI, MCP tools).
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
MANIFEST="$REPO_ROOT/client/rust/Cargo.toml"

# State lives beside the git common dir, not under the checkout, so subagents in
# separate `git worktree`s contend for the same lock as the main checkout.
# MIND_CPU_QUEUE_DIR overrides it (absolute, or relative to the repo root).
resolve_queue_dir() {
  if [[ -n "${MIND_CPU_QUEUE_DIR:-}" ]]; then
    local override="$MIND_CPU_QUEUE_DIR"
    [[ "$override" = /* ]] || override="$REPO_ROOT/$override"
    printf '%s\n' "$override"
    return 0
  fi
  local common
  if common="$(git -C "$REPO_ROOT" rev-parse --git-common-dir 2>/dev/null)" && [[ -n "$common" ]]; then
    [[ "$common" = /* ]] || common="$REPO_ROOT/$common"
    if common="$(cd -- "$common" 2>/dev/null && pwd)"; then
      printf '%s/mindustry-cpu-queue\n' "$common"
      return 0
    fi
  fi
  printf '%s/runs/cpu-queue\n' "$REPO_ROOT"
}

QUEUE_DIR="$(resolve_queue_dir)"
LOCK_FILE="$QUEUE_DIR/lock"
HOLDER_FILE="$QUEUE_DIR/holder"
WAITER_DIR="$QUEUE_DIR/waiters"
HISTORY_FILE="$QUEUE_DIR/history.jsonl"

TASK=""
TASK_NAME=""
TASK_ARGS=()
TASK_CMD=()
ACTION="run"
WAIT_MODE=1
TIMEOUT="${MIND_CPU_QUEUE_TIMEOUT:-0}"
QUIET="${MIND_CPU_QUEUE_QUIET:-0}"
DRY_RUN=0
HISTORY_N=10
WAITER_FILE=""
HELD=0
WAITED=0
START_EPOCH=0

die() {
  printf 'run-cpu-intensive-task.sh: %s\n' "$*" >&2
  exit 2
}

usage() {
  sed -n '4,56p' "${BASH_SOURCE[0]}" | sed -e 's/^# \{0,1\}//' -e '/^$/d'
}

list_tasks() {
  cat <<'EOF'
CPU-intensive tasks (one at a time):
  --ci               full local gate (tools/ci.sh)
  --build            tools/build.sh (GDExtension + headless oracle)
  --cargo-build      cargo build (workspace)
  --cargo-test       cargo test (workspace)
  --cargo-check      cargo check (workspace)
  --cargo-fmt        cargo fmt --all (in place)
  --cargo-clippy     cargo clippy --workspace --all-targets
  --cargo-run        cargo run (workspace)
  --godot            tools/godot.sh (headless one-shots)
  --pack             tools/pack.sh (defaults to `pack`)
  --parity           tools/parity.sh
  --pcam-parity      tools/pcam_parity.sh
  --export           tools/export.sh
  --goldens          tools/regen_goldens.sh
  --stdb             server/build.sh
  --bench            release bench_baseline
  --cmd -- CMD...    arbitrary command under the lock
EOF
}

# --- queue state helpers ----------------------------------------------------

queue_dir_prepare() {
  mkdir -p "$QUEUE_DIR" "$WAITER_DIR"
}

history_append() {
  # $1=event $2=rc(or "null") $3=extra json members (leading comma)
  local event="$1" rc="${2:-null}" extra="${3:-}"
  local task
  task="$(printf '%s' "$TASK_NAME" | sed 's/\\/\\\\/g; s/"/\\"/g')"
  printf '{"at":"%s","event":"%s","task":"%s","pid":%s,"rc":%s%s}\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$event" "$task" "$$" "$rc" "$extra" \
    >>"$HISTORY_FILE"
}

write_holder() {
  local command
  command="$(printf '%s ' "${TASK_CMD[@]}" | tr '\n' ' ')"
  {
    printf 'task=%s\n' "$TASK_NAME"
    printf 'pid=%s\n' "$$"
    printf 'started=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'cwd=%s\n' "$PWD"
    printf 'host=%s\n' "$(hostname 2>/dev/null || echo unknown)"
    printf 'command=%s\n' "$command"
  } >"$HOLDER_FILE.tmp"
  mv -f "$HOLDER_FILE.tmp" "$HOLDER_FILE"
}

write_waiter() {
  WAITER_FILE="$WAITER_DIR/$$.waiter"
  {
    printf 'task=%s\n' "$TASK_NAME"
    printf 'pid=%s\n' "$$"
    printf 'since=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf 'cwd=%s\n' "$PWD"
  } >"$WAITER_FILE"
}

holder_field() {
  local key="$1"
  [[ -f "$HOLDER_FILE" ]] || return 0
  sed -n "s/^${key}=//p" "$HOLDER_FILE" 2>/dev/null | head -n 1 || true
}

prune_waiters() {
  local f pid
  [[ -d "$WAITER_DIR" ]] || return 0
  for f in "$WAITER_DIR"/*.waiter; do
    [[ -e "$f" ]] || continue
    pid="${f##*/}"
    pid="${pid%.waiter}"
    [[ "$pid" =~ ^[0-9]+$ ]] || continue
    if ! kill -0 "$pid" 2>/dev/null; then
      rm -f "$f"
    fi
  done
}

print_status() {
  prune_waiters
  local busy=0
  if [[ -e "$LOCK_FILE" ]] && command -v flock >/dev/null 2>&1; then
    exec 8>"$LOCK_FILE"
    if ! flock -n 8; then
      busy=1
    fi
    exec 8>&-
  fi

  if (( busy == 0 )); then
    echo "cpu-queue: idle"
    if [[ -f "$HOLDER_FILE" ]]; then
      echo "  (stale holder state; the lock is free, so it is ignored)"
    fi
  else
    echo "cpu-queue: busy"
    if [[ -f "$HOLDER_FILE" ]]; then
      sed 's/^/  /' "$HOLDER_FILE"
    else
      echo "  holder state not written yet"
    fi
    local count=0 pid f
    for f in "$WAITER_DIR"/*.waiter; do
      [[ -e "$f" ]] || continue
      pid="${f##*/}"
      pid="${pid%.waiter}"
      count=$((count + 1))
      printf '  waiting: pid %s: %s\n' "$pid" "$(tr '\n' ' ' <"$f")"
    done
    if (( count > 0 )); then
      echo "  queued behind: $count task(s)"
    fi
  fi

  if [[ -s "$HISTORY_FILE" ]]; then
    echo "  last: $(tail -n 1 "$HISTORY_FILE")"
  fi
  printf '  state: %s\n' "$QUEUE_DIR"
}

print_history() {
  if [[ ! -s "$HISTORY_FILE" ]]; then
    echo "cpu-queue: no history yet ($HISTORY_FILE)"
    return 0
  fi
  tail -n "$HISTORY_N" "$HISTORY_FILE"
}

cleanup() {
  local rc=$?
  trap - EXIT INT TERM
  if [[ -n "$WAITER_FILE" ]]; then
    rm -f "$WAITER_FILE"
  fi
  if (( HELD == 1 )); then
    local duration=$(( $(date +%s) - START_EPOCH ))
    history_append "finish" "$rc" ",\"duration_s\":$duration,\"waited_s\":$WAITED"
    rm -f "$HOLDER_FILE"
    if (( QUIET == 0 )); then
      printf 'cpu-queue: %s exit %d (%ss)\n' "$TASK_NAME" "$rc" "$duration"
    fi
    exec 9>&- 2>/dev/null || true
  fi
  exit "$rc"
}

# --- argument parsing -------------------------------------------------------

while (( $# > 0 )); do
  case "$1" in
    --ci) TASK=ci; TASK_NAME=ci; shift; TASK_ARGS=("$@"); break ;;
    --build) TASK=build; TASK_NAME=build; shift; TASK_ARGS=("$@"); break ;;
    --cargo-build) TASK=cargo-build; TASK_NAME=cargo-build; shift; TASK_ARGS=("$@"); break ;;
    --cargo-test) TASK=cargo-test; TASK_NAME=cargo-test; shift; TASK_ARGS=("$@"); break ;;
    --cargo-check) TASK=cargo-check; TASK_NAME=cargo-check; shift; TASK_ARGS=("$@"); break ;;
    --cargo-fmt) TASK=cargo-fmt; TASK_NAME=cargo-fmt; shift; TASK_ARGS=("$@"); break ;;
    --cargo-clippy) TASK=cargo-clippy; TASK_NAME=cargo-clippy; shift; TASK_ARGS=("$@"); break ;;
    --cargo-run) TASK=cargo-run; TASK_NAME=cargo-run; shift; TASK_ARGS=("$@"); break ;;
    --godot) TASK=godot; TASK_NAME=godot; shift; TASK_ARGS=("$@"); break ;;
    --pack) TASK=pack; TASK_NAME=pack; shift; TASK_ARGS=("$@"); break ;;
    --parity) TASK=parity; TASK_NAME=parity; shift; TASK_ARGS=("$@"); break ;;
    --pcam-parity) TASK=pcam-parity; TASK_NAME=pcam-parity; shift; TASK_ARGS=("$@"); break ;;
    --export) TASK=export; TASK_NAME=export; shift; TASK_ARGS=("$@"); break ;;
    --goldens) TASK=goldens; TASK_NAME=goldens; shift; TASK_ARGS=("$@"); break ;;
    --stdb) TASK=stdb; TASK_NAME=stdb; shift; TASK_ARGS=("$@"); break ;;
    --bench) TASK=bench; TASK_NAME=bench; shift; TASK_ARGS=("$@"); break ;;
    --cmd|--raw)
      TASK=raw
      shift
      if (( $# > 0 )) && [[ "$1" == "--" ]]; then
        shift
      fi
      TASK_ARGS=("$@")
      [[ -n "$TASK_NAME" ]] || TASK_NAME="$(basename -- "${TASK_ARGS[0]:-raw}")"
      break
      ;;
    --name)
      (( $# >= 2 )) || die "--name needs a value"
      TASK_NAME="$2"
      TASK=raw
      shift 2
      if (( $# > 0 )) && [[ "$1" == "--" ]]; then
        shift
      fi
      TASK_ARGS=("$@")
      break
      ;;
    --status) ACTION=status; shift ;;
    --list) ACTION=list; shift ;;
    --history)
      ACTION=history
      shift
      if (( $# > 0 )) && [[ "$1" =~ ^[0-9]+$ ]]; then
        HISTORY_N="$1"
        shift
      fi
      ;;
    --no-wait) WAIT_MODE=0; shift ;;
    --timeout)
      (( $# >= 2 )) || die "--timeout needs a value"
      TIMEOUT="$2"
      shift 2
      ;;
    --quiet|-q) QUIET=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage; exit 0 ;;
    --) die "missing task before -- (see --help)" ;;
    *)
      printf 'run-cpu-intensive-task.sh: unknown argument %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

[[ "$TIMEOUT" =~ ^[0-9]+$ ]] || die "--timeout needs a whole number of seconds"

case "$ACTION" in
  status)
    print_status
    exit 0
    ;;
  list)
    list_tasks
    exit 0
    ;;
  history)
    print_history
    exit 0
    ;;
esac

[[ -n "$TASK" ]] || {
  printf 'run-cpu-intensive-task.sh: no task given\n\n' >&2
  usage >&2
  exit 2
}

# Task arguments are forwarded verbatim: `--cargo-clippy -- -D warnings` and
# `--cargo-fmt -- --check` reach cargo/rustfmt with their `--` separator intact.
[[ -n "$TASK_NAME" ]] || TASK_NAME="$TASK"

case "$TASK" in
  ci) TASK_CMD=(bash "$REPO_ROOT/tools/ci.sh") ;;
  build) TASK_CMD=(bash "$REPO_ROOT/tools/build.sh") ;;
  cargo-build) TASK_CMD=(cargo build --manifest-path "$MANIFEST") ;;
  cargo-test) TASK_CMD=(cargo test --manifest-path "$MANIFEST") ;;
  cargo-check) TASK_CMD=(cargo check --manifest-path "$MANIFEST") ;;
  cargo-fmt) TASK_CMD=(cargo fmt --manifest-path "$MANIFEST" --all) ;;
  cargo-clippy) TASK_CMD=(cargo clippy --manifest-path "$MANIFEST" --workspace --all-targets) ;;
  cargo-run) TASK_CMD=(cargo run --manifest-path "$MANIFEST") ;;
  godot) TASK_CMD=(bash "$REPO_ROOT/tools/godot.sh") ;;
  pack)
    TASK_CMD=(bash "$REPO_ROOT/tools/pack.sh")
    if (( ${#TASK_ARGS[@]} == 0 )); then
      TASK_ARGS=(pack)
    fi
    ;;
  parity) TASK_CMD=(bash "$REPO_ROOT/tools/parity.sh") ;;
  pcam-parity) TASK_CMD=(bash "$REPO_ROOT/tools/pcam_parity.sh") ;;
  export) TASK_CMD=(bash "$REPO_ROOT/tools/export.sh") ;;
  goldens) TASK_CMD=(bash "$REPO_ROOT/tools/regen_goldens.sh") ;;
  stdb) TASK_CMD=(bash "$REPO_ROOT/server/build.sh") ;;
  bench)
    TASK_CMD=(cargo run -q --release --manifest-path "$MANIFEST" -p mind-headless --
      bench --ticks 100000 --scenario bench_baseline)
    ;;
  raw)
    (( ${#TASK_ARGS[@]} > 0 )) || die '--cmd/--name needs a command (use --cmd -- CMD...)'
    TASK_CMD=("${TASK_ARGS[@]}")
    ;;
  *) die "unknown task '$TASK'" ;;
esac

if [[ "$TASK" != "raw" ]]; then
  TASK_CMD+=("${TASK_ARGS[@]}")
fi

if (( DRY_RUN == 1 )); then
  printf 'cpu-queue: would run: %s\n' "$(printf '%q ' "${TASK_CMD[@]}")"
  exit 0
fi

if [[ "${MIND_CPU_QUEUE_DISABLE:-0}" == "1" ]]; then
  echo "cpu-queue: WARNING: MIND_CPU_QUEUE_DISABLE=1, running '$TASK_NAME' WITHOUT the lock" >&2
  exec "${TASK_CMD[@]}"
fi

command -v flock >/dev/null 2>&1 || {
  echo "run-cpu-intensive-task.sh: 'flock' is required (util-linux; WSL/Linux has it)." >&2
  echo "  Use MIND_CPU_QUEUE_DISABLE=1 only to debug on a host without flock." >&2
  exit 1
}

# --- queue + run ------------------------------------------------------------

queue_dir_prepare
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

WAIT_START=$(date +%s)
exec 9>"$LOCK_FILE"
if ! flock -n 9; then
  if (( WAIT_MODE == 0 )); then
    echo "cpu-queue: busy, '$TASK_NAME' not started (--no-wait)" >&2
    [[ -f "$HOLDER_FILE" ]] && sed 's/^/  /' "$HOLDER_FILE" >&2
    exit 75
  fi

  write_waiter
  holder_task="$(holder_field task)"
  holder_pid="$(holder_field pid)"
  if [[ -n "$holder_task" ]]; then
    printf 'cpu-queue: waiting for %s (pid %s); queued as pid %s\n' \
      "$holder_task" "${holder_pid:-?}" "$$"
  else
    printf 'cpu-queue: waiting for the CPU queue lock; queued as pid %s\n' "$$"
  fi

  if (( TIMEOUT > 0 )); then
    if ! flock -w "$TIMEOUT" 9; then
      echo "cpu-queue: timed out after ${TIMEOUT}s waiting for the lock" >&2
      exit 75
    fi
  else
    flock 9
  fi
  rm -f "$WAITER_FILE"
  WAITER_FILE=""
fi

WAITED=$(( $(date +%s) - WAIT_START ))
HELD=1
START_EPOCH=$(date +%s)
write_holder
history_append "start" "null" ",\"waited_s\":$WAITED"
if (( QUIET == 0 )); then
  printf 'cpu-queue: %s running (pid %s, waited %ss)\n' "$TASK_NAME" "$$" "$WAITED"
fi

export MIND_CPU_QUEUE_HELD=1
export MIND_CPU_QUEUE_TASK="$TASK_NAME"
export MIND_CPU_QUEUE_PID="$$"

set +e
"${TASK_CMD[@]}"
rc=$?
set -e

exit "$rc"
