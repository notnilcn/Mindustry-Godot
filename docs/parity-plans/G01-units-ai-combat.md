<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G01 — Units, AI and combat simulation (gameplay-parity plan, 2026-10-13)

This plan is self-contained. Read `docs/parity-plans/00-environment.md` first (session/worktree,
build, tests, goldens, gmcp eval rules, Ground Zero recipe, commit rules); this file only
references its sections (`env §N`). Work the tasks in the order of §3. Each task is one commit.
If what you see in the code differs from what a task says (a symbol is missing, a line moved
and cannot be found with `grep -n`, a test value differs from the derivation), stop that task,
write down what you saw in your summary, and continue with the next independent task. Do not
improvise designs. G01 is the **simulation** only: anything input-side belongs to G08,
anything drawn belongs to G03 (see §6).

Precondition — start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line).

Path abbreviations (`core/`, `gdext/`, `headless/`, `ui/`, `scenes/`, `J/`, `ARC/`) are defined in
`env` (top table). Java line numbers were checked on 2026-10-13. Port line numbers drift: when a
line moved, `grep -n` for the quoted function name.

Common shorthand used below:
* `RT` = `core/sim/unit_runtime.rs` (the live unit runtime: `unit_update_system`,
  `bullet_collide_system`, `run_wave_system`, `Sim::resolve_deaths`, `acquire_target`).
* `BUL` = `core/combat/bullet/mod.rs`; `LINE` = `core/combat/damage/line.rs`;
  `AREA` = `core/combat/damage/area.rs`; `UDEF` = `core/content/registries/units/mod.rs`;
  `BDEF` = `core/content/registries/bullets.rs`; `WDEF` = `core/content/registries/units/weapon.rs`;
  `GRT` = `core/game/runtime.rs` (team AI system).
* Test helpers in `core/sim/combat_tests.rs`: `live_sim(size) -> Sim` (block + unit runtime,
  **no** `CampaignRuntime`; every team except 0 is then an AI team), `place(&mut sim, x, y,
  "name", team) -> Entity`, `feed(&mut sim, turret, "item", n)`, `health(&sim, e)`,
  `Sim::spawn_live_unit(name, team, x, y, rot) -> Option<Entity>`,
  `Sim::spawn_player_unit(name, team, x, y, rot)`. To get a player team (CommandAI units) copy
  the setup of `live_unit_command_switch_is_stored` (same file): insert
  `CampaignRuntime::new(PlaySession::new(Rules { default_team: 1, ..Rules::default() }), campaign)`.
* JVM reference values: `parity/java/jvm_golden_content.json` (unit fields `health, armor, speed,
  hit_size, item_capacity, range, max_range, can_attack, …`). Compare script used in this audit
  (keep it; T2 uses it):
  ```bash
  python3 - <<'EOF'
  import json
  j=json.load(open('parity/java/jvm_golden_content.json'));p=json.load(open('parity/golden_content.json'))
  U=lambda g:{e['name']:e['fields'] for t in g['types'] if t.get('type')=='unit' for e in t['entries']}
  J,P=U(j),U(p)
  for n,f in J.items():
      if n not in P: print('MISSING',n); continue
      for k,v in f.items():
          pv=P[n].get(k)
          if (isinstance(v,(int,float)) and isinstance(pv,(int,float)) and abs(v-pv)>1e-3*max(1,abs(v))) or (not isinstance(v,(int,float)) and pv!=v): print(n,k,'jvm',v,'port',pv)
  EOF
  ```
  On the audited tree it prints (besides `MISSING turret-unit-build-tower`, out of scope):
  `pulsar/quasar/corvus/spiroct/arkyid/toxopid/eclipse/omura/locus range -4` (JVM 65/146/456/36/51/86/184/496/156),
  their `max_range 0`, `vela range 116` (176), `oxynoe max_range 96` (57.2), `cyerce range 126` (221.66),
  `navanax range 151` (91), `cleroi range 96` (136), `evoke/incite/emanate can_attack True` (False).

---

## §1 Inventory (Java -> port -> status)

Status legend: **OK** ported and live; **NW** ported but not wired into the live client;
**P** partial/divergent; **M** missing. "Task" = the task in §2 that fixes it.

### 1.1 Content (unit types, weapons, bullets, abilities, statuses)

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `J/content/UnitTypes.java` (all 69 vanilla units incl. missiles: anthicus/quell/disrupt-missile, 4 scathe missiles) | `core/content/registries/units/{standard,erekir,special}.rs` | OK (names/ids) | JVM-vs-port unit list identical except `turret-unit-build-tower` (Java build-tower turret unit; not used by vanilla gameplay paths audited here) | — |
| Unit `range`/`maxRange` derivation (`UnitType.init` 966-997) | `UDEF` ~2033-2063 | **P** | 14 units with wrong range (list above): bullet `calculateRange` kind overrides missing in `BDEF::compute_range`; `RepairBeamWeapon`/`PointDefenseWeapon` initializer defaults missing in `WDEF` ~745-772 | T2 |
| `UnitType.canAttack` (`UnitType.java:1065`) | `UDEF:2124` | **P** | evoke/incite/emanate true (RepairBeamWeapon lacks `noAttack`) | T2 |
| `BulletType` subclasses `J/entities/bullet/*` | `core/combat/bullet/kinds/*`, `behavior.rs:105 behavior_for` | see 1.2 | | |
| `J/entities/abilities/*` (ForceField, RepairField, ShieldRegenField, StatusField, EnergyField, SuppressionField, ShieldArc, MoveEffect, SpawnDeath, Regen, LiquidRegen, LiquidExplode; unused by vanilla: ArmorPlate, MoveLightning, UnitSpawn, EmptyData) | `core/combat/abilities.rs` (`update_ability` 243-376, `run_death_abilities` 879) | OK | every ability used by `UnitTypes.java` has a branch; MoveEffect is view-only by design | — |
| `J/content/StatusEffects.java` + `StatusComp` (apply/transition/opposite/affinity, damage, interval damage, multipliers) | `core/combat/damage/status.rs` (`UnitStatusApply`, `update_unit_status`) | OK, except floor statuses | `StatusComp.update` floor status (`StatusComp.java:192-197`) not ported | T13 |
| Weapons `J/type/weapons/*` (Build, Mine, PointDefense, PointDefenseBullet, RepairBeam) | `core/weapons/*` | OK (behaviour); defaults **P** | see T2 | T2 |

### 1.2 Bullets and damage

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `BasicBulletType`, `MissileBulletType`, `BombBulletType`, `ArtilleryBulletType`, `ExplosionBulletType`, `FlakBulletType`, `InterceptorBulletType`, `EmpBulletType`, `LiquidBulletType`, `FireBulletType`, `PointBulletType`, `MultiBulletType`, `MassDriverBolt`, `EmptyBulletType`, `LightningBulletType` | `kinds/{basic,flak,interceptor,emp,liquid,fire,point,multi,mass_driver,empty,lightning}.rs`, base behaviour | OK | field-driven; covered by `combat_tests.rs` / `bullet/mod.rs` tests | — |
| `LaserBulletType`, `ContinuousBulletType` (+Laser/Flame/PointLaser), `ShrapnelBulletType` -> `Damage.collideLine` | `kinds/{laser,continuous,shrapnel}.rs` -> `LINE::collide_line` | **P (critical)** | `collide_line` (LINE:60-112) damages **buildings only**; units on the line are never hit (lancer/meltdown/fuse/quasar/vela/corvus/… never damage units) | T3 |
| `RailBulletType` (locus, omura, foreshadow) | `BulletKind::Rail` -> `behavior_for` falls to `BASE` | **M** | `behavior.rs:125` `_ => &BASE`; rail `speed 0, collides false, lifetime 1` => deals nothing | T3 |
| `SapBulletType` (spiroct, arkyid) | `kinds/sap.rs` | **P** | uses `LINE::linecast` (buildings only, stops on own-team buildings), no unit hit, no `sapStrength` heal | T3 |
| `BulletType.hitTile` heal branch (`healPercent/healAmount`, `collidesTeam`) | `BUL::collide_bullet` 940-1000 | **P (critical)** | same-team hit calls `hit_building` => **damages** friendly blocks (nova/pulsar/poly/mega heal shots hurt the player's base); heal never applied | T4 |
| `BulletType.damageMultiplier` x `rules.unitDamage/blockDamage`; `unitHealth/blockHealth` intake | `combat/bullet/spawn.rs:106-121`, `AREA::apply_health` | **M** | `Rules::unit_damage/block_damage/unit_health/block_health` (`core/game/rules.rs:166-196`) are never called by combat | T5 |
| `Damage.dynamicExplosion` (building) | `core/combat/damage/explosion.rs:26` | P | waves ok; lightning/fireball/ExplosionShield absorption missing; `Block.baseExplosiveness` etc. absent | T14 |
| `UnitComp.destroy` -> `dynamicExplosion` (`UnitComp.java:887-893`) | — | **M** | `resolve_deaths` (RT ~374) does crash damage only | T14 |
| `Fires.create` gate `rules.fire && hasEnv(oxygen)` | `core/combat/fires.rs:293 env_allows_fire` | NW | `CombatEnv` resource is never inserted in the live sim (only a test inserts it) => fires burn on Erekir/fire-off maps | T14 |
| `BuildingComp.damage(Bullet,…)` -> `BuildDamageEvent` | `core/world/events.rs:123` struct only | **M** | never emitted (minimap attack indicators + RtsAI `damagedSet` have no feed) | T15 |

### 1.3 AI and controllers

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `AIController.updateWeapons/target/findMainTarget/retarget` | RT `acquire_target` 2355, `set_mount_targets` | P | no retarget timers (every tick), mount fallback "anchor building within range+16" not in Java | T6a |
| `GroundAI` (`fieldCore` flowfield to enemy cores, stop at `range*0.5`, core override at `range/1.3`, drop-zone stop, stuck) | RT generic branch + `core/ai/types/ground.rs:101 update_ground` | **P (high)** | move target = **nearest enemy building of any kind** (`nearest_enemy_building`, RT 2404), not the enemy-core field | T6a |
| `FlyingAI` (`findMainTarget` with `targetFlags`, circle/moveTo 0.8 range, idle player flyers go to spawner) | RT generic branch, `ai/types/flying.rs` | P | `targetFlags` ignored (flare/horizon/zenith/... target priorities), no main-target over range | T6b |
| `CommandAI` command controllers (`UnitCommand.controller`: repair->RepairAI, rebuild->BuilderAI, assist->BuilderAI(onlyAssist), mine->MinerAI) | RT Command branch 1153-1195 | **M** | Command branch only moves/targets; `UnitCommandComp` stored but ignored; mono/poly/mega from factories idle | T10 |
| `MinerComp.update` for non-player units | only player (`ai/types/player.rs:184 update_mining`) | **M** | AI units never mine | T10a |
| `MinerAI` | `ai/types/miner.rs` (never called live; no delivery) | NW/P | | T10a |
| `RepairAI` | `ai/types/repair.rs` (never called live; heals +1/tick directly, Java shoots heal bullets) | NW/P | | T10b |
| `BuilderAI` (rebuild from `team.data().plans`, assist, follow, flee) + `BuilderComp` for non-player units | `ai/types/builder.rs` stub; `Sim::update_builders` (`core/sim/builder.rs:942`) **player unit only** | P | | T10c |
| `UnitStance` (holdFire, pursueTarget, patrol, ram, boost, holdPosition, mineAuto, item stances) | `SimCommand::UnitStance` -> relay runtime only (`core/sim/mod.rs:820`) | **M (live)** | live units never get stances | T11 |
| `DefenderAI` (oct: follow healthiest friendly) | `ai/types/defender.rs` (targets hostiles — wrong) | NW/wrong | | T18a |
| `FlyingFollowAI` (quell/disrupt wave support) | `ai/types/flying_follow.rs` | NW | not dispatched | T18a |
| `HugAI` (renale/latum) | `ai/types/hug.rs` | NW | not dispatched | T18a |
| `LogicAI` (ucontrol) | `ai/types/logic.rs`, slot set by `core/logic/executor/unit_control.rs` | NW | `update_logic` never called => `ucontrol move/approach/...` no-op live | G06 T9 |
| `CargoAI` (manifold), `AssemblerAI` (assembly-drone) | `ai/types/{cargo,assembler}.rs` | NW | not dispatched | T18c |
| `SuicideAI`, `MissileAI`, `BoostAI`, `Player` | RT 1149-1355 | OK | prior plan T14a/T14c, PAR-0088 | — |
| `PrebuildAI` (experimental, `rules.prebuildAi` teams) | `ai/types/prebuild.rs` stub | P | behaviour deferred (§2 "Deferred") | T17 (spawn only) |
| `BaseBuilderAI` | `core/ai/base_builder_ai.rs` + `GRT::run_team_base_builder` | **P (regression)** | scatter in world px instead of tiles (Java bug-for-bug: tiles) -> `units_base_build` golden fails; RNG call order/kinds differ; fill-cores/core-unit spawn ignored | T1, T17 |
| `RtsAI` | `core/ai/rts_ai.rs`, `GRT::team_ai_system` | P | `estimateStats` without turrets; unit rows wrongly filtered by targetAir/Ground | T17 |
| `WaveSpawner` + `SpawnGroup.createUnit` | `core/ai/wave_spawner.rs`, RT `run_wave_system` 1678, `core/game/spawn_group.rs:136` | **P (high)** | count split round-robin instead of N **per spawn**; no spread; flyers spawn on ground spawns (Java: off-map edge); no shockwave (`dropZoneRadius` kill); no `spawnEffect` (unmoving 30 / invincible 60); difficulty `enemySpawnMultiplier` ignored; rotation faces spawn centroid not map centre | T7 |
| `waves.rs` default/generated wave tables, boss/shield/effect per wave | `core/game/waves.rs` | OK | shields/effect applied in `create_unit` | — |

### 1.4 Unit lifecycle and player

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `PlayerComp.update` death -> `CoreBuild.requestSpawn` -> `CoreBlock.playerSpawn` (core `unitType`) | `gdext/sim_host.rs:1913 spawn_player_unit` (hard-coded `"alpha"`, called once at sector start) | **M** | after the player unit dies it never respawns; foundation/nucleus/bastion cores give alpha | T8 |
| `CoreBlock.unitType` (shard alpha, foundation beta, nucleus gamma, bastion evoke, citadel incite, acropolis emanate) | not on `BlockDef` | M | `Blocks.java:3150-3228` | T8 |
| `InputHandler.unitControl/unitClear` sim side, `UnitComp.update` spawnedByCore despawn | `SimCommand::UnitControl/UnitClear` only store an id (`core/sim/mod.rs:786-793`) | **M** | possession never happens | T9 |
| `Units.canCreate/getCap`, `TeamData.unitCap` (`BlockIndexer` +=`unitCapModifier`), `UnitComp.add` cap death | `queries.rs:185 can_create` unused; `TeamData.unit_cap` never set | **M** | factories produce without cap | T12 |
| `UnitComp.update` environment: bounds repel/clamp/kill, NaN kill, drowning, floor status, `floor.damageTaken`, kill-on-solid / boost, spawn-zone knockback, AI death on captured sector, env death, dead flyer fall | `entities/comp/unit/movement.rs:123 update_kinematics` (statuses, abilities, kinds, boosting only) | **M** | none of the listed branches exist | T13 |
| Crash damage (`UnitComp.java:924-930`) | RT ~400-430 | OK (no ExplosionShield check) | | T14 |
| Payload pickup/drop (PAR-0138), floor speed (PAR-0137), physics push, aim lead, lifesteal, rebuild ghosts | various | OK | prior plan T4,T6,T8,T12,T15,T16 | — |
| `CommandAI.attackTarget` tracking (unit/building target follow) | `apply_live_unit_command` resolves the target to a tile once (`core/sim/mod.rs:389-497`) | P | moving targets are not followed; no position seam for the reticle | T16 |

### 1.5 Already done / verified (do not redo)

* Prior plan `audit-combat-units.md` tasks T1-T12, T14a-c, T15-T17 (commits listed in its
  summary): scathe missiles, under-bullets, weather cooldown, boosting mechs, derelict
  targeting, flying circle, unit physics, payload key path + unit save rev 2, dynamicExplosion
  for buildings, live command id storage, aim lead, Suicide/Builder(commanded)/Boost dispatch,
  bullet owner + lifesteal, rebuild ghosts, flyer crash damage.
* Status effects application/transitions, all used abilities, shoot-on-death, TimedKill,
  MissileAI, spawnUnit bullets.
* The prior plan's T13 (save entity count) belongs to G04 (§6).

---

## §2 Tasks

(Tasks follow; numbering is by topic, execution order is §3.)

### T1 — `units_base_build` regression: BaseBuilderAI scatter is in tiles, not pixels (coordinator addendum)

**Symptom / acceptance.** `cargo test -p mind-headless --lib units_scenarios::tests::committed_goldens_match`
fails for `units_base_build` (`queued_plans: 0, pass: false`; checksum `1aedac089f2e9e05`,
`core_units_spawned 2`, `registered_parts 1` unchanged). In a live attack sector the enemy base
builder only ever tries tiles within ~19 tiles of its core (and in the scenario every attempt is
rejected). After: the scenario passes (`pass: true`, `queued_plans > 0`, `invalid_plans 0`) and
the live builder scatters up to 150 **tiles** from its core like Java.

**Root cause (verified by reading).** Commit `7b785a3` ("Fixes PAR-0139 (round 2)") changed
`core/ai/base_builder_ai.rs` `BaseBuilderAi::update` (~253-262) from tile units to world pixels,
citing `world.tile(pos.x + trnsx(angle, len))`. Java does **not** do that:
`J/ai/BaseBuilderAI.java:152-160`:
```java
int range = 150;
Position pos = randomPosition();                 // data.cores.random() (a Building) or a spawn Tile
if(pos == null) return;
Tmp.v1.rnd(Mathf.random(range));                 // Mathf.random(int) -> rand.nextInt(151): INTEGER 0..150
int wx = (int)(World.toTile(pos.getX()) + Tmp.v1.x), wy = (int)(World.toTile(pos.getY()) + Tmp.v1.y);
Tile tile = world.tiles.getc(wx, wy);            // clamped
if(spawner.getSpawns().contains(t -> t.within(tile, tilesize * 40f))) continue;   // 320 px
```
`Vec2.rnd(len)` (`ARC/arc-core/src/arc/math/geom/Vec2.java:408,591`) = `theta = Mathf.random(0f, PI2)`
(**after** the length draw) then `(cos θ, sin θ) * len`. So the offset is up to 150 *tiles*
(Java bug-for-bug; the map clamp makes edge tiles common). In the scenario the spawn equals the
core tile (10,10), so with the pixel scatter (≤150 px ≈ 18.75 tiles) every candidate is within
40 tiles of the spawn and is skipped => 0 plans. The golden (28 plans) was produced by the
tile-unit code.
Further Java details the port gets wrong (fix in the same task):
* `Mathf.chance(emptyChance)` = `rand.nextFloat() < 0.01` (`ARC/.../Mathf.java:240`); port uses
  `ArcRand::chance` (`next_double`).
* `tryPlace`: `int rotation = Mathf.range(2)` = `random(-2, 2)` = `-2 + nextInt(5)`
  (`BaseBuilderAI.java:198`, `Mathf.java:224,250`); port draws `next_int_bound(2)` (0..1).
  `Schematics::rotate` already handles negative counts (`core/game/schematics.rs:832`).
* Part selection: `if(tile.drop() != null && bases.forResource(tile.drop()).any()) parts = forResource(drop); else if(Mathf.chance(emptyChance)) parts = bases.parts;`
  — port `continue`s when the drop exists but has no parts (never tries the empty chance).
* `parts.random()` = `items[rand.random(0, size-1)]` = `nextInt(size)`: port matches.

**Port today.** `core/ai/base_builder_ai.rs`: `update` ~253-262 (angle draw before length,
`random_float(PLACE_RANGE)`, world-pixel sum), part selection ~273-284 (`tile_drop` then
`for_resource`, `rng.chance(EMPTY_CHANCE)`), rotation `input.rng.next_int_bound(2)` ~289.
`ArcRand` API (`core/math/rand_arc.rs`): `random_inclusive(max)` = `nextInt(max+1)`,
`random_range_float(min,max)`, `range_int(n)` = `nextInt(2n+1) - n`, `next_float()`.

**Changes.**
1. Slice A (proves the root cause, no golden change): in `update`, replace the scatter block with
   ```rust
   // Java scatters in TILE units (`World.toTile(pos) + Tmp.v1`), BaseBuilderAI.java:159-160.
   let len = input.rng.random_inclusive(150) as f32;            // Mathf.random(int range)
   let theta = input.rng.random_range_float(0.0, std::f32::consts::TAU); // Vec2.rnd -> setToRandomDirection
   let wx = (pos.x() as f32 + theta.cos() * len) as i32;        // (int)(toTile + v.x)
   let wy = (pos.y() as f32 + theta.sin() * len) as i32;
   let tile = input.grid.tiles.getc(wx, wy);
   ```
   Delete the "Upstream scatters in WORLD pixels" comment and `PLACE_RANGE` usage (keep the const
   but change its doc to "tiles"; value 150.0). Note: this changes the RNG draw order vs the
   golden (old code drew angle `next_float()*TAU` first, length `random_float` second), so first
   run the golden test with the **old order but tile units** to confirm the root cause:
   temporarily use `let angle = input.rng.next_float()*TAU; let len = input.rng.random_float(PLACE_RANGE); let wx = pos.x() as i32 + (angle.cos()*len) as i32;`
   `UNVERIFIED: executor confirms with $L cargo test -p mind-headless --lib committed_goldens_match;`
   expected: the `units_base_build` mismatch disappears (`queued_plans 28`). Record the result in
   the commit body, then apply the Java order above (this changes the golden — step 4).
2. `chance`: replace `input.rng.chance(EMPTY_CHANCE)` with `input.rng.next_float() < EMPTY_CHANCE as f32`.
3. Part selection:
   ```rust
   let drop = tile_drop(tile, input.content);
   let parts = match drop.map(|item| input.bases.for_resource(BaseResource::Item(item))) {
       Some(parts) if !parts.is_empty() => parts,
       _ if input.rng.next_float() < EMPTY_CHANCE as f32 => &input.bases.parts,
       _ => continue,
   };
   ```
   and rotation `let rotation = input.rng.range_int(2);` (drawn after `index`, as now).
4. Regenerate `client/rust/mind-headless/tests/golden/units/units_base_build.json`: there is no
   dump CLI; run the golden test, copy the new `queued_plans`/`invalid_plans` values from the
   `assert_eq!` diff into the file (keep key order and formatting byte-identical otherwise), re-run
   until green, then `sha256sum client/rust/mind-headless/tests/golden/units/units_base_build.json`
   and put it into `parity/golden_manifest.json` entry `"id": "harness_units_base_build"`.
   Expected after: `pass: true`, `invalid_plans: 0`, `registered_parts 1`, `core_units_spawned 2`,
   `path_found_at 2`, `total_calcs 1`, `checksum 1aedac089f2e9e05` (all unchanged), `queued_plans`
   = new value > 0 (UNVERIFIED number; must be a multiple of 2 because the part has 2 tiles).

**Tests.** Add `base_builder_scatter_reaches_far_tiles` at the end of `core/ai/base_builder_ai.rs`
tests: 200x200 grid, one core at (100,100), copper ore floor everywhere, a 1-tile copper-wall part
registered with `BaseResource::Item(copper)`, spawns empty, `team_rule.build_ai_tier = 1.0`
(interval 120 ticks); run 120*60 updates and collect `plan.x`; assert some plan has
`|x-100| > 40` (impossible with pixel scatter: max offset 150/8 = 18.75 tiles).

**Verify in-engine.** Headless only for the golden. Optional live check (attack sector with
`buildAi`): `$G --eval "$H.load_sector(\"serpulo\", 225); return true" --eval "return $MC.start_sector(\"serpulo\", 225)"`,
step 3600 ticks, and `grep "base builder queued" $(ls -t $U/logs/* | head -1)` shows placements.

**Pitfalls.** Do not touch `try_place`'s axis/rotator math. The live caller
(`GRT::run_team_base_builder`) shares the code; its RNG is `CampaignRuntime.ai_rng`, so live
replays change too (expected). PAR-0139 is verified; mention in the commit that its round-2
"pixel scatter" note was wrong vs `BaseBuilderAI.java:159-160`.

**Commit.** `Gameplay G01 T1: BaseBuilderAI scatters in tiles like Java (units_base_build golden)`

---

### T2 — Unit range / canAttack: bullet `calculateRange` kind overrides and special-weapon defaults

**Symptom / acceptance.** AI pulsar/quasar/corvus/spiroct/arkyid/toxopid/eclipse/omura/locus have
`range = -4` (targeting falls back to 8 px in `acquire_target`), so as wave or RTS units they
barely shoot; vela/oxynoe/cyerce/navanax/cleroi engage at wrong distances; evoke/incite/emanate
claim `can_attack`. After: the compare script (top of this file) prints only `MISSING turret-unit-build-tower`.

**Java.**
* `BulletType.calculateRange` (`J/entities/bullet/BulletType.java:443-448`):
  `rangeOverride>0 ? rangeOverride : spawnUnit ? spawnUnit.lifetime*speed : despawnUnit ? … : max(zero(drag) ? speed*lifetime : speed*(1-(1-drag)^lifetime)/drag, maxRange)` (port has this).
* Overrides: `LaserBulletType:62`, `ContinuousBulletType:47` (also ContinuousLaser/ContinuousFlame
  subclasses), `ShrapnelBulletType:50`, `SapBulletType:68` => `max(length, maxRange)`;
  `RailBulletType:31` => `length`; `LightningBulletType:27` => `(lightningLength + lightningLengthRand/2f) * 6f`
  (defaults set by its ctor: `lightningLength = 25, lightningLengthRand = 0`);
  `MultiBulletType:36` => `max over bullets[] of child.calculateRange()`. `rangeOverride` still
  wins? No: the overrides replace the whole method (no rangeOverride check) — transcribe exactly.
  `PointLaserBulletType` has no override (base formula).
* Special weapons' instance initializers: `PointDefenseWeapon.java:33-40`
  (`predictTarget=false; autoTarget=true; controllable=false; rotate=true; useAttackRange=false; targetInterval=10f`),
  `RepairBeamWeapon.java:52-64` (`reload=1; predictTarget=false; autoTarget=true; controllable=false; rotate=true; recoil=0; noAttack=true; useAttackRange=false`).
  Spec values given in `UnitTypes.java` override these (port: `apply!` macro after the match).

**Port today.** `BDEF::compute_range` (`core/content/registries/bullets.rs:1157`) ignores `kind`;
called from `init_self` ~1506 (`self.range = self.compute_range()`). Multi children are in
`def.spawn_bullets` (see `kinds/multi.rs:27`). `WDEF` ~745-772: `WeaponKind::PointDefenseWeapon => {}`
(empty) and `RepairBeamWeapon` lacks `no_attack`/`use_attack_range`.
Unit range derivation `UDEF` ~2033-2063 is correct once inputs are right.

**Changes.**
1. `compute_range`: before the base formula, `match self.kind { BulletKind::Laser | BulletKind::Continuous | BulletKind::ContinuousLaser | BulletKind::ContinuousFlame | BulletKind::Shrapnel | BulletKind::Sap => return self.length.max(self.max_range), BulletKind::Rail => return self.length, BulletKind::Lightning => return (self.lightning_length as f32 + self.lightning_length_rand as f32 / 2.0) * 6.0, _ => {} }`
   (put it first, i.e. before `range_override`, matching the Java overrides).
2. Multi: in the bullet link pass (`grep -n "Post-\`init()\` bullet link pass" BDEF`, ~1512) after
   all bullets are registered, for every `kind == Multi` set `range = max(child.range)` over
   `spawn_bullets` (children precede parents in id order). Then make sure unit weapon
   `ResolvedBullet.range` is resolved **after** this pass: `grep -n "range:" UDEF` near
   `ResolvedBullet {` construction; if it is filled before the bullet link pass, move the fill or
   re-read `content.bullet(id).range` in the unit `init`. UNVERIFIED: confirm order by reading
   `create_base_content` call sequence (`grep -n "link(" core/content/*.rs`).
3. `WDEF` match: `WeaponKind::PointDefenseWeapon => { def.predict_target = false; def.auto_target = true; def.controllable = false; def.rotate = true; def.use_attack_range = false; def.target_interval = 10.0; }`
   and add `def.no_attack = true; def.use_attack_range = false;` to `RepairBeamWeapon`.
4. Regenerate `parity/golden_content.json` (env §2) and its manifest sha; commit body lists the
   changed unit `range`/`max_range`/`can_attack` values.
5. Re-run the compare script; any remaining mismatch: read that unit's weapons in
   `UnitTypes.java` (grep `<name> = new`) and fix the specific field; if it cannot be explained,
   list it in the summary (do not guess).

**Tests.** In `UDEF` tests (end of file) add `unit_ranges_match_jvm`: for
`[("pulsar",65.0),("quasar",146.0),("vela",176.0),("corvus",456.0),("spiroct",36.0),("arkyid",51.0),("toxopid",86.0),("eclipse",184.0),("omura",496.0),("locus",156.0),("cleroi",136.0),("navanax",91.0)]`
assert `(def.range - v).abs() < 0.01`; `spiroct.max_range == 71`, `eclipse.max_range == 226`,
`oxynoe.max_range ≈ 57.2`, `cyerce.range ≈ 221.66`; `!evoke.can_attack && !incite.can_attack && !emanate.can_attack`.
Derivation example: quasar laser `length 150` -> bullet range 150 -> unit range `150 - 4 = 146`;
locus rail `length 160` -> 156; corvus laser `length 460` -> 456.

**Verify in-engine.** Headless only (content values); T3's in-engine check exercises it.

**Pitfalls.** Turret ranges are separate (`TurretConfig.range`), do not touch. The golden
regeneration also changes bullet `range` fields — expected. `combat_golden`
(`client/rust/mind-core/tests/golden/combat.json`) may change if it records ranges: regenerate
only if the diff is limited to range values, and name it in the commit body.

**Commit.** `Gameplay G01 T2: bullet calculateRange overrides and special-weapon defaults (unit ranges match JVM)`

---

### T3 — Line weapons hit units: `Damage.collideLine` unit half, `RailBulletType`, `SapBulletType`

**Symptom / acceptance.** Lancer, meltdown, fuse, quasar, vela, corvus and every other
`LaserBulletType`/`Continuous*`/`Shrapnel` bullet only damages buildings; enemy units walk through a lancer beam unharmed. Locus/omura/foreshadow rails do
nothing; spiroct/arkyid saps never hurt units nor heal themselves. After: a team-1 `quasar` kills a
team-2 `dagger` 100 px away; a `locus` kills a dagger 120 px away; a damaged `spiroct` heals.

**Java.** `J/entities/Damage.java:256-337` `collideLine(hitter, team, x, y, angle, length, large, laser, pierceCap)`:
```java
length = findLength(hitter, length, laser, pierceCap);   // :151-160: pierceCap>0 -> findPierceLength; else if laser -> findLaserLength (absorbLasers blocks)
hitter.fdata = length;
collidedBlocks.clear(); vec.trnsExact(angle, length);
if(hitter.type.collidesGround && hitter.type.collidesTiles){
  raycastEachNoDiagonalWorld(x, y, x+vec.x, y+vec.y, (cx, cy) -> {
    Building tile = world.build(cx, cy);
    boolean collide = tile != null && tile.collide(hitter) && hitter.checkUnderBuild(tile, cx*8, cy*8)
      && ((tile.team != team && tile.collide(hitter)) || hitter.type.testCollision(hitter, tile)) && collidedBlocks.add(tile.pos());
    if(collide){ collided.add((cx*8, cy*8, tile));  /* + d4 neighbours if large or the segment crosses their bounds */ }
    return false; });
}
rect = segment bounds grown by 6 (expand 3 * 2);
Units.nearbyEnemies(team, rect, u -> { if(u.checkTarget(collidesAir, collidesGround) && u.hittable()){
   hitrect = u.hitbox grown by 6; Vec2 v = Geometry.raycastRect(x, y, x2, y2, hitrect); if(v != null) collided.add((v.x, v.y, u)); } });
collided.sort(by hitter.dst2(c));
for c in collided: if(hitter.damage > 0 && (pierceCap <= 0 || count < pierceCap)){
   if(c is Unit u){ u.collision(hitter, c.x, c.y); hitter.collision(u, c.x, c.y); count++; }
   else if(c is Building tile){ float health = tile.health;
      if(tile.team != team && tile.collide(hitter)){ tile.collision(hitter); hitter.type.hit(hitter, c.x, c.y); count++; }
      if(hitter.type.testCollision(hitter, tile)) hitter.type.hitTile(hitter, tile, c.x, c.y, health, false); } }
```
`Geometry.raycastRect` (`ARC/arc-core/src/arc/math/geom/Geometry.java:310-339`): slab test; returns
the entry point or null:
```java
dx=endx-startx; dy=endy-starty; sx=1/dx; sy=1/dy; signX=sign(sx); signY=sign(sy);
nearX=(cx - signX*halfx - startx)*sx; nearY=(cy - signY*halfy - starty)*sy;
farX =(cx + signX*halfx - startx)*sx; farY =(cy + signY*halfy - starty)*sy;
if(nearX > farY || nearY > farX) return null;
near=max(nearX,nearY); far=min(farX,farY); if(near >= 1 || far <= 0) return null;
t=clamp(near,0,1); return (startx + t*dx, starty + t*dy);
```
`BulletComp.collision(other, x, y)` for a unit: `type.hit(b, x, y)` (frags/splash), then
`if(!type.pierce){ hit=true; remove(); } else collided.add(other.id)`, then
`type.hitEntity(b, other, health)`.
`RailBulletType` (`J/entities/bullet/RailBulletType.java`): ctor `speed 0, pierceBuilding, pierce,
reflectable false, collides false, keepVelocity false, lifetime 1, delayFrags`; `init`: `b.fdata = length;
Damage.collideLine(b, b.team, b.x, b.y, b.rotation(), length, false, false, pierceCap)`;
`handlePierce(b, initialHealth, x, y)`: `sub = max(initialHealth * pierceDamageFactor, 0); if(b.damage <= 0){ fdata = min(fdata, dst); return; } b.damage -= min(b.damage, sub); if(b.damage <= 0) fdata = min(fdata, dst)`;
`hitEntity` -> super + `handlePierce(b, health, entity.x, entity.y)`; `hitTile` -> `handlePierce` only
(no `hit`); `testCollision(b, tile) = b.team != tile.team`.
`SapBulletType.init` (`SapBulletType.java:72-98`): `len = Mathf.random(length, length+lengthRand)`;
`target = Damage.linecast(b, x, y, rot, len)`; if target: `owner.heal(max(min(target.health, damage), 0) * sapStrength)`;
unit target -> `hit.collision(b,…); b.collision(hit,…)`; building -> `if(tile.collide(b)){ tile.collision(b); hit(b, tile.x, tile.y); }`.
`Damage.linecast` (`Damage.java:369-433`): first **enemy** building along `raycastEachWorld` (only if
`collidesGround`), plus the closest enemy unit whose hitbox grown by 3 (each side) intersects the
segment (`checkTarget(air, ground) && targetable(team)`); returns the building if it is not
farther than the unit, else the unit.

**Port today.** `LINE::collide_line` (60-112) buildings only, returns `(first, total)`, used by
`kinds/laser.rs:43`, `kinds/continuous.rs:49`, `kinds/shrapnel.rs:38`. `LINE::linecast` (27-54)
stops at the first building of **any** team. `kinds/sap.rs` hits buildings only, no heal.
`BulletKind::Rail` -> `BASE` (`behavior.rs:125`). Unit hit pipeline to reuse:
`BUL::behavior_hit` (private, 589), `BUL::hit_entity` (pub, 1286), unit gates as in
`BUL::collide_units` (1161-1283: dead check, team, `unit_is_flying`, `def.hittable`,
`HitboxComp.hit_size`). Lifesteal/heal helper `AREA::heal_health`.

**Changes.**
1. `LINE`: add `pub fn raycast_rect(sx, sy, ex, ey, cx, cy, halfx, halfy) -> Option<(f32, f32)>` (transcription above).
2. `BUL`: make `behavior_hit` `pub(crate)` and add
   `pub(crate) fn line_units(ctx: &CombatCtx, bullet: Entity, x: f32, y: f32, x2: f32, y2: f32, expand: f32) -> Vec<(f32, f32, Entity)>`
   iterating `Unit`-marker entities sorted by `(EntitySeq, index)` (query
   `(Entity, &Unit, &Pos, &TeamComp)`), applying the `collide_units` gates (enemy team, air/ground
   vs `def.collides_air/ground`, hittable, alive) and `raycast_rect` with half size
   `hit_size/2 + expand`. Add `pub(crate) fn line_hit_unit(ctx, bullet, unit, cx, cy)`:
   `behavior_hit(ctx, bullet, cx, cy, true); let health = …; if !def.pierce { set(HIT) } else { push collided }; hit_entity(ctx, bullet, unit, health);`
3. New `pub fn collide_line_bullet(ctx: &mut CombatCtx, bullet: Entity, x, y, angle, length, large: bool, pierce_cap: i32) -> f32`
   in `BUL` (or a new `core/combat/bullet/line_hit.rs` with the GPL header) implementing the Java
   order: building candidates from the existing raycast (skip same team unless
   `def.heals() && def.collides_team` i.e. `testCollision`), unit candidates from `line_units`
   (expand 3.0 per Java `hitrect.grow(expand*2)`), sort by `dst2` from `(x, y)`, then hit in order
   while `bullet.damage > 0 && (pierce_cap <= 0 || count < pierce_cap)`: building -> existing
   `hit_building` + `behavior_hit` (enemy) / heal via T4's `hit_tile` (friendly, after T4);
   unit -> `line_hit_unit`. Returns the final length (store into `fdata`). Keep `LINE::collide_line`
   for its existing building-only tests but switch the three behaviours to the new function.
   `large` = `def.large_hit` for continuous (`ContinuousBulletType.applyDamage` passes `largeHit`),
   `false` for laser/shrapnel/rail; `laser` absorb (`findLaserLength`) — keep the existing
   `find_length` behaviour (absorbLasers is plastanium walls; if `BlockDef` has `absorb_lasers`,
   stop the length there; otherwise note as deferred).
4. Rail: new `core/combat/bullet/kinds/rail.rs` (`RailBehavior`): `init` calls
   `collide_line_bullet(ctx, b, x, y, rot, def.length, false, def.pierce_cap)`, sets `fdata`;
   add `handle_pierce` per Java (apply after each unit/building hit: reduce `bullet.damage` by
   `initial_health * pierce_damage_factor`), and `hit_tile` override that only does
   `handle_pierce` (no splash). Register `K::Rail => &super::kinds::rail::RAIL` in `behavior_for`.
   Check `BDEF` `BulletKind::Rail` defaults (~973) match the Java ctor list above.
5. Sap: rewrite `kinds/sap.rs::init` per Java: `len = rng.range(Sim, length, length + length_rand)`;
   new `LINE`/`BUL` helper `linecast_enemy(ctx, bullet, x, y, angle, len) -> Option<Entity>`
   (enemy building first along `raycast_each`, skipping same team; closest unit via
   `line_units(expand 3)`; pick building if `dst2(building) <= dst2(unit)`); heal owner
   (`state.owner`, `AREA::heal_health`) by `max(min(target_health, damage), 0) * def.sap_strength`
   (`grep -n sap_strength BDEF`; add `pub sap_strength: f32` default 0.5 if missing — Java
   `SapBulletType.sapStrength = 0.5f`); then unit -> `line_hit_unit`, building -> `hit_building` + `behavior_hit`.

**Tests** (`core/sim/combat_tests.rs`):
* `quasar_laser_kills_dagger`: `live_sim(48)`; `spawn_live_unit("quasar", 1, 100.0, 100.0, 0.0)`,
  `spawn_live_unit("dagger", 2, 200.0, 100.0, 180.0)`; tick ≤ 600; assert the dagger entity is
  gone. Derivation: quasar laser damage 45 (`UnitTypes.java` quasar weapon), dagger health 130?
  — read it from content: assert `ceil(dagger.health / 45)` shots at reload 55 fit in 600 ticks.
  Needs T2 (range 146 ≥ 100).
* `locus_rail_kills_dagger`: locus team 1 at (100,100), dagger team 2 at (220,100); ≤ 600 ticks; rail damage 48, reload 18.
* `rail_pierces_units_with_damage_falloff`: three daggers team 2 at x = 140,160,180 (y 100), locus
  at (100,100) with weapons forced (set mount aim to (300,100) and `shoot = true` via
  `WeaponsComp`), after one shot: first dagger lost 48, second lost `48 - min(48, 150*0.8)` = 0? —
  compute from content `pierce_damage_factor 0.8` and dagger health H: second loses
  `max(48 - 0.8*H, 0)`. Assert exactly these values (read H from content).
* `spiroct_sap_heals`: spiroct team 1 at (100,100) with health set to `max - 100`, dagger team 2 at
  (130,100); after 300 ticks spiroct health > `max - 100` and dagger damaged.
* Existing `bullet/mod.rs` tests `pierce_damages_each_wall_in_order` / `remove_after_pierce_cap_order`
  use the harness "rail" def (`core/combat/harness.rs:907`, speed 12, collides true): if they fail
  after the Rail behaviour lands, change the harness def to the Java rail shape (`speed 0,
  lifetime 1, collides false, length 100, pierce_cap 3`) and keep their assertions.

**Verify in-engine.** Ground Zero (env §4.1). `content_list("unit").find("dagger")` -> D,
`.find("quasar")` -> Q. Spawn `{"op":"spawn_unit","unit":Q,"team":1,"px":1032,"py":520}` and
`{"op":"spawn_unit","unit":D,"team":2,"px":1100,"py":520}` with `apply_sim_command_json`; step
600; `unit_debug_positions()` no longer lists the team-2 dagger. Screenshot
`runs/<date>-s<N>-G01-T3/quasar.png`.

**Pitfalls.** Iterate units in `EntitySeq` order (determinism). `CombatCtx` has no unit list —
query the world. Do not damage a unit twice per line (dedupe by entity). Continuous bullets call
this every `damage_interval` tick: keep it cheap (pre-filter units by the grown segment AABB).

**Commit.** `Gameplay G01 T3: line weapons (laser/continuous/shrapnel/rail/sap) hit units`

---

### T4 — Heal bullets heal friendly buildings (`BulletType.hitTile`) instead of damaging them

**Symptom / acceptance.** Nova/pulsar/quasar laser bolts, poly missiles and mega heal shots
(`healPercent > 0`, `collidesTeam = true`) **damage** the player's own buildings they pass through.
After: a friendly copper wall at 100/320 hit by a nova bolt goes to `100 + 0.05*320 = 116`; an
enemy wall loses the bolt damage.

**Java.** `BulletComp.tileRaycast` collision (`J/entities/comp/BulletComp.java`, `checkUnderBuild`
block): `if(build.team != team) remove = build.collision(self());` (damage only enemies);
`if(remove || type.collidesTeam){ if(!type.pierceBuilding){ hit = true; remove(); } else collided.add(build.id); }`;
`type.hitTile(self(), build, x*8, y*8, health, true)`.
`BulletType.hitTile` (`J/entities/bullet/BulletType.java`, `public void hitTile(`):
```java
if(makeFire && build.team != b.team) Fires.create(build.tile);
if(heals() && build.team == b.team && !(build.block instanceof ConstructBlock)){
    healEffect.at(...); build.heal(healPercent / 100f * build.maxHealth + healAmount);
}else if(build.team != b.team && direct){ hit(b, x, y); }
handlePierce(b, initialHealth, x, y);
```
`heals() = healPercent > 0 || healAmount > 0`. Nova bolt: `LaserBoltBulletType(5.2f, 13){ healPercent = 5f; collidesTeam = true; }`.

**Port today.** `BUL::collide_bullet` ~940-1000: `same_team && !collide_team` skips; otherwise
`hit_building(ctx, bullet, target)` (always damages) then `behavior_hit` for enemies; the
"direct overlap fallback" ~1010-1035 does the same. `BulletDef::heals()` exists (`BDEF:1177`),
`make_fire` field: `grep -n "make_fire" BDEF`. ConstructBlock check: building kind
`BlockKind::ConstructBlock` (`grep -n "ConstructBlock" core/content/registries/blocks/mod.rs`).

**Changes.**
1. In both building-hit sites of `collide_bullet`: `if !same_team { initial = health; hit_building(...); }`
   then call a new `BUL::hit_tile(ctx, bullet, target, x, y, initial_health, direct=true)`:
   `make_fire && !same_team` -> `crate::combat::fires::create(world, tx, ty, rng)`;
   `def.heals() && same_team && kind != ConstructBlock` -> `AREA::heal_health(world, target, def.heal_percent/100*max_health + def.heal_amount)`;
   `else if !same_team && direct` -> `behavior_hit(...)` (replace the existing unconditional
   `behavior_hit` call for enemies so it is not run twice). Keep the existing pierce bookkeeping.
2. Route `BulletBehavior::hit_tile` (`behavior.rs`) default to this new function so kinds can
   override (Rail overrides it in T3).

**Tests** (`core/combat/bullet/mod.rs` tests): add harness def `heal_bolt` in
`combat/harness.rs` defs (`BulletKind::LaserBolt`, speed 5.2, damage 13, lifetime 60,
heal_percent 5, collides_team true, hit_size 4). `heal_bolt_heals_friendly_wall`: wall team 1 at
(10,8), set its `Health.health = 100`, spawn `heal_bolt` team 1 from tile (6,8) angle 0, 30 ticks:
health == 116.0 (5% of 320 = 16). `heal_bolt_damages_enemy_wall`: same with wall team 2: health ==
320 - 13 = 307.

**Verify in-engine.** Ground Zero: `debug_add_core_items` not needed. Place `copper-wall` at
(129,62) (`place_block`), possess-free check: spawn a team-1 `nova` at world (1032,470) and team-2
dagger at (1032,560) beyond the wall; after 600 steps `building_debug(129,62)` health is not below
its pre-value. Headless test is the acceptance; in-engine optional.

**Pitfalls.** `hit_building` also handles unit targets in its else branch — keep that path. Don't
heal buildings under construction. Shared helper with T3.

**Commit.** `Gameplay G01 T4: heal bullets heal friendly buildings (BulletType.hitTile)`

---

### T5 — Team rule multipliers in combat (`unitDamage`, `blockDamage`, `unitHealth`, `blockHealth`, `unitMineSpeed`)

**Symptom / acceptance.** Campaign difficulty (`CampaignRules.apply` sets wave-team
`unitHealthMultiplier`/`blockHealthMultiplier`, `core/game/campaign_rules.rs:247-250`), sector
rules and logic `setrule unitDamage` have no effect on combat. After: with wave-team
`unit_health_multiplier = 2`, a 20-damage bullet removes 10 hp from a wave unit; with
`unit_damage_multiplier = 2` for team 1, a team-1 dagger bullet (damage 20) deals 40.

**Java.** `BulletType.damageMultiplier(b)` (`BulletType.java:546-551`):
`owner instanceof Unit u ? u.damageMultiplier() * rules.unitDamage(b.team) : owner instanceof Building ? rules.blockDamage(b.team) : 1`;
bullet damage at create = `damage * damageMultiplier(b)`; splash uses `splashDamage * b.damageMultiplier()` (`:608`).
`ShieldComp.damage` (`ShieldComp.java:30,38,50`): `amount / healthMultiplier / rules.unitHealth(team)`.
`BuildingComp.damage` (`:2064-2086`): `dm = rules.blockHealth(team); damage = zero(dm) ? health+1 : damage/dm`.
`MinerComp` rate `type.mineSpeed * rules.unitMineSpeed(team)` and `canMine()` requires it > 0.
`CrawlComp:82`, `TankComp:92`, `LegsComp:197` crush/splash `* rules.unitDamage(team)`.
`EnergyFieldAbility:147` `damage * rules.unitDamage(team) * unit.damageMultiplier`.
`Rules` accessors exist in the port: `core/game/rules.rs:156-200`.

**Port today.** `combat/bullet/spawn.rs:106-121` multiplies by the owner's status
`damage_multiplier` only. `AREA::apply_health` (~225) divides by status `health_multiplier` only;
used by both units and buildings. No rules lookup anywhere in combat.

**Changes.**
1. Add `pub fn rules_of(world: &World) -> Option<&crate::game::rules::Rules>` in
   `core/game/runtime.rs` (returns `world.get_resource::<CampaignRuntime>().map(|r| &r.session.rules)`);
   `None` => multipliers 1.0 (tests/harness worlds).
2. `spawn.rs`: owner has `UnitTypeComp` -> `* rules.unit_damage(spawn.team)`; owner has
   `Building` -> `* rules.block_damage(spawn.team)`.
3. `AREA::apply_health`: after the status divisor, if the entity has `UnitTypeComp` divide by
   `rules.unit_health(team)`; if it has `Building`: `dm = rules.block_health(team)`;
   `if dm.abs() <= 1e-6 { amount = health + 1 } else { amount /= dm }`. (Team from `TeamComp`.)
4. Player mining (`ai/types/player.rs:239`) and T10a's MinerComp: rate `* rules.unit_mine_speed(team)`.
5. Crush damage sites: `grep -n "crush_damage" core/entities/comp/unit/*.rs` -> multiply by
   `rules.unit_damage(team)`; leg splash (`grep -n "leg_splash_damage"`) likewise; energy field in
   `combat/abilities.rs:532 energy_field_tick`.

**Tests** (`combat_tests.rs`): `wave_unit_health_multiplier_halves_damage` (CampaignRuntime with
`rules.team_rule_mut(2).unit_health_multiplier = 2.0` — find the mutator with
`grep -n "fn team_rule" core/game/rules.rs`; call `AREA::damage_entity(world, content, dagger, 20.0, true, 1.0)`;
health drops by exactly 10). `unit_damage_multiplier_scales_bullets` (team 1 `unit_damage_multiplier = 2`,
spawn a dagger bullet via the dagger weapon — or directly a `BulletSpawn` with `owner = Some(dagger)`;
assert `Bullet.damage == 2 * def.damage`). `block_health_zero_kills` (block_health 0 -> any damage
kills).

**Verify in-engine.** Headless only (rule values are 1.0 on normal difficulty).

**Pitfalls.** `apply_health` is called by statuses (`update_unit_status`) too — Java `damage()`
for statuses goes through `ShieldComp.damage` as well, so the divisor applies there too (correct).
Do not clone `Rules` per hit (borrow).

**Commit.** `Gameplay G01 T5: apply team damage/health/mine-speed rule multipliers in combat`

---
### T6a — GroundAI: walk the enemy-core flowfield (not to the nearest building), Java targeting cadence

**Symptom / acceptance.** Enemy ground waves walk to the **nearest player building of any kind**
(a lone drill or conveyor far from the core attracts the whole wave; arriving there they
re-target the next nearest building). Naval units path with the ground cost. Each unit with a
different target rebuilds the team's flowfield every tick (the cache key is `(team, cost)` only:
`core/ai/pathfinder/mod.rs:103-127`), a CPU sink. After: wave daggers path along the enemy-core
field (`Pathfinder.fieldCore`), shoot whatever enters range on the way, stop at
`range*0.5` from the core, and player-team AI ground units on wave maps stop at the drop zone.

**Java.** `J/ai/types/GroundAI.java:21-85` (`updateMovement`):
```java
core = unit.closestEnemyCore();
if(core != null && unit.within(core, unit.range()/1.3f + core.block.size*tilesize/2f)){
    target = core; for(mount : unit.mounts) if(mount.weapon.controllable && mount.weapon.bullet.collidesGround) mount.target = core; }
if(core == null || !unit.within(core, unit.type.range * 0.5f)){
    boolean move = true;
    if(state.rules.waves && unit.team == state.rules.defaultTeam){
        Tile spawner = getClosestSpawner();
        if(spawner != null && unit.within(spawner, state.rules.dropZoneRadius + 120f)) move = false;
        if(spawner == null && core == null) move = false; }
    if(core == null && (!state.rules.waves || getClosestSpawner() == null)) move = false;
    if(move) pathfind(Pathfinder.fieldCore, true, stuckTime >= stuckThreshold);
}
if(unit.type.canBoost && unit.elevation > 0.001f && !unit.onSolid()) unit.elevation = approachDelta(elevation, 0, type.descentSpeed);
faceTarget();
// stuck bookkeeping: stuckRange = 12px; threshold = max(1, stuckRange*2/type.speed) (avoidance flag only)
```
`EnemyCoreField.getPositions` (`J/ai/Pathfinder.java:555-588`): all enemy cores
(`indexer.getEnemy(team, BlockFlag.core)`), plus every spawn tile when
`rules.waves && team == rules.defaultTeam` (randomWaveAI branch: deferred, default off).
`AIController.pathfind` (`AIController.java:138-160`): cost = `unit.type.flowfieldPathType`;
`targetTile = field.getNextTile(tile)`; `if(tile == targetTile && stop) || !unit.canPass(target) return;`
`movePref(vec.trns(angleTo(targetTile), prefSpeed()))`.
`AIController.updateTargeting/updateWeapons` (`:113-260`): `retarget()` = `timer.get(timerTarget, target == null ? 40 : 90)`;
on retarget `target = findMainTarget(x, y, unit.range(), type.targetAir, type.targetGround)`
(GroundAI: = `target(...)` = `Units.closestTarget(team, x, y, range, u -> u.checkTarget(air, ground), t -> ground && (type.targetUnderBlocks || !t.block.underBullets))`);
invalid targets (dead, out of `range`) are cleared every tick; per mount (non-`singleTarget`):
`if(ret) mount.target = findTarget(mountX, mountY, weapon.range(), bullet.collidesAir, bullet.collidesGround)`.

**Port today.** RT generic branch (~1356-1420): `acquire_target` (RT 2355) returns
`(anchor_tile = nearest_enemy_building, mount_target = closest_target(range) or anchor within range+16)`
every tick; `update_ground(world, grid, content, pathfinder, team, entity, target)`
(`core/ai/types/ground.rs:101`) builds `get_field(Cost::Ground, team, &[target])`.
`TargetQueries::closest_target` (`core/combat/targeting.rs`) implements `Units.closestTarget`.
Spawn tiles: `runtime.wave_spawner.get_spawns()` (set by `run_wave_system`).
Rules: `CampaignRuntime.session.rules` (`waves`, `default_team`, `drop_zone_radius`).

**Changes.**
1. New `UnitRuntime` helper `fn enemy_core_targets(world, team) -> Vec<TilePos>`: tiles
   (`BuildingComp.pos`) of entities with `CoreBuild` whose `TeamComp.team != team` and not
   `is_derelict(world, team)`; sort by `(y, x)`; append the spawn tiles when
   `rules.waves && team == rules.default_team`. Cache per team per tick (HashMap is fine for a
   cache but iterate the sorted Vec).
2. Add `pub struct GroundAiState { stuck_time: f32, stuck_x: f32, stuck_y: f32, retarget: f32, target: Option<Entity> }`
   component (insert lazily) — or reuse `ControllerSlot.timer` for `retarget`. Rewrite the
   ground half of the generic branch:
   * retarget timer: `retarget += 1; if retarget >= (if target.is_none() {40} else {90}) { retarget = 0; target = closest_target(range) }`;
     clear the target when dead or farther than `range` (Java `invalid`).
   * `core = closest enemy core entity`; core override (`range/1.3 + size*4`) sets `target = core`
     and every controllable mount that `collides_ground` targets the core.
   * movement: if `core.is_none() || dist(core) > def.range * 0.5`: apply the `move` rules above
     (drop zone uses `rules.drop_zone_radius + 120`); if move, step toward
     `get_field(cost, team, &enemy_core_targets).get_target_tile(...)` — refactor
     `update_ground` into `pathfind_field(world, grid, content, pathfinder, team, entity, cost, &targets) -> bool`
     using `Cost::from_id(def.flowfield_path_type as u8).unwrap_or(Cost::Ground)`.
   * `set_mount_targets(world, content, entity, target)`; `face_target` as now.
   * Commanded move orders (`slot.target.is_some()`) keep the existing `update_ground(..target)` path.
3. Delete the anchor fallback for ground units (Java has none). `nearest_enemy_building` stays for
   flying until T6b.

**Tests** (`combat_tests.rs`): `ground_wave_paths_to_core_not_nearest_building`: `live_sim(96)`,
CampaignRuntime with `default_team 1, waves true`; `place(&mut sim, 10, 48, "core-shard", 1)`;
`place(&mut sim, 80, 10, "copper-wall", 1)` (decoy far from the core, closer to the dagger);
dagger team 2 at tile (85, 48) world `(85*8, 48*8)`; tick 3000; assert the dagger's x decreased by
> 300 px (it walked toward the core) and the decoy wall has full health.
`field_is_stable_across_units`: two daggers with the same team; after 10 ticks
`runtime.pathfinder.updates` grew by ≤ 2 (one build per cost/team).
Re-run `placed_duo_kills_enemy_dagger`, `crawler_rams_and_explodes`, T7's tests.

**Verify in-engine.** Ground Zero wave probe from the prior plan (`audit-combat-units.md` §6):
`run_wave()`, poll `unit_debug_positions()` every 6 s for 90 s: daggers walk toward core tile
(129,55) and die; `get_hud_state().hasCore == true`. Then place a `copper-wall` at (150,150)
before the wave: the daggers must not detour to it.

**Pitfalls.** `Flowfield` targets must be identical each tick or the cache rebuilds (sort them).
Determinism: core iteration sorted. The relay `unit_command_runtime` is untouched.
`team_ai_system` RTS orders write `slot.target`, so RTS squads keep working.

**Commit.** `Gameplay G01 T6a: GroundAI follows the enemy-core flowfield with Java targeting cadence`

### T6b — FlyingAI: `findMainTarget` with `targetFlags`, idle player flyers

**Symptom / acceptance.** Wave flares/horizons/zeniths go for the nearest building instead of
their target priorities (horizon bombs factories, zenith goes for launch pads/storage/batteries,
flare goes for generators). After: a wave `horizon` with a factory and a core both present
targets the factory.

**Java.** `J/ai/types/FlyingAI.java:16-72`:
```java
updateMovement(): unloadPayloads();
  if(target != null && unit.hasWeapons()){ if(type.circleTarget) circleAttack(type.circleTargetRadius); else { moveTo(target, type.range*0.8f); unit.lookAt(target);} }
  if(target == null && rules.waves && unit.team == rules.defaultTeam) moveTo(getClosestSpawner(), rules.dropZoneRadius + 130f);
findTarget(x,y,range,air,ground){ result = findMainTarget(...); return checkTarget(result, x, y, range) ? target(x, y, range, air, ground) : result; }
findMainTarget(x,y,range,air,ground){
  core = targetFlag(x, y, BlockFlag.core, true);           // closest enemy core, ANY distance
  if(core != null && within(x,y,core,range)) return core;
  for(flag : type.targetFlags){                            // default {null}
     if(flag == null){ r = target(x,y,range,air,ground); if(r != null) return r; }
     else if(ground){ r = targetFlagActive(x,y,flag,true); if(r != null) return r; } }   // closest enemy building with flag, any distance, items.any() || status != noInput, block.targetable
  return core; }
```
`targetFlags`: flare `{generator, null}`, horizon `{factory, core}`... read exact arrays at
`J/content/UnitTypes.java:1055,1105,1152,1199,1275,1493`; port: `UnitTypeDef.target_flags: Vec<Option<BlockFlag>>` (`UDEF:1297`).

**Port today.** RT generic branch flying half: `mount_target` from `acquire_target`, circle or
`move_to(range*0.8)`, else `update_flying` to the nearest building tile.
`BlockIndexer` flags: `core/ai/block_indexer.rs` (`flags` per team/flag, `flag_index`);
check for an accessor `grep -n "pub fn get_enemy\|pub fn flagged" core/ai/block_indexer.rs`
(none found in the audit: add `pub fn enemy_flagged(&self, world, team, flag) -> impl Iterator<Item=Entity>`
over all teams != team, excluding derelict).

**Changes.** New `fn flying_main_target(world, runtime, queries, entity, team, def) -> Option<Entity>`
implementing `findMainTarget` (status `noInput`: use `crate::world::update::status(world, e)` if it
exists — `grep -rn "fn block_status\|BlockStatus::NoInput" core/world | head`; if absent treat
"active" as `items.any()`); keep the 40/90 retarget cadence from T6a; movement per Java above;
`target == None && waves && team == default_team` -> `move_to(closest spawn, drop_zone_radius + 130)`.

**Tests.** `horizon_prefers_factory`: campaign default 1; core-shard team 1 at (10,10),
`graphite-press` team 1 at (40,40) with 1 coal in it (`debug`: `ItemModule::add`), horizon team 2
at (60,60) world; tick 600; horizon distance to the press < distance at start and < 60 px at some
tick. `flare_falls_back_to_core`: no generators -> flare ends within `range` of the core.

**Verify in-engine.** Ground Zero `run_wave()` until a flare wave (or spawn a flare team 2 via
`spawn_unit`): it flies to the core (no generators on GZ).

**Pitfalls.** `targetFlag` ignores range: do not filter by range. `ground` gate applies only to
flag entries.

**Commit.** `Gameplay G01 T6b: FlyingAI main target follows targetFlags`

---

### T7 — Wave spawner fidelity (per-spawn counts, spread, flyer edge spawns, shockwave, spawn statuses, difficulty)

**Symptom / acceptance.** On maps with several spawn points waves are N/spawns times too small;
flyers appear on the ground spawn instead of off the map edge; buildings/units inside the drop
zone survive the wave (Java kills them); spawned units are immediately vulnerable and mobile;
campaign difficulty spawn multiplier is ignored; units spawn stacked. After: with 2 spawns and a
group of 3 daggers, 6 daggers spawn (3 within 16 px of each spawn); a team-1 copper wall within
`dropZoneRadius` (300 px) of a spawn is destroyed when the wave spawns; new wave units carry
`unmoving` (30 ticks) and `invincible` (60 ticks).

**Java.** `J/ai/WaveSpawner.java:62-120, 176-200, 228-235`:
```java
spawnEnemies(): spawning = true;
  eachGroundSpawn(-1, (sx, sy, doShockwave) -> { if(doShockwave) doShockwave(sx, sy); });   // spawn tiles only (core spawns pass false)
  for(group : rules.spawns){ if(group.type == null) continue;
    int spawned = group.getSpawned(state.wave - 1); if(spawned == 0) continue;
    if(state.isCampaign()) spawned = max(1, group.effect == boss ? (int)(spawned*mult) : Mathf.round(spawned*mult));   // mult = planet.campaignRules.difficulty.enemySpawnMultiplier
    if(group.type.flying){ float spread = margin/1.5f;                 // margin = 0f (WaveSpawner.java:23) => spread 0
        eachFlyerSpawn(group.spawn, (x, y) -> { for i<spawned: spawnUnit(group, x + Mathf.range(spread), y + Mathf.range(spread)); }); }
    else { float spread = tilesize*2;
        eachGroundSpawn(group.spawn, (x, y, s) -> { for i<spawned: { Tmp.v1.rnd(spread); spawnUnit(group, x + v1.x, y + v1.y); } }); } }
  Time.run(121f, () -> spawning = false);
spawnUnit(group, x, y): group.createUnit(team, x, y, Angles.angle(x, y, world.width()/2f*8, world.height()/2f*8), state.wave-1, this::spawnEffect);
doShockwave(x, y): Fx.spawnShockwave; Damage.damage(rules.waveTeam, x, y, rules.dropZoneRadius, 99999999f, true);   // complete=true: units + buildings of other teams
eachFlyerSpawn(filter, cons): for tile in spawns: if(!rules.airUseSpawns){
     angle = Angles.angle(w/2, h/2, tile.x, tile.y); trns = max(w, h) * sqrt2 * 8;
     sx = clamp(w*8/2 + trnsx(angle, trns), -margin, w*8 + margin); sy = clamp(h*8/2 + trnsy(angle, trns), -margin, h*8 + margin); cons(sx, sy);
   } else cons(tile.worldx(), tile.worldy());
   + if(wavesSpawnAtCores && attackMode && waveTeam active) for core in waveTeam cores: cons(core.x, core.y);
spawnEffect(unit): unit.apply(unmoving, 30f); unit.apply(invincible, 60f); unit.unloaded(); UnitSpawnEvent; Fx.
```
Filter: `group.spawn == -1 || group.spawn == tile.pos()` (`SpawnGroup.canSpawn` — port has `can_spawn(packed)`).
`margin = 0f, coreMargin = 16f, maxSteps = 30` (`WaveSpawner.java:23`): flyers spawn exactly on the clamped map edge, no spread.
`Damage.damage(team, x, y, radius, damage, complete=true)` = units and buildings not of `team`
(air and ground). Port: `AREA::damage_area(world, content, Some(wave_team), x, y, radius, damage, DamageOptions{..})` —
check `DamageOptions` fields for air/ground/complete (`grep -n "pub struct DamageOptions" -A20 AREA`).

**Port today.** `core/ai/wave_spawner.rs:95-140 spawn_enemies` (round-robin cursor over spawns,
exact tile centre, facing spawn centroid, `count` total); RT `run_wave_system` 1678-1806 builds
spawn tiles (overlay + core spawns) and calls `group.create_unit` (`core/game/spawn_group.rs:136`).
`spawn_effect_statuses()` (`wave_spawner.rs:143`) returns `(30, 60)` but is unused.
Difficulty: `PlaySession.difficulty_wave_time_multiplier` set in `core/game/play.rs:310` from
`planet_record.campaign_rules.difficulty`; no spawn multiplier stored.

**Changes.**
1. `game/play.rs`: add `pub difficulty_enemy_spawn_multiplier: f32` (default 1.0) next to
   `difficulty_wave_time_multiplier`, set at the same place with `.enemy_spawn_multiplier()`.
   (Shared file with G04 — one-line field + one assignment.)
2. Rewrite `WaveSpawner::spawn_enemies` signature to
   `spawn_enemies(&mut self, groups, wave, ctx: &SpawnContext, rng: &mut SimRng, spawn_unit: impl FnMut(&SpawnGroup, f32, f32, f32) -> Option<Entity>) -> u32`
   with `SpawnContext { width_px, height_px, ground: Vec<(f32,f32,i32 /*packed pos*/,bool /*shockwave*/)>, flyer: Vec<(f32,f32,i32)>, campaign_mult: Option<f32> }`
   computed in `run_wave_system` (ground: overlay spawns `(tile*8, tile*8)` — Java `worldx()` is
   `x*8` (tile centre at integer multiples; NOT +4) — plus core spawns with `shockwave=false`;
   flyer: per spawn tile the edge position above unless `rules.air_use_spawns`, plus wave cores).
   Loop per Java: per group, count (with campaign rounding: boss uses truncation), flying vs
   ground, spread draws: ground `rnd(16)` = `len 16`, angle `rng.range(Sim, 0, 360)` ->
   (cos,sin)*16; flyer: Java still draws `Mathf.range(0)` twice per unit (two RNG draws returning 0) — draw `rng.range(Sim, -0.0, 0.0)` twice to keep the draw count.
   Rotation `atan2(h/2*8 - y, w/2*8 - x)` in degrees.
3. Shockwave before groups: for each ground spawn with `shockwave` call `damage_area(world, content,
   Some(wave_team), x, y, rules.drop_zone_radius, 99_999_999.0, complete/air+ground)`; buildings
   that die are removed by `resolve_deaths` the same tick.
4. In the spawn closure after `create_unit`: `apply_status(world, unit, unmoving, 30.0)`,
   `apply_status(world, unit, invincible, 60.0)` (`core/combat/damage/status.rs:190`;
   ids via `content.status_by_name("unmoving"/"invincible")`).
5. Keep `last_spawn_count`/`spawning` window semantics.

**Tests** (`core/ai/wave_spawner.rs` + `combat_tests.rs`):
* `two_spawns_double_the_group`: spawns at tiles (10,10),(50,10), one dagger group amount 3 at
  wave 1 -> 6 units, 3 within 16 px of (80,80) and 3 within 16 px of (400,80).
* `flyers_spawn_at_map_edge`: 64x64 map, spawn tile (32,60); flare group -> angle from map
  centre (32,32) to (32,60) is 90°, `trns = 64*1.41421*8 = 724.1`, so x = `clamp(256 + cos90*724, 0, 512)` ≈ 256.0,
  y = `clamp(256 + 724, 0, 512)` = 512.0 exactly.
* `wave_shockwave_destroys_drop_zone_building`: campaign session (waves true, default 1, wave
  team 2); copper-wall team 1 at 5 tiles from the spawn; run a wave via `runtime.pending_wave = Some(1)`
  (see how `MindCampaign.run_wave` sets it: `grep -n "pending_wave" core/sim/unit_runtime.rs`);
  after one tick + `resolve_deaths` the wall is gone; a wall 50 tiles away survives.
* `spawned_wave_unit_is_invincible`: status list of a new wave unit contains both statuses with
  durations 30/60 (minus ticks elapsed).
* `campaign_multiplier_rounds`: mult 1.5, group amount 3 -> 5 (`Math.round(4.5)` = 5); boss effect
  amount 1 -> `max(1, (int)1.5)` = 1.

**Verify in-engine.** Ground Zero: place a `copper-wall` at (141,231) (≈ 5 tiles from the dagger
spawn near world (1156,1844) = tile (144,230)); `run_wave()`; next poll `tile_debug(141,231)`
shows no building. Screenshot `runs/<date>-s<N>-G01-T7/shockwave.png`.

**Pitfalls.** `Damage.damage` with 99,999,999 also kills the **player unit** standing in the drop
zone (Java does that) — expected. Keep `units_scenarios` wave goldens in mind: run
`$L cargo test -p mind-headless --lib units_scenarios` and regenerate only scenarios whose spawn
count/positions changed because of this task (name them in the commit body with the Java reason).
Rng: use `runtime.rng` (`RngStream::Sim`).

**Commit.** `Gameplay G01 T7: WaveSpawner per-spawn groups, flyer edge spawns, drop-zone shockwave, spawn statuses`

---

### T8 — Player respawn at the core with the core's unit type

**Symptom / acceptance.** When the player's unit dies nothing happens: no new unit ever spawns
(the camera stays, WASD does nothing) for the rest of the session. Foundation/nucleus cores
should give beta/gamma, Erekir bastion/citadel/acropolis evoke/incite/emanate; the port always
spawns `alpha`. After: kill the player unit; 60 ticks later a new core-type unit exists at the
best core, controlled by the player (`has_player_unit() == true`).

**Java.** `PlayerComp.update` (`J/entities/comp/PlayerComp.java:214-243`): `if(unit != null && !unit.isValid()) clearUnit();`
dead -> `core = bestCore()`; `deathTimer += delta; if(deathTimer >= deathDelay /*60*/){ core.requestSpawn(this); deathTimer = 0; }`.
`bestCore()` (`:121-126`): team cores min by (bigger block size first, then distance to the
player position); single core always; prefer cores whose `unitType.supportsEnv(rules.env)`.
`CoreBuild.requestSpawn` (`J/world/blocks/storage/CoreBlock.java:639-644`): return if
`!unitType.supportsEnv(rules.env) || !allowSpawn`; `playerSpawn(tile, player)` (`:95-125`):
`unit = spawnType.create(team); for(mount) mount.reload = mount.weapon.reload; unit.set(core); unit.rotation(90); unit.impulse(0, 3); unit.spawnedByCore(true); unit.controller(player); unit.add();`
Core unit types: `Blocks.java:3150` shard alpha, `3162` foundation beta, `3175` nucleus gamma,
`3190` bastion evoke, `3210` citadel incite, `3228` acropolis emanate.

**Port today.** `gdext/sim_host.rs:1913 spawn_player_unit` (hard-codes `"alpha"`, idempotent,
called once from `gdext/campaign.rs:816` on sector start). Core: `Sim::spawn_player_unit(name, team, x, y, rot)`
(`RT:873`), `Sim.player_unit: Option<Entity>`. `UnitCore.spawned_by_core` exists.
`BlockSpec/BlockDef` (`core/content/registries/blocks/mod.rs` ~1210/2024 near `unit_cap_modifier`) has no core unit type.

**Changes.**
1. `BlockSpec`: `pub core_unit: Option<&'static str>`; `BlockDef`: `pub core_unit: Option<&'static str>`
   (default None, copied in the spec->def conversion next to `unit_cap_modifier`). Set it on the
   six cores in `core/content/registries/blocks/storage.rs` (names above). Not part of the
   golden field subset (no golden change; verify with `cargo test content`).
2. `Sim`: fields `player_death_timer: f32`, `player_team: Option<u8>` (set in
   `spawn_player_unit`). New `pub fn spawn_player_at_best_core(&mut self, team: u8) -> Option<Entity>`:
   best core = `CoreBuild` entities of `team`, sort by `(-size, dist2 to last player position or 0)`;
   unit name = `def.core_unit.unwrap_or("alpha")`; skip if the unit type's env check fails
   (`unit.env_required/env_enabled` vs `rules.env`; Java `supportsEnv(env) = (envEnabled & env) != 0 && (envDisabled & env) == 0 && (envRequired == 0 || (envRequired & env) == envRequired)`);
   spawn at the core centre (`Pos` of the core), rotation 90, then `vel.y += 3 / mass` (mass =
   `hit_size² · π`), `UnitCore.spawned_by_core = true`, every mount `reload = weapon.reload`.
3. `Sim::update_player_respawn(&mut self)` called in `Sim::tick` right after `resolve_deaths`:
   if `player_unit` is Some but the entity no longer exists or is dead -> set `player_unit = None`;
   if None and `player_team` is Some and a team core exists: `player_death_timer += 1`; at
   `>= 60` -> `spawn_player_at_best_core(team)`; timer reset 0.
4. gdext `spawn_player_unit` (sim_host.rs:1913): call `self.sim.spawn_player_at_best_core(team)`
   instead of the hard-coded alpha (no game rule left in gdext).
5. `has_player_unit` keeps working (checks `player_unit`).
6. Campaign unlock (`CoreBlock.playerSpawn`: `if(state.isCampaign() && player == Vars.player) spawnType.unlock();`,
   `CoreBlock.java:121-123`): in `spawn_player_at_best_core`, when a `CampaignRuntime` is present and
   `session.is_campaign()`, unlock the spawned unit type through the same unlock path research uses
   (`grep -rn "fn unlock" core/game/tech_tree.rs core/content/*.rs` — call the existing content
   unlock + persist function; if unlocking needs the gdext unlock store, queue the name on a
   `PendingUnlocks(Vec<String>)` resource and drain it in `gdext/campaign.rs` where other unlocks are
   persisted). Test: `player_spawn_unlocks_type_in_campaign` (beta becomes unlocked).

**Tests** (`core/sim/combat_tests.rs` or a new `core/sim/player_tests.rs` with the header):
`player_respawns_after_death`: live_sim(48) + campaign default 1; place `core-foundation` team 1
at (20,20); `spawn_player_at_best_core(1)` -> type `beta`, `spawned_by_core`; set its health 0
(`Health.dead = true`); tick 59 -> `player_unit()` None; tick 1 more -> Some, type beta, position
= core centre ± 3 px. `core_shard_spawns_alpha`.

**Verify in-engine.** Ground Zero: `$G --eval "return $H.has_player_unit()"` -> true; kill it:
`apply_sim_command_json` has no kill op — use `$H.step(1)` after spawning 6 team-2 daggers next
to the player (`spawn_unit` at world (1032,460)) and wait until `has_player_unit()` turns false,
then `step(70)` -> true again and `unit_debug_json()` lists a team-1 `alpha` with
`spawned_by_core`. Screenshot.

**Pitfalls.** `PlayerInput` resource (`ai/types/player.rs`) must survive the swap. The landing
cutscene (`campaign.rs`) still calls `spawn_player_unit` once; keep it idempotent. Do not respawn
when the team has no core (game over is G04's).

**Commit.** `Gameplay G01 T8: player respawns at the best core with CoreBlock.unitType`

---

### T9 — Unit possession / clear (sim side of `unitControl` / `unitClear`)

**Symptom / acceptance.** Ctrl-click possession of an own unit and "respawn at core" do nothing
in the sim: `SimCommand::UnitControl { unit }` only stores an id (`core/sim/mod.rs:786-793`).
After: `UnitControl { unit: seq }` on an own AI dagger makes it the player unit (controller
`Player`), the previous core-spawned alpha is despawned; `UnitClear` gives the player a fresh core
unit at the best core and returns the possessed dagger to AI.

**Java.** `InputHandler.unitControl` (`J/input/InputHandler.java:775-822`): `unit == null ->
player.clearUnit()`; else if `unit.isAI() && unit.team == player.team() && !unit.dead && unit.playerControllable()`:
`before = player.unit(); player.unit(unit); if(before != null){ if(before.spawnedByCore) unit.dockedType = before.type; …; if(before.spawnedByCore && !before.isPlayer()) Call.unitDespawn(before); }`.
`unitClear` (`:824-875`): if not dead and current unit not `spawnedByCore`: docking branch (when
`dockedType.coreUnitDock` — gamma/evoke… `coreUnitDock`: spawn `docked.create` at the unit,
behind it when both fly, controller player, spawnedByCore) and return; otherwise
`player.clearUnit(); player.checkSpawn()` (immediate core spawn).
`UnitComp.update` (`:869-871`): `if(spawnedByCore && !isPlayer() && !dead) Call.unitDespawn(self())` (removal without death effects).
When the player leaves a unit its controller resets: `resetController()` -> `type.createController(unit)`.

**Port today.** `Sim.controlled_unit: Option<i32>` (mod.rs:178) unused by the runtime;
`spawn_player_unit` sets `ControllerSlot.kind = Player`, inserts `comp::Player`, sets
`self.player_unit`. `select_team_controller(world, def, team)` (`entities/comp/unit/lifecycle.rs:131`).

**Changes.**
1. `Sim::possess_unit(&mut self, seq: i32) -> bool` per Java (find entity by `EntitySeq`,
   validate team = `player_team`, not dead, `def.player_controllable`, current controller not
   Player). Move the `comp::Player` marker and `ControllerSlot.kind = Player` to the new unit;
   previous unit: if `spawned_by_core` -> despawn (`world.despawn` + remove from `runtime.units`,
   no death abilities); else restore its controller to `select_team_controller`. Store a
   `DockedType(UnitTypeId)` component on the new unit when the previous was core-spawned.
2. `Sim::clear_unit(&mut self)`: if the current unit is not core-spawned and has `DockedType`
   whose def has `core_unit_dock` -> spawn that type next to it (offset behind when both fly:
   `trns(rotation+180, hit/2 + new_hit/2)`), possess it; else release the current unit to AI and
   `spawn_player_at_best_core` immediately.
3. `SimCommand::UnitControl { unit: Some(seq) }` -> `possess_unit`; `None` and `UnitClear` ->
   `clear_unit`. Keep `controlled_unit` updated for compatibility.
4. In `unit_update_system`, add Java's despawn rule: a `spawned_by_core` unit whose controller is
   not `Player` is despawned (covers death-of-player edge cases).

**Tests.** `possess_own_dagger_despawns_core_alpha`; `clear_unit_returns_to_core`;
`cannot_possess_enemy_or_dead_unit`.

**Verify in-engine.** G08 owns the Ctrl-click path; here: Ground Zero, spawn a team-1 dagger
(`spawn_unit`), read its id from `selectable_units_json()`, then
`$G --eval 'return get_node("/root/Spine/SimHost").apply_sim_command_json("{\"op\":\"unit_control\",\"unit\":ID}")'`
— the JSON parser in `gdext/sim_host.rs` (~590, `match op { "unit_command" => … }`) has no
`unit_control`/`unit_clear` ops today: add `"unit_control" => SimCommand::UnitControl { unit: json["unit"].as_i64().map(|v| v as i32) }`
and `"unit_clear" => SimCommand::UnitClear` next to `"unit_command"` (same file is shared; keep it minimal). Expect `unit_debug_json()`
to show the dagger with controller `Player` and no alpha.

**Pitfalls.** `player_unit` is used by builders/mining/HUD — update all through the one setter.
`PlayerInput` mine tile must be cleared on swap.

**Commit.** `Gameplay G01 T9: unit possession and unit clear in the simulation`

---
### T10 — CommandAI command controllers (mine / repair / rebuild / assist)

Shared design for T10a-c. Java `CommandAI.updateUnit` (`J/ai/types/CommandAI.java:117-170`):
```java
if(command == mineCommand && !hasStance(mineAuto) && !ItemUnitStance.all().contains(this::hasStance)) setStance(mineAuto);
...
if(command == null && type.commands.size > 0) command = type.defaultCommand == null ? type.commands.first() : type.defaultCommand;
if(lastCommand != command){ lastCommand = command; commandController = command == null ? null : command.controller.get(unit); }
if(commandController != null){ commandController.updateUnit(); }      // RepairAI / BuilderAI / BuilderAI(onlyAssist) / MinerAI
else { defaultBehavior(); boost handling … }
```
`UnitCommand.loadAll` (`J/ai/UnitCommand.java:73-116`): `repair -> RepairAI`, `rebuild -> BuilderAI`,
`assist -> BuilderAI{onlyAssist=true}`, `mine -> MinerAI`; move/enterPayload/loadUnits/loadBlocks/
unloadPayload/loopPayload -> `null` (default behaviour). `CommandAI.command(c)` clears `mineTile`
and build plans. Defaults: mono `mineCommand`, poly `rebuildCommand`, mega `repairCommand`
(`UnitTypes.java:1346,1365,1423`).

Port: the command id lives in **two** places — `ControllerSlot.command` (set by factories,
`core/world/blocks/units/behavior.rs:70-84`) and `UnitCommandComp` (set by
`Sim::apply_live_unit_command`, `core/sim/mod.rs:484-490`). The RT Command branch ignores both.

**Common change (do it in T10a).** Make `ControllerSlot.command` the single source: in
`apply_live_unit_command` also write `slot.command = Some(command_id)` (keep inserting
`UnitCommandComp` for existing readers; mark it deprecated in its doc). In the RT Command branch
resolve `let cmd = slot.command.or(def.default_command)` and dispatch by
`content.unit_command(cmd).name`: `"mine"` -> T10a, `"repair"` -> T10b, `"rebuild"`/`"assist"` ->
T10c, else the existing default behaviour. Per-unit controller state lives in components
(`MinerAiState`, `RepairAiState`, `BuilderAiState`) inserted lazily; on command change
(compare a stored `last_command`) remove the old state component and clear `MinerComp.mine_tile`
and `BuilderPlans.plans` (Java `command()`).

### T10a — `MinerComp.update` for every unit + `MinerAI` (mono)

**Symptom / acceptance.** A mono from an air factory hovers; ore is never mined by units. After:
`live_sim` with a player core and copper ore 10 tiles away: a team-1 mono (default command mine)
delivers ≥ 20 copper to the core within 2000 ticks.

**Java.** `MinerComp.update` (`J/entities/comp/MinerComp.java:68-122`) quoted in full in the
audit; key lines:
```java
if(mineTile == null) return;
core = closestCore(); item = getMineResult(mineTile);   // mineFloor && tile.block()==air ? tile.drop() : mineWalls ? tile.wallDrop() : null; then canMine(item) (tier >= hardness)
if(core != null && item != null && !acceptsItem(item) && within(core, mineTransferRange /*220*/) && !offloadImmediately()){
   accepted = core.acceptStack(item(), stack.amount, this); if(accepted > 0){ transferItemTo(core); clearItem(); } }
if(!validMine(mineTile)) { mineTile = null; mineTimer = 0; }               // validMine: within(tile, type.mineRange) && getMineResult != null
else if(mining() && item != null){                                         // mining(): mineTile != null && !activelyBuilding()
   mineTimer += delta * type.mineSpeed * rules.unitMineSpeed(team);
   if(mineTimer >= 50 + (type.mineHardnessScaling ? item.hardness*15 : 15)){ mineTimer = 0;
      if(rules.sector != null && team == defaultTeam) sector.info.handleProduction(item, 1);
      if(core != null && within(core, 220) && core.acceptStack(item, 1, this) == 1 && offloadImmediately()) { … player path … }
      else if(acceptsItem(item)) addItem(item);          // (via transferItemToUnit effect)
      else { mineTile = null; mineTimer = 0; } } }
```
`MinerAI.updateMovement` (`J/ai/types/MinerAI.java:24-91`):
```java
core = unit.closestCore(); if(!unit.canMine() || core == null) return;
if(!unit.validMine(unit.mineTile)) unit.mineTile(null);
if(mining){
  if(timer.get(timerTarget2, 240) || targetItem == null)
     targetItem = (ai != null && !ai.hasStance(mineAuto))
        ? content.items().min(i -> hasOre(i) && canMine(i) && ai.hasStance(itemStance(i)), i -> core.items.get(i))
        : type.mineItems.min(i -> ((mineFloor && indexer.hasOre(i)) || (mineWalls && hasWallOre(i))) && unit.canMine(i), i -> core.items.get(i));
  if(targetItem != null && core.acceptStack(targetItem, 1, unit) == 0){ unit.clearItem(); unit.mineTile = null; return; }
  if(unit.stack.amount >= type.itemCapacity || (targetItem != null && !unit.acceptsItem(targetItem))) mining = false;
  else {
     if(timer.get(timerTarget3, 60) && targetItem != null){ ore = null; if(type.mineFloor) ore = indexer.findClosestOre(core.x, core.y, targetItem); if(ore == null && type.mineWalls) ore = indexer.findClosestWallOre(core.x, core.y, targetItem); }
     if(ore != null){ moveTo(ore, type.mineRange / 2f, 20f); if(unit.within(ore, type.mineRange) && unit.validMine(ore)) unit.mineTile = ore; } }
} else {
  unit.mineTile = null;
  if(unit.stack.amount == 0){ mining = true; return; }
  if(unit.within(core, type.range)){ if(core.acceptStack(stack.item, stack.amount, unit) > 0) transferItemTo(core); unit.clearItem(); mining = true; }
  circle(core, type.range / 1.8f); }
if(!type.flying) unit.updateBoosting(type.boostWhenMining || floor.isDuct || floor.damageTaken > 0 || floor.isDeep());
```
`type.mineItems` default `[copper, lead, titanium, thorium]` (`UnitType.java`, grep `mineItems`).
`content.items().min(pred, key)`: lowest `core.items.get(i)`; ties -> first in id order.
Mono: `mineTier 1, mineSpeed 2.5, itemCapacity 20, range 50, mineRange 70` (default), flying.
Copper hardness 1 -> threshold `50 + 15 = 65` -> `65 / 2.5 = 26` ticks per item -> 20 items in 520 ticks.

**Port today.** Player-only mining `ai/types/player.rs:184 update_mining` (+ `offload_into_core`
274, `closest_core` 299) keyed on the `PlayerInput` resource. `MinerComp { mine_tile, mining }`
(`entities/comp/unit/comp.rs:571`) has no timer. `ai/types/miner.rs` (`find_ore_tile` around the
unit, `update_miner` never called). Core item helpers used by the player path:
`dispatch_get_maximum_accepted`, `item_count`, `dispatch_handle_stack` (imports at the top of
`player.rs`).

**Changes.**
1. `MinerComp`: add `pub mine_timer: f32`. Move the body of `update_mining` into a generic
   `pub fn update_miner_comp(world, grid, content, entity)` in a new
   `core/entities/comp/unit/miner.rs` (GPL header) implementing the Java block above for **any**
   unit with `MinerComp.mine_tile` set; `offloadImmediately()` = entity has `comp::Player`.
   Player path: keep `PlayerInput.mine_tile` as the order source but write it into
   `MinerComp.mine_tile` and call the generic function (delete the duplicate logic; keep
   `PlayerInput.mined`/production delta behaviour). Call it for every unit in `unit_update_system`
   after kinematics (Java order: component `update` after controller).
2. `core/ai/types/miner.rs`: replace `update_miner` with a faithful `MinerAiState { mining: bool (init true), target_item: Option<ItemId>, ore: Option<TilePos>, timer2: f32, timer3: f32 }`
   + `update_miner_ai(ctx, unit, state, stances)`; add `find_closest_ore_for_item(grid, content, item, x, y) -> Option<TilePos>`
   (full-map row-major scan, floor/overlay drop == item and `tile.block == AIR`, strictly-smaller
   `dist2` wins => deterministic; document the deviation: Java takes the first ore of each
   indexer quadrant). `has_ore(item)` = the same scan returning bool (cache per tick in
   `UnitRuntime` if profiling shows cost; not required).
   Movement: add `ai_controller::move_to_pos(world, grid, content, unit, x, y, circle_length, smooth)`
   (Java `moveTo(target, circleLength, smooth)`, keepDistance = flying, `length = clamp((dst - circle)/smooth, -1, 1)`)
   and `circle_pos(world, grid, content, unit, x, y, circle_length)` (Java `circle`: if `dst < r`
   rotate the vector by `(r - dst)/r*180`, set length `prefSpeed`, `movePref`) by generalising
   `ai/types/flying.rs:198 move_to` (it hard-codes smooth 100 and an entity target).
3. RT Command branch: `"mine"` -> `update_miner_ai` (+ stances from T11 when present).

**Tests.** `mono_mines_and_delivers` (`combat_tests.rs`): campaign default 1; `core-shard` team 1
at (10,10); ore: `sim.grid.tiles.get_mut(20+i, 10).overlay = ore-copper id` for i in 0..3 then
`sim.refresh_unit_runtime()`; mono team 1 at the core centre with `slot.kind = Command` (as in
`live_unit_command_switch_is_stored`) and no explicit command (default `mine`); tick 2000;
`core copper >= 20` (read with `crate::world::modules::ItemModule` on the core entity).
`miner_comp_respects_tier`: mono ordered onto titanium ore (hardness 3 > tier 1) never mines.
Existing player mining tests in `ai/types/player.rs` must stay green.

**Verify in-engine.** Ground Zero: spawn a team-1 mono at the core (`spawn_unit`, team 1, px
1032, py 440); step 2400; `core_items_json()` copper increased by ≥ 20 (copper ore at
(130..131,48..49)). Screenshot.

**Pitfalls.** `Tile.drop()` vs overlay: reuse `tile_drop` semantics from `base_builder_ai.rs:562`.
Do not double-count sector production (the player path already pushes `production_deltas`;
generic path must push for all default-team units, as Java does).

**Commit.** `Gameplay G01 T10a: unit mining for all units and faithful MinerAI (mono mine command)`

### T10b — `RepairAI` (mega repair command)

**Symptom / acceptance.** Mega (default `repair`) idles. After: a mega near a damaged friendly
wall flies to `range*0.65` and its heal shots restore the wall (needs T4).

**Java.** `J/ai/types/RepairAI.java:15-80`: constants `retreatDst 160, fleeRange 310, retreatDelay 180`.
`updateTargeting`: every 15 ticks `damagedTarget = Units.findDamagedTile(team, x, y)` (closest
damaged building of the own team, any distance; null if `ConstructBuild`); if null ->
`super.updateTargeting()` (enemy targeting) else `target = damagedTarget`.
`updateMovement`: if target is a Building: `shoot = target.within(unit, type.range)`; aim at it;
`controlWeapons(shoot)`; else if target null `controlWeapons(false)`.
If target is an own building and not holdPosition: `circleTarget ? circleAttack(r) : if(!within(range*0.65)) moveTo(target, range*0.65)`; lookAt.
Not repairing and not holding: every 40 ticks `avoid = target(x, y, fleeRange, true, true)`;
`retreatTimer += 1; if >= retreatDelay && avoid != null && core != null && !within(core, retreatDst)) moveTo(core, retreatDst)`;
else `retreatTimer = 0`.

**Port today.** `ai/types/repair.rs` heals buildings directly (+1/tick) — wrong; never called.
`BlockIndexer::damaged()` (`core/ai/block_indexer.rs:151`) lists damaged buildings
(rebuilt with the indexer; check freshness: `grep -n "fn rebuild\|damaged.push" core/ai/block_indexer.rs`).

**Changes.** Rewrite `update_repair` per Java with `RepairAiState { avoid, retreat_timer, timer15, timer40, target }`;
weapons: set every controllable mount's `aim` to the target centre and `shoot = shoot` (reuse
`set_mount_targets(world, content, entity, Some(target))` then override `shoot` with the range
check). Remove the direct heal. RT Command branch `"repair"` -> it.

**Tests.** `mega_repairs_damaged_wall`: campaign default 1; core-shard team 1 (10,10);
copper-wall team 1 at (30,10) health set 100; mega team 1 at (10,14)*8 Command; tick 900; wall
health > 100 and mega within `range` (178 px) of the wall at the end.

**Verify in-engine.** Ground Zero: damage a wall with a team-2 dagger, spawn a team-1 mega;
`building_debug` health rises.

**Commit.** `Gameplay G01 T10b: faithful RepairAI for the repair command`

### T10c — `BuilderAI` rebuild/assist + builder update for every builder unit

**Symptom / acceptance.** Destroyed player blocks leave rebuild ghosts (prior T16) but polys
(default `rebuild`) never rebuild them; the `assist` command does nothing; AI-team core units
(alpha/beta/gamma/evoke/incite/emanate on AI teams) do not build. After: a destroyed copper wall
is rebuilt by a nearby poly within 1200 ticks (costs taken from the core).

**Java.** `J/ai/types/BuilderAI.java:18-262` (constants `buildRadius 1500, retreatDst 110,
retreatDelay 120, defaultRebuildPeriod 120`; `init`: `rebuildPeriod = 10` if `team.rules().buildAi`).
Key `updateMovement` flow (transcribe exactly; full text in the Java file):
1. look at target or `prefRotation`; `unit.updateBuilding = true`; validate `assistFollowing`/`following`.
2. `following` -> copy its first build plan (`unit.plans.clear(); addFirst(following.buildPlan())`).
3. else if `(buildPlan == null || alwaysFlee) && !hold`: every 40 ticks `enemy = target(x, y, fleeRange /*370*/, true, true)`;
   after `retreatTimer >= retreatDelay` (or alwaysFlee) with an enemy: `clearBuilding()`, move to the
   closest core within `retreatDst`.
4. with a build plan: validity = construct of the same block present, or `validBreak`/`validPlace`;
   valid -> `moveTo(req.tile(), min(type.buildRange - hitSize*2, buildRadius), 20)`; invalid -> drop it.
5. without a plan: (assist follow move) every 20 ticks look for a nearby building ally to follow
   (`Units.nearby(team, x, y, buildRadius)`, `u.canBuild() && u.activelyBuilding()`, reachable
   before `cons.buildCost*0.9`); `onlyAssist` -> follow the closest builder player.
   Every `rebuildPeriod` ticks with `!onlyAssist && team.data().plans` non-empty and not following:
   `block = plans.first()`; if already placed -> `removeFirst`; else if `Build.validPlace(...)` (and
   not near an enemy when alwaysFlee) -> `unit.addBuild(new BuildPlan(x, y, rot, block, config))`
   and rotate the queue head to the tail; else rotate head to tail.
6. ground units: `updateBoosting(boostWhenBuilding || moving || floor deep/duct/damaging)`.
`fallback()` / `useFallback()`: AI team with `prebuildAi` -> PrebuildAI; wave team w/o `rtsAi` on
wave maps -> FlyingAI/GroundAI (so wave-team alpha/beta/gamma fight).
`shouldShoot = !isBuilding && type.canAttack`.

**Port today.** `core/sim/builder.rs:942 update_builders` only advances the **player** unit's
`BuilderPlans` (`fn update_builder(world, grid, runtime, unit, plans)` at :415 is generic in its
arguments). Team rebuild plans: `CampaignRuntime.session.teams.get(team).plans: VecDeque<BlockPlan>`
(`core/game/teams.rs`; filled by prior T16). Placement validation:
`crate::world::build::valid_place(content, table, rules, counter, grid, block, team, rot, x, y)`.
`ai/types/builder.rs` is a stub; RT dispatch only with a commanded tile.

**Changes.**
1. `update_builders`: iterate **all** units with `BuilderPlans` and non-empty plans in
   `EntitySeq` order (player first is not required); same `update_builder` call per unit.
   Ensure `BuilderPlans` is inserted on every unit whose def has `build_speed > 0`
   (`grep -n "BuilderPlans::default" core` — spawn site in `lifecycle.rs`; add it there if only the
   player gets it).
2. Rewrite `ai/types/builder.rs` per Java with `BuilderAiState { following, assist_following, enemy, last_plan, retreat_timer, timers[4], only_assist, always_flee, flee_range, rebuild_period }`.
   Constructors: command `rebuild` -> `BuilderAI()` (alwaysFlee false, fleeRange 370); `assist` ->
   `onlyAssist = true`; AI-team core units (`ControllerKind::Builder`/`BuilderOrCommand` on AI teams) ->
   `BuilderAI(true, 400)` for alpha/beta/gamma (`UnitTypes.java:2495`) and `BuilderAI(true, coreFleeRange)`
   for evoke/incite/emanate (`coreFleeRange = 500f`, `UnitTypes.java:4352`).
   Fallback: if `team_is_ai && rule.prebuild_ai` -> stay on builder (PrebuildAI deferred, log once);
   if `rules.waves && team == wave_team && !rule.rts_ai` -> run the GroundAI/FlyingAI path (T6).
3. RT: dispatch `AiKind::Builder` (any slot) and Command `rebuild`/`assist` to the new function;
   delete the `slot.target.is_some()` special case (keep the commanded-tile behaviour as the
   "plan at tile" case: convert it into a `BuilderPlans` entry).

**Tests.** `poly_rebuilds_destroyed_wall`: campaign default 1, `core-shard` team 1 with 500 copper
(`debug` via `ItemModule::add`), copper-wall team 1 at (20,10) destroyed (set health 0, tick so
`resolve_deaths` adds the ghost plan), poly team 1 (Command, default rebuild) at the core; tick
1200 -> the tile holds `copper-wall` team 1 again and core copper decreased by 6 (copper-wall cost
6 copper; `rules.build_cost_multiplier` 1). `wave_alpha_falls_back_to_ground_ai` (wave team alpha
moves toward the player core). Keep `commanded_builder_repairs_target` green or update it to
the new semantics (document).

**Verify in-engine.** Ground Zero: place a copper wall, break it with `break_block`? (that is not
a destruction) — instead spawn team-2 daggers next to it until it dies, then spawn a team-1 poly;
after 1200 steps `tile_debug` shows the wall back.

**Pitfalls.** `update_builder` mutates the grid and the block runtime: one unit at a time, take
`BuilderPlans` out with `std::mem::take` like the player path. Construct progress per builder must
not double count when two units build the same plan (Java allows assisting; progress adds).

**Commit.** `Gameplay G01 T10c: BuilderAI rebuild/assist and builder updates for every builder unit`

---

### T11 — Live unit stances

**Symptom / acceptance.** The stance buttons/keys (hold fire, pursue, boost, hold position, mine
item) never reach live units (`SimCommand::UnitStance` -> relay only, `core/sim/mod.rs:820-828`).
After: hold-fire daggers stop shooting; `boost` makes novas fly; mono with a `copper`-item stance
only mines copper.

**Java.** `CommandAI.setStance/disableStance` (`CommandAI.java:72-96`: `stance == stop` is a no-op
here — `stop` is handled by input as `clearCommands`; `stances.andNot(stance.incompatibleStanceBits); stances.set(id)`),
`shouldFire() = !hasStance(holdFire)` (`:527`) applied in `updateWeapons` (`mount.shoot = false`),
`shouldBoost() = hasStance(boost) || command == enterPayload` (`:172-174`) and the boost block in
`updateUnit` (`:150-163`), `pursueTarget` (`:124-126`: `commandTarget(target, false)` when idle),
`holdPosition` read by Builder/Repair/Miner AIs, `mineAuto` and `ItemUnitStance` read by MinerAI,
`stanceChanged()` -> MinerAI clears `targetItem` if its item stance was removed.

**Port today.** `StanceBits` (`core/ai/unit_stance_runtime.rs`), `set_stance_enabled(&mut bits, content, stance, enabled)`
used by the relay (`core/sim/command.rs:176-193`). Stance registry `core/content/registries/stances.rs`.

**Changes.** Component `UnitStancesComp(StanceBits)` on live units; `Sim::apply_live_unit_stance(ids, stance, enabled) -> usize`
mirroring `apply_live_unit_command`'s filters (default team, Command controller, stance allowed
by the type: `allows_stance` — `grep -n "fn allows_stance\|getUnitStances" core/ai/unit_command_runtime.rs`);
`SimCommand::UnitStance` tries live first, relay when 0 (same pattern as UnitCommand).
Consumers: Command default behaviour honours `holdFire` (after `set_mount_targets` force
`mount.shoot = false`), `boost` (set `UnitCore.boosting = true` unless within range of the attack
target — Java `updateBoosting(false)` near target), `pursueTarget` (idle with a mount target ->
`slot.target = target tile`), `holdPosition` (T10 AIs skip movement), mine stances (T10a
`target_item` selection; mine command auto-adds `mineAuto` when no item stance is set).
`patrol` and `ram`: deferred (listed in §2 Deferred).

**Tests.** `hold_fire_stops_shooting` (dagger with an enemy in range: shots counted over 300 ticks
== 0 after the stance; > 0 before); `boost_stance_lifts_nova`; `mono_item_stance_filters_item`.

**Verify in-engine.** G08 owns the buttons. Here: Ground Zero, two team-1 daggers selected via
`Input.selection_json()` flow is G08's; use the command JSON path
(add a `"unit_stance"` op to the `apply_sim_command_json` parser in `gdext/sim_host.rs` ~590:
`{"op":"unit_stance","units":[seq,…],"stance":<id in content_list("unitStance")>,"enabled":true}` ->
`SimCommand::UnitStance { units, stance, enabled }`), then count team-1 bullets with
`get_group_count("bullet")` over 300 steps: 0 with holdFire.

**Commit.** `Gameplay G01 T11: live unit stances (holdFire, boost, pursueTarget, holdPosition, mine items)`

---

### T12 — Unit cap (`Units.canCreate`, `getCap`, cap death)

**Symptom / acceptance.** Factories keep producing past the unit cap; the HUD cap is
meaningless. After: with `rules.unit_cap = 0`, `unit_cap_variable = true` and one core-shard
(`unitCapModifier 8`), the 9th dagger from a ground factory is not released (the factory holds
it) and a unit added past the cap by other means dies (`unitCapDeath`).

**Java.** `Units.canCreate(team, type) = !type.useUnitCap || (team.data().countType(type) < getCap(team) && !type.isBanned())`
(`J/entities/Units.java:115-125`); `getCap(team)`: `Integer.MAX_VALUE` if
`(team == waveTeam && !pvp) || (isCampaign && team == waveTeam) || rules.disableUnitCap || team.ignoreUnitCap`;
else `max(0, unitCapVariable ? rules.unitCap + team.data().unitCap : rules.unitCap)`.
`TeamData.unitCap` += / -= `block.unitCapModifier` when a building of that team is added/removed
(`J/ai/BlockIndexer.java:216,614`; cores 8/16/24 Serpulo, 15 Erekir; other blocks 0).
`UnitComp.add` (`:601-611`): `if(type.useUnitCap && count() > cap() && !spawnedByCore && !dead && !rules.editor) Call.unitCapDeath(self())`.
`UnitPayload.dump` (`J/world/blocks/payloads/UnitPayload.java:104-112`): `if(!Units.canCreate(team, type)) return false` (factory keeps the payload).
Note: counts are per **type** (`countType(type)`), not total.

**Port today.** `can_create` helper (`core/entities/comp/unit/queries.rs:185`) unused;
`TeamData.unit_cap` (`core/game/teams.rs:141`) never written; `TeamData.type_counts` filled by the
team snapshot (`teams.rs:443-497`). Factory output: `world/blocks/units/behavior.rs:265-298`
(`UnitFactory` `update_tile` spawns immediately via `spawn_output_unit_with_command`); reconstructor
at ~444. Rules: `unit_cap`, `unit_cap_variable`, `disable_unit_cap` (`core/io/json/rules.rs:524,606,608`);
`Team.ignore_unit_cap` (`core/game/team.rs:55`).

**Changes.**
1. `pub fn unit_cap(world: &World, team: u8) -> i32` and `pub fn can_create(world, team, def) -> bool`
   in `core/entities/comp/unit/queries.rs` (compute the per-team core modifier sum directly from
   live `Building`s of `team` whose `BlockDef.unit_cap_modifier != 0`; count live units of the type
   from `UnitRuntime.units` (TeamComp + UnitTypeComp) — do not rely on the stale snapshot).
2. Factory/reconstructor output (shared file with G02): before spawning, `if !can_create(...) { keep progress capped; return }`
   — Java holds the finished payload; port equivalent: do not consume/spawn and keep
   `progress` at the plan time so it spawns as soon as the cap frees up. Add a 1-line comment
   citing `UnitPayload.dump`.
3. `UnitRuntime::spawn*` / `spawn_unit_def`: after insertion, if `def.use_unit_cap && count > cap && !spawned_by_core` -> mark dead
   (Java `unitCapDeath` kills without explosion: set `Health.dead` and a `NoDeathEffects` marker
   if one exists; otherwise just despawn).

**Tests.** `factory_respects_unit_cap` (ground-factory with dagger plan, infinite items via
`debug`, cap 1 by `rules.unit_cap = 1, unit_cap_variable = false`): after long ticking exactly 1
dagger exists. `core_modifier_raises_cap` (shard -> cap 8). `wave_team_uncapped`.

**Verify in-engine.** Headless (factory feeding needs G02 item-source config).

**Commit.** `Gameplay G01 T12: unit cap (Units.canCreate/getCap) gates factories and spawns`

---
### T13 — `UnitComp.update` environment: drowning, floor status, bounds, solid-tile kill, drop-zone push, captured-sector death

**Symptom / acceptance.** Ground units walk into deep water/tar/slag and never drown; no unit
gets `wet`/`tarred`/`melting`/`muddy`/`freezing` from the floor; units pushed off the map
stay there; units on solid tiles (e.g. a block built under them) survive; player units are not
pushed out of the enemy drop zone; ice does not reduce drag. After (each a test): a dagger held
in `deep-water` dies after `drownTime` math below; a dagger on `shallow-water` carries `wet`; a
unit at x = -300 dies; a dagger on a copper wall tile dies, a nova lifts off; a team-1 dagger
inside a wave map's drop zone is pushed outward.

**Java.** `J/entities/comp/UnitComp.java:662-860` (`update`), constants `warpDst = 8`
(`UnitComp.java:40`), `finalWorldBounds = 250` (`Vars.java:117`):
```java
// bounds (type.bounded, default true)
bot=0,left=0,top=world.unitHeight(),right=world.unitWidth();
if(rules.limitMapArea && !team.isAI()){ bot=limitY*8; left=limitX*8; top=limitHeight*8+bot; right=limitWidth*8+left; }
dx=0,dy=0; if(x<left) dx += -(x-left)/warpDst; if(y<bot) dy += -(y-bot)/warpDst;
if(x>right-8) dx -= (x-(right-8))/warpDst; if(y>top-8) dy -= (y-(top-8))/warpDst;
velAddNet(clamp(dx, -10, 10), clamp(dy, -10, 10));
x = clamp(x, left-8, right-8+8); y = clamp(y, bot-8, top-8+8);
if(isGrounded()){ x = clamp(x, left, right-8); y = clamp(y, bot, top-8); }
if(x < -250+left || y < -250+bot || x >= right+250 || y >= top+250) kill();
if(isNaN(health)) kill();
updateDrowning():  floor = drownFloor();
   if(floor != null && floor.isLiquid && floor.drownTime > 0 && canDrown()){ drownTime += 1 / (hitSize/8 * type.drownTimeMultiplier * floor.drownTime); if(drownTime >= 0.999f) kill(); }
   else drownTime -= 1/50f;   drownTime = clamp(drownTime, 0, 1);
   canDrown() = isGrounded() && type.canDrown;  drownFloor(): base floorOn(); Mech: hitSize>=12 -> null unless all 8 neighbours deep; Legs/Crawl: lastDeepFloor; Tank: canDrown() ? lastDeepFloor : null
if(team.isOnlyAI() && state.isCampaign() && sector.isCaptured()) kill();
if(!type.supportsEnv(rules.env) && !dead) unitEnvDeath (kill)
drag = type.drag * (isGrounded() ? floorOn().dragMultiplier : 1) * dragMultiplier(status) * rules.dragMultiplier;
if(team != waveTeam && state.hasSpawns() && hittable()){ relativeSize = dropZoneRadius + hitSize/2 + 1;
   for(spawn) if(within(spawn.worldx(), spawn.worldy(), relativeSize)) velAddNet(v.set(this).sub(spawn).setLength(0.1f + 1f - dst(spawn)/relativeSize).scl(0.45f)); }
if(tile != null && isGrounded() && !type.hovering){ if(tile.build != null) tile.build.unitOn(self()); if(floor.damageTaken > 0) damageContinuous(floor.damageTaken); }
if(tile != null && !canPassOn()){ if(type.canBoost) elevation = 1; else kill(); }
```
`StatusComp.update` (`StatusComp.java:192-197`): `if(isGrounded() && !type.hovering) apply(floor.status, floor.statusDuration)`.
Floor data (`J/content/Blocks.java` floors): deep-water wet 120 drown 200; shallow-water wet 90;
tainted-water wet 90; deep-tainted-water wet 140 drown 200; darksand-tainted-water wet 60
(ShallowLiquid takes the liquid base's status); sand-water / darksand-water wet 50; tar tarred 240
drown 230; pooled-cryofluid freezing 240 drown 150; molten-slag melting 240 drown 230; mud muddy 30;
arkycite-floor drown 200 (no status); dragMultiplier: ice 0.35, ice-snow 0.6, red-ice 0.4.
No vanilla floor sets `damageTaken` (keep the hook, value 0).

**Port today.** `movement.rs:123 update_kinematics` runs statuses, abilities, kind movement,
boosting, tether. Floor meta (`core/world/behavior/environment.rs:45 FloorMeta`: is_deep, is_liquid,
shallow, speed_multiplier, drown_time). `BlockSpec` has `is_liquid`, `drown_time`, no status/drag.
`last_deep_floor` fields on legs/crawl/tank state (`entities/comp/unit/comp.rs:202,229`). Solidity:
`path_tile::is_solid(content, block)` (`core/ai/pathfinder/path_tile.rs:108`),
`water_move::water_solid` (`entities/comp/unit/water_move.rs:32`). Kill = set `Health.dead = true`
(`resolve_deaths` handles it). Unit fields: `def.can_drown`, `drown_time_multiplier`, `hovering`,
`bounded`, `can_boost`, env masks (`UDEF:1157-1425`).

**Changes.**
1. Content (shared with G02, floors only): `BlockSpec`/`BlockDef` fields
   `floor_status: Option<&'static str>`, `floor_status_duration: f32` (default 60 — Java
   `Floor.statusDuration = 60f`), `floor_drag_multiplier: f32` (default 1), `floor_damage_taken: f32` (0);
   fill the values listed above in `core/content/registries/blocks/environment.rs`.
2. New `core/entities/comp/unit/environment.rs` (GPL header) with
   `pub fn update_unit_environment(world, grid, content, rules: Option<&Rules>, spawns: &[TilePos], entity)`
   implementing, in Java order: bounds — when `grid.limit_area == Some((lx, ly, lw, lh))` and the
   unit's team is **not** AI (`team_ai_flags`), use `left = lx*8, bot = ly*8, right = (lx+lw)*8,
   top = (ly+lh)*8` (Java `rules.limitX..limitHeight`), else the full map (`grid.width()*8`,
   `grid.height()*8`). **Dependency: G04 T5** sets `WorldGrid.limit_area` (`core/world/mod.rs:152`)
   from the sector rules and on `setrule` changes; until it lands the full-map branch runs. Test
   `limit_area_clamps_player_units` sets `grid.limit_area` directly (team-1 grounded dagger at a
   tile left of the area ends clamped to `x >= lx*8`; a team-2 AI unit is not clamped), NaN, drowning (new `UnitCore.drown_time: f32` or a `DrownComp`; per-kind drown floor as
   above), captured-sector AI death (needs `session.sector` captured flag — `grep -n "captured" core/game/play.rs`),
   env death (`supportsEnv`), floor status (`apply_status(world, e, status, duration)` when grounded &
   not hovering), drag (`PhysicsComp.drag = def.drag * floor_drag * status.drag_multiplier * rules.drag_multiplier` —
   check how drag is consumed: `grep -n "\.drag" core/entities/comp/unit/*.rs core/ai/types/*.rs`),
   drop-zone push, `damage_taken`, solid-tile kill/boost (solidity per kind: legs ->
   `legs solid` if `allow_leg_step` (only static walls; check for an existing predicate, else
   `is_solid` of a static wall block), naval -> `water_solid`, mech/ground -> `is_solid`, flying -> none).
3. Call it from `unit_update_system` right after `movement::update_kinematics` (skip `Player`?
   No: Java runs it for player units too).
4. Wire `building.unitOn(unit)` hook only as a no-op call site comment (conveyor pushing units
   is G02 — §6).

**Tests** (`combat_tests.rs`): `dagger_drowns_in_deep_water` — floor `deep-water` around (20,20)
(set `grid.tiles.get_mut(x,y).floor` for a 5x5 area, `refresh_unit_runtime`), dagger team 2 held
in place (set `ControllerSlot.kind = NoAi`); per tick `drownTime += 1/(8/8 * 1 * 200) = 0.005`
-> dies at tick ⌈0.999/0.005⌉ = 200 (± 1); assert alive at 195, dead by 205.
`shallow_water_applies_wet` (status `wet` present after 1 tick, duration 90).
`unit_out_of_bounds_dies` (x = -300 on a 48x48 map, `-250 + 0` -> kill).
`dagger_on_wall_dies_nova_boosts`. `drop_zone_pushes_player_units` (waves true, spawn overlay at
(24,24), team-1 dagger at 50 px from it: after 10 ticks its distance grew).

**Verify in-engine.** Ground Zero has deep water? (UNVERIFIED: `tile_debug` a few water tiles).
Headless acceptance is sufficient; optional: spawn a team-2 dagger on a deep-water tile and
`step(300)`.

**Pitfalls.** Order matters for determinism (statuses before drowning). Do not kill units
carried as payload (they have no `Pos` in the world? check `PayloadHolder` semantics: carried units
are despawned/hidden — skip entities with a "carried" marker if one exists).
The dead-flyer **fall** (`elevation -= fallSpeed` until grounded, then `unitDestroy`) is deferred
(see §2 Deferred) — current deaths stay instant.

**Commit.** `Gameplay G01 T13: unit environment update (drowning, floor status/drag, bounds, solid kill, drop-zone push)`

---

### T14 — Explosions: unit death `dynamicExplosion`, ExplosionShield absorption, fire/oxygen rules, block explosiveness fields

**Symptom / acceptance.** A unit carrying blast compound/pyratite/surge dies without exploding;
force projectors never absorb block/unit explosions or crash damage; fires start on Erekir and on
maps with `rules.fire = false`. After: a dagger carrying 30 blast-compound dies and damages
neighbours (`explosiveness = 2 + 1.2*30*1.53 = 57.08` -> `waves = clamp((int)(57.08/11),1,25) = 5`,
`damagePerWave = 28.54`, radius `clamp(radius + 57.08, 0, 50) = 50` at the last wave); a fire
cannot be created when `rules.fire == false` or the env lacks oxygen.

**Java.**
* `UnitComp.destroy` (`J/entities/comp/UnitComp.java:883-895`):
  `explosiveness = 2 + item.explosiveness * stack.amount * 1.53f; flammability = item.flammability * amount / 1.9f; power = item.charge * pow(amount, 1.11f) * 160f;`
  `if(!spawnedByCore) Damage.dynamicExplosion(x, y, flammability, explosiveness, power, (bounds() + type.legLength/1.7f)/2f, rules.damageExplosions && rules.unitCrashDamage(team) > 0, item.flammability > 1, team, type.deathExplosionEffect, 0f)`
  with `bounds() = hitSize * 2` (`:459`).
* `Damage.dynamicExplosion` (`J/entities/Damage.java:93-133`): lightning `for i < clamp(power/700, 0, 8)`:
  `Lightning.create(derelict, Pal.power, 3 + pow(power, 0.35f), x, y, random(360), 5 + clamp((int)(pow(power,0.98f)/500),1,18) + range(2))`;
  fire `for i < clamp(flammability/4, 0, 30)`: `createBullet(Bullets.fireball, derelict, x, y, random(360), fireball.damage, 1, 1)`;
  waves: `shields = ignoreTeam == null ? null : indexer.getEnemy(ignoreTeam, BlockFlag.shield); if(shields empty || !shields.contains(s -> s instanceof ExplosionShield e && e.absorbExplosion(x, y, damagePerWave))) damage(ignoreTeam, x, y, clamp(radius + explosiveness, 0, 50)*((f+1)/waves), damagePerWave, false)`.
* Crash damage (`UnitComp.java:924-930`): same ExplosionShield test with `crashDamage`.
* `ForceBuild.absorbExplosion(ex, ey, damage)` (`J/world/blocks/defense/ForceProjector.java:270-278`):
  `absorb = !broken && isInRegularPolygon(sides, x, y, realRadius(), shieldRotation, ex, ey); if(absorb){ hit = 1; buildup += damage * crashDamageMultiplier; }`.
* `Fires.create` (`J/entities/Fires.java:17-18`): return unless `rules.fire && rules.hasEnv(Env.oxygen)` (`oxygen = 1<<7`).
* `Block.baseExplosiveness` (default 0), `flammabilityScale`/`explosivenessScale` (default 1) used
  by `BuildingComp.onDestroyed` (`:1491-1508`); set in `Blocks.java` for e.g. `battery` …
  (`grep -n "baseExplosiveness\|explosivenessScale\|flammabilityScale" J/content/Blocks.java` and copy
  every hit).

**Port today.** `explosion.rs:26 dynamic_explosion(world, content, rng, x, y, flam, expl, power, radius, damage, fire, ignore_team)`:
no lightning (comment), one fire instead of fireballs, no shield test.
`ShieldState::absorb_explosion(x, y, ex, ey, damage) -> bool` exists (`core/world/blocks/defense/shields.rs:153`).
`CombatEnv` resource (`fires.rs:302`) never inserted live. Lightning: `core/combat/lightning.rs`
(`grep -n "pub fn create" core/combat/lightning.rs`). Fireball bullet: `grep -n "fireball" core/content/registries/bullets.rs`.
`building_explosion_terms` (RT ~922) notes the missing block fields.

**Changes.**
1. RT `resolve_deaths` unit loop: before `kill_unit_with_abilities`, compute the terms from
   `ItemsComp.item` (`(item, amount)`) and call `dynamic_explosion` with radius
   `(def.hit_size*2 + def.leg_length/1.7)/2`, `damage = rules.damage_explosions && rules.unit_crash_damage(team) > 0`,
   `fire = item.flammability > 1`, `ignore_team = Some(team)`, skipped when `spawned_by_core`.
2. `dynamic_explosion`: lightning bolts via `lightning::create` (team derelict 0, damage
   `3 + power^0.35`, length per Java; draws in the order: angle `rng.range(0,360)`, length
   `range(2)`) — Java delays them with `Time.run`; port fires immediately (document);
   fireballs: spawn `fireball` bullets (team 0, random angle) `clamp(flam/4, 0, 30)` times instead of
   one `fires::create`; ExplosionShield: before each wave, iterate enemy (≠ ignore team) force
   projector entities with `ShieldState` and call `absorb_explosion`; skip the wave damage if any
   absorbs. Same check in the crash-damage block.
3. `CombatEnv` sync: at the start of `unit_update_system` (or `Sim::tick` before fires),
   insert/update `CombatEnv { fire: rules.fire, oxygen: rules.env & (1<<7) != 0, heat: <existing heat attr source> }`
   from `CampaignRuntime` (leave untouched when no campaign runtime).
4. `BlockSpec/BlockDef`: `base_explosiveness: f32 (0)`, `flammability_scale: f32 (1)`,
   `explosiveness_scale: f32 (1)`; apply in `building_explosion_terms` exactly as
   `BuildingComp.onDestroyed`: `explosiveness = block.baseExplosiveness; … explosiveness *= explosivenessScale; flammability *= flammabilityScale` (read the Java lines to get the exact order).

**Tests.** `unit_with_blast_compound_explodes` (dagger team 2 with 30 blast-compound next to a
team-1 copper wall at 8 px; kill it; wall loses ≥ 28.54 after the waves);
`core_spawned_unit_does_not_explode`; `force_projector_absorbs_explosion` (powered projector over
the explosion: wall undamaged; needs `power-source` — `place` both, verify `ShieldState` radius > 0
after ticks, else note); `fire_disabled_rule_blocks_fire` (`rules.fire = false` -> `fires::create` None).

**Verify in-engine.** Headless only (needs item-carrying enemy units).

**Commit.** `Fixes PAR-0091 residual: unit death explosions, ExplosionShield absorption, fire env rules`
(use `Gameplay G01 T14: …` if PAR-0091 is already closed by the coordinator — see §4).

---

### T15 — `BuildDamageEvent`: emit on bullet damage, drain seam for the minimap, RtsAI `damagedSet`

**Symptom / acceptance.** The minimap never pulses attack indicators; RtsAI's damaged-building
set is approximated by "health below max". After: a team-2 dagger shooting a team-1 wall makes
`SimHost.drain_build_damage_json()` return `[[x,y,1],…]` once per hit (then empty).

**Java.** `BuildingComp.damage(Bullet bullet, Team source, float damage)` (`:1783-1787`):
`damage(source, damage); Events.fire(BuildDamageEvent.set(self(), bullet))` — called from
`collision(Bullet)` (direct hits, `:1761-1776`) and `Damage.tileDamage`/`damage` with a bullet
source (`Damage.java:573,634`). Listeners: `Control.java:68-72` (`if(e.build.team == player.team()) indicators.add(tileX, tileY)`),
`RtsAI.java:47-55` (`if(team.rules().rtsAi) data.rtsAi.damagedSet.add(build)`).

**Port today.** `BuildDamageEvent { x, y, amount }` (`core/world/events.rs:123-132`), never
constructed. Hit sites: `BUL::hit_building` (514), splash: `BUL::create_splash_damage` (705) ->
`AREA::damage_area`. Minimap: `gdext/render/minimap.rs:364 add_indicator(x, y)` (#[func]).
RtsAI damaged: `GRT::team_ai_damaged` + `collect_team_ai_rows` health approximation (GRT ~1167).

**Changes.**
1. Add `pub team: u8` to `BuildDamageEvent`. World resource `BuildDamageLog(pub Vec<BuildDamageEvent>)`
   (insert in `install_unit_runtime`); push in `hit_building` (building targets only) and in the
   splash path for buildings damaged by a bullet-sourced splash (pass a `source_bullet: bool`
   through `DamageOptions` if needed).
2. `Sim::drain_build_damage_events(&mut self) -> Vec<BuildDamageEvent>` (take + clear; cap the
   log at 4096 entries per tick to bound memory if nobody drains).
3. gdext `SimHost`: `#[func] fn drain_build_damage_json(&mut self) -> GString` -> `[[x,y,team],…]`.
   G03 wires it to `MindMinimap.add_indicator` filtered by the player team (§6).
4. RtsAI: keep a per-team `damaged: BTreeSet<Entity>` in `TeamData.rts_ai` fed from the log in
   `team_ai_system` **before** the HUD drains (drain order: `team_ai_system` reads a copy; the
   log is cleared only by `drain_build_damage_events` or at the start of the next tick —
   choose: clear in `Sim::tick` start after copying to `TeamData` damaged sets).

**Tests.** `bullet_hit_emits_build_damage_event` (dagger shooting a wall: after 300 ticks the
drained list is non-empty, entries have the wall tile and team 1; a second drain is empty).

**Verify in-engine.** Ground Zero: spawn team-2 daggers near a player wall, step 300,
`$G --eval "return $H.drain_build_damage_json()"` -> non-empty.

**Commit.** `Gameplay G01 T15: emit BuildDamageEvent and expose a drain seam`

---

### T16 — Attack-target tracking for commanded units + resolver seam

**Symptom / acceptance.** Ordering units to attack a moving enemy unit sends them to where the
target *was*; the HUD cannot draw the attack reticle (`AttackTarget` holds an id). After: a
commanded dagger follows a moving target dagger and kills it; `SimHost.command_targets_json()`
returns `[{"id":seq,"x":wx,"y":wy,"attack":true}]` for units with an attack target.

**Java.** `CommandAI.commandTarget(Teamc moveTo, boolean stopAtTarget)` sets `attackTarget`;
`defaultBehavior` (`CommandAI.java:212-246`): invalid attack target -> `attackTarget = null; targetPos = null`;
with one: `targetPos.set(attackTarget)` every tick (dummy -> its building); grounded vs solid
building target -> `findClosestEdge`. `engageRange = type.range - 10`; within it the unit stops
(`withinAttackRange`).

**Port today.** `apply_live_unit_command` (`core/sim/mod.rs:389-497`) converts
`CommandTarget::Unit(id)` to a fixed tile. `ControllerSlot.target: Option<TilePos>`.

**Changes.** Component `AttackTargetComp(Option<Entity>)`; in `apply_live_unit_command` for
`CommandTarget::Unit/Building` store the entity (resolve building by tile via `grid.tiles.getn(x,y).build`);
RT Command branch: if the attack target is dead/despawned -> clear both; else refresh
`slot.target` to the target's current tile each tick and stop moving within `def.range - 10`
(still facing/shooting). Seam: `Sim::command_targets() -> Vec<(i32, f32, f32, bool)>` (seq,
target pos, is attack) for units with `slot.target` or an attack target; gdext
`#[func] command_targets_json()`.

**Tests.** `commanded_dagger_chases_moving_target` (target dagger with a move order walking away;
attacker follows; target dies within 1500 ticks). `command_target_seam_reports_position`.

**Verify in-engine.** G03 draws; here an eval of `command_targets_json()` after a command.

**Commit.** `Gameplay G01 T16: commanded units track attack targets; command target seam`

---

### T17 — PAR-0156: estimateStats turret DPS, prebuildAi core defenders, aiCoreSpawn, fillItems

**Symptom / acceptance.** RtsAI squads ignore enemy turrets when weighing an attack (they attack
into turret lines); `prebuildAi` teams never get core defenders; `aiCoreSpawn` teams never spawn
core units; `fillItems` / BaseBuilderAI core fill never happen.

**Java.**
* `RtsAI.estimateStats` (`J/ai/RtsAI.java:329-363`): turrets: for each enemy building with flag
  `turret` that is a `Turret` with `(targetAir && air) || (targetGround && !air)` and
  `distanceSegmentPoint(from, to, turret) <= t.range() + 50`: `health += t.health; dps += t.estimateDps()`.
  Units: `Units.nearbyEnemies(team, rect(from..to normalized, grown by 280 i.e. 140 each side))`
  with `distanceSegmentPoint <= other.range() + 50`: `health += other.health; dps += other.type.dpsEstimate`
  (**no** air/ground filter for units).
* `Turret.estimateDps` (`Turret.java:335-338`): `hasAmmo ? shoot.shots / reload * 60 * peekAmmo().estimateDPS() * potentialEfficiency * timeScale : 0`;
  `ContinuousTurret` (`:43-46`): `hasAmmo ? shootType.damage * 60 / (ContinuousBulletType c ? c.damageInterval : 5) : 0`;
  `TractorBeamTurret` (`:150-153`): `any && damage > 0 ? damage * 60 * efficiency * coolantMultiplier : 0`; others 0.
  `BulletType.estimateDPS` (`BulletType.java:422-440`): `spawnUnit ? spawnUnit.estimateDps() : despawnUnit ? … : (damage + splashDamage*0.75) * (pierce ? (pierceCap == -1 ? 2 : clamp(pierceCap,1,2)) : 1) + frag.estimateDPS()*fragBullets/2 + sum(spawnBullets)`.
* `Logic.java:573-585` prebuildAi: per active team with `rules.prebuildAi` (not editor): per core,
  if no unit of `core.unitType` has `flag == core.pos()`, spawn one at the core with `flag = core.pos()`.
* `BaseBuilderAI.update` (`:51-71`): fill the first core with `getMaximumAccepted` of every item;
  `aiCoreSpawn && timer(360) && hasCore`: if `countType(core.unitType) < cores.size * 2` spawn one
  at a random core.
* `Logic.java:552-559` `fillItems`: first core gets every item on the planet that is not hidden.

**Port today.** `GRT::team_ai_enemy_stats` (~1237, units only, filtered by targets in
`rts_ai.rs:402 estimate_stats`); `TeamAiBuildingRow` (GRT ~720); turret state
`TurretState.config: Arc<TurretConfig>` (`reload`, `shoot.shots`, `range`, `target_air`,
`target_ground`, `kind`), `turrets::peek_ammo(world, e) -> Option<BulletId>`, `has_ammo`
(`core/world/blocks/defense/turrets/mod.rs:936,969`); `Building.potential_efficiency`,
`time_scale` (`entities/comp/building.rs:43,50`). Bullet DPS is computed only inside
`UDEF::link` into a local Vec. `RtsLiveUnit.flag` is always 0 (no unit flag field).
`BaseBuildActions.fill_core_items/spawn_core_unit` ignored (`GRT::run_team_base_builder`,
`core_unit_count: 0`). `TeamRule.prebuild_ai`, `fill_items`, `ai_core_spawn` exist (`core/io/json/rules.rs`).

**Changes.**
1. `pub fn bullet_estimate_dps(content, id) -> f32` in `BDEF` (recursive, memo-free is fine;
   spawn_unit -> `content.unit(id).dps_estimate`), and make `UDEF::link` use it.
2. `EnemyStat`: add `is_turret: bool`; `estimate_stats`: apply the air/ground filter only when
   `is_turret`; for units also require the point inside the from->to AABB grown by 140 per side.
3. `collect_team_ai_rows`: turret rows (entities with `TurretState`): `range = config.range`,
   `health`, `dps` by kind per Java (`Item/Liquid/Power/Laser` = Turret formula; `Continuous` =
   ContinuousTurret; `TractorBeam` = its formula using `config.beam_damage`; others 0).
   `team_ai_enemy_stats` appends them.
4. Unit flag: add `pub flag: f64` to `UnitCore` (Java `unit.flag`, also used by logic `flag`;
   check `grep -rn "flag" core/logic/executor/unit_control.rs` — if logic already stores a flag
   elsewhere, reuse it) and feed it into `RtsLiveUnit.flag`. Save: not in this task (G04 §6).
5. `team_ai_system`: per active team: `fill_items` -> fill first core; `rule.build_ai` path apply
   `actions.fill_core_items` (first core, every item to its maximum accepted) and
   `actions.spawn_core_unit` (spawn `core_unit` of the core block — T8 field — at a random core
   via `ai_rng`), passing the real `core_unit_count` (count units of that type on the team);
   `rule.prebuild_ai` -> per core without a unit of `core_unit` whose `flag == core.pos().pack()`
   spawn one with that flag.

**Tests.** `enemy_stats_include_loaded_turrets` (duo with copper -> one row, dps =
`1 shot / 20 reload * 60 * copper bullet dps(9 + 0)… ` — compute from content in the test:
`shots / reload * 60 * bullet_estimate_dps(peek)`; empty duo -> 0). `prebuild_ai_spawns_one_defender_per_core`
(two system runs -> exactly one unit with the flag). `ai_core_spawn_caps_at_two_per_core`.
`estimate_stats_units_ignore_air_filter`.

**Verify in-engine.** Attack sector 225 (fresh-sector recipe, env §4.1): after 1200 steps
`unit_debug_json()` lists enemy core units (if the sector's rules enable `aiCoreSpawn`;
UNVERIFIED — check `get_sector_state()`/rules first).

**Commit.** `Fixes PAR-0156: turret DPS in estimateStats, prebuildAi defenders, aiCoreSpawn and fillItems`

---

### T18 — Remaining AI kinds (faithful ports + live dispatch)

All three slices: add a branch in RT `unit_update_system` building an `AiCtx` like the Missile
branch, with per-unit state in a component inserted lazily; follow the Java text exactly.

Do **not** add an `AiKind::Logic` branch in T18a/T18c (G06 T9 owns it).

**T18a — DefenderAI (oct), FlyingFollowAI (quell, disrupt), HugAI (renale, latum).**
* `DefenderAI` (`J/ai/types/DefenderAI.java:13-45`): `updateMovement`: `unloadPayloads(); if(target != null){ moveTo(target, (target hitSize/2*1.1 or 0) + unit.hitSize/2 + 15, 50); lookAt(target); }`;
  `updateTargeting`: `if(retarget()) target = findTarget(x, y, range, true, true)`; `findTarget`:
  closest **friendly** unit within `max(range, 400)` (`!dead && type != unit.type && targetable && playerControllable`)
  minimising `-maxHealth + dst2/6400`; else own closest core; else (wave team on wave maps) enemy core.
  The port's `defender.rs` targets hostiles — replace it.
* `FlyingFollowAI` (`FlyingFollowAI.java:10-60`): follows the best ally (`!dead && controller not FlyingFollowAI && type != unit.type`,
  same min key, radius `max(range, 400)`, re-chosen every 30 ticks); `moveTo(following, …+15, 50)` or
  `moveTo(target, 80)`; `useFallback() = rules.pvp || rules.waveTeam != unit.team` -> FlyingAI.
* `HugAI` (`HugAI.java:15-61`): core override at `range/1.1 + size*4`; drop-zone stop for the
  default team (`dropZoneRadius + 120`); if target within range and the tile raycast is clear of
  impassable tiles (d4c around each step): within `(hit + targetHit)/2` -> circle
  (`movePref(vec(target-unit).rotate(90).setLength(speed))`) else straight; else `pathfind(fieldCore)`;
  boosting land; `faceTarget()`.
  Tests: `oct_follows_healthiest_ally`, `quell_follows_ally_on_wave_team`, `renale_hugs_core`.
  Commit: `Gameplay G01 T18a: DefenderAI, FlyingFollowAI and HugAI live`.

**T18b — LogicAI: owned by G06 T9 — do not implement here.** (G06 adds the live `AiKind::Logic` branch with ubind/ucontrol/ulocate.)

**T18c — CargoAI (manifold) and AssemblerAI (assembly-drone).** Port audit only: read
`J/ai/types/CargoAI.java` (179 lines) and `AssemblerAI.java` against `ai/types/cargo.rs`/`assembler.rs`,
fix deviations, dispatch. Needs the unit cargo loader/unload point blocks (G02) to be useful;
if those blocks lack a live state, dispatch only and record. Commit:
`Gameplay G01 T18c: CargoAI and AssemblerAI live`.

---

### T19 — Boss list (`Teams.bosses`, `state.boss()`) + `boss_json` seam

**Symptom / acceptance.** No sim notion of wave bosses: the HUD boss bar (G03 T9) and the
`guardian` hint (G05 hints) have no data. After: a wave-team dagger with the `boss` status makes
`SimHost.boss_json()` return `{"count":1,"names":["dagger"],"value":1.0,"first":{"id":seq,"type":"dagger","armor":0,"flying":false,"health":H,"max_health":H}}`;
with no boss: `{"count":0,"names":[],"value":0.0,"first":null}`.

**Java.** `Teams.updateTeamStats` (`J/game/Teams.java:213-223`): `bosses` cleared each update, then
every unit with `unit.team == rules.waveTeam && unit.isBoss()` is added in `Groups.unit` order;
`isBoss() = hasEffect(StatusEffects.boss)` (`StatusComp.java:107`); `GameState.boss() = teams.bosses.firstOpt()`
(`GameState.java:56`). HUD (`HudFragment`): text = up to 6 boss unit icons, value =
`Σhealth / Σmaxhealth` over `bosses`. Guardian hint reads `boss.armor`, `boss.isFlying()`.

**Port today.** No boss list (`grep -rn "bosses" core` = 0). Wave groups apply the `boss` status via
`SpawnGroup.create_unit` (`core/game/spawn_group.rs:136`); statuses in `StatusComp.statuses`.

**Changes.**
1. `Sim::bosses(&self) -> Vec<Entity>`: live `UnitRuntime.units` in `EntitySeq` order whose
   `TeamComp.team == wave team` (`CampaignRuntime.session.wave_team()`, else `UnitRuntime.wave_team`),
   not dead, with a `StatusComp` entry of `content.status_by_name("boss")`.
2. `#[func] SimHost.boss_json() -> GString` with the shape above: `names` = type names of the first
   6 bosses, `value` = Σhealth/Σmax (0.0 when Σmax is 0), `first` = `state.boss()` with
   `armor` = `def.armor` (status `armor_override` when ≥ 0), `flying` = `unit_is_flying`.
   This replaces G03 T9's planned gdext-side `boss_status_json` computation: G03 reads
   `count/names/value` from `boss_json()` (or keeps its `boss_status_json` name as a thin wrapper
   calling `Sim::bosses`); G05 reads `first` for the guardian hint (`boss != null && armor >= 4 && !flying`).
3. No caching needed (O(units)); callers poll at most every 10 frames.

**Tests.** `boss_list_tracks_wave_boss` (campaign session wave team 2; dagger team 2 with
`apply_status(boss, 999999)` -> `bosses().len() == 1`; team-1 boss-status unit not listed; after
the dagger dies -> empty).

**Verify in-engine.** Ground Zero: spawn a team-2 dagger (`spawn_unit`); no apply-status seam
exists, so add `#[func] SimHost.debug_apply_status(seq: i64, status: GString, ticks: f64) -> bool`
(debug seam, like `debug_add_core_items`); then `return $H.boss_json()` shows count 1.

**Commit.** `Gameplay G01 T19: boss list (Teams.bosses) and boss_json seam`

---

### Deferred (not in this round; why)

* `PrebuildAI` behaviour (`J/ai/types/PrebuildAI.java`, experimental upstream, only for
  `prebuildAi` teams; T17 spawns the defenders, they run BuilderAI).
* Enemy base plans built by builder units instead of the instant `AiPlaceRequest` placement
  (`GRT::run_team_base_builder`): needs T10c + T17 first; the instant placement is a known
  deviation (log it in the summary).
* `randomWaveAI` campaign rule (default off): EnemyCoreField random targets + FlyingAI branch.
* Stances `patrol`, `ram`; `UnitGroup` formations (prior T18); `enterPayload/loadUnits/loadBlocks/
  unloadPayload/loopPayload` command behaviours (payload commands: Erekir carrier gameplay; next round).
* Dead flying unit fall (`UnitComp.update` "simulate falling down"; deaths are instant today).
* Delayed explosion waves/lightning (`Time.run`) — immediate in the port.
* `isFlying() && !canLand()` boost term (needs `Units.any`), minor.
* `turret-unit-build-tower` unit type (build tower turret).

---
## §3 Execution order, dependencies, shared files

Order (by player impact, respecting dependencies):

1. **T1** (coordinator request; makes `mind-headless` green again).
2. **T2** -> **T3** (T3's live tests need correct ranges) -> **T4** (T3's line-hit helper calls T4's `hit_tile` for friendly buildings; if T4 is not merged yet, T3 skips friendly buildings and T4 adds the call).
3. **T5**.
4. **T8** -> **T9** (T9 reuses `spawn_player_at_best_core`).
5. **T6a** -> **T6b** -> **T7** (T7's tests observe T6a movement).
6. **T10a** -> **T10b** (needs T4 for heals) -> **T10c** -> **T11** (stances feed T10 AIs).
7. **T12**, **T13**, **T14**, **T15**, **T16** (independent; any order).
8. **T17** (needs T8's `BlockDef.core_unit`).
9. **T18a**, **T18c** (T18b is G06 T9's), then **T19** (boss seam; before G03 T9 / G05 hints if possible).

After every task: `$L cargo test -p mind-core --lib --features msav-import` (previous count + new
tests, 0 failed), `$L cargo test -p mind-headless --lib` (only the G00 known-failure list may
fail; after T1 `units_scenarios::tests::committed_goldens_match` must pass), clippy clean,
`cargo fmt`. After T3, T6a, T7, T8, T10a rebuild + `tools/session-play.sh N --import` and re-run
the Ground Zero wave probe (prior plan `audit-combat-units.md` §6): the wave dies, `hasCore` true,
`grep -c panic` of the newest log = 0.

Files shared with other areas (pull `main` before touching; keep edits minimal and cite the task):
* `core/content/registries/blocks/{mod.rs,storage.rs,environment.rs}` (G02): new fields
  `core_unit` (T8), `floor_status/_duration/floor_drag_multiplier/floor_damage_taken` (T13),
  `base_explosiveness/flammability_scale/explosiveness_scale` (T14).
* `core/world/blocks/units/behavior.rs` (G02): unit-cap gate (T12), command source unification (T10).
* `core/sim/mod.rs` (`Sim::tick`, `apply_live_unit_command`, SimCommand dispatch): T8, T9, T10, T11, T15, T16.
* `core/sim/builder.rs` (G08 uses the player plan queue): T10c generalises `update_builders`.
* `core/game/play.rs` (G04): one field for the spawn multiplier (T7).
* `core/game/runtime.rs` (G04 campaign systems live here too): T5 `rules_of`, T17.
* `gdext/sim_host.rs` (everyone): T8 (`spawn_player_unit` body), T9/T11 JSON ops, T15/T16 `#[func]`s.
* Goldens: `parity/golden_content.json` (T2), `client/rust/mind-headless/tests/golden/units/*.json`
  (T1, maybe T7), `client/rust/mind-core/tests/golden/combat.json` (maybe T2/T3) — always with the
  manifest sha and the Java reason in the commit body.

### 3.1 Seams other areas call (exact signatures)

For **G08** (input; G08 T12 calls these, G01 provides the sim behaviour):
* Possession: emit `SimCommand::UnitControl { unit: Some(seq) }` (existing variant, `RemoteAction::UnitControl`);
  sim handler `Sim::possess_unit(&mut self, seq: i32) -> bool` (T9). JSON/MCP: `{"op":"unit_control","unit":seq}`.
* Respawn / leave unit (`Binding.respawn`, today only recenters the camera at
  `gdext/input/desktop.rs:496`): emit `SimCommand::UnitClear` -> `Sim::clear_unit(&mut self) -> bool` (T9). JSON: `{"op":"unit_clear"}`.
* Automatic respawn after death needs no input: `Sim::update_player_respawn` (T8, 60 ticks).
  Read-back: existing `#[func] SimHost.has_player_unit() -> bool`; new
  `#[func] SimHost.player_respawn_ticks() -> f64` (remaining ticks; add in T8 for the HUD).
* Stances: existing `#[func] MindInput.set_unit_stance(stance: i64, enabled: bool)` ->
  `RemoteAction::SetUnitStance` -> `SimCommand::UnitStance { units, stance, enabled }`; T11 makes
  `Sim::apply_live_unit_stance(&mut self, ids: &[i32], stance: u16, enabled: bool) -> usize` handle it for live units.
* Commands: existing `SimCommand::UnitCommand`; T10 makes non-move commands act.
For **G03 / G05** (boss): `#[func] SimHost.boss_json() -> GString`
  `{"count":n,"names":[type…≤6],"value":f,"first":{"id":seq,"type":s,"armor":f,"flying":b,"health":f,"max_health":f}|null}` (T19);
  core `Sim::bosses(&self) -> Vec<Entity>`. G03 T9 boss bar uses count/names/value; G05 `guardian` hint uses `first`.
For **G03** (drawing):
* `#[func] SimHost.drain_build_damage_json() -> GString` `[[tile_x, tile_y, team], …]` (T15) -> `MindMinimap.add_indicator`.
* `#[func] SimHost.command_targets_json() -> GString` `[{"id":seq,"x":wx,"y":wy,"attack":bool}]` (T16) -> `Drawf.target` reticle.

---

## §4 Ledger changes to report (do not edit `chains.json`)

Status of the items given to G01 in the audited tree (main + sessions 2-5, 2026-10-13):

| Item | Now | Proposed | Evidence / note |
|---|---|---|---|
| PAR-0088 | fixing | **fixed (headless)** | spawnUnit path + MissileAI live, four scathe missiles + carrier ammo (`content/registries/units/special.rs:43 scathe_missiles`, `advanced.rs` scathe ammo), tests `spawn_unit_bullet_spawns_missile`, `scathe_launches_missile_and_kills`. In-engine scathe check still blocked on item-source config (G02); quell/disrupt/anthicus missiles verified earlier. |
| PAR-0091 | fixing | **fixed** for its title (puddles/fires update live, building explosions) | residuals (unit death explosion, ExplosionShield absorption, fire env gating, block explosiveness fields) move to a new item handled by T14. |
| PAR-0137 | fixing | **fixed** | floor speed + boosting mechs live (`movement.rs` `unit_speed`, `update_kinematics` boosting), tests `dagger_slowed_on_deep_water`, `boosting_nova_lifts_off`; `canLand` term deferred (minor). |
| PAR-0138 | fixing | **fixed** | unit + building payload pickup/drop, key path, unit save revision 2 (prior T4); tests `mega_picks_up_and_drops_*`. |
| PAR-0140 | verified | (unchanged) | live stances are a separate new item below. |
| PAR-0156 | seeded | **fixing** -> T17 | |
| PAR-0126 | fixing | not G01 (render) | |

New items to file (title -> task):
* "Line weapons (laser, continuous, shrapnel, rail, sap) never damage units; rail bullets deal no damage" -> T3.
* "Heal bullets (nova/pulsar/poly/mega) damage the player's own buildings instead of healing them" -> T4.
* "Units with laser/rail/sap weapons have range -4 (quasar, pulsar, corvus, omura, locus, spiroct, arkyid, toxopid, eclipse…) and never engage as AI" -> T2.
* "Wave ground units walk to the nearest building instead of following the enemy-core flowfield" -> T6a; "Flying wave units ignore targetFlags" -> T6b.
* "Wave spawner: groups split across spawns, flyers spawn on ground spawns, no drop-zone shockwave, no spawn invincibility, difficulty spawn multiplier ignored" -> T7.
* "Player unit never respawns after death; core unit type ignored (always alpha)" -> T8.
* "Unit possession / unit clear do nothing in the simulation" -> T9.
* "Mono/poly/mega idle: command controllers (mine/repair/rebuild/assist) not run; AI units cannot mine" -> T10.
* "Unit stances never reach live units" -> T11.
* "Unit cap is never enforced" -> T12.
* "Units never drown, get no floor status (wet/tarred/melting), survive off-map and on solid tiles" -> T13.
* "Units carrying explosive items do not explode on death; force projectors never absorb explosions; fires ignore rules.fire/oxygen" -> T14.
* "BuildDamageEvent never emitted (no minimap attack indicators, RtsAI damaged set approximated)" -> T15.
* "Commanded units do not follow moving attack targets" -> T16.
* "Team rule multipliers (unit/block damage/health, mine speed) ignored by combat; campaign difficulty has no effect on enemy durability" -> T5.
* "BaseBuilderAI scatter regression (units_base_build golden): pixel scatter introduced by PAR-0139 round 2" -> T1 (note on PAR-0139, which is verified).
* "DefenderAI/FlyingFollowAI/HugAI/CargoAI/AssemblerAI not dispatched live" -> T18 (LogicAI -> G06 T9).
* "No boss list (`state.boss()`/`teams.bosses`): HUD boss bar and guardian hint have no data" -> T19.

---

## §5 Done criteria

* Every task in §3 committed with the subject given, or reported as blocked with the observed
  reason in `docs/parity-plans/G01-units-ai-combat_summary.md` (env §6 shape).
* `$L cargo test -p mind-core --lib --features msav-import`: 0 failed, count = baseline + new tests.
* `$L cargo test -p mind-headless --lib`: only the G00 known failures remain; `units_base_build`
  passes (T1).
* The compare script (top of this file) prints only `MISSING turret-unit-build-tower` (T2).
* clippy clean, fmt applied, GDScript parse gate clean (only if `.gd` touched — G01 should not).
* In-engine: Ground Zero wave probe passes after T3/T6a/T7/T8/T10a; T8 respawn and T10a mono
  evidence saved under `runs/<date>-s<N>-G01-*`; `grep -c panic` of every run log = 0.
* Summary lists the §4 ledger changes and every deviation (Java behaviour not reproduced) with file:line.

---

## §6 Cross-area findings

1. **G04 (saves)** — prior plan T13: `core/io/sim_io.rs` `LiveEntitySource::entity_count` counts
   `unit_count()` while `write_entities` skips units missing `UnitCore/UnitTypeComp/Pos`; build the
   list once. New unit state from this plan (`MinerComp.mine_timer`, drown time, stances, attack
   target, unit flag, `DockedType`, AI states) is not saved; Java saves `flag`, `mineTile`,
   `stances`/`command` (CommandAI `keepState`), `plans`.
2. **G04** — captured-sector flag and `Team.isOnlyAI()` (players on team) needed by T13's
   "AI units die on captured sectors"; T13's limit-area clamp depends on **G04 T5** setting
   `WorldGrid.limit_area`.
3. **G02** — `Building.unitOn/unitOnAny` hooks (conveyors push units, `UnitComp.java:837-841`) have
   no call site; T13 leaves a marked call site. `Wall.handleDamage`/deflect (phase/plastanium walls)
   not audited here. Unit factory payload dump + command config (`UnitFactory.java:425-433`) — the
   port spawns immediately with `None` config. Item-source configuration from the bridge (`audit-combat-units.md`
   T19) blocks in-engine turret ammo tests (scathe, T3 lancer).
4. **G02** — force projector stays at efficiency 0 with power sources (rendering summary
   blocker 2); T14's absorption test depends on a working projector.
5. **G03** — draw T15 indicators and T16 reticles; status effect particles (`StatusEffect.effect`)
   and floor `walkEffect`/`unitLand` FX are view-side.
6. **G08** — `Binding.respawn` only recenters the camera (`gdext/input/desktop.rs:496`): must emit
   `UnitClear` (§3.1); Ctrl-click possession must emit `UnitControl`.
7. **G05** — (folded into T8 step 6: `spawnType.unlock()` on player spawn.)
8. **G06** — `LogicAi` is installed by `ucontrol` but never updated: owned by G06 T9 (live
   `AiKind::Logic` branch). The unit `flag` (T17 adds `UnitCore.flag`) should back `ucontrol flag`/`sensor @flag`.
9. **Performance (all)** — the flowfield cache key is `(team, cost)` only
   (`core/ai/pathfinder/mod.rs:103-127`), so any code that requests per-unit target sets rebuilds
   the field every call; T6a fixes the main offender, other callers (`command.rs` move orders,
   base builder enemy field) should use stable target sets.
