# AGENTS.md — mind-headless (headless test-rig / oracle)

`mind-headless` is the Godot-free test-rig and oracle for the `mind-core`
simulation. It links `mind-core` (with `mind-atlas` and `mind-stdb`) and never
links Godot: it runs file-backed scenarios, state dumps, benchmarks, the
cross-cutting `parity` verification harness, and the dedicated server shape that
`mind-gdext`'s `MindNet` drives. Read the root [`AGENTS.md`](../../../AGENTS.md)
first; the workspace map is [`client/rust/AGENTS.md`](../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Crate root; `run_from_args` parses via `Cli` and dispatches to `exec::run`. |
| `src/main.rs` | Thin binary over `mind_headless::run_from_args`. |
| `src/cli.rs` | clap `Cli`/`Command` tree: every subcommand and flag. |
| `src/args.rs` | Re-exports `mind_core::platform::args` (`LaunchArgs`, `parse`, `data_root`) plus launch-arg tests. |
| `src/exec.rs` | Dispatch table and the scenario/bench/dump/replay/suite handlers. |
| `src/registry.rs` | File-backed `ScenarioFixture` registry (`SCENARIOS`, `find`, `names`, `bench_alias`); name = `{system}_{case}`. |
| `src/report.rs` | serde report structs (`RunReport`, `TileCheck`, `BenchReport`, `SimReport`, `StdbReport`, IO/content reports). |
| `src/paths.rs` | Marker-based discovery of `scenarios/`, the repo root and `mind-core` (`find_scenarios_dir`, `find_repo_root`, `find_mind_core_dir`). |
| `src/<system>_scenarios.rs` | Per-system suites: `blocks`, `campaign`, `combat`, `fx`, `input`, `logic`, `mp`, `network`, `render`, `stdb`, `ui`, `units`, `audio`. |
| `src/parity/` | Verification harness and the `parity/` registries. |
| `src/server/` | Dedicated server implementation. |
| `tests/` | Integration golden tests, fixtures and committed scenario goldens. |
| `bench/` | Committed network baselines (`bench_liquid.json`, `bench_heat.json`, `bench_power.json`). |

## Responsibilities

- Execute repo-root `scenarios/*.json` through `mind-core` and assert expected checksums, per-tile blocks and command logs (`run`, `run-all`).
- Replay `.jsonl`/`.simlog` command streams deterministically and sample checksums (`replay`).
- Time `Sim::tick` and subsystem loops (`bench`, `power|liquid|heat bench`, `render bench`, `io bench-save`).
- Validate the machine-readable registries under `parity/` and roll them up.
- Serve as the dedicated server: `server` (console/config/commands/socket) and `serve` (service-identity host shape), both Godot-free.

## Command surface

- Core: `list`, `run <scenario>`, `run-all`, `sim`, `replay`, `bench`, `dump`, `version`.
- Per-system families (with `list`/`scenario`/`bench` where applicable): `content`, `assets`, `io`, `meta`, `trace`, `world`, `maps`, `editor`, `blocks`, `combat`, `units`, `logic`, `audio`, `mods`, `render`, `campaign`, `fx`, `power`, `liquid`, `heat`, `ui`, `input`.
- `parity`: `check`, `run`, `matrix`, `registry`, `goldens`, `budgets`, `mcp`, `scenarios`, `soak`, `desync-inject`, `mcp-parity`, `screenshots`, `report`, `gate`, `bench-gate`, `checksums`, `replay-fuzz`, `mirror`.
- `render list` covers the `render_*` scenarios (`render_flat_floor`, `render_block_change`, `render_layer_order`, `render_layer_order_nosort`, `render_darkness_radius`, `render_menu_world`, `render_layers_full`); `parity screenshots` validates the fixed-pose baseline manifest and runs headless PNG tolerance diffs (`--diff-a`/`--diff-b`).

## Key types

- `cli::{Cli, Command, ParityCommand, RenderCommand, InputCommand}`.
- `exec::{run, run_scenario, run_suite, run_catalog_entries, sample_checksums}`.
- `registry::{ScenarioFixture, SCENARIOS, LIVE_CONTENT_TYPES, find, names, bench_alias}`.
- `report::{RunReport, SimCoreReplayReport, SimCoreProfileReport, BenchReport, StdbReport}`.
- `parity::{run, find_repo}` plus `parity::golden::GoldenManifest`, `parity::scenario::ScenarioCatalog`, `parity::registry::ChecksumRegistry`, `parity::budgets::Budgets`, `parity::screenshot::ScreenshotManifest`, `parity::soak::Soak`.
- `server::{ServerOptions, ServerState, run}` and `server::dedicated::{ServeOptions, run}`.

## Server mode

`server` boots `ServerState` from `ServerOptions` and runs console commands; `serve` boots `dedicated::ServeOptions`. Submodules: `config.rs`, `console.rs`, `commands.rs`, `host.rs`, `net_host.rs`, `admin.rs`, `rules_file.rs`, `autosave.rs`, `logs.rs`, `socket.rs`, `dedicated.rs`. `net_host::NetHost` boots offline and delegates to the `mind-stdb` connector on demand.

## Invariants

- The crate never links Godot; all game rules and simulation live in `mind-core`.
- `run_from_args` returns the process code: `0` pass, `1` assertion/golden mismatch, `2` usage/IO error (`exec::{EXIT_PASS, EXIT_FAIL, EXIT_USAGE}`).
- `stdb_*` scenarios (`stdb_offline_boot`, `stdb_binder_replay`, `stdb_command_order`) are offline proofs; STDB sockets open only on env-gated (`MIND_STDB_IT=1`) integration paths.
- Per-system scenario names are `{system}_{case}` and append-only.
- `paths::find_scenarios_dir` locates `scenarios/` by the `spine_place_break.json` marker; `--scenarios-dir`/`MIND_SCENARIOS_DIR` and `--repo`/`MIND_REPO` take precedence.

## Golden tests

- Integration tests under `tests/`: `input_golden.rs`, `render_golden.rs`, `fx_golden.rs`, `audio_golden.rs`, `logistics_golden.rs`, `maps_editor_golden.rs`.
- Committed scenario goldens under `tests/golden/<system>/` (`input/`, `render/`, `units/`, `campaign/`, `network/`, `audio/`, `fx/`); harness goldens under `tests/golden(s)/**` are pinned by sha256 in `parity/golden_manifest.json`.
- Replay fixtures under `tests/fixtures/` (e.g. `tests/fixtures/input/focus_guards.events.jsonl`).
- Committed baselines: `parity/bench_budgets.json`, root `bench/baselines.json`, `client/rust/mind-core/bench/baselines.json`, and `client/rust/mind-headless/bench/bench_{power,liquid,heat}.json`.

## Rules

- Add a scenario as a committed `scenarios/<name>.json` plus a `registry.rs` fixture (or a per-system suite) and a `parity/scenario_catalog.json` entry.
- New external crates go through `[workspace.dependencies]`; the crate opts into workspace lints with `[lints] workspace = true`.
- Port Mindustry sources and cite the upstream file in the header; new files are LF/UTF-8 GPL-3.0-only.
- Golden edits must update the matching `parity/golden_manifest.json` sha256 in the same change.
- Keep `unwrap`/`expect` out of non-test code; unit tests opt out with `#![cfg_attr(test, allow(...))]`.

## Verification

From the repo root (WSL2 Ubuntu):

```bash
cargo run -p mind-headless -- run spine_place_break --json
cargo run -p mind-headless -- bench --ticks 100000
cargo run -p mind-headless -- parity check
cargo test -p mind-headless
```

Add `--manifest-path client/rust/Cargo.toml` when invoking from outside the workspace.
