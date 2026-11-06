# AGENTS.md — Mindustry-Godot

Godot 4.7 client over the Rust simulation in `client/rust/`, exported as the `mind-gdext` GDExtension: a 1:1 parity port of [Mindustry](https://github.com/Anuken/Mindustry). Rust owns behavior and the native UI; `.tscn` scenes name the classes.

## Top-level map

| Path | Responsibility |
|---|---|
| `client/` | Godot project: `project.godot`, scenes (rooted on the native UI classes), shaders, vendored addons, the UI manifests. |
| `client/rust/` | Cargo workspace: `mind-core`, `mind-gdext`, `mind-headless`, `mind-stdb`, `mind-tools`, `mind-atlas` and the proc-macro crates. |
| `assets/`, `assets-raw/` | Packed runtime assets vs authored art (fonts, icons, source sprites). |
| `scenarios/` | Canonical scenario JSON; mirrored to `client/scenarios/`. |
| `parity/` | Oracle registries, sha256 goldens, screenshots, audit ledgers and gate reports. |
| `server/` | SpacetimeDB `mindustry_godot` module and publish/bindings tooling. |
| `tools/` | Build, pack, verify, launch and parity scripts. |
| `docs/` | Implementation plans, round summaries and platform handoffs. |
| `bench/` | Recorded performance baselines behind `parity/bench_budgets.json`. |
| `.github/` | CI workflows: `ci.yml`, `parity-nightly.yml`. |
| `CI_PENDING.json` | Batch of landed changes for the next full CI run; write it with `tools/ci_pending.py` ([`docs/CI-WORKFLOW.md`](docs/CI-WORKFLOW.md)). |

## Generated / transient — do not hand-edit

- `client/bin/`, `client/.godot/`, `client/rust/target/`, `server/spacetimedb/target/`, `build/` — build and import caches (`tools/build.sh`, `bash tools/pack.sh`); `client/scenarios/` — mirror of `scenarios/` (`bash tools/sync_scenarios.sh`).
- `assets/sprites/sprites*.png`, `sprites.atlas.json`, `fallback/`, `block_colors.png`, `assets/icons/icon_codes.json`, `assets/icons/icons.properties`, `assets/sounds.index.json`, `assets/locales/`, `assets/shaders/godot/`, `assets/shaders/shader.index.json`, `assets/parity/region_names.txt` — atlas/index outputs (`bash tools/pack.sh pack`); `assets/ASSET_PROVENANCE.md` — `mind-tools migrate` output; `client/rust/mind-stdb/src/module_bindings/` — `spacetime generate` output (`bash server/build.sh`).
- `runs/`, `parity-java-home/`, `screenshots_for_ai/`, `client/assets/version.properties`, `.opencode/` session state (the tracked `.opencode/agents/` definitions are source), `.claude/worktrees/` — transient outputs and session state; the worktrees are stale duplicates, ignore them. `runs/ci/` holds per-run gate records (see below); the CPU queue state lives in `<git-common-dir>/mindustry-cpu-queue/`.

## Global invariants

- Every source file carries `SPDX-License-Identifier: GPL-3.0-only` and cites the upstream Mindustry file it ports; UTF-8/LF only.
- Toolchain: Rust `1.98.1` (`rust-toolchain.toml`), Godot `4.7.x` (`4.7.2` recommended).
- Rust owns behavior and the native UI (`client/rust/mind-gdext/src/ui/`); scenes only name the classes and never mutate simulation state. **All non-addon code is `.rs` or `.tscn`**: there is no hand-written GDScript outside vendored addons, no `.gd`/`.gd.uid` file, and no `script = ExtResource(...)` on a non-addon scene. `mind-core` stays Godot-free and tokio-free; hosts call in, never the reverse.
- ABIs are append-only: content IDs, entity field order, UI manifests, STDB schema and bindings. Never hand-edit generated outputs.
- Only one CPU-intensive task at a time: route builds, tests and gates through `tools/run-cpu-intensive-task.sh` (see **CPU-intensive work** below).

## CPU-intensive work: one at a time

Never run two heavy jobs concurrently — a full `ci.sh` racing a `cargo build` has
crashed the dev laptop. Every build, test, gate, asset pack, export and headless
Godot batch is serialized by `tools/run-cpu-intensive-task.sh`; a second call
waits its turn, it does not compete.

```bash
tools/run-cpu-intensive-task.sh --ci                  # full gate (the CI fixer runs this)
tools/run-cpu-intensive-task.sh --cargo-build -p mind-gdext
tools/run-cpu-intensive-task.sh --cargo-test -p mind-core --lib
tools/run-cpu-intensive-task.sh --cargo-clippy -- -D warnings
tools/run-cpu-intensive-task.sh --status              # holder, waiters, recent history
```

- The lock is one `flock` under `<git-common-dir>/mindustry-cpu-queue/`, shared by
  every worktree of the repo. `--no-wait` exits `75`; `--timeout SECS` bounds the
  wait; `--history` shows recent runs. `MIND_CPU_QUEUE_DIR` overrides the state dir.
- `tools/ci.sh`, `build.sh`, `pack.sh`, `parity.sh`, `pcam_parity.sh`, `export.sh`,
  `regen_goldens.sh`, `server/build.sh` and `everything_you_need.sh` re-exec through
  the wrapper, so a direct `bash tools/build.sh` also queues. Nested calls detect
  `MIND_CPU_QUEUE_HELD` and never take the lock twice.
- Not queued: an interactive `tools/godot.sh` editor/game session (it would hold the
  lock for hours) and `tools/mcp-smoke.sh` (needs the editor). Still do not run
  headless Godot checks or MCP batches alongside a build; stop the editor before a
  release build on Windows.
- **Do not run the full gate after every edit.** Run the targeted check for the
  touched area, record the change in `CI_PENDING.json`, and leave the full gate to
  the CI fixer. Details, schema and the fixer contract:
  [`docs/CI-WORKFLOW.md`](docs/CI-WORKFLOW.md) and
  [`docs/CI-FIXER-PROMPT.md`](docs/CI-FIXER-PROMPT.md). The opencode agent is
  [`.opencode/agents/ci-fixer.md`](.opencode/agents/ci-fixer.md).

## Close what you open

Every agent **must close whatever it opened before it reports done** — the dev
laptop is shared by the user, the main agent and every subagent. A Godot editor
or game instance, a dev server, an SSH tunnel, a background job, a subagent or a
detached shell left running keeps burning CPU/RAM and starves later work.
Cleanup is part of the task: a turn is **incomplete** while a process it started
is still running, even if the edit landed. This applies to *anything* an agent
opens, not just the examples below.

- **Godot editors and games**: stop the game you launched (`godot_game stop`, or
  the equivalent stop action in the MCP client you use), close the editor
  (`godot_instance terminate`, the `tools/godot.sh -e` window, or
  `tools/parity-session.sh --stop` for a parity-session editor), then sweep
  orphans as the addon guide describes: `pgrep -f "godot --path"` and kill any
  process whose `--editor-pid` is gone.
- **Servers and sessions**: `tools/mp-local.sh down` for the local SpacetimeDB
  harness, `tools/parity-session.sh --stop` for the Xvfb + editor bridge, and the
  matching shutdown for any other server, watcher, tunnel or display your session
  opened.
- **Background jobs and subagents**: wait for or cancel every job and subagent
  you started before returning; never leave one running "for later", and never
  leave a `&`/`nohup` process behind.
- **Failures and interruptions count**: a failed, timed-out or interrupted
  command does not transfer cleanup to the user or the next agent. Stop what you
  started, then report.
- **Only what you started**: never kill the user's editor/session, another
  agent's processes, or the shared `dsh web`/MCP host. Verify with the tool's
  status call (`godot_game status`, `godot_instance list`) plus `pgrep -af <name>`.

"The next agent can close it" is not an excuse — the next agent should start it
fresh.

## CI handoff: `CI_PENDING.json`

Every landed change that a later full gate must validate gets one entry, written
with the lock-protected helper (parallel agents can append concurrently):

```bash
python3 tools/ci_pending.py add "Add MindFoo dialog + widget" \
  --agent ui-port \
  --area client/rust/mind-gdext/src/ui \
  --files path/foo.rs,path/dialogs.rs \
  --gates fmt,clippy,cargo-check,godot-import,ui-widgets-check \
  --repro "cargo test -p mind-gdext foo" \
  --expected "dialog_sweep_check reports failed=0" \
  --risk "UI manifest/row sizing; check MindTable min-size propagation"
python3 tools/ci_pending.py list
```

- Always use `tools/ci_pending.py` (`add`/`list`/`show`/`check`/`remove`/`clear`):
  it takes `CI_PENDING.json.lock` and replaces the file atomically. Hand-editing
  while another agent appends loses data.
- `tools/ci.sh` archives the file to `runs/ci/<run-id>/pending.json`, resets
  `CI_PENDING.json`, and writes `runs/ci/<run-id>/{run.json,ci.log}` plus
  `runs/ci/last-run.json`. On failure `run.json` names the failed stage; that
  archive is the CI fixer's input. Entries added while a run is in progress belong
  to the next batch.
- An entry should answer: what changed, where, how to reproduce the local check,
  what success looks like, and what is most likely to fail. Fields: `summary`,
  `area`, `files`, `gates`, `repro`, `expected`, `risk`, `notes`; the gate
  vocabulary and stage triage map are in
  [`docs/CI-WORKFLOW.md`](docs/CI-WORKFLOW.md).

## Quickstart (`tools/AGENTS.md` covers every script and gate)

```bash
./everything_you_need.sh    # first checkout: Rust build + asset pack + scenario sync
tools/build.sh              # rebuild mind-gdext + mind-headless, sync scenarios
bash tools/pack.sh pack     # regenerate the runtime asset tree under assets/
tools/run-cpu-intensive-task.sh --ci   # full local gate (queued; the CI fixer runs this)
bash tools/godot.sh         # launch the client (-e opens the editor)
```

Direct calls to `tools/ci.sh`, `tools/build.sh` and the other heavy scripts
self-queue through the wrapper; the wrapper form is canonical because it makes the
serialization explicit.

## Guide index

| Guide | Scope |
|---|---|
| [`client/AGENTS.md`](client/AGENTS.md) | Scene graph, native UI classes, dialog/widget conventions, manifest ABI. |
| [`client/rust/AGENTS.md`](client/rust/AGENTS.md) | Workspace layout, crate dependency direction, lints, toolchain. |
| [`client/rust/mind-core/AGENTS.md`](client/rust/mind-core/AGENTS.md) | Simulation core: modules, ECS/schedule, determinism, content IDs. |
| `client/rust/mind-core/src/` — simulation/data | [`sim`](client/rust/mind-core/src/sim/AGENTS.md), [`ai`](client/rust/mind-core/src/ai/AGENTS.md), [`combat`](client/rust/mind-core/src/combat/AGENTS.md), [`content`](client/rust/mind-core/src/content/AGENTS.md), [`maps`](client/rust/mind-core/src/maps/AGENTS.md), [`mods`](client/rust/mind-core/src/mods/AGENTS.md). |
| `client/rust/mind-core/src/` — world/game/io | [`world`](client/rust/mind-core/src/world/AGENTS.md), [`world/blocks`](client/rust/mind-core/src/world/blocks/AGENTS.md), [`game`](client/rust/mind-core/src/game/AGENTS.md), [`io`](client/rust/mind-core/src/io/AGENTS.md). |
| `client/rust/mind-core/src/` — view/control | [`fx`](client/rust/mind-core/src/fx/AGENTS.md), [`render`](client/rust/mind-core/src/render/AGENTS.md), [`ui`](client/rust/mind-core/src/ui/AGENTS.md), [`input`](client/rust/mind-core/src/input/AGENTS.md), [`logic`](client/rust/mind-core/src/logic/AGENTS.md). |
| [`client/rust/mind-gdext/AGENTS.md`](client/rust/mind-gdext/AGENTS.md) | `Mind*` classes, frame pump, view/input/audio/net/platform glue. |
| [`client/rust/mind-headless/AGENTS.md`](client/rust/mind-headless/AGENTS.md) | Scenarios, goldens, benches, parity harness, dedicated server modes. |
| [`client/rust/mind-stdb/AGENTS.md`](client/rust/mind-stdb/AGENTS.md) | Connector lifecycle, subscription waves, table binders, command relay. |
| [`client/rust/mind-tools/AGENTS.md`](client/rust/mind-tools/AGENTS.md) | Offline asset pipeline: migrate, pack, icons, sounds, shaders, mods classmap. |
| [`client/addons/open_godot_mcp/AGENTS.md`](client/addons/open_godot_mcp/AGENTS.md) | Editor plugin and runtime autoload for MCP inspection and playtesting. |
| [`parity/AGENTS.md`](parity/AGENTS.md) | Registries, goldens, screenshots, ledgers and `parity` harness rules. |
| [`server/AGENTS.md`](server/AGENTS.md) | Module map, publish/bindings workflow, protocol sync. |
| [`server/spacetimedb/AGENTS.md`](server/spacetimedb/AGENTS.md) | Tables, reducers, views, relay/chat/admin/campaign schema ABI. |
| [`tools/AGENTS.md`](tools/AGENTS.md) | Script reference, environment overrides, CI gate contents, export layout. |

## Routing hints

- Gameplay rule, sim state or determinism: `client/rust/mind-core/AGENTS.md` and the module guide under `src/`.
- Godot scene, dialog, widget or theme: `client/AGENTS.md`.
- Networking: `server/spacetimedb/AGENTS.md` for schema/reducers, `client/rust/mind-stdb/AGENTS.md` for the client.
- Asset pipeline, atlas or icons: `client/rust/mind-tools/AGENTS.md`.
- Parity evidence: `parity/AGENTS.md` and `TWIN-VERIFICATION.json` (minimal items: `title`/`status`/`similarity`/`fork`/`action`/`images`, plus paired recipes and gaps); in-engine checks use the `playtest` skill, Java↔Godot walks use `parity-eval`.
- Full local CI, a red gate, or handing a pending batch to the CI fixer: [`docs/CI-WORKFLOW.md`](docs/CI-WORKFLOW.md) and [`docs/CI-FIXER-PROMPT.md`](docs/CI-FIXER-PROMPT.md).
