<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S12 — Split `sim/mod.rs` (3700), `sim/runtime.rs` (2961) and `sim/builder.rs` (2806)

**Wave:** W1 (pure mechanical split, RULES §3). **Determinism-sensitive: read RULES §5.**
**Owns:**
- `client/rust/mind-core/src/sim/mod.rs`. You do not touch the test-module declarations at ~3195–3202
  or F3's `test_support` line;
- the new `sim/spine/**` and `sim/spine_tests/**`;
- `sim/runtime.rs` → `sim/runtime/**`;
- `sim/builder.rs` → `sim/builder/**`.

**Effort:** large.

Read `client/rust/mind-core/src/sim/AGENTS.md` first. The `sim/` directory already has
`accelerator.rs, boot.rs, clock.rs, command.rs, config.rs, dump.rs, events.rs, fixed.rs, fog.rs, io_set.rs, logic.rs, player_items.rs, reset.rs, schedule.rs, tests.rs, world_apply.rs`.
New module names must not collide with them; that is why the `impl Sim` bodies from `mod.rs` go
under `spine/`.

## Part A — `sim/mod.rs` (Logic.update + GameState: the `Sim` struct and its core impl)

`mod.rs` keeps:
- 1–281: `pub mod` decls 8–25, `use`s, re-exports at 43 and 67–75, `HintEvents` 53–66,
  `SimError`, `SimSnapshot`, `ChecksumScope`, `ConfigTapKind`, and `struct Sim` (177–281);
- 3195–3202, the test-module declarations;
- the new `mod spine; #[cfg(test)] mod spine_tests;` lines.

The single `impl Sim` block (283–3046) and the free items after it are moved into
`sim/spine/*.rs`. Each file holds an `impl Sim { … }` (or the free fns). `spine/mod.rs` declares the
children.

| File | Lines at `291e30a8` | Contents | Upstream |
|---|---|---|---|
| `spine/construct.rs` | 283–380 | `new`, `from_scenario`, `bump_entity_seq` | `core/Logic.java` |
| `spine/apply.rs` | 381–513 | `apply(Command)` | `core/Logic.java` |
| `spine/placement.rs` | 514–595, 1127–1302 | team/placement predicates; `runtime_place`, `block_counter`, `is_own_core`, `place_for_team`, `place_loadout`, `with_world_ctx`, `stock_core`, `runtime_break` | `world/Build.java` |
| `spine/world_config.rs` | 596–743 | `apply_world_config`, `run_placed_hooks`, `configure_power_node`, `with_power_graphs` | `world/blocks/power/PowerNode.java` (configure) |
| `spine/live_unit_cmd.rs` | 744–1126 | `apply_live_unit_command` (232 lines), `apply_live_unit_stance`, `command_targets`, `drain_build_damage_events`, `drain_wall_flashes` | `ai/types/CommandAI.java`, `input/InputHandler.java` |
| `spine/tick.rs` | 1303–1393 | `tick` | `core/Logic.java` (`update`) |
| `spine/logic_bridge.rs` | 1394–1836 | `update_logic_globals`, `drain_logic_events`, `take_pending_logic_scripts`, `drain_logic_world_events`, `apply_logic_sync`, `take_logic_*`, `logic_set_block`, `logic_cutscene`, `reset_logic_presentation` | `logic/LExecutor.java`, `core/NetServer.java` |
| `spine/checksum.rs` | 1837–1999 | `state_name`, `group_counts`, accessors, `checksum`/`checksum_value` 1936, `checksum_hex`, `checksum_scoped` (**keep the fold order**) | – |
| `spine/command_dispatch.rs` | 2000–2448 | `command(SimCommand)`, 449 lines (**move whole**) | – |
| `spine/accessors.rs` | 2449–2523, 2700–2927 | snapshot/dump/seed/map name; phase, pause, selection, logic player view, content accessors; relay runtime accessors (`ensure_unit_commands`, `ensure_world_apply`, `world_*_id`, `push_event`) | – |
| `spine/rules_world.rs` | 2524–2699 | `apply_rules_to_world`, `check_all_allow_update`, `debug_set_building_team`, `reset_map_area` | `core/World.java` |
| `spine/event_flush.rs` | 2928–3046 | `flush_events` | – |
| `spine/abilities.rs` | 3047–3193 | `KIND_BUILDING`, `spawn_logic_bullet` 3064, `drain_ability_queues_system` 3115 | `entities/abilities/{MoveEffectAbility,UnitSpawnAbility}.java` |
| `spine_tests/{logic_sink_tests,team_ai_host_tests,ability_queue_tests,relay_player_team_tests}.rs` | 3207–3376, 3381–3498, 3502–3668, 3672–3700 | the four inline test mods, keeping their module names. Their test paths become `sim::spine_tests::<name>::…`. A re-export cannot preserve a test path, so grep `TWIN-VERIFICATION.json` and `parity/` for `sim::ability_queue_tests`, `team_ai_host_tests`, `relay_player_team_tests` and `logic_sink_tests`, and list every hit with its new path for C01. Each moved test module had `use super::*;` and now sits one level deeper: write `use crate::sim::*;` plus the extra imports it needs (RULES §3.3) | – |

**Visibility.**
- Every private `impl Sim` method you move becomes `pub(in crate::sim)`. Before the move it was
  visible to all of `sim::*`, because it was defined in `sim` itself. Examples: `push_event` (used by
  `reset.rs`, `builder.rs`), `runtime_place` (used by `unit_runtime.rs:4987`),
  `apply_live_unit_command`, `with_world_ctx`.
- Items that are already `pub` (e.g. `pub const KIND_BUILDING`, `pub fn with_world_ctx`) keep their
  visibility exactly. Only **private** items are widened, to `pub(in crate::sim)`.
- **`sim/tests.rs` (not yours) does `use super::*`** at line 5 and relies on `sim/mod.rs`'s private
  imports. When an import in `mod.rs` becomes unused by non-test code, check whether `sim/tests.rs`
  (or `fog.rs`'s tests, `reset.rs`, …) still needs it through the glob. If so, keep it, marked
  `#[cfg(test)]` if only tests need it.
- Re-export `spawn_logic_bullet` and `drain_ability_queues_system` from `mod.rs` so that
  `crate::sim::spawn_logic_bullet` (2 importers) and the schedule paths stay valid. Everything at
  `crate::sim::*` must keep resolving.

## Part B — `sim/runtime.rs` → `sim/runtime/`

| File | Lines | Contents | Upstream |
|---|---|---|---|
| `runtime/mod.rs` | 1–105, 1231–1257 | `RuntimeError`, `SimRuntimeConfig`, `ContentIndex`, `load_vanilla_content`, `ReplayBuilding`; `mod`/`pub use` lines | – |
| `runtime/hooks.rs` | 106–167 | `SimHooks` + `impl WorldHooks` | – |
| `runtime/block_runtime.rs` | 168–498, 1192–1230 | `struct SimRuntime`, placement and config (`placement_allowed`, `resolve_config`, `place`, `placed_hooks`, `can_place`, `place_schematic`, `with_world_ctx`), `checksum` | `world/Build.java` |
| `runtime/tile_ops.rs` | 499–937 | `set_block_raw`, `construct_block`, `begin_place`, `begin_break`, `drop_deconstruct_liquid`, `break_block`, `remove_block_raw`, `destroy_building`, `pickup_building` | `world/Tile.java` (`setBlock`), `world/blocks/ConstructBlock.java` |
| `runtime/materialize.rs` | 938–1191 | predicates, `remove_building`, `materialize_buildings`, proximity, power graph | `entities/comp/BuildingComp.java` |
| `runtime/passes.rs` | 1259–1753 | `impl Sim` accessors 1261–1280, the post-tick passes (`refresh_facing_drills`, `refresh_assembler_occupancy`, `refresh_thermal_generators`, `update_item_explode_consumers`, `drain_consumer_damage`, `apply_generator_explosions` 1524, `refresh_beam_nodes`) | – |
| `runtime/install.rs` | 1754–1875 | install | – |
| `runtime/teams.rs` | 1876–2106 | `set_building_team`, `destroy_team_to_derelict`, `time_destroy`, `team_core_positions` (**unsorted archetype order; keep it**), `live_buildings_by_seq`, `kill_building` | `game/Teams.java` |
| `runtime/campaign.rs` | 2107–2310 | `apply_campaign_world_requests`, `apply_dirty_rules_to_world`, `materialize_map_buildings`, `capture_lost_sector` | – |
| `runtime/load.rs` | 2311–2401 | `place_replay_building`, `clear_world_for_load`, `rebuild_power_graphs` | – |
| `runtime/dynamic_consume.rs` | 2402–2486 | `SimDynamicConsume`. **Re-export it**: `TWIN-VERIFICATION.json:30019` cites `sim::runtime::SimDynamicConsume` | `world/consumers/ConsumeItemDynamic.java` (check) |
| `runtime/tests.rs` | 2488–2961 | 9 tests | – |

Paths to keep (re-export from `runtime/mod.rs`):
- `ContentIndex` (6 importers);
- `load_vanilla_content` (`legacy.rs`, `game/runtime.rs`, `content/boot.rs`, `unit_runtime`);
- `SimRuntimeConfig` (mind-gdext `sim_host.rs`);
- `ReplayBuilding`;
- `RuntimeError`;
- `SimRuntime` (`builder.rs:41`).

## Part C — `sim/builder.rs` → `sim/builder/`

| File | Lines | Contents | Upstream |
|---|---|---|---|
| `builder/mod.rs` | 1–190 | `DERELICT`, `block_bounds`, `rects_overlap`, `check_overlapping_ghosts`, `UnitBuildPlan`, `BuilderPlans` | `entities/comp/BuilderComp.java` |
| `builder/checks.rs` | 191–412 | `BuildRuleView`, `team_core` 238 (**lowest EntitySeq; keep**), `building_team`, `validate_plans`, `should_skip`, `tile_world`, `within`, `no_unit_overlap`, `unit_overlap_at` 357 | `world/Build.java` |
| `builder/update.rs` | 413–904, 1375–1498 | `BuilderOutcome`, `chain_bridge`, `update_builder` (422 lines, **whole**), `update_builders`, `emit_construct_fx` | `entities/comp/BuilderComp.java` (`update`) |
| `builder/queue.rs` | 906–1374 | the `impl Sim` queue API | `input/InputHandler.java` |
| `builder/tests.rs` | 1500–2806 | 31 tests, fixtures 1505–1632 | – |

Paths to keep: `BuilderPlans` (12 importers), `UnitBuildPlan` (7), `unit_overlap_at` (mind-gdext
`sim_host.rs:6818`).

## Order

Do Part B first (smallest blast radius), then C, then A. Commit after each part, and run
`run --cargo-check --workspace --all-targets` after each.

## Verification (RULES §6, §5)

```bash
B=$(git merge-base main HEAD); S=client/rust/mind-core/src/sim
python3 tools/refactor/move_check.py --base $B --old $S/mod.rs --new $S/mod.rs $S/spine $S/spine_tests
python3 tools/refactor/move_check.py --base $B --old $S/runtime.rs --new $S/runtime
python3 tools/refactor/move_check.py --base $B --old $S/builder.rs --new $S/builder
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-core --lib
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --cargo-run -q -p mind-headless -- run-all --tier T0
for s in spine_place_break spine_determinism spine_many_commands; do run --cargo-run -q -p mind-headless -- run $s --json; done
run --goldens --check
```

**Do not** edit `sim/schedule.rs`. If a system path breaks there, add a re-export instead.

Report "Docs to update in C01": the `sim/AGENTS.md` layout table.
