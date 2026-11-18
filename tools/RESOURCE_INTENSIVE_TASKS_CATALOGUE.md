<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Resource-intensive tasks catalogue

Every resource-intensive task in this repository runs one at a time through
[`tools/ai-tools/resource-gate/run-resource-intensive-task.sh`](ai-tools/resource-gate/run-resource-intensive-task.sh).
The existing `tools/run-cpu-intensive-task.sh` remains supported; when it is
present both wrappers use the same lock state under
`<git-common-dir>/mindustry-cpu-queue/`, so a build started through one blocks
the other.

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

State defaults to `<git-common-dir>/mindustry-cpu-queue/` (the same directory as
`tools/run-cpu-intensive-task.sh`). Override it with `RESOURCE_QUEUE_DIR` or the
legacy `MIND_CPU_QUEUE_DIR`.

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
- Adding a new heavy entry point means adding a row here and a task flag in the
  wrapper; the task must re-exec through the wrapper (or source the repo's
  existing guard) rather than run directly.
