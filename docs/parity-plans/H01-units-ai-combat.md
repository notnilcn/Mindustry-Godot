<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H01 — Units, AI and combat simulation (round H, 2026-10-14)

Area: the unit/AI/combat **simulation** (`core/sim/unit_runtime.rs`, `core/ai/**`,
`core/entities/comp/unit/**`, `core/combat/**`, `core/weapons/**`). Input-side work is H08,
drawing is H03, block behaviour is H02, saves/campaign loop is H04, campaign UI is H05, logic
(incl. `LogicAI`) is H06, weather/audio is H07, the uncovered sweep is H09.

**Read `docs/H-environment.md` first** (session/worktree, build, tests, goldens, gmcp eval rules,
Ground Zero recipe, seams table, commit rules), then `docs/H-INDEX.md` §3, then this file. This
file only references those sections (`env §N`). Path abbreviations (`core/`, `gdext/`,
`headless/`, `ui/`, `J/`, `ARC/`) are the ones defined in `env`.

**Start state.** `main` contains H00: `git log main --oneline | grep -m1 'H00'` prints a line.
After H00 the Serpulo `rules.env` is 233 and the player alpha spawns on Ground Zero by itself
(`$H.has_player_unit()` -> `true` right after `start_sector`), and player `Place{player:Some}`
commands queue like Java. If either is not true in your session, stop and report.

**Drift rule.** Java line numbers below were read on 2026-10-14. Port line numbers drift: re-locate
every port symbol with `grep -n` before editing. If the code you find differs from what a task says
(symbol missing, behaviour already different, a test value does not match the derivation), stop
**that task**, write down what you saw in your summary, and continue with the next independent task.
Do not improvise designs.

Shorthand used below:

* `RT` = `core/sim/unit_runtime.rs` (live unit runtime: `unit_update_system`, `resolve_deaths`,
  `update_ground_ai`, `update_flying_ai`, `acquire_target`, `set_mount_targets`, spawn/possess).
* `CT` = `core/sim/combat_tests.rs` (helpers `live_sim(size) -> Sim` — block + unit runtime and
  **no** `CampaignRuntime`; `place(&mut sim, x, y, "name", team) -> Entity`; `Sim::spawn_live_unit
  (name, team, x, y, rot) -> Option<Entity>`). To get a campaign session copy the setup of
  `player_respawns_after_death` / `live_unit_command_switch_is_stored` in the same file
  (`CampaignRuntime::new(PlaySession::new(Rules { default_team: 1, .. }), campaign)`).
* `L="flock /tmp/opencode/mg-build.lock"; export CARGO_BUILD_JOBS=6` (env §2), run from
  `WT/client/rust`.
* gmcp shorthands `G`, `H`, `I`, `C`, `MC` as in env §4.

---

## §1 Inventory (Java -> port -> status after round G)

Legend: **OK** ported + live; **P** partial/divergent; **M** missing; **NW** ported, not wired live.

### 1.1 Movement, physics, coordinates

| Java | Port | Status | Task |
|---|---|---|---|
| `Tile.worldx() = x*tilesize`, `Building.set(tile.drawx(), tile.drawy())` (`J/world/Tile.java:166-177`, `J/entities/comp/BuildingComp.java:126`), `World.toTile = Math.round(c/8)` (`J/core/World.java:174`) | units/flowfield use Java convention (`WorldGrid::to_tile` = round); building `Pos` = `(tile+0.5)*8` for **every** size (`core/world/update.rs` `BlockInstance::spawn`); bullet->building raycast uses `floor(v/8)` (`core/combat/bullet/mod.rs` `collide_bullet`); fires/puddles/lightning/AI helpers use `(t+0.5)*8` | **P (high)** | T1 |
| `AIController.updateWeapons` (`J/entities/units/AIController.java:172-257`): per-mount `findTarget`, target `hitSize/2`, `singleTarget`, `controllable/noAttack/aiControllable`, `autoDropBombs`, rotate-back | `RT::set_mount_targets` (one target for all mounts, shooter's own hitSize, no flags) | **P (high)** | T2 |
| `FlyingAI.findTarget` (`J/ai/types/FlyingAI.java:34-39`) | `RT::update_flying_ai` mount target: condition **inverted** | **P (high)** | T2 |
| `Weapon.update` auto-target search for `!controllable && autoTarget` (`J/type/Weapon.java:324-349`) | `core/weapons/mod.rs::update_weapon` keeps a stored target, never searches | **P** | T2 |
| `VelComp.update` + `EntityCollisions.move` + `UnitComp.moveAt/movePref/rotateMove` + drag (`J/entities/comp/VelComp.java:23-34`, `J/entities/EntityCollisions.java:36-104`, `J/entities/comp/UnitComp.java:101-153,790`) | kinematic: `core/ai/types/ground.rs::approach` sets `vel = dir*speed` and `pos += vel` in one step; flyers/player/missile integrate themselves without drag; no tile collision resolution; external `Vel` (knockback, wind, bounds warp, drop-zone push) discarded or hand-stepped | **P (high)** (PAR-0202) | T4 |
| `UnitComp.update` drag line 790, captured-sector AI death 750-753, `hasSpawns` gate 793 | `core/entities/comp/unit/environment.rs` (drag never set, captured death missing, push gated on `rules.waves` only) | P | T5 |
| `UnitComp.killed()` / falling wreck (`J/entities/comp/UnitComp.java:980-991, 802-830`) | `RT::resolve_deaths` destroys immediately | M | T6 |
| `PhysicsProcess` push | `RT::unit_physics_system` | OK | — |
| `AvoidanceProcess` + `getTargetTile(.., avoidanceId)` (`J/async/AvoidanceProcess.java`, `J/ai/Pathfinder.java:382-436`, `J/ai/types/GroundAI.java:54,63-84`) | stuck bookkeeping only, no avoidance grid | M | T11 |

### 1.2 Player lifecycle

| Java | Port | Status | Task |
|---|---|---|---|
| `PlayerComp.update/bestCore`, `CoreBlock.playerSpawn/requestSpawn` | `RT::update_player_respawn`, `best_core_for_player`, `spawn_player_at_best_core` | OK in code, never verified live (PAR-0174); `player_team` is only set by a successful spawn (no retry when the first spawn fails); `spawnType.unlock()` deferred | T3 |
| `InputHandler.unitClear` dock branch (`J/input/InputHandler.java:823-873`) | `RT::clear_unit` | **P (bug)**: the possessed unit keeps `AiKind::Player` + `Player` marker when docking | T3 |

### 1.3 Combat / damage

| Java | Port | Status | Task |
|---|---|---|---|
| `Damage.dynamicExplosion` lightning + fireballs (`J/entities/Damage.java:93-133`) | `core/combat/damage/explosion.rs::dynamic_explosion` (no lightning, one `fires::create`) | P | T7 |
| ExplosionShield test `ignoreTeam == null ? skip : indexer.getEnemy(ignoreTeam, shield)` + `contains` short-circuit | `explosion_absorbed`: checks shields when `ignore_team == None`, includes derelict, every absorbing projector takes buildup | **P (bug)** | T7 |
| `AIController.targetFlagActive` (`J/entities/units/AIController.java:276-279`) + `BuildingComp.status()` (`J/entities/comp/BuildingComp.java:638-652`) | `RT::target_flag_active` uses `items.any()` only | P | T8 |

### 1.4 AI controllers

| Java | Port | Status | Task |
|---|---|---|---|
| `GroundAI`, `FlyingAI` main-target, `CommandAI` (move/attack/mine/repair/rebuild), `MinerAI`, `RepairAI`, `BuilderAI` (rebuild), `SuicideAI`, `MissileAI`, `BoostAI`, stances holdFire/boost/pursueTarget/holdPosition/mine | `RT` branches + `core/ai/types/*` | OK (round G) | — |
| `DefenderAI` (oct), `FlyingFollowAI` (quell, disrupt), `HugAI` (renale, latum), `NoAI` (dummy) | `AiKind::{Defender,FlyingFollow,Hug,NoAi}` fall into the generic ground/flying branch of `unit_update_system` | **M (live)** | T9 |
| `BuilderAI` following/assistFollowing (`J/ai/types/BuilderAI.java:46-80,128-170`) | `core/ai/types/builder.rs` (deferred; `assist` command idles) | M | T12 |
| `CargoAI` (manifold), `AssemblerAI` (assembly-drone) | `core/ai/types/{cargo,assembler}.rs` not dispatched | NW | T13 |
| `RtsAI.estimateStats` turret term + unit AABB (`J/ai/RtsAI.java:329-364`) | `core/ai/rts_ai.rs::estimate_stats` + `core/game/runtime.rs::team_ai_enemy_stats` (units only; air/ground filter on units) | P (PAR-0156) | T10 |
| `BaseBuilderAI.update` core fill + `aiCoreSpawn` (`J/ai/BaseBuilderAI.java:51-71`), `Logic` `fillItems`/`prebuildAi` (`J/core/Logic.java:551-584`) | actions computed in `core/ai/base_builder_ai.rs`, ignored by `core/game/runtime.rs::run_team_base_builder` (`core_unit_count: 0`) | M (PAR-0156) | T10 |
| `PrebuildAI` behaviour, `randomWaveAI`, `unloadPayloads`, payload commands, `patrol`/`ram` stances, command queue | — | M | Deferred (§2 end) |
| `LogicAI` | H06 | — | — |

### 1.5 Done / verified — do not redo

Round G (DeepSeek) items verified by reading the code on 2026-10-14 (spot-checks marked ✔ were
re-read against Java for this plan):

* ✔ G01 T2 unit range / canAttack: the JVM-vs-port compare script (G01 plan header) prints only
  `MISSING turret-unit-build-tower` today.
* ✔ G01 T6a GroundAI (`RT::update_ground_ai`): core override `range/1.3 + size*4`, half-range stop,
  drop-zone stop, field walk via `pathfind_field` at `x*8` targets, 40/90 retarget. Matches
  `J/ai/types/GroundAI.java:21-85` except the avoidance flag (T11).
* ✔ G01 T6b `flying_main_target` matches `FlyingAI.findMainTarget`; its **mount** half is wrong (T2).
* ✔ G01 T8 respawn code (`RT::update_player_respawn`, `spawn_player_at_best_core`) matches
  `PlayerComp.update`/`CoreBlock.playerSpawn`; the unlock step was a documented deferral (T3).
* ✔ G01 T9 possession: `possess_unit` matches `unitControl`; `clear_unit` dock branch is buggy (T3).
* ✔ G01 T12 unit cap (`core/entities/comp/unit/queries.rs::can_create_unit`, factory gate in
  `core/world/blocks/units/behavior.rs`, cap death in `lifecycle.rs`).
* ✔ G01 T13 environment: bounds/drowning/floor status/env death/solid kill exist; drag, captured
  death and the hand-stepped warp/push are the T5 residuals.
* ✔ G01 T14: unit death `dynamicExplosion` + crash damage exist; lightning/fireballs/absorption
  semantics are T7.
* G01 T1, T3, T4, T5, T7, T10a, T10b, T10c (rebuild half), T11, T15, T16, T19 (summary claims; tests
  exist: `grep -n "fn poly_rebuilds\|fn mega_repairs\|fn boss_list" core -r`).
* PAR-0206 (player aims/fires from desktop input, G08 T1) is verified; the G00 smoke note "alpha
  cannot shoot" predates it and was caused by the missing player unit (H00).

---

## §2 Tasks

Each task = one commit. Order = §3. After every task: `$L cargo test -p mind-core --lib
--features msav-import` (previous count + new tests, 0 failed), `$L cargo test -p mind-headless
--lib` (only the H00-recorded known failures may fail), clippy clean, `cargo fmt` (env §2).

---

### T1 — Java world convention for building positions and bullet/fire/AI tile math

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. On Ground Zero `best_core_position()` (grid-derived, already Java `drawx`) = (1032, 440) while the alpha spawned from the sim core `Pos` sits at `player_unit_pos()` = **(1036, 444)** = (129.5*8, 55.5*8) (H00 Change 1 applied). Every port site listed under "Port today" exists at the quoted lines (`world/update.rs:96-97`, `bullet/mod.rs:964`, the `+ 0.5) *` and `floor()` lists match exactly). Text fixed below: `building_debug` prints NO position (keys: block/rotation/team/efficiency/...), and `best_core_position()` returns world **pixels**, not a tile.

**Symptom / acceptance.** The port mixes two world conventions. Units, the flowfield
(`pathfind_field` steers at `x*8`), `WorldGrid::to_tile` (`round`) and the gdext renderer
(`gdext/render/floor.rs` "tile (x, y)'s centre is `(x * tilesize, y * tilesize)`",
`gdext/render/building_cache.rs:212` `anchor = tile*8 + def.offset`) use Java's: **tile (x,y)
centre = (x*8, y*8), tile covers [x*8-4, x*8+4)**. But the sim gives every building
`Pos = ((x+0.5)*8, (y+0.5)*8)` (correct only for even sizes), and bullets look up the building tile
with `floor(v/8)`, i.e. tile x covers [x*8, x*8+8). Consequences: 1x1/3x3 buildings (walls, duo,
core-shard) sit 4 px off in both axes for targeting, range checks, turret muzzles and core spawns;
a bullet travelling along `y = 79` hits row 9 instead of row 10 — units can shoot "past" a wall
they are aiming at; fires/puddles/lightning use half-tile offsets. After: building `Pos` =
`tile*8 + def.offset`, bullet raycasts use `WorldGrid::to_tile`, all listed helpers use `x*8`.

**Java.**
* `J/world/Tile.java:166-177`:
  ```java
  public float worldx(){ return x * tilesize; }
  public float drawx(){ return block().offset + worldx(); }
  ```
* `J/world/Block.java:774` (and `:1450`): `offset = ((size + 1) % 2) * tilesize / 2f;`
  (port: `BlockDef.offset`, checked by `core/world/block.rs::assert_derivations`).
* `J/entities/comp/BuildingComp.java:126`: `set(tile.drawx(), tile.drawy());`
* `J/entities/comp/BulletComp.java:187`: `tileRaycast(World.toTile(lastX), World.toTile(lastY), tileX(), tileY());`
  with `J/core/World.java:174-176` `return Math.round(coord / tilesize);`
* `J/entities/Fires.java:26`: `fire.set(tile.worldx(), tile.worldy());`
* `AIController.getClosestSpawner()` returns a `Tile`; `unit.within(tile, r)` measures to
  `tile.getX() = tile.drawx()` = `x*8` for the (air) spawn overlay.

**Port today** (verify each with the grep; line numbers are 2026-10-14):
* `core/world/update.rs` `BlockInstance::spawn` (~96): `center_x = (tile.x() as f32 + 0.5) * TILE_SIZE`.
* `core/combat/bullet/mod.rs::collide_bullet` (~964): `let to_tile = |v: f32| (v / tile_size).floor() as i32;`
* `grep -rn "+ 0.5) \*" core/combat core/ai core/sim core/entities` lists the half-tile sites:
  `combat/lightning.rs:145-146`, `combat/fires.rs:110-111`, `combat/puddles.rs:123-124,322`,
  `combat/damage/line.rs:172-173`, `combat/bullet/kinds/mod.rs:59-60`, `combat/bullet/line_hit.rs:484`,
  `ai/wave_spawner.rs:105-106` (`player_near`), `ai/rts_ai.rs:215-216,950-951`,
  `ai/types/ground.rs:83-86` (`tile_center`), `sim/mod.rs:783-784` (`command_targets`),
  `sim/unit_runtime.rs` `closest_spawn_tile`, `within_spawn`, `tile_center` (~4341) and the FlyingAI
  spawner shuttle (`(spawn.x() as f32 + 0.5) * TILE_SIZE`, ~3955).
* `grep -rn "/ ts).floor()\|/ tile_size).floor()" core/combat` lists the floor lookups:
  `combat/lightning.rs:126-128`, `combat/bullet/kinds/mod.rs:43-45` (range-box bounds — keep `floor`
  there but widen by one tile, see step 4), `combat/bullet/kinds/liquid.rs:44`, `combat/bullet/kinds/fire.rs:49`.

**Changes.**
1. `core/world/update.rs` `BlockInstance::spawn`: replace the two centre lines with
   ```rust
   // `Building.set(tile.drawx(), tile.drawy())` (BuildingComp.java:126):
   // `drawx = tile.worldx() + block.offset` (Tile.java:166-177).
   let center_x = tile.x() as f32 * super::block::TILE_SIZE + self.def.offset;
   let center_y = tile.y() as f32 * super::block::TILE_SIZE + self.def.offset;
   ```
   (`self.def` is the `BlockDef` already used in that function; check the field name with
   `grep -n "fn spawn" -A15 core/world/update.rs`.) Then `grep -rn "\.get_mut::<.*Pos>\|Pos {" core/world core/sim/runtime.rs`
   for any other place that re-centres a building (payload drop `core/world/blocks/payloads/payload_block.rs:339-340`
   is H02's — list it in your summary, do not change it).
2. `RT::building_center` fallback and `RT::tile_center`: return `(x*8, y*8)` (rename the helper
   `tile_world(tile)`; doc `Tile.worldx()`).
3. `core/combat/bullet/mod.rs::collide_bullet`: `let to_tile = |v: f32| crate::world::WorldGrid::to_tile(v);`
   (Java `Math.round`; note Rust `f32::round` rounds .5 away from zero — for the negative half-way
   case `-0.5` Java gives 0, Rust -1; both are out of bounds, so no special case is needed).
4. `combat/bullet/kinds/liquid.rs:44`, `combat/bullet/kinds/fire.rs:49`: tile under the bullet =
   `WorldGrid::to_tile(x)` (Java `Vars.world.tileWorld(x, y)` = `tile(Math.round(x/8), Math.round(y/8))`).
   `combat/lightning.rs:126-128` and `combat/bullet/kinds/mod.rs:43-45` scan a box: keep `floor` for
   the box bounds but compute them as `((x - range) / ts).floor() as i32 - 1 ..= ((x + range) / ts).ceil() as i32 + 1`
   so no tile is missed after the shift; the per-tile distance uses `bx = tx*8 + offset_of_that_build`
   — simpler: use the building's `Pos` (`ctx.world.get::<Pos>(build)`), which is now Java-exact.
5. `combat/fires.rs:110-111` -> `x: tx as f32 * ts, y: ty as f32 * ts` (Fires.java:26).
   `combat/puddles.rs:123-124,322`: read `J/entities/Puddles.java` `deposit` (`puddle.set(ax, ay)`
   with `ax = (tile.worldx() + source.worldx()) / 2f` when `source != tile`, else `tile.worldx()`) and use
   `x*8` worlds; keep the existing averaging.
6. `combat/damage/line.rs:172-173`, `combat/bullet/line_hit.rs:484`: use the building `Pos` of the
   hit build instead of the tile centre (Java `Damage.collideLine` `build.x/build.y`; `findLength`
   uses `tile.worldx()`; read the surrounding Java comment cited in the function doc and match it).
7. `ai/wave_spawner.rs:105-106`, `RT::closest_spawn_tile`, `RT::within_spawn`, the FlyingAI spawner
   shuttle, `ai/rts_ai.rs:215-216,950-951`, `sim/mod.rs:783-784`, `ai/types/ground.rs::tile_center`:
   `(x*8, y*8)`. For `ground.rs::tile_center` keep the function name, change the body and doc to
   `Tile.worldx()`; its callers (`update_ground`, `update_flying`, `ai_controller.rs`) then steer
   at Java tile centres.
8. Re-run every test; fix tests that **hard-coded** the old centre for odd-size buildings
   (e.g. `(x + 0.5) * 8` for a 1x1/3x3 block) to `x*8 + offset`. Do not change
   `BuildHarness::tile_center`/`CombatHarness::tile_center` (they are used as generic "somewhere in
   this tile" points); if a test fails because it put a unit at `tile_center` expecting the building
   centre, switch that test to the building's `Pos`. List every edited test in the commit body.
9. Goldens: run `$L cargo test -p mind-headless --lib` and `cargo run -q -p mind-headless -- parity goldens`.
   Regenerate only the unit/combat goldens that changed (env §2 rules; name each with the reason
   "building Pos = drawx/drawy (BuildingComp.java:126); bullet tileRaycast uses World.toTile").

**Tests** (new, end of `CT`):
* `building_pos_uses_block_offset`: `live_sim(64)`; `place(.., 10, 10, "duo", 1)` -> `Pos == (80.0, 80.0)`;
  `place(.., 20, 20, "core-shard", 1)` -> `(160.0, 160.0)`; `place(.., 30, 30, "mechanical-drill", 1)` ->
  `(244.0, 244.0)`; `place(.., 40, 40, "core-foundation", 1)` -> `(324.0, 324.0)`.
* `bullet_row_lookup_uses_round` (in `core/combat/bullet/mod.rs` tests, `CombatHarness::new(48, 32, 5)`):
  `harness.place(11, 10, copper_wall, 0, true)` (team = harness default team, check
  `world/harness.rs::place`: `self.rules.default_team`); spawn a basic bullet of a **different** team
  (pick a name from `harness.names_map()` whose def kind is basic, e.g. the duo copper bullet; print the
  map once to choose) at `(100.0, 79.0)` angle `180.0`; tick 30; assert
  `harness.building_health_at(11, 10) < <full health>`. Derivation: Java row `round(79/8) = 10`;
  the old `floor` gives row 9 and the bullet passes under the wall.
* `fire_sits_on_tile_world`: `fires::create(world, 5, 7, rng)` -> fire `Pos == (40.0, 56.0)`.

**Verify in-engine.** Ground Zero (env §4.1). `building_debug` prints no position (verified
2026-10-14), so read the core-spawned alpha instead: before T1 `player_unit_pos()` = `(1036, 444)`,
after T1 it must equal `best_core_position()` = `(1032, 440)` ± 3 (world **pixels**; core-shard
`drawx` = 129*8; `best_core_position()` is grid-derived and already reads `(1032, 440)` before T1).
`has_player_unit()` true. Screenshot
`runs/<date>-s<N>-H01-T1/duo.png` centred on (132,48): the duo sprite and its range circle (H03)
coincide.

**Pitfalls.** Power-node laser range (`core/world/blocks/power/nodes.rs:70-101`) and
`core/world/blocks/power/explosion.rs:50-51` still use `(x+0.5)*8` — H02's; do not touch, list them.
The physics push in `RT::unit_physics_system` and `environment.rs` solid checks use `to_tile`
already. A golden checksum change is expected; never "fix" a golden by reverting step 1.

**Commit.** `Gameplay H01 T1: Java world convention for building positions and bullet tile lookups`

---

### T2 — `AIController.updateWeapons` per-mount fidelity (+ FlyingAI `findTarget`, `autoDropBombs`)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED for (c) and (d). (d): a team-2 horizon spawned at (1150,500) or (1250,440) flew over the conveyor line (140..144,60) at y≈470-482 with mount `total_shots` 0 and every belt at health 1.0 (`runs/20261014-s2-verify/H01-T2-horizon.png`). (c): the inverted `match` is at `unit_runtime.rs:3896-3905` as described. Fixed wrong numbers in (b) and the tests: the dagger weapon range is **150** (bullet speed 2.5 × lifetime 60, `UnitTypes.java:107-117`), not 60 — Java engages a core-shard at 150 + 12 = 162 px, the port at 150 + 4 = 154 px. Note: with the plan's own spawn point (1150,500) the horizon sometimes heads straight to the core and never crosses the belts; read the horizon mounts' `total_shots` too.

**Symptom / acceptance.**
(a) Every AI mount gets the unit's single target; Java gives each controllable mount its own
`findTarget(mountX, mountY, weapon.range(), bullet.collidesAir, bullet.collidesGround)`
(non-`singleTarget` units), so ground-only weapons never waste shots on flyers.
(b) The shoot range adds the **shooter's** `hitSize/2`; Java adds the **target's** (`Sized.hitSize()/2`,
for a building `size*8`). A dagger vs a core-shard: Java `150 + 12`, port `150 + 4` (dagger bullet range = speed 2.5 ×
lifetime 60 = 150, `UnitTypes.java:107-117`; corrected 2026-10-14, the plan said 60).
(c) FlyingAI mount target is inverted: Java keeps the main target when it is valid and in range and
only falls back to the closest target when the main target is invalid/out of range; the port does
the opposite.
(d) `autoDropBombs` (horizon, `J/content/UnitTypes.java:1102`, and the second user at `:1482`) never
drops bombs on whatever is under it.
(e) Non-controllable / `noAttack` / `!aiControllable` mounts are overwritten by the AI (Java skips
them); `!controllable && autoTarget` weapons never search for a target.
(f) `acquire_target` adds a non-Java fallback ("nearest enemy building within range + 16").
After: a wave horizon circling a player conveyor line damages it; a dagger starts shooting a
core-shard at 162 px; a flare keeps shooting the core it is circling.

**Java** (`J/entities/units/AIController.java:172-257`, transcribed):
```java
public void updateWeapons(){
    float rotation = unit.rotation - 90;
    boolean ret = retarget();
    if(ret){ target = findMainTarget(unit.x, unit.y, unit.range(), unit.type.targetAir, unit.type.targetGround); }
    noTargetTime += Time.delta;
    if(invalid(target)){ if(target instanceof Healthc h && !h.isValid()) targetInvalidated(); target = null; }
    else{ noTargetTime = 0f; }
    unit.isShooting = false;
    for(var mount : unit.mounts){
        Weapon weapon = mount.weapon; float wrange = weapon.range();
        if(!weapon.controllable || weapon.noAttack) continue;
        if(!weapon.aiControllable){ mount.rotate = false; continue; }
        float mountX = unit.x + Angles.trnsx(rotation, weapon.x, weapon.y), mountY = unit.y + Angles.trnsy(rotation, weapon.x, weapon.y);
        if(unit.type.singleTarget){ mount.target = target; }
        else{
            if(ret){ mount.target = findTarget(mountX, mountY, wrange, weapon.bullet.collidesAir, weapon.bullet.collidesGround); }
            if(checkTarget(mount.target, mountX, mountY, wrange)){ mount.target = null; }
        }
        boolean shoot = false;
        if(mount.target != null){
            shoot = mount.target.within(mountX, mountY, wrange + (mount.target instanceof Sized s ? s.hitSize()/2f : 0f)) && shouldShoot();
            if(unit.type.autoDropBombs && !shoot){
                if(bomberTarget == null || !bomberTarget.isAdded() || !bomberTarget.within(unit, unit.hitSize/2f + ((Sized)bomberTarget).hitSize()/2f)){
                    bomberTarget = Units.closestTarget(unit.team, unit.x, unit.y, unit.hitSize, u -> !u.isFlying(), t -> true);
                }
                shoot = bomberTarget != null;
            }
            Vec2 to = Predict.intercept(unit, mount.target, weapon.bullet);
            mount.aimX = to.x; mount.aimY = to.y;
        }
        mount.shoot = mount.rotate = shoot;
        if(!shouldFire()) mount.shoot = false;
        unit.isShooting |= mount.shoot;
        if(mount.target == null && !shoot && !Angles.within(mount.rotation, mount.weapon.baseRotation, 0.01f) && noTargetTime >= rotateBackTimer){
            mount.rotate = true;
            Tmp.v1.trns(unit.rotation + mount.weapon.baseRotation, 5f);
            mount.aimX = mountX + Tmp.v1.x; mount.aimY = mountY + Tmp.v1.y;
        }
        if(shoot){ unit.aimX = mount.aimX; unit.aimY = mount.aimY; }
    }
}
public boolean checkTarget(Teamc target, float x, float y, float range){ return Units.invalidateTarget(target, unit.team, x, y, range); }
public Teamc findTarget(x, y, range, air, ground){ return target(x, y, range, air, ground); }   // AIController default
public Teamc target(x, y, range, air, ground){ return Units.closestTarget(unit.team, x, y, range, u -> u.checkTarget(air, ground), t -> ground && (unit.type.targetUnderBlocks || !t.block.underBullets)); }
```
`Units.invalidateTarget` (`J/entities/Units.java:151-157`):
```java
return target == null || (range != Float.MAX_VALUE && !target.within(x, y, range + (target instanceof Sized hb ? hb.hitSize()/2f : 0f)))
    || (target instanceof Teamc t && t.team() == team) || (target instanceof Healthc h && !h.isValid())
    || (target instanceof Unit u && !u.targetable(team));
```
`FlyingAI.findTarget` (`J/ai/types/FlyingAI.java:34-39`):
```java
var result = findMainTarget(x, y, range, air, ground);
//if the main target is in range, use it, otherwise target whatever is closest
return checkTarget(result, x, y, range) ? target(x, y, range, air, ground) : result;
```
`rotateBackTimer = 60f * 5f` (`J/entities/units/AIController.java:22`).
`BuildingComp implements ... Sized` (`J/entities/comp/BuildingComp.java:54`), `hitSize()` = `tile.block().size * tilesize` (`:2054-2056`).
`Weapon.update` auto-target (`J/type/Weapon.java:324-349`) and `findTarget` (`:453-455`):
```java
if(!controllable && autoTarget){
    if((mount.retarget -= Time.delta) <= 0f){
        mount.target = findTarget(unit, mountX, mountY, bullet.range, bullet.collidesAir, bullet.collidesGround);
        mount.retarget = mount.target == null ? targetInterval : targetSwitchInterval; }
    if(mount.target != null && checkTarget(unit, mount.target, mountX, mountY, bullet.range)) mount.target = null;
    boolean shoot = false;
    if(mount.target != null){
        shoot = mount.target.within(mountX, mountY, bullet.range + Math.abs(shootY) + (mount.target instanceof Sized s ? s.hitSize()/2f : 0f)) && can;
        if(predictTarget){ Vec2 to = Predict.intercept(unit, mount.target, bullet); mount.aimX = to.x; mount.aimY = to.y; }
        else{ mount.aimX = mount.target.x(); mount.aimY = mount.target.y(); } }
    mount.shoot = mount.rotate = shoot; }
protected Teamc findTarget(Unit unit, float x, float y, float range, boolean air, boolean ground){
    return Units.closestTarget(unit.team, x, y, range + Math.abs(shootY), u -> u.checkTarget(air, ground), t -> ground && (unit.type.targetUnderBlocks || !t.block.underBullets)); }
```
`PointDefenseWeapon`, `PointDefenseBulletWeapon`, `RepairBeamWeapon` override `findTarget`; the port
has their own modules (`core/weapons/{point_defense_weapon,point_defense_bullet_weapon,repair_beam_weapon}.rs`).

**Port today.** `RT::set_mount_targets(world, content, entity, target, core_override)` (~4422):
loops all mounts, sets `mount.target = chosen` for every mount, range check from the unit centre with
`weapon.range() + hit_size / 2.0` where `hit_size` is the **shooter's** `HitboxComp`. Callers: every
`AiKind` branch in `unit_update_system` (grep `set_mount_targets(`). `RT::update_flying_ai` builds
`mount_target` with the inverted `match` (~3893). `RT::acquire_target` (~4237) returns
`(anchor_tile, mount_target)` with the anchor fallback. `TargetQueries::closest_target(world, team, x,
y, range, air, ground, true, target_under_blocks)` (`core/combat/targeting.rs`) = `Units.closestTarget`.
Weapon fields: `WeaponDef.{controllable, ai_controllable, no_attack, auto_target, x, y, shoot_y,
base_rotation, target_interval, target_switch_interval, predict_target?}` (`core/content/registries/units/weapon.rs`;
grep each; `predict_target` may not exist — if absent, treat as `true`, Java default). Unit fields
`UnitTypeDef.{single_target, auto_drop_bombs, hit_size, target_under_blocks}`. `WeaponMount.{target,
aim_x, aim_y, shoot, rotate, rotation, retarget}` (`core/weapons/mount.rs`).
`core/weapons/mod.rs::update_weapon` (~383): the `!controllable && auto_target` block keeps a stored
target and only counts `retarget` down.

**Changes.**
1. New file-private helper in `RT`:
   ```rust
   /// `Sized.hitSize()` of a target: unit `HitboxComp.hit_size`, building `size * 8`
   /// (BuildingComp.hitSize), else 0.
   fn target_hit_size(world: &World, content: &ContentRegistry, target: Entity) -> f32
   ```
   and
   ```rust
   /// `Units.invalidateTarget(target, team, x, y, range)` (Units.java:151-157);
   /// `range = f32::MAX` skips the distance term.
   fn invalidate_target(world: &World, content: &ContentRegistry, team: u8, target: Option<Entity>, x: f32, y: f32, range: f32) -> bool
   ```
   (reuse `invalid_target` for the team/valid/targetable part; add the distance term with
   `range + target_hit_size/2`).
2. Replace `set_mount_targets` by
   ```rust
   pub(crate) struct AiWeaponCtx { pub main_target: Option<Entity>, pub retarget: bool, pub flying_find: bool, pub core_override: Option<Entity> }
   pub(crate) fn update_ai_weapons(world: &mut World, content: &ContentRegistry, queries: &TargetQueries, entity: Entity, team: u8, ctx: AiWeaponCtx)
   ```
   implementing the Java loop above in order: skip `!controllable || no_attack`; `!ai_controllable` ->
   `mount.rotate = false; continue`; mount position `unit + trns(rotation - 90, weapon.x, weapon.y)`
   (copy `trnsx/trnsy` from `core/weapons/mod.rs:214-222` or make them `pub(crate)`);
   `single_target` -> `mount.target = main_target`; else on `retarget` -> `mount.target =` the
   per-mount `findTarget` (for FlyingAI kinds: `flying_find = true` -> `FlyingAI.findTarget` above with
   `findMainTarget = flying_main_target(...)` at the **mount** position and range `wrange`; for the
   others `queries.closest_target(world, team, mx, my, wrange, bullet.collides_air,
   bullet.collides_ground, true, def.target_under_blocks)`), then `invalidate_target(.., wrange)`
   clears it; shoot = within `wrange + target_hit_size/2` **from the mount position** and
   `!hold_fire` (existing `CommandAI.shouldFire` stance check) and the unit's `shouldShoot`
   (BuilderAI: `!is_building`, see `ai/types/builder.rs` if a helper exists; else `true`);
   `auto_drop_bombs && !shoot` -> bomber target per Java (store `bomber_target: Option<Entity>` in a new
   component `AiWeaponsState { bomber_target, no_target_time }` inserted lazily) using
   `queries.closest_target(world, team, x, y, def.hit_size, false /*air*/, true /*ground*/, true, true)`
   (Java predicate `u -> !u.isFlying()` and buildings `t -> true`); aim via the existing
   `intercept_positions` call; rotate-back block with `no_target_time` and `rotateBackTimer`.
   The GroundAI core override keeps its semantics: when `core_override` is `Some`, mounts whose bullet
   `collides_ground` get `mount.target = core` **before** the loop (GroundAI.java:29-35 runs in
   `updateMovement`, i.e. before `updateTargeting` of the next tick — set it and skip the per-mount
   search for those mounts this tick).
3. Retarget flag: `update_ground_ai` / `update_flying_ai` already compute the 40/90 timer — pass
   `retarget = (timer fired this tick)`. Command / Builder / Suicide / Repair branches have no timer:
   add `retarget: f32` to a small `CommandAiState`-like component or reuse `AiWeaponsState`
   (`timer.get(timerTarget, target == null ? 40 : 90)`, seeded 39 like `GroundAiState`) and compute
   their main target with `queries.closest_target(unit.x, unit.y, max_range, target_air,
   target_ground, ...)` on retarget only (CommandAI inherits `AIController.findMainTarget` ->
   `findTarget` -> `target`). Delete `acquire_target`'s anchor fallback (`nearest_enemy_building`
   + `range + 16`) and the anchor return value; keep `nearest_enemy_building` only if another caller
   remains (grep).
4. `RT::update_flying_ai`: delete the inverted `match` and pass `flying_find = true`.
5. `core/weapons/mod.rs::update_weapon` `!controllable && auto_target` block: on `retarget <= 0`
   search `closest_target(mount_x, mount_y, bullet.range + |shoot_y|, bullet.collides_air,
   bullet.collides_ground, true, def.target_under_blocks)` — `update_weapon` has a `CombatCtx`, not
   `TargetQueries`; build a one-off query with `crate::combat::targeting::TargetQueries::build(ctx.world, ctx.content)`
   **only when the timer fires** (cost) — **but** first check whether the weapon kind is a
   point-defense / repair-beam weapon (they have their own search in their modules: grep
   `fn update` in those files); apply the generic search only to plain weapons. Then the
   invalidate/shoot/aim lines per Java (`predict_target` false -> aim at target pos).
6. Keep `RT::face_target` as is.

**Tests** (`CT`, new):
* `ground_weapon_ignores_air_target_per_mount`: campaign session (waves true, default 1, wave team 2);
  team-2 `dagger` at (200,200); team-1 `flare` at (230,200) (flying, `ControllerSlot.kind = NoAi` so it
  hovers) and a team-1 `copper-wall` at tile (29,25) (Pos (232,200)). Tick 120: the dagger mount target
  is the wall (dagger bullet `collidesAir` true in Java? — read `J/content/UnitTypes.java:100-140`; if the
  dagger bullet collides air, use `ripple`-like ground-only unit instead: pick a unit whose first weapon
  bullet has `collides_air == false` via `content.units().find(..)` and document which).
* `dagger_engages_core_at_target_hitsize`: dagger at distance `150 + 12 - 1 = 161` px (dagger weapon
  range 150 — read it from content: `def.weapons[0].range()`; derive `range + 24/2`) from a team-1
  core-shard centre; after the first retarget tick `mount.shoot == true`; at `range + 12 + 2` false.
* `flying_mount_keeps_in_range_main_target`: flare team 2 in range of a team-1 core-shard with a
  team-1 dagger closer: mount target stays the core (core-in-range wins in `findMainTarget`, and
  `findTarget` keeps a valid main target).
* `horizon_auto_drops_bombs_on_conveyor`: horizon team 2 at a team-1 `conveyor` line (no core in
  range, no factory) — tick 600, the conveyor tiles lost health.
* `non_controllable_mount_not_overwritten`: a unit with a `!controllable` weapon (find one with
  `content.units()` + `weapons.iter().any(|w| !w.controllable)`, e.g. a repair-beam unit) — after a
  tick with an enemy in range its mount target is not the AI main target unless its own search chose it.
Re-run `placed_duo_kills_enemy_dagger`, `ground_wave_paths_to_core_not_nearest_building`,
`horizon_prefers_factory`, `flare_falls_back_to_core` (names may differ: `grep -n "fn .*horizon\|fn .*flare" CT`).

**Verify in-engine.** Ground Zero; `$H.place_block(140,60,"conveyor")` ×5 along x=140..144;
spawn a team-2 horizon at world (1150, 500)
(`{"op":"spawn_unit","unit":<content_list("unit").find("horizon")>,"team":2,"px":1150,"py":500}`),
`set_paused(true)`, `step(600)`, read `building_debug(140,60)` (health) or `tile_debug` — the belt
lost health / is gone. Screenshot `runs/<date>-s<N>-H01-T2/horizon.png`. `grep -c panic` = 0.

**Pitfalls.** `TargetQueries` is built once per tick before the unit loop; the per-mount searches
reuse it (do not rebuild per mount). Keep `EntitySeq` determinism (closest-target tie-breaks are
already in `TargetQueries`). `Player` units never call this (player.rs sets mounts). Missile branch
keeps its own proximity search.

**Commit.** `Gameplay H01 T2: AI weapons pick per-mount targets like AIController.updateWeapons`

---

### T3 — Player lifecycle: live respawn check (PAR-0174), team retry, dock release, kill seam, unlock feed

> **Coordinator override (2026-10-14).** Change 4 (unlock feed) must use H04 T2's pipeline instead of a separate `PendingUnlocks` resource: add a variant `Unit(UnitId)` to H04's `UnlockRef` enum and push `UnlockRef::Unit(id)` into `CampaignRuntime.pending_unlocks` (same dedupe). If H04 T2 is not on `main` yet, create `pub enum UnlockRef { Item(ItemId), Liquid(LiquidId), Unit(UnitId) }` and `CampaignRuntime.pending_unlocks: Vec<UnlockRef>` in `core/game/runtime.rs` with exactly those names (H04 T2 then only fills the Item/Liquid producers). Do not add `Sim::drain_pending_unlocks`; the gdext consumer is H04 T2's `take_unlocked_content`. Adjust the test `campaign_spawn_queues_unlock` to read `pending_unlocks`.

> **In-engine verification (2026-10-14, session 2):** PARTLY. (a) With H00 Change 1 applied, PAR-0174's live respawn already works: the alpha (killed with `debug_apply_status(<id>, "melting", 2000)`, ~400 ticks) was back ~60 ticks after death (Java `deathDelay = 60`, `PlayerComp.java:36,237`). (b) code-confirmed (`update_player_respawn` returns early with no team; on main the alpha never spawns). (c) the dock branch only runs for `coreUnitDock` types, which are **evoke, incite, emanate** only (`UnitTypes.java:4355,4421,4500`) — the Ground Zero round trip below exercises the standard branch, and it works live (dagger `Player` -> `CommandAI`, a new alpha `Player`); to see (c) live use Onset (`erekir` 10, player unit evoke) — not run here. (d) confirmed: no `debug_kill_unit`; until it lands, `debug_apply_status(seq, "melting", 2000)` kills an alpha. Test text fixed (the dock types).

**Symptom / acceptance.** (a) PAR-0174 was never verified live (env blocker until H00). (b) If the
first `spawn_player_unit` fails (no supported core yet), `Sim.player_team` stays `None` and
`update_player_respawn` never retries; Java's player always has a team and retries every 60 ticks.
(c) `clear_unit` dock branch (possessed non-core unit + docked `coreUnitDock` type) spawns the docked
unit but leaves the possessed unit with `AiKind::Player` and the `Player` marker (two "player"
units). (d) No seam can kill a unit for in-engine tests. (e) `CoreBlock.playerSpawn` unlocks the
spawned type in the campaign (`spawnType.unlock()`); the port drops it.
After: killing the player alpha on Ground Zero gives a new alpha at the core 60 ticks later;
`unit_clear` while possessing a dagger docks back cleanly.

**Java.**
* `J/entities/comp/PlayerComp.java:214-243` (`update`): dead -> `core = bestCore()`;
  `deathTimer += Time.delta; if(deathTimer >= deathDelay){ core.requestSpawn(self()); deathTimer = 0; }`
  (`deathDelay = 60f`: `grep -n "deathDelay" J/entities/comp/PlayerComp.java`).
* `J/entities/comp/PlayerComp.java:281-310` (`unit(Unit)`): `if(this.unit != null){ this.unit.resetController(); ... } this.unit = unit; if(unit != null){ unit.team(team); unit.controller(this); }`.
* `J/input/InputHandler.java:823-873` (`unitClear`), dock branch:
  ```java
  Unit unit = docked.create(player.team());
  unit.set(player.unit());
  if(player.unit().isFlying() && unit.type.flying){ Tmp.v1.trns(player.unit().rotation + 180f, player.unit().hitSize / 2f + unit.hitSize / 2f); unit.x += Tmp.v1.x; unit.y += Tmp.v1.y; }
  unit.rotation(player.unit().rotation);
  unit.controller(player);      // -> PlayerComp.unit(unit) -> old unit.resetController()
  unit.spawnedByCore(true);
  unit.add();
  return;
  ```
  standard branch: `player.clearUnit(); player.checkSpawn(); player.deathTimer = Player.deathDelay + 1f;`
* `J/world/blocks/storage/CoreBlock.java:95-125` (`playerSpawn`), lines 121-123:
  `if(state.isCampaign() && player == Vars.player){ spawnType.unlock(); }`.

**Port today.** `RT::spawn_player_unit` sets `player_team`; `RT::update_player_respawn` returns early
when `player_team` is `None`; `gdext/sim_host.rs::spawn_player_unit` (#[func], ~2580) calls
`self.sim.spawn_player_at_best_core(team)` with `team = last_sector_rules().default_team`.
`RT::clear_unit` dock branch (~1490-1520) calls `spawn_player_unit(&name, team, sx, sy, rotation)`
and returns without touching `current`. No kill seam (`grep -n "fn debug_kill" gdext/sim_host.rs`
shows only `debug_kill_building`).

**Changes.**
1. `Sim::set_player_team(&mut self, team: u8)` (RT, next to `spawn_player_unit`): sets
   `player_team = Some(team)`, `player_death_timer = 0.0`. gdext `spawn_player_unit`: call
   `self.sim.set_player_team(team)` **before** `spawn_player_at_best_core` (so a failed first spawn
   is retried by `update_player_respawn` every 60 ticks like Java). Keep the log lines.
2. `RT::clear_unit` dock branch: before `spawn_player_unit(...)`, release `current` exactly like the
   standard branch does (remove `Player`, `slot.kind = select_team_controller(...)`, `slot.target =
   None`). Order: compute everything you need from `current` first, release it, then spawn.
   Extract the release into `fn release_unit_to_ai(&mut self, entity: Entity, team: u8)` and use it
   in both branches and in `possess_unit`'s non-core-spawned `before` branch.
3. `Sim::debug_kill_unit(&mut self, seq: i64) -> bool`: find the unit by `EntitySeq`, set
   `Health.health = 0.0; dead = true` (same as `environment.rs::kill`), return whether found.
   gdext: `#[func] pub fn debug_kill_unit(&mut self, seq: i64) -> bool` next to `debug_kill_building`
   (debug seam; no game rule in gdext).
4. Unlock feed (provider half): World resource `PendingUnlocks(pub Vec<String>)` (new, in RT or
   `core/game/runtime.rs` next to `CampaignRuntime`); in `spawn_player_at_best_core`, after a
   successful spawn, when a `CampaignRuntime` exists and `session.is_campaign()`, push the unit name
   (no duplicates). `pub fn drain_pending_unlocks(&mut self) -> Vec<String>` on `Sim`. The consumer
   (gdext/campaign unlock store) is H05's — §6.
5. Do **not** add a respawn countdown seam (Java shows none).

**Tests** (`CT`): `player_team_set_without_core_retries_spawn` (live_sim + campaign; no core;
`set_player_team(1)`; tick 120 -> no player; place `core-shard` team 1; tick 60 -> player alpha
exists). `clear_unit_dock_releases_possessed_unit` (player alpha from a `core-nucleus`? — the docked
type must have `core_unit_dock`; find one with `content.units().filter(|u| u.core_unit_dock)` (Java:
evoke, incite, emanate — the only `coreUnitDock = true` types, `UnitTypes.java:4355,4421,4500`); possess a team-1
dagger, `clear_unit()` -> exactly one entity with `comp::Player`, the dagger's slot kind is
`Command`). `campaign_spawn_queues_unlock` (campaign session, `is_campaign()` true; spawn ->
`drain_pending_unlocks() == ["alpha"]`; second drain empty). Keep `player_respawns_after_death` green.

**Verify in-engine** (closes PAR-0174). Ground Zero (env §4.1):
```bash
$G --eval "return $H.has_player_unit()"                                  # true
$G --eval "return $H.player_unit_pos()"                                  # ~(1032,440)
$G --eval "return $H.unit_debug_json()"                                  # find the row with "kind":"Player" -> id P
$G --eval "$H.set_paused(true); return $H.debug_kill_unit(P)"            # true
$G --eval "$H.step(2); return $H.has_player_unit()"                      # false
$G --eval "$H.step(59); return $H.has_player_unit()"                     # true (60-tick death delay)
$G --eval "return $H.unit_debug_json()"                                  # new "alpha", kind "Player", near (1032,440)
```
Then the possession round trip (G01 T9 never ran live): spawn a team-1 dagger at (1060,440), read its
id D, `apply_sim_command_json("{\"op\":\"unit_control\",\"unit\":D}")`, `step(2)` -> the dagger row has
`"kind":"Player"` and no alpha row; `apply_sim_command_json("{\"op\":\"unit_clear\"}")`, `step(2)` ->
one alpha `Player` row, the dagger `CommandAI`. Screenshots `runs/<date>-s<N>-H01-T3/respawn.png`.
`grep -c panic` = 0.

**Pitfalls.** `PlayerInput` survives (resource). `update_player_respawn` despawns the dead entity:
keep that. Do not respawn without a core (game over is H04's).

**Commit.** `Fixes PAR-0174: player respawn retries without a team, dock release, kill seam, unlock feed`

---

### T4 — Velocity model: `VelComp.update`, `EntityCollisions.move`, `moveAt`/`movePref`, drag (PAR-0202)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (the wind seam exists, `entities/comp/weather.rs:132-141`, but every unit reads `vel (0,0)` and the alpha did not move in 600 ticks of an active sandstorm, wind (0.61,0.79)). FIXED the acceptance numbers: Java moves a unit `a/drag` px per tick at steady state with `a = force*intensity/mass` (`ParticleWeather.java:42-50`, sandstorm `force = 0.1`, `VelComp.update`); a dagger (drag 0.3, mass 64π) drifts only ≈ 0.0017 px/tick ≈ **1.0 px per 600 ticks**, so "> 4 px" was impossible; a flare (drag 0.04, mass 81π) drifts ≈ 0.0098 px/tick ≈ **5.9 px per 600 ticks**. In-engine check and test changed below to use those values.

**Symptom / acceptance.** Ground units are moved kinematically (`approach`: `vel = dir*speed;
pos += vel`) so every external velocity contribution is lost or must be hand-stepped: bullet
knockback (`core/combat/bullet/mod.rs:~1447 vel += v / mass`), weather wind (G07 T9, PAR-0202),
bounds warp and drop-zone push (`environment.rs` steps `pos` by hand), `impulse(0,3)` on core spawn,
the CommandAI idle branch zeroes `vel`. Flyers and the player integrate without drag; there is no
tile collision resolution (`EntityCollisions.move`), so a corner clip is "pushed out" by
`environment.rs`. After: one `vel_update` per unit per tick integrates `vel` with tile collisions and
drag; every controller only **adds** velocity (`moveAt`); a dagger standing in a sandstorm drifts;
knockback moves units.

**Java.**
* `J/entities/comp/VelComp.java:23-34`:
  ```java
  //velocity needs to be called first, as it affects delta and lastPosition
  @MethodPriority(-1) @Override public void update(){
      if(!net.client() || isLocal()){
          float px = x, py = y;
          move(vel.x * Time.delta, vel.y * Time.delta);
          if(Mathf.equal(px, x)) vel.x = 0;
          if(Mathf.equal(py, y)) vel.y = 0;
          vel.scl(Math.max(1f - drag * Time.delta, 0));
      } }
  void move(float cx, float cy){ SolidPred check = solidity(); if(check != null){ collisions.move(self(), cx, cy, check); }else{ x += cx; y += cy; } }
  ```
  `Mathf.equal(a, b)` = `|a - b| <= 0.000001` (`ARC/arc-core/src/arc/math/Mathf.java`, `FLOAT_ROUNDING_ERROR`).
* Solidity per kind: `ElevationMoveComp.java:14-16` `isFlying() || ignoreSolids() ? null : EntityCollisions::solid`;
  `LegsComp.java:40-42` `ignoreSolids() ? null : type.allowLegStep ? legsSolid : solid`;
  `CrawlComp.java:30-32` `legsSolid`; `WaterMoveComp.java:65-67`/`WaterCrawlComp.java:18-20`
  `isFlying() || ignoreSolids() ? null : waterSolid`. Port predicate: `core/ai/types/ground.rs::unit_can_pass`
  (already per kind) — solid = `!unit_can_pass`.
* `J/entities/EntityCollisions.java:36-104` (transcribe exactly):
  ```java
  private static final float seg = 1f, maxDelta = 1000f;
  public void move(Hitboxc entity, float deltax, float deltay, SolidPred solidCheck){
      if((Math.abs(deltax) < 0.0001f && Math.abs(deltay) < 0.0001f) || deltax != deltax || deltay != deltay) return;
      deltax = Mathf.clamp(deltax, -maxDelta, maxDelta); deltay = Mathf.clamp(deltay, -maxDelta, maxDelta);
      boolean movedx = false; entity.hitboxTile(r1); int r = Math.max(Math.round(r1.width / tilesize), 1);
      while(Math.abs(deltax) > 0 || !movedx){ movedx = true;
          moveDelta(entity, Math.min(Math.abs(deltax), seg) * Mathf.sign(deltax), 0, r, true, solidCheck);
          if(Math.abs(deltax) >= seg){ deltax -= seg * Mathf.sign(deltax); }else{ deltax = 0f; } }
      boolean movedy = false;
      while(Math.abs(deltay) > 0 || !movedy){ movedy = true;
          moveDelta(entity, 0, Math.min(Math.abs(deltay), seg) * Mathf.sign(deltay), r, false, solidCheck);
          if(Math.abs(deltay) >= seg){ deltay -= seg * Mathf.sign(deltay); }else{ deltay = 0f; } } }
  public void moveDelta(Hitboxc entity, float deltax, float deltay, int r, boolean x, SolidPred solidCheck){
      entity.hitboxTile(r1); entity.hitboxTile(r2); r1.x += deltax; r1.y += deltay;
      int tilex = Math.round((r1.x + r1.width / 2) / tilesize), tiley = Math.round((r1.y + r1.height / 2) / tilesize);
      for(int dx = -r; dx <= r; dx++){ for(int dy = -r; dy <= r; dy++){
          int wx = dx + tilex, wy = dy + tiley;
          if(solidCheck.solid(wx, wy)){
              tmp.setSize(tilesize).setCenter(wx * tilesize, wy * tilesize);
              if(tmp.overlaps(r1)){ Vec2 v = Geometry.overlap(r1, tmp, x); r1.x += v.x; r1.y += v.y; } } } }
      entity.trns(r1.x - r2.x, r1.y - r2.y); }
  ```
  `HitboxComp.hitboxTile` (`J/entities/comp/HitboxComp.java:69-76`): `size = Math.min(hitSize * 0.66f, 7.8f); rect.setCentered(x, y, size, size);`
  `Mathf.sign(f)` = `f < 0 ? -1 : 1` (`ARC/.../Mathf.java:198`), `Mathf.round(v)` = `(int)(v + 16384.5) - 16384`
  (floor(v + 0.5)). `Rect.overlaps` = `x < r.x + r.width && x + width > r.x && y < r.y + r.height && y + height > r.y`.
  `Geometry.overlap(a, b, x)` (`ARC/arc-core/src/arc/math/geom/Geometry.java:346-397`): centres `ax,bx,ay,by`;
  `nx = ax - bx, ny = ay - by`; `xoverlap = a.w/2 + b.w/2 - |nx|`; if `|xoverlap| > 0`: `yoverlap = a.h/2 + b.h/2 - |ny|`;
  if `|yoverlap| > 0`: if `|xoverlap| < |yoverlap|` -> `(n = (nx < 0 ? 1 : -1, 0), pen = xoverlap)` else
  `(n = (0, ny < 0 ? 1 : -1), pen = yoverlap)`; `m = max(pen, 0)`; return `(-m*n.x, -m*n.y)` (the `x` flag is unused).
* `J/entities/comp/UnitComp.java:101-153`:
  ```java
  public void moveAt(Vec2 vector, float acceleration){ Vec2 t = tmp1.set(vector); tmp2.set(t).sub(vel).limit(acceleration * vector.len() * Time.delta); vel.add(tmp2); }
  public void movePref(Vec2 movement){ if(type.omniMovement){ moveAt(movement); }else{ rotateMove(movement); } }
  public void moveAt(Vec2 vector){ moveAt(vector, type.accel); }
  public void rotateMove(Vec2 vec){ moveAt(Tmp.v2.trns(rotation, vec.len())); if(!vec.isZero()){ rotation = Angles.moveToward(rotation, vec.angle(), type.rotateSpeed * Time.delta * speedMultiplier); } }
  ```
* `J/entities/comp/PhysicsComp.java:18-30`: `mass() = hitSize*hitSize*PI`; `impulse(x, y){ float mass = mass(); vel.add(x / mass, y / mass); }`.
* `J/entities/comp/UnitComp.java:790`: `drag = type.drag * (isGrounded() ? (floorOn().dragMultiplier) : 1f) * dragMultiplier * state.rules.dragMultiplier;`
  (`dragMultiplier` = status multiplier; set in T5) and `:805` `drag = 0.01f` while dead (T6).
* `AIController.pathfind` (`J/entities/units/AIController.java:149-161`) ends with
  `unit.movePref(vec.trns(unit.angleTo(targetTile.worldx(), targetTile.worldy()), prefSpeed()));`;
  `moveTo` (`:375-430`) ends with `unit.movePref(vec)` (read lines 420-430 for the `rotateMoveFirst`
  branch and copy it).

**Port today.** `core/ai/types/ground.rs::approach` (~101) integrates `pos`; callers:
`update_ground`, `pathfind_field`, `core/ai/types/command.rs:~431` (`update_command_move`),
`core/ai/ai_controller.rs:~251`, `core/ai/types/flying.rs:~440` (`update_flying`).
`core/ai/types/flying.rs::move_pref` (~69, already `moveAt`) + `write_back` (~375, `pos += vel`, no drag).
`core/ai/types/player.rs::update_player` (~160-176) does `moveAt` then `pos += vel` itself.
`core/ai/types/missile.rs` (~124-141) `moveAt` + `pos += vel`. `environment.rs` steps `pos` for the
warp (~155) and push (~274) and has a corner "push-out" in the solid branch (~287-335).
`RT::unit_update_system` Command idle branch zeroes `vel` (`None => { vel.x = 0.0; vel.y = 0.0; }`).
`PhysicsComp.drag` (`core/entities/comp/unit/comp.rs:89`) is set from `def.drag` at spawn, never read.

**Changes.**
1. New file `core/entities/comp/unit/velocity.rs` (GPL header, `pub mod velocity;` in
   `core/entities/comp/unit/mod.rs`):
   ```rust
   //! `VelComp.update`, `EntityCollisions.move`, `UnitComp.moveAt/movePref/rotateMove`,
   //! `PhysicsComp.impulse` (VelComp.java:23-90, EntityCollisions.java:36-104,
   //! UnitComp.java:101-153, PhysicsComp.java:18-30).
   pub fn move_at(world: &mut World, entity: Entity, vx: f32, vy: f32, accel: f32)
   pub fn move_pref(world: &mut World, content: &ContentRegistry, entity: Entity, vx: f32, vy: f32)
   pub fn rotate_move(world: &mut World, content: &ContentRegistry, entity: Entity, vx: f32, vy: f32)
   pub fn impulse(world: &mut World, entity: Entity, ix: f32, iy: f32)
   pub fn collisions_move(pos: &mut Pos, hit_size: f32, dx: f32, dy: f32, solid: &dyn Fn(i32, i32) -> bool)
   pub fn vel_update(world: &mut World, grid: &WorldGrid, content: &ContentRegistry, entity: Entity)
   ```
   `move_at` per Java (`limit(accel * |vector|)`; `|vector| == 0` -> limit 0 -> no change).
   `move_pref`: `def.omni_movement` -> `move_at(def.accel)`, else `rotate_move` (rotation step uses
   `def.rotate_speed * StatusComp.speed_multiplier`, `crate::weapons::angles_move_toward`).
   `collisions_move` per the transcription (round = `(v + 0.5).floor() as i32`, sign = `if f < 0.0 {-1.0} else {1.0}`).
   `vel_update`: read `Pos`, `Vel`, `PhysicsComp.drag`, `HitboxComp.hit_size`; `px,py = pos`;
   if the unit is flying (`crate::combat::bullet::unit_is_flying`) or has no solidity -> `pos += vel`,
   else `collisions_move(&mut pos, hit, vel.x, vel.y, &|x, y| !unit_can_pass(world_snapshot..., x, y))`
   — `unit_can_pass` borrows `&World`: compute `pos` on a copy, then write back; `if (px - pos.x).abs() <= 1e-6 { vel.x = 0.0 }`
   (same for y); `vel *= (1.0 - drag).max(0.0)`.
   Move `flying.rs::move_pref` callers onto `velocity::move_pref` and delete the local copy;
   delete `flying.rs::write_back`'s `pos + vel` (write only `vel`/`rotation`).
2. `ground.rs::approach` -> rename `steer_to(world, grid, content, entity, dest_x, dest_y, arrive) -> bool`:
   when `dist <= arrive` return `true` (no velocity change; drag stops the unit); else
   `move_pref(world, content, entity, dx/dist * min(speed, dist), dy/dist * min(speed, dist))`
   (Java `vec.set(target).sub(unit).limit(speed)` — this is the CommandAI final-point brake;
   document the deviation from `moveTo(..arrive=true)`), return `false`. Update every caller listed in
   "Port today"; `pathfind_field` uses `move_pref(vec.trns(angle_to(nx*8, ny*8), speed))` exactly
   (no `min(dist)`, Java has none). Remove all `pos.x += vel.x` lines in AI helpers.
3. `player.rs::update_player`: keep the `moveAt` block; delete `pos.x += vel.x; pos.y += vel.y;`
   (the next tick's `vel_update` moves the unit — Java's player input also only sets `vel`).
   `missile.rs`: same (keep `moveAt`, delete the integration; the missile has `drag` from its type).
4. `RT::unit_update_system`: at the top of the per-unit loop, **before** the controller dispatch and
   before `let before = ...pos` is read, call `velocity::vel_update(world, &runtime.grid,
   &runtime.content, entity)` (Java `@MethodPriority(-1)`). Compute `before` **before**
   `vel_update` so the kinematics `delta` includes this tick's movement. Delete the Command idle
   `vel = 0` branch (Java CommandAI idle does nothing; drag stops the unit).
5. `environment.rs`: delete the two hand-steps (`pos.x += warp_x` / `pos.x += push_x` and y), keep
   `vel +=`; delete the corner push-out in the solid branch and restore Java: `if(tile != null &&
   !canPassOn()){ if(canBoost) elevation = 1; else kill(); }` (`J/entities/comp/UnitComp.java:849-855`)
   — `EntityCollisions.move` now prevents the corner clips that motivated it.
6. Spawn impulse: `spawn_player_at_best_core` keeps `vel.y += 3.0 / mass` (= `impulse(0,3)`); route it
   through `velocity::impulse`. Bullet knockback (`core/combat/bullet/mod.rs` ~1447): use
   `velocity::impulse` too (same math; confirm it is `vel += v / mass`).
7. Goldens: unit scenario checksums change (`$L cargo test -p mind-headless --lib units_scenarios`).
   Regenerate the changed `client/rust/mind-headless/tests/golden/units/*.json` per env §2 and name
   each in the commit body with "VelComp.update + EntityCollisions.move replace the kinematic mover".

**Tests** (`core/entities/comp/unit/velocity.rs` tests + `CT`):
* `move_at_limits_by_accel`: vel (0,0), `move_at(.., 2.0, 0.0, 0.5)` -> vel `(1.0, 0.0)` (limit `0.5*2`); again -> `(2.0, 0.0)`.
* `vel_update_applies_drag_and_stops_on_wall`: 20x20 grid, a `stone-wall`... use `place(&mut sim, 10, 5, "copper-wall", 1)`;
  dagger at `(10*8 - 8.0, 40.0)` with vel `(3.0, 0.0)`; after one `vel_update` the hitbox right edge
  `x + 7.8*0.66/2`... derive: dagger `hitSize 8` -> box `5.28`, wall box `[76, 84)`; final
  `x <= 76 - 2.64 = 73.36` and `vel.x == 0` (position stopped changing); a second dagger in free space
  with vel `(1, 0)` and drag 0.3 -> after `vel_update` vel `(0.7, 0)`, x advanced by `1.0`.
* `knockback_moves_ground_unit`: `velocity::impulse(world, dagger, mass*2.0, 0.0)` (mass = 8²π) on an idle
  `NoAi` dagger -> after 1 tick x grew by 2.0; after 30 ticks it stopped (vel < 0.01).
* `sandstorm_wind_pushes_idle_dagger` (PAR-0202): G07's wind seam is `core/entities/comp/weather.rs:132-141`
  (verified 2026-10-14); create a sandstorm (intensity 1) and assert an idle team-1 dagger on a
  floor without `dragMultiplier` moved `≈ 300 * (0.1/(64π))/0.3 ≈ 0.50` px (± 0.1, after the
  speed-up) along the wind vector in 300 ticks (Java steady state `a/drag` per tick; > 1 px would
  NOT be Java), or use a flare (≈ 2.9 px in 300 ticks); otherwise add a direct
  `velocity::impulse` test and note the seam.
* Re-run: `command_ai_move_order_arrives_and_clears`, `command_ai_ground_order_routes_around_walls`,
  `ground_wave_paths_to_core_not_nearest_building`, `ground_zero_drill_fed_duo_kills_wave_daggers`,
  every `flying`/`player` test. A test that times out because units now accelerate may get **more ticks**
  (record which); never weaken a position/target assertion.

**Verify in-engine.** Ground Zero: `$H.debug_create_weather("sandstorm", 1.0, 1200.0)` (signature
`(name, intensity, duration)`, verified), spawn a team-1 **flare** at (1100,420) (`spawn_unit`, unit
index `content_list("unit").find("flare")`; ground units drift too little to measure), read its
`pos`/`vel` from `unit_debug_json()`, `step(600)`, read again: it moved downwind by about 6 px
(Java ≈ `a/drag` per tick = 0.1/(81π)/0.04 ≈ 0.0098 px/tick) along the `weather_json()` wind
vector, and `vel` is non-zero (PAR-0202). An idle dagger would move only ≈ 1 px. Also
run the Ground Zero wave probe (T14 in-engine half). Screenshot `runs/<date>-s<N>-H01-T4/wind.png`.

**Pitfalls.** `unit_physics_system` (separation push) still writes `Pos` directly (Java
`PhysicsProcess` also moves bodies directly) — keep. Units carried as payload: skip `vel_update`
when the unit has no `Pos` in the world grid sense (check for the payload marker used by
`payload_pickup_unit`; grep `remove::<Pos>\|Carried`). Legs units: `LegsComp` with `allow_leg_step`
use `legsSolid` through `unit_can_pass` — good. Determinism: no RNG here.

**Commit.** `Fixes PAR-0202: units integrate velocity with drag and tile collisions (VelComp/EntityCollisions)`

---

### T5 — Unit environment residuals: drag, captured-sector AI death, grounded test, `hasSpawns`

> **In-engine verification (2026-10-14, session 2):** NOT CHECKABLE in-engine (headless task). Code claims confirmed: `PhysicsComp.drag` (`comp.rs:89`) never read, `floor_drag_multiplier` at `blocks/mod.rs:1340`, `StatusComp.drag_multiplier` `comp.rs:466`, `Rules.drag_multiplier` `rules.rs:610`, `team_ai_flags` `lifecycle.rs:113`; Java ice `dragMultiplier = 0.35` confirmed.

**Symptom / acceptance.** Ice does not make units slide; statuses with `dragMultiplier` do nothing;
enemy AI units keep living after the sector becomes captured; the drop-zone push only runs on wave
maps but Java uses `state.hasSpawns()` (also attack maps with wave-team cores); drowning/floor
status use `!isFlying` instead of `isGrounded` (`elevation < 0.001`). After: a dagger on `ice`
coasts farther than on `stone`; enemy units die when `rules.waves == false && rules.attack_mode == false`
in a campaign sector.

**Java** (`J/entities/comp/UnitComp.java`):
* `:80-82` `isGrounded(){ return elevation < 0.001f; }`; `isFlying()` = `elevation >= 0.09f`
  (`grep -n "public boolean isFlying" J/entities/comp/FlyingComp.java J/entities/comp/UnitComp.java`).
* `:750-753` `if(team.isOnlyAI() && state.isCampaign() && state.getSector().isCaptured()){ kill(); }`;
  `Team.isOnlyAI()` (`J/game/Team.java:117-119`) = `isAI() && data().players.size == 0`;
  `Team.isAI()` (`:112-114`) = `(rules.waves || rules.attackMode || state.isCampaign()) && this != rules.defaultTeam && !rules.pvp`;
  `Sector.isCaptured()` for the sector being played = `!state.rules.waves && !state.rules.attackMode`.
* `:790` drag line (T4 quotes it).
* `:793` `if(team != state.rules.waveTeam && state.hasSpawns() && (!net.client() || isLocal()) && hittable())`;
  `GameState.hasSpawns()` (`J/core/GameState.java:68-70`) = `rules.waves && ((rules.waveTeam.cores().size > 0 && rules.attackMode) || rules.spawns.size > 0)`
  — **note**: `rules.spawns` is the spawn *group* list (`Seq<SpawnGroup>`), not spawn tiles.
* `StatusComp.update` (`J/entities/comp/StatusComp.java:192-197`): floor status when `isGrounded() && !type.hovering`.

**Port today.** `core/entities/comp/unit/environment.rs::update_unit_environment` (header lists the
deferrals). `grounded = !unit_is_flying(..)`. Push gated on `rules_waves`. No drag write.
`team_ai_flags(world, team)` (`lifecycle.rs:113`) = Java `isAI`. "Players on team": the port has one
local player — `Sim.player_team` is not visible from the world; use "team == rules.default_team"
as the player team (single-player; document).
Floor drag: `BlockDef.floor_drag_multiplier: Option<f32>` (`core/content/registries/blocks/mod.rs:1340`).
Status drag: `StatusComp.drag_multiplier` (`comp.rs:466`). Rules: `Rules.drag_multiplier`
(`core/io/json/rules.rs:610`).

**Changes.**
1. `grounded = core.elevation < 0.001` (read `UnitCore.elevation`) for drowning, floor status, floor
   damage and the drag line.
2. Drag (after the floor status, Java order): `physics.drag = def.drag * (if grounded {floor_drag} else {1.0}) * status.drag_multiplier * rules.map_or(1.0, |r| r.drag_multiplier)`
   where `floor_drag = block(floor).floor_drag_multiplier.unwrap_or(1.0)`. Write `PhysicsComp.drag`.
3. Captured-sector death: when `rules` exist, `session.is_campaign()` (read via `CampaignRuntime`),
   `!rules.waves && !rules.attack_mode`, `team_is_ai` and `team != rules.default_team` -> kill.
   (Insert after `updateDrowning`, Java order.)
4. Push gate: `has_spawns = rules.waves && ((rules.attack_mode && <wave team has a core>) || !rules.spawns.is_empty())`
   (wave-team core: `crate::sim::unit_runtime::has_core_of_team` is private — make a `pub(crate)` copy
   or pass a bool from `unit_update_system`, computed once per tick); the spawn list still comes from
   `runtime.wave_spawner.get_spawns()`; Java `within` = `dst2 < r*r` (strict).
5. Update the module doc (remove the deferrals that are now done; keep "dead-flyer fall -> T6").

**Tests** (`CT`): `ice_reduces_drag` (two NoAi daggers with vel (2,0), one on `ice` floor, one on
`stone`; after 20 ticks the ice one travelled farther; derive drag: dagger `drag` from content ×
`ice` `dragMultiplier 0.35` — read `J/content/Blocks.java` `ice` block); `enemy_dies_on_captured_sector`
(campaign session with waves false, attack false; team-2 dagger dies next tick; team-1 dagger lives);
`attack_map_pushes_from_drop_zone` (waves true, attack true, wave-team core present, spawn group list
non-empty).

**Verify in-engine.** Headless only (needs an ice floor near the player; optional: Frozen Forest
sector 64 has ice — `tile_debug` to find one).

**Commit.** `Gameplay H01 T5: unit drag, captured-sector AI death and hasSpawns drop-zone push`

---

### T6 — Dead flying units fall and crash (`UnitComp.killed` wreck)

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: a team-1 flare killed with `melting` disappeared within 5 ticks of reaching 0 hp (Java keeps a wreck ~56 ticks). Java defaults confirmed (`UnitType.java:82,102,211`). The in-engine step needs T3's `debug_kill_unit`; `debug_apply_status(id, "melting", 3000)` works as a stand-in (~230 ticks for a flare).

**Symptom / acceptance.** A killed flare/horizon/zenith vanishes instantly where it died; Java keeps it
as a falling wreck (no AI, low drag, `elevation -= fallSpeed`) that crashes where it lands (crash
damage hits whatever is under it). After: a killed flying `createWreck` unit keeps existing with
`Health.dead` until grounded, then the normal destroy path runs (explosion, crash damage, death
abilities, kill stats).

**Java.**
* `J/entities/comp/UnitComp.java:980-991` (`killed`):
  `health = Math.min(health, 0); dead = true; if(!type.flying || !type.createWreck){ destroy(); }else{ type.wreckSound... }`
* `:800-830` (in `update`): `if(dead || health <= 0){ drag = 0.01f; ...effects...; elevation -= type.fallSpeed * Time.delta; if(isGrounded() || health <= -maxHealth * type.wreckHealthMultiplier){ Call.unitDestroy(id); } }`
* `:860-862` controller only when `!dead`; `Units.invalidateTarget` treats `!isValid()` (dead) as invalid,
  so wrecks are not targets.
* Defaults (`J/type/UnitType.java:82,102,211`): `fallSpeed = 0.018f`, `wreckHealthMultiplier = 0.25f`, `createWreck = true`.

**Port today.** `RT::resolve_deaths` collects `h.dead || health <= 0` and destroys immediately
(explosion, `run_shoot_on_death`, crash damage, `kill_unit_with_abilities`, kill stat).
Content: `UnitTypeDef.{fall_speed, wreck_health_multiplier, create_wreck, flying}`
(`core/content/registries/units/mod.rs:1037,1060,1166`).

**Changes.**
1. Marker component `Falling` (RT). In `resolve_deaths`, for each dead unit: if `def.flying &&
   def.create_wreck` and not `Falling` and `UnitCore.elevation >= 0.001` -> set `health.dead = true`,
   `health.health = health.health.min(0.0)`, insert `Falling`, **skip** the destroy body this tick.
   A `Falling` unit is destroyed (the existing body) when `elevation < 0.001` or
   `health.health <= -def.health * def.wreck_health_multiplier`.
2. `unit_update_system`: a unit with `Falling`: run `vel_update` (T4; drag set to `0.01` by T5's
   drag code when `health.dead`), `elevation -= def.fall_speed`, skip controller/targeting/weapons
   (also make `sync_weapon_state`/`update_weapons` skip dead units — check `UnitState.can_shoot`),
   skip `update_unit_environment` except the bounds block (Java runs the full update; keep it simple:
   bounds only, document).
3. Every place that treats `health.dead` units as alive must keep excluding them (targeting already
   does via `invalid_target`). The physics push skips dead bodies already.
4. Player unit: a dead falling player unit is "dead" for `update_player_respawn` (it already checks
   `health.dead`) — but do not despawn it there while `Falling`; let `resolve_deaths` destroy it.

**Tests** (`CT`): `dead_flare_falls_then_crashes` (flare team 2 at elevation 1 with vel (2,0);
set health 0; tick 1 -> entity exists, dead; after `ceil(1/0.018) = 56` ticks (±1) it is gone and
its x advanced > 2 px); `ground_unit_dies_instantly` (dagger gone after one tick);
`wreck_destroyed_by_overkill` (falling flare health set to `-70*0.25 - 1` -> gone next tick).

**Verify in-engine.** Ground Zero: spawn a team-2 flare at (1100,600), `step(5)`, `debug_kill_unit(id)`,
`step(10)` -> `unit_debug_json` still lists it (dead); `step(60)` -> gone. Headless is the acceptance.

**Commit.** `Gameplay H01 T6: dead flying units fall as wrecks before crashing`

---

### T7 — Explosion residuals: lightning bolts, fireballs, ExplosionShield semantics (PAR-0091 residual)

> **Coordinator override (2026-10-14, see H-INDEX §3).** H09 T7 owns `dynamic_explosion` lightning, fireballs, wave stagger and the building explosion terms. In THIS task do ONLY Change 1 (`explosion_absorbed`: no absorption when `ignore_team` is `None`, skip team 0 and the ignore team, sorted candidates, stop at the FIRST absorbing shield) and only the tests `no_absorption_without_ignore_team` and `only_first_shield_absorbs`. Skip Changes 2-5 and the lightning/fireball tests. Commit subject: `PAR-0091: ExplosionShield absorption semantics (ignoreTeam, first shield only)`.

> **In-engine verification (2026-10-14, session 2):** NOT CHECKABLE in-engine (headless task; coordinator cut it to `explosion_absorbed`). Code confirmed: `explosion_absorbed` at `combat/damage/explosion.rs:95`.

**Symptom / acceptance.** Units/blocks with `charge` items (surge, batteries' stored power, thorium?) never
throw lightning on death; flammable explosions start one fire instead of a spray of fireballs; force
projectors absorb explosions with no `ignoreTeam` and from derelict shields, and several projectors take
buildup for one wave. After: a unit carrying 30 surge-alloy dies with
`clamp(power/700, 0, 8)` lightning bolts; flammable deaths spawn `clamp(flam/4, 0, 30)` fireball bullets.

**Java** (`J/entities/Damage.java:93-133`):
```java
if(damage){
    for(int i = 0; i < Mathf.clamp(power / 700, 0, 8); i++){
        int length = 5 + Mathf.clamp((int)(Mathf.pow(power, 0.98f) / 500), 1, 18);
        Time.run(i * 0.8f + Mathf.random(4f), () -> Lightning.create(Team.derelict, Pal.power, 3 + Mathf.pow(power, 0.35f), x, y, Mathf.random(360f), length + Mathf.range(2)));
    }
    if(fire){
        for(int i = 0; i < Mathf.clamp(flammability / 4, 0, 30); i++){
            Time.run(i / 2f, () -> Call.createBullet(Bullets.fireball, Team.derelict, x, y, Mathf.random(360f), Bullets.fireball.damage, 1, 1));
        }
    }
    int waves = explosiveness <= 2 ? 0 : Mathf.clamp((int)(explosiveness / 11), 1, 25);
    float damagePerWave = explosiveness / 2f;
    for(int i = 0; i < waves; i++){
        int f = i;
        Time.run(i * 2f, () -> {
            var shields = ignoreTeam == null ? null : indexer.getEnemy(ignoreTeam, BlockFlag.shield);
            if(shields == null || shields.isEmpty() || !shields.contains(b -> b instanceof ExplosionShield s && s.absorbExplosion(x, y, damagePerWave))){
                damage(ignoreTeam, x, y, Mathf.clamp(radius + explosiveness, 0, 50f) * ((f + 1f) / waves), damagePerWave, false);
            }
            Fx.blockExplosionSmoke.at(x + Mathf.range(radius), y + Mathf.range(radius));
        });
    }
}
```
`Pal.power = Color.valueOf("fbad67")` (`J/graphics/Pal.java:92`). `Bullets.fireball = new FireBulletType(1f, 4){{ hittable = false; }}`
(`J/content/Bullets.java:44-46`; port id `BulletId::new(4)`, asserted at
`core/content/registries/bullets.rs:2141`). `indexer.getEnemy(team, flag)` = buildings of **other
active teams** with the flag (derelict is not an active team). `Seq.contains(pred)` stops at the first match.

**Port today.** `core/combat/damage/explosion.rs::dynamic_explosion(world, content, rng, x, y, flam,
expl, power, radius, damage, fire, ignore_team)` (no lightning; one `fires::create`);
`explosion_absorbed(world, x, y, damage, ignore_team)` (iterates `iter_entities()`, includes
`ignore_team == None` and team 0, applies buildup to every absorbing projector). Lightning:
`core/combat/lightning.rs::create(ctx: &mut CombatCtx, team, color, damage, x, y, rotation, length)`.
Callers of `dynamic_explosion`: `RT::resolve_deaths` (unit ~446, building ~774),
`core/combat/bullet/kinds/mass_driver.rs:~313`.

**Changes.**
1. `explosion_absorbed`: return `false` immediately when `ignore_team` is `None`; skip team 0
   (`is_derelict`) and the ignore team; iterate candidates sorted by `(EntitySeq, index)`; **stop at the
   first** absorbing shield (projector or base shield) — only that one gets `hit`/buildup.
2. `dynamic_explosion` gains `bursts: &mut ExplosionBursts` (new `pub struct ExplosionBursts {
   pub lightning: Vec<(f32 /*damage*/, f32 /*angle*/, i32 /*length*/)>, pub fireballs: Vec<f32 /*angle*/> }`):
   draws in Java order with `rng.range(RngStream::Sim, ..)` — lightning loop: `length = 5 + clamp((power^0.98 / 500) as i32, 1, 18)`,
   draw `delay = rng.range(0, 4)` (discarded; Java delay), `angle = rng.range(0, 360)`,
   `extra = rng.range(-2, 2)` -> `length + extra as i32` (Java `Mathf.range(2)` is a float added to an int
   argument: `length + Mathf.range(2)` is a float passed to an `int` parameter — check the
   `Lightning.create` signature in `J/entities/Lightning.java`; if it takes `int length`, Java truncates
   toward zero), push `(3 + power^0.35, angle, len)`; fire loop: push `rng.range(0,360)` per fireball
   (`clamp(flam/4, 0, 30)` iterations, loop bound computed as `f32` like Java `i < clamp(...)`).
   Remove the single `fires::create`.
3. New `pub fn fire_explosion_bursts(ctx: &mut CombatCtx, x: f32, y: f32, bursts: ExplosionBursts)`:
   lightning via `lightning::create(ctx, 0 /*derelict*/, Rgba::from_rgba8888(0xfbad67ff), damage, x, y, angle, len)`;
   fireballs via `ctx.spawn(&BulletSpawn { def: FIREBALL, team: 0, x, y, angle, damage: 4.0,
   velocity_scl: 1.0, lifetime_scl: 1.0, ..BulletSpawn::default() })`. Add
   `pub const FIREBALL: BulletId = BulletId::new(4);` next to `DAMAGE_LIGHTNING` in
   `core/content/registries/bullets.rs` with a doc citing `Bullets.java:44`.
4. Callers: in `resolve_deaths` build a `CombatCtx` from the runtime parts (`content, grid, rng, fx,
   seq`, an `AudioSinkRes` taken from the world like `unit_update_system` does, a local `spawned`
   Vec) after `dynamic_explosion`, call `fire_explosion_bursts`, then `runtime.bullets.extend(spawned)`.
   The building-death call site (~774) runs outside the unit-runtime borrow: collect
   `(x, y, bursts)` into a Vec and fire them after the loop with a fresh `CombatCtx`.
   `mass_driver.rs` already has a `CombatCtx`: call `fire_explosion_bursts(ctx, ..)` directly.
   Java's `Time.run` delays are dropped (immediate, same order) — document.
5. Update the module doc of `explosion.rs`.

**Tests** (`core/combat/damage/explosion.rs` tests + `CT`):
`explosion_spawns_lightning_for_power` (power 2000 -> `clamp(2000/700,0,8)` = 2.857 -> 3 iterations
(i = 0,1,2 < 2.857) -> 3 lightning entries, damage `3 + 2000^0.35`; compute in the test with `powf`);
`explosion_spawns_fireballs` (flam 10, fire true -> `i < 2.5` -> 3 fireball bullets after
`fire_explosion_bursts`); `no_absorption_without_ignore_team`; `only_first_shield_absorbs` (two
powered projectors over the point: only one has buildup > 0). Keep `unit_with_blast_compound_explodes`
and `force_projector_absorbs_explosion` green.

**Verify in-engine.** Headless only (needs item-carrying enemy units and power items).

**Commit.** `PAR-0091: explosion lightning, fireballs and ExplosionShield absorption semantics`
(use `Fixes PAR-0091: ...` only if the coordinator's ledger lists lightning/fireballs as the last residual).

---

### T8 — `targetFlagActive` uses the Java building status

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: a team-2 flare spawned at (1300,700) flew past a working `solar-panel` at (146,62) and circled the core (an unfuelled `combustion-generator` at (150,40) was correctly ignored). Text fixed below: most tiles east/north-east of the core are ice walls (`place_block` returns false there) — use (146,62) for the panel.

**Symptom / acceptance.** Flares (`targetFlags {generator, null}`), horizons (`{factory, core}`) and
zeniths only accept flagged buildings that hold items; Java also accepts any building whose
`status() != noInput` (a powered/working generator or factory with no item buffer, solar panels,
batteries). After: a wave flare targets a lone `solar-panel` (flag `generator`? — check the port's
`BlockDef.flags` for `solar-panel`; Java `PowerGenerator` sets `flags = EnumSet.of(BlockFlag.generator)`)
instead of falling through to `null`/core.

**Java.** `J/entities/units/AIController.java:276-279`:
```java
public Teamc targetFlagActive(float x, float y, BlockFlag flag, boolean enemy){
    if(unit.team == Team.derelict) return null;
    return Geometry.findClosest(x, y, enemy ? indexer.getEnemy(unit.team, flag) : indexer.getFlagged(unit.team, flag), t -> ((t.items != null && t.items.any()) || t.status() != BlockStatus.noInput) && t.block.targetable);
}
```
`J/entities/comp/BuildingComp.java:638-652`:
```java
public BlockStatus status(){
    if(!enabled) return BlockStatus.logicDisable;
    if(!shouldConsume()) return BlockStatus.noOutput;
    if(efficiency <= 0 || !productionValid()) return BlockStatus.noInput;
    return ((state.tick / 30f) % 1f) < efficiency ? BlockStatus.active : BlockStatus.noInput;
}
```
(Subclasses override `status()` — e.g. turrets, `PowerNode`, batteries: `grep -rn "public BlockStatus status()" J/world/blocks`;
transcribe only the overrides of blocks that carry a target flag used by vanilla `targetFlags`:
`generator`, `factory`, `core`, `launchPad`, `storage`, `battery`, `turret`, `repair`, `reactor` — read
`grep -n "targetFlags" J/content/UnitTypes.java` for the full set.)

**Port today.** `RT::target_flag_active` (~4033) uses `ItemModule.any()`. `Building` has
`enabled`, `efficiency`, `should_consume` (`core/entities/comp/building.rs:35-48`).
`core/world/status.rs::block_status` is a UI approximation (do not reuse). Tick: there is no
`SimClock` resource in the world; use `world.get_resource::<crate::logic::world::LogicWorldState>().map(|s| s.tick)`
(set every tick in `Sim::update_logic_globals`), else 0.

**Changes.** New `pub fn java_block_status(world: &World, entity: Entity, tick: u64) -> BlockStatus`
in `core/world/status.rs` (doc cites BuildingComp.java:638-652; `productionValid()` = `true` unless a
port field exists — grep `production_valid`; document). Implement the overrides found above that matter
for the flag set (read each Java override; if one needs state the port lacks, keep the base rule and
note it). `target_flag_active`: active = `items.any() || status != NoInput`; also skip
`!def.targetable` blocks (field: `grep -n "pub targetable" core/content/registries/blocks/mod.rs`;
if absent, skip that term and note it).

**Tests** (`CT`): `flare_targets_powered_generator_without_items` (team-1 `solar-panel` far from the core,
team-2 flare out of core range; tick 300 -> flare distance to the panel shrank and its main target is the
panel); `unpowered_factory_is_inactive` (`graphite-press` with no input: not chosen by a horizon when a
core exists -> horizon goes to the core).

**Verify in-engine.** Ground Zero: `place_block(146,62,"solar-panel")` and
`place_block(150,40,"combustion-generator")` (no fuel; many tiles east of the core are ice walls,
`place_block` returns false there); spawn a team-2 flare at (1300,700); after 600 steps the flare
circles the panel at world (1168,496) (`unit_debug_positions`). Before T8 (verified 2026-10-14)
it flies past the panel to the core.

**Commit.** `Gameplay H01 T8: targetFlagActive uses the Java building status`

---

### T9 — Live dispatch for `DefenderAI`, `FlyingFollowAI`, `HugAI`, `NoAI`

> **In-engine verification (2026-10-14, session 2):** CONFIRMED for NoAI: a team-3 `dummy` (kind reads `NoAI`) spawned on Onset walked from (2298,898) to (2242,843) in 300 ticks. Defender/FlyingFollow/Hug code-confirmed (generic branch; stub modules `ai/types/{defender,flying_follow,hug}.rs` 80/118/89 lines).

**Symptom / acceptance.** Oct (DefenderAI), quell/disrupt (FlyingFollowAI), renale/latum (HugAI) and the
dummy (NoAI) run the generic GroundAI/FlyingAI branch. After: a wave oct follows the healthiest
non-oct ally; a wave quell follows an ally and only fights when no ally is around; renale hugs the
target; a dummy never moves.

**Java.**
* `J/ai/types/DefenderAI.java:12-45`:
  ```java
  updateMovement(){ unloadPayloads(); if(target != null){ moveTo(target, (target instanceof Sized s ? s.hitSize()/2f * 1.1f : 0f) + unit.hitSize/2f + 15f, 50f); unit.lookAt(target); } }
  updateTargeting(){ if(retarget()) target = findTarget(unit.x, unit.y, unit.range(), true, true); }
  findTarget(x, y, range, air, ground){
      var result = Units.closest(unit.team, x, y, Math.max(range, 400f), u -> !u.dead() && u.type != unit.type && u.targetable(unit.team) && u.playerControllable(),
          (u, tx, ty) -> -u.maxHealth + Mathf.dst2(u.x, u.y, tx, ty) / 6400f);
      if(result != null) return result;
      var core = unit.closestCore(); if(core != null) return core;
      if(state.rules.waves && unit.team == state.rules.waveTeam){ return unit.closestEnemyCore(); }
      return null; }
  ```
  (No `updateWeapons` — DefenderAI overrides `updateTargeting`, so an oct never fires its weapons
  through the AI; its abilities (force field, repair) work on their own.)
* `J/ai/types/FlyingFollowAI.java:11-60`:
  ```java
  updateMovement(){ unloadPayloads();
      if(following != null){ moveTo(following, (following instanceof Sized s ? s.hitSize()/2f * 1.1f : 0f) + unit.hitSize/2f + 15f, 50f); }
      else if(target != null && unit.hasWeapons()){ moveTo(target, 80f); }
      if(shouldFaceTarget()){ unit.lookAt(target); } else if(following != null){ unit.lookAt(following); }
      if(timer.get(timerTarget3, 30f)){
          following = Units.closest(unit.team, unit.x, unit.y, Math.max(unit.type.range, 400f), u -> !u.dead() && !(u.controller() instanceof FlyingFollowAI) && u.type != unit.type, (u, tx, ty) -> -u.maxHealth + Mathf.dst2(u.x, u.y, tx, ty) / 6400f); } }
  shouldFaceTarget(){ return target != null && (following == null || unit.within(target, unit.range())); }
  fallback(){ return new FlyingAI(); }   useFallback(){ return Vars.state.rules.pvp || Vars.state.rules.waveTeam != unit.team; }
  ```
  (targeting = FlyingAI's `findMainTarget`/`findTarget` + `updateWeapons`.)
* `J/ai/types/HugAI.java:16-61`:
  ```java
  Building core = unit.closestEnemyCore();
  if(core != null && unit.within(core, unit.range() / 1.1f + core.block.size * tilesize / 2f)){ target = core; for(var mount : unit.mounts){ if(mount.weapon.controllable && mount.weapon.bullet.collidesGround) mount.target = core; } }
  boolean move = true;
  if(state.rules.waves && unit.team == state.rules.defaultTeam){ Tile spawner = getClosestSpawner(); if(spawner != null && unit.within(spawner, state.rules.dropZoneRadius + 120f)) move = false; }
  if(target != null && unit.within(target, unit.type.range) && !World.raycast(unit.tileX(), unit.tileY(), target.tileX(), target.tileY(), (x, y) -> { for(Point2 p : Geometry.d4c){ if(!unit.canPass(x + p.x, y + p.y)) return true; } return false; })){
      if(unit.within(target, (unit.hitSize + (target instanceof Sized s ? s.hitSize() : 1f)) * 0.5f)){ unit.movePref(vec.set(target).sub(unit).rotate(90f).setLength(unit.speed())); }
      else{ unit.movePref(vec.set(target).sub(unit).limit(unit.speed())); }
  }else if(move){ pathfind(Pathfinder.fieldCore); }
  if(unit.type.canBoost && unit.elevation > 0.001f && !unit.onSolid()){ unit.elevation = Mathf.approachDelta(unit.elevation, 0f, unit.type.descentSpeed); }
  faceTarget();
  ```
  (targeting = `AIController` default: `findMainTarget` = `target(x, y, range, air, ground)`.)
  `Geometry.d4c` = `(1,0),(0,1),(-1,0),(0,-1),(0,0)` (`ARC/arc-core/src/arc/math/geom/Geometry.java:16-22`).
* `NoAI` (`J/ai/types/NoAI.java`): `updateUnit(){}`.
* `Units.closest(team, x, y, range, pred, sort)` (`J/entities/Units.java`, `grep -n "public static Unit closest(Team team, float x, float y, float range, Boolf<Unit> predicate, Sortf sort)" -A20`): minimises `sort(u, x, y)` over team units within range — read and port the exact loop (distance gate `u.within(x, y, range + u.hitSize/2f)` if present).

**Port today.** `AiKind::{Defender, FlyingFollow, Hug, NoAi}` exist (`core/ai/controller.rs`); `select_ai`
maps them (`core/ai/controller.rs:145-170`). `unit_update_system`'s last `else` sends them to
`update_flying_ai`/`update_ground_ai`. Stub modules `core/ai/types/{defender,flying_follow,hug}.rs`
(defender targets hostiles — wrong). Helpers: `flying.rs::move_to_point(world, grid, content, entity,
tx, ty, circle_length, smooth, look_at)` (keepDistance is hard-wired `true`; DefenderAI/FlyingFollowAI
units are flying so that matches `unit.isFlying()`), `ground.rs::pathfind_field`, `RT::enemy_core_targets`,
`RT::closest_enemy_core`, `RT::closest_spawn_tile`, T2's `update_ai_weapons`.

**Changes.**
1. Rewrite `core/ai/types/defender.rs`: `pub struct DefenderAiState { retarget: f32, target: Option<Entity> }`
   (component, seeded 39) and `pub fn defender_find_target(world, content, team, x, y, range, unit_type, wave_rules) -> Option<Entity>`
   per Java (iterate live units in `EntitySeq` order; `u.type != unit.type`, not dead, targetable,
   `def.player_controllable`; key `-max_health + dst2/6400`; ties keep the first); `closestCore` =
   own-team core closest; enemy core fallback only on wave maps for the wave team. RT branch
   `AiKind::Defender`: retarget timer -> main target; movement `move_to_point(target, (target_hit/2*1.1)
   + hit/2 + 15, smooth 50, look_at true)` where `target_hit` = T2's `target_hit_size` (buildings are `Sized` in Java,
   `BuildingComp.java:54`, `hitSize = size*8`). No weapons update (Java has none) — but mounts with
   `!controllable` still self-target in `update_weapon` (T2 step 5).
2. Rewrite `core/ai/types/flying_follow.rs`: state `{ following: Option<Entity>, follow_timer: f32 }`;
   RT branch `AiKind::FlyingFollow`: if `useFallback` (`rules.pvp || wave_team != team`) -> run the
   existing `update_flying_ai`; else FlyingAI targeting (`flying_main_target` with the 40/90 timer + T2
   `update_ai_weapons(flying_find = true)`) and the movement/look rules above; follow re-pick every 30 ticks
   (`timer.get` fires on the first call in Java? — `Interval.get(id, time)` returns true when
   `time` elapsed since the last true; first call: elapsed since 0 -> check `ARC/arc-core/src/arc/util/Interval.java`
   and mirror it; the existing `GroundAiState` seeds 39 for the 40-tick timer — do the same pattern).
   "controller is FlyingFollowAI" = `ControllerSlot.kind == AiKind::FlyingFollow`.
3. Rewrite `core/ai/types/hug.rs` + RT branch `AiKind::Hug`: GroundAI-style retarget/main target
   (`closest_target(max_range, target_air, target_ground)`), core override at `range/1.1 + size*4`,
   drop-zone stop, raycast via `crate::world::raycast::raycast_each(ux, uy, tx, ty, |x, y| ...)`
   (returns `true` to stop = blocked) with the d4c `unit_can_pass` test, circle/straight `move_pref`
   (T4), else `pathfind_field(.., enemy_core_targets, stop_at_target_tile = true)` (Java `pathfind(fieldCore)` = stop true),
   boost descend, `face_target`, `update_ai_weapons`.
4. `AiKind::NoAi`: `continue` to the kinematics tail (Java `updateUnit(){}`; `vel_update`/environment
   still run since they are `Unit.update`, not the controller).
5. Do not touch `AiKind::Logic` (H06) or `Cargo/Assembler` (T13).

**Tests** (`CT`, campaign session waves true, wave team 2, default 1): `oct_follows_healthiest_ally`
(team-2 oct at (100,100), team-2 dagger at (200,100) and team-2 mace at (300,100): mace has more max
health -> after 600 ticks the oct is within `mace_hit/2*1.1 + oct_hit/2 + 15 + 10` px of the mace);
`quell_follows_ally_on_wave_team` (team-2 quell + team-2 dagger: quell ends near the dagger);
`quell_uses_flying_ai_on_player_team` (team-1 quell -> `useFallback` -> moves to an enemy core / idles
per FlyingAI); `renale_hugs_target` (team-2 renale, team-1 dagger in range with clear line -> distance
shrinks below `(renale_hit + dagger_hit)/2 + 2`); `dummy_never_moves` (team-2 `dummy`, 300 ticks, same Pos).
Check unit names exist: `content.unit_by_name("oct"/"quell"/"renale"/"dummy")`.

**Verify in-engine.** Headless only (these types do not appear on early Serpulo sectors; Erekir).
Optional: spawn a team-2 oct + dagger on Ground Zero and watch `unit_debug_positions()`.

**Commit.** `Gameplay H01 T9: DefenderAI, FlyingFollowAI, HugAI and NoAI run live`

---

### T10 — PAR-0156: enemy base AI residuals (turret DPS in estimateStats, core fill, aiCoreSpawn, fillItems, prebuildAi defenders)

> **Coordinator override (2026-10-14).** The `TeamRule.fillItems` half of Change 5 belongs to H09 T8 — do not implement it here (keep the `prebuild_ai` half of Change 5 and Changes 1-4). For setting core item amounts use `ItemModule::set` if H09 T8 has added it (`grep -n "pub fn set" client/rust/mind-core/src/world/modules/mod.rs`); otherwise add exactly `pub fn set(&mut self, item: ItemId, amount: i32)` as H09 T8 Change 1 describes. Drop the test `fill_items_rule_fills_first_core`.

> **In-engine verification (2026-10-14, session 2):** PARTLY. Code confirmed (`game/runtime.rs:1216-1220` passes `core_unit_count: 0` and drops `update_base_build`'s actions). Sector: Onset (`erekir` 10) loads with `attack:true, enemyCores:2` — malis (team 3) `core-bastion`s at about (287,196) and (293,288) — but its team-3 rule is `{fillItems, infiniteResources, rtsAi, buildSpeedMultiplier 0.4}` with **no `buildAi`**, so BaseBuilderAI/aiCoreSpawn cannot be observed there; sector 225 was not checked. Use Onset only for the rtsAi/turret-DPS half; the buildAi half stays headless unless a `buildAi` sector is found.

**Symptom / acceptance.** RtsAI squads ignore enemy turrets when weighing attacks; enemy cores on attack
sectors are never filled (Java fills the first core to capacity every tick when `buildAi` is on), so
enemy builders run dry; `aiCoreSpawn` (default **true**) never spawns enemy core units; Erekir's
`enemyInfiniteItems` -> `fillItems` is set but never applied; `prebuildAi` teams get no defenders.
After: in an attack sector with a `buildAi` enemy, the enemy core holds max items and up to
`2 * cores` enemy core units exist after 6 s each.

**Java.**
* `J/ai/RtsAI.java:329-364` (`estimateStats`):
  ```java
  for(var turret : Vars.indexer.getEnemy(data.team, BlockFlag.turret)){
      if(turret instanceof BaseTurretBuild t && turret.block instanceof Turret tb && ((tb.targetAir && air) || (tb.targetGround && !air)) && Intersector.distanceSegmentPoint(fromX, fromY, x, y, t.x, t.y) <= t.range() + extraRadius){
          health[0] += t.health; dps[0] += t.estimateDps(); } }
  Tmp.r1.set(fromX, fromY, x - fromX, y - fromY).normalize().grow(140f * 2f);
  Units.nearbyEnemies(data.team, Tmp.r1, other -> {
      if(Intersector.distanceSegmentPoint(fromX, fromY, x, y, other.x, other.y) <= other.range() + extraRadius){
          health[0] += other.health; dps[0] += other.type.dpsEstimate; } });
  ```
  (`extraRadius = 50f`; `Rect.normalize()` makes width/height positive; `grow(280)` adds 140 on each side.)
* `Turret.estimateDps` (`J/world/blocks/defense/turrets/Turret.java:335-338`):
  `if(!hasAmmo()) return 0f; return shoot.shots / reload * 60f * (peekAmmo() == null ? 0f : peekAmmo().estimateDPS()) * potentialEfficiency * timeScale;`
  `ContinuousTurret` (`ContinuousTurret.java:43-46`): `if(!hasAmmo()) return 0f; return shootType.damage * 60f / (shootType instanceof ContinuousBulletType c ? c.damageInterval : 5f);`
  `TractorBeamTurret` (`TractorBeamTurret.java:150-153`): `if(!any || damage <= 0) return 0f; return damage * 60f * efficiency * coolantMultiplier;`
  `BulletType.estimateDPS` (`J/entities/bullet/BulletType.java:422-440`): `spawnUnit != null ? spawnUnit.estimateDps() : despawnUnit != null ? despawnUnit.estimateDps() : (damage + splashDamage*0.75f) * (pierce ? pierceCap == -1 ? 2 : clamp(pierceCap, 1, 2) : 1) + frag.estimateDPS() * fragBullets / 2 + Σ spawnBullets.estimateDPS()`.
* `J/ai/BaseBuilderAI.java:51-71`:
  ```java
  if(data.team.cores().size > 0){ var core = data.team.cores().first(); for(Item item : content.items()){ core.items.set(item, core.getMaximumAccepted(item)); } }
  if(data.team.rules().aiCoreSpawn && timer.get(timerSpawn, 60 * 6f) && data.hasCore()){
      CoreBlock block = (CoreBlock)data.core().block; int coreUnits = data.countType(block.unitType);
      if(!state.isEditor() && coreUnits < data.cores.size * coreUnitMultiplier){ Unit unit = block.unitType.create(data.team); unit.set(data.cores.random()); unit.add(); Fx.spawn.at(unit); } }
  ```
* `J/core/Logic.java:551-584`: `if(rules.fillItems && data.cores.size > 0){ var core = data.cores.first(); content.items().each(i -> { if(i.isOnPlanet(state.getPlanet()) && !i.isHidden()){ core.items.set(i, core.getMaximumAccepted(i)); } }); }`
  … `if(rules.prebuildAi && !state.isEditor()){ for(var core : data.cores){ var units = data.getUnits(((CoreBlock)core.block).unitType); if(units == null || !units.contains(u -> u.flag == core.pos())){ Unit unit = ((CoreBlock)core.block).unitType.spawn(core, data.team); unit.flag = core.pos(); unit.add(); ... } } }`
  `Rules.TeamRule.aiCoreSpawn = true` default (`J/game/Rules.java:353`); `fillItems` set for the wave team
  when `planet.enemyInfiniteItems` (`J/core/Logic.java:111-114`; port `core/game/play.rs:551-554`).

**Port today.** `core/ai/rts_ai.rs::estimate_stats` (402) filters every row by
`(target_air && air) || (target_ground && !air)`; `EnemyStat` (50) has no `is_turret`;
`core/game/runtime.rs::team_ai_enemy_stats` (~1415) = units only; `collect_team_ai_rows` (~1278).
Bullet DPS is computed only inside `core/content/registries/units/mod.rs::link` (~2405, local Vec,
ignores `spawn_unit`). `core/game/runtime.rs::run_team_base_builder` (~1170-1240) passes
`core_unit_count: 0` and drops the returned actions (`let _ = crate::ai::update_base_build(..)`);
`BaseBuildActions { fill_core_items, spawn_core_unit, .. }` (`core/ai/base_builder_ai.rs:~105`).
Turret state: `TurretState`, `TurretConfig { range, reload, shoot: ShootPatternSpec{shots,..},
target_air, target_ground, kind: TurretKind, .. }`, `turrets::peek_ammo(world, e) -> Option<BulletId>`,
`turrets::has_ammo(world, e)` (`core/world/blocks/defense/turrets/mod.rs:155-237,1033,1066`);
`Building.potential_efficiency`, `time_scale` (`core/entities/comp/building.rs:43,50`). Core unit type:
`BlockDef.core_unit: Option<&'static str>`. No unit `flag` field yet (`grep -rn "pub flag" core/entities/comp/unit` —
logic may store one; H06 owns `ucontrol flag`: reuse theirs if it exists, else add `UnitCore.flag: f64`).

**Changes.**
1. `BulletDef.estimate_dps: f32` (new field, default 0) filled in `units/mod.rs::link` (the existing
   bottom-up loop) including the `spawn_unit` / `despawn_unit` branches (`content.unit(id).estimate_dps()`
   — careful: unit DPS depends on bullet DPS; spawn-unit bullets point at missile units whose weapons'
   bullets precede them in id order; compute unit DPS for spawn units first if needed, document). Make
   `link` write `bullet.estimate_dps` and keep its unit `dps_estimate` computation reading it.
2. `EnemyStat.is_turret: bool`; `estimate_stats`: air/ground filter only for turret rows; unit rows also
   require the point inside the normalized from->to rect grown by 140 per side.
3. `collect_team_ai_rows`/`team_ai_enemy_stats`: append one row per entity with `TurretState` of another
   active non-derelict team: `range = config.range` (+ ammo `range_change` if the port tracks it — grep),
   `health`, `dps` per kind: `Item/Liquid/Power` (and `Laser`? — Java `LaserTurret extends PowerTurret`:
   `Turret.estimateDps`) -> `shots / reload * 60 * peek.estimate_dps * potential_efficiency * time_scale`
   (0 without ammo); `Continuous` -> `ContinuousTurret` formula (`damage_interval` field of the bullet:
   grep); `TractorBeam` -> its formula (fields `damage`, `any`, `coolant_multiplier`: grep the tractor
   state; if missing use 0 and note); others 0. `target_air/target_ground` from the config.
4. `run_team_base_builder`: apply `actions.fill_core_items` (first core of the team: every item to its
   maximum accepted — find the port's `getMaximumAccepted` (`grep -rn "fn maximum_accepted\|fn get_maximum_accepted" core/world`)
   and use it) and `actions.spawn_core_unit` (spawn `core_unit` of `data.core()` block at a random team
   core: `ai_rng` index into the sorted core list; `Sim::spawn_live_unit` is not reachable from the
   system — use `UnitRuntime::spawn(world, name, team, x, y, 0.0)` and push into `runtime.units` the
   way `run_wave_system` does); pass `core_unit_count = live_type_count(world, team, core_unit_id)`.
   Check the 360-tick timer lives in `BaseBuilderAi` state (`grep -n "timer_spawn\|TIMER_SPAWN" core/ai/base_builder_ai.rs`).
5. `team_ai_system` (or the campaign tick that calls the base builder; grep `fn team_ai_system`):
   per active team, `fill_items` -> first core filled with every non-hidden item on the current planet
   (`item.is_on_planet(planet)` + `!hidden`: grep the item def fields); `prebuild_ai` -> per core
   without a live unit of `core_unit` whose `flag == core_tile.pack()` spawn one with that flag
   (`UnitType.spawn(core, team)` = create at core position + `add`). These units run BuilderAI
   (`PrebuildAI` behaviour stays deferred: note it).

**Tests.** `estimate_stats_counts_loaded_turret` (`core/game/runtime.rs` tests or `CT`: duo with copper
ammo -> row dps = `shots / reload * 60 * copper_bullet.estimate_dps` read from content; empty duo ->
0); `estimate_stats_units_ignore_air_filter`; `base_builder_fills_enemy_core` (attack session, team 2
`buildAi`, core-shard team 2: after one `team_ai_system` run copper == max accepted);
`ai_core_spawn_caps_at_two_per_core` (run 3 spawn windows: 2 units with one core);
`prebuild_ai_spawns_one_defender_per_core`; `fill_items_rule_fills_first_core`.

**Verify in-engine.** Attack sector (env §4.1 fresh-sector recipe). Verified 2026-10-14: Onset
(`load_sector("erekir", 10)` + `start_sector("erekir", 10)`) has two malis (team 3) `core-bastion`s at
about tiles (287,196) and (293,288), but its team-3 rule has no `buildAi` (only `fillItems`,
`infiniteResources`, `rtsAi`), so the BaseBuilderAI/aiCoreSpawn half needs a `buildAi` sector (none
found; sector 225 unchecked) or stays headless: after 1200 steps
`unit_debug_json()` lists team-2 core units (alpha/beta/gamma) and `building_items_json(<enemy core
tile>)` shows full stacks. Record the sector used.

**Commit.** `Fixes PAR-0156: turret DPS in estimateStats, enemy core fill, aiCoreSpawn, fillItems and prebuildAi defenders`

---

### T11 — GroundAI stuck avoidance (`AvoidanceProcess`)

> **In-engine verification (2026-10-14, session 2):** NOT CHECKABLE in-engine (headless). Code confirmed (`flowfield.rs:178 get_target_tile`). Related, not in this task: the live flowfield has uniform step costs (no `costGround` deep/solid/team terms) — see T14's note.

**Symptom / acceptance.** A wave squad jammed in a 1-tile corridor never re-routes: Java marks units
"stuck" (`stuckTime >= stuckThreshold`) and asks the flowfield for a next tile that is not occupied by a
lower-id AI ground unit. After: in a corridor test two daggers stuck nose-to-nose separate faster than
without avoidance (measurable: the trailing dagger picks a different next tile).

**Java.** `J/ai/types/GroundAI.java:24,54,63-84` (stuck threshold, `pathfind(fieldCore, true, stuckTime >= stuckThreshold)`,
neighbour propagation), `J/entities/units/AIController.java:149-161` (`avoidance && unit.collisionLayer() == layerGround ? unit.id : 0`),
`J/ai/Pathfinder.java:382-436` (`getTargetTile` with `avoid[packed] > Integer.MAX_VALUE - avoidanceId ? 1 : 0`,
`cost < value && avoidance == 0`), `J/async/AvoidanceProcess.java:52-106` (per team that `isAI() && !rtsAi`,
every ground-layer unit marks tiles within `r = clamp(ceil(hitSize*0.6/8*2), 1, 20)` and `dx²+dy² <= rad²`
(`rad = hitSize * unitCollisionRadiusScale / tilesize * 2`) with `max(buffer, MAX - unit.id)`; double-buffered:
queries read the previous frame's grid).

**Port today.** `RT::update_ground_ai` tracks `stuck_time` (no propagation, threshold unused);
`core/ai/types/ground.rs::pathfind_field` calls `field.get_target_tile(index, width, height, true)`
(`core/ai/pathfinder/*.rs`; grep `fn get_target_tile`). Unit id = `EntitySeq` (use the low 31 bits as
`i32`, document).

**Changes.**
1. `UnitRuntime.avoidance: Vec<i32>` (len `w*h`) + `avoidance_active: bool`. At the **start** of
   `unit_update_system` (before the loop) and only when active: rebuild from the previous tick's unit
   positions (Java reads last frame's buffer: build it at the start of the tick from current positions =
   the end of last tick — equivalent). Teams: `team_ai_flags(world, team) == Some((true, false))`;
   layer ground = `collision_layer(..) == 0`.
2. `get_target_tile(.., avoid: Option<(&[i32], i32 /*id*/)>)`: add the avoidance term exactly as Java;
   all existing callers pass `None`.
3. `pathfind_field(.., avoidance: bool)`: pass `Some((&runtime.avoidance, id))` when `avoidance` and the
   unit is on the ground layer; first request sets `avoidance_active = true` (Java lazily activates).
4. `update_ground_ai`: `stuck_threshold = max(1, stuck_range*2/def.speed)`; pass
   `state.stuck_time >= stuck_threshold`; port the neighbour propagation (`GroundAI.java:67-75`) over
   same-team `AiKind::Ground` units within `radius = hitSize*0.6*2` (+ other's `hitSize*0.6`), in
   `EntitySeq` order, writing their `GroundAiState`.

**Tests.** `stuck_dagger_uses_avoidance` (corridor of copper walls, two team-2 daggers, a team-1 core
at the end; with avoidance both reach range of the core within N ticks; assert the trailing one's
`stuck_time` reset at least once); unit test for `get_target_tile` with an avoidance grid that blocks
the best neighbour -> returns the second best.

**Verify in-engine.** Headless only.

**Commit.** `Gameplay H01 T11: GroundAI stuck avoidance (AvoidanceProcess + flowfield avoid id)`

---

### T12 — `BuilderAI` following / assist (assist command, idle poly helping the player build)

> **In-engine verification (2026-10-14, session 2):** PARTLY: symptom code-confirmed (`ai/types/builder.rs:267` returns early for `only_assist`); the seam text was wrong and is fixed below: `content_list("unitCommand")` exists and returns `[move, repair, rebuild, assist, mine, enterPayload, loadUnits, loadBlocks, unloadPayload, loopPayload]`, so assist = index 3.

**Symptom / acceptance.** A poly with the `assist` command idles; an idle `rebuild` poly never helps a
building player/unit. After: a team-1 poly with `assist` moves to the player and copies the player's
current build plan (construct progress grows faster than the player alone).

**Java.** `J/ai/types/BuilderAI.java:46-80` (follow/assistFollowing validation, copy
`following.buildPlan()` into `unit.plans`), `:128-170` (every 20 ticks: `Units.nearby(team, x, y, buildRadius = 1500)`
first unit with `u.canBuild() && u != unit && u.activelyBuilding()` whose plan tile is a `ConstructBuild`
reachable: `dist = Math.min(cons.dst(unit) - unit.type.buildRange, 0); if(dist / unit.speed() < cons.buildCost * 0.9f)`;
`onlyAssist` -> `assistFollowing` = unit of the closest alive builder player of the team; `moveTo(assistFollowing,
assistFollowing.type.hitSize + unit.type.hitSize/2f + 60f)` when no plan). Read the full method and
transcribe these two blocks in order; they precede the rebuild pickup already ported.

**Port today.** `core/ai/types/builder.rs::update_builder_ai` (deferral note at the top; `only_assist`
returns early). "Actively building" = `crate::ai::types::player::player_actively_building(world, content,
entity, pos)` (BuilderComp.activelyBuilding). Player: `Sim.player_unit` is not in the world — use the
unit with the `comp::Player` marker. Construct state: `grep -rn "pub struct ConstructState\|build_cost" core/world/blocks/construct*`.

**Changes.** Add `following: Option<Entity>`, `assist_following: Option<Entity>`, `follow_timer: f32` to
`BuilderAiState`; port the two blocks; copying a plan = clear the unit's `BuilderPlans.plans` and push
the leader's front plan. Keep `update_builders` (core/sim/builder.rs) — progress from two builders adds
(Java allows assisting).

**Tests.** `assist_poly_follows_building_player` (player alpha with a queued `copper-wall` plan far away,
team-1 poly `assist`: after 600 ticks the poly is within `alpha_hit + poly_hit/2 + 65` of the player and its
plan equals the player's front plan); `idle_rebuild_poly_helps_builder`.

**Verify in-engine.** Ground Zero: spawn a team-1 poly (`spawn_unit`), give it `assist`
(`{"op":"unit_command","units":[<id>],"command":3,"x":<px>,"y":<px>}`; assist =
`content_list("unitCommand").find("assist")` = 3, verified 2026-10-14), queue a player build via `apply_sim_command_json` `place` with
`"player":0` far from the core; `construct_progress(x,y)` grows; screenshot.

**Commit.** `Gameplay H01 T12: BuilderAI follows and assists active builders`

---

### T13 — CargoAI / AssemblerAI audit and dispatch (low priority)

> **In-engine verification (2026-10-14, session 2):** NOT CHECKABLE in-engine (headless). Code confirmed (`AiKind::Cargo/Assembler` exist; `cargo.rs` 345, `assembler.rs` 72 lines).

**Symptom / acceptance.** `manifold` (CargoAI) and `assembly-drone` (AssemblerAI) never move.
After: both run their Java controllers when their blocks exist (Erekir; unit cargo loader/unload point,
unit assembler).

**Java.** `J/ai/types/CargoAI.java` (179 lines), `J/ai/types/AssemblerAI.java` (25 lines) — read fully.

**Port today.** `core/ai/types/cargo.rs` (345 lines), `core/ai/types/assembler.rs` (72 lines), not
dispatched (`AiKind::Cargo/Assembler` fall into the generic branch).

**Changes.** Compare line by line; fix deviations; add RT branches building an `AiCtx` like the
Missile branch with lazily-inserted state. If the needed block state (cargo loader/unload point
`unloadPoints`, assembler `UnitAssemblerBuild`) does not exist live (grep `UnitCargoLoader\|cargo_loader`
in `core/world/blocks`), dispatch only, record it, and add an H02 seam request in your summary.

**Tests.** `manifold_dispatches_cargo_ai` (spawns, does not run GroundAI: its slot kind stays `Cargo` and it
does not path to the enemy core); `assembly_drone_dispatches`.

**Verify in-engine.** Headless only.

**Commit.** `Gameplay H01 T13: CargoAI and AssemblerAI dispatched live`

---

### T14 — Ground Zero wave-1 fight acceptance (headless regression + in-engine)

> **In-engine verification (2026-10-14, session 2, with H00 T1+T2 temporarily applied):** CONFIRMED FAILING — but three of the causes are not in this plan. (1) Wave index off-by-one: `run_wave()` at wave 1 spawns 1 dagger + 2 flares; Java wave 1 spawns **1 dagger only** (`WaveSpawner.java:74` `getSpawned(state.wave - 1)` runs before `state.wave++`, `Logic.java:319-321`); port: `gdext/campaign.rs::run_wave` and `game/runtime.rs:402` pass the pre-increment wave and `run_wave_system` calls `get_spawned(wave)`. (2) The dagger drowned in deep water near (1088,1100) because the live flowfield has uniform step costs (Java `Pathfinder.costGround` deep +6000, all-deep impassable, `Pathfinder.java:56-62`). (3) The 2 flares circled the core; the duo (ammo 25 -> 0 in ~900 ticks) hit ~18% because port turrets never lead (`turrets/mod.rs:1829` vs `Turret.java:459-471` `Predict.intercept`); the core died ~3400 ticks after `run_wave`. Fixed: Java duo `range = 160` (`Blocks.java:3337`), not 110. The headless test must state which wave index it spawns; with Java's index wave 1 = one dagger. Evidence `runs/20261014-s2-verify/H00-T3-gameover.png`, `verify-s2-evidence.json`.

**Symptom / acceptance.** Round G's G00 smoke ("the wave dies to a duo, the core survives") failed:
the Ground Zero wave (observed 1 dagger + 2 flares — the port's wave-index off-by-one; Java wave 1 is
1 dagger, see the verification note) killed the core; the player alpha never existed
(env, fixed by H00) and no in-engine fight was ever passed. What a correct Java Ground Zero wave-1 fight
needs (re-derived): the wave groups come from the sector's own rules (`groundZero.msav` `rules.spawns`);
daggers walk the core flowfield (G01 T6a ✔) and stop at `range*0.5` of the core; flares
(`targetFlags {generator, null}`, `circleTarget`, `minShootVelocity 2`, `J/content/UnitTypes.java:1047-1088`)
find no generator, so `findMainTarget` returns the core (or the closest in-range target) and they
circle-attack it at `circleTargetRadius 60`; a drill-fed `duo` next to the core (`targetAir` and
`targetGround` true, range 160) shoots both; the player alpha (desktop input) only shoots while the
mouse is held, so the defence is the duo alone. Correct building positions (T1), per-mount targeting
(T2) and the velocity model (T4) are the pieces round G lacked. After: a headless test and the live
probe both kill wave 1 with `hasCore` true.

**Java.** As cited above, plus `J/world/blocks/defense/turrets/ItemTurret.java` ammo, `Blocks.duo`
(`grep -n "duo = new ItemTurret" -A70 J/content/Blocks.java`: `range = 160` at `Blocks.java:3337`, `targetAir` default true;
corrected 2026-10-14, the plan said 110).

**Port today.** `RT` tests `ground_zero_wave_spawns_live_units`, `ground_zero_drill_fed_duo_kills_wave_daggers`
(spawns a lone dagger, not the real wave), `ground_zero_wave_destroys_undefended_core`. Seams:
`MindCampaign.run_wave()`, `get_rules_json()` (wave groups), `SimHost.unit_debug_json()`.

**Changes.**
1. New headless test `ground_zero_wave_one_dies_to_drill_fed_duo` in the same `msav_tests` module as the
   existing Ground Zero tests: load `groundZero.msav` exactly like `ground_zero_drill_fed_duo_kills_wave_daggers`,
   install a `CampaignRuntime` with the map's rules (copy `ground_zero_wave_destroys_undefended_core`'s
   setup), place the drill + duo at the found site, tick 1800 (ammo), then `sim.request_wave_spawn(0)`
   (or the campaign `run_wave` path the other test uses) and tick until every team-2 unit is gone or
   6000 ticks pass. Assert: all wave units dead, the core exists, the duo exists. Print the wave
   composition (`eprintln!` type names) so the summary can record it.
2. No production code change in this task unless the test fails; if it fails, record which unit
   survives and why (positions/targets from `unit_debug_json`-like dumps) and file it in the summary —
   do not tune numbers.

**Verify in-engine.** Run this probe **before T1 as a baseline (no commit)** and again after T4 and at
the end:
```bash
$G --eval "$H.load_sector(\"serpulo\", 170); return true" --eval "return $MC.start_sector(\"serpulo\", 170)" \
   --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
$G --eval "return $H.has_player_unit()"                                       # true (H00)
$G --eval "return $H.place_block(130,48,\"mechanical-drill\")" --eval "return $H.place_block(132,48,\"duo\")"
$G --eval "$H.set_paused(true); $H.step(600); return $H.get_tick()"            # x3 (1800 ticks of ammo)
$G --eval "return JSON.parse_string($MC.get_rules_json()).spawns"             # record wave groups
$G --eval "return $MC.run_wave()"
# repeat 10x: step 600, then read unit_debug_positions() and get_hud_state().hasCore
$G --eval "$H.step(600); return $H.unit_debug_positions()"
$G --eval "return $MC.get_hud_state()"                                        # "hasCore": true at the end
```
Pass = after ≤ 6000 ticks no team-2 unit remains, `hasCore` true, `has_player_unit()` true.
Screenshots every 1200 ticks to `runs/<date>-s<N>-H01-T14/`. `grep -c panic` = 0.

**Pitfalls.** If the rules' wave 1 has more than the duo can handle in Java too, the test must still
reflect Java: compute the expected outcome from the rules, do not add turrets beyond one duo without
writing why. If `run_wave` spawns nothing (`waveTimer` false on the imported save, env §4.1 note),
use `request_wave_spawn` semantics through `run_wave()` only — record it.

**Commit.** `Gameplay H01 T14: Ground Zero wave-1 fight acceptance test`

---

### T15 — Java `Pathfinder` tile data and per-cost weights (ground units stop walking into deep water) (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** On Ground Zero the wave-1 dagger walks straight through deep
water near world (1088,1100) (tiles ~136,137-139), gets `wet`, slows to 0.09 px/tick and
drowns ~1500 ticks after `run_wave()` (verification evidence
`runs/20261014-s2-verify/verify-s2-evidence.json`, H00 T3 row). In Java the same dagger
walks around the water: ground costs add +6000 per deep tile and make all-deep tiles
impassable, and `getTargetTile` refuses to step from a safe tile onto a dangerous one.
Cause in the port: the flowfield has no floor data at all (`build_tiles` passes
`deep = false, damages = false`, never sets liquid/near-*/all-deep/team-passable bits),
every passable tile costs 1 (diagonals √2), and every tile's team is the PATHING team
instead of the tile's own team. After: the port packs `PathTile` exactly like
`Pathfinder.packTile`, the six Java cost functions decide weights and passability, and
the dagger reaches the core over land.

**Java.**
* `J/ai/Pathfinder.java:221-276` `packTile(Tile tile)`:
  ```java
  boolean nearLiquid = false, nearSolid = false, nearLegSolid = false, nearGround = false, solid = tile.solid(), allDeep = tile.isDeep(), nearDeep = allDeep;
  for(int i = 0; i < 4; i++){
      Tile other = tile.nearby(i);
      if(other != null){
          Floor floor = other.floor();
          boolean osolid = other.solid();
          boolean deep = other.isDeep();
          if(floor.isLiquid && deep) nearLiquid = true;
          if(osolid && !other.block().teamPassable) nearSolid = true;
          if(!floor.isLiquid) nearGround = true;
          if(!deep){ allDeep = false; }else{ nearDeep = true; }
          if(other.legSolid()) nearLegSolid = true;
          //other tile is now near solid
          if(solid && !tile.block().teamPassable && other.array() < tiles.length){
              tiles[other.array()] |= PathTile.bitMaskNearSolid;
          }
      }
  }
  //check diagonals for allDeep
  if(allDeep){
      for(int i = 0; i < 4; i++){
          Tile other = tile.nearby(Geometry.d8edge[i]);
          if(other != null && !other.isDeep()){ allDeep = false; break; }
      }
  }
  int tid = tile.getTeamID();
  return PathTile.get(
      tile.build == null || !solid || tile.block() instanceof CoreBlock ? 0 : Math.min((int)(tile.build.health / 40), 80),
      tid == 0 && tile.build != null && state.rules.coreCapture ? 255 : tid,
      solid,
      tile.floor().isLiquid && !tile.block().solid,   // liquid
      tile.legSolid(),
      nearLiquid, nearGround, nearSolid, nearLegSolid,
      tile.isDeep(),                                   // deep
      tile.floor().damages(),
      allDeep, nearDeep,
      tile.block().teamPassable);
  ```
  `Tile.solid()` = `block.solid || floor.solid || (build != null && build.checkSolid())`
  (`J/world/Tile.java:462-464`); `Tile.isDeep()` = `block == Blocks.air && floor.isDeep()`
  (`:475-477`); `Floor.isDeep()` = `drownTime > 0` (`Floor.java:339-341`);
  `Floor.damages()` = `damageTaken > 0f || (status != null && status.damage > 0f)`
  (`Floor.java:431-433`); `Tile.dangerous()` = `!block.solid && (floor.isDeep() || floor.damages())`
  (`Tile.java:471-473`); `teamPassable` is true only for `AutoDoor` (`AutoDoor.java:38`).
  `Geometry.d4` = (1,0),(0,1),(-1,0),(0,-1); `d8edge` = (1,1),(-1,1),(-1,-1),(1,-1).
* `J/ai/Pathfinder.java:34` `impassable = -1`; costs `:53-91`:
  ```java
  //ground
  (team, tile) ->
  (PathTile.allDeep(tile) || ((PathTile.team(tile) == team && !PathTile.teamPassable(tile)) || PathTile.team(tile) == 0) && PathTile.solid(tile)) ? impassable : 1 +
  PathTile.health(tile) * 5 + (PathTile.nearSolid(tile) ? 2 : 0) + (PathTile.nearLiquid(tile) ? 6 : 0) + (PathTile.deep(tile) ? 6000 : 0) + (PathTile.damages(tile) ? 30 : 0),
  //legs
  (team, tile) -> PathTile.legSolid(tile) ? impassable : 1 + (PathTile.deep(tile) ? 6000 : 0) + (PathTile.solid(tile) ? 5 : 0),
  //water
  (team, tile) ->
  (!PathTile.liquid(tile) || (PathTile.solid(tile) && (PathTile.team(tile) == team || PathTile.team(tile) == 0)) ? 7000 : 1 + PathTile.health(tile) * 5) +
  (PathTile.nearGround(tile) || PathTile.nearSolid(tile) ? 14 : 0) + (PathTile.deep(tile) ? 0 : 1) + (PathTile.damages(tile) ? 35 : 0),
  //neoplasm veins
  (team, tile) -> (PathTile.deep(tile) || (PathTile.team(tile) == 0 && PathTile.solid(tile))) ? impassable : 1 +
  (PathTile.health(tile) * 3) + (PathTile.nearSolid(tile) ? 2 : 0) + (PathTile.nearLiquid(tile) ? 2 : 0),
  //none (flat cost)
  (team, tile) -> 1,
  //hover
  (team, tile) -> (((PathTile.team(tile) == team && !PathTile.teamPassable(tile)) || PathTile.team(tile) == 0) && PathTile.solid(tile)) ? impassable : 1 +
  PathTile.health(tile) * 5 + (PathTile.nearSolid(tile) ? 2 : 0)
  ```
  Note the ground ternary's precedence: `allDeep || (((team==team && !teamPassable) || team==0) && solid)`.
  An ENEMY team's solid building is NOT impassable for ground (it costs `health*5`): units
  path through enemy walls and shoot them.
* `Flowfield.passable(pos)` (`:698-703`): `amount != impassable && !(cost == naval && amount >= 6000)`.
* Frontier (`:497-533`): integer weights; neighbours = `Geometry.d4` ONLY;
  `if((weights[newPos] > cost + otherCost || searches[newPos] < search) && otherCost != impassable)`
  -> `weights[newPos] = cost + otherCost` (cost of ENTERING the neighbour).
* `getTargetTile` (`:400-436`): neighbours `diagonals ? d8 : d4`; picks the smallest
  `values[packed]` that is `< value` (current tile) and `path.passable(packed)`, skipping a
  diagonal whose two orthogonal tiles are not passable; then
  `if(current == null || tl == impassable || (path.cost == costTypes.items[costGround] && current.dangerous() && !tile.dangerous())) return tile;`
* `Pathfinder.updateTile(tile)` (`:306-320`): re-packs every linked tile of the building
  (`getLinkedTiles`) and marks a refresh.

**Port today.**
* `core/ai/pathfinder/path_tile.rs`: bit layout matches `PathTileStruct` (bits for
  liquid/legSolid/near*/deep/damages/allDeep/nearDeep/teamPassable exist as consts), but
  `build_tiles(grid, content, team)` only sets health (from `def.health`, the block's MAX
  health, also for cores and non-solid blocks), `team` = the pathing team for EVERY tile,
  and `solid`; accessors exist only for health/team/solid/deep/damages.
* `core/ai/pathfinder/cost.rs`: `Cost::passable` = `!solid` (naval `!solid && deep`); no
  weights.
* `core/ai/pathfinder/flowfield.rs` `advance`: Dijkstra over `DIRS` incl. diagonals,
  `f32` weights, step 1 or `SQRT2`. `get_target_tile(node, w, h, diagonals)`: passability
  = finite weight; no dangerous check.
* `core/ai/pathfinder/mod.rs` `Pathfinder::rebuild(grid, content, team)` and
  `update_tile(grid, content, x, y, team)` (same simplified packing, one tile).
* `core/world/tile.rs:132` `Tile::dangerous` always returns `false`.
* Floor data is on `BlockDef`: `is_liquid`, `is_deep`, `drown_time`, `floor_damage_taken`,
  `floor_status` (`core/content/registries/blocks/mod.rs` ~2184-2198); status damage via
  the status registry (`grep -n "pub damage" core/content/registries/status*.rs`).
* Callers: `core/ai/types/ground.rs:180,250`, `core/ai/ai_controller.rs:293`
  (`get_target_tile(.., true)`), `core/ai/control_pathfinder.rs:130` (`build_tiles`).

**Changes.**
1. `core/world/tile.rs`: implement `Tile::dangerous(&self, content)` =
   `!block.solid && (floor.is_deep || floor_damages(floor))` and add
   `pub fn floor_damages(def: &BlockDef, content: &ContentRegistry) -> bool`
   (`floor_damage_taken > 0 || floor_status.is_some_and(|s| content.status(s).damage > 0)`),
   `pub fn is_deep(&self, content) -> bool` (`block == AIR && floor.is_deep`) and
   `pub fn solid_full(&self, content) -> bool` (`block.solid || floor.solid` — the port
   has no `checkSolid` override; grep `fn check_solid` first and include it if present).
2. `path_tile.rs`: add the missing accessors (`liquid`, `leg_solid`, `near_liquid`,
   `near_ground`, `near_solid`, `near_leg_solid`, `all_deep`, `near_deep`,
   `team_passable`) and a full constructor. Replace `build_tiles` with a faithful
   `pack_tile(grid, content, rules_core_capture: bool, x, y, tiles: &mut [PathTile]) -> PathTile`
   (it also ORs `BIT_NEAR_SOLID` into the 4 neighbours, like Java) and make
   `build_tiles(grid, content, core_capture)` loop it in row-major order (Java packs in
   `tiles` index order; the neighbour OR makes order irrelevant for the final bits).
   Health = `build.health / 40` capped 80 only when the tile has a building, is solid and is
   not a core; team = the tile's building team (0 when none); 255 for team-0 buildings when
   `rules.core_capture`. The `team` parameter is removed — check every caller
   (`grep -rn "build_tiles(\|update_tile(" client/rust/mind-core/src/ai`) and pass
   `rules.core_capture` (false when no rules).
3. `cost.rs`: `pub const IMPASSABLE: i32 = -1;` and
   `pub fn cost(self, team: u8, tile: PathTile) -> i32` transcribing the six lambdas above
   exactly (ids: Ground 0, Legs 1, Naval 2, Neoplasm 3, None 4, Hover 5). Replace
   `passable` with Java's `Flowfield.passable`:
   `let c = self.cost(team, tile); c != IMPASSABLE && !(self == Cost::Naval && c >= 6000)`
   (takes `team` now; update callers).
4. `flowfield.rs`: integer weights (`i32`, unreached = `i32::MAX`), neighbours = d4 only in
   `advance`, `next_weight = weights[node] + cost.cost(team, tiles[next])` when that cost
   is not `IMPASSABLE`. Keep the existing priority queue (the converged weights equal Java's
   label-correcting result); keep the per-tick budget. `Flowfield` needs the field's team:
   store it at creation (`FieldKey.team`).
5. `get_target_tile`: passability via `cost.passable(team, tiles[i])` (needs `tiles` and
   `cost` arguments), d8 when `diagonals`, the corner-trap rule, strict `<` against the
   current value, then Java's final guard: return `None` (stay) when nothing qualifies, or
   when `cost == Cost::Ground && dangerous[next] && !dangerous[node]`. `dangerous` must come
   from `Tile::dangerous` — compute it once per rebuild into a `Vec<bool>` kept next to
   `tiles` in `Pathfinder` (and refresh it in `update_tile`).
6. `Pathfinder::update_tile`: re-pack the building's linked tiles (all tiles of a multiblock:
   use the grid's multiblock footprint helper, `grep -n "fn linked_tiles\|fn footprint" client/rust/mind-core/src/world`)
   plus their 4 neighbours (their near-solid bit can change), then invalidate fields as today.
7. Update callers in `ground.rs`, `ai_controller.rs`, `control_pathfinder.rs`, and every test
   that builds tiles with `from_parts` (keep `from_parts` for tests).

**Tests** (`core/ai/pathfinder/*.rs` tests + one msav test):
* `ground_cost_matches_java`: hand-built `PathTile`s -> plain floor 1; near-solid 3;
  deep 6001 (+6 if near liquid); all-deep `IMPASSABLE`; own-team solid `IMPASSABLE`;
  enemy-team solid with health 10 -> `1 + 50 = 51`; team-0 solid `IMPASSABLE`; damaging
  floor +30.
* `naval_cost_land_is_not_passable` (cost 7000+ -> `passable` false).
* `flowfield_routes_around_deep_water`: 9x5 grid, target at (8,2), a 3x3 deep pool in the
  middle (floor `deep-water`), start (0,2): following `get_target_tile` never visits a deep
  tile and reaches the target.
* `get_target_tile_refuses_dangerous_step`: start on safe ground next to a shallow damaging
  floor that is the only downhill neighbour -> returns `None`.
* `pack_tile_marks_neighbours_near_solid` and `pack_tile_team_is_building_team`.
* `msav_tests::ground_zero_wave_dagger_avoids_deep_water`: Ground Zero loader prologue
  (copy `ground_zero_wave_spawns_live_units`), spawn the wave as that test does, tick 3000,
  sample the dagger every 30 ticks: it is never on a tile whose floor `is_deep`, and it
  never gains the `wet` status from deep water (read `StatusComp`).
* Goldens: `units_*` goldens that move units will change; regenerate per H-environment §2
  and cite `Pathfinder.java:53-91`.

**Verify in-engine.** After H00 (wave 1 = one dagger):
```bash
# Ground Zero via "Go"; drill + duo as in H00 T3 is optional (we want to watch the path)
$G --eval "$H.set_paused(true); return $MC.run_wave()"
# sample 12 times:
$G --eval "$H.step(250); return $H.unit_debug_positions()"
```
The dagger's positions must never fall on the deep pool around world (1088,1100)
(tiles ~(136,137..139)); `unit_debug_json()` never shows `wet` on it while it is away from
shallow water; it ends near the core (within `type.range * 0.5` of (1036,444)/(1032,440)).
Screenshot `runs/<date>-s<N>-H01-T15/dagger-path.png`.

**Pitfalls.** The neighbour-OR of `BIT_NEAR_SOLID` must survive the per-tile repack in
`update_tile` — repack the 4 neighbours too. Enemy solid buildings stay passable for ground
(cost = health*5) — do not "fix" that; GroundAI shoots its way through. Keep the existing
avoidance/stuck code (T11) working with integer weights.

**Commit.** `Gameplay H01 T15: Java Pathfinder tile data and per-cost weights (ground units avoid deep water)`

---

### Deferred (not this round; why)

* `PrebuildAI` behaviour (`J/ai/types/PrebuildAI.java`, 250 lines, upstream "EXPERIMENTAL, DO NOT USE";
  T10 only spawns its defenders).
* Enemy base placement through builder units instead of the instant `AiPlaceRequest` placement
  (`core/game/runtime.rs::run_team_base_builder`) — large; after T10/T12.
* `randomWaveAI` (`FlyingAI.java:49-60`, `EnemyCoreField` random targets) — rule default off.
* `unloadPayloads()`, payload commands (`enterPayload/loadUnits/loadBlocks/unloadPayload/loopPayload`),
  CommandAI command queue + `patrol`/`ram` stances (`J/ai/types/CommandAI.java:124-131,250-360,486`) —
  RTS/Erekir carrier gameplay; needs H08's queued-order input first.
* `Time.run` delays of explosion waves/lightning (immediate in the port).
* `isFlying() && !canLand()` boost term (needs `Units.any` overlap query).
* `turret-unit-build-tower`.
* Saving the new per-unit state (drown time, falling, AI states, unit `flag`) — H04 (§6).

---

## §3 Execution order, dependencies, shared hot files

Order (player impact on Ground Zero / early Serpulo first; dependencies respected):

1. T14 in-engine probe as an uncommitted **baseline** (record the result).
2. **T1** (coordinates) -> **T15** (pathfinder costs; Ground Zero daggers drown without it) -> **T2** (weapons; uses T1's positions) -> **T3** (player lifecycle; PAR-0174).
3. **T4** (velocity) -> **T5** (drag uses T4) -> **T6** (falling uses T4/T5).
4. **T7**, **T8** (independent).
5. **T9** (uses T2's `update_ai_weapons` and T4's `move_pref`).
6. **T10** (PAR-0156).
7. **T11**, **T12**, **T13** (lowest).
8. **T14** commit (headless test + final in-engine probe).

After T1, T2, T4 and T14: rebuild + `tools/session-play.sh N --import` and run the T14 in-engine probe.

Shared hot files (pull `main` before touching; minimal edits; cite the task):
* `core/sim/unit_runtime.rs` — every task. Biggest conflict risk with H09/H06 (LogicAI branch) and
  H04 (wave/run_wave). Keep new code in new helper functions/modules where the task allows.
* `core/world/update.rs` (H02): T1 one-line change of the building centre.
* `core/combat/bullet/mod.rs` (H02 turrets / H07 weather wind): T1 `to_tile`, T4 knockback via `impulse`.
* `core/ai/types/{ground,flying,player,missile}.rs` (H08 uses `player.rs`): T4.
* `core/entities/comp/unit/environment.rs`: T4, T5.
* `core/game/runtime.rs` (H04): T10 only.
* `core/ai/pathfinder/*`, `core/world/tile.rs`: T15 (H09 T2 fog gates also touch targeting, not the pathfinder).
* `core/content/registries/{bullets.rs,units/mod.rs}`: T7 `FIREBALL`, T10 `BulletDef.estimate_dps`
  (not part of `golden_content.json` field subset? — check; if it is, regenerate per env §2).
* `gdext/sim_host.rs` (everyone): T3 `debug_kill_unit` + `set_player_team` call.
* Goldens: `client/rust/mind-headless/tests/golden/units/*.json` (T1, T4, maybe T2/T6),
  `client/rust/mind-core/tests/golden/combat.json` (T1/T7 maybe) — always with manifest sha and the Java
  reason in the commit body.

---

## §4 Ledger changes to report (executors never edit `chains.json`)

| Item | Now | Proposed after this plan | Note |
|---|---|---|---|
| PAR-0174 | fixing | **verified** after T3's in-engine check | live respawn + possession round trip |
| PAR-0202 | fixing | **verified** after T4 (in-engine sandstorm drift) | kinematic mover replaced |
| PAR-0156 | seeded | **fixed** (T10) | prebuildAi behaviour stays deferred |
| PAR-0091 | fixing | **fixed** if T7 lands (lightning/fireballs were the last residual) | |
| PAR-0229 | verified | note: mount half of FlyingAI.findTarget was inverted, fixed by T2 | reopen-note only |
| PAR-0237 | verified | note: absorption semantics fixed by T7 | |
| PAR-0175 | verified | note: dock branch left the possessed unit player-controlled; fixed by T3 | |

New items to file:
* "Buildings sit half a tile off for odd sizes and bullets look up tiles with floor (units shoot past walls)" -> T1.
* "AI weapons share one target, use the shooter's hitSize for range, never auto-drop bombs (horizon)" -> T2.
* "Dead flying units vanish instead of falling as wrecks" -> T6.
* "Unit drag/ice sliding missing; enemy units survive on captured sectors" -> T5.
* "Flying target flags ignore working buildings without items (status)" -> T8.
* "DefenderAI/FlyingFollowAI/HugAI/NoAI never run live" -> T9.
* "Wave ground units never use stuck avoidance" -> T11.
* "Assist command / BuilderAI follow does nothing" -> T12.
* "CargoAI/AssemblerAI never run" -> T13.
* "Ground Zero wave-1 fight never verified" -> T14 (acceptance item).

---

## §5 Done criteria

* Every task committed with its subject, or reported blocked with the observed reason in
  `docs/H01-units-ai-combat_summary.md` (env §6 shape, main checkout, not committed).
* `$L cargo test -p mind-core --lib --features msav-import`: 0 failed, count = baseline + new tests.
* `$L cargo test -p mind-headless --lib`: only the H00 known failures.
* The G01 JVM compare script still prints only `MISSING turret-unit-build-tower`.
* clippy clean, fmt applied; GDScript gate only if a `.gd` was touched (H01 should not touch any).
* In-engine: T3 respawn + possession, T4 sandstorm drift, T14 wave-1 fight pass; evidence under
  `runs/<date>-s<N>-H01-*`; `grep -c panic` = 0 for every run log.
* Summary lists every deviation (Java behaviour not reproduced) with file:line, and §4.

---

## §6 Cross-area seams (provider / consumer, exact names)

**H01 provides:**
* `Sim::set_player_team(&mut self, team: u8)`; `Sim::debug_kill_unit(&mut self, seq: i64) -> bool`;
  `#[func] SimHost.debug_kill_unit(seq: i64) -> bool` (T3) — usable by every plan's in-engine checks.
* `Sim::drain_pending_unlocks(&mut self) -> Vec<String>` (T3) — **consumer H05** (campaign unlock store in
  `gdext/campaign.rs`: drain once per frame and unlock each unit name through the existing research
  unlock path `core/game/tech_tree.rs::unlock`/`UnlockFields::unlock`, Java `CoreBlock.java:121-123`).
* `crate::entities::comp::unit::velocity::{move_at, move_pref, impulse, vel_update, collisions_move}` (T4) —
  **consumers H07** (weather wind must call `velocity::impulse` or add to `Vel` only, never `Pos`) and
  **H08** (player input writes `Vel` via `move_at`, never integrates `Pos`).
* `crate::world::status::java_block_status(world, entity, tick) -> BlockStatus` (T8) — H03 may use it for
  the F6 status markers instead of the UI approximation.
* Building `Pos` = `tile*8 + def.offset` (T1) — **H02 / H03 / H09** must not assume `(tile+0.5)*8`.

**H01 needs:**
* **H02**: `core/world/blocks/power/nodes.rs:70-101` (laser range), `core/world/blocks/power/explosion.rs:50-51`,
  `core/world/blocks/payloads/payload_block.rs:328-340` still use the half-tile convention — convert to
  `tile*8 + offset` / `WorldGrid::to_tile` after T1 lands. Also `Building.unitOn/unitOnAny` call sites
  (`J/entities/comp/UnitComp.java:833-844`, conveyors push units) remain unwired: H02 should expose
  `pub fn building_unit_on(world, build: Entity, unit: Entity)` and H01's `environment.rs` will call it
  (marked call-site comment already there).
* **H04**: save/load of new unit state (`DrownComp`, `Falling`, `GroundAiState.stuck_*`, unit `flag`,
  `AiWeaponsState`), Java saves `flag`, `mineTile`, CommandAI `keepState`, `plans`.
* **H06**: if `ucontrol flag` / `sensor @flag` already stores a unit flag, T10 reuses that field; name it in
  `docs/H-INDEX.md` §3.
* **H08**: player movement must stop integrating `Pos` itself (T4 changes `player.rs`); command queue +
  patrol/ram stances input before the deferred CommandAI work.
* **env doc**: `docs/H-environment.md` §4 says "World pixel = tile*8 (+4 = tile centre)"; Java (and the
  gdext renderer) put the tile centre **at** `tile*8`. After T1 the sim agrees; the coordinator should
  fix that line.
