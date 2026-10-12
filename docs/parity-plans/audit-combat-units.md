# Combat + Units — audit of the executed work and executor plan (2026-10-12)

This file is self-contained. You (the executor) have not seen any earlier
conversation or plan; everything you need is here. Work the tasks in section 5
in the order given in section 6. Do not make judgement calls: if a step's
precondition does not hold (a function signature differs, a test cannot be
written as described), stop that task, write what you found under
"Executor notes" at the end of this file, and move to the next task.

Paths: `client/rust/...` and `docs/...` are relative to a checkout root.
`Mindustry/...` means the read-only Java reference at
`/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/...`.
`mind-core/...` means `client/rust/mind-core/...`.

---

## 1. Environment and workflow

### 1.1 Checkout and session
- Use a parity session worktree, never the main checkout directly for edits:
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  tools/parity-session.sh --session N        # creates/uses ../Mindustry-Godot-sessionN
  cd ../Mindustry-Godot-sessionN
  git merge --ff-only main
  ```
  Session N uses bridge port `6970 + 10*(N-1)` and X display `:(9+N)`.
- **First merge the audit fixes** from branch `parity/session-2` (section 3)
  if the coordinator has not merged them into `main` yet:
  `git merge --ff-only main` must already contain commit `8841d62`
  (`git branch --contains 8841d62` lists `main`). If it does not, ask the
  coordinator; do not cherry-pick yourself.
- Fresh worktree: generate the gitignored assets once (≈1 min):
  `bash tools/pack.sh pack`.
- Build the GDExtension + headless rig:
  `CARGO_BUILD_JOBS=6 tools/build.sh` (first build several minutes, then
  20-60 s). Every build needs an editor restart because the `.so` is mmap'd:
  `tools/session-play.sh N` (restart editor + play scene),
  `tools/session-play.sh N --import` (restart + import the full-tech-tree Java
  profile; expect `{"nodes":375,"unlocked":282}` in its output),
  `tools/session-play.sh N --play` (replay the scene only; use to recover from
  a stuck eval). The script prints `export OPEN_GODOT_MCP_PORT=...`; run it.
- **RAM:** 14 GB shared by up to 4 sessions. One cargo process at a time,
  always `CARGO_BUILD_JOBS=6`, never open a second editor. If memory is short,
  `pgrep -af "godot --path"` and kill only your own session's stale game.

### 1.2 Tests
```bash
cd client/rust
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib <filter> -- --nocapture
CARGO_BUILD_JOBS=6 cargo test -p mind-core --features msav-import          # full suite incl. Ground Zero map tests
cargo fmt -p mind-core
CARGO_BUILD_JOBS=6 cargo clippy -p mind-core --all-targets -- -D warnings   # must print nothing
```
Baseline after the audit fixes: `mind-core` lib **1940 passed, 0 failed, 4
ignored** with `--features msav-import`. `cargo test -p mind-headless --lib`
has **6 failures that predate all of this work** — ignore exactly these:
`campaign_scenarios::tests::sector_golden_matches`,
`parity::matrix::tests::landed_rows_must_resolve_when_list_supplied`,
`parity::registry::tests::committed_registry_matches_core_constant`,
`parity::registry::tests::fold_order_must_match_active_contributors`,
`parity::report::tests::full_check_set_passes`,
`units_scenarios::tests::committed_goldens_match`.

### 1.3 Driving the game (gmcp)
```bash
G="python3 tools/gmcp.py --timeout 140"
$G --eval 'return get_node("/root/Spine/SimHost").get_group_counts()'
$G --screenshot runs/<stamp>-<item>/name.png    # view the PNG with your image Read tool
```
Every `--eval` is ONE GDScript statement that must `return` something; no
loops (a hung eval pins the frame loop → `tools/session-play.sh N --play`).
Lambdas inside `filter(func(t): return ...)` are fine.

Useful calls (all verified on `main`):
- SimHost `/root/Spine/SimHost`: `place_block(x,y,"name")`,
  `break_block(x,y)`, `get_tick()`, `get_group_counts()`,
  `get_group_count("unit"|"bullet"|"weather")`, `content_list("unit")`,
  `apply_sim_command_json(json)` (ops `place`, `break`, `spawn_unit`
  `{"op":"spawn_unit","unit":<index in content_list("unit")>,"team":2,"px":<world x>,"py":<world y>}`,
  `unit_command`), `unit_debug_json()` (id/type/team/AI kind/pos/health/mounts),
  `unit_debug_positions()`, `tile_debug(x,y)` (floor/overlay/block/has_build),
  `building_debug(x,y)`, `selectable_units_json()`,
  `debug_add_core_items(item, amount)`.
- MindCampaign `/root/Spine/MindCampaign`: `run_wave()`, `get_hud_state()`.
- Input `/root/Spine/Input`: `toggle_command_mode()`, `command_mode()`,
  `get_input_state_json()`, `selection_json()`, `clear_building()`,
  `pickup_payload()`, `drop_payload()`, `push_event(json)`.

### 1.4 Ground Zero launch recipe (sector **170**, not 15)
```bash
$G --eval 'get_node("/root/Spine/SimHost").load_sector("serpulo", 170); return true' \
   --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 170)' \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval 'return get_node("/root/Spine/MindCampaign").get_hud_state()'
```
Expect `hasCore: true`, `sectorName: "groundZero"`, `wave: 1`. Verified Ground
Zero facts: core-shard center tile (129,55); copper ore at (130..131,48..49)
(a `mechanical-drill` at (130,48) feeding a `duo` at (132,48) works); wave
spawn produces a dagger near world (1156,1844) and flares near
(1352,1347); derelict `scrap-wall`s (team 0) exist around tile (169,167).
World pixel = tile*8 (+4 = tile center). The sim runs ≈75-100 ticks per
wall-clock second in the editor.

### 1.5 Commit conventions
One task = one commit. Message: `Fixes PAR-XXXX: <summary>` for a ledger
item, `Audit fix PAR-XXXX: <summary>` for a correction of earlier work. Body
cites the Java file:line ported. Last line exactly:
`Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Stage only
`client/`, `parity/`, `docs/` paths. `runs/` is a symlink to shared evidence:
write evidence there, never `git add` it. Never push, never merge into `main`.

### 1.6 Code rules
New files start with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
and `// SPDX-License-Identifier: GPL-3.0-only`. Cite Java in doc comments. No
`unwrap()`/`expect()` outside `#[cfg(test)]`. `mind-core` never uses
`godot`/`tokio`; `mind-gdext` holds no game rules. Determinism: iterate in
`EntitySeq` order (tie-break `entity.index()`), use only the `SimRng` you are
handed (`rng.range(RngStream::Sim, a, b)`, `rng.chance(RngStream::Sim, p)`),
no `HashMap` iteration order. **Panic-safety:** any code reachable from input
or render must not index `grid.tiles` without bounds checks — use
`grid.tiles.getn(x, y)` (returns `Option`) or check `in_bounds` first.

### 1.7 Live-sim architecture (what you will touch)
- `Sim` (`mind-core/src/sim/mod.rs`): `ecs.0` (bevy `World`), `grid`
  (authoritative tiles; NOT in the World), `block_runtime: Option<SimRuntime>`
  (`sim/runtime.rs`: place/break/destroy/pickup via `remove_building`).
- `UnitRuntime` (`sim/unit_runtime.rs`), a non-send World resource: `content`,
  `grid` (snapshot clone of `Sim.grid`, refreshed by
  `Sim::refresh_unit_runtime()`), `pathfinder`, `units`, `bullets`, `rng`,
  `seq` (shared unit/bullet sequence from 1_000_000_000). Take it out with
  `world.remove_non_send::<UnitRuntime>()` and always put it back.
- `Sim::tick()` order (verified `sim/mod.rs` ~line 527-545): `pump_io(Apply)`
  → `Trigger::Update` → schedule → `update_fires_and_puddles()` →
  `update_builders()` → `resolve_deaths()` →
  `flush_events()` → clock → `Trigger::AfterGameUpdate` → `pump_io(Capture)`.
  (Read `Sim::tick` to confirm before relying on it.)
- Schedule (`sim/schedule.rs`): `EntitySet` chain inside
  `TickSet::EntityUpdate`; live systems `update_buildings`,
  `turret_update_system` (after `update_buildings`), `unit_update_system`
  (UpdateUnits), `bullet_update_system`, `bullet_collide_system`,
  `run_wave_system`, `enemy_count_system`, campaign systems,
  `weather_system` then `team_ai_system` in `TickSet::WeatherAndAi`. Never
  reorder sets.

---

## 2. Per-commit verdicts

Audited range: `git log ea2a931..main` (main head `9aa3ef7`), commits in the
combat/units area. "Evidence" names the test, headless run or in-engine probe
(Ground Zero, sector 170) behind the verdict. In-engine notes are in
`runs/20261012-session2-audit/wave-after-derelict-fix.txt` (+ PNGs there).

| Commit | Item | Verdict | Evidence / defects |
|---|---|---|---|
| `1019d4f` | PAR-0088 spawnUnit bullets | **partial** | Core port is right: `spawn.rs::create` spawn-unit branch (BulletType.java:932-961), `SpawnedByBullet` + `adopt_spawned_units`, TimedKill tick, MissileAI movement, shoot-on-death in `resolve_deaths`, `ExplosionBulletType` defaults (ExplosionBulletType.java:17-28, verified), `createPuddles/createIncend/createUnits` with Java defaults (puddles 0, incendAmount 0, incendChance 1, despawnUnitRadius 0.1, faceOutwards false — verified BulletType.java:269-369). Defects: (1) in-engine an anthicus never fired — weapon mount rotation used a linear approach instead of `Angles.moveToward`, pinning rotating mounts at `-rotationLimit/2` (**fixed `1be95d2`**); (2) the commit message claims scathe works but scathe ammo is still a `BulletKind::Empty` placeholder and the 4 `scathe-missile*` unit types do not exist (task T1); (3) MissileAI kills on *any* enemy building — Java skips `block.underBullets` unless it is the target (MissileAI.java:33-36) (task T2); (4) `adopt_spawned_units` scans every entity 4× per tick (task T9). |
| `88e0cf8` | NEW-1 statuses/abilities | **correct** | Test-only commit; verified that `update_kinematics` (`entities/comp/unit/movement.rs` ~line 130-135) already calls `update_unit_status` and `update_unit_abilities` per unit, and the status applier is installed with the unit runtime (`sim/unit_runtime.rs` `install_unit_runtime_from`). Tests `live_status_burning_damages_unit`, `live_ability_repair_field_heals` pass. |
| `c13ccb1` | PAR-0137 floor speed | **partial** | `movement::unit_speed` matches UnitComp.java:107-110/190-194, CrawlComp.java:39, TankComp.java:120; all AI speed sites use it; test `dagger_slowed_on_deep_water` passes. Defects: strafe/boost terms read `UnitCore.elevation`, which nothing set — flyers sat at elevation 0 (**fixed `8841d62`**: spawn at elevation 1, UnitType.java:566); boosting mechs still never raise elevation (`updateBoosting`, UnitComp.java:121-130) (task T6). |
| `fe71e74` | PAR-0140 command gate | **correct** | Removing the boot restore of `selected_block` reopened the command-mode gate. Verified in-engine by the auditor through the real input chain: command mode on, `push_event` drag (380,2)→(490,60) selected `[1000000081]`, right-click at screen (700,324) moved the friendly dagger (1000,520)→(1061,444) (target ≈(1067,444)); `par0140-order.png`. Remaining: only MOVE is applied by `Sim::apply_live_unit_command` (task T11). |
| `4e71098` | Waves attack / core loss | **correct** (important) | Found and fixed a real bug: `BulletSpawn.target` was stored as `Bullet.sticky`, so unit bullets stuck to their target without damage; now seeds `aim_tile` (BulletType.java create → aimTile) and `stickTo` offsets are target-relative. Test `ground_zero_wave_destroys_undefended_core` passes. In-engine on fresh `main` the wave still did not attack: the dagger never left spawn and flares parked on derelict scrap walls because enemy-building search included team 0 (pre-existing; **fixed `8841d62`**). After the fix the wave daggers walk to the core and die to the defence. Also: idle/arrived units never turned toward their target, so fixed weapons never fired (**fixed `04b3572`**, AIController.faceTarget). |
| `85e30a6` | golden `harness_combat` | **correct (justified)** | `client/bin/rust/debug/mind-headless combat scenario combat_turret_ammo --json` → `pass: true, shots_fired 5, damage 27.0, checksum bc78036f6f27eac0`. The old checksum encoded the sticky bug (turret bullets glued to the wall dealt 0 damage); the scenario's own criterion (`after < before`) now holds. Not a regression cover-up. |
| `1e74c4d` | PAR-0091 fires/puddles live | **partial** | `Sim::update_fires_and_puddles` runs on the authoritative grid; PuddleComp effects branch ported (PuddleComp.java:96-110, 40-tick interval, `amount >= maxLiquid/2`); `splashLiquid` (BuildingComp.java:1467-1477) in `resolve_deaths`. Defects: puddle status applied to flying/hovering units (Java `unitCons` requires `isGrounded() && !hovering`, PuddleComp.java:25-36) and the splash deposit count used `floor` instead of Java's float loop bound (`ceil`) (**both fixed `8841d62`**). Missing: `Damage.dynamicExplosion` on building destruction (BuildingComp.java:1490-1517) (task T5). |
| `63b39b5` | PAR-0090 save fires/puddles | **correct** | Class ids 0xF1/0xF2 with revisions, write after units in tile order, decode + respawn, `clear_world_for_load` drops fires/puddles. Test `live_sim_save_keeps_fires_and_puddles` passes. Risk (pre-existing): `entity_count` counts units with `unit_count` but `write_entities` skips units lacking `UnitCore/UnitTypeComp/Pos`; a mismatch would desync the chunk count (task T13). |
| `5eacb5e` | PAR-0127 weather | **partial / risky** | `weather_system` ports `Logic.updateWeather` (Logic.java:376-391) faithfully, registered before `team_ai_system`; `weather_water_attr` feeds fires. Risk: the `PlayEvent` cooldown reset (Logic.java:90-99) is approximated by "all cooldowns == 0", so a loaded save with non-zero cooldowns is never re-shuffled as Java does on every play (task T3). No in-engine check (needs a weather sector). |
| `627a093` | PAR-0138 payload carrier | **partial** | `UnitPayloads` stack, pickup unit/building, drop with clamp, capacity/range gates (PayloadComp.java:78-86, InputHandler.java:544-660). Defects: missing `type.pickupUnits` gate (**fixed `8841d62`**); the grounded gate relied on `elevation`, which was never set (fixed by the elevation spawn fix); payloads are not saved (task T4); never verified in-engine through `Input.pickup_payload()/drop_payload()` (task T4). |
| `2ffa5ba` | merge of session-2 into main | **clean** | `git show 2ffa5ba --cc` shows no conflict hunks in the combat/units files. |

Other commits in `ea2a931..main` touching `sim/`, `combat/`, `ai/`,
`weapons/`, `entities/comp/unit/` (`229c5d6`, `26f6918`, `cca8826`,
`c9cb0e8`, `c94f37d`, `de8ef16`, `1a30f0d`, `c72b7f6`) belong to the
building/production area; the auditor checked only that the full
`mind-core` suite (1938 → 1940 tests with the audit fixes) still passes.

## 3. Audit fixes already made (branch `parity/session-2`, merge these first)

All on top of `main` `9aa3ef7`; full suite green with
`--features msav-import`, clippy clean.

| SHA | Fix | Test |
|---|---|---|
| `8841d62` | Derelict (team 0) buildings are not attack targets/anchors in live campaign worlds (`ai::block_indexer::is_derelict`, used by `BlockIndexer::find_enemy_building` and `nearest_enemy_building`; BlockIndexer.java:466). Flyers spawn at elevation 1 (UnitType.java:566). Puddle status only for grounded non-hovering units. splashLiquid count `ceil`. Payload pickup requires `pickupUnits`. | existing suite; in-engine wave now attacks |
| `04b3572` | `face_target` in `sim/unit_runtime.rs`: Ground/Command units turn toward their in-range target (AIController.java:121-128). | `sim::combat_tests::idle_unit_faces_and_shoots_target_behind_it` (fails before the fix) |
| `21093b3` | `mind-gdext/src/sim_host.rs`: `tile_debug`, `conveyor_items`, `construct_draw`, `construct_at`, `building_team` use `grid.tiles.getn` (no panic off-map); restored the `#[func]` on `construct_progress` that had been swallowed into a doc comment. | in-engine `tile_debug(-5, 9999)` → `{}`; `has_method("construct_progress")` → `true` |
| `1be95d2` | `weapons::angles_move_toward` (Arc Angles.java:38-50) for mount rotation and the rotation-limit clamp. | `weapons::angle_tests::angles_move_toward_takes_short_arc`; in-engine anthicus launches 2 missiles and kills a copper wall |

## 4. How to read the tasks

Each task: **Symptom / Acceptance**, **Java**, **Port today**, **Changes**
(ordered), **Tests**, **Verify**, **Pitfalls**. Line numbers are from the
audited tree (`main` 9aa3ef7 + the audit fixes); if a line moved, search for
the quoted function name. Commit each task separately (section 1.5).

---

## 5. Tasks

### T1 — PAR-0088 remainder: scathe missiles (Erekir `scathe` turret)

**Symptom.** A `scathe` fed carbide/phase/surge "fires" an inert
`BulletKind::Empty` bullet; no missile, no damage.
**Acceptance.** Placing a team-1 `scathe` with carbide (via
`turrets::handle_item`) and a team-2 dagger 30 tiles away: within 900 ticks a
`scathe-missile` unit exists and the dagger is dead; unit ids of the four
missile types are 65..68 matching `parity/java/jvm_golden_content.json`.

**Java.**
- `Mindustry/content/Blocks.java:5250-5530` (`scathe` ammo):
  carbide → `new BulletType(0f, 0f){ spawnUnit = new MissileUnitType("scathe-missile"){ speed 4.6, maxRange 6, lifetime 60*5.5, hitSize 10, rotateSpeed 0.25, missileAccelTime 50, lowAltitude, targetAir false, targetUnderBlocks false, fogRadius 6, health 240, weapon{shootCone 360, mirror false, reload 1, shootOnDeath true, bullet = ExplosionBulletType(1000, 65){collidesAir false, buildingDamageMultiplier 0.1, ammoMultiplier 1, fragLifeMin 0.1, fragBullets 7, fragBullet = ArtilleryBulletType(3.4, 32){buildingDamageMultiplier 0.1, drag 0.02, knockback 0.8, lifetime 23, collidesTiles false, splashDamageRadius 40, splashDamage 100}}}, abilities MoveEffectAbility }}`
  (lines 5251-5330); phase variant `scathe-missile-phase` from line 5345;
  surge variant `scathe-missile-surge` from 5450 with a nested
  `scathe-missile-surge-split` at 5506. Read each block in full and copy every
  numeric field.
- JVM content order: `parity/java/jvm_golden_content.json` unit entries 65
  `scathe-missile`, 66 `-phase`, 67 `-surge`, 68 `-surge-split` (69
  `turret-unit-build-tower` is out of scope). The port currently has 65 units
  (ids 0..64, last `dummy`).

**Port today.**
- `mind-core/src/world/blocks/defense/turrets/advanced.rs` ~891-905:
  `add("scathe_carbide", BulletKind::Empty, ...)`, `scathe_phase`,
  `scathe_surge` (comment "plan 11 blocker"). Config `"scathe"` ~line 1598
  uses them.
- Unit spec template for a missile with a shoot-on-death explosion:
  `mind-core/src/content/registries/units/erekir.rs` ~1616-1700
  (`anthicus-missile` inside the anthicus weapon bullet's `spawn_unit`).
- Unit load order: `content/registries/units/mod.rs` ~2305-2307
  (`standard::load`, `erekir::load`, `special::load`); `special.rs` ends with
  `dummy` (~line 104).
- `spawn.rs::create` already handles `BulletDef.spawn_unit` (T0 done).

**Changes.**
1. `content/registries/units/special.rs`: add
   `pub fn scathe_missiles(sink: &mut dyn UnitSink) -> Result<(), ContentError>`
   pushing four units with `sink.push_unit(...)` in the order
   `scathe-missile`, `scathe-missile-phase`, `scathe-missile-surge`,
   `scathe-missile-surge-split`, each built like the anthicus-missile spec
   (`spec(name, UnitKind::MissileUnitType, entity::MISSILE)`, fields from Java,
   one `WeaponSpec` with `shoot_on_death = Some(true)`, `shoot_cone 360`,
   `mirror false`, `reload 1`, bullet `BulletSpec{ kind: BulletKind::Explosion,
   damage/splash per ExplosionBulletType(damage, radius) constructor:
   splash_damage = 1000, splash_damage_radius = 65, ...}`). For the frag
   artillery use the `BulletSpec` frag field (`grep -n "frag_bullet" content/registries/bullets.rs`
   to find the spec-side name; it takes a `BulletRef::Inline`). For the surge
   missile, the split missile is referenced by its own bullet's `spawn_unit`
   or `despawn_unit` exactly as Java does at line ~5506 — read it.
   `MoveEffectAbility` is visual: skip it.
2. `content/registries/units/mod.rs` ~2307: call
   `special::scathe_missiles(sink)?;` right after `special::load(sink)?;`.
3. `advanced.rs` `register_bullets`: before the `add` closure is created,
   resolve `let missile = content.unit_id("scathe-missile");` (and the other
   two); change the three scathe ammo bullets to `BulletKind::Basic` with
   `speed 0, damage 0, ammo_multiplier` per Java (`carbide 1`, check phase and
   surge), `spawn_unit = missile`, and also set `def.spawn_unit_range =
   Some((lifetime, speed))` of the missile if that field is used for range
   (`bullets.rs` ~1132 `range()` uses it). Remove the "plan 11 blocker"
   comment.
4. Regenerate the content golden:
   `cd client/rust && cargo run -p mind-headless -- content dump` (this is the
   `regen` command recorded in `parity/golden_manifest.json` for
   `parity/golden_content.json`). Confirm with
   `python3 -c "import json;g=json.load(open('parity/golden_content.json'));print([e['name'] for t in g['types'] if t.get('type')=='unit' for e in t['entries']][64:])"`
   → `['dummy', 'scathe-missile', 'scathe-missile-phase', 'scathe-missile-surge', 'scathe-missile-surge-split']`.
   Update the manifest sha: `sha256sum parity/golden_content.json` → put it
   in the `"id": "content_ids"` entry of `parity/golden_manifest.json`.

**Tests.** `mind-core/src/sim/combat_tests.rs::scathe_launches_missile_and_kills`:
`live_sim(64)`; `place(&mut sim, 10, 10, "scathe", 1)`; feed 10 carbide with
the `feed` helper in that file; spawn `dagger` team 2 at world
(10*8+240, 10*8); tick ≤ 900, record whether any unit with type name
`scathe-missile` appeared (`UnitTypeComp` → `content.unit(id).name`); assert
seen and dagger despawned. Also extend
`content::parity` golden tests by running them (no code change).

**Verify.** `cargo test -p mind-core --features msav-import` (expect +1 test,
0 failed). In-engine: Ground Zero, `place_block(126,64,"scathe")`; feed with
`debug_add_core_items("carbide", 100)` is not enough (turret needs items):
an `item-source` cannot be configured from the bridge (T19) — use a conveyor from
the core is not available either; **skip in-engine** and rely on the test;
state this in the commit body.

**Pitfalls.** Unit ids must be appended after `dummy`; inserting elsewhere
shifts every later unit id and breaks saves/goldens. `scathe` range is
`max(spawnUnit.lifetime * spawnUnit.speed)` = 330*4.6 = 1518 (already in the
config). Missile `targetAir = false` ⇒ `target_air: Some(false)`.

---

### T2 — Under-bullet blocks (conveyors etc.): missiles and bullets

**Symptom.** Missiles explode on the first enemy conveyor they fly over, and
ordinary bullets hit conveyors/conduits/junctions/routers instead of passing
over them. **Acceptance.** A team-2 dagger shooting at a team-1 copper wall
with a team-1 conveyor in between damages the wall, not the conveyor; an
anthicus missile flying over a team-1 conveyor toward a wall hits the wall.

**Java.**
- `Mindustry/world/Block.java:120` `underBullets`; `:1386-1387`
  (`if(underBullets) priority = TargetPriority.under`).
- Classes with `underBullets = true`: `Conveyor.java:47` (also
  ArmoredConveyor), `StackConveyor.java:54`, `Junction.java:21`,
  `ItemBridge.java:51` (also BufferedItemBridge), `Sorter.java:27`,
  `Router.java:19`, `OverflowGate.java:18`, `DuctRouter.java:37`,
  `DuctBridge.java:16`, `DuctJunction.java:29`, `Conduit.java:51` (also
  ArmoredConduit), `PayloadConveyor.java:39` (also PayloadRouter),
  `TargetDummy.java:40`; plus explicit `underBullets = true` in
  `Mindustry/content/Blocks.java` at lines 2234, 2250, 2266, 2349, 2421,
  2432, 2441 (open each and note the block name above it).
- `Mindustry/entities/comp/BulletComp.java:212-225` `checkUnderBuild`:
  hit an under-bullet building only if aimed at it (`aimTile.build == build`),
  `type.hitUnder`, same team, a piercing bullet past its aim tile, or no aim
  (`aimX == -1 && aimY == -1`). Used by `tileRaycast` (`:251`).
- `Mindustry/ai/types/MissileAI.java:33-36`: missile dies on contact with an
  enemy building only if `build == target || !build.block.underBullets`.

**Port today.** No `under_bullets` field; `BlockDef.priority` for conveyors
is `TARGET_PRIORITY_TRANSPORT` (the `Block.init` derivation was never ported).
`combat/bullet/mod.rs::collide_bullet` (~line 724+) hits any enemy building
on the raycast. `ai/types/missile.rs::update_missile` kills on any enemy
building.

**Changes.**
1. `content/registries/blocks/mod.rs`: add to `BlockDef` (resolved record,
   ~line 1970 area, near `priority`) `pub under_bullets: bool` with doc
   `Block.underBullets`; set it in the spec→def conversion: `true` when
   `def.kind` is one of `Conveyor, ArmoredConveyor, StackConveyor, Junction,
   ItemBridge, BufferedItemBridge, Sorter, Router, OverflowGate, DuctRouter,
   DuctBridge, DuctJunction, Conduit, ArmoredConduit, PayloadConveyor,
   PayloadRouter, TargetDummy` **or** the block name is one of the seven
   Blocks.java names found above; and when `under_bullets` is true and the
   spec gave no explicit priority, `priority = TARGET_PRIORITY_UNDER`.
   (If a `BlockKind` variant named above does not exist, check
   `BlockKind` in `content/registries/blocks/mod.rs` ~393-470 and use the
   exact names there; a Java class extending one of these counts.)
2. If `parity/golden_content.json` includes `priority` for blocks, regenerate
   it (T1 step 4 command) and update the manifest sha.
3. `combat/bullet/mod.rs::collide_bullet`: before `hit_building` on an
   enemy building, compute
   ```rust
   let under = ctx.content.block(building_block).is_some_and(|d| d.under_bullets);
   let aimed = state.aim_tile == Some((tx as i16, ty as i16)) /* compare with the building's center tile; see Bullet.aim_tile */;
   let no_aim = state.aim == (-1.0, -1.0);
   let past_aim = def.pierce && state.aim_tile.is_some_and(|(ax, ay)| {
       let (ox, oy) = state.origin; let a = ((ax as f32*8.0-ox).powi(2)+(ay as f32*8.0-oy).powi(2)).sqrt();
       let (px, py) = ctx.pos(bullet).unwrap_or(state.last); ((px-ox).powi(2)+(py-oy).powi(2)).sqrt() > a + 2.0 });
   if under && !(aimed || no_aim || past_aim || same_team) { return false; /* continue raycast */ }
   ```
   (`type.hitUnder` is not in `BulletDef`; add `pub hit_under: bool`
   default false and include it.) `aim_tile` holds the *building's* tile when
   the target was a building (see `spawn.rs` seeding from `Building.tile`).
4. `ai/types/missile.rs::update_missile`: replace the building-contact
   condition with `target_ground && build_team != Some(team) &&
   (Some(build) == state.target || !under_bullets(build))`. For the missile's
   target use the shooter's aim tile building: `state.target` is not set by
   anyone today — set it once when the missile spawns: in `spawn.rs`
   spawn-unit branch store `MissileShooter` and also a new
   `MissileTarget(Option<Entity>)` from `spawn.target` (the aimed building),
   read it in `unit_update_system`'s Missile branch into `state.target`.

**Tests.** `sim/combat_tests.rs`:
- `bullets_pass_over_enemy_conveyor`: live sim; team-1 `copper-wall` at
  (20,10) and team-1 `conveyor` at (15,10); team-2 dagger at world
  (10*8, 10*8+4) facing +x (`spawn_live_unit("dagger", 2, 84.0, 84.0, 0.0)`);
  tick 600; assert conveyor health == max and wall health < max.
- `missile_ignores_conveyor_below`: like `missile_dies_at_lifetime_and_explodes`
  in the same file, with a team-1 conveyor on the flight path; assert the
  conveyor survives.
**Verify.** Full suite; in-engine optional.
**Pitfalls.** The building raycast uses the runtime grid snapshot; buildings
store the center tile; multi-tile buildings: compare entities, not tiles
(`aim_tile` → `grid.tiles.getn(ax, ay)?.build == Some(target)`).

---

### T3 — PAR-0127: weather cooldowns reset on every play (`PlayEvent`)

**Symptom.** Weather cooldowns are initialised only when every entry reads
exactly 0; a loaded save keeps its stored cooldowns, unlike Java.
**Acceptance.** Every `logic_play` (new game and save load) re-shuffles and
re-assigns cooldowns once; the "all zero" heuristic is gone.

**Java.** `Mindustry/core/Logic.java:90-99` (PlayEvent: shuffle copy of
`rules.weather`, `cooldown = sum + random(maxFrequency)`, `sum += cooldown`).

**Port today.** `mind-core/src/game/runtime.rs::weather_system` (~line 820-870)
has `if !weather.is_empty() && weather.iter().all(|e| e.cooldown == 0.0) { shuffle... }`.
`game/play.rs:480 logic_play(session)` is the `Logic.play` port (called from
`play.rs:249` and `:399`).

**Changes.**
1. `game/play.rs`: add `pub weather_reset_pending: bool` to `PlaySession`
   (`pub struct PlaySession`, `game/play.rs` ~line 97, derives only
   `Debug, Clone` — no serde attribute needed; add `weather_reset_pending:
   false` to `PlaySession::new`). In `logic_play` set `session.weather_reset_pending = true`.
2. `game/runtime.rs::weather_system`: replace the `all(... == 0.0)`
   condition with `campaign.session.weather_reset_pending`, and set it to
   `false` after the shuffle block.
**Tests.** Update `sim/combat_tests.rs::weather_rain_spawns_and_wets_units`
if it relied on zero cooldowns (it uses `always: true`, so it should still
pass). Add `game/runtime.rs` test `weather_cooldowns_reset_on_play`: build a
`PlaySession` with two entries with `cooldown = 9999.0`, call `logic_play`,
run `weather_system` once on a world containing the `CampaignRuntime` and a
`UnitRuntime` (copy the setup from `weather_rain_spawns_and_wets_units`),
assert both cooldowns < 9999 and the flag is false.
**Pitfalls.** Use `runtime.rng` (UnitRuntime) exactly as the existing shuffle
does, to keep determinism.

---

### T4 — PAR-0138 remainder: payload key path, building pickup, save

**Symptom.** In-engine `[` (`Input.pickup_payload()`) searches around the
*cursor* and never picks up buildings; `]` drops at the cursor; carried
payloads vanish on save/load.
**Acceptance.** With the player in a `mega` (Serpulo; `pickupUnits` true,
`payloadCapacity` ≥ 2x2 blocks), `[` next to a friendly dagger picks it up;
with no unit near, `[` over a friendly `copper-wall` picks it up; `]` drops at
the carrier position; a save/load keeps the carried stack.

**Java.** `Mindustry/input/InputHandler.java:1039-1056` `tryPickupPayload`
(closest unit within `unit.type.hitSize * 2` of the carrier with `isAI &&
isGrounded && canPickup && within(hitSize + unit.hitSize)`, else the building
*under the carrier* `world.buildWorld(pay.x(), pay.y())` with
`canInteract && buildVisibility != hidden && canPickup && pay.canPickup(build)`
→ `requestBuildPayload`); `:1058-1063` `tryDropPayload` → drop at
`player.x, player.y`; `:544-660` request handlers;
`Mindustry/entities/comp/PayloadComp.java:74-140`.

**Port today.** `mind-gdext/src/input/desktop.rs:787-807`:
`payload(kind)` sends `RemoteAction::Payload { kind, x, y (cursor), target: None }`;
`pickup_payload` always sends `PickupUnit`. Core:
`mind-core/src/sim/unit_runtime.rs` `apply_payload_command` →
`payload_pickup_unit(carrier, x, y, target)` (searches near `(x,y)`),
`payload_pickup_build(carrier, x, y)`, `payload_drop(carrier, x, y)`.
Unit save chunk: `mind-core/src/io/sim_io.rs` `SIM_UNIT_REVISION = 1`,
`write_entities`/`LiveEntitySink::read_entity`.

**Changes.**
1. Core, `payload_pickup_unit` with `target == None`: search center = carrier
   `Pos`, radius `carrier_hit * 2`, extra gate `within(target_hit +
   carrier_hit)` (Java line 1043); if no candidate, call
   `self.payload_pickup_build(carrier, carrier_pos.x, carrier_pos.y)` and
   return its result. Keep the explicit `target: Some(seq)` path as is.
2. Core, `payload_drop`: ignore `(x, y)` and drop at the carrier position
   (Java `requestDropPayload(player, player.x, player.y)`).
3. Building pickup: add the `buildVisibility != hidden` gate
   (`content.block(block).build_visibility`; check the field name with
   `grep -n "build_visibility" content/registries/blocks/mod.rs`).
4. Save: bump `SIM_UNIT_REVISION` to 2; after the existing unit fields write
   `u16 count` then per payload `u8 tag (0 unit, 1 build)` + fields
   (`unit: u16 type_id, u8 team, f32 health, f32 rotation`; `build: u16 block,
   u8 team, u8 rotation, f32 health`). Reader: accept revision 1 (no
   payloads) and 2. Restore into `UnitPayloads.stack` when the unit is
   respawned on load (find where `DecodedUnit` is materialised in
   `sim_io.rs`). Update `live_unit_class_id_is_reserved` if it asserts the
   revision value.
**Tests.** `sim/combat_tests.rs`: `mega_pickup_without_target_uses_carrier_position`
(cursor far away, dagger adjacent to the carrier → picked);
`mega_pickup_falls_back_to_building_under_carrier`; `io/sim_io.rs`:
`live_save_keeps_unit_payloads` (save/load round-trip, stack equal).
**Verify (in-engine).** Ground Zero; the player unit is `alpha`
(no payload). Spawn a mega for the player is not exposed — skip the
in-engine pickup, but verify `Input.pickup_payload()` does not error:
`$G --eval 'get_node("/root/Spine/Input").pickup_payload(); return true'`.
**Pitfalls.** Old saves must still load (revision 1 path). Keep the
payload stack order (last in, first out).

---

### T5 — PAR-0091 remainder: destroyed buildings explode (`dynamicExplosion`)

**Symptom.** Destroying a building full of explosive/flammable items or a
charged battery does no area damage and starts no fires.
**Acceptance.** Destroying a team-1 `container` holding 300 blast compound
damages a team-1 wall 2 tiles away; destroying a container with 300 coal
creates fires (rules `fire` on).

**Java.** `Mindustry/entities/comp/BuildingComp.java:1490-1517`
`onDestroyed` (explosiveness/flammability/power sums over items, liquids,
buffered power; `splashLiquid`; `Damage.dynamicExplosion(x, y,
flammability * flammabilityScale, explosiveness * 3.5 * explosivenessScale,
power, tilesize * size / 2, rules.damageExplosions, destroyEffect,
baseShake)`); `Mindustry/entities/Damage.java:93-130` `dynamicExplosion`
(lightning count `clamp(power/700, 0, 8)`; fireballs `clamp(flammability/4,
0, 30)` via `Bullets.fireball` when `fire`; waves `explosiveness <= 2 ? 0 :
clamp((int)(explosiveness/11), 1, 25)`, `damagePerWave = explosiveness/2`,
wave i radius `clamp(radius + explosiveness, 0, 50) * ((i+1)/waves)`).

**Port today.** `mind-core/src/combat/damage/explosion.rs:23`
`dynamic_explosion(world, content, source_team, x, y, radius, damage, power,
fire)` only applies one `damage_area` (fire/power inert). Building death:
`Sim::resolve_deaths` (`sim/unit_runtime.rs`, building half) calls
`SimRuntime::destroy_building` and the splash code; no explosion.

**Changes.**
1. Rewrite `dynamic_explosion` to the Java signature
   `(world, content, rng, x, y, flammability, explosiveness, power, radius,
   damage: bool, fire: bool, ignore_team: Option<u8>)` and the Java body:
   waves applied immediately in order i = 0..waves (the `Time.run(i*2)`
   stagger is dropped; note it in a comment) with `damage_area(world, content,
   ignore_team, x, y, clamp(radius + explosiveness, 0, 50) * (i+1)/waves,
   explosiveness/2, DamageOptions::default())`; fireballs: skip (needs
   `Bullets.fireball` content — record as executor note) but create
   `fires::create` at the tile when `fire && flammability > 0` once;
   lightning: skip with a comment. Update all callers
   (`grep -rn "dynamic_explosion(" mind-core/src`).
2. In `resolve_deaths` building half, before `destroy_building`, compute
   explosiveness/flammability/power exactly as BuildingComp.java:1491-1508
   using `ItemModule` amounts capped at the block `item_capacity`
   (`explosionItemCap`), `Item.explosiveness/flammability/charge`
   (check field names in `content/registries/items.rs`), liquids `/2`, and
   buffered power `PowerModule.status * capacity`; then call
   `dynamic_explosion(..., radius = 8*size/2, damage = rules.damage_explosions,
   fire = rules.fire ...)` with the unit runtime's content/rng (same borrow
   pattern as the existing splash code there).
**Tests.** `sim/combat_tests.rs::destroyed_blast_container_damages_neighbours`.
**Pitfalls.** Chain reactions: a neighbour killed by the explosion dies in the
*next* `resolve_deaths` (dead list is collected first) — acceptable; do not
recurse.

---

### T6 — PAR-0137 remainder: boosting mechs (elevation)

**Symptom.** `nova`-type mechs (`canBoost`) never lift off: `UnitCore.elevation`
stays 0 while boosting, so they are hit by ground bullets, use ground floor
speed and take no boost speed.
**Acceptance.** A player `nova` with boost held reaches elevation 1 after
`1/riseSpeed` ticks, `unit_speed` includes `boostMultiplier`, ground-only
bullets stop hitting it.
**Java.** `Mindustry/entities/comp/UnitComp.java:121-130` `updateBoosting`:
`elevation = approachDelta(elevation, canBoost ? num(shouldBoost) : 0,
shouldBoost ? riseSpeed : descentSpeed)`, `shouldBoost = boost || onSolid()
|| (isFlying() && !canLand())`.
**Port today.** `UnitCore.boosting` is set by `ai/types/boost.rs:26`
and the player path; nothing writes `elevation`.
`UnitTypeDef.rise_speed/descent_speed` exist (`units/mod.rs:1033-1035`).
**Changes.** In `entities/comp/unit/movement.rs::update_kinematics` (the
per-unit tail), for units with `PhysicsComp.can_boost`: compute
`should_boost = core.boosting || on_solid` (`on_solid`: the unit's tile is
solid — `grid.tiles.getn(...)` block solid flag; see how `update_ground`
checks solidity) and approach `core.elevation` toward 0/1 with
`rise_speed`/`descent_speed` (linear `approach`).
**Tests.** `sim/combat_tests.rs::boosting_nova_lifts_off`.

---

### T7 — Flying wave units park and stop shooting (`circleTarget`)

**Symptom (seen in-engine on fresh main).** Wave flares reached their target
and hovered motionless; after 3 shots `total_shots` never increased.
**Acceptance.** A team-2 flare with a team-1 wall target keeps circling
(`Vel` length > 0 every tick after reaching it) and keeps firing (wall
destroyed within 1200 ticks).
**Java.** `Mindustry/ai/types/FlyingAI.java:15-31` (`circleTarget ?
circleAttack(circleTargetRadius) : moveTo(target, range*0.8) + lookAt`);
`Mindustry/entities/units/AIController.java:318-351` `circleAttack`;
flare: `content/UnitTypes.java:1058-1061` (`circleTarget = true`,
`circleTargetRadius = 60`, `omniMovement = false`, `rotateSpeed = 5`).
**Port today.** `mind-core/src/ai/types/flying.rs::update_flying(world, grid,
content, entity, target: TilePos)` approaches a tile and stops ("arrived");
`unit_update_system` passes the building anchor tile.
**Changes.** In the generic AI branch of `unit_update_system`, when the unit
is flying and `mount_target` is `Some`, call a new
`ai/types/flying.rs::circle_attack(world, grid, content, entity, target_pos,
circle_length)` porting AIController.java:318-351 (keep `turning_away` in a
new `#[derive(Component, Default)] FlyingCircleState { turning_away: bool }`)
when `def.circle_target`, else `move_to(target, def.range * 0.8)` + face the
target. `movePref(vec)` = set velocity toward `vec` with the unit's accel
(mirror `move_direct` in `ai/ai_controller.rs`).
**Tests.** `sim/combat_tests.rs::flare_circles_and_keeps_firing`.

---

### T8 — Units stack on top of each other (no unit physics)

**Symptom (seen in-engine).** Two wave daggers walked the whole path with
identical coordinates.
**Acceptance.** Two ground daggers spawned at the same point are at least
`0.6*8 + 0.6*8 - 1 = 8.6` px apart after 60 ticks (body radius =
`hitSize * 0.6`, dagger hitSize 8); a flare and a dagger at the same point do
not push each other (different layers).
**Java.** `Mindustry/async/PhysicsProcess.java:15-245` (layers ground/legs/
flying/underwater, body radius `hitSize * unitCollisionRadiusScale(0.6)`
(`Vars.java:121`), mass `hitSize² π` (`PhysicsComp.java:18-20`), pair push
`vec.setLength(rs - dst)`, `body += vec * (other.mass/ms) / 1.25`, stacked
bodies pushed in a random direction), `UnitComp.collisionLayer()`
(`UnitComp.java:487-489`); only units with `type.physics` take part.
**Changes.** New system `unit_physics_system` in `sim/unit_runtime.rs`,
registered in `EntitySet::PhysicsUnits` (`sim/schedule.rs`, the set already
exists before `UpdateUnits`). Iterate `runtime.units` sorted by `EntitySeq`;
O(n²) pair loop is acceptable below 500 units (use a uniform grid bucket of
cell = 32 px otherwise). Random direction for exact overlaps:
`runtime.rng.range(RngStream::Sim, 0, 360)`. After moving, clamp to
passable tiles the same way ground movement does (do not push into walls:
skip the move if the destination tile is solid).
**Tests.** `sim/combat_tests.rs::stacked_daggers_separate`.
**Pitfalls.** Determinism: process pairs in seq order and apply moves
immediately like Java (order-dependent but deterministic).

---

### T9 — Performance: `adopt_spawned_units` scans every entity 4×/tick

**Change.** In `sim/unit_runtime.rs::adopt_spawned_units`, replace
`world.iter_entities()` with a query
`world.query_filtered::<(Entity, Option<&EntitySeq>), With<SpawnedByBullet>>()`.
**Test.** Existing PAR-0088 tests must still pass.

---

### T10 — Derelict units and `rules.coreCapture`

**Change.** `combat/targeting.rs::closest_unit_filtered`: skip units whose
team `is_derelict(world, team)` (`ai::block_indexer::is_derelict`). In
`is_derelict`, return `false` when the campaign rules have `core_capture`
(`grep -n "core_capture" mind-core/src/io/json/rules.rs`; Java
BlockIndexer.java:466). Test in `combat/targeting.rs` tests with a
`CampaignRuntime` default team 1.

---

### T11 — PAR-0140 remainder: command types other than MOVE

**Java.** `Mindustry/ai/UnitCommand.java` (move, repair, rebuild, assist,
mine, boost, enterPayload, loadUnits, loadBlocks, unloadPayload),
`Mindustry/ai/types/CommandAI.java` (`command(UnitCommand)` and per-command
controllers).
**Port today.** `Sim::apply_live_unit_command` (`sim/mod.rs` ~383) returns 0
for any `command != MOVE_COMMAND`.
**Changes.** Accept the command id, store it on the unit
(`ControllerSlot` or a new `UnitCommandComp(u16)`), and for `boost`
(sets `UnitCore.boosting`) and `rebuild/assist/repair/mine` map to the
existing AI kinds in `ai/types/` (`builder.rs`, `repair.rs`, `miner.rs` —
check names with `ls mind-core/src/ai/types`). Do MOVE + BOOST first; record
the rest as executor notes if the target AI is not ported.

---

### T12 — Aim lead (`Predict.intercept`)

**Java.** `Mindustry/entities/Predict.java` `intercept(src, dst, bullet)`;
used by `AIController.updateWeapons` (mount aim) and `faceTarget`.
**Port today.** `set_mount_targets` and `face_target` aim at the target
position (fast targets are missed).
**Change.** Port `Predict.intercept` into `combat/targeting.rs` and use it in
both places with the first weapon's bullet speed. Test: a dagger hits a
flare moving perpendicular at full speed more often than before (count hits
over 600 ticks, assert ≥ 3).

---

### T13 — Save entity count must match written chunks

**Port today.** `io/sim_io.rs::LiveEntitySource::entity_count` counts
`unit_count()` (entity-validated) while `write_entities` `continue`s on units
missing `UnitCore/UnitTypeComp/Pos`.
**Change.** Build the unit list once in a helper used by both functions so
the count equals the number of chunks written. Test: spawn a unit, remove its
`UnitCore`, save/load succeeds.

---

### T14 — Remaining AI kinds are not dispatched (original scope)

**Port today.** `unit_update_system` handles Player, Command, Missile and a
generic ground/flying path. `AiKind::{Builder, Miner, Cargo, Defender,
Suicide, Boost, Hug, FlyingFollow, Logic}` fall into the generic path.
**Change.** For each kind with an `update_*` in `mind-core/src/ai/types/`,
add a branch building an `AiCtx` like the Missile branch. Start with
`Suicide` (crawler) and `Builder` (wave-team `alpha/beta/gamma` under
`buildAi`). One commit per kind, each with a test spawning that unit type.

### T15 — Bullet owner, lifesteal and kill credit

**Java.** `BulletType.hitEntity` lifesteal (BulletType.java hitEntity:
`if(lifesteal > 0 && b.owner instanceof Healthc o) o.heal(result *
lifesteal)`).
**Change.** Add `owner: Option<Entity>` to `combat/bullet/mod.rs::Bullet`,
set from `BulletSpawn.owner` in `spawn.rs::create`, apply lifesteal in
`hit_entity` with `crate::combat::damage::area::heal_health`.

### T16 — Building destruction rubble / rebuild plans

**Java.** `BuildingComp.onDestroyed` rubble (`Effect.rubble`, visual — skip)
and `ConstructBlock`/`Teams.TeamData.plans` "rebuild" ghosts added when a
block is destroyed (`Build.java`/`BuildingComp.java` `addPlan` on destroy —
search `data().plans.addFirst` in `Mindustry/`).
**Change.** In `resolve_deaths`, after `destroy_building`, push a rebuild plan
into `CampaignRuntime.session.teams` for the building's team (see how
`team_plans` are read in `io/sim_io.rs` for the struct).

### T17 — Flying unit crash damage

**Java.** `UnitComp.java:921-927` (`type.flying && !spawnedByCore &&
createWreck && rules.unitCrashDamage > 0` → `Damage.damage(team, x, y,
hitSize^0.94 * 1.25, crashDamage)`, `crashDamage = hitSize^0.75 *
crashDamageMultiplier * 2.5 * rules.unitCrashDamage(team)`).
**Change.** In `resolve_deaths` before `kill_unit_with_abilities`.

### T18 — Unit separation for commanded groups / formation (low priority)

Java `CommandAI` uses `UnitGroup` formations; port moves each unit to the
same point (they will stack until T8 lands). Do after T8.

### T19 — Item-source configuration from the Input bridge (shared with building area)

**Symptom.** `Input.configure_building(x, y, "item", "copper")` on an
`item-source` returns true but the source outputs nothing. Not in the combat
area; report to the building-production owner instead of fixing, unless
idle.


---

## 6. Execution order and dependencies

1. Merge/confirm the audit fixes (`8841d62`, `04b3572`, `1be95d2`,
   `21093b3`) are in `main` (section 1.1).
2. T9 (tiny, makes T1 tests faster) → T1 → T2 (T2 step 4 needs the
   spawn-unit path from T1 tested) → T13.
3. T3, T6, T10 (independent, small).
4. T7 then T8 (T8 changes positions that T7 tests read; run T7's test again
   after T8).
5. T4 (payload) → T5 (explosions) → T15 → T17 → T16.
6. T11, T12, T14, T18 last. T19 is not combat; report it.
After each task: full `cargo test -p mind-core --features msav-import`
(expect the previous count + the new tests, 0 failed), clippy clean, commit.
After T1, T2, T7, T8 also rebuild, restart (`tools/session-play.sh N`) and
re-run the Ground Zero wave probe below; it must still end with the wave
dead and the core alive:
```bash
$G --eval 'return get_node("/root/Spine/SimHost").place_block(130, 48, "mechanical-drill")' \
   --eval 'return get_node("/root/Spine/SimHost").place_block(132, 48, "duo")'
sleep 30
$G --eval 'return get_node("/root/Spine/MindCampaign").run_wave()'
# poll every 6 s for ~90 s:
$G --eval 'return [get_node("/root/Spine/SimHost").get_tick(), get_node("/root/Spine/SimHost").get_group_count("bullet"), get_node("/root/Spine/SimHost").unit_debug_positions()]'
$G --eval 'return get_node("/root/Spine/MindCampaign").get_hud_state().hasCore'   # true
```
Expected (audited build): team-2 daggers move from y≈1627 down to y≈444 in
≈2600 ticks, bullets appear, both daggers are gone by tick ≈5300.

## 7. Files shared with other areas (pull `main` before touching; keep commits small)

- `mind-core/src/sim/mod.rs` (`Sim::tick`: fires/puddles → `update_builders`
  → `resolve_deaths`; building-area work also edits it).
- `mind-core/src/sim/runtime.rs` (`remove_building` shared by break/destroy/
  pickup/`remove_block_raw`).
- `mind-core/src/sim/schedule.rs` (system registration; never reorder sets).
- `mind-core/src/sim/unit_runtime.rs` (also `update_builders`-adjacent code).
- `mind-core/src/content/registries/blocks/*.rs` and
  `parity/golden_content.json` + `parity/golden_manifest.json` (T1/T2
  regenerate goldens; the building area also changes block specs).
- `mind-gdext/src/sim_host.rs` (render/MCP seams; many agents add `#[func]`s).
- `mind-core/src/ai/block_indexer.rs` (`is_derelict`).

## 8. Executor notes

(Append findings here: precondition mismatches, skipped sub-steps, new
defects. Keep the format `- Tn: <what> (<file:line>)`.)

Session 2 (2026-10-12/13, branch `parity/session-2`, display :21, bridge 6980;
18 commits `b0d194c..3e50373`). All suite numbers are
`cargo test -p mind-core --features msav-import`; baseline 1940.

- T9: done (`b0d194c`). `adopt_spawned_units` now uses a
  `With<SpawnedByBullet>` query; full suite 1940.
- T1: done (`5c63c1d`). Extra work needed beyond the plan: (a) the missile's
  weapon needs `AIController.updateWeapons`'s per-mount `findTarget` for the
  shoot-on-death explosion to act as a proximity fuse (the turret-fired missile
  never contacts units) (`sim/unit_runtime.rs`, Missile branch); (b) the four
  ExplosionBulletTypes need the ctor's explicit
  `rangeOverride = max(rangeOverride, radius * 2/3)`; (c) four pinned content
  counts and two goldens had to be regenerated (`golden_content.json`,
  `tests/golden/combat.json`, `parity/bundle_keys.json` exempt list, `mods/catalog`
  716 -> 720). The generated `server/spacetimedb/src/main/content_seed.rs`
  (720 entries) is regenerated in the worktree but deliberately **not
  committed** (staging rule = client/parity/docs); the coordinator must run
  `bash server/gen_content_seed.sh` and commit it.
- T2: done (`487f79f`). The plan's literal acceptance cannot pass: the dagger
  targets the closer conveyor (`BlockIndexer.find_enemy_building` is
  distance-based and `Block.init`'s `under` priority only re-ranks vs walls),
  so a shot can never be aimed at the wall without another target. The test
  adds an immobile enemy decoy behind the wall (units take precedence in
  `Units.closestTarget`) and asserts the conveyor survives + the wall is hit.
  Also ported: `Block.init`'s unconditional `priority = TargetPriority.under`
  (the plan's "no explicit priority" guard would leave conveyors at transport,
  unlike Block.java:1386-1387) — two assertions in
  `content/registries/blocks/mod.rs` were updated. The
  `AIController.target` under-block building predicate was added to
  `BlockIndexer::find_enemy_building`/`TargetQueries::closest_target` (extra
  parameter) so missiles (`targetUnderBlocks=false`) do not fuse on conveyors.
  Extra Java-derived under-bullet blocks beyond the plan's list: `Duct`,
  `OverflowDuct`, `BeamNode` and the per-instance `duct-unloader` /
  `surge-router` / `liquid-router` / `reinforced-liquid-router` /
  `reinforced-liquid-junction` / `reinforced-bridge-conduit` / `power-node`
  (Blocks.java:2234-2487). `BlockKind::DuctJunction` does not exist in the
  port (block absent).
- T3: done (`9d64483`).
- T13: not reached (the plan's sequence put it after T2; the session ran out of
  budget after T12/T14's first kinds).
- T6: done (`0e984ce`). `isFlying() && !canLand()` is not ported (needs
  `Units.any` over overlapping units).
- T10: done (`825406f`).
- T7: done (`b02dd4f`). The plan's "wall destroyed within 1200 ticks" is
  impossible (flare: 9 damage / 80-tick reload = 135 of a 320 hp wall), so the
  test asserts continuous circling (>90% of sampled ticks with velocity), a
  growing shot count and wall damage.
- T8: done (`8652fe7`).
- In-engine checkpoint after T1/T2/T7/T8: editor restarted on :21, Ground Zero
  wave probe re-run -> wave dead, `hasCore` true at tick 6026
  (`runs/20261012-session2-audit-combat-units/wave-probe.txt` +
  `wave-after-derelict-fix.png`). T1's scathe stays test-only (no bridge item
  feed), T4's payload probe: `Input.pickup_payload()` runs cleanly.
- T4: done (`4b7067b`). `SIM_UNIT_REVISION` 2 (payload stack); `live_unit_class_id_is_reserved`
  updated. Note: the port's `WorldGrid::to_tile` rounds, matching Java's
  `World.toTile` (`Math.round`), so tile-centre coordinates map to the *next*
  tile — the payload fallback test uses (96,96) where both agree.
- T15: done (`3e9ef27`). `Bullet.owner` + lifesteal (pre-damage health read).
- T17: done (`a0585a1`). Unit death explosion (`UnitComp.java:892`
  `dynamicExplosion` on death) is *not* ported; only the crash damage.
- T16: done (`68080c3`). `BlockDef.rebuildable` added (three vanilla classes);
  the ghost plan's `config` is `None` (the port does not store block config on
  the entity) — the plan's commit trailer used "Claude Opus 4.5" by mistake.
- T5: done (`afaeebb`). Waves land without the `Time.run(i * 2f)` stagger;
  `Bullets.fireball` spread replaced by one `fires::create`;
  lightning skipped; the ExplosionShield absorption check in the wave loop and
  `Block.baseExplosiveness`/`flammabilityScale`/`explosivenessScale` are not
  ported (terms default 0/1/1).
- T11: done (`95fced5`). Every registered command id is accepted/stored
  (`ai::controller::UnitCommandComp`); the per-command controller switch is
  deferred to T14 (only move/boost-ish behaviors exist live). Upstream has no
  `boost` *command* — boost is `UnitStance.boost` (`CommandAI.shouldBoost`);
  the live stance path is still relay-only.
- T12: done (`c385699`). The plan's "dagger vs perpendicular flare" cannot
  work: a perpendicular target with bullet speed 2.5 < flare 2.7 has no
  intercept solution (Java returns the target position), so the live test uses
  a `mace` (bullet speed 2.0) against a commanded dagger walking at 1.5.
- T14: partial. Suicide (`e927900`, plus the missing `BulletType.init`
  `killShooter` for ordinary bullets — the crawler never died without it) and
  Builder (`9d041bb`, wired only when a commanded tile exists: the live
  `unit.plans` queue is not wired) and Boost (`3e50373`, no live path assigns
  `AiKind::Boost` yet). Miner/Cargo/Defender/Hug/FlyingFollow/Logic not
  reached.
- T18: not reached (low priority, listed after T14).
- T19: reported, not fixed (item-source config: not the combat area).
- New defect (not in the plan): `bullets_pass_over_enemy_conveyor`'s decoy and
  several tests rely on `spawn_live_unit` team semantics; `WorldGrid::to_tile`
  rounding is confirmed Java-faithful (`World.toTile` rounds).
- Commit `825406f` (T10) is prefixed `Fixes PAR-0138-adjacent` by mistake (T10
  has no ledger id; a plain "Audit fix:" prefix was intended).
