<!-- SPDX-License-Identifier: GPL-3.0-only -->
# CI workflow — one heavy task at a time, batched full gates

This repo is developed by several agents at once. Two rules keep that safe on a
single laptop and keep the expensive gates useful:

1. **Only one CPU-intensive task runs at a time.** `ci.sh`, `cargo build`,
   `cargo test`, asset packs, parity gates and headless Godot batches all go
   through `tools/run-cpu-intensive-task.sh`, which serializes them on one
   kernel lock. A second invocation waits its turn; it does not compete.
2. **The full gate is batched, not run after every edit.** Agents run targeted
   checks while they work and append an entry to [`CI_PENDING.json`](../CI_PENDING.json).
   The dedicated CI fixer runs `tools/ci.sh` over the batch and fixes what breaks.

The failure this prevents: two subagents each starting a `ci.sh` + `cargo build`
on the same checkout. rustc and the linker take every core and all available
memory, and the machine goes down. Serializing costs a wait; not serializing
costs the session.

## The queue

`tools/run-cpu-intensive-task.sh` is the only sanctioned way to start heavy
work. It takes an exclusive `flock`, records the holder, runs the task with the
lock held, and releases it on exit (including signals). Waiting tasks are
listed, and the start/finish events are appended to a history file.

State lives in the **git common directory**, so agents in separate
`git worktree`s queue against the main checkout instead of running in parallel:

```
<git-common-dir>/mindustry-cpu-queue/
  lock              # the flock target
  holder            # task/pid/started/cwd/host/command of the running task
  waiters/          # one file per queued pid
  history.jsonl     # start/finish events with durations and exit codes
```

`MIND_CPU_QUEUE_DIR` overrides the location (absolute, or relative to the repo
root); without git the fallback is `runs/cpu-queue/`. The CI run records are
separate, under `runs/ci/` (see below).

Worktrees share the lock and queue history, but `CI_PENDING.json` and `runs/ci/`
are ordinary files in each checkout. A batch written on an unmerged branch is
tested after that branch is checked out or merged; the fixer only sees the
checkout it runs in. Queue calls from any worktree still wait on the same lock,
which is the part that prevents the crash.

### Usage

```bash
# Full local gate (the CI fixer's job; queues behind any running build)
tools/run-cpu-intensive-task.sh --ci

# Targeted checks while developing (also queued)
tools/run-cpu-intensive-task.sh --cargo-build -p mind-gdext
tools/run-cpu-intensive-task.sh --cargo-test -p mind-core --lib
tools/run-cpu-intensive-task.sh --cargo-clippy -- -D warnings
tools/run-cpu-intensive-task.sh --cargo-fmt -- --check
tools/run-cpu-intensive-task.sh --godot --headless --path client --quit

# Arbitrary command under the lock, and queue inspection
tools/run-cpu-intensive-task.sh --cmd -- bash tools/pack.sh pack
tools/run-cpu-intensive-task.sh --status
tools/run-cpu-intensive-task.sh --history 20
```

| Task flag | Runs |
|---|---|
| `--ci` | `bash tools/ci.sh` |
| `--build` | `bash tools/build.sh` (GDExtension + headless oracle) |
| `--cargo-build` / `--cargo-test` / `--cargo-check` | `cargo …` against `client/rust/Cargo.toml` |
| `--cargo-fmt` | `cargo fmt --all` (in place; add `-- --check` to check) |
| `--cargo-clippy` | `cargo clippy --workspace --all-targets` |
| `--cargo-run` | `cargo run` (e.g. a `mind-headless` command) |
| `--godot` | `bash tools/godot.sh` (headless one-shots, **not** an editor session) |
| `--pack` | `bash tools/pack.sh` (defaults to `pack`) |
| `--parity` / `--pcam-parity` | `tools/parity.sh` / `tools/pcam_parity.sh` |
| `--export` | `tools/export.sh` |
| `--goldens` | `tools/regen_goldens.sh` |
| `--stdb` | `server/build.sh` |
| `--bench` | release `bench_baseline` (`mind-headless`) |
| `--cmd -- CMD…` | any command, still serialized |

Everything after the task flag is forwarded to the underlying command
(`--cargo-clippy -- -D warnings` keeps its `--`). Queue options come before the
task flag:

| Option | Effect |
|---|---|
| `--status` | holder, waiters, last history event, state path |
| `--list` | list task flags |
| `--history [N]` | last N queue events |
| `--no-wait` | exit `75` immediately if the lock is busy |
| `--timeout SECS` | stop waiting after SECS; exit `75` (`0` = forever, default) |
| `--quiet` | suppress the queue start/finish lines |
| `--dry-run` | print the resolved command without locking |

Exit status is the task's own, or `75` when busy/timeout (`EX_TEMPFAIL`), or `2`
for usage errors.

### Direct script calls self-queue

The heavy entry points source `tools/lib/cpu-guard.sh` and re-exec through the
wrapper when they are not already under the lock:

`tools/ci.sh`, `tools/build.sh`, `tools/pack.sh`, `tools/parity.sh`,
`tools/pcam_parity.sh`, `tools/export.sh`, `tools/regen_goldens.sh`,
`server/build.sh`, `everything_you_need.sh`.

So `bash tools/build.sh` is safe too — it waits like any other task. Nested
calls (`ci.sh` → `build.sh` → `pack.sh`) inherit `MIND_CPU_QUEUE_HELD` and do
not try to take the lock again, which would deadlock. `--help` bypasses the
queue.

`MIND_CPU_QUEUE_DISABLE=1` runs a task without the lock. It exists only to debug
the queue itself; it does not excuse parallel heavy work.

### What is deliberately outside the lock

- `tools/godot.sh` started for an interactive editor/game session. It would
  hold the lock for the entire session and block every build. Start it directly,
  not through `--godot`. Do not run `tools/mcp-smoke.sh` (it needs the editor)
  at the same time as a build.
- Cheap commands: `git`, reads, `python3 tools/ci_pending.py …`, a single
  `cargo fmt` on one file, log tails.

If a heavy task is not in the list above, add the guard and a wrapper task
rather than running it bare (see **Extending this**).

## Batch model

Full `ci.sh` performs a release build, a pinned benchmark, three spine goldens,
four in-engine Godot oracles and the STDB checks. It is minutes of CPU, not a
save-step check. Running it after every edit is what made redundant parallel
gates common in the first place.

While developing:

1. Make the change.
2. Run only the fast check that covers the area, through the queue
   (`--cargo-check -p mind-core`, `--cargo-test -p mind-gdext foo`,
   `--cargo-fmt -- --check`, a targeted `--godot` scene, …).
3. Append one entry to `CI_PENDING.json` with the helper.
4. Hand the batch to the CI fixer when it is worth a full gate: before a merge,
   after a multi-crate change, after touching the bench/parity/UI-import path,
   or when the user asks for green CI.

Do not add an entry for a pure read-only investigation or for a change that was
already covered by a full run. Do add one for every source/AI/schema/scenario
change that a later full gate must validate.

## `CI_PENDING.json`

The live file is a small JSON document with `schema`, `description`, `policy`,
`updated` and `entries`. It is tracked, and helpers do all writes:

```bash
python3 tools/ci_pending.py add "Add MindFoo dialog + widget" \
  --agent ui-port \
  --area client/rust/mind-gdext/src/ui \
  --files client/rust/mind-gdext/src/ui/foo.rs,client/rust/mind-gdext/src/ui/dialogs.rs \
  --gates fmt,clippy,cargo-check,godot-import,ui-widgets-check \
  --repro "cargo test -p mind-gdext foo" \
  --expected "dialog_sweep_check reports failed=0; MindFoo builds" \
  --risk "touches the UI manifest/row sizing; check MindTable min-size propagation"

python3 tools/ci_pending.py list
python3 tools/ci_pending.py show CP-20261105-120115
python3 tools/ci_pending.py check        # schema validation
python3 tools/ci_pending.py remove CP-…  # drop an entry that no longer applies
python3 tools/ci_pending.py clear        # reset the live file
```

Every mutation takes an exclusive `flock` on `CI_PENDING.json.lock` and replaces
the file atomically, so parallel agents never lose an entry. Always use the
helper — hand-editing while another agent appends will lose data. The helper
fills `id`, `created_at`, agent, branch/head/worktree and the dirty-file count
automatically.

### Entry fields the fixer needs

| Field | Meaning |
|---|---|
| `summary` | One line: what landed. Required. |
| `area` | Subsystem/path, e.g. `client/rust/mind-gdext/src/ui`, `server/spacetimedb`. |
| `files` | Changed files (comma-separated or repeat `--files`). |
| `gates` | Likely affected stages: `fmt`, `clippy`, `cargo-check`, `cargo-test`, `parity`, `goldens`, `spine`, `bench`, `godot-build`, `godot-import`, `shaders-check`, `ui-layout-check`, `ui-widgets-check`, `dialog-sweep`, `boundary`, `stdb-check`, `stdb-bindings`. |
| `repro` | Command that exercises the change locally. |
| `expected` | What a passing result looks like. |
| `risk` | What could break, or the first thing to inspect on failure. |
| `notes` | Anything else (known flake, upstream quirk, dependency bump). |
| `status` | `pending` (default), `blocked`, `resolved`. |

A useful entry answers: *what changed, where, how do I reproduce the local
check, what does success look like, and what is most likely to fail in the full
gate?* If you cannot fill `files`/`gates`, say what you do know in `risk` — the
fixer can still start from the log.

### Lifecycle

```
agent: ci_pending.py add ──► CI_PENDING.json ──► ci fixer: run-cpu-intensive-task.sh --ci
                                                        │
                                     ci.sh begin-run ───┤ archive to runs/ci/<run-id>/pending.json
                                                        │ reset CI_PENDING.json
                                                        ▼
                                                run all gate stages ──► finish-run writes run.json
                                                        │
                            pass: archive is the record │ fail: archive + stage + log are the fixer's input
```

- `tools/ci.sh` archives and resets **at the start** of every run, so the
  snapshot always matches the tree the gate is testing.
- Entries added while a run is in progress are written after the reset and
  belong to the next batch. That is intentional; they were never tested by the
  current run.
- A failed run does not restore the live file; the batch is in the archive and
  `runs/ci/last-run.json`.

### Run records

`tools/ci.sh` writes one directory per run under `runs/ci/` (gitignored):

```
runs/ci/
  <run-id>/
    run.json        # status, stage, exit_code, pending_count, paths, branch/head, pid/host
    pending.json    # the CI_PENDING.json snapshot under test
    ci.log          # full gate output
  last-run.json     # copy of the most recent run.json
  history.jsonl     # one line per finished run
```

`run.json` on failure also carries `failure.log_tail`, but `ci.log` is the
authoritative log (tee can land the last few lines after the shell exits).
`run-id` is UTC `<YYYYMMDD-HHMMSS>-<pid>`; `MIND_CI_RUN_ID` overrides it for a
deliberate rerun.

## The CI fixer agent

The fixer is the only role that runs the full gate. It is defined as the
opencode agent `.opencode/agents/ci-fixer.md`, and the harness-agnostic brief is
[`CI-FIXER-PROMPT.md`](CI-FIXER-PROMPT.md) (paste it into a subagent on any
harness). One fixer at a time.

Its loop:

1. Read `AGENTS.md`, `tools/AGENTS.md`, this document, `CI_PENDING.json`, and
   `runs/ci/last-run.json` if the live file is empty.
2. Check `tools/run-cpu-intensive-task.sh --status`; the wrapper queues anyway,
   but the status explains a long wait.
3. Run `tools/run-cpu-intensive-task.sh --ci`.
4. **Pass** → report. `CI_PENDING.json` is already reset; the archive is the
   record of what was tested.
5. **Fail** → open `runs/ci/last-run.json`, read `stage`, `exit_code`, `log` and
   the `pending.json` snapshot, then fix the root cause. Use targeted queued
   checks while iterating. Record each fix batch with
   `ci_pending.py add` (the next run archives it), and rerun the full gate.
6. Repeat until green or genuinely blocked (max ~3–5 full runs; report the
   failing stage and evidence rather than grinding).

### Triage map

| Failed stage | First move |
|---|---|
| `cargo fmt` | `--cargo-fmt`, then re-check the touched files. |
| `cargo clippy` | Fix the lint; if an allow is genuinely required, justify it in the pending entry. |
| `cargo check` / `cargo test …` | Targeted `--cargo-test -p <crate> <filter>`; fix, never delete/skip a failing test. |
| `parity check` / `parity goldens` | Inspect the registry/golden mismatch; regenerate only when the change legitimises it, and say so. |
| `spine_*` goldens | Determinism regression — fix the sim; do not bless the new output. |
| scenario mirror | `bash tools/sync_scenarios.sh` then `diff -r scenarios client/scenarios`. |
| bench budget | Treat as a regression; profile first. A budget change needs evidence, not a convenience bump. |
| `godot: build …` | Compile error in `mind-gdext`/`mind-headless`; fix and rebuild. |
| headless import/parse | Fix the `.rs`/`.tscn`; `client/.godot/` and `client/bin/` are regenerated, never hand-edited. |
| `shaders_check` / `ui_layout_check` / `ui_widgets_check` / `dialog_sweep_check` | Native-UI/manifest bug; check the upstream Mindustry behaviour cited by the port. |
| `mind-core` boundary | Remove the `godot`/`tokio` dependency or import from `mind-core`. |
| `mind-stdb` / STDB typecheck / bindings drift | Use the official `server/build.sh` generation path; never hand-edit `module_bindings/`. |

### Fixer guardrails

- One gate at a time; the queue enforces it, and `MIND_CPU_QUEUE_DISABLE` is not
  for production work.
- Do not weaken or delete tests, lints, budgets or gates to get green.
- Never hand-edit generated outputs (`client/bin/`, `client/.godot/`,
  `client/rust/mind-stdb/src/module_bindings/`, atlas/index outputs) or bless
  goldens/bench baselines without a stated reason.
- Do not touch `TWIN-VERIFICATION.json` or other agents' uncommitted work.
- Do not commit unless the user asks. Do not run the interactive MCP smoke.
- Do not spawn parallel subagents that build; the fixer is sequential.

## Extending this

- **New heavy script:** source `tools/lib/cpu-guard.sh` after `REPO_ROOT`, call
  `mind_cpu_guard <name> "$REPO_ROOT/<script>" "$@"`, add a task flag in
  `tools/run-cpu-intensive-task.sh`, and add a row to `tools/AGENTS.md`.
- **New gate:** add a `say`/command stage to `tools/ci.sh` (the stage label is
  what the fixer sees in `run.json`), add the gate name to the `--gates`
  vocabulary above, and update `tools/AGENTS.md`.
- **New run-record consumer:** read `runs/ci/last-run.json`; the path is stable
  and the file is replaced atomically by `ci_pending.py finish-run`.

## Environment

| Variable | Effect |
|---|---|
| `MIND_CPU_QUEUE_DIR` | Queue state directory (default: `<git-common-dir>/mindustry-cpu-queue`). |
| `MIND_CPU_QUEUE_TIMEOUT` | Default `--timeout` seconds. |
| `MIND_CPU_QUEUE_QUIET` | Default `--quiet` (`1`). |
| `MIND_CPU_QUEUE_DISABLE` | `1` = run without the lock (debugging only). |
| `MIND_CPU_QUEUE_HELD` | Internal re-entrancy marker; never set it by hand. |
| `MIND_CI_PENDING_FILE` | Override the live pending file path. |
| `MIND_CI_RUNS_DIR` | Override `runs/ci` for run records. |
| `MIND_CI_RUN_ID` | Override the run id (UTC timestamp-pid by default). |
| `CI_PENDING_AGENT` | Default `--agent` value for entries. |
