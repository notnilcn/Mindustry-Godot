# Combat + Units simulation — implementation plan (parity session 2)

Executor brief. Everything you need is in this file; you do not need any prior
conversation. Work items top to bottom (section 3 order). Each item lists the
symptom, acceptance criterion, Java reference, current port state, ordered
changes, tests, verification commands and pitfalls. Do not skip the tests.

All paths below are relative to the worktree root
`/home/clintonnguyen/g/code_examples/Mindustry-Godot-session2` unless they start
with `Mindustry/` (the read-only Java reference at
`/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/...`).
Rust paths under `mind-core/` mean `client/rust/mind-core/`.

---

## 0. Environment & workflow

- **Worktree:** `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session2`,
  branch `parity/session-2`. Commit only here. Never touch
  `/home/clintonnguyen/g/code_examples/Mindustry-Godot` (main checkout) or other
  `-sessionN` worktrees. Do not push, do not merge.
- **Do not edit** `chains.json` or `parity-ledger.json` (coordinator owns them).
- `runs/` is a symlink to the shared main checkout's `runs/`; write evidence to
  `runs/20261012-session2/` but do **not** `git add` it (git refuses: "beyond a
  symbolic link").
- **RAM is tight** (shared by 4 agents): always `CARGO_BUILD_JOBS=6`, one cargo
  process at a time, never start an extra Godot editor.
- **Rust tests (fast loop, ~1-4 min incremental):**
  ```bash
  cd client/rust
  CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib <filter> -- --nocapture
  # Ground Zero map tests are behind a feature flag:
  CARGO_BUILD_JOBS=6 cargo test -p mind-core --features msav-import --lib ground_zero -- --nocapture
  CARGO_BUILD_JOBS=6 cargo test -p mind-core            # full core suite (expect 1852+ pass, 0 fail)
  cargo fmt -p mind-core
  CARGO_BUILD_JOBS=6 cargo clippy -p mind-core --all-targets -- -D warnings   # must print nothing
  ```
- **Known pre-existing failures (NOT yours, do not fix, do not be alarmed):**
  `cargo test -p mind-headless --lib` fails 6 tests on the untouched baseline:
  `campaign_scenarios::tests::sector_golden_matches`,
  `parity::matrix::tests::landed_rows_must_resolve_when_list_supplied`,
  `parity::registry::tests::committed_registry_matches_core_constant`,
  `parity::registry::tests::fold_order_must_match_active_contributors`,
  `parity::report::tests::full_check_set_passes`,
  `units_scenarios::tests::committed_goldens_match`. Any *other* new failure is yours.
- **Build the GDExtension + restart the editor** (the `.so` is mmap'd, so every
  build needs a restart):
  ```bash
  CARGO_BUILD_JOBS=6 tools/build.sh          # ~5 min cold, ~20-60 s incremental
  tools/session-play.sh 2                    # restart editor on display :11, bridge 6980, play game.tscn
  tools/session-play.sh 2 --play             # replay the scene only (use after a stuck eval)
  export OPEN_GODOT_MCP_PORT=6980
  ```
- **Driving Godot** (`tools/gmcp.py`): every `--eval` must be a single GDScript
  expression statement that `return`s; no loops (a bad eval can pin the frame
  loop; recover with `tools/session-play.sh 2 --play`). Several `--eval` flags
  run in order in one call.
  ```bash
  G="python3 tools/gmcp.py"
  $G --eval 'return get_node("/root/Spine/SimHost").get_group_counts()'
  $G --screenshot runs/20261012-session2/<name>.png      # then view it with the Read tool
  ```
  Useful SimHost funcs: `place_block(x,y,"name")`, `break_block(x,y)`,
  `get_state_json()`, `get_group_counts()`, `get_group_count("unit"|"bullet")`,
  `get_tick()`, `step(n)`, `set_paused(b)`, `content_list("unit")`,
  `apply_sim_command_json(json)` with ops `place`, `break`, `spawn_unit`
  (`{"op":"spawn_unit","unit":<index in content_list("unit")>,"team":2,"px":<world x>,"py":<world y>}`;
  dagger is index 0), `unit_command`. Input node `/root/Spine/Input`:
  `toggle_command_mode()`, `command_mode()`, `selection_json()`,
  `clear_building()`, `select_block_by_name(n)`, `configure_building(x,y,kind,value)`,
  `transfer_item(x,y,item,amount,deposit)`, `pickup_payload()`, `drop_payload()`,
  `push_event(json)`, `get_input_state_json()`.
- **Enter Ground Zero the way the UI does** (sector 15):
  ```bash
  $G --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 15); return true' \
     --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 15)' \
     --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
  ```
  Ground Zero facts (verified): core-shard occupies tiles (128..130, 54..56),
  center (129,55). A free 2x2 copper-ore patch for a `mechanical-drill` is at
  (133,49) with a free tile at (135,49) for a `duo` (the drill feeds the duo).
  World pixel = tile * 8 (+4 for tile center). The sim runs ~100+ ticks per
  wall-second in the editor.
- **Rendering caveat:** in the session-2 editor (software GL on Xvfb) the world
  draws as white/blank tiles. That is the rendering agent's problem; verify by
  state JSON / group counts, keep screenshots only as context.
- **Commit conventions:** one commit per fix, message
  `Fixes PAR-XXXX: <summary>` (or `PAR-XXXX: <summary>` for tests/evidence),
  body explains the port, last line exactly:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  `git add -A client/rust parity docs` (never `runs/`).
- **Code invariants (client/rust/AGENTS.md):** every new file starts with
  `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
  and `// SPDX-License-Identifier: GPL-3.0-only`; cite the Java
  class/method in doc comments; no `unwrap()`/`expect()` outside `#[cfg(test)]`
  (clippy denies them); `mind-core` must not `use godot`/`use tokio`;
  `mind-gdext` holds no game rules; determinism: iterate entities in a stable
  order (sort by `EntitySeq`, then `entity.index()`), draw randomness only from
  the `SimRng` you are handed (`rng.range(RngStream::Sim, a, b)`,
  `rng.chance(RngStream::Sim, p)`), never `HashMap` iteration order.

### 0.1 Live-sim architecture you must know

- `Sim` (`mind-core/src/sim/mod.rs`) owns `ecs.0: bevy World`, `grid: WorldGrid`
  (the authoritative tile grid, **not** in the World), and
  `block_runtime: Option<SimRuntime>` (`sim/runtime.rs`; place/break/destroy
  through tile ops).
- `UnitRuntime` (`sim/unit_runtime.rs`) is a **non-send resource** in the World
  (`world.remove_non_send::<UnitRuntime>()` ... `world.insert_non_send(runtime)`).
  It owns `content: ContentRegistry`, `grid: WorldGrid` (a *snapshot clone* of
  `Sim.grid`, refreshed by `Sim::refresh_unit_runtime()` after tile changes),
  `pathfinder`, `units: Vec<Entity>`, `bullets: Vec<Entity>`, `rng`, `seq`
  (shared entity-sequence allocator for units *and* bullets, starts at
  `UNIT_SEQ_BASE = 1_000_000_000`), counters.
- Both runtimes boot content via `sim/runtime.rs::load_vanilla_content()`
  (which also calls `turrets::install_turret_ammo`). Two separately built but
  identical registries → identical ids.
- Schedule (`sim/schedule.rs::build_sim_schedule`): `EntitySet` chain
  `PoolCleanup → PhysicsBullets → PhysicsUnits → UpdatePlayers → UpdateEffects →
  UpdateAll → UpdateUnits → UpdateUnitsEditor → UpdatePowerGraph →
  UpdateBuildings → UpdateBullets → CollideBullets` inside `TickSet::EntityUpdate`.
  Registered live systems: `update_buildings` + `turret_update_system`
  (UpdateBuildings, turret after buildings), `unit_update_system` (UpdateUnits),
  `bullet_update_system` (UpdateBullets), `bullet_collide_system`
  (CollideBullets), `run_wave_system` (TickSet::RunWave), `enemy_count_system`
  (TickSet::AfterGameUpdate), campaign systems (`campaign_tick_system`,
  `objectives_tick_system`, `game_state_tick_system`, `team_ai_system` in
  `TickSet::WeatherAndAi`). Never reorder the sets.
- `Sim::tick()` (`sim/mod.rs` ~line 527): `pump_io(Apply)` → `Trigger::Update` →
  `schedule.run` → **`resolve_deaths()`** → `flush_events` → time/clock/state →
  `Trigger::AfterGameUpdate` → `pump_io(Capture)`. Work that needs `&mut
  Sim.grid` or `block_runtime` (anything that edits tiles) must run here, in
  `Sim::tick`, not inside a scheduled system.
- Borrow pattern for combat passes (copy it):
  ```rust
  let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else { return; };
  let audio = world.remove_resource::<AudioSinkRes>().unwrap_or_else(AudioSinkRes::noop);
  let mut spawned: Vec<Entity> = Vec::new();
  {
      let UnitRuntime { content, grid, rng, fx, seq, .. } = &mut runtime;
      let mut ctx = CombatCtx { world, content, grid, rng, fx: fx.as_ref(),
                                audio: &audio, seq, spawned: &mut spawned };
      /* ... */
  }
  world.insert_resource(audio);
  runtime.bullets_created += spawned.len() as u64;
  runtime.bullets.extend(spawned);
  world.insert_non_send(runtime);
  ```

---

## 1. Already done — do not redo

| Item | Commit | What landed | Evidence |
|---|---|---|---|
| PAR-0086 turrets inert | `91ed4a4`, test `8b64998` | Turret ammo bullets named in `ContentRegistry` (`name_bullet`/`bullet_by_name`/`bullet_names`, `content/load.rs`); `turrets::install_turret_ammo` called from `load_vanilla_content`; `defense::register` reads `content.bullet_names()` (the `FIXTURE_NAMES` OnceLock is gone); `turret_update_system` (`sim/unit_runtime.rs`) registered in `EntitySet::UpdateBuildings` after `update_buildings`. | `sim/combat_tests.rs::placed_duo_kills_enemy_dagger`; `sim/unit_runtime.rs::msav_tests::ground_zero_drill_fed_duo_kills_wave_daggers` (feature `msav-import`); in-engine Ground Zero: drill (133,49) + duo (135,49), spawned team-2 daggers die (unit count 2 → 1, bullets seen). Notes in `runs/20261012-session2/PAR-0086-0141-ingame.txt`. |
| PAR-0141 bullets pass through units | `91ed4a4` | `combat/bullet/mod.rs`: `unit_is_flying`, `swept_collide` (Arc `EntityCollisions.collide`), `collide_units` (`Groups.bullet.collide` unit half), `hit_entity` (`BulletType.hitEntity`: maxDamageFraction, shield, armor/pierceArmor/armorMultiplier, knockback impulse, status chance, handlePierce). Wired in `bullet_collide_system`. | same tests |
| Nothing ever died (new gap, fixed) | `91ed4a4` | `Sim::resolve_deaths()` (`sim/unit_runtime.rs`, called in `Sim::tick` after the schedule): units with `Health.dead`/hp≤0 → `kill_unit_with_abilities` (adopts `SpawnDeathAbility` spawns into `runtime.units`); buildings at hp≤0 → `SimRuntime::destroy_building` (`sim/runtime.rs`: `on_destroyed`/`after_destroyed` + shared `remove_building`) + `BlockBrokenEvent` + `refresh_unit_runtime`. | same tests |
| Targeting order/gates (new gap, fixed) | `91ed4a4` | `combat/targeting.rs::closest_target` now units-first (`Units.bestTarget`), unit gate `(flying && target_air) || (!flying && target_ground)`, range `+ hitSize/2`, dead skipped; new `closest_unit_filtered`. Turret `validate_target` drops dead targets. | full suite green |
| PAR-0087 turret coolant | `c3d9d1a` | 20 turret specs in `content/registries/blocks/turrets.rs` carry `consume_optional(consume_coolant(x,true,false)).update(false)` or `consume_optional(consume_liquid("water", x)).update(false)` + `has_liquids: Some(true)`; `parity/golden_content.json` has_liquids true for 19 turrets; `TurretConfig.coolant_liquids: Vec<LiquidId>`; `config_for` → `apply_block_coolant` + `COOLANT_MULTIPLIERS` table; `accept_liquid`/`handle_liquid` (block `liquidCapacity`); `handle_reload` drains coolant (`updateCooling`). | `sim/combat_tests.rs::duo_water_coolant_speeds_fire_and_drains` |

Not yet verified in-engine for PAR-0087 (needs a conduit/pump into a duo); the
unit test covers the sim. Optional in-engine check: in Ground Zero there is no
water near the core; skip unless you find water.

---

## 2. Item order and dependencies

1. PAR-0088 spawnUnit bullets (needs: MissileAI dispatch, TimedKill tick,
   weapon shootOnDeath) — section 3.
2. NEW-1 live unit statuses + abilities (section 4) — do before PAR-0091
   (puddles apply statuses) and PAR-0127 (weather applies statuses).
3. PAR-0137 floor speed (section 5) — independent, small.
4. PAR-0140 RTS selection/orders (section 6) — independent (gdext + core).
5. Wave attack / core loss verification (section 7) — after 1-2.
6. PAR-0091 fires/puddles live update (section 8), then PAR-0090 save/load
   (section 9).
7. PAR-0127 weather (section 10) — after NEW-1.
8. PAR-0138 payload carrier (section 11) — largest; do last.
9. Other gaps (section 12) — only if time remains.

---

## 3. PAR-0088 — bullets with `spawnUnit` never spawn their unit

**Symptom:** scathe volleys and the anthicus / quell / disrupt missile weapons
deal no damage (no missile unit appears).
**Acceptance:** a bullet whose `BulletDef.spawn_unit` is `Some(id)` spawns that
unit at the bullet origin instead of a bullet; the missile flies forward,
homes on the shooter's aim, dies at end of lifetime or on enemy-building
contact, and its `shootOnDeath` weapon fires its explosion bullet, damaging the
target. Test: an anthicus (team 2) next to a team-1 copper wall/dagger destroys
it within N ticks; a `*-missile` unit entity exists in between.

### 3.1 Java reference
- `Mindustry/entities/bullet/BulletType.java:932-961` (`create(...)`):
  if `spawnUnit != null`: `spawned = spawnUnit.create(team)`, `set(x,y)`,
  `rotation = angle`, `if(spawnUnit.missileAccelTime <= 0) vel.trns(angle,
  spawnUnit.speed)`, `MissileAI.shooter = shooter unit`, `add()`; then
  `if(killShooter && owner alive) owner.kill()`; **return null (no bullet)**.
- `BulletType.java:638-647` `createUnits` (`despawnUnit`, called from
  `hit` line 577 and `despawned` 650-660 when `!despawnHit`): spawn
  `despawnUnitCount` units of `despawnUnit` within `despawnUnitRadius`,
  `rotation = faceOutwards ? angle : b.rotation()`, chance `despawnUnitChance`.
- `Mindustry/ai/types/MissileAI.java:19-38` `updateMovement`:
  ```java
  float time = unit instanceof TimedKillc t ? t.time() : 1000000f;
  if(time >= unit.type.homingDelay && shooter != null && !shooter.dead()) unit.lookAt(shooter.aimX, shooter.aimY);
  unit.moveAt(vec.trns(unit.rotation, unit.type.missileAccelTime <= 0f ? unit.speed()
       : Mathf.pow(Math.min(time / unit.type.missileAccelTime, 1f), 2f) * unit.speed()));
  var build = unit.buildOn();
  if(build != null && unit.type.targetGround && build.team != unit.team && (build == target || !build.block.underBullets)){
      unit.hasTarget = true; unit.kill();
  }
  ```
  `MissileAI.java:42` target filter: `Units.closestTarget(team, x, y, range, u -> u.checkTarget(air, ground) && !u.isMissile(), ...)`.
- `Mindustry/entities/comp/TimedKillComp.java:16-23`:
  `time = min(time + Time.delta, lifetime); if(time >= lifetime) kill();`
  (`lifetime` comes from `UnitType.lifetime`, default MissileUnitType `60*1.7`).
- `Mindustry/type/unit/MissileUnitType.java:12-44` defaults (flying, speed 4,
  lifetime 102, rotateSpeed 2.5, range 6, controller MissileAI, TimedKillUnit).
- `Mindustry/entities/comp/UnitComp.java:884-920` `destroy()`; lines 910-919:
  ```java
  for(WeaponMount mount : mounts){
      if(mount.weapon.shootOnDeath && !(mount.weapon.bullet.killShooter && mount.totalShots > 0)){
          mount.reload = 0f; mount.shoot = true; mount.weapon.update(self(), mount);
      }
  }
  ```

### 3.2 Current port state
- `mind-core/src/content/registries/bullets.rs:405` `BulletDef.spawn_unit:
  Option<UnitTypeId>`, `:407` `despawn_unit`. Filled for unit weapons by
  `content/registries/units/mod.rs:2213-2261` (anthicus/quell/disrupt missiles
  are real unit defs: `units/erekir.rs:1616, 2661, 2774`).
- `mind-core/src/combat/bullet/spawn.rs:86` `create(world, content, rng, seq,
  spawn) -> Option<Entity>` ignores `spawn_unit` entirely (always builds a bullet).
- Scathe ammo is a placeholder: `world/blocks/defense/turrets/advanced.rs`
  ~line 885-905 registers `scathe_carbide`/`scathe_phase`/`scathe_surge` as
  `BulletKind::Empty` with **no** `spawn_unit` ("plan 11 blocker" comment).
  Java scathe missiles: `Mindustry/content/Blocks.java:5257` (`scathe-missile`),
  `:5345` (`scathe-missile-phase`), `:5450` (`scathe-missile-surge`),
  `:5506` (`scathe-missile-surge-split`). These unit types do **not** exist in
  the port's unit registry.
- `mind-core/src/ai/types/missile.rs:28` `update_missile(ctx: &mut AiCtx, unit,
  state)` exists (harness only) and **kills the missile immediately when
  `TimedKillComp.time <= 0`** — wrong semantics (time counts *up* from 0 in Java).
- `entities/comp/unit/lifecycle.rs:278-283` inserts `TimedKillComp { time: 0.0,
  lifetime: 0.0 }` — lifetime never set from the type.
- `sim/unit_runtime.rs::unit_update_system` (~line 612) dispatches only
  `AiKind::Player`, `AiKind::Command`, else ground/flying anchor logic. A
  missile (`AiKind::Missile`, mapped from `ControllerKind::Missile` in
  `ai/controller.rs:142`) would walk to the enemy core like a flyer.
- `WeaponDef.shoot_on_death: bool` exists (`content/registries/units/weapon.rs:610`)
  but nothing fires on death. `weapons/mod.rs:550` already has the
  `kill_shooter && total_shots > 0` gate.
- Unit kill entry point: `entities/comp/unit/lifecycle.rs:337`
  `kill_unit_with_abilities(world, content, tiles, rng, seq, entity)`, called by
  `Sim::resolve_deaths` (`sim/unit_runtime.rs:318`).

### 3.3 Changes (in order)
1. **TimedKill lifetime.** `entities/comp/unit/lifecycle.rs` ~line 278: set
   `TimedKillComp { time: 0.0, lifetime: unit.lifetime }` (`UnitTypeDef.lifetime`,
   `content/registries/units/mod.rs:1417`). The enclosing fn is
   `insert_kind_components(world, entity, unit: &UnitTypeDef, x, y, rotation)`
   (`lifecycle.rs:213`), so `unit` is the def.
2. **TimedKill tick.** In `sim/unit_runtime.rs::unit_update_system`, at the end
   of the per-unit loop body (after kinematics), add:
   ```rust
   // `TimedKillComp.update`: time = min(time + delta, lifetime); kill at lifetime.
   if let Some(mut timed) = world.get_mut::<TimedKillComp>(entity) {
       timed.time = (timed.time + 1.0).min(timed.lifetime);
       if timed.lifetime > 0.0 && timed.time >= timed.lifetime {
           if let Some(mut h) = world.get_mut::<Health>(entity) { h.health = 0.0; h.dead = true; }
       }
   }
   ```
   (Marking dead lets `resolve_deaths` run the normal kill path including
   shoot-on-death; do not despawn here.)
3. **Fix `ai/types/missile.rs`.** Rewrite `update_missile` to the Java
   `updateMovement` above. Add `pub shooter: Option<Entity>` to `MissileAi`.
   Signature stays `(ctx: &mut AiCtx, unit, state) -> bool`. Steps: read
   `time` from `TimedKillComp` (default `1_000_000.0`); if `time >=
   def.homing_delay` and shooter alive, set `UnitCore.rotation` toward the
   shooter's aim (`UnitWeapons.mounts[0].aim_x/aim_y` of the shooter, see
   `set_mount_targets` in `unit_runtime.rs` for the field names) using the
   unit's `rotate_speed` (`ctx.face_target` exists in `ai/ai_controller.rs`);
   speed = `def.speed` scaled by `min(time/missile_accel_time,1)^2` when
   `missile_accel_time > 0`; set `Vel` to `(cos(rot)*speed, sin(rot)*speed)`
   and integrate is done by kinematics/approach — check how `move_direct`
   (`ai_controller.rs:252`) writes `Pos`/`Vel` and mirror it exactly. Then:
   tile under unit (`WorldGrid::to_tile(pos)`), `grid.tiles.getn(tx,ty)?.build`
   → if building team != unit team and `def.target_ground` → mark the missile
   dead (`Health.dead = true`, `health = 0`). Delete the old "time <= 0 → kill"
   branch. Update the existing test `missile_homes_toward_target` accordingly.
4. **Dispatch MissileAI live.** In `unit_update_system`, add a branch before the
   generic `else`: `else if slot.kind == AiKind::Missile { ... }` that builds an
   `AiCtx` (see `ai/ai_controller.rs:30-41` fields: `world, grid, content,
   pathfinder, team`) from `runtime` and calls `update_missile`. Store the
   per-unit `MissileAi` state as a component: add
   `#[derive(Component, Debug, Clone, Copy, Default)] pub struct MissileShooter(pub Option<Entity>);`
   in `ai/types/missile.rs` and read/write it around the call.
5. **Spawn unit instead of bullet.** In `combat/bullet/spawn.rs::create`, right
   after the `create_chance` early-return and before building the bullet:
   ```rust
   if let Some(unit_id) = def.spawn_unit {
       let Some(udef) = content.unit(unit_id) else { return None; };
       let entity = crate::entities::comp::unit::lifecycle::spawn_unit_def(
           world, seq, udef, spawn.team, spawn.x, spawn.y, angle);
       if udef.missile_accel_time <= 0.0 {
           if let Some(mut vel) = world.get_mut::<crate::entities::comp::Vel>(entity) {
               let r = angle.to_radians();
               vel.x = r.cos() * udef.speed; vel.y = r.sin() * udef.speed;
           }
       }
       if let Some(owner) = spawn.owner {
           world.entity_mut(entity).insert(crate::ai::types::missile::MissileShooter(Some(owner)));
       }
       // killShooter: mark the owner dead (resolve_deaths kills it).
       if def.kill_shooter && let Some(owner) = spawn.owner
           && let Some(mut h) = world.get_mut::<crate::entities::comp::Health>(owner) {
           h.health = 0.0; h.dead = true;
       }
       world.entity_mut(entity).insert(SpawnedByBullet);
       return None;
   }
   ```
   Check `spawn_unit_def`'s exact signature in `entities/comp/unit/lifecycle.rs`
   (it is `spawn_unit_def(world, seq, def, team, x, y, rotation) -> Entity`, used
   by `UnitRuntime::spawn_by_id`). The `seq` passed to `create` was already
   consumed by the caller (`CombatCtx::spawn` increments it before calling), so
   the unit takes that seq — fine and deterministic.
   `SpawnedByBullet` is a new marker `Component` (define it in
   `combat/bullet/spawn.rs`) so the runtime can adopt the unit.
6. **Adopt bullet-spawned units into `runtime.units`.** After each combat pass
   that can spawn bullets (`unit_update_system` weapons block,
   `turret_update_system`, `bullet_update_system`, `bullet_collide_system`) call
   a new helper in `sim/unit_runtime.rs`:
   ```rust
   fn adopt_spawned_units(world: &mut World, runtime: &mut UnitRuntime) {
       let mut found: Vec<(u64, Entity)> = world.iter_entities()
           .filter(|e| e.contains::<crate::combat::bullet::spawn::SpawnedByBullet>())
           .map(|e| (e.get::<EntitySeq>().map_or(u64::MAX, |s| s.0), e.id())).collect();
       found.sort_unstable_by_key(|(s, e)| (*s, e.index()));
       for (_, e) in found { world.entity_mut(e).remove::<crate::combat::bullet::spawn::SpawnedByBullet>();
           runtime.units.push(e); runtime.units_created += 1; }
   }
   ```
   Call it after `world.insert_resource(audio)` and before
   `world.insert_non_send(runtime)` in each of the four systems. Also note
   `ctx.spawned` will not contain the unit (create returned `None`) — correct.
7. **despawnUnit.** In `combat/bullet/mod.rs::despawn_bullet` (~line 600, the
   `if despawn_hit { ... }` branch) add the `else` branch calling a new
   `create_units(ctx, bullet, x, y)`, and call `create_units` from
   `hit_bullet` (~line 545, where the comment `createPuddles/createIncend/
   createUnits/suppression` sits). Port `BulletType.createUnits` 638-647 using
   `def.despawn_unit`, `despawn_unit_count`, `despawn_unit_chance`,
   `despawn_unit_radius`, `face_outwards` — check these field names exist in
   `BulletDef` (`grep -n "despawn_unit\|face_outwards" content/registries/bullets.rs`);
   add any missing ones to `BulletDef` with Java defaults (count 1, chance 1,
   radius 0.1, faceOutwards true) and to `BulletDef::new` defaults and the
   spec→def copy in `bullets.rs`. Spawn via `spawn_unit_def` + the
   `SpawnedByBullet` marker; random offset `rng.range(RngStream::Sim, 0, radius)`
   along a random angle `rng.range(RngStream::Sim, 0, 360)`.
8. **shootOnDeath.** In `entities/comp/unit/lifecycle.rs::kill_unit_with_abilities`
   is the wrong layer (no CombatCtx). Instead, in `Sim::resolve_deaths`
   (`sim/unit_runtime.rs:318`), *before* calling `kill_unit_with_abilities` for
   a dead unit, run its shoot-on-death mounts:
   - build a `CombatCtx` exactly like `turret_update_system` does;
   - `sync_weapon_state(world, entity)` (`lifecycle.rs`), then
     `let state = world.get::<crate::weapons::UnitState>(entity).copied().unwrap_or_default();`
     and take the component: `let Some(mut w) = world.entity_mut(entity).take::<crate::weapons::UnitWeapons>()`.
     `UnitWeapons` (`weapons/mod.rs:68`) = `{ weapons: Vec<WeaponDef>, mounts:
     Vec<WeaponMount>, pending: Vec<PendingShot> }` (same indices).
   - for each index `i` where `w.weapons[i].shoot_on_death &&
     !(w.weapons[i].bullet.kill_shooter && w.mounts[i].total_shots > 0)`:
     `w.mounts[i].reload = 0.0; w.mounts[i].shoot = true;` then
     `crate::weapons::update_weapon(&mut ctx, entity, &state, &w.weapons, &mut w.mounts, &mut w.pending, i)`
     (`weapons/mod.rs:287`; destructure `w` into three locals first to satisfy
     the borrow checker); put the component back
     (`world.entity_mut(entity).insert(w)`). Check the mount fields
     (`reload`, `shoot`, `total_shots`) in `weapons/mount.rs`.
   - push spawned bullets into `runtime.bullets`.
   Keep the per-death order sorted by `EntitySeq` (it already iterates
   `runtime.units` in spawn order — keep that).
9. **Scathe missiles (optional slice, do after 1-8 pass).** Add unit specs for
   `scathe-missile`, `scathe-missile-phase`, `scathe-missile-surge`,
   `scathe-missile-surge-split` to `content/registries/units/erekir.rs`
   (copy the `anthicus-missile` spec shape at `erekir.rs:1616`; values from
   `Blocks.java:5257-5530`: speed, lifetime, weapons with `shootOnDeath = true`
   and their `ExplosionBulletType` damage/radius). Then in `advanced.rs`
   register `scathe_carbide` etc. as `BulletKind::Basic` with `speed = 0`,
   `spawn_unit = content.unit_id("scathe-missile")`. **Pitfall:** adding unit
   types changes unit ids → `parity/golden_content.json` unit entries and
   unit-count tests (`content::parity::tests::names_ids_match_golden`) will fail;
   Java's order is the authority: these units are created inside `Blocks.java`
   (blocks load before units in `ContentLoader`?) — check
   `Mindustry/core/ContentLoader.java` order and the golden's unit list
   (`jq '.types[] | select(.type=="unit") | .entries[].name' parity/golden_content.json | grep missile`)
   before deciding where to insert. If the golden already lists them, match its
   ids exactly; if not, stop this sub-step and record it as remaining.

### 3.4 Tests (add to `mind-core/src/sim/combat_tests.rs`)
- `spawn_unit_bullet_spawns_missile`: `live_sim(64)`; find an anthicus weapon
  bullet id: `content.unit_id("anthicus")` → `content.unit(id).weapons` →
  first weapon with `bullet.spawn_unit.is_some()` (read
  `content/registries/units/weapon.rs` for the field holding the bullet id).
  Call `crate::combat::bullet::spawn::create` through a `CombatCtx` (build it
  like `turret_update_system`) with `BulletSpawn { def, x, y, angle: 0, team: 2,
  ..Default::default() }`; assert it returns `None` and that one entity with
  `UnitTypeComp` of `anthicus-missile` exists, `Vel.x > 0`.
- `missile_dies_at_lifetime_and_explodes`: spawn an anthicus (team 2) at
  (100,100) via `sim.spawn_live_unit("anthicus", 2, ...)`, place a team-1
  `copper-wall` at tile (20,12) (world ~(164,100)); tick ≤ 600; assert the wall
  is gone (`sim.grid.tiles.get(20,12).build.is_none()`) or its health dropped,
  and that at some tick a unit named `anthicus-missile` existed (record in loop).
- `timed_kill_counts_up`: spawn a missile type directly with
  `spawn_live_unit("anthicus-missile", 1, ...)`, tick `lifetime as usize + 2`
  times, assert entity gone.
- Update `ai/types/missile.rs` test `missile_homes_toward_target` for the new
  semantics.

### 3.5 Verify
```bash
cd client/rust && CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib combat_tests missile -- --nocapture
CARGO_BUILD_JOBS=6 cargo test -p mind-core && cargo fmt -p mind-core && CARGO_BUILD_JOBS=6 cargo clippy -p mind-core --all-targets -- -D warnings
```
In-engine (Serpulo sandbox is fine, Erekir content need not be unlocked for
`spawn_unit`): after `tools/build.sh` + `tools/session-play.sh 2` + Ground Zero
launch, spawn an anthicus (team 2) near the duo: index =
`$G --eval 'return get_node("/root/Spine/SimHost").content_list("unit").find("anthicus")'`,
then `apply_sim_command_json` `spawn_unit` at px 1160, py 396; poll
`get_group_count("unit")` every second for 10 s: expect count to rise by ≥1
(missiles) and buildings near (135,49) to disappear from
`JSON.parse_string(get_state_json()).world.tiles.filter(func(t): return t.x == 135 and t.y == 49)`.

### 3.6 Pitfalls
- `CombatCtx::spawn` (`combat/bullet/mod.rs:192`) calls `apply_kind_init` on the
  returned entity — it returns `None` for spawn-unit bullets so nothing runs; OK.
- Missile units are `flying`; `collide_units` gates by `collides_air`. Missiles
  are `hittable`? Java `MissileUnitType` doesn't set `hittable=false`; keep data.
- `resolve_deaths` already adopts `SpawnDeathAbility` spawns by seq range; keep
  that logic working when you add shoot-on-death (bullets, not units).
- Determinism: no `HashMap`; sort adopted units by seq.

**Status 2026-10-12:** done — commit 1019d4f. `spawnUnit` bullets spawn the
missile (`BulletType.create` branch + `SpawnedByBullet` adoption), `TimedKillComp`
counts up to the type lifetime, live `AiKind::Missile` dispatch steers at the
shooter's aim and dies on enemy building contact, and `resolve_deaths` runs
`shootOnDeath` before removal. `ExplosionBulletType` defaults added (golden
regenerated for bullets 93/105/107, matching the JVM golden); `createPuddles` /
`createIncend` / `createUnits` wired into `hit`/`despawned`. Tests:
`spawn_unit_bullet_spawns_missile`, `timed_kill_counts_up`,
`missile_dies_at_lifetime_and_explodes`, updated `missile_homes_toward_shooter_aim`.
In-engine evidence in `runs/20261012-session2/PAR-0088-ingame.txt`.
Scathe missile unit specs (step 9, optional) not done — section 11 blocker note
still applies.

---

## 4. NEW-1 — live unit statuses and abilities never update

**Symptom (found 2026-10-12):** the live `unit_update_system` never calls
`combat::damage::status::update_statuses` or the ability update, and the status
applier is never installed, so `burning`, `wet`, `slowed`, shields from
abilities, regen etc. never apply/tick in-game (only in harnesses).
**Acceptance:** a unit hit by a bullet with `status = burning` takes burning
damage over time; `apply_status` works in the live sim; a `nova`/`poly`
ability (e.g. `RepairFieldAbility`) heals allies live.

### 4.1 Java reference
- `Mindustry/entities/comp/StatusComp.java` `update()` (status timers, damage,
  speed/reload/damage multipliers).
- `Mindustry/entities/comp/UnitComp.java` `update()` → `for(Ability a :
  abilities) a.update(self())` (search `abilities` in `UnitComp.update`).

### 4.2 Current port state
- `combat/damage/status.rs:180` `install_status_applier(world, content)`;
  `:190` `apply_status`; `:206` `update_statuses(world, content)`.
  Only harness tests call them.
- Abilities: `combat/abilities.rs` `update_unit_abilities(world, content, unit)`
  (used in its tests ~line 1061).

### 4.3 Changes
1. `sim/unit_runtime.rs::Sim::install_unit_runtime_from` (~line 432): after
   inserting the runtime, call
   `crate::combat::damage::status::install_status_applier(&mut self.ecs.0, &content)`
   — content is moved into `UnitRuntime::new`; so install first using `&content`
   then build the runtime.
2. `unit_update_system`: before the per-unit movement loop, call
   `crate::combat::damage::status::update_statuses(world, &runtime.content);`
   then inside the loop (after kinematics) call
   `crate::combat::abilities::update_unit_abilities(world, &runtime.content, entity);`
   (signature verified: `(world: &mut World, content: &ContentRegistry, entity: Entity)`,
   `combat/abilities.rs:211`). Abilities that spawn units/puddles
   (`combat/abilities.rs:933, 978`) use their own paths; after the loop, adopt
   any new `UnitTypeComp` entities not in `runtime.units` (same seq-range trick
   used in `Sim::resolve_deaths`).
3. Unit death from status damage is handled by `resolve_deaths` already.

### 4.4 Tests (`sim/combat_tests.rs`)
- `live_status_burning_damages_unit`: live sim, spawn dagger, call
  `apply_status(&mut sim.ecs.0, dagger, content.status_id("burning"), 300.0)`,
  tick 120, assert health dropped.
- `live_ability_repair_field_heals`: spawn `poly` (team 1) and a damaged dagger
  (set `Health.health = 50`) 16 px apart, tick 400, assert health > 50.

### 4.5 Pitfalls
- Golden/checksum tests that install the unit runtime may change if statuses
  now tick (e.g. `sim/unit_runtime.rs` determinism tests compare two runs —
  still equal). If a committed golden in `mind-headless` changes, check whether
  it is one of the 6 pre-existing failures first.

**Status 2026-10-12:** done — applier install in `1019d4f`, tests in `88e0cf8`.
`install_unit_runtime_from` installs the content-backed `StatusApplier`; the
live loop ticks `update_unit_status` + `update_unit_abilities` per unit via
`update_kinematics` (already present on main). Tests:
`live_status_burning_damages_unit`, `live_ability_repair_field_heals`.
Vanilla live-update abilities spawn units only through `SpawnDeathAbility`,
which `resolve_deaths` already adopts by seq range.

---

## 5. PAR-0137 — ground units ignore floor speed multipliers

**Symptom:** units cross deep water / sand-water / darksand-water / tar at full
speed. **Acceptance:** a dagger walking over `deep-water` (speedMultiplier 0.2)
moves ≤ 0.25× its stone speed per tick; flying units unaffected; status speed
multiplier still applies.

### 5.1 Java reference
- `Mindustry/entities/comp/UnitComp.java:107-110`:
  ```java
  public float floorSpeedMultiplier(){
      Floor on = isFlying() || type.hovering ? Blocks.air.asFloor() : floorOn();
      return (float)Math.pow(on.speedMultiplier, type.floorMultiplier) * speedMultiplier;
  }
  ```
- `UnitComp.java:190-194` `speed()`:
  `strafePenalty = isGrounded() || !isPlayer() ? 1 : lerp(1, type.strafePenalty, angleDist(vel.angle(), rotation)/180)`;
  `boost = lerp(1, type.canBoost ? type.boostMultiplier : 1, elevation)`;
  `return type.speed * strafePenalty * boost * floorSpeedMultiplier();`
- Overrides: `CrawlComp.java:39` (`on.isDeep() ? 0.45 : on.speedMultiplier`,
  × `lastCrawlSlowdown`), `TankComp.java:120` (× `lastSlowdown`).

### 5.2 Current port state
- Helper exists unused: `mind-core/src/world/behavior/environment.rs:92`
  `floor_speed_multiplier(table: &BlockTable, block) -> f32`. Also
  `content.block(floor_id).speed_multiplier` (`BlockDef.speed_multiplier`,
  `content/registries/blocks/mod.rs:2110`) works with just the registry.
- Speed computed without floor in four places:
  `ai/types/ground.rs:~113-121` (`update_ground`),
  `ai/types/flying.rs:~52-58`, `ai/types/command.rs:~336-342`,
  `ai/types/player.rs:~74-80` (comment says floor/strafe not modeled), and
  `ai/ai_controller.rs:54` `AiCtx::pref_speed` (used by `move_direct` and
  `move_to`-style helpers at lines ~191 and ~253).
- `UnitTypeDef.floor_multiplier` (`units/mod.rs:1015`), `hovering` (`:1175`),
  `strafe_penalty` (`:1065`).

### 5.3 Changes
1. New fn in `mind-core/src/entities/comp/unit/movement.rs`:
   ```rust
   /// `UnitComp.speed()` (UnitComp.java:190) with `floorSpeedMultiplier` (:107).
   pub fn unit_speed(world: &World, grid: &WorldGrid, content: &ContentRegistry, unit: Entity) -> f32
   ```
   - base = `PhysicsComp.speed`; status = `StatusComp.speed_multiplier` (1.0 default);
   - flying = `crate::combat::bullet::unit_is_flying(world, unit)`;
   - `def = UnitTypeComp → content.unit(type_id)`;
   - floor mult = 1.0 if flying or `def.hovering`; else
     `content.block(tile.floor).map_or(1.0, |b| b.speed_multiplier).powf(def.floor_multiplier)`
     where `tile = grid.tiles.getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y))`
     (check the floor field name on `Tile`: `grep -n "pub floor" mind-core/src/world/tile.rs`);
     for units with `CrawlComp` use 0.45 when the floor `is_deep` (via
     `environment::floor_is_deep` with the `BlockTable` resource, or a
     `BlockDef` deep flag — check `grep -n "is_deep" content/registries/blocks/mod.rs`);
   - boost = `lerp(1, if physics.can_boost {physics.boost_multiplier} else {1}, UnitCore.elevation)`;
   - strafe = 1 unless the unit is `AiKind::Player` and not grounded;
   - return `base * strafe * boost * floor * status`.
2. Replace the inline speed computations in the 4 AI files and in
   `AiCtx::pref_speed` with `unit_speed(...)`. `update_ground` has `grid`
   but no content — add a `content: &ContentRegistry` parameter and update its
   caller in `unit_runtime.rs` (`update_ground(world, grid, pathfinder, team,
   entity, target)` → pass `&runtime.content`; destructure `content` alongside
   `grid, pathfinder`). Same for `update_flying` / `update_command_move` if
   needed; update every call site (`grep -rn "update_ground(\|update_flying(\|update_command_move(" mind-core/src`).
   `AiCtx` already has `grid` and `content`.

### 5.4 Tests
- `sim/combat_tests.rs::dagger_slowed_on_deep_water`: `Sim::new(1, 40, 40,
  floor, AIR)` with `floor = content.block_id("deep-water")` — note
  `Sim::new(seed, w, h, floor: BlockId, wall: BlockId)`; for a stone control,
  `stone`. Spawn a team-2 dagger, give it a `ControllerSlot.target` 20 tiles
  away (set via `crate::entities::comp::unit::lifecycle::set_move_target`), tick
  60, measure displacement; assert `water_disp < 0.3 * stone_disp`.
  Deep water may be impassable to ground pathing; if the dagger cannot move at
  all, use `sand-water` (speedMultiplier 0.8, assert ratio ≈0.8±0.05) or
  `darksand-tainted-water` — read `content/registries/blocks/environment.rs`
  for a passable floor with multiplier < 1.

### 5.5 Pitfalls
- `PhysicsComp.speed` may already include something from spawn; read
  `lifecycle.rs` where `PhysicsComp` is built to make sure you don't double-apply.
- Ground pathing treats deep water as solid for most ground units; that is
  correct Java behavior.

**Status 2026-10-12:** done — commit c13ccb1. `movement::unit_speed` ports
`UnitComp.speed()`/`floorSpeedMultiplier()` plus the crawl/tank overrides and is
used by ground/flying/command/player AI and `AiCtx::pref_speed`. Test
`dagger_slowed_on_deep_water` (stone 30 px vs deep-water 6 px over 60 ticks;
flares unaffected by the floor).

---

## 6. PAR-0140 — RTS selection and orders do nothing in-engine

**Symptom:** holding Shift (command mode) and drag-selecting friendly units
selects nothing usable; right-click move orders are ignored.
**Acceptance (in-engine):** in Ground Zero, spawn a friendly dagger (team 1),
enter command mode, select it, right-click a world point; within 600 ticks the
dagger's position is within 2 tiles of the point.

### 6.1 Java reference
- `Mindustry/type/UnitType.java:283` default controller: player-team units get
  `CommandAI` (idle until ordered).
- `Mindustry/ai/types/CommandAI.java` (`updateUnit`/`moveTo`, ~line 278+:
  path to `targetPos`).
- `Mindustry/input/DesktopInput.java` command-mode branch: `commandMode` only
  when `player.block == null` etc.

### 6.2 Current port state (round 1 landed before this session)
- Core side works: `Sim::apply_live_unit_command` (`sim/mod.rs:383-466`) only
  handles the `move` command and requires the unit `AiKind::Command` and
  `team == live_default_team()`; `Sim::selectable_units_json`
  (`sim/unit_runtime.rs` ~1050-1090) publishes rows with `commandable`.
- Controller selection: `entities/comp/unit/lifecycle.rs:115-138`
  (`team_ai_flags`, `select_team_controller`) → `ai/controller_registry.rs:30-55`.
  Friendly units on the default team in a campaign get `AiKind::Command`.
- **Blocker found:** the command-mode gate
  `mind-gdext/src/input/desktop.rs:432` `command_mode_allowed()` requires
  `controller.state.block.is_none()`. At boot
  `mind-gdext/src/sim_host.rs:253-258` restores the last selected block from
  `user://settings.json` (`settings::read().selected_block`, written in
  `exit_tree` ~line 208) → `Sim::set_selected_block("duo")`, and
  `mind-gdext/src/input/mod.rs:221-239` `sync_selected_block()` copies it into
  the input state → `block = Some(duo)` → command mode is forced off. Java does
  not persist `player.block` across sessions.
- Ledger observation: an ordered dagger "keeps walking +y under GroundAI" →
  that unit was spawned with a non-Command AI (spawned via `spawn_unit` op, team
  given by JSON; if team ≠ default team it gets AI). Use `"team": 1` and make
  sure the campaign `rules.default_team` is 1.

### 6.3 Changes
1. `mind-gdext/src/sim_host.rs` ~253: remove the `settings::read()` →
   `set_selected_block` restore (keep writing settings if other code reads it;
   grep `settings::read` for other users). Comment: "`player.block` is not
   persisted upstream; a restored block closes the command-mode gate (PAR-0140)".
2. Verify `mind-gdext/src/input/mod.rs::sync_selected_block` then yields `None`
   (it maps `BlockId::AIR` to `None`; `Sim::selected_block` defaults to AIR at
   `sim/mod.rs:221`).
3. Extend `apply_live_unit_command` (`sim/mod.rs:383`) beyond `move`: read the
   command ids in `mind-core/src/content/registries/commands.rs` and handle at
   least `move` (exists) and the stance path if `SimCommand::UnitStance` is
   applied elsewhere (grep `UnitStance` in `sim/mod.rs`). Keep scope: move only
   is acceptable for this item if the in-engine chain works.
4. Command-mode drawing (selection overlay, Java `drawCommanded`) is the
   rendering agent's — do not implement.

### 6.4 Tests
- Existing core tests near `sim/unit_runtime.rs:1674` cover Command AI; add
  `sim/combat_tests.rs::commanded_dagger_reaches_target`: needs a campaign
  runtime for team flags — copy the setup from `sim/unit_runtime.rs` test that
  builds `CampaignRuntime` (~line 1745-1800: `PlaySession::new(Rules{...})`,
  `CampaignRuntime::new(session, campaign)`), spawn a team-1 dagger, assert
  `AiKind::Command`, issue `SimCommand::UnitCommand { units: smallvec![seq as i32],
  command: MOVE_COMMAND.raw(), x, y }` (see how `resolve_wire_target` decodes
  `x,y` in `sim/mod.rs`; read its doc), tick 600, assert distance < 16 px.

### 6.5 In-engine verify
```bash
$G --eval 'return get_node("/root/Spine/Input").get_input_state_json()'      # "block" must be null/absent
$G --eval 'return get_node("/root/Spine/SimHost").apply_sim_command_json("{\"op\":\"spawn_unit\",\"unit\":0,\"team\":1,\"px\":1000.0,\"py\":440.0}")'
$G --eval 'return get_node("/root/Spine/SimHost").selectable_units_json()'   # row with commandable:true, note "id"
$G --eval 'return get_node("/root/Spine/Input").toggle_command_mode()'       # true
$G --eval 'return get_node("/root/Spine/SimHost").apply_sim_command_json("{\"op\":\"unit_command\",\"units\":[<id>],\"x\":<...>,\"y\":<...>}")'
```
Read `mind-gdext/src/sim_host.rs` `"unit_command"` arm (~line 463) for the
exact JSON keys (`units`, `command`, `x`, `y`) and how x/y are encoded before
using it. Then poll the unit position from `get_state_json()` entities (or
`selectable_units_json()` x/y) every 2 s. For the real input chain use
`Input.push_event` with a drag (see `.opencode/skills/playtest/SKILL.md` /
`/home/clintonnguyen/g/code_examples/.claude/skills/playtest/SKILL.md` for the
event JSON format) and `selection_json()`.

**Status 2026-10-12:** done — commit fe71e74. The boot `settings.json` →
`set_selected_block` restore is gone (the file is still written on exit;
`player.block` is not persisted upstream). In-engine: with
`"selected_block":"duo"` on disk before boot, input state reports
`"block": null`, `toggle_command_mode()` is true, and an ordered dagger
reached within 8.5 px of the target. Test `commanded_dagger_reaches_target`
(campaign core + team-1 dagger + `SimCommand::UnitCommand`). Stances were left
out — `move` is the scope keeper per §6.3 step 3. Evidence:
`runs/20261012-session2/PAR-0140-ingame.txt`.

---

## 7. Wave attack / core loss verification (PAR-0139-adjacent)

**Acceptance:** In Ground Zero, a wave (via `request_wave_spawn`) of daggers
walks to the player core, shoots it, buildings in their way take damage and
are destroyed (now possible since `resolve_deaths`), and if the core-shard
dies the campaign reports `GameOver`/sector lost.

### 7.1 Port state
- Units pick `acquire_target` (`sim/unit_runtime.rs` ~1150) → nearest enemy
  building anchor + in-range mount target; weapons fire; bullets hit buildings
  via `collide_bullet`; deaths via `resolve_deaths`.
- Core loss: `game/runtime.rs` campaign tick sweeps dead core entities out of
  team data (~line 375-399, "liveness sweep", PAR-0084), then
  `game_state_tick_system` (`game/runtime.rs:641`) → `game/play.rs:498
  check_game_state` → `:510` `player_core_count() == 0` → `PlayEvent::GameOver`.

### 7.2 Changes
Probably none; this is a verification item. Add a test, then fix whatever breaks.

### 7.3 Test (`sim/unit_runtime.rs::msav_tests`, feature `msav-import`)
`ground_zero_wave_destroys_undefended_core`: load Ground Zero exactly like
`ground_zero_drill_fed_duo_kills_wave_daggers` (same file), install a
`CampaignRuntime` (see the attack-mode test in the same file, ~line 1745, for
`PlaySession`/`Campaign::from_registry`/`register_core`), set core health low
(`Health.health = 50` on the core entity at tile (129,55)) to keep the test
short, spawn 3 team-2 daggers 10 tiles from the core, tick ≤ 3000, assert
the core tile has no building and the `CampaignRuntime.session.game_over` is
true (or a `GameOver` event was produced — read `game/play.rs:498-560` for the
field that records it).

### 7.4 In-engine
Ground Zero launch → `$G --eval 'return get_node("/root/Spine/MindCampaign")...'`
— find the wave-skip/next-wave func by `grep -n "#\[func\]" -A1
client/rust/mind-gdext/src/campaign.rs | grep "fn "` (e.g. `run_wave`/`skip_wave`).
Poll `get_group_count("unit")` and core tiles. Expect the core to take damage
(no direct health getter: add a debug-only `#[func] fn building_health(x,y) -> f64`
to `MindSimHost` if needed — debug reads are allowed in gdext).

---

## 8. PAR-0091 — fires and puddles never update live

**Symptom:** puddles from liquid bullets (e.g. atrax) never evaporate, spread,
react or apply status; fires never burn/spread.
**Acceptance:** a deposited water puddle shrinks to nothing over time on a dry
floor; a unit standing in a puddle gets its status (`wet` for water); a fire on
a tile damages a building there and burns out; `incendChance` bullets create fires;
`puddleLiquid` bullets deposit puddles; destroyed liquid containers splash.

### 8.1 Java reference
- `Mindustry/entities/comp/FireComp.java:40` `update()`; `Mindustry/entities/Fires.java` `create`.
- `Mindustry/entities/comp/PuddleComp.java:52` `update()`; `Mindustry/entities/Puddles.java` `deposit`.
- `Mindustry/entities/bullet/BulletType.java:562-577` `hit` → `createPuddles`
  (`:597-604`), `createIncend` (`:591-595`) → `Damage.createIncend`
  (`Mindustry/entities/Damage.java:135-144`).
- `Mindustry/entities/comp/BuildingComp.java:1467-1477` `splashLiquid`
  (amount/4 clamp 0..10, `clamp(amount/5, 0, 30)` deposits staggered by
  `Time.run(i/2)` — run them immediately in the port), called from
  `onDestroyed` `:1490-1514` (`liquids.each(this::splashLiquid)` when
  `rules.damageExplosions`), and `Damage.dynamicExplosion` (`:1517`).

### 8.2 Port state
- `combat/fires.rs:146` `update_fires(world, tiles: &mut Tiles, content, rng, fx, water_attr)`,
  `:90` `create(world, tx, ty, rng)`.
- `combat/puddles.rs:236` `update_puddles(world, tiles: &mut Tiles, content, rng, fx)`,
  `:98` `deposit(world, tiles: &Tiles, content, tx, ty, liquid, amount, rng)`.
- Both need `&mut Tiles` — the live grid is `Sim.grid` (not in the World).
  Only `CombatHarness::fire_tick/puddle_tick` (`combat/harness.rs:180-200`) call them.
- `combat/bullet/mod.rs` `hit_bullet` (~line 545-580) has the comment
  `createPuddles/createIncend/createUnits/suppression: M3/11.` — not ported.
  `BulletDef` has `puddles, puddle_range, puddle_amount, puddle_liquid`
  (`bullets.rs:397-403`) and `make_fire, incend_amount, incend_spread,
  incend_chance` (`:559-565`).
- Liquid bullets already deposit (`combat/bullet/kinds/liquid.rs:45`), fire
  bullets create fires (`kinds/fire.rs:50`), but they never update.

### 8.3 Changes
1. `Sim::update_fires_and_puddles(&mut self)` in `sim/unit_runtime.rs` `impl Sim`:
   ```rust
   let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else { return; };
   let fx = runtime.fx.clone();
   crate::combat::fires::update_fires(&mut self.ecs.0, &mut self.grid.tiles, &runtime.content,
       &mut runtime.rng, fx.as_ref(), /* water_attr */ 0.0);
   crate::combat::puddles::update_puddles(&mut self.ecs.0, &mut self.grid.tiles, &runtime.content,
       &mut runtime.rng, fx.as_ref());
   self.ecs.0.insert_non_send(runtime);
   ```
   (`FxHandle = Arc<dyn FxSink>` (`combat/view.rs:84`), so `.clone()` is cheap.) `water_attr`: Java passes
   `state.envAttrs.get(Attribute.water)`; use 0.0 until weather lands (§10),
   then the sum of active weather `attrs`.
   Call it in `Sim::tick` right after `self.schedule.run(...)` and before
   `resolve_deaths()` (Java: fires/puddles are entity groups updated in
   `updateEntities`, before damage resolution matters).
   Fires/puddles damage buildings/units through `apply_health` → deaths picked
   up by `resolve_deaths` the same tick. Fire tile slots live in
   `self.grid.tiles`; after the update, the unit runtime snapshot is stale for
   fire slots only — acceptable (pathing ignores fires).
2. In `hit_bullet` (`combat/bullet/mod.rs` ~545) replace the TODO comment with:
   `create_puddles(ctx, bullet, x, y); create_incend(ctx, bullet, x, y);`
   (Java order: frags, puddles, incend, splash... check lines 562-580 and match
   the order exactly). Implement:
   ```rust
   fn create_puddles(ctx, bullet, x, y) // for i in 0..def.puddles: tile = toTile(x + rng.range(-r,r)), toTile(y + rng.range(-r,r)); puddles::deposit(ctx.world, &ctx.grid.tiles, ctx.content, tx, ty, def.puddle_liquid, def.puddle_amount, ctx.rng)
   fn create_incend(ctx, bullet, x, y)  // if incend_chance > 0 && rng.chance(incend_chance): for i in 0..incend_amount: fires::create(world, toTile(x+range(spread)), toTile(y+range(spread)), rng)
   ```
   `Mathf.range(r)` = uniform in `[-r, r]`. Guard `puddle_liquid` against the
   "none" id: check how `LiquidId` none is represented in `BulletDef::new`.
   **Pitfall:** `ctx.grid` is the runtime *snapshot*; `deposit` takes
   `&Tiles` (read-only) — fine.
3. Destruction splash: in `SimRuntime::destroy_building` (`sim/runtime.rs:359`)
   before `remove_building`, if the building has a `LiquidModule` and
   `BuildRules`/rules `damage_explosions` (find the flag: `grep -rn
   "damage_explosions" mind-core/src/game`; default true), collect
   `(liquid, amount)` pairs and after removal call a new
   `Sim`-level splash (needs `UnitRuntime` content + rng): simplest is to return
   the spill list from `destroy_building` and have `resolve_deaths` deposit via
   `puddles::deposit` using `self.grid.tiles` and the unit runtime's
   content/rng: for each liquid `splash = (amount/4).clamp(0,10)`, count
   `(amount/5).clamp(0,30) as i32`, each at
   `x + rng.range(-size*4, size*4)`, `y + ...` (size*tilesize/2 = size*4).
   Also call `Damage.dynamicExplosion` equivalent if
   `combat/damage/explosion.rs:23 dynamic_explosion` fits (read its signature);
   flammability/explosiveness from items/liquids per `BuildingComp.java:1491-1508`.
   This step is optional for PAR-0091; mark remaining if skipped.

### 8.4 Tests (`sim/combat_tests.rs`)
- `live_puddle_evaporates`: `live_sim(16)`, deposit water at (5,5) amount 20
  with `crate::combat::puddles::deposit(&mut sim.ecs.0, &sim.grid.tiles, &content, 5, 5, water, 20.0, &mut rng)`
  — borrow pattern: clone what you need or take the runtime out
  (`sim.ecs.0.remove_non_send::<UnitRuntime>()`) and put it back. Tick 2000;
  assert `puddles::find_at(&sim.ecs.0, 5, 5).is_none()` or amount decreased.
- `live_fire_burns_building`: place a team-1 `copper-wall` at (5,5), call
  `fires::create(&mut sim.ecs.0, 5, 5, &mut rng)`, tick 300, assert wall health < max.
- `incend_bullet_creates_fire`: a bullet def with `incend_chance = 1,
  incend_amount = 1, incend_spread = 0` hitting at (5,5) leaves a fire there
  (use `fires::has(&world, 5, 5)` right after the hit).

---

## 9. PAR-0090 — save/load drops fires and puddles

**Acceptance:** save with a live fire and puddle, load, both exist with the
same tile/amount/time.

### 9.1 Port state
- Codecs exist: `combat/fires.rs:267/279` `write_fire`/`read_fire`
  (`FIRE_REVISION = 1`), `combat/puddles.rs:66/78` `write_puddle`/`read_puddle`
  (`PUDDLE_REVISION = 1`); they take `BuildingWriter`/`BuildingReader`
  (`crate::world::BuildingWriter`) — check whether those wrap a `WireWriter`
  (`grep -n "pub struct BuildingWriter" -A10 mind-core/src/world/*.rs`).
- Live save path: `mind-core/src/io/sim_io.rs` — `SIM_UNIT_CLASS_ID = 0xF0`
  (`:50`), `SIM_UNIT_REVISION` (`:53`), `LiveEntitySink` (`:100-145`,
  `supports_class`, `read_entity`, `after_read_all`), `LiveEntitySource`
  (`entity_count` `:180` counts units only; `write_entities` `:188-237`).
  The v1 writer writes `entities.entity_count()` then calls `write_entities`
  (`io/save/versions/v1.rs:844-877`); each entity is one chunk
  (`scratch.write_chunk`) whose first byte is the class id and then an `i32` id.

### 9.2 Changes
1. `io/sim_io.rs`: add `pub const SIM_FIRE_CLASS_ID: u8 = 0xF1;` and
   `pub const SIM_PUDDLE_CLASS_ID: u8 = 0xF2;` (must stay clear of upstream
   ids; extend the test `live_unit_class_id_is_reserved` at `:695` to assert
   both are unmapped).
2. `LiveEntitySource::entity_count`: units + count of `FireState` + count of
   `PuddleState` entities.
3. `write_entities`: after units, iterate fires then puddles sorted by
   `EntitySeq` (fires/puddles may lack `EntitySeq` — check `fires::create`; if
   absent, sort by `(tile.0, tile.1)`), each as a chunk:
   `ub(class) i(seq or 0) us(REVISION) + codec payload`.
4. `LiveEntitySink`: `supports_class` accepts the 3 ids; `read_entity` decodes
   into `fires: Vec<FireState>` / `puddles: Vec<PuddleState>`.
5. Wherever decoded units are materialized after load (find with
   `grep -n "\.units" mind-core/src/io/sim_io.rs` — the consumer of
   `LiveEntitySink.units`), also spawn fire/puddle entities and register their
   tile slots (look at how `fires::create` / `puddles::deposit` register slots
   in `Tiles`; `update_fires`/`update_puddles` re-register every tick via
   `clear_fire_slots`, so just spawning the component is enough).

### 9.3 Test
`io/sim_io.rs` tests: build a live sim, create a fire + deposit a puddle, save
via the same path the existing save/load round-trip test in `sim_io.rs` uses
(find with `grep -n "fn .*round" mind-core/src/io/sim_io.rs`), load into a
fresh sim, assert one `FireState` and one `PuddleState` with equal fields.

---

## 10. PAR-0127 — weather never happens

**Acceptance:** with `rules.weather = [rain entry]`, a `WeatherRuntime` entity
appears after its cooldown, ground units get `wet`, it expires after its
duration, and the cooldown re-arms. Rendering of weather is not ours.

### 10.1 Java reference
- `Mindustry/core/Logic.java:90-99` (`PlayEvent`: shuffle `rules.weather`,
  `cooldown = sum + random(maxFrequency)`), `:376-391` `updateWeather()`,
  called at `:548-550` (serverside, before teams/waves), env attrs `:603-606`.
- `Mindustry/type/Weather.java:258-260` `createWeather`; `WeatherEntry`
  `:262-...` (`minFrequency, maxFrequency, minDuration, maxDuration, cooldown,
  intensity, always`).

### 10.2 Port state
- `mind-core/src/entities/comp/weather.rs`: `WeatherRuntime` component (`:36`),
  `apply_weather_status` (`:71`), `update_weather_state(world, content, entity)`
  (`:118`; decrements life, applies status, despawns). Needs the status applier
  (§4). No scheduler: nothing spawns `WeatherRuntime` live.
- Rules: `io/json/rules.rs:632` `pub weather: Vec<WeatherEntry>` (`:429-446`),
  reachable as `CampaignRuntime.session.rules.weather` (`game/runtime.rs`).

### 10.3 Changes
1. New system `weather_system(world: &mut World)` in `game/runtime.rs` next to
   `team_ai_system` (`:740`), registered in `sim/schedule.rs` in
   `TickSet::WeatherAndAi` **before** `team_ai_system`
   (`.before(crate::game::runtime::team_ai_system)`). Body:
   - take `CampaignRuntime` (`world.remove_resource`), return if not
     `State::Playing` or `is_client` (mirror `team_ai_system`'s guard);
   - for each entry (index order): resolve `content.weather_id(&entry.weather)`
     — content: use the `UnitRuntime` non-send (`world.get_non_send::<UnitRuntime>()`)
     registry; skip unknown names (Java `removeAll(w -> w.weather == null)`);
   - `entry.cooldown -= 1.0`;
   - active = any `WeatherRuntime` entity with that weather id;
   - if `(entry.cooldown < 0 || entry.always) && !active`: `duration = if always
     {f32::INFINITY} else {rng.range(min_duration, max_duration)}`;
     `entry.cooldown = duration + rng.range(min_frequency, max_frequency)`;
     spawn `(WeatherRuntime::new(id, entry.intensity, duration), crate::entities::comp::WeatherState, EntitySeq(next))`
     with wind = random unit vector (`angle = rng.range(0,360)`).
   - then for every `WeatherRuntime` entity (sorted by seq) call
     `update_weather_state(world, content, e)`.
   - RNG: use a deterministic stream — `UnitRuntime.rng` (take the non-send
     resource out and put it back) with `RngStream::Sim`.
2. On sector play (Java `PlayEvent`): when the campaign session starts playing
   (find where `PlayEvent`/`State::Playing` is entered in `game/runtime.rs` or
   `game/play.rs`), shuffle-less port: `sum = 0; for entry: entry.cooldown = sum
   + rng.range(0, max_frequency); sum += entry.cooldown`. (Java shuffles first —
   use a Fisher-Yates with the same rng to stay deterministic.)
3. Water attribute for fires (§8): sum `def.attrs[water] * opacity` over
   active weathers; expose as a helper and pass to `update_fires`.

### 10.4 Tests (`game/runtime.rs` tests or `sim/combat_tests.rs`)
`weather_rain_spawns_and_wets_units`: live sim + `CampaignRuntime` with
`rules.weather = vec![WeatherEntry{ weather: "rain".into(), always: true,
intensity: 1.0, ..Default::default() }]`, spawn a dagger, tick 10, assert one
`WeatherRuntime` exists and the dagger has the `wet` status
(`StatusComp::has_effect_of`, see `combat/abilities.rs` tests).

### 10.5 In-engine
Weather sectors (Frozen Forest) need a fresh profile; just verify with
`get_group_counts()["weather"]` after editing rules is not possible from UI —
rely on the unit test. Report the in-engine check as blocked by profile.

---

## 11. PAR-0138 — payload carrier pickup/drop is a no-op

**Symptom:** pressing `[` / `]` with a payload-capable unit (mega, quell,
oct, Erekir carriers) logs a rejected command; nothing is carried.
**Acceptance:** player-controlled `mega` (Serpulo, payloadCapacity 2x2 blocks)
picks up an adjacent friendly dagger and drops it elsewhere; can pick up a
1x1/2x2 friendly building and place it back on drop.

### 11.1 Java reference
- `Mindustry/entities/comp/PayloadComp.java:74-140`: `payloadUsed()` (sum of
  payload sizes²), `canPickup(Unit)` (`:78`), `canPickup(Building)` (`:82`),
  `canPickupPayload` (`:86`), `hasPayload` (`:90`), `addPayload` (`:94`),
  `pickup(Unit)` (`:98`: remove unit from world, add `UnitPayload`),
  `pickup(Building)` (`:111`: `tile.remove()`, add `BuildPayload`),
  `dropLastPayload` (`:121`), `tryDropPayload` (`:133`: units → `dropUnit`,
  builds → `dropBlock` placing at the unit tile if `Build.validPlace`).
- `Mindustry/input/InputHandler.java:544` `requestUnitPayload`, `:556`
  `requestBuildPayload`, `:582` `pickedUnitPayload`, `:591`
  `pickedBuildPayload`, `:621` `requestDropPayload`, `:642` `payloadDropped`.
  Range checks: `player.unit().within(target, tilesize * type.hitSize ...)` —
  copy the exact expressions.

### 11.2 Port state
- `sim/mod.rs:785-794` `SimCommand::Payload { .. } => Err(CommandError::Unsupported("payload"))`.
- Wire format `determinism/command.rs:580-592` (`kind: 0 pickup unit, 1 pickup
  build, 2 drop; x, y; target: Option<i32>`).
- Unit carrier data: `entities/comp/unit/comp.rs:251` `PayloadComp { capacity }`
  only; `lifecycle.rs:253-263` also inserts building-path `PayloadHolder` and
  `UnitPayloadSize(hit_size)` for payload-capable units.
- Building payload helpers to reuse: `world/blocks/payloads/mod.rs`
  `payload_ref` (`:177`), `payload_size` (`:196`), `payload_fits` (`:220`),
  `create_build_payload` (`:239`), `handle_payload` (`:349`), `take_payload` (`:371`).
- gdext input: `mind-gdext/src/input/mod.rs` `pickup_payload`/`drop_payload`
  (~line 922-931) emit through `bridge`.

### 11.3 Changes (first slice: units only, then buildings)
1. Add `#[derive(Component, Debug, Clone, Default)] pub struct UnitPayloads { pub stack: Vec<CarriedPayload> }`
   in `entities/comp/unit/comp.rs` with
   `pub enum CarriedPayload { Unit { type_id: UnitTypeId, team: u8, health: f32, rotation: f32 }, Build { block: BlockId, team: u8, rotation: u8, health: f32 } }`
   and insert it next to `PayloadComp` in `lifecycle.rs:253`.
2. In `sim/mod.rs` replace the `Unsupported("payload")` arm with
   `self.apply_payload_command(kind, x, y, target)` implemented in
   `sim/unit_runtime.rs` `impl Sim`: carrier = the player's unit
   (`self.player_unit()` at `unit_runtime.rs:~490`).
   - kind 0: find live unit with `EntitySeq == target`; same team, not the
     carrier, `!flying` target unless Java allows; size check
     `payloadUsed + hitSize² <= capacity` (Java `canPickup`: `type.allowedInPayloads
     && payloadUsed() + u.hitSize*u.hitSize <= type.payloadCapacity + 0.001 && u.team == team && u.isAI()`);
     despawn the target (remove from `runtime.units`), push `CarriedPayload::Unit`.
   - kind 1: building at tile `(x,y)` (world→tile), same team,
     `block.size² * 64 <= capacity - used` (Java uses `build.block.size * tilesize` squared),
     `block.canPickup`; push `CarriedPayload::Build`, then remove the building
     through `block_runtime.break_block`-like path **without** refunds —
     reuse `SimRuntime::destroy_building`'s shared `remove_building`
     (make a `pub(crate) fn pickup_building(world, grid, entity)` that calls
     `remove_building` without `on_destroyed`), then `refresh_unit_runtime()`.
   - kind 2: pop last; unit → `runtime.spawn_by_id(...)` at carrier pos with
     stored health/rotation; build → `runtime_place(tile, block, rot, team)`
     at the carrier tile if valid, else keep it carried (Java returns false).
3. gdext: no rule changes needed if the bridge already emits
   `SimCommand::Payload`; verify with `grep -n "Payload" mind-gdext/src/input/*.rs`.
4. Save: add `UnitPayloads` to the live unit chunk only if time permits
   (append a revision 2 to `SIM_UNIT_REVISION`, keep reading revision 1).

### 11.4 Tests
`sim/combat_tests.rs::mega_picks_up_and_drops_dagger`: live sim with a
player unit: `sim.spawn_player_unit("mega", 1, x, y, 0.0)` (signature
`(name, team, x, y, rotation) -> Option<Entity>`, `unit_runtime.rs:566`),
spawn team-1 dagger adjacent, apply `SimCommand::Payload{kind:0, target:Some(seq)}`,
assert dagger entity gone and `UnitPayloads.stack.len()==1`, move carrier,
`kind:2`, assert a dagger exists at the new spot.

---

## 12. Other gaps found (not ledger items yet; report them)

1. **Unit AI dispatch is coarse:** `unit_update_system` only distinguishes
   Player / Command / everything-else (ground/flying anchor). `AiKind::Missile,
   Builder, Miner, Cargo, Defender, Suicide, Boost, Hug, FlyingFollow, Logic`
   all fall into the generic path. Ports exist under `mind-core/src/ai/types/`
   (`builder.rs`, `miner.rs`, `suicide.rs`, ...). Dispatch each to its
   `update_*` with an `AiCtx`. Missile is done in §3.
2. **Bullet lifesteal / owner:** `Bullet` has no `owner` field
   (`combat/bullet/mod.rs:65`); `BulletType.hitEntity` lifesteal and
   kill-credit events are skipped. `BulletSpawn.owner` exists — store it on
   the `Bullet` and apply `lifesteal` in `hit_entity`.
3. **Building destruction side effects:** no rubble decal, no `ConstructBlock`
   rebuild ghost / team `plans` entry (Java `BuildingComp.onDestroyed` +
   `Teams.registerCore`/`blocks` queue for rebuild), no
   `Damage.dynamicExplosion` for explosive contents (see §8.3 step 3).
4. **Unit collision damage / crash damage** (`UnitComp.destroy` `:921-927`
   crash damage for flying units) not ported.
5. **Item-source configure from the Input bridge:**
   `Input.configure_building(x, y, "item", "copper")` on an `item-source`
   returned true but the source did not feed an adjacent duo in-engine
   (unverified root cause; `ConfigValue::String("copper")` may not map to the
   item config). Low priority; the drill path works.
6. **Mounted unit weapons vs closest_target change:** `acquire_target`
   (`unit_runtime.rs` ~1150) now gets units-first targeting; if a wave walks
   past a turret to shoot units, that is Java behavior.

---

## 13. Final checklist per item

- [ ] code + doc comments citing Java file:line
- [ ] tests added and passing (`cargo test -p mind-core`, plus
      `--features msav-import` for Ground Zero tests)
- [ ] `cargo fmt -p mind-core`; clippy clean
- [ ] `tools/build.sh` + `tools/session-play.sh 2` + in-engine check where
      listed; evidence saved under `runs/20261012-session2/` (not committed)
- [ ] commit `Fixes PAR-XXXX: ...` with the Co-Authored-By line
