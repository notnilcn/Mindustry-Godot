# AGENTS.md — sim/ (fixed-step simulation spine)

`sim` is the spine every game mutation crosses: a `Sim` owns the ECS world, grid, deterministic
clock/RNG, IO queue and the opt-in live runtimes; commands enter through `Sim::apply`/`Sim::command`
and time advances only through `Sim::tick`. Parent boundary and crate rules (no `godot`, no `tokio`,
no wall clock, license/lints) live in [`mind-core/AGENTS.md`](../../AGENTS.md).

## Layout

| File / dir | Responsibility |
|---|---|
| `mod.rs` | Module root: module declarations, the `pub use` surface (`SimBuilder`, `SimConfig`, `SimClock`, `FixedStepRunner`, `IoSet`, `SimRuntime`, `UnitCommandRuntime`, `WorldApplyRuntime`, …), `SimError`, `SimSnapshot`, `HintEvents`, `ChecksumScope`, `ConfigTapKind`. |
| `boot.rs` | `SimBuilder`, `BootError`, `TickReport` and `Sim::tick_report`. |
| `spine/` | The `impl Sim` bodies, one topic per file: `apply`, `command_dispatch`, `tick`, `checksum`, `accessors`, `event_flush`, `placement`, `construct`, `rules_world`, `world_config`, `logic_bridge`, `live_unit_cmd`, `abilities` (`drain_ability_queues_system`, `KIND_BUILDING`). |
| `query.rs` | Shared `pub(crate)` read helpers: `sorted_by_seq`, `team_of`, `unit_def`, `tile_world`, `dist2` (see the "do not unify" note below). |
| `schedule.rs` | Frozen `TickSet` (23) / `EntitySet` (12) order, `RunContext`, `schedule_trace`/`render_trace`, `build_sim_schedule`. |
| `fixed.rs` | `SIM_STEP` and `FixedStepRunner` frame accumulator/catch-up. |
| `clock.rs` | `SimClock`/`RunQueue`: tick clock, delayed runs, lazy cancellation. |
| `config.rs` | `SimConfig` knobs (fixed Hz, catch-up, seed, workers, order trace). |
| `command.rs` | Relay-side `UnitCommandRuntime`; `pack_wire_target`/`resolve_wire_target`. |
| `world_apply.rs` | Relay-side `WorldApplyRuntime`: inventories, build plans, commanded buildings, digest. |
| `runtime/` | Opt-in plan-07 `SimRuntime` (`mod.rs`: `SimRuntime`, `SimRuntimeConfig`, re-exports); children `install`, `load`, `passes`, `block_runtime`, `teams`, `tile_ops`, `materialize`, `dynamic_consume`, `campaign`, `hooks`, `tests`. |
| `unit_runtime/` | Opt-in plan-11 `UnitRuntime` (`mod.rs`: `UnitRuntime`, `unit_by_id`, `TILE_SIZE`, `UNIT_SEQ_BASE`); children `payload_units`, `command_queue`, `deaths`, `install`, `combat_glue`, `player`, `debug`, `physics`, `update`, `systems`, `waves`, `payload_cmds`, `targeting`, `ai_weapons`, `ai/` (AI controller bodies), `ai_arms/` (the `unit_update_system` dispatch arms, one file per kind) and the test modules. |
| `builder/` | `BuilderPlans`/`UnitBuildPlan` (`mod.rs`) plus `queue`, `checks`, `update`: player build queue and the `BuilderComp.updateBuildLogic` pass. |
| `fog.rs` | `FogControl` glue: reset on load/rules, per-tick update, static mask, view seams. |
| `player_items.rs` | Live player-unit item withdraw/deposit/drop with Java range/team checks. |
| `accelerator.rs` | Interplanetary Accelerator `canLaunch`/launch campaign half. |
| `logic.rs` | State-free `Logic` run predicates (`is_game`, `is_game_active`, `check_game_state`). |
| `events.rs` | `Trigger` registry (41 values, registration-order listeners); view-only triggers never fire here. |
| `reset.rs` | `play`/`pause`/`resume`/`reset` flows. |
| `io_set.rs` | `IoQueue`/`IoSet`/`IoHandler` save-load boundaries, `DeferringIoHandler`. |
| `dump.rs` | `StateDump` JSON schema and canonical builder. |
| `test_support.rs` | Test-only fixtures shared by the sim suites (`live_sim_seeded`, `live_sim`, `runtime_sim`, `tick_n`, `place_at`, `place_team1`, `unit_content_block`, `campaign_runtime`, …). |
| `tests.rs` | Test-only P0 spine/determinism/dump suites. |
| `spine_tests/` | Test-only suites split out of `mod.rs`: `logic_sink_tests`, `team_ai_host_tests`, `ability_queue_tests`, `relay_player_team_tests`. |
| `combat_tests/` | Test-only live combat/unit fixtures (`mod.rs`: `live_sim` and helpers) plus 19 topic files. |
| `production_tests/` | Test-only production/block-runtime suites (`mod.rs` plus 17 topic files). |
| `logic_campaign_tests/` | Test-only logic/campaign runtime suites (`mod.rs` plus 7 topic files). |

### The nine core lookups — do not unify

`sim` spells the "find a core" read nine ways on purpose. They differ in predicate, marker and
tie-break, so a shared helper would change behaviour; keep each local copy (H08 report):

| Function | File | Predicate | Selection / tie-break |
|---|---|---|---|
| `has_core_of_team` | `unit_runtime/deaths.rs` | `TeamComp == team` + block content kind `BlockKind::CoreBlock` | `any` (archetype order) |
| `closest_enemy_core` | `unit_runtime/targeting.rs` | `BuildingComp.team != team`, not derelict, `CoreBuild` marker | min `dist2`, tie `entity.index()` |
| `closest_core_of_team` | `unit_runtime/targeting.rs` | `CoreBuild` marker + `TeamComp == team` | min `dist2`, tie `entity.index()` |
| `best_core_for_player` | `unit_runtime/player.rs` | `CoreBuild` + `TeamComp == team`; unsupported-env cores dropped when >1 core | max `size`, then min `dst2` (no index tie-break) |
| `enemy_core_targets` | `unit_runtime/targeting.rs` | `BuildingComp.team != team`, not derelict, `CoreBuild` marker | `sort_by_key((y, x))` + dedup; spawn tiles appended+dedup when `waves && default_team == team` |
| `builder::team_core` | `builder/checks.rs` | `CoreBuild` + `TeamComp == team` | lowest `EntitySeq` (`u64::MAX` if missing) |
| `builder::team_has_core` | `builder/queue.rs` | `team_core(..).is_some()` | delegates |
| `runtime::team_core_positions` | `runtime/teams.rs` | `CoreBuild` + `TeamComp == team` + `Building` | raw archetype order (unsorted) |
| `sim::is_own_core` | `spine/placement.rs` | `TeamComp == player_team` + `Building` + block-runtime instance kind `CoreBlock` | bool, first match |

The same rule applies to `building_center`: `unit_runtime/targeting.rs` prefers the stored `Pos`,
while `logic/executor/unit_control.rs` recomputes `tile * tilesize + block.offset` from `Building`.
Their fallbacks differ, so both stay (RULES §4).

## Contracts

- **Schedule order is frozen.** `TICK_SETS` is the 23-slot `Frame`→`PostQueue` chain; `ENTITY_SETS`
  is the 12-slot chain nested under `TickSet::EntityUpdate`. `build_sim_schedule` configures exactly
  that order on a single-threaded executor and systems register into named sets only; `RunContext`
  gates exist for `schedule_trace`, not as installed run conditions.
- **`Sim::tick` is the real pipeline.** Pre-passes (`Trigger::Update`, logic globals, fog, assembler
  occupancy) → `schedule.run` → post-passes (drills, beams, fires/puddles, builders, deaths, respawn,
  campaign requests, dirty rules) → `flush_events` → `time.update`/`clock.update`/`state.advance` →
  `Trigger::AfterGameUpdate` → `IoSet::Capture`. Most slots have no scheduled system yet; these
  hand-written passes are part of parity.
- **Fixed step.** `SIM_STEP = 1/60`; `SimClock.delta` is `1.0` inside a tick. `FixedStepRunner::new()`
  caps at 5, but `SimConfig` defaults `max_ticks_per_frame = MAX_TICKS_PER_FRAME = 4`; hosts build
  with `for_rate(config.fixed_hz, config.max_ticks_per_frame)`. Excess backlog is dropped
  (`accumulator %= step`); non-finite/non-positive deltas are ignored.
- **Command drain.** `Sim` holds no command queue: `apply`/`command` mutate immediately and must run
  before the tick they are stamped for (`ScenarioPlayer::step`; the host drains `pending_commands`
  at each tick start). Invalid placements/breaks are deterministic no-ops; `commands_applied` counts
  them. Unsupported/invalid `SimCommand`s return `CommandError`.
- **IO boundaries.** `tick` pumps `IoSet::Apply` pre-tick and `IoSet::Capture` after `AfterGameUpdate`;
  no mid-tick IO. The queue filters pending by set, hands them to the `IoHandler` and appends
  `IoResponse`s; without a handler they stay pending (`take_io_requests`). `flush_io()` pumps
  `Capture` then `Apply` without a tick (exit save).
- **Determinism/checksum.** Randomness only via `SimRng`/`JavaRandom`; iteration uses `EntitySeq`,
  sorted `entity.index()` or `BTreeMap`/`Vec`, never `HashMap` order. The stream is frozen: phase,
  tick, update_id, clock `time_units`, clock `runs_len`, `SimRng` `sim_state`, seed, grid row-major
  (block, team, rot, build id), then `BuildingComp` by seq; changes bump `CHECKSUM_VERSION`.
  `ChecksumScope` is a projection only (`include_view` false; `checksum_scoped` == `checksum`).
  Units/bullets/AI/inventories are outside it (`runtime_checksum`, `unit_runtime_checksum`,
  `unit_command_checksum`, `world_apply_checksum`).
- **Reset vs load.** `reset()` clears entities/grid/delayed runs/clock/state/map name/hints and
  returns to `Menu`, keeping content, runtime registries, RNG streams and entity sequence.
  `clear_world_for_load()` keeps phase/tick/schedule/runtime resources, dropping entities and fog.
  `SimIoHandler::load` restores `state.tick` from meta and re-derives controllers via
  `reset_loaded_controllers`, but neither restores nor clears `SimClock` (time/runs), so old-world
  delayed runs survive the load and a resumed save does not checksum-match the pre-save turn.

## Key types

- Spine: `Sim`, `SimBuilder`, `SimSnapshot`, `TickReport`, `SimConfig`, `HintEvents`, `ConfigTapKind`.
- Time/order: `FixedStepRunner`, `SimClock`, `TickSet`, `EntitySet`, `RunContext`.
- IO: `IoSet`, `IoQueue`, `IoRequest`, `IoResponse`, `IoStatus`, `IoHandler`, `DeferringIoHandler`.
- Opt-in runtimes: `SimRuntime`, `SimRuntimeConfig`, `UnitRuntime`, `BuilderPlans`, `UnitBuildPlan`,
  `WorldApplyRuntime`, `UnitCommandRuntime`.
- Dump/checksum: `StateDump`, `ChecksumScope`.

## Verification

From `client/rust/` (add `--manifest-path client/rust/Cargo.toml` from the repo root):

```bash
cargo test -p mind-core sim::                            # every sim module test
cargo test -p mind-core sim::schedule::tests::tick_set_order_is_frozen
cargo test -p mind-core sim::io_set::tests::io_boundaries_run_in_order_without_mid_tick_io
cargo test -p mind-core sim::tests::same_seed_same_checksums
cargo test -p mind-core io::sim_io::tests::live_sim_save_then_load_roundtrips_the_tile_grid

cargo run -p mind-headless -- run spine_place_break --json        # scenario golden
cargo run -p mind-headless -- run sim_core_reset_play_cycle --json
cargo run -p mind-headless -- trace order --json                  # 23 TickSet / 12 EntitySet
cargo run -p mind-headless -- dump --scenario spine_place_break --out /tmp/spine.json
```

## Gotchas

- Do not call `Sim::tick` from GDScript/UI glue or add a second mutation path: commands go through
  `apply`/`command` at tick starts. `tick` has no pause gate — callers must honor `is_paused()`.
- `Sim::checksum` is not a whole-world digest once `install_block_runtime`/`install_unit_runtime`
  ran; use the per-runtime digests above when asserting on units, bullets or inventories.
- Scenario checksums live in `scenarios/*.json` (`expect.checksum`); harness goldens under
  `client/rust/mind-headless/tests/golden/**` are pinned by sha256 in `parity/golden_manifest.json`.
  Change the stream only together with `CHECKSUM_VERSION` and both golden sets.
- Do not assume `build_sim_schedule` executes the whole tick (see Contracts); and do not "fix" the
  `FixedStepRunner` default of 5 against the configured default of 4 — the oracle uses the config.

