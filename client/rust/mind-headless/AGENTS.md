# AGENTS.md — mind-headless (headless test-rig / oracle)

`mind-headless` is the Godot-free test-rig and oracle for the `mind-core` simulation. It links
`mind-core`, `mind-atlas` and `mind-stdb` and never links Godot: it runs file-backed scenarios,
state dumps, benchmarks, and the standalone dedicated server shape (`server`/`serve`). The parity
harness itself is cross-cutting and owned by [`parity/AGENTS.md`](../../../parity/AGENTS.md); this
crate implements it under `src/parity/`. Read the root [`AGENTS.md`](../../../AGENTS.md) first; the
workspace map is [`client/rust/AGENTS.md`](../AGENTS.md).

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
| `src/parity/` | Harness implementation (`matrix`, `registry`, `golden`, `scenario`, `budgets`, `mcp`, `soak`, `checksum`, `determinism`, `screenshot`, `report`, `desync`, `minitoml`). |
| `src/server/` | Dedicated server implementation (`server`/`serve`, console, host, socket, admin, autosave, logs). |
| `tests/` | Integration golden tests, fixtures and committed scenario goldens. |
| `bench/` | Committed network baselines (`bench_liquid.json`, `bench_heat.json`, `bench_power.json`). |

## Command surface

- Core: `list`, `run <scenario>`, `run-all`, `sim`, `replay`, `bench`, `dump`, `version`, `server`,
  `serve`.
- Per-system families: `content`, `assets`, `io`, `meta`, `trace`, `world`, `maps`, `editor`,
  `blocks`, `combat`, `units`, `logic`, `audio`, `mods`, `render`, `campaign`, `fx`, `power`,
  `liquid`, `heat`, `ui`, `input`. Each family opens its own subcommands (`list`/`scenario`/`bench`
  where applicable; `--help` is authoritative).
- `render list` covers the embedded `render_*` scenarios (`render_flat_floor`,
  `render_block_change`, `render_layer_order`, `render_layer_order_nosort`,
  `render_darkness_radius`, `render_menu_world`, `render_layers_full`).
- `parity` runs the cross-cutting harness. Its registry and evidence semantics are owned by
  [`parity/AGENTS.md`](../../../parity/AGENTS.md); the subcommand inventory is `src/cli.rs`
  (`--help` is authoritative). Do not restate the semantics here.

## Scenarios and goldens

- `scenarios/*.json` at the repo root are canonical; `client/scenarios/` is the generated mirror
  (`bash tools/sync_scenarios.sh`). Scenario names are `{system}_{case}` and append-only.
- A fixture is a `ScenarioFixture` entry in `registry.rs` plus either a committed
  `scenarios/<name>.json` or an embedded per-system fixture. Add the `parity/scenario_catalog.json`
  entry per `parity/AGENTS.md`.
- Execute with `run`/`run-all` and assert expected checksums, per-tile blocks and command logs.
  `replay` re-runs `.jsonl`/`.simlog` command streams; `sim`/`dump` produce fresh-world state.
- Integration goldens live in `tests/` (`input_golden.rs`, `render_golden.rs`, `fx_golden.rs`,
  `audio_golden.rs`, `logistics_golden.rs`, `maps_editor_golden.rs`); committed scenario goldens
  under `tests/golden/<system>/` (`input/`, `render/`, `units/`, `campaign/`, `network/`, `audio/`,
  `fx/`) plus `logistics.json` and `maps_editor.json`. Replay fixtures live under `tests/fixtures/`.
- Harness goldens under `client/rust/*/tests/golden(s)/**` are pinned by sha256 in
  `parity/golden_manifest.json`; the pin/update rule is owned by
  [`parity/AGENTS.md`](../../../parity/AGENTS.md).
- Bench baselines: `parity/bench_budgets.json` (budget registry), root `bench/baselines.json`,
  `client/rust/mind-core/bench/baselines.json`, and this crate's `bench/bench_{power,liquid,heat}.json`.

## Dedicated server modes

`server` boots `ServerState` from `ServerOptions` and runs the console (config, commands, rules,
autosave, logs, socket). `serve` boots `dedicated::ServeOptions`: a service identity from a token
file (never a player token), a parsed match config, auto-pause and map rotation. Both are Godot-free.
Submodules: `config.rs`, `console.rs`, `commands.rs`, `host.rs`, `net_host.rs`, `admin.rs`,
`rules_file.rs`, `autosave.rs`, `logs.rs`, `socket.rs`, `dedicated.rs`. `net_host::NetHost` boots
offline and delegates to the `mind-stdb` connector (reducers plus `MatchSession`) on demand.

## Key types

- `cli::{Cli, Command, ParityCommand, RenderCommand, InputCommand}`.
- `exec::run`, `report::{RunReport, SimCoreReplayReport, SimCoreProfileReport, BenchReport, StdbReport}`.
- `registry::{ScenarioFixture, SCENARIOS, LIVE_CONTENT_TYPES, find, names, bench_alias}`.
- `server::{ServerOptions, ServerState, run}` and `server::dedicated::{ServeOptions, run}`.

## Invariants

- The crate never links Godot; all game rules and simulation live in `mind-core`.
- `run_from_args` returns the process code: `0` pass, `1` assertion/golden mismatch, `2` usage/IO
  error (`exec::{EXIT_PASS, EXIT_FAIL, EXIT_USAGE}`).
- `stdb_*` scenarios (`stdb_offline_boot`, `stdb_binder_replay`, `stdb_command_order`) are offline
  proofs; STDB sockets open only on env-gated (`MIND_STDB_IT=1`) integration paths.
- Per-system scenario names are `{system}_{case}` and append-only.
- `paths::find_scenarios_dir` locates `scenarios/` by the `spine_place_break.json` marker;
  `find_repo_root` uses `assets-raw/sprites/pack.json`. `--scenarios-dir`/`MIND_SCENARIOS_DIR` and
  `--repo`/`MIND_REPO` take precedence where a command exposes them.

## Rules

- Add a scenario as a committed `scenarios/<name>.json` plus a `registry.rs` fixture (or a per-system
  suite), then catalogue it per `parity/AGENTS.md`.
- Workspace dependency, lint and test opt-out policy: [`client/rust/AGENTS.md`](../AGENTS.md).
- Do not duplicate the parity harness contract here; edit the owning registry/guide instead.

## Verification

Run from the repo root (Linux):

```bash
# spine_place_break golden checksum: 3e48c583a94fbea4
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- run spine_place_break --json
cargo run --manifest-path client/rust/Cargo.toml -p mind-headless -- bench --ticks 100000
cargo test --manifest-path client/rust/Cargo.toml -p mind-headless
```
