<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Resource-intensive tasks catalogue

Every resource-intensive task in this repository runs one at a time through the
packaged queue at
[`tools/ai-tools/resource-gate/run-resource-intensive-task.sh`](ai-tools/resource-gate/run-resource-intensive-task.sh).
The wrapper and its wave monitor live in the `tools/ai-tools` submodule; this
page only records the repo-specific task map and wave rules.

## Tasks

| Task flag | Runs | Notes |
|---|---|---|
| `--ci` | `tools/ci.sh` | Full local gate. The CI fixer owns it; do not run it after every edit. |
| `--build` | `tools/build.sh` | `mind-gdext` + `mind-headless` + the Phantom Camera addon, then scenario sync. |
| `--cargo-build` | `cargo build` | Workspace build; forward cargo flags after the task flag. |
| `--cargo-test` | `cargo test` | Workspace tests; scope with `-p <crate>` and a filter. |
| `--cargo-check` | `cargo check` | Fast type check; the normal scoped check for a package agent. |
| `--cargo-fmt` | `cargo fmt --all` | In-place formatting; `-- --check` for the gate form. |
| `--cargo-clippy` | `cargo clippy --workspace --all-targets` | Add `-- -D warnings` for the gate form. |
| `--cargo-run` | `cargo run` | Runs a workspace binary; forwards its arguments. |
| `--godot` | `tools/godot.sh` | Headless one-shots only; never an interactive editor session. |
| `--pack` | `tools/pack.sh` | Asset pack; defaults to the `pack` subcommand. |
| `--parity` | `tools/parity.sh` | Parity harness driver. |
| `--export` | `tools/export.sh` | Godot export; heavy I/O and CPU. |
| `--goldens` | `tools/regen_goldens.sh` | Golden verification/regeneration. |
| `--stdb` | `server/build.sh` | SpacetimeDB module build/publish helpers. |
| `--bench` | release `bench_baseline` | `mind-headless bench --ticks 300000 --scenario bench_baseline`. |
| `--cmd -- CMD...` | arbitrary command | Runs under the same exclusive lock; use it for a wave sweep, not for ordinary file edits. |

## Queue controls

| Option | Effect |
|---|---|
| `--status` | Show the holder, waiters and last history event. |
| `--history [N]` | Print the last N queue events (default 10). |
| `--list` | List the named tasks. |
| `--no-wait` | Exit `75` instead of waiting when the lock is busy. |
| `--timeout SECS` | Bound the wait; `0` means wait forever. |
| `--quiet` | Suppress the start/finish lines. |
| `--dry-run` | Print the resolved command without taking the lock. |

State defaults to `<git-common-dir>/resource-queue/`. Override it with
`RESOURCE_QUEUE_DIR`; the legacy `MIND_CPU_QUEUE_DIR` name is still accepted.

## Wave guard

Before dispatching a wave of subagents, the orchestrator runs the packaged
monitor and keeps the wave at the width it recommends:

```bash
tools/ai-tools/resource-gate/resource-monitor.sh \
  --cpu-percent 85 --ram-percent 85 --disk-percent 90 --streak 3 --init
tools/ai-tools/resource-gate/resource-monitor.sh --log runs/resource-monitor.log
```

`--init` saves the limits to `tools/ai-tools/.resource-monitor-limits` (in the
submodule's ignored scratch area) and exits; later runs read them. `--once`
samples the current machine without taking the monitor lock. While the monitor
runs it prints a `recommended_parallel_subagents` value and, after the
configured streak over any limit, writes an alert JSON under
`.tmp/resource-monitor/alert.json` and exits `3`. Stop spawning, cut the width
6 → 4 → 2 → 1, kill only the process trees you started, and restart the monitor
after two clean samples. Queued work is exempt from the monitor's CPU/RAM/disk
checks, so a running build does not trigger it.

## Wave rules

- Package agents run only scoped checks (`--cargo-fmt`, `--cargo-check -p
  <crate> --all-targets`, scoped clippy, `--cargo-test -p <crate> [filter]`).
  They do not run `--ci`, `--build`, `--pack`, `--export`, `--stdb`, `--bench`,
  `--parity`, `--godot`, or `--goldens`.
- The orchestrator runs one `--cmd` wave sweep after every package reports:
  one lock acquisition, one row per worktree. Do not nest wrapper calls inside
  a sweep; the inner call would wait on the lock the sweep already holds.
- Share one `CARGO_TARGET_DIR` across the wave instead of copying `target/`.
- A heavy task that is already running means wait. Do not run "a quick one"
  beside it, kill the holder, or disable the queue.
- A new heavy entry point must source `tools/lib/cpu-guard.sh` (or re-exec the
  packaged wrapper itself), add a row here, and add a task flag upstream in the
  packaged wrapper. A one-off command can use `--cmd` without changing the
  wrapper.
