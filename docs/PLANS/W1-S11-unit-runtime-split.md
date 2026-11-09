<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S11 — Split `sim/unit_runtime.rs` (9496 lines) into `sim/unit_runtime/`

**Wave:** W1 (pure mechanical split, RULES §3). **Determinism-sensitive: read RULES §5.**
**Owns:** `client/rust/mind-core/src/sim/unit_runtime.rs` → `client/rust/mind-core/src/sim/unit_runtime/**`.
**Effort:** large.

Read `client/rust/mind-core/AGENTS.md` and `client/rust/mind-core/src/sim/AGENTS.md` first.

## What it is

The unit half of the simulation. It covers:
- `UnitRuntime`, a **non-send** ECS resource (`struct` 367–421, impl 423–634);
- one 1885-line `impl Sim` block (700–2584): deaths, install/waves, player/possession, debug;
- the ECS systems: `unit_physics_system` 2784, `unit_update_system` 2889–3838 (948 lines),
  turret/bullet systems 3994–4163, `run_wave_system` 4210, `enemy_count_system` 5066;
- the AI controllers (5125–6036), targeting (6037–6706) and AI weapons (6404–7033);
- tests (7048–9496).

Upstream files: `entities/comp/UnitComp.java`, `core/Logic.java` (`updateEntities`, `runWave`),
`ai/WaveSpawner.java`, `ai/types/{GroundAI,FlyingAI,DefenderAI,FlyingFollowAI,…}.java`,
`entities/units/AIController.java`, `entities/UnitPayload…`, `input/InputHandler.java`, `entities/Damage.java`,
`entities/comp/{BuildingComp,LegsComp,CrawlComp}.java`, `async/PhysicsProcess.java`,
`async/AvoidanceProcess.java`.

## Target layout

`git mv sim/unit_runtime.rs sim/unit_runtime/mod.rs`. `sim/mod.rs` keeps `pub mod unit_runtime;`
unchanged; it is not yours to edit.

| File | Lines at `291e30a8` | Contents |
|---|---|---|
| `mod.rs` | 1–91, 332–634 | header, imports (incl. `use super::runtime::{RuntimeError, load_vanilla_content}` at 64; leave it as is, S12 keeps that path valid), `UNIT_SEQ_BASE` 71, `unit_by_id` 80, `TILE_SIZE` 334, `Falling` 341, `WreckDecalInput` 350, `UnitRuntime` + impl; `mod` lines; the `pub use` block |
| `payload_units.rs` | 92–273 | `create_unit_payload` 105, `spawn_unit_payload` 129, `unit_payload_can_dump` 159, `dump_carried_unit` 234 |
| `command_queue.rs` | 274–331 | `pop_live_command_queue` (CommandAI; its one caller is ~3263) |
| `deaths.rs` | 635–698, 700–1345 (as `impl Sim`), 2585–2675 | `run_shoot_on_death`, `resolve_deaths` (533 lines; **move whole**), `emit_building_destroyed`, `has_core_of_team`, `explosion_item_cap`, `building_explosion_terms` |
| `install.rs` | 1346–1561, 7034–7046 | `install_unit_runtime[_from]`, `set_unit_wave_team`, `request_wave_spawn`, `logic_skip_wave`, `logic_spawn_wave_at`, `refresh_unit_runtime`, `groups_from_rules` |
| `combat_glue.rs` | 1562–1699 | `update_fires_and_puddles`, `with_combat_ctx` 1623, `drain_delayed_explosions` 1665, `unit_runtime_checksum` 1691 |
| `player.rs` | 1700–2410 | `spawn_live_unit`, `held_payload*`, `set_player_*`, mining, `spawn_player_*`, `best_core_for_player` 1866, `update_player_respawn`, `possess_unit` 2044, `release_unit_to_ai`, `clear_unit` 2191, `control_select_core`, `control_block_unit_id`, `ensure_router_block_unit` |
| `debug.rs` | 2411–2584, 4419–4585 | `player_unit`, `debug_kill_unit`, `debug_apply_status`, `boss_json`, `bosses`, `player_unit_position/moved`; `selectable_units_json`, `any_unit_has_command`, `unit_debug_json` |
| `physics.rs` | 2676–2888 | `PhysicsBody`, `collision_layer`, `rebuild_avoidance`, `unit_physics_system` |
| `update.rs` | 2889–3993 | `unit_update_system` (**move whole; do not touch its body**), `apply_movement_damage` 3856, `adopt_spawned_units` 3967 |
| `systems.rs` | 3994–4163, 5063–5096 | `turret_update_system`, `bullet_update_system`, `bullet_collide_system`, `enemy_count_system` |
| `waves.rs` | 4164–4418, 5018–5062 | `spawn_group_unit` 4172, `run_wave_system` 4210, `core_spawn_position` |
| `payload_cmds.rs` | 4586–5017 | `apply_payload_command`, `payload_pickup_unit` 4675, `payload_pickup_build` 4819, `payload_drop` 4919 |
| `ai/mod.rs` | 5097–5124 | `builder_falls_back`, `resolve_command_kind` |
| `ai/ground.rs` | 5125–5433 | `GroundAiRules` 5130, `update_ground_ai` 5153, `propagate_stuck` 5379 |
| `ai/flying.rs` | 5434–5579 | `update_flying_ai` 5439 |
| `ai/defender.rs` | 5580–5650 | `update_defender_ai` |
| `ai/follow.rs` | 5651–5824 | `update_flying_follow_ai` |
| `ai/hug.rs` | 5825–6036 | `update_hug_ai` |
| `targeting.rs` | 6037–6403, 6552–6706 | `look_at_world`, `flying_main_target`, `block_targetable`, `target_flag_active`, `entity_pos_within`, `unit_flowfield_cost`, `unit_control_cost`, `enemy_core_targets`, `closest_enemy_core` 6277, `closest_core_of_team` 6311, `closest_spawn_tile`, `within_spawn`, `invalid_target` 6371; `building_center`, `tile_world`, `face_target`, `target_hit_size(_of)`, `invalidate_target(_entity)` |
| `ai_weapons.rs` | 6404–6551, 6707–7033 | `control_weapons_for`, `update_ai_weapons_for`, `command_ai_weapon_pass`, `logic_ai_weapon_pass`, `ROTATE_BACK_TIMER`, `MountUpdate`, `AiWeaponsState`, `AiWeaponCtx`, `update_ai_weapons` 6765 |
| `tests.rs` | 7048–8241 | `mod tests` (33 tests). **The module name must stay `tests`**: `TWIN-VERIFICATION.json` cites `sim::unit_runtime::tests::{legged_unit_splash…, crawler_crushes…, world_logic_blocks_are_not_targetable}`. Declare it `#[cfg(test)] mod tests;` |
| `msav_tests.rs` | 8243–9111 | keep `#[cfg(all(test, feature = "msav-import"))]` on its `mod` line |
| `markers_region_tests.rs` | 9113–9192 | same cfg |
| `unit_payload_tests.rs` | 9197–9496 | |

`ai/` is a new subdirectory of `unit_runtime/` (`unit_runtime/ai/mod.rs`). It does not collide with
`crate::ai`. Inside the sim, always write `crate::ai::…` for the top-level AI crate module, and
`super::ai::…` (or `crate::sim::unit_runtime::ai::…`) for this one.

## Visibility and re-exports

- Items that were private in `unit_runtime` and are used across the new files get
  `pub(in crate::sim::unit_runtime)`, or `pub(super)` for direct children of `mod.rs`. Known
  cross-file users:
  - `TILE_SIZE`: 16 uses;
  - `GroundAiRules`: 8;
  - `update_ai_weapons`: 10;
  - `adopt_spawned_units`: 4;
  - `face_target`: 4;
  - `closest_spawn_tile`: 4;
  - `tile_world`, `building_center`, `core_spawn_position` (used ~4334/4337 from waves), …

  Let the compiler tell you the rest.
- Private `Sim` **fields** are already visible, because every file is a descendant of `sim`.
- Private methods of `impl Sim` that you move here and that other `sim` files call need
  `pub(in crate::sim)`; let the compiler point at each one. (Methods defined in `sim/mod.rs`, such as
  `apply_live_unit_command` at 744, are S12's, not yours.)
- `mod.rs` must `pub use` / `pub(crate) use` every item named by path elsewhere:
  - `UnitRuntime`: about 30 sites, incl. mind-gdext `sim_host.rs` and mind-headless;
  - `unit_by_id`, `create_unit_payload`, `spawn_unit_payload`, `unit_payload_can_dump`,
    `dump_carried_unit`, `UNIT_SEQ_BASE`, `Falling`;
  - the seven systems (`sim/schedule.rs:443–461`; **do not edit schedule.rs**; `unit_update_system` is
    also used by `game/runtime.rs:2201` and `audio/steps.rs`; `run_wave_system` by `ai/wave_spawner.rs`);
  - `closest_enemy_core`, `closest_core_of_team`, `invalid_target`, `control_weapons_for`,
    `update_ai_weapons_for`, `target_hit_size_of`, `invalidate_target_entity`.

  `release_unit_to_ai`, `payload_pickup_unit`, `payload_pickup_build` and `payload_drop` are
  `impl Sim` **methods**, not free items. They need no re-export (a `use` cannot name a method); they
  only need the visibility rule.

  Get the full list with:
  `grep -rhoE "unit_runtime::\{?[A-Za-z_, ]+" client/rust --include=*.rs | tr ',{' '\n\n' | sed 's/unit_runtime:://; s/ //g' | sort -u`.

## Rules specific to this file

- **Never edit inside** `unit_update_system`, `resolve_deaths` or any system body. Their loop order,
  `runtime.units.clone()` order, RNG call order, `else if` arm order and the four `continue`s are
  pinned by checksums. X02 (W3) refactors the dispatch with its own safety net.
- Keep `remove_non_send`/`insert_non_send` pairs inside the same function, as they are.

## Verification (RULES §6, §5)

```bash
python3 tools/refactor/move_check.py --base $(git merge-base main HEAD) \
   --old client/rust/mind-core/src/sim/unit_runtime.rs --new client/rust/mind-core/src/sim/unit_runtime
run --cargo-check --workspace --all-targets
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-core --lib
run --cargo-test -p mind-core --features msav-import sim::unit_runtime   # record the result at baseline and after; it must not regress
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --cargo-run -q -p mind-headless -- run-all --tier T0
run --cargo-run -q -p mind-headless -- run spine_determinism --json
run --goldens --check
```

The key determinism tests are `sim::tests::same_seed_same_checksums` and
`sim::unit_runtime::tests::live_units_are_deterministic`. The test count must be unchanged
(`tests-mind-core.txt` baseline), with only paths changed for the moved test modules.

Report "Docs to update in C01": the layout table in `sim/AGENTS.md`.
