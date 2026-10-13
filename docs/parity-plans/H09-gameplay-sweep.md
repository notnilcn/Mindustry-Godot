<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H09 — uncovered-gameplay sweep (fog of war sim, allowUpdate, fires/puddles, destruction explosions, team fill, core incineration, game stats)

Written 2026-10-14 against `main` @ `ffbe84dd` (+ H00, which this plan assumes landed).

**Read `docs/H-environment.md` first, fully** (paths `REPO`/`WT`/`core/`/`gdext/`/`J/`,
session/worktree/build rules, RAM rules, gmcp eval rules, commit rules). Then read
`docs/H-INDEX.md` §3 if it exists. Then this file.

Start state:
```bash
cd WT && git merge --ff-only main
git log main --oneline | grep -m1 'H00'      # must print a commit; if empty: stop, H00 has not landed
git merge-base --is-ancestor $(git log main --oneline | grep -m1 'H00' | cut -d' ' -f1) HEAD && echo H00-IN
```
After H00: `JSON.parse_string($MC.get_rules_json()).env` on Ground Zero is `233`, and a
`"player":0` placement queues a build plan instead of finishing instantly.

**Drift rule.** Java line numbers were checked 2026-10-14. Port line numbers drift: when a
quoted port line moved, `grep -n` for the quoted function/identifier. If the function no
longer exists, or the code already does what a task asks, stop that task, write down what
you saw in your summary, and go to the next task. Never guess a seam: `grep -n "fn <name>"`
and confirm `#[func]` directly above it before calling it from gmcp.

Commit subjects: `Gameplay H09 T<n>: <summary>` or `Fixes PAR-XXXX: <summary>` where the
task names a ledger id. One task = one commit.

---

## 1. Inventory

### 1.1 Systems (Java -> port -> status)

Status legend: **faithful** (no task), **H0x** (missing/wrong, owned by that area plan;
forwarded in §6), **T<n>** (task in this plan).

| System | Java | Port | Status |
|---|---|---|---|
| Fog of war simulation (`FogControl.update`, dynamic + static buffers, threads) | `J/game/FogControl.java:44-430`, called from `J/core/Logic.java:533-535` | `core/game/fog.rs` (688 lines) exists but has **no live owner**: no `Sim` field/resource, never called from `Sim::tick`; `update()` only redraws the dynamic buffer once (`just_loaded`), flush interval 40 ticks instead of 40 ms; `gdext/campaign.rs` `fog_query` is a placeholder (`"discovered": false`) | **T1** |
| Fog gates in gameplay (`Units.closestEnemy/bestEnemy` `inFogTo`, `BlockIndexer.findEnemyTile` `isDiscovered`, `Build.validPlace` static fog) | `J/entities/Units.java:304,326`; `J/ai/BlockIndexer.java:468`; `J/world/Build.java:252`; `J/entities/comp/UnitComp.java:232-248`; `J/entities/comp/BuildingComp.java:315-333,2231-2249` | `core/combat/targeting.rs` `closest_unit_filtered`, `core/ai/block_indexer.rs` `find_enemy_tile`/`find_enemy_building`, `core/world/build.rs` `valid_place_at`: no fog checks at all | **T2** |
| Fog sources: radar progress, turret/core fog radius, `forceUpdate` | `J/world/blocks/defense/Radar.java:52-73`; `J/world/blocks/defense/turrets/BaseTurret.java:59`, `Turret.java:426-428,555-561`; `J/world/blocks/storage/CoreBlock.java:158-159` | `core/world/behavior/defense.rs` `RadarBehavior::update_tile` is empty; `RadarState` has only `progress`; turrets/cores never get `BlockFlag::HasFogRadius` (content has no turret range; `init_self` only flags explicit `fog_radius`) | **T3** |
| `BuildingComp.allowUpdate/checkAllowUpdate` (derelict, env, map-area disable) + `World.checkMapArea` | `J/entities/comp/BuildingComp.java:627-632,2261-2265,136,1842`; `J/core/World.java:44-52,469-481` | `core/world/hooks.rs:68` `check_allow_update` is an empty default; nothing ever sets `Building.enabled = false`; turrets (`core/world/blocks/defense/turrets/mod.rs:1348 update_turrets`) ignore `enabled` | **T4** |
| Fires (`FireComp.update`, `Tile.getFlammability`) | `J/entities/comp/FireComp.java:45-115`; `J/world/Tile.java:142-160` | `core/combat/fires.rs:153-272`: no lifetime growth/decay terms, no fireballs (timer counts but never fires), no unit damage/burning, building damage bypasses `BuildingComp.damage`; `core/world/tile.rs:99-109` returns 0 for every building | **T5** |
| Puddles (`PuddleComp.update`, `Puddles.deposit`, `CellLiquid.update`, `onDeconstructed` liquid drop) | `J/entities/comp/PuddleComp.java:46-113`; `J/entities/Puddles.java:36-130`; `J/type/CellLiquid.java:36-95`; `J/entities/comp/BuildingComp.java:1480-1487`, `J/world/Build.java:52` | `core/combat/puddles.rs`: `update_time` decremented twice per tick, same-liquid deposit bypasses `accepting`, spread counts buildings instead of `block() == air`, no reaction fireball, no `liquid.update` (neoplasm spread), `on_deconstructed` never called (`core/world/behavior/mod.rs:224`) | **T6** (Fixes PAR-0091) |
| Destroyed-building explosion + `Damage.dynamicExplosion` | `J/entities/comp/BuildingComp.java:1463-1518`; `J/world/blocks/storage/StorageBlock.java:99-103`; `J/entities/Damage.java:93-133` | `core/sim/unit_runtime.rs` `building_explosion_terms` (~1715) misses `baseExplosiveness`, `explosionItemCap` (linked storage), `* 3.5 * explosivenessScale`, `flammabilityScale`; destruction call (~774) passes `damage=true` (rule ignored there is OK, it is inside `if damage_explosions`) but no scales; `core/combat/damage/explosion.rs` has no lightning, no fireballs (one `fires::create`), no wave stagger, shield check even with `ignore_team == None` | **T7** |
| `TeamRule.fillItems` (AI cores refilled every tick) | `J/core/Logic.java:552-561`, set at `Logic.java:110-115` | set by `core/game/play.rs:551-555`, **never consumed** | **T8** |
| `CoreBlock.incinerateNonBuildable` + core intake accounting | `J/world/blocks/storage/CoreBlock.java:54,785-799,884-909`; `J/content/Blocks.java` core-bastion/citadel/acropolis | field absent; `core/world/blocks/storage/core_block.rs:249-305` stores sand/coal/etc in Erekir cores and skips stats/sector accounting when the core is full | **T9** |
| `GameStats` + `CampaignStats` event counters | `J/core/Logic.java:52-60,186-235,436` | only `placedBlockCount` (`sim/mod.rs:1790`), `coreItemCount`, `enemyUnitsDestroyed`, planet `wavesLasted` move; `buildingsBuilt/Deconstructed/Destroyed`, `destroyedBlockCount`, `unitsCreated` and every planet `CampaignStats` map except `waves_lasted` stay 0 | **T10** |
| `static-fog-data` save chunk + building `visibleFlags` in saves | `J/game/FogControl.java:437-505`; `J/entities/comp/BuildingComp.java:187,216,270` | `FogControl::write/read` exist, no `CustomChunk` registered; `core/world/map_source.rs:110` passes `fog=false` | **T11** |
| `ConstructBlock` progress/accumulator/refund (`buildCostMultiplier`, `deconstructRefundMultiplier`) | `J/world/blocks/ConstructBlock.java:290-400` | `core/world/construct.rs:141-320` | faithful |
| `StatusEffects` transitions/affinities/opposites (wet+shocked etc.) | `J/type/StatusEffect.java`, `J/content/StatusEffects.java` | `core/combat/damage/status.rs:14-170` | faithful (H01 owns combat use) |
| Rules team multipliers (`unitHealth`, `blockHealth`, `unitDamage`, `blockDamage`, `buildSpeed`, `unitBuildSpeed`, `unitCost`, `unitMineSpeed`, `unitCrashDamage`) | `J/game/Rules.java:286-333` | `core/game/rules.rs:155-240`, consumed in `combat/damage/area.rs:262-282`, `combat/bullet/spawn.rs:126-131`, `world/blocks/units/behavior.rs:405-413`, `entities/comp/unit/miner.rs:166`, `sim/builder.rs:211` | faithful |
| `TeamData.present/units/buildings` (`Teams.updateTeamStats`) | `J/game/Teams.java:181-240` | `core/game/teams.rs:423 update_team_stats` is only called from tests; only cores are registered live | not tasked (fog T1 computes its team list from the ECS; noted for H01/H04 in §6) |
| `Pathfinder` cost types (naval/legs/hover) | `J/ai/Pathfinder.java` | `core/ai/pathfinder/cost.rs` | H01 |
| Unit drag physics (`rules.dragMultiplier`, `floor.dragMultiplier`) | `J/entities/comp/UnitComp.java:790` | velocity never integrates drag (`core/entities/comp/unit/environment.rs:10-11` says so) | H01 |
| Heat network, UnitAssembler, payload units into blocks | `J/world/blocks/heat/*`, `J/world/blocks/units/UnitAssembler.java` | `core/world/blocks/heat`, `.../units` | H02 |

### 1.2 `Rules` field table (`J/game/Rules.java`)

"Consumed at" = first in-sector sim read site outside `io/json`, `editor/`, `ui/`, tests
(port paths under `core/`). Fields marked **n/a** have no campaign in-sector effect
(PvP/editor/multiplayer only) and get no task.

| Rules field (Java line) | Consumed at (port) | Status |
|---|---|---|
| allowEditRules (25) | game/rules_event.rs:97 | faithful |
| infiniteResources (27) | sim/builder.rs:209,223; maps/fix.rs:120 | faithful |
| coreBuildAndConfig (29) | — | n/a (default false; `CoreBlock.java:176-336` editor/custom) |
| teams (31) | game/rules.rs `team_rule` | see TeamRule table |
| waveTimer (33) / waves (37) / waveSpacing (153) / initialWaveSpacing (155) / winWave (157) | game/runtime.rs, game/play.rs:499 | faithful (H04) |
| waveSending (35) | — (HUD skip button) | **H03** (`HudFragment.java:1235`) |
| airUseSpawns (39) | sim/unit_runtime.rs:2773 | faithful |
| wavesSpawnAtCores (41) | sim/unit_runtime.rs:2806 | faithful |
| pvp (43) / pvpAutoPause (45) / cleanupDeadTeams (131) / showOtherTeamPings (99) | — | n/a |
| pauseDisabled (47) | only set (game/planet.rs:508) | **H08** (`HudFragment.java:409-417`, `Control.java:253,620`) |
| waitEnemies (49) | game/play.rs:535 | faithful |
| attackMode (51) | many | faithful |
| editor (53) | many | faithful |
| derelictRepair (55) | only set (game/planet.rs:392); `world/limits.rs:32` field never synced | **H02** (`Build.java:95,254`, `BuildPlan.java:78`, `BuilderComp.java:168`) |
| canGameOver (57) | game/runtime.rs:772 | faithful |
| coreCapture (59) | ai/block_indexer.rs:355 | faithful |
| reactorExplosions (61) | world/blocks/power/generator.rs:40 | faithful |
| possessionAllowed (63) | — | **H08** (`DesktopInput.java:423`, `InputHandler.java:749,779`) |
| schematicsAllowed (65) | — | **H08** (`SchematicsDialog.java:124,192`) |
| damageExplosions (67) | sim/unit_runtime.rs:439,586 | faithful (T7 keeps it) |
| fire (69) | combat/fires.rs `env_allows_fire` via `CombatEnv` | faithful; T6 adds the `CellLiquid.update` gate |
| randomWaveAI (71) | only set | **H01** (`Pathfinder.java:556`, `FlyingAI.java:49`) |
| unitPayloadUpdate (73) / unitPayloadsExplode (75) | — | **H01/H02** low (`BuildPayload.java:58`, `PayloadComp.java:71`; both default false) |
| unitCapVariable (77) / unitCap (159) / disableUnitCap (161) | entities/comp/unit/queries.rs:171-179 | faithful |
| hideSpawns (79) | only set | **H03** (`MinimapRenderer.java:296`) |
| solarMultiplier (81) | world/blocks/power/behavior.rs:577 | faithful |
| unitBuildSpeedMultiplier .. unitFactoryActivationDelay (83-95) | game/rules.rs accessors (see 1.1) | faithful |
| ghostBlocks (97) | sim/unit_runtime.rs:659 | faithful |
| logicUnitControl/Build/Deconstruct (101-105), worldProcessorPlayerLink (107), allowEditWorldProcessors (109), disableWorldProcessors (111), allowLogicData (251) | logic/* | faithful (H06) |
| blockHealthMultiplier .. buildSpeedMultiplier (113-119) | accessors | faithful |
| deconstructRefundMultiplier (121) | sim/builder.rs:214,230 | faithful |
| objectiveTimerMultiplier (123) | game/runtime.rs:565 | faithful |
| enemyCoreBuildRadius (125) | sim/unit_runtime.rs:689; game/rules.rs:160 `build_radius` (no caller) | H02 checks `Build.validPlace` core radius uses `buildRadius(team)` incl. `extraCoreBuildRadius` (`Teams.java:64`) |
| polygonCoreProtection (127) | — | **H02** (`Build.java:191`) |
| placeRangeCheck (129) | only set | **H02** (`Build.java:268`; generated Erekir sectors set it, `ErekirPlanetGenerator.java:425` -> H05 PAR-0112) |
| onlyDepositCore (133) | only set (game/planet.rs:403) | **H08** (`BuildingComp.java:745` `allowDeposit`; Erekir sets it) |
| allowCoreUnloaders (135) | world/blocks/storage/core_block.rs:331 | faithful |
| itemDepositCooldown (137) | — | **H08** (`InputHandler.java:2303-2313`) |
| coreDestroyClear (139) | sim/unit_runtime.rs:681 | faithful |
| hideBannedBlocks (141) | — | **H08** block picker (`Block.java:1032`) |
| allowEnvironmentDeconstruct (143) | world/build.rs:127 | faithful |
| instantBuild (145) | 11 sites | faithful |
| blockWhitelist/unitWhitelist (147-149) | game/rules.rs:231-236 | faithful |
| dropZoneRadius (151) | sim/unit_runtime.rs:1942,2854 | faithful |
| dragMultiplier (163) | — (no drag integration) | **H01** (`UnitComp.java:790`) |
| env (165) | H00 | H00 |
| attributes (167) | 34 sites | faithful |
| music fields (175-179, ambient/dark) | audio/* | faithful (H07) |
| spawns (181) / loadout (183) / weather (185) | many | faithful (H01/H04/H07) |
| blockLimits (187) | world/limits.rs:78 reads `BuildRules.block_limits`; `Rules.block_limits` never synced into it | **H02** (`Block.java:1046`) |
| bannedBlocks/bannedUnits/revealedBlocks/researched (189-195) | world/build.rs:185, maps/fix.rs, tech_tree.rs | faithful |
| objectives (197) / objectiveFlags (199) | game/map_objectives.rs | faithful (H04) |
| **fog (201)** | only set/printed | **T1, T2, T3, T11** |
| **staticFog (203)** | only set | **T1, T2, T11** |
| staticColor/dynamicColor (205-207) | render/rules.rs | H03 |
| lighting (209) / ambientLight (211) | render/rules.rs, game/runtime.rs:1111 | H03 |
| unitLight (213) | only logic setrule | **H03** (`UnitType.java:1801`) |
| defaultTeam (215) / waveTeam (217) | many | faithful |
| cloudColor (219) | — | **H03** (`CoreBlock.java:460-474`, `World.java:324`) |
| modeName (221) / mission (223) | ui/hud_text.rs | faithful |
| coreIncinerates (225) | world/blocks/storage/core_block.rs:228,239 | faithful (T9 adds incinerateNonBuildable) |
| borderDarkness (227) / limitMapArea + limitX/Y/W/H (229-231) | sim/mod.rs:1585-1591 | faithful |
| **disableOutsideArea (233)** | — | **T4** |
| tags (235) / planet (249) / sector | many | faithful |
| background* / customBackgroundCallback / planetBackground (237-247) | — | n/a for Serpulo/Erekir presets (asteroid/space maps) |

### 1.3 `TeamRule` fields (`J/game/Rules.java:351-403`)

| Field | Consumed at | Status |
|---|---|---|
| aiCoreSpawn | ai/base_builder_ai.rs:206 | faithful |
| protectCores / extraCoreBuildRadius | game/rules.rs:160-166 `build_radius` (no live caller) | H02 (placement core radius) |
| checkPlacement | only tests | **H02** (`Build.validPlace` uses `team.rules().checkPlacement`) |
| cheat | sim/builder.rs:223, world/update.rs:433 | faithful |
| **fillItems** | only set | **T8** |
| infiniteResources | game/rules.rs:226 `is_infinite_resources` | faithful |
| prebuildAi | — | **H01** (PAR-0156; `Logic.java:574-588`, `BuilderAI.java:242`, `BlockIndexer.java:123`) |
| buildAi / buildAiTier | game/runtime.rs:949, ai/base_builder_ai.rs:248 | faithful |
| rtsAi / rtsMinSquad / rtsMaxSquad / rtsMinWeight | game/runtime.rs:901-926 | faithful |
| unit*/block*/buildSpeed multipliers, unitFactoryActivationDelay | game/rules.rs accessors | faithful |

---

## 2. Tasks

General test helpers (all exist, confirm with grep before use): in `core/sim/combat_tests.rs`
`live_sim(size)` and `campaign_sim(size, |rules| ..)` (installs a `CampaignRuntime` with
`default_team = 1`); in `core/sim/production_tests.rs` `runtime_sim()`, `place(&mut sim, "name",
x, y)`, `item()`, `liquid()`, `stored_item()`, `install_campaign_session(&mut sim, rules)`.
`Sim::spawn_live_unit("dagger", team, px, py, rot)` spawns a live unit. A team counts as AI
(`Rules::team_is_ai`) only when `rules.waves || rules.attack_mode || is_campaign`: every fog
test must set `rules.waves = true` so team 2 is AI and team 1 is the player.

Put new tests in the file the task names, inside the existing `#[cfg(test)] mod tests` (or
in `core/sim/combat_tests.rs` for whole-`Sim` tests). Run the fast loop with
`$L cargo test -p mind-core --lib <filter>` and the full suite before committing (H-environment §2).

---

### T1 — Live FogControl: owned by the Sim, updated every tick, faithful dynamic flush

> **Coordinator addition (2026-10-14).** H03 T12 (fog rendering) reads two plain `pub fn` (not `#[func]`) seams on `MindSimHost` that this task must add in `gdext/sim_host.rs`, exactly:
> ```rust
> /// Discovered (static) bits for `team`, one byte per tile (0/255, row-major, y up), or None when fog is off.
> pub fn fog_static_bytes(&self, team: u8) -> Option<(i32, i32, Vec<u8>)>
> /// Dynamic vision sources for `team`: (world x, world y, radius in world px).
> pub fn fog_sources(&self, team: u8) -> Vec<(f32, f32, f32)>
> ```
> Implement them from the live `FogControl` resource and `collect_fog_sources` (radius = fog radius in tiles * 8). Add one test in `core/sim/fog.rs` for the Rust half they call.

> **In-engine verification (2026-10-14, session 2):** CONFIRMED. Onset (`erekir` 10) loads with `hasCore:true`, rules `fog:true, staticFog:true, env:17` (H00 Change 1 applied; player evoke at (2212,860), core-bastion footprint tiles 275-278 × 106-109, anchor (276,107)); `MindCampaign.fog_query(1,129,55)` on Ground Zero returns the placeholder `{discovered:false, fog:false, staticFog:false}`; no `FogControl` owner (`FogControl::new` only in `game/fog.rs`, no `Resource` derive at `fog.rs:156`); the constant is `rules.rs:38` (not 37). Java `fogRadius = max(58*3, hitSize*2)/8` confirmed (`UnitType.java:989-991`). Text fixed below: `best_core_position()` returns world **pixels** (Onset (2208,856)) — divide by 8 for `fog_debug_json`; probes at y ≥ 80 on Ground Zero are outside the playable `limitMapArea` (x 90-159, y 20-79) but fog is still computed there.

**Symptom / acceptance.** On Erekir (`rules.fog = true`, `rules.static_fog = true` from the
planet rule setter, `core/game/planet.rs:395-404`) the port has no fog state at all: nothing
is ever discovered or visible. After this task: a `FogControl` resource lives in
`sim.ecs.0`, is (re)initialised on every world load / rules apply, and `Sim::tick` calls
`FogControl::update` when `rules.fog`. A player-team unit with `fog_radius 10` standing at tile
(20,20) makes tile (25,20) visible within 3 ticks and keeps (20,20)-(30,20) discovered after
it moves away; tile (40,20) stays invisible and undiscovered.

**Java** (`J/game/FogControl.java`):
```java
// :23  private static final int dynamicUpdateInterval = 1000 / 25; //25 FPS   (40 ms)
// :46-61 WorldLoadEvent: stop(); loadedStatic=false; justLoaded=true; ww/wh;
//        if(fog && staticFog){ pushStaticBlocks(true); updateStatic(); loadedStatic = true; }
// :63-76 TileChangeEvent: if(fog && build != null && isCenter && !build.team.isOnlyAI() && flags.contains(hasFogRadius)){
//            data(team).dynamicUpdated = true; if(staticFog) pushEvent(FogEvent.get(x, y, round(build.fogRadius()), team), false); }
// :79-86 TilePreChangeEvent (tile removed): same guard -> data.dynamicUpdated = true
// :89-94 UnitDestroyEvent: if(fog && fog[team] != null) fog[team].dynamicUpdated = true
// :104-111 isDiscovered(team,x,y): if(!staticFog || !fog || team == null || team.isAI()) return true; ... data.get(x + y*ww)
// :117-123 isVisibleTile(team,x,y): if(!fog || team == null || team.isAI()) return true; data.read.get(clamp(x)+clamp(y)*ww)
// :229-257 update(): for(var team : state.teams.present){ if(!team.team.isOnlyAI()){
//     unitEventQueue.clear(); FogData data = fog[team.id] ?: new FogData();
//     for(var unit : team.units){ int tx = unit.tileX(), ty = unit.tileY(), pos = tx + ty * ww;
//        if(unit.type.fogRadius <= 0f) continue;
//        long event = FogEvent.get(tx, ty, (int)unit.type.fogRadius, team.team.id);
//        unitEventQueue.add(event);
//        if(unit.lastFogPos != pos){ pushEvent(event, false); unit.lastFogPos = pos; data.dynamicUpdated = true; } }
//     if(data.dynamicUpdated && Time.timeSinceMillis(data.lastDynamicMs) > dynamicUpdateInterval){
//        data.dynamicUpdated = false; data.lastDynamicMs = Time.millis();
//        for(var build : indexer.getFlagged(team.team, BlockFlag.hasFogRadius)) dynamicEventQueue.add(FogEvent.get(build.tile.x, build.tile.y, round(build.fogRadius()), build.team.id));
//        dynamicEventQueue.addAll(unitEventQueue); } } }
// :259-275 if(dynamicEventQueue.size > 0){ dynamicEvents = copy; if(justLoaded){ updateDynamic(new Bits(256)); justLoaded = false; } notify dynamic thread (-> updateDynamic) }
// :390-433 updateDynamic: per team clear `write` once, circle(write, x, y, rad + 1) (rad <= 0 skipped), then swap read/write for every cleared team
```
`Logic.java:533-535`: `if(state.rules.fog){ fogControl.update(); }` — runs after
`state.teams.updateTeamStats()` and **before** the entity update pass.
`Team.isOnlyAI()` (`Team.java:117`) = `isAI() && data().players.size == 0`; the port's
`Rules::team_is_only_ai(team, is_campaign)` already equals it.

**Port today.**
* `core/game/fog.rs:157-470` `FogControl` has the buffers and `circle`/`hline`, but:
  `update()` (line ~332) pushes a static event and sets `dynamic_updated` for **every** unit
  every tick (Java only when the unit's tile changed); the "chunked building visibility"
  loop (line ~350-372) sets `dynamic_updated` (Java calls `updateFogVisibility`, a render-only
  `wasVisible` pass); after a flush it moves events into `dynamic_events` but only calls
  `update_dynamic` when `just_loaded` — so `read` is drawn once and never again.
* `core/game/rules.rs:37` `DYNAMIC_UPDATE_INTERVAL_TICKS = 40` (comment "25 Hz at 60 tps" is
  wrong: 40 **ms** = 2.4 ticks).
* No `Resource` derive on `FogControl`; no field on `Sim`; `rg FogControl::new core/sim` is empty.

**Changes.**
1. `core/game/rules.rs:37`: change the constant to `pub const DYNAMIC_UPDATE_INTERVAL_TICKS: u64 = 3;`
   with the doc comment: "`FogControl.dynamicUpdateInterval` (`1000 / 25` ms, FogControl.java:23):
   `timeSinceMillis(last) > 40` at 60 Hz first holds on the 3rd frame (50 ms)." Keep the `>=`
   comparison in `update()` (3 ticks = 50 ms > 40 ms).
2. `core/game/fog.rs`: add `bevy_ecs::prelude::Resource` to the `FogControl` derive
   (`#[derive(Debug, Clone, bevy_ecs::prelude::Resource)]`). Add a field
   `pub last_fog_pos: indexmap::IndexMap<bevy_ecs::entity::Entity, i32>` (initialised empty in
   `new()`, cleared in `stop()`). It replaces `UnitComp.lastFogPos` (lookups only, never
   iterated for logic).
3. `core/game/fog.rs`: new input struct and rewrite `update` to Java's shape. Replace the
   signature by:
   ```rust
   /// One unit fog source for `FogControl::update` (`team.units` row).
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub struct FogUnit {
       pub entity: bevy_ecs::entity::Entity,
       pub tile_x: i32,
       pub tile_y: i32,
       /// `(int)unit.type.fogRadius` (truncation, FogControl.java:240).
       pub radius: i32,
       pub team: u8,
   }

   /// `FogControl.update` (FogControl.java:186-282), single-threaded.
   /// `teams` = present teams that are NOT only-AI, ascending id.
   pub fn update(&mut self, tick: u64, teams: &[u8], blocks: &[FogSource], units: &[FogUnit], static_fog: bool)
   ```
   Body, in this order:
   * `if static_fog && !self.loaded_static { self.push_static_blocks(blocks, false); self.update_static(); self.loaded_static = true; }`
   * `self.dynamic_event_queue.clear();`
   * delete the old chunked building loop entirely (wasVisible is done in change 6).
   * `for &team in teams`: `self.unit_event_queue.clear()`; ensure
     `self.fog[team] = Some(FogData::new(self.world_len()))` if `None`; for each `unit` in
     `units` with `unit.team == team` (keep slice order): `if unit.radius <= 0 { continue; }`
     `let pos = unit.tile_x + unit.tile_y * self.ww as i32; let event = fog_event(unit.tile_x, unit.tile_y, unit.radius, team);`
     push to `unit_event_queue`; `if self.last_fog_pos.get(&unit.entity) != Some(&pos) { if static_fog { self.push_event(event, false); } self.last_fog_pos.insert(unit.entity, pos); data.dynamic_updated = true; }`.
     (Java's `pushEvent` returns early when `!staticFog`, FogControl.java:166.)
     Then the existing flush check (`dynamic_updated && tick - last_dynamic_tick >= DYNAMIC_UPDATE_INTERVAL_TICKS`):
     on flush push every `blocks` row with `block.team == team` (rounded radius already in
     `FogSource.radius`) then `extend` with `unit_event_queue`.
   * `self.update_static();` (deterministic replacement for the static thread; keep).
   * `if !self.dynamic_event_queue.is_empty() { self.dynamic_events = std::mem::take(&mut self.dynamic_event_queue); let mut cleared = [false; 256]; self.update_dynamic(&mut cleared); self.just_loaded = false; }`
     — always redraw (this is the dynamic thread's work, done synchronously).
   Delete the now-unused `headless`/`fog_enabled` params (callers only call `update` when
   `rules.fog`).
4. `core/game/fog.rs`: add `pub fn mark_dynamic(&mut self, team: u8)` (sets
   `dynamic_updated = true` when `fog[team]` is `Some`) and
   `pub fn on_source_changed(&mut self, src: FogSource, static_fog: bool)` = Java
   TileChangeEvent body: `mark_dynamic(src.team); if static_fog && self.fog[src.team].is_some() { self.push_event(fog_event(src.x, src.y, src.radius, src.team), false); }`.
   Note Java's `forceUpdate(team, build)` (FogControl.java:178-188) is the same body; T3 calls it.
5. New file `core/sim/fog.rs` (header lines per H-environment §3; `mod fog;` in
   `core/sim/mod.rs`), an `impl Sim` block:
   ```rust
   /// `FogControl` live glue (FogControl.java + Logic.java:533-535).
   impl Sim {
       /// `WorldLoadEvent` half: resets and sizes the fog for the current grid.
       pub fn reset_fog(&mut self);
       /// `Logic.update` -> `fogControl.update()` when `rules.fog`.
       pub(crate) fn update_fog(&mut self);
       /// `FogControl.isVisibleTile(team, x, y)`.
       pub fn fog_visible_tile(&self, team: u8, x: i32, y: i32) -> bool;
       /// `FogControl.isDiscovered(team, x, y)`.
       pub fn fog_discovered(&self, team: u8, x: i32, y: i32) -> bool;
   }
   /// Collects `(blocks, units)` fog sources from the ECS in stable order.
   pub(crate) fn collect_fog_sources(world: &World, content: &ContentRegistry) -> (Vec<FogSource>, Vec<FogUnit>);
   ```
   * `reset_fog`: read rules via `crate::game::runtime::rules_of(&self.ecs.0)` (no
     `CampaignRuntime` -> fog off -> remove the resource and return). Get or insert the
     `FogControl` resource, then call `on_world_load(width as u16, height as u16, rules.fog, rules.static_fog, &blocks)`
     where `blocks` come from `collect_fog_sources`, filtered to teams that are not only-AI.
   * `update_fog`: return unless `rules.fog`. Build `teams`: every team id that owns at least
     one unit or one building in the ECS (Java `state.teams.present` =
     `buildings.size > 0 || units`, Teams.java:188,219), sorted ascending, minus
     `rules.team_is_only_ai(team, is_campaign)` (`is_campaign` =
     `runtime.session.is_campaign()`). Call `fog.update(self.state.tick, &teams, &blocks, &units, rules.static_fog)`
     with `resource_scope` or remove/insert the resource. Then run change 6.
   * `collect_fog_sources`: buildings = entities with `Building` + `TeamComp` whose block def
     has `BlockFlag::HasFogRadius` (T3 extends the radius), radius
     `crate::sim::fog::building_fog_radius(world, def, entity).round() as i32`
     (for now `def.fog_radius as f32`); units = entities with `UnitTypeComp` + `Pos` +
     `TeamComp` + not dead, `tile_x = (pos.x / 8.0).floor() as i32` (port tiles span
     `[x*8, x*8+8)`, H-environment §4), `radius = def.fog_radius as i32` (truncate).
     Sort both by `entity.index()` (determinism).
   * `fog_visible_tile`/`fog_discovered`: `rules_of` + `team_is_ai` then delegate to
     `FogControl::is_visible_tile`/`is_discovered`; with no resource return `true` when
     `!rules.fog` (or no rules), `false` otherwise.
6. `core/sim/fog.rs` `update_fog`, after `fog.update`: the `updateFogVisibility` pass
   (`BuildingComp.java:2285-2293`, FogControl.java:205-227) for the **default team only**
   (single-player `player.team()`): chunked exactly like Java — `size` = number of buildings
   (sorted by entity index), `chunks = min(5, size)`, `iterated = max(1, size / chunks)`,
   start at `last_entity_update_index % size` (store this cursor on `FogControl`, field
   exists), for each visited building: if `!was_visible && !in_fog_to(default_team)` set
   `visible_flags |= 1 << default_team` and `was_visible = true` on the `Building` component.
   `in_fog_to` is T2's helper; until T2 lands write a private copy of BuildingComp.inFogTo
   (quoted in T2) here and have T2 replace it.
7. `core/sim/mod.rs` `Sim::tick`: insert `self.update_fog();` directly after
   `self.update_logic_globals();` and before `self.schedule.run(..)` (Java order: fog update
   precedes the entity pass).
8. `core/sim/mod.rs` `Sim::apply_rules_to_world` (~line 1584): at the end call
   `self.reset_fog();` **only when** the fog flags changed or the resource is absent: keep a
   `last_fog_flags: Option<(bool, bool)>` field on `Sim` (init `None`, reset in the world-clear
   path `clear_world_for_load`), compare `(rules.fog, rules.static_fog)`; reset when
   different. Also call `self.reset_fog()` at the end of `clear_world_for_load` (world reload).
9. Events (Java TileChange/TilePreChange/UnitDestroy listeners): in `Sim::flush_events`
   (`core/sim/mod.rs` ~1783), for `SimEvent::BlockPlacedEvent` and `BlockBrokenEvent`, if a
   `FogControl` resource exists and `rules.fog` and the block has `HasFogRadius` and the team
   is not only-AI: `BlockPlaced` -> `on_source_changed(source, rules.static_fog)` (source from
   `collect`-style radius of the new building, look it up by `grid.entity_at`), `BlockBroken`
   -> `mark_dynamic(team)` (needs the team: T10 adds `team` to `BlockBrokenEvent`; if T10 has
   not landed, read the team before removal is impossible here, so call `mark_dynamic` for
   **every** non-AI team — harmless, it only forces a redraw). Unit deaths: in
   `core/sim/unit_runtime.rs` where `killed_units += 1` (~line 530), if the resource exists,
   `mark_dynamic(unit_team)`.
10. gdext seam (new, `gdext/sim_host.rs`, next to `building_enabled`):
    ```rust
    /// Fog probe (`FogControl.isVisibleTile/isDiscovered`) for playtests and the H03 fog renderer.
    #[func]
    pub fn fog_debug_json(&self, team: i32, x: i32, y: i32) -> GString
    ```
    returns `{"fog":bool,"staticFog":bool,"visible":bool,"discovered":bool,"teams":[ids with Some(FogData)]}`
    built from `rules_of` and the two `Sim` probes; `{"fog":false}` when no `CampaignRuntime`.
    And a debug toggle (rules are not editable in campaign):
    ```rust
    /// Debug: forces `rules.fog`/`rules.staticFog` on the live session and re-initialises fog.
    #[func]
    pub fn debug_set_fog(&mut self, fog: bool, static_fog: bool) -> bool
    ```
    (sets both fields on `CampaignRuntime.session.rules`, then `self.sim.reset_fog()`; false
    without a `CampaignRuntime`). Also replace the body of `gdext/campaign.rs` `fog_query` to
    report `"discovered"` from the SimHost probe? **No** — MindCampaign does not own the Sim;
    leave it and add a doc line pointing to `SimHost.fog_debug_json`.

**Tests** (`core/sim/combat_tests.rs`):
* `fog_unit_reveals_and_discovers`: `campaign_sim(64, |r| { r.waves = true; r.fog = true; r.static_fog = true; })`,
  `sim.reset_fog()`, spawn `"dagger"` team 1 at world (20*8+4, 20*8+4) (dagger `fog_radius`
  = 174/8 = 21.75 -> event radius 21). `sim.step`/`tick` 3 times. Assert
  `fog_visible_tile(1, 30, 20)` true, `fog_visible_tile(1, 50, 20)` false (distance 30 >
  21+1), `fog_discovered(1, 30, 20)` true, `fog_discovered(1, 50, 20)` false. Team 2 (AI):
  `fog_visible_tile(2, 60, 60)` true. Move the dagger to (50*8+4, 20*8+4) (write `Pos`), tick
  3 times: `fog_visible_tile(1, 20, 20)` false (beyond 21+1 from x=50? |50-20| = 30 -> false),
  `fog_discovered(1, 20, 20)` still true.
* `fog_off_everything_visible`: same without `r.fog`; every probe true, no `FogControl`
  resource present.
* `fog_dynamic_interval_is_three_ticks`: unit test on `FogControl` in `core/game/fog.rs`:
  call `update` at ticks 0,1,2,3 with one moving unit; assert the second flush happens at tick 3,
  not before (`last_dynamic_tick`).
* Keep the existing `core/game/fog.rs` tests green (adapt the `update` call sites to the new signature).

**Verify in-engine** (bridge seams added in change 10; confirm with
`grep -n -A1 '#\[func\]' client/rust/mind-gdext/src/sim_host.rs | grep fog`):
1. `tools/build.sh`, `tools/session-play.sh N --import`, enter Ground Zero (H-environment §4.1).
2. `$G --eval "return $H.debug_set_fog(true, true)"` -> `true`.
3. `$G --eval "$H.set_paused(true); $H.step(10); return $H.fog_debug_json(1, 129, 60)"` ->
   `"visible": true, "discovered": true` (next to the core: core fog radius is T3's; the
   player alpha unit, `fog_radius` default `max(58*3, hitSize*2)/8 = 21.75`, already covers it).
4. `$G --eval "return $H.fog_debug_json(1, 129, 120)"` -> `"visible": false, "discovered": false`.
5. `$G --eval "return $H.fog_debug_json(2, 129, 120)"` -> `"visible": true` (wave team is AI).
6. Erekir (preferred, if it loads): `$H.load_sector("erekir", 10)` + `$MC.start_sector("erekir", 10)`
   + `set_menu_visible(false)`; `JSON.parse_string($MC.get_rules_json()).fog` -> `true`;
   `fog_debug_json(1, 276, 107)` (Onset core-bastion anchor tile; `$H.best_core_position()` returns
   world **pixels** (2208,856) — divide by 8) visible true. Verified 2026-10-14: Onset loads with
   `hasCore:true`, `fog:true`, `staticFog:true`. If it fails to load, write that down and rely on steps 2-5.
7. `grep -c panic` on the newest log = 0. Save the eval outputs to `runs/<date>-s<N>-H09-T1/`.

**Pitfalls.**
* `FogControl.fog` is a `Box<[Option<FogData>; 256]>`: never clone the resource per tick.
* Use `resource_scope::<FogControl, _>` or remove/insert; never hold `get_resource_mut` across
  `collect_fog_sources` (needs `&World`).
* `rules.fog` false must cost nothing (Serpulo): return before collecting sources.
* Do not iterate `last_fog_pos` (IndexMap lookups only); prune entries for despawned units in
  `update_fog` with `retain(|e, _| world.get_entity(*e).is_ok())` every 600 ticks to bound it.

**Commit.** `Gameplay H09 T1: live FogControl (world-load reset, per-tick update, dynamic flush every 3 ticks)`

---

### T2 — Fog gates: targeting skips units/buildings hidden in fog; undiscovered tiles reject placement

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code: no fog terms in `targeting.rs:98`, `block_indexer.rs:203/258`, `build.rs:163`). The in-engine recipe was wrong and is fixed below: (129,120) is a `snow-pine` outside Ground Zero's limit area, so the place is already rejected today without fog (`apply_sim_command_json` -> false); use (112,35) (air, inside the area, ≈ 27 tiles from the alpha, accepted today -> true; undiscovered once T1 is in and before T3's core radius). The JSON `place` op needs `"team":1` (default 0), and `str(...)` concatenation in the eval is unnecessary: copper-wall is block index 216 (`content_list("block").find("copper-wall")`).

**Symptom / acceptance.** With fog on, player turrets and player units shoot at enemies they
cannot see; player AI targets undiscovered enemy buildings; the player can build into black
(undiscovered) fog. After: `closest_target` for team 1 ignores a team-2 dagger standing on an
invisible tile; `find_enemy_tile` for team 1 ignores an undiscovered team-2 building; a
`player:0` placement on an undiscovered tile is rejected. AI teams are unaffected.

**Java.**
```java
// UnitComp.java:232-248
public boolean inFogTo(Team viewer){
    if(this.team == viewer || !state.rules.fog) return false;
    if(hitSize <= 16f){ return !fogControl.isVisible(viewer, x, y); }
    else{ float trns = hitSize / 2f;
        for(var p : Geometry.d8){ if(fogControl.isVisible(viewer, x + p.x * trns, y + p.y * trns)) return false; } }
    return true;
}
// BuildingComp.java:315-333 isDiscovered(viewer)
if(state.rules.limitMapArea && world.getDarkness(tile.x, tile.y) >= 3) return false;
if(viewer == null || !state.rules.staticFog || !state.rules.fog) return true;
if(block.size <= 2) return fogControl.isDiscovered(viewer, tile.x, tile.y);
int s = block.size / 2;
return isDiscovered(viewer, x, y) || (x - s, y - s) || (x - s, y + s) || (x + s, y + s) || (x + s, y - s);
// BuildingComp.java:2231-2249 inFogTo(viewer)
if(team == viewer || !state.rules.fog) return false;
int size = block.size, of = block.sizeOffset, tx = tile.x, ty = tile.y;
if(!isDiscovered(viewer)) return true;
for(x in 0..size) for(y in 0..size) if(fogControl.isVisibleTile(viewer, tx + x + of, ty + y + of)) return false;
return true;
// Units.java:304 closestEnemy / :326 bestEnemy:  ... || !e.targetable(team) || e.inFogTo(team)) return;
// BlockIndexer.java:468: findTile(enemy, x, y, range, b -> pred.get(b) && b.isDiscovered(team), true, source)
// Build.java:252 (validPlace per covered tile):
(state.rules.staticFog && state.rules.fog && !fogControl.isDiscovered(team, wx, wy)) ||
```
Overrides: `ForceProjector.java:202` and `BaseShield.java:121` `inFogTo` -> read them and
port the same override for `ForceProjectorState`/`BaseShieldState` buildings (they return
`false` when the shield is visible — copy the exact body from Java).

**Port today.** `core/combat/targeting.rs:98-140` `closest_unit_filtered` filters team,
derelict, dead, air/ground, range only. `core/ai/block_indexer.rs:203-257 find_enemy_tile`
and `:258 find_enemy_building` filter team/flags/range only. `core/world/build.rs:163
valid_place_at` has no fog term.

**Changes.**
1. New module `core/game/fog_gate.rs` (`pub mod fog_gate;` in `core/game/mod.rs` or wherever
   `game` submodules are declared):
   ```rust
   /// `UnitComp.inFogTo(viewer)` (UnitComp.java:232-248).
   pub fn unit_in_fog_to(world: &World, unit: Entity, viewer: u8) -> bool;
   /// `BuildingComp.isDiscovered(viewer)` (BuildingComp.java:315-333).
   pub fn building_discovered(world: &World, content: &ContentRegistry, grid: Option<&WorldGrid>, build: Entity, viewer: u8) -> bool;
   /// `BuildingComp.inFogTo(viewer)` (BuildingComp.java:2231-2249).
   pub fn building_in_fog_to(world: &World, content: &ContentRegistry, grid: Option<&WorldGrid>, build: Entity, viewer: u8) -> bool;
   ```
   All three read `crate::game::runtime::rules_of(world)` (None -> not in fog / discovered),
   `is_campaign` from the `CampaignRuntime`, the `FogControl` resource, and call
   `FogControl::is_visible`/`is_visible_tile`/`is_discovered` with
   `is_ai = rules.team_is_ai(viewer, is_campaign)`. Unit hit size from `HitboxComp.hit_size`;
   `Geometry.d8` = `[(1,0),(1,1),(0,1),(-1,1),(-1,0),(-1,-1),(0,-1),(1,-1)]`. Building `size`,
   `size_offset` from `BlockDef` (`def.size_offset` exists). The darkness term
   (`limitMapArea && getDarkness >= 3`) uses `crate::world::darkness` with the grid when
   `grid` is `Some` (grep `pub fn get_darkness\|border_darkness` in `core/world/darkness.rs`
   for the combined function; if none combines wall+border darkness, use the same expression as
   `core/world/build.rs:195-200`).
2. `core/combat/targeting.rs` `closest_unit_filtered`: after the dead check add
   `if crate::game::fog_gate::unit_in_fog_to(world, entity, team) { continue; }`.
3. `core/ai/block_indexer.rs` `find_enemy_tile` and `find_enemy_building`: after the team
   check add `if !crate::game::fog_gate::building_discovered(world, content, None, entity, team) { continue; }`
   (`find_enemy_building` gets `content` if it lacks one — check its signature; if adding a
   param ripples through more than 5 call sites, read `BlockTable` from the world resource
   instead: `world.get_resource::<crate::world::BlockTable>()` gives `instance(block).def`).
   Java only checks `isDiscovered` here (not `inFogTo`), keep that.
4. Replace T1 change 6's private `in_fog_to` copy with `building_in_fog_to`.
5. Placement (`Build.java:252`): add to `core/world/limits.rs` `BuildRules` a field
   `pub static_fog_mask: Option<std::sync::Arc<StaticFogMask>>` with
   ```rust
   /// Player-team static fog snapshot for `Build.validPlace` (Build.java:252).
   #[derive(Debug, Clone, PartialEq)]
   pub struct StaticFogMask { pub team: u8, pub width: i32, pub height: i32, pub bits: crate::game::fog::FogBits }
   ```
   In `valid_place_at` (`core/world/build.rs`), inside the per-covered-tile loop (find the
   loop that checks each `(wx, wy)` of the footprint), add: if
   `rules.static_fog_mask` is `Some(mask)` and `mask.team == team` and
   `!(0..mask.width).contains(&wx) || !(0..mask.height).contains(&wy) || !mask.bits.get((wx + wy * mask.width) as usize)`
   -> `return false`. In `core/sim/fog.rs` `update_fog` (and `reset_fog`): when
   `rules.fog && rules.static_fog`, rebuild the mask for `rules.default_team` **only when the
   static buffer changed** (track `static_dirty: bool` on `FogControl`, set `true` in
   `update_static` when it drew at least one event, cleared by the Sim after copying) and write
   it into the `BuildRules` resource (`world.get_resource_mut::<BuildRules>()`); set `None`
   when fog is off. This keeps `valid_place_at` signature unchanged (H02 edits that function
   too — keep the hunk to the per-tile check only).

**Tests** (`core/sim/combat_tests.rs`):
* `turret_ignores_unit_in_fog`: `campaign_sim(64, |r| { r.waves = true; r.fog = true; })`,
  `reset_fog()`, place a team-1 `duo` (via the runtime API used by other turret tests in that
  file; grep `"duo"` there) at (10,10), feed it copper the way existing duo tests do; spawn a
  team-2 dagger at tile (25,10) (inside duo range 160 px = 20 tiles? 15 tiles = 120 px, yes).
  Before T3 the duo is not a fog source: assert after 30 ticks the duo has no target
  (`TurretState.target == None`). Then spawn a team-1 `dagger` at (24,10) (reveals), tick 5,
  tick 30 more: target is `Some`.
* `ai_team_sees_through_fog`: a team-2 duo targets a team-1 dagger with no team-2 fog sources.
* `find_enemy_tile_skips_undiscovered`: `campaign_sim` with `fog`+`static_fog`; team-2 core
  at (40,40); call `BlockIndexer::find_enemy_tile` for team 1 from (10,10) with a huge range ->
  `None`; spawn a team-1 unit next to (40,40), tick 3 -> `Some`.
* `placement_rejected_in_static_fog`: with fog+static fog and no team-1 sources,
  `valid_place` for a `copper-wall` team 1 at (30,30) is false; after a team-1 unit at (30,30)
  and 1 tick, true.

**Verify in-engine.** Ground Zero after T1 steps 1-2 (`debug_set_fog(true,true)`):
* `$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"player\":0,\"team\":1,\"block\":216,\"x\":112,\"y\":35,\"rotation\":0}')"`
  -> `false` (undiscovered; 216 = `content_list("block").find("copper-wall")`, `"team":1` is required —
  the op defaults team to 0); the same at (129,60) -> `true`. (Verified 2026-10-14 without fog: both
  return `true` today, while the old probe tile (129,120) is a `snow-pine` outside the limit area and
  returns `false` regardless of fog. `clear_player_build_plans()` afterwards.)
* Spawn a crux dagger far from the core: `{"op":"spawn_unit","unit":<dagger idx>,"team":2,"px":1032,"py":960}`;
  place a duo + drill at the documented spot (130,48)/(132,48) (H-environment §4.1) and step
  600: the duo does not fire at the dagger while `fog_debug_json(1, 129, 120).visible` is
  false (`unit_debug_json` health of the dagger unchanged). Headless covers the reveal half.

**Pitfalls.** Do not gate `closest_target` for team 0/derelict differently (Java returns early
for derelict before the fog check). `rules_of` returns `None` in non-campaign sims: every gate
must then be a no-op (combat goldens unchanged — run `cargo run -q -p mind-headless -- parity goldens`).

**Commit.** `Gameplay H09 T2: fog hides units/buildings from targeting; static fog blocks placement`

---

### T3 — Fog sources: radar discovery, turret and core fog radius, forceUpdate

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: `RadarBehavior::update_tile` is empty (`world/behavior/defense.rs:354`), `RadarState` has only `progress`; a powered `radar` (+ adjacent `power-source`, efficiency 1.0 after 600 steps) exposes no progress readout — `building_debug` keys are block/rotation/team/efficiency/..., so the plan's "add `radarProgress`" fallback is required. Java numbers confirmed: duo `range = 160` -> fog radius 20 (`Blocks.java:3337`, `BaseTurret.java:59`), core `(int)(lightRadius/8*3)+13` (`CoreBlock.java:158-159`). `init_self` is at `blocks/mod.rs:2746` (radar `HasFogRadius` at 1807).

**Symptom / acceptance.** The Erekir `radar` (fogRadius 34) never reveals anything and its
progress never grows; turrets and cores reveal nothing (Java: every turret reveals its range,
every core `max(fogRadius, (int)(lightRadius/8*3)+13)` tiles). After: a powered radar's
`RadarState.progress` reaches 1.0 after 600 ticks at full efficiency and reveals 34 tiles;
a `duo` reveals `round(160/8) = 20` tiles; a `core-bastion` reveals its radius.

**Java.**
```java
// Radar.java:18-19,33   discoveryTime = 60f * 10f; rotateSpeed = 2f; fogRadius = 10 (radar: 34, Blocks.java:1993)
// Radar.java:55-73
public float fogRadius(){ return fogRadius * progress * smoothEfficiency; }
public void updateTile(){
    smoothEfficiency = Mathf.lerpDelta(smoothEfficiency, efficiency, 0.05f);
    if(Math.abs(fogRadius() - lastRadius) >= 0.5f){ Vars.fogControl.forceUpdate(team, this); lastRadius = fogRadius(); }
    progress += edelta() / discoveryTime; progress = Mathf.clamp(progress);
    totalProgress += efficiency * edelta();
}
// fields: progress, lastRadius = 0f, smoothEfficiency = 1f, totalProgress
// BaseTurret.java:59 (init)   fogRadius = Math.max(Mathf.round(range / tilesize * fogRadiusMultiplier), fogRadius);
// Turret.java:426-428         fogRadius() = (range + (hasAmmo() ? peekAmmo().rangeChange : 0f)) / tilesize * fogRadiusMultiplier;
// Turret.java:555-561         if(state.rules.fog){ float newRange = hasAmmo() ? peekAmmo().rangeChange : 0f;
//                                if(newRange != lastRangeChange){ lastRangeChange = newRange; fogControl.forceUpdate(team, this); } }
// CoreBlock.java:158-159      lightRadius = 30f + 20f * size; fogRadius = Math.max(fogRadius, (int)(lightRadius / 8f * 3f) + 13);
// Block.java:1390             if(fogRadius > 0) flags = flags.with(BlockFlag.hasFogRadius);
// Blocks.java:5661 (scathe)   fogRadiusMultiplier = 0.4f;
// BuildingComp.java:1375-1379 onRepaired(): if(block.flags.contains(hasFogRadius)) fogControl.forceUpdate(team, self());
```
Note `CoreBlock.java:158` overwrites `lightRadius` for cores (the per-core `lightRadius =`
lines in Blocks.java are for other blocks): core-shard (size 3) -> light 90 ->
`(int)(90/8*3)+13 = 33+13 = 46`; size 4 -> 110 -> 41+13 = 54; size 5 -> 130 -> 48+13 = 61;
bastion (4) 54, citadel (5) 61, acropolis (6) -> 150 -> 56+13 = 69.

**Port today.** `RadarBehavior::update_tile` empty (`core/world/behavior/defense.rs` ~354);
`RadarState { progress }` only (`core/entities/comp/building.rs:202`); turret `TurretConfig`
(`core/world/blocks/defense/turrets/mod.rs:160`) has `range` but no `fog_radius_multiplier`;
`BlockDef::init_self` (`core/content/registries/blocks/mod.rs:2751`) only flags explicit
`fog_radius > 0`. Bullet defs have no `range_change` field (see §6 H02).

**Changes.**
1. `core/entities/comp/building.rs` `RadarState`: add `pub last_radius: f32`,
   `pub smooth_efficiency: f32` (default **1.0**: implement `Default` by hand),
   `pub total_progress: f32`. Save layout is unchanged (Java writes only `progress`).
2. `core/world/behavior/defense.rs` `RadarBehavior::update_tile`: port `Radar.updateTile`
   exactly. `efficiency` = `Building.efficiency`; `edelta` =
   `crate::world::update::edelta(world, e)` (grep to confirm the name); `lerpDelta(a, b, t)` =
   `a + (b - a) * (t * delta).min(1.0)` with delta 1.0 (use the shared helper if
   `grep -rn "fn lerp_delta" core/` finds one). `fog_radius()` = `def.fog_radius as f32 *
   progress * smooth_efficiency` (`def` via `BlockTable` instance). On the `>= 0.5` change call
   `fog_force_update(world, e)` (change 5) then set `last_radius`. Clamp `progress` to `0..=1`.
3. `core/sim/fog.rs`: add
   ```rust
   /// `Building.fogRadius()` for every source kind (BuildingComp.java:579-581 + overrides).
   pub(crate) fn building_fog_radius(world: &World, def: &BlockDef, e: Entity) -> f32
   ```
   order: `RadarState` -> `def.fog_radius * progress * smooth_efficiency`; `TurretState` ->
   `config.range / 8.0 * config.fog_radius_multiplier` (+ ammo `rangeChange` once H02 adds it;
   leave a `// TODO(H02 range_change)` comment and list it as a deviation); `CoreBuild` ->
   `max(def.fog_radius, (((30 + 20*size) as f32 / 8.0 * 3.0) as i32) + 13) as f32`; else
   `def.fog_radius as f32`. In `collect_fog_sources` treat an entity as a source when
   `def.flags` has `HasFogRadius` **or** it has `TurretState`/`CoreBuild`/`RadarState`, and
   use `building_fog_radius(..).round() as i32` (Java `Mathf.round` = `floor(x + 0.5)`; use
   `(x + 0.5).floor() as i32`, not Rust `round`, which differs on negative halves — radii are
   non-negative, so both agree, but keep Java's form).
4. `TurretConfig`: add `pub fog_radius_multiplier: f32` (1.0 in every arm; find each
   `TurretConfig {` literal with `grep -n "TurretConfig {" core/world/blocks/defense/turrets/*.rs`
   and add the field, or add it to a `..Default` if one exists); set `0.4` in the `"scathe"`
   arm (`Blocks.java:5661`; verify with `grep -n fogRadiusMultiplier J/content/Blocks.java`:
   it is the only vanilla turret with a multiplier).
5. `core/sim/fog.rs`: `pub(crate) fn fog_force_update(world: &mut World, e: Entity)` =
   `FogControl.forceUpdate(team, build)` (FogControl.java:178-188): read rules (`fog`,
   `static_fog`), the building's team/tile and radius (change 3), and call
   `FogControl::on_source_changed`. No-op without the resource or when `!rules.fog`.
6. `core/content/registries/blocks/mod.rs` `init_self`: for core kinds (find how core blocks
   are recognised, e.g. `BlockKind::Core`; grep `CoreBlock` in that file) set
   `self.fog_radius = self.fog_radius.max(((30.0 + 20.0 * self.size as f32) / 8.0 * 3.0) as i32 + 13)`
   **before** the existing `HasFogRadius` flag line. Turret flags: in the same function, for
   turret kinds push `BlockFlag::HasFogRadius` (the radius itself is computed at runtime from
   `TurretConfig`, because content has no turret range). Regenerate nothing: `fog_radius` is
   not in `parity/golden_content.json` block fields (checked 2026-10-14); run
   `cargo run -q -p mind-headless -- parity goldens` and the content dump diff anyway — if
   `golden_content.json` changes, regenerate per H-environment §2 and name `CoreBlock.java:159`
   in the commit body.
7. Derelict repair forceUpdate (`BuildingComp.onRepaired`): if H02 has landed a derelict
   repair completion hook (grep `on_repaired` in `core/`), call `fog_force_update` there;
   otherwise skip and note it.

**Tests.**
* `core/world/behavior/defense.rs` tests (or `core/sim/production_tests.rs`): place `radar`
  + `power-source` (sandbox, API place) in a `campaign_sim`-style sim with `fog = true`; tick
  600 -> `RadarState.progress` within `1e-3` of 1.0; tick 300 -> ~0.5.
* `core/sim/combat_tests.rs` `duo_reveals_its_range`: fog on, team-1 duo at (10,10) only;
  `reset_fog()`, tick 3 -> `fog_visible_tile(1, 30, 10)` true (20 + 1), `(32, 10)` false.
* `core_fog_radius_values`: assert `content.block(core-shard).fog_radius == 46`,
  `core-bastion == 54`, `core-acropolis == 69` (block names from the registry).

**Verify in-engine.** Ground Zero + `debug_set_fog(true,true)`: `fog_debug_json(1, 129, 100)`
is `visible:true` (core-shard radius 46 from (129,55) reaches y=101) and `(129,103)` is false
when the player unit is parked at the core (move nothing). Place a `radar` with a
`power-source` via `place_block` at (140,60)/(141,60), step 600,
`building_debug(140,60)` shows progress (if `building_debug` does not print radar progress,
add `"radarProgress"` to its JSON). Panic count 0.

**Pitfalls.** The radar's forceUpdate must not run when `!rules.fog` (Java: `forceUpdate`
checks `state.rules.fog && fog[team] != null`). Cores of the wave team are sources too but the
team is only-AI, so `update` never draws them — do not special-case.

**Commit.** `Gameplay H09 T3: radar discovery, turret/core fog radius and forceUpdate`

---

### T4 — `checkAllowUpdate`: derelict, env-unsupported and outside-map-area buildings stop updating; turrets honour `enabled`

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: a derelict `mechanical-drill` placed with `apply_sim_command_json('{"op":"place","x":130,"y":48,"block":325,"rotation":0,"team":0}')` (no `player`) mined 3 copper in 600 ticks and `building_enabled(130,48)` is true; `check_allow_update` is the empty default at `world/hooks.rs:68`. `debug_set_building_team` does not exist yet (this task adds it); the team-0 command place above is a pre-T4 alternative and also exercises the "newly created building" call site after T4.

**Symptom / acceptance.** Derelict (team 0) buildings keep running (drills mine, factories
craft, unit factories produce derelict units, captured enemy bases converted to derelict by
H04/G04 T7 keep working); blocks that do not support the planet env run anyway; when a map
area shrinks (Erekir world processors `setrule mapArea`), buildings outside keep updating.
Also turrets ignore `Building.enabled` (`control enabled 0` from a processor cannot stop a
turret). After: a derelict `mechanical-drill` on copper produces nothing in 1625 ticks;
`building_enabled` reports false for it; a team-1 drill outside a limited map area is disabled;
a disabled duo does not fire.

**Java.**
```java
// BuildingComp.java:627-632
public boolean allowUpdate(){
    return team != Team.derelict && block.supportsEnv(state.rules.env) &&
        (tile instanceof EditorTile || block.privileged || !state.rules.limitMapArea || !state.rules.disableOutsideArea ||
         Rect.contains(state.rules.limitX, state.rules.limitY, state.rules.limitWidth, state.rules.limitHeight, tile.x, tile.y));
}
// BuildingComp.java:2261-2265
public void checkAllowUpdate(){ if(!allowUpdate()){ enabled = false; } }
// call sites: BuildingComp.java:136 (create/add), :1842 (changeTeam), Build.java:103 (derelict repair start),
//             World.java:44-52 (WorldLoadEvent: every build), World.java:469-481 checkMapArea(prevX, prevY, prevW, prevH):
for(var team : state.teams.present){ for(var build : team.buildings){
    if(!build.enabled && build.block.autoResetEnabled && !Rect.contains(x, y, w, h, build.tile.x, build.tile.y)){ build.enabled = true; }
    build.checkAllowUpdate(); } }
// BuildingComp.java:2276-2281 update(): updateConsumption(); if(enabled || !block.noUpdateDisabled) updateTile();
// Block.java:170 autoResetEnabled = true; SwitchBlock.java:23 autoResetEnabled = false
```
`World.checkMapArea` is called from `LExecutor.java:1880` with the **previous** area when
`setrule mapArea` changes it. `Rect.contains(x, y, w, h, px, py)` is `px >= x && py >= y && px < x + w && py < y + h`
(check `ARC/arc-core/src/arc/math/geom/Rect.java` `contains(float,float,float,float,float,float)`).

**Port today.** `core/world/hooks.rs:68` empty `check_allow_update`; no code sets
`enabled = false` outside tests; turret update (`core/world/blocks/defense/turrets/mod.rs:1348
update_turrets` -> `update_turret`) has no `enabled` check; `Building.enabled` is honoured by
`core/world/update.rs:330-345` for non-turret behaviors.

**Changes.**
1. New `core/world/allow_update.rs` (`pub mod allow_update;` in `core/world/mod.rs`):
   ```rust
   /// `BuildingComp.allowUpdate()` (BuildingComp.java:627-632).
   pub fn allow_update(def: &BlockDef, team: u8, tile: TilePos, env: i32, limit_area: Option<(i32,i32,i32,i32)>, disable_outside_area: bool) -> bool {
       team != crate::game::team::DERELICT.0
           && crate::world::build::supports_env_bits(def, env)
           && (def.privileged || limit_area.is_none() || !disable_outside_area
               || limit_area.is_some_and(|(x, y, w, h)| { let (tx, ty) = (tile.x() as i32, tile.y() as i32); tx >= x && ty >= y && tx < x + w && ty < y + h }))
   }
   /// `BuildingComp.checkAllowUpdate()`.
   pub fn check_allow_update(world: &mut World, e: Entity, env: i32, limit_area: Option<(i32,i32,i32,i32)>, disable_outside_area: bool);
   ```
   `check_allow_update` resolves `def` via `world.get_resource::<BlockTable>()` +
   `instance(building.block)`, team via `TeamComp`, and sets `Building.enabled = false` when
   `allow_update` is false. It never sets `true`.
2. `core/sim/mod.rs`: `pub fn check_all_allow_update(&mut self)` — reads rules
   (`rules_of`; with no `CampaignRuntime` use `env = DEFAULT_ENV` from
   `crate::game::rules::DEFAULT_ENV`, no limit, `disable_outside_area = true`), collects
   building entities sorted by index, calls `check_allow_update` for each.
   * Call it from `Sim::apply_rules_to_world` (the WorldLoadEvent equivalent — sector start and
     every rules apply). **Before** overwriting `self.grid.limit_area`, keep
     `let prev = self.grid.limit_area;` and, if `prev != new` and `prev.is_some()`, run the
     `checkMapArea(prev)` reset first: for each building, if `!enabled && def.auto_reset_enabled`
     and the building tile is **not** inside `prev`, set `enabled = true`; then
     `check_all_allow_update()`. Add `pub auto_reset_enabled: bool` to `BlockDef` (default
     `true`, `false` for `switch`/`world-switch`? — Java sets it only in `SwitchBlock`
     constructor, which covers `switch` and `world-switch`; grep `SwitchBlock(` in
     `J/content/Blocks.java` to list them) if the field is absent (grep first).
   * Call `check_allow_update` for a newly created building: find the spot where the live
     runtime inserts a building (`core/sim/runtime.rs` `set_block_raw` / the place path that
     runs `create_state`) and call it after `create_state` (Java `BuildingComp.java:136`).
   * Team change: find the derelict conversion (`destroy_to_derelict` callers in
     `core/sim/` / `apply_campaign_world_requests`) and any `TeamComp` write for an existing
     building; after the write call `check_allow_update` (Java `:1842`).
3. Turrets: in `core/world/blocks/defense/turrets/mod.rs` `update_turret` (or the loop in
   `update_turrets`), skip the per-turret `updateTile` work when `Building.enabled == false`
   (Java: `Turret` has `noUpdateDisabled = false` default, so a disabled turret does nothing in
   `updateTile`; its `updateConsumption` still zeroes efficiency, handled elsewhere). Keep
   reload/heat decay as Java does: read `Turret.updateTile` (`J/world/blocks/defense/turrets/Turret.java:520-620`)
   — everything there is inside `updateTile`, so skip all of it.
4. gdext debug seam (`gdext/sim_host.rs`):
   ```rust
   /// Debug: `Tile.setTeam` for the building at (x, y) (`BuildingComp.changeTeam` -> checkAllowUpdate).
   #[func]
   pub fn debug_set_building_team(&mut self, x: i32, y: i32, team: i32) -> bool
   ```
   implemented through a new `Sim::debug_set_building_team(x, y, team: u8) -> bool` in
   `core/sim/mod.rs` that writes `TeamComp`, refreshes whatever team indices the runtime keeps
   (grep how `destroy_to_derelict` changes team on buildings and reuse that function), then
   calls `check_allow_update`.

**Tests** (`core/sim/production_tests.rs`):
* `derelict_drill_does_not_mine`: `runtime_sim()`, ore floor under a `mechanical-drill` (copy
  the setup from the existing drill test, grep `fn full_drill_stops`), plus a container; switch
  the drill to team 0 with `Sim::debug_set_building_team`; tick 1625 -> container copper 0 and
  `Building.enabled == false`. Control: same at team 1 -> copper 9 (G02 T6 number).
* `outside_map_area_disabled_and_reset`: install a campaign session with
  `limit_map_area = true, limit_x/y = 0, limit_width/height = 16`, drill at (20,20);
  `apply_rules_to_world` -> drill disabled. Then rules with width/height 32 and apply again ->
  the drill (outside the previous 16x16 area, `auto_reset_enabled`) is re-enabled.
* `env_unsupported_block_disabled`: rules `env = 17` (Erekir) and a Serpulo-only block (find
  one whose `env_enabled` lacks `Env.scorching`, e.g. `mechanical-drill` is
  `Env.terrestrial` -> supported on Erekir? verify with `supports_env_bits` in the test; pick a
  block that returns false, e.g. one requiring `Env.oxygen` like `combustion-generator`? —
  assert with a direct `supports_env_bits` precondition so the test documents the choice).
* `disabled_duo_does_not_fire` (`core/sim/combat_tests.rs`): set `enabled = false` on a fed
  duo with an enemy in range; 120 ticks -> no bullets spawned (`get_group_count("bullet")`
  equivalent: `UnitRuntime.bullets.len()` or the bullet query used in other turret tests).

**Verify in-engine.** Ground Zero: place `mechanical-drill` (130,48) + `container` (132,48)
via `place_block`; `$H.debug_set_building_team(130, 48, 0)`; `building_enabled(130,48)` ->
`false`; step 1700; `building_items_json(132,48)` -> `{}`. Then break/re-place at team 1 ->
items appear. Panic 0.

**Pitfalls.**
* `supports_env_bits` depends on H00's env fix; with the old `env = 31` some Serpulo blocks
  may wrongly read unsupported — run the env test only after H00 (start-state check).
* Never set `enabled = true` except in the checkMapArea reset (logic `control enabled` owns
  the rest).
* The editor sim (`rules.editor`) uses `EditorTile` = always allowed: skip
  `check_all_allow_update` when `rules.editor`.

**Commit.** `Gameplay H09 T4: checkAllowUpdate disables derelict/env/outside-area buildings; turrets honour enabled`

---

### T5 — Fires: Java lifetime terms, fireballs, unit damage + burning, building flammability

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code): `Tile::get_flammability` returns 0 for every non-air tile (`world/tile.rs:99-109`); the fireball timer is clamped `.min(40.0)` and never fires (`combat/fires.rs:243-244`); Java `baseLifetime = 1000` (`Fires.java:14`) and the random timer starts (`FireComp.java:35-37`) confirmed. In-engine: destroying a container holding 300 coal spawned 0 bullets in 30 ticks. There is no fire seam (`debug_create_fire` absent) and `get_group_counts()` has no `fire`/`puddle` group (keys: all/build/bullet/draw/effect/player/power_graph/sync/unit/weather) — add the seam as the plan says, and read fires via a new group or a debug count.

**Symptom / acceptance.** Fires never throw fireballs (so they spread only from flammable
floors), never damage or ignite units, burn as long on bare ground as on fuel, and a burning
container full of coal has flammability 0. After: a fire on a container holding 300 coal
(`flammability 1.0`) lives longer than `baseLifetime`, spawns `fireball` bullets every
`fireballDelay` worth of accumulation, and a dagger standing in it takes 3 damage every 40
ticks and gets `burning` for 300 ticks.

**Java.**
```java
// FireComp.java:23-26 frames = 40, duration = 90; spreadDelay = 22f, fireballDelay = 40f,
//   ticksPerFrame = 90/40, warmupDuration = 20f, damageDelay = 40f, tileDamage = 1.8f, unitDamage = 3f
// FireComp.java:58-104 (update, after the time/remove checks)
Building entity = tile.build; boolean damage = entity != null;
float flammability = tile.getFlammability() + puddleFlammability;
if(!damage && flammability <= 0){ time += Time.delta * 8; }
if(damage){ lifetime += Mathf.clamp(flammability / 8f, 0f, 0.6f) * Time.delta; }
if(flammability > 1f && (spreadTimer += Time.delta * Mathf.clamp(flammability / 5f, 0.3f, 2f)) >= spreadDelay){
    spreadTimer = 0f; Point2 p = Geometry.d4[Mathf.random(3)]; Fires.create(world.tile(tile.x + p.x, tile.y + p.y)); }
if(flammability > 0 && (fireballTimer += Time.delta * Mathf.clamp(flammability / 10f, 0f, 0.5f)) >= fireballDelay){
    fireballTimer = 0f; Bullets.fireball.createNet(Team.derelict, x, y, Mathf.random(360f), -1f, 1, 1); }
if((damageTimer += Time.delta) >= damageDelay){
    damageTimer = 0f; Puddle p = Puddles.get(tile); puddleFlammability = p != null ? p.getFlammability() / 3f : 0;
    if(damage){ entity.damage(tileDamage); }
    Damage.damageUnits(null, tile.worldx(), tile.worldy(), tilesize, unitDamage,
        unit -> !unit.isFlying() && !unit.isImmune(StatusEffects.burning),
        unit -> unit.apply(StatusEffects.burning, 60 * 5)); }
// Tile.java:142-160 getFlammability()
if(block == Blocks.air){ return floor.liquidDrop != null ? floor.liquidDrop.flammability : 0; }
else if(build != null){ float result = 0f;
    if(block.hasItems && build.items.total() > 0) result += build.items.sum((item, amount) -> item.flammability * amount) / Math.max(block.itemCapacity, 1) * Mathf.clamp(block.itemCapacity / 2.4f, 1f, 3f);
    if(block.hasLiquids) result += build.liquids.sum((liquid, amount) -> liquid.flammability * amount / 1.6f) / Math.max(block.liquidCapacity, 1) * Mathf.clamp(block.liquidCapacity / 30f, 1f, 2f);
    return result; }
return 0;
// Damage.java:436-454 damageUnits(team=null, x, y, size, damage, pred, acceptor):
//   rect = (size*2) square centred on (x,y); for every unit (Units.nearby) with pred && hittable whose hitbox overlaps: unit.damage(damage); acceptor(unit)
// Bullets.java:44-46 fireball = new FireBulletType(1f, 4){{ hittable = false; }};  createNet(..., damage -1 -> type damage, velocityScl 1, lifetimeScl 1)
```
(`fireballTimer` starts at `Mathf.random(40f)`? — check FireComp.java:33-36 field initialisers
and keep the port's existing random starts if they match.)

**Port today.** `core/combat/fires.rs:153-272 update_fires`: no `time += 8` on empty
ground, no `lifetime +=`, `puddle_flammability` recomputed every tick as
`base + liquid.flammability * amount` (Java: tile flammability + **stored** puddle value, which
is refreshed only on the damage tick and divided by 3), fireball timer clamped at 40 and never
fires, no unit damage, building damage writes `Health` directly (bypasses
`rules.blockHealth`). `core/world/tile.rs:99-109 get_flammability` returns 0 for buildings.

**Changes.**
1. `core/combat/fires.rs`: add `pub const FIREBALL_DELAY: f32 = 40.0;` and a
   `pub fn tile_flammability(world: &World, tiles: &Tiles, content: &ContentRegistry, tx: i32, ty: i32) -> f32`
   implementing `Tile.getFlammability` exactly (air: floor `liquid_drop` flammability — reuse
   `Tile::get_flammability`; building: items term with `def.item_capacity`, liquids term with
   `def.liquid_capacity`, `def.has_items`/`def.has_liquids`; `ItemModule.items` index = item id,
   `LiquidModule.liquids` index = liquid id; non-air non-building -> 0). `BlockDef` via
   `BlockTable` resource.
2. Rewrite the body of `update_fires` after the `time >= lifetime` removal to Java order:
   * `flammability = tile_flammability(..) + fire.puddle_flammability` (stored value).
   * `if build.is_none() && flammability <= 0.0 { fire.time += 8.0; }`
   * `if build.is_some() { fire.lifetime += (flammability / 8.0).clamp(0.0, 0.6); }`
   * spread: unchanged logic, but `rng.random(RngStream::Sim, 4)` must be Java
     `Mathf.random(3)` = inclusive 0..=3; confirm `SimRng::random(stream, n)` semantics with
     grep (if it is exclusive `0..n`, `4` is right).
   * fireball: `if flammability > 0.0 { fire.fireball_timer += (flammability / 10.0).clamp(0.0, 0.5); if fire.fireball_timer >= FIREBALL_DELAY { fire.fireball_timer = 0.0; queue a fireball } }`
     (remove the `.min(40.0)` clamp).
   * damage tick: `puddle_flammability = puddle.map_or(0.0, |p| liquid.flammability * p.amount / 3.0)`;
     building: `crate::combat::damage::area::apply_health(world, build, TILE_DAMAGE)`;
     units: new helper `damage_units_square(world, content, cx, cy, half = 8.0, UNIT_DAMAGE, burning)`
     that, for every unit (stable index order) that is not flying
     (`crate::combat::bullet::unit_is_flying`), not immune to burning
     (`UnitTypeDef.immunities: Vec<StatusId>` contains it), hittable, and whose hitbox square
     (`hit_size/2`) overlaps the `16x16` square centred on the tile centre, calls
     `crate::combat::damage::area::damage_entity(world, content, unit, UNIT_DAMAGE, false, 1.0)`
     then `crate::combat::damage::status::apply_status(world, unit, burning, 300.0)`.
     `burning = content.status_id("burning")`.
3. Fireballs need a `CombatCtx` (bullet spawn). Change `update_fires` to return
   `Vec<(f32, f32)>` = fireball origins (fire centre) and spawn them in
   `Sim::update_fires_and_puddles` (`core/sim/unit_runtime.rs:988`) with a `CombatCtx` built
   the same way as `turret_update_system` (`core/sim/unit_runtime.rs:2542-2577`): for each
   origin `ctx.spawn(&BulletSpawn { def: <fireball id>, team: 0, x, y, angle: rng.range(RngStream::Sim, 0.0, 360.0), damage: <fireball def damage = 4>, ..BulletSpawn::default() })`,
   then push the spawned bullets into `runtime.bullets` like `turret_update_system` does. The
   fireball `BulletId`: resolve by name — grep how bullets are named in
   `core/content/registries/bullets.rs` (the test at line ~2141 uses `BulletId::new(4)`; prefer a
   name lookup if one exists, else add `pub const FIREBALL: BulletId = BulletId::new(4)` next to
   the other bullet id constants with a test asserting it is the fireball).
   `damage: -1` in Java means "type damage": check `BulletSpawn` docs for the sentinel; pass
   the def's damage explicitly if no sentinel exists.
4. The `FireBehavior` (`core/combat/bullet/kinds/fire.rs`) already creates trail fires; keep.

**Tests** (`core/combat/fires.rs` tests use `CombatHarness`; whole-sim tests in
`core/sim/combat_tests.rs`):
* `fire_on_bare_ground_dies_fast`: fire on stone, no building: after `ceil(1000/9) = 112`
  ticks the fire is gone (time grows 1+8 per tick).
* `fire_on_coal_container_lives_longer_and_throws_fireballs`: container (itemCapacity 300)
  with 300 coal: `tile_flammability` = `300*1.0/300 * clamp(300/2.4,1,3)` = **3.0**; lifetime
  after 100 ticks = `1000 + 100*0.375` = **1037.5**; with flammability 3.0 the fireball timer
  grows 0.3/tick -> first fireball after 134 ticks (from the random start: set
  `fireball_timer = 0` in the test); assert a fireball bullet exists after the tick it fires.
* `fire_burns_ground_units`: dagger on the fire tile; set `damage_timer = 39`; tick once ->
  dagger health `150 - (3 - armor)` (dagger armor 0? check `UnitTypes.java:104-120`; use the
  registry value) and has `burning`; a flare (flying) is untouched.
* `fire_damage_respects_block_health_multiplier`: `rules.block_health_multiplier = 2` ->
  tile damage 0.9 per hit.
* Update `spread_timer_ignites_neighbor_with_flammable_puddle` and the other existing fire
  tests if their expectations encoded the old behaviour (write down each changed number and
  the Java line that justifies it in the commit body).

**Verify in-engine.** Ground Zero: `place_block(140,60,"container")`,
`debug_add_building_items(140,60,"coal",300)`; start a fire with an existing seam — grep
`fn debug_.*fire\|create_fire` in gdext; if none exists add
`#[func] debug_create_fire(x: i32, y: i32) -> bool` on SimHost calling `fires::create`
through the unit runtime's rng; step 600; `get_group_count("bullet")` > 0 at some step (poll
every 60 ticks), and `building_debug(140,60)` health drops. Screenshot. Panic 0.

**Pitfalls.** `apply_health` divides by `blockHealth` — do not divide again. Keep the fire
cache sync (`tiles.set_fire`) at the end of each fire's update. Iterate fires in stable order
(the current `iter_entities` collect is archetype order — sort by `entity.index()` before the
loop, as other systems do).

**Commit.** `Gameplay H09 T5: fires follow FireComp.update (lifetime terms, fireballs, unit burning, building flammability)`

---

### T6 — Puddles: one update-time decrement, `accepting`, air-only spread, reaction fireballs, CellLiquid spread, deconstruct drop

> **In-engine verification (2026-10-14, session 2):** CONFIRMED (code): `update_time -= 1.0` at `combat/puddles.rs:269` and `:379`; spread test `other.build.is_none()` at `:279`; `on_deconstructed` (`world/behavior/mod.rs:224`) has no caller. NOT CHECKABLE in-engine yet: no `debug_add_building_liquid` seam and no `puddle` group (see T5).

**Symptom / acceptance (PAR-0091).** Puddles apply their status twice as often as Java,
same-liquid deposits grow instantly, puddles spread into walls/boulders, neoplasm never
spreads into water-filled buildings, and deconstructing a tank full of slag leaves no puddle.
After: status re-apply interval 40 ticks; a 2-deposit tick grows by `max` not sum; a
`liquid-tank` with 1500 slag being deconstructed drops `1500/9*2` per covered tile.

**Java.** (quoted in §1 rows; key lines)
```java
// PuddleComp.java:53-58  amount -= delta*(1-viscosity)/(5+addSpeed); amount += accepting; amount = min(amount, maxLiquid); accepting = 0
// PuddleComp.java:60-70  if(amount >= maxLiquid/1.5f){ deposited = min((amount - maxLiquid/1.5f)/4f, 0.3f*delta);
//   for d4: other = world.tile(..); if(other != null && (other.block() == Blocks.air || liquid.moveThroughBlocks)){ targets++; Puddles.deposit(other, tile, liquid, deposited, false); }
//   amount -= deposited * targets; }
// PuddleComp.java:87-101 effects every 40: updateTime = 40f;  ...  PuddleComp.java:110 updateTime -= Time.delta;   (once)
// PuddleComp.java:112   liquid.update(self());
// Puddles.java:90-92    else if(p.liquid == liquid){ p.accepting = Math.max(amount, p.accepting); ...
// Puddles.java:116-121  flammable+hot: Fires.create(tile); if(Mathf.chance(0.006 * amount)) Bullets.fireball.createNet(Team.derelict, x, y, Mathf.random(360f), -1f, 1f, 1f);
// CellLiquid.java:36-95 update(puddle): if(!state.rules.fire) return; spreadTarget logic (read the full body)
// BuildingComp.java:1480-1487 onDeconstructed(builder):
if(liquids != null && liquids.currentAmount() > 0 && (!liquids.current().incinerable || block.deconstructDropAllLiquid)){
    float perCell = liquids.currentAmount() / (block.size * block.size) * 2f;
    tile.getLinkedTiles(other -> Puddles.deposit(other, liquids.current(), perCell)); }
// Build.java:50-53 beginBreak: if(tile.build != null){ prevBuild.add(tile.build); tile.build.onDeconstructed(unit); tile.build.dead = true; }
```
Read `CellLiquid.java:36-95` in full before change 5 (spread to adjacent buildings holding
`spreadTarget`, damage the building under it, absorb adjacent `spreadTarget` puddles, and the
`reacted` -> `removeScaling` tail).

**Port today.** `core/combat/puddles.rs:239-385`: `puddle.update_time -= 1.0` at line ~269
**and** ~380; spread target test `other.build.is_none() || move_through_blocks` (~277);
`deposit` same-liquid branch (~151) adds immediately; `react_puddle` (~186) no fireball; no
`liquid.update`; `core/world/behavior/mod.rs:224 on_deconstructed` is never called
(`core/sim/runtime.rs:545 begin_break` does not call it).

**Changes.**
1. Delete the first `puddle.update_time -= 1.0;` (the one next to `accepting = 0.0`); keep the
   end-of-update decrement.
2. Spread targets: replace `other.build.is_none()` with `other.block == BlockId::AIR` (the
   `Tile` struct's block field; grep its name in `core/world/tile.rs`) in both loops.
3. `deposit` same-liquid: `puddle.accepting = puddle.accepting.max(amount);` and nothing else
   (the update loop already folds `accepting` in).
4. `react_puddle` flammable+hot: after `fires::create`, `if rng.chance(RngStream::Sim, (0.006 * amount) as f64)`
   (check `chance` param type) queue a fireball at the tile centre: give `deposit`/`react_puddle`
   access to a `&mut Vec<(f32, f32)>` fireball sink, or store pending fireball origins in a new
   `bevy_ecs` resource `PendingFireballs(Vec<(f32,f32)>)` that T5's spawn code drains in
   `Sim::update_fires_and_puddles` (preferred: no signature churn; T5 then drains both).
5. `CellLiquid.update` (only `neoplasm` has `cell` data, `core/content/registries/liquids.rs:342`):
   at the end of each puddle update, if `def.cell` is `Some` and the fire rule is on
   (`CombatEnv.fire`), port `CellLiquid.java:36-95` line by line: building liquid reads/writes
   via `LiquidModule` (`get`/`remove`), building damage via
   `crate::combat::damage::area::apply_health`, deposits via `deposit`.
6. Deconstruct drop: new behavior-agnostic function in `core/sim/runtime.rs`
   `fn drop_deconstruct_liquid(&self, world: &mut World, grid: &WorldGrid, entity: Entity, def: &BlockDef)`
   implementing `onDeconstructed` (current liquid = the port's `LiquidModule` "current"
   — grep `fn current` in `core/world/modules/mod.rs`; it returns an amount, find the id
   accessor; `incinerable` from the liquid def; `def.deconstruct_drop_all_liquid` — add the
   field (default false) if missing and set it where Java does:
   `grep -n deconstructDropAllLiquid J/content/Blocks.java`). Deposit into every linked tile
   of the footprint. Call it in `begin_break` right before the swap to the construct block
   (Java order), and also call `inst.behavior.on_deconstructed(world, entity, None)` there so
   per-kind overrides (none in vanilla today) can hook in. Puddle deposit needs the unit
   runtime rng: take it out like `update_fires_and_puddles` does.

**Tests** (`core/combat/puddles.rs` tests):
* `status_interval_is_40_ticks`: a full slag puddle and a dagger: count `apply_status` calls
  (or observe status duration refresh) over 80 ticks -> 2 applications, not 4.
* `same_liquid_deposit_uses_max_accepting`: deposit 5 and 3 water on an existing puddle in
  one tick -> after `update_puddles` amount grew by 5 minus evaporation, not 8.
* `spread_skips_static_walls`: full puddle next to a `stone-wall` tile -> no puddle on it.
* `neoplasm_spreads_into_water_building`: neoplasm puddle next to a conduit holding water ->
  the conduit loses water and a neoplasm puddle appears on its tile.
* `core/sim/production_tests.rs` `deconstructing_slag_tank_drops_puddles`: `liquid-tank`
  (3x3) with slag via `debug`-style insertion (grep an existing liquid fill helper), begin a
  break through the player build path (or call `begin_break` directly) -> 9 slag puddles each
  `min(amount/9*2, maxLiquid)`.

**Verify in-engine.** Ground Zero: `place_block(140,60,"liquid-tank")`, fill slag (add a
`debug_add_building_liquid(x,y,name,amount)` seam if none exists — grep first), queue a
player break `{"op":"break","player":0,"x":140,"y":60}`, step 120, `liquid_debug` /
`get_group_count` shows puddles (add `"puddle"` to `get_group_count` if it is not a known
group — grep its match arms). Panic 0.

**Pitfalls.** Determinism: puddles iterate in `iter_entities` order today; sort by
`entity.index()` like T5. Do not double-call `on_deconstructed` from the instant-deconstruct
path (Java skips it for `instantDeconstruct`).

**Commit.** `Fixes PAR-0091: puddles follow PuddleComp.update/Puddles.deposit; CellLiquid spread; deconstruct liquid drop`

---

### T7 — Destroyed buildings explode like Java; `dynamicExplosion` lightning, fireballs and wave stagger

> **Coordinator note (2026-10-14).** You own lightning, fireballs, wave stagger and the building terms (H01 T7 was cut down to `explosion_absorbed` semantics only — do not touch `explosion_absorbed`). Unit-death explosion inputs stay H01's.

> **In-engine verification (2026-10-14, session 2):** CONFIRMED: a full `battery` (bar `bar.poweramount` 4000) at (140,60) with a `power-source` at (141,60), and a `container` with 300 coal at (140,66), were destroyed with `debug_kill_building`: 0 bullets after 1/10/30 ticks (no lightning, no fireballs); a `copper-wall` at (143,66) dropped to 0.82 from the damage waves. FIXED below: Java battery capacity is **4000** (`consumePowerBuffered(4000f)`, `Blocks.java:2512`), so the test expects 6 bolts, not 2; the in-engine tiles (150,70)/(152,70) are ice walls — use (140,66)/(143,66).

**Symptom / acceptance.** A destroyed container full of blast compound barely explodes (the
`* 3.5` factor and scales are missing), batteries never arc lightning, vaults linked to a core
explode at full strength, conduits explode at full strength, and every explosion does all its
damage waves in one tick with one fire instead of fireballs. After: the explosion inputs of a
destroyed building match `BuildingComp.onDestroyed`; `dynamic_explosion` spawns
`clamp(power/700,0,8)` lightning bolts and `clamp(flammability/4,0,30)` fireballs and spaces
damage waves 2 ticks apart.

**Java.**
```java
// BuildingComp.java:1463-1465  public int explosionItemCap(){ return block.itemCapacity; }
// StorageBlock.java:99-103      explosionItemCap(): linkedCore != null ? Math.min(itemCapacity/60, 6) : itemCapacity;
// BuildingComp.java:1490-1518 onDestroyed():
float explosiveness = block.baseExplosiveness; float flammability = 0f; float power = 0f;
if(block.hasItems){ for(Item item : content.items()){ int amount = Math.min(items.get(item), explosionItemCap());
    explosiveness += item.explosiveness * amount; flammability += item.flammability * amount; power += item.charge * Mathf.pow(amount, 1.1f) * 150f; } }
if(block.hasLiquids){ flammability += liquids.sum((l, a) -> l.flammability * a / 2f); explosiveness += liquids.sum((l, a) -> l.explosiveness * a / 2f); }
if(block.consPower != null && block.consPower.buffered){ power += this.power.status * block.consPower.capacity; }
if(block.hasLiquids && state.rules.damageExplosions){ liquids.each(this::splashLiquid); }
Damage.dynamicExplosion(x, y, flammability * block.flammabilityScale, explosiveness * 3.5f * block.explosivenessScale, power,
    tilesize * block.size / 2f, state.rules.damageExplosions, block.destroyEffect, block.baseShake);
// Block.java:197-201 baseExplosiveness = 0f; explosivenessScale = 1f; flammabilityScale = 1f;
// Blocks.java: battery baseExplosiveness 1 (2513), battery-large 5 (2520); PowerGenerator.java:60 baseExplosiveness = 5f (every PowerGenerator);
//   explosivenessScale = flammabilityScale = conduit 10/20 (2327), pulse-conduit 16/40 (2335), plated-conduit 16/50 (2343),
//   liquid-router 20/120 (2352), bridge-conduit 20/100 (2383), phase-conduit 20/100 (2396), reinforced-conduit 20/50 (2422),
//   reinforced-bridge-conduit 20/120 (2443), reinforced-liquid-router 40/150 (2458)
// Damage.java:93-120 dynamicExplosion(x, y, flammability, explosiveness, power, radius, damage, fire, ignoreTeam, fx, baseShake):
if(damage){
  for(int i = 0; i < Mathf.clamp(power / 700, 0, 8); i++){
    int length = 5 + Mathf.clamp((int)(Mathf.pow(power, 0.98f) / 500), 1, 18);
    Time.run(i * 0.8f + Mathf.random(4f), () -> Lightning.create(Team.derelict, Pal.power, 3 + Mathf.pow(power, 0.35f), x, y, Mathf.random(360f), length + Mathf.range(2))); }
  if(fire){ for(int i = 0; i < Mathf.clamp(flammability / 4, 0, 30); i++){
    Time.run(i / 2f, () -> Call.createBullet(Bullets.fireball, Team.derelict, x, y, Mathf.random(360f), Bullets.fireball.damage, 1, 1)); } }
  int waves = explosiveness <= 2 ? 0 : Mathf.clamp((int)(explosiveness / 11), 1, 25); float damagePerWave = explosiveness / 2f;
  for(int i = 0; i < waves; i++){ int f = i; Time.run(i * 2f, () -> {
      var shields = ignoreTeam == null ? null : indexer.getEnemy(ignoreTeam, BlockFlag.shield);
      if(shields == null || shields.isEmpty() || !shields.contains(b -> b instanceof ExplosionShield s && s.absorbExplosion(x, y, damagePerWave))){
          damage(ignoreTeam, x, y, Mathf.clamp(radius + explosiveness, 0, 50f) * ((f + 1f) / waves), damagePerWave, false); }
      Fx.blockExplosionSmoke.at(x + Mathf.range(radius), y + Mathf.range(radius)); }); } }
if(explosiveness > 15f) Fx.shockwave.at(x, y); if(explosiveness > 30f) Fx.bigShockwave.at(x, y);
float shake = Math.min(explosiveness / 4f + baseShake, 9f); Effect.shake(shake, shake, x, y); explosionFx.at(x, y, radius / 8f);
```
`Mathf.clamp(power / 700, 0, 8)` is a float bound: the loop runs `ceil` times for fractional
values (i = 0, 1, ... while `i < bound`). Same for fireballs.

**Port today.** `core/sim/unit_runtime.rs` `building_explosion_terms` (~1715-1764): no
`base_explosiveness`, cap = `item_capacity` always; caller (~770-789) passes raw terms (no
`3.5`, no scales). `core/combat/damage/explosion.rs:31-89` `dynamic_explosion`: no lightning
(comment says "not ported"), one `fires::create` instead of fireballs, all waves immediately,
`explosion_absorbed` runs even when `ignore_team` is `None` (Java skips shields then).

**Changes.**
1. `BlockDef`: add `pub base_explosiveness: f32` (0), `pub explosiveness_scale: f32` (1),
   `pub flammability_scale: f32` (1) — check first that none exists (grep returned nothing on
   2026-10-14). Set the values above in the block registries (`core/content/registries/blocks/power.rs`
   for batteries/generators — every `PowerGenerator` subclass kind gets 5: find the kinds via
   `BlockKind` names containing `Generator`/`Reactor`; `.../liquid.rs` for the conduit family).
   If any of these fields is emitted into `parity/golden_content.json`, regenerate per
   H-environment §2 and cite the Blocks.java lines.
2. `building_explosion_terms`: start `explosiveness = def.base_explosiveness`; cap =
   `explosion_item_cap(world, entity, def)` = for a `StorageBuild` with `linked_core.is_some()`
   (`core/world/blocks/storage/storage_block.rs`) `(def.item_capacity / 60).min(6)`, else
   `def.item_capacity`. Caller: pass `flammability * def.flammability_scale` and
   `explosiveness * 3.5 * def.explosiveness_scale` (the destruction call site at ~774).
3. `core/combat/damage/explosion.rs`: new signature taking the combat context so it can spawn
   bullets/lightning:
   ```rust
   pub fn dynamic_explosion(ctx: &mut CombatCtx<'_>, x: f32, y: f32, flammability: f32, explosiveness: f32,
       power: f32, radius: f32, damage: bool, fire: bool, ignore_team: Option<u8>)
   ```
   * lightning: `let bolts = (power / 700.0).clamp(0.0, 8.0); let mut i = 0; while (i as f32) < bolts { ... i += 1; }`
     each: `length = 5 + ((power.powf(0.98) / 500.0) as i32).clamp(1, 18)`, damage
     `3.0 + power.powf(0.35)`, angle `rng.range(0,360)`, length `+ rng.range(-2..2)` rounded
     like Java `length + Mathf.range(2)` (float added to int -> passed as int: Java's
     `Lightning.create(..., int length)` receives `(int)(length + range(2))`? — check the
     overload in `J/entities/Lightning.java`; match it), color = `Pal.power` (grep the port's
     palette constant), team 0. Delay `i*0.8 + random(4)` ticks.
   * fireballs: `(flammability / 4.0).clamp(0.0, 30.0)` bullets, delay `i / 2` ticks.
   * waves: delay `i * 2` ticks; shield check only when `ignore_team.is_some()`
     (`explosion_absorbed` stays as is otherwise).
   * Delays: add a resource `core/combat/damage/explosion.rs`
     `#[derive(Resource, Default)] pub struct DelayedExplosionActions(pub Vec<(u64, DelayedAction)>)`
     with `enum DelayedAction { Lightning{..}, Fireball{x,y,angle}, Wave{x,y,radius,damage,ignore_team} }`,
     scheduled at `current_tick + ceil(delay)` (delay 0 -> runs this tick, immediately). Drain
     due actions in `Sim::update_fires_and_puddles` (same CombatCtx as T5's fireballs) in
     insertion order. Document the `ceil` rounding as a deviation (Arc `Time.run` fires on the
     first frame where elapsed >= delay).
   * Keep the FX half as-is or emit `Fx.shockwave`/`bigShockwave` only if the port's `FxSink`
     has those ids (grep `SHOCKWAVE` in `core/content/registries/fx_meta.rs`); shake is H03's.
4. Update both callers (`core/sim/unit_runtime.rs:446` unit death, `:774` building) to build a
   `CombatCtx` (copy the construction from `turret_update_system`) and pass it. Unit death
   (`UnitComp.java:892`) passes its own terms — do not change those values (H01 owns them).
5. Mass driver bolt (`MassDriverBolt.java:102`) and core destroy (`CoreBlock.java:681`) call
   `dynamicExplosion` too — grep the port for other `dynamic_explosion(` callers and convert
   them; if absent, list them in §6 for H02 (do not add them here).

**Tests** (`core/sim/combat_tests.rs`):
* `blast_container_explosion_terms`: container with 100 blast-compound (explosiveness 1.2,
  flammability 0.4): terms `(40.0*1.0, 120.0*3.5 = 420.0, 0.0)` after scaling.
* `linked_vault_explodes_weakly`: vault adjacent to a core, 1000 items capacity -> cap
  `min(1000/60, 6) = 6`.
* `battery_lightning_count`: battery `power.status = 1`, `consPower.capacity` **4000**
  (`Blocks.java:2512`; corrected 2026-10-14, the plan said 1000) -> `power = 4000` ->
  `4000/700 = 5.71` -> 6 bolts (`i = 0..5`), each length `5 + clamp((int)(4000^0.98/500), 1, 18)`
  = 11 (± the `Mathf.range(2)` term) and damage `3 + 4000^0.35` ≈ 21.2; assert 6 lightning
  actions scheduled.
* `explosion_waves_are_staggered`: explosiveness 33 -> waves `clamp(3,1,25) = 3` at ticks
  +0, +2, +4: health of a nearby wall drops in three steps.
* Existing G01 T14 tests (`grep -n "explosion" core/sim/combat_tests.rs`) stay green; if a
  number changes, cite the Java line in the commit body.

**Verify in-engine.** Ground Zero: `place_block(140,60,"battery")` + `place_block(141,60,"power-source")`
(charges to 4000 in < 120 steps; `building_bars_json(140,60)` power 1.0), step 120,
`debug_kill_building(140,60)`, step 10: `get_group_count("bullet")` > 0 (lightning bullets).
`place_block(140,66,"container")` + `place_block(143,66,"copper-wall")` +
`debug_add_building_items(140,66,"coal",300)`, kill the container, step 30: bullets > 0
(fireballs), wall health reduced (`building_bars_json(143,66)`; `building_debug` has no health).
(150,70)/(152,70) are ice walls. Before T7 (verified 2026-10-14): 0 bullets both times. Panic 0.

**Pitfalls.** `CombatCtx` borrows the unit runtime: take `UnitRuntime` out of the world
once, do all explosion work, put it back. Bullets created here must be added to
`runtime.bullets` (see `turret_update_system`) or they never update.

**Commit.** `Gameplay H09 T7: Building.onDestroyed explosion terms and full Damage.dynamicExplosion`

---

### T8 — `TeamRule.fillItems`: AI cores are refilled every tick

> **In-engine verification (2026-10-14, session 2):** CONFIRMED in-engine on Onset (`erekir` 10): the malis (team 3) `core-bastion` at about (287,196) holds `{}` after 600 ticks although `get_rules_json().teams["3"]` = `{fillItems:true, infiniteResources:true, rtsAi:true, buildSpeedMultiplier:0.4, ...}`; second malis core at about (293,288). Text fixed below to use Onset instead of the unverified frontier/50 suggestion.

**Symptom / acceptance.** On campaign sectors the wave team gets
`infiniteResources = fillItems = true` (`Logic.java:110-115`, port `core/game/play.rs:551-555`),
but nothing fills the enemy core: enemy-base turrets fed by unloaders from the core starve,
and enemy cores show 0 items. After: every tick, each active team with `fill_items` and at
least one core has every on-planet, non-hidden item set to `get_maximum_accepted` in its first
core (campaign: `i32::MAX / 2` because `coreIncinerates`).

**Java.**
```java
// Logic.java:551-561 (inside `if(!net.client() && !state.isEditor())`, per active team, before buildAi/rtsAi)
for(TeamData data : state.teams.getActive()){
    var rules = data.team.rules();
    if(rules.fillItems && data.cores.size > 0){
        var core = data.cores.first();
        content.items().each(i -> { if(i.isOnPlanet(Vars.state.getPlanet()) && !i.isHidden()){ core.items.set(i, core.getMaximumAccepted(i)); } });
    }
```
`UnlockableContent.isOnPlanet` (`J/ctype/UnlockableContent.java:132-134`):
`planet == null || planet == Planets.sun || shownPlanets.isEmpty() || shownPlanets.contains(planet)`.
`ItemModule.set` replaces the amount and fixes `total`.

**Port today.** `core/game/runtime.rs` team-AI system (~890-955, `if rule.rts_ai` / `if rule.build_ai`)
has no `fill_items` branch.

**Changes.**
1. `core/world/modules/mod.rs` `ItemModule`: add `pub fn set(&mut self, item: ItemId, amount: i32)`
   (`total += amount - old`).
2. `core/game/runtime.rs`, in the same per-team loop as `rts_ai`/`build_ai` (keep Java's
   order: fillItems first), but **outside** the `!rules.pvp` guard (Java's fillItems is not
   pvp-gated; restructure minimally: run a separate loop over `runtime.session.teams.active`
   before the pvp block). Core = `runtime.session.teams.get_or_null(team).and_then(|d| d.core())`
   (first core). Items: iterate the registry items (the content is reachable from the unit
   runtime or the `ContentIndex` resource — grep how this system already gets content);
   filter `!item.is_hidden()` and on-planet (`item.<ctype/unlock>.shown_planets` empty or
   contains the session planet id — grep the field path, e.g. `item.unlock.shown_planets`).
   Amount = the core behavior's `get_maximum_accepted(world, core, item)` (go through the
   `BlockTable` instance like other callers do), then `ItemModule::set`.
3. Must run while the sim is not editor (`rules.editor` false).

**Tests** (`core/sim/production_tests.rs`): `fill_items_fills_ai_core`: install a campaign
session with `team_rule_mut(2).fill_items = true`, `core_incinerates = true`, place a team-2
`core-shard` (API place with team — use the runtime helper the core tests use, grep
`core_capacity_unifies_adjacent_storage`), tick once -> copper `== i32::MAX / 2`; a hidden
item stays 0. With `core_incinerates = false` -> copper = core `storage_capacity` (4000).

**Verify in-engine.** Headless only is acceptable if no enemy-core sector is loadable in the
session profile; otherwise use Onset (verified 2026-10-14): `load_sector("erekir", 10)` + `start_sector("erekir", 10)` +
`set_menu_visible(false)`; malis (team 3, `fillItems: true`) `core-bastion`s at about tiles (287,196)
and (293,288); `building_items_json(287,196)` reads `{}` before T8 and large counts after (one
step is enough). Write down which path you used.

**Pitfalls.** `i32::MAX / 2` per item makes `total` overflow if summed naively over ~20
items: use `i64` internally or saturating adds in `ItemModule::set`/`total` (Java's `total`
is an `int` and overflows silently — mirror with `wrapping_add` and document).

**Commit.** `Gameplay H09 T8: TeamRule.fillItems refills AI cores every tick`

---

### T9 — Core intake: `incinerateNonBuildable` (Erekir cores) and Java's accounting order

> **Coordinator note (2026-10-14).** H02 T1 reorders `handle_item` to Java's accounting order and makes campaign cores incinerate overflow. If H02 T1 is on `main` (`git log main --oneline | grep -m1 "H02 T1"`), skip the reorder part of Change 2 and only add the `incinerate` flag and Change 3; if it is not, do the full task and tell the coordinator in your summary so H02 T1 drops its reorder.

> **In-engine verification (2026-10-14, session 2):** CONFIRMED with the plan's own recipe on Onset (player core-bastion footprint 275-278 × 106-109): `place_block(280,107,"item-source")`, `place_block_rot(279,107,"conveyor",2)`, `$I.configure_building(280,107,"content","sand")`, `step(300)` -> `core_items_json()` = `{"sand":23}` (Java incinerates). `building_config_string(280,107)` returned null even though the source was configured.

**Symptom / acceptance.** Erekir cores (`core-bastion/citadel/acropolis`) store sand, coal,
spore pods, blast compound and pyratite (Java incinerates them); a full core stops counting
`stats.coreItemCount` and `SectorInfo.handleCoreItem` for overflow items (Java still counts).
After: a bastion receiving sand via a conveyor shows the incineration effect and keeps 0 sand;
a full shard keeps counting.

**Java.** `CoreBlock.java:54` `public boolean incinerateNonBuildable = false;` (set true for
the three Erekir cores at `Blocks.java:3197,3216,3234`); `Items.java` `buildable = false` for
sand (37), coal (46), spore-pod (91), blast-compound (97), pyratite (103).
```java
// CoreBlock.java:785-799 handleStack
boolean incinerate = incinerateNonBuildable && !item.buildable;
int realAmount = incinerate ? 0 : Math.min(amount, storageCapacity - items.get(item));
super.handleStack(item, realAmount, source);
if(team == state.rules.defaultTeam && state.isCampaign()){
    if(!incinerate){ state.rules.sector.info.handleCoreItem(item, amount); }
    if(realAmount == 0 && wasVisible){ Fx.coreBurn.at(x, y); } }
// CoreBlock.java:884-909 handleItem
boolean incinerate = incinerateNonBuildable && !item.buildable;
if(team == state.rules.defaultTeam){ state.stats.coreItemCount.increment(item); }
if(net.server() || !net.active()){
    if(team == state.rules.defaultTeam && state.isCampaign() && !incinerate){ state.rules.sector.info.handleCoreItem(item, 1); }
    if(items.get(item) >= storageCapacity || incinerate){ if(!noEffect){ incinerateEffect(this, source); } noEffect = false; }
    else{ super.handleItem(source, item); } }
```
Also `acceptItem`/`getMaximumAccepted` are unchanged (`CoreBlock.java:743-751`).

**Port today.** `core/world/blocks/storage/core_block.rs:249-305`: no `incinerate_non_buildable`;
`handle_item` returns early when full **before** `add_core_item` and `handle_core_item`;
`handle_stack` ignores incineration.

**Changes.**
1. `BlockDef` (or the core-specific def the core behavior reads; grep how
   `CoreBlockBehavior` gets per-block data): add `pub incinerate_non_buildable: bool`
   (default false); set true for `core-bastion`, `core-citadel`, `core-acropolis` in the
   registry file that defines them (grep `"core-bastion"` in `core/content/registries/blocks/`).
2. `handle_item`: compute `incinerate = def.incinerate_non_buildable && !item_def.buildable`
   (`ItemDef.buildable` exists, `core/content/registries/items.rs:55`). Reorder to Java: stats
   increment first (default team), then `handle_core_item(item, 1)` when campaign && default
   team && `!incinerate`, then the full-or-incinerate branch (effect + `no_effect = false`),
   else store.
3. `handle_stack`: `real = if incinerate { 0 } else { amount.min(cap - current) }`; campaign
   accounting `handle_core_item(item, amount)` only when `!incinerate`; when `real == 0` and
   `was_visible` emit `Fx.coreBurn` if the port has that effect id (grep `CORE_BURN`).

**Tests** (`core/world/blocks/storage/core_block.rs` tests): `erekir_core_incinerates_sand`
(bastion + `handle_item(sand)` -> 0 sand, beryllium stored), `full_core_still_counts_items`
(campaign hooks spy: `handle_core_item` called once for an item arriving at a full core).
Use the existing `core_incinerates_when_rule_set` test as the template for the hooks spy.

**Verify in-engine.** Headless only unless T1's Erekir entry worked: then place a
`core-bastion`-adjacent conveyor fed by an `item-source` set to sand (API place; configure
with `configure_building(x,y,"content","sand")`), step 300, `core_items_json()` has no sand.

**Commit.** `Gameplay H09 T9: Erekir cores incinerate non-buildable items; core intake accounting order`

---

### T10 — GameStats and CampaignStats event counters

> **Coordinator note (2026-10-14).** H04 T5 (the same listeners) was withdrawn in favour of this task. Also make sure `GameStats.destroyed_block_count` is keyed by block NAME, because H05 T12 sums `core-shard`/`core-foundation`/`core-nucleus` from it for the `cores_destroyed` hint.

> **In-engine verification (2026-10-14, session 2, H00 T1+T2 applied):** CONFIRMED: two player-queued copper walls (`{"op":"place",...,"team":1,"player":0}`) finished, `game_stats_json()` still `"built":0` (it also shows `destroyed:1, wave:1` carried from the imported save — compare deltas, not absolutes). `BlockBrokenEvent` has no `team` (`core/event.rs`).

**Symptom / acceptance.** The game-over dialog and the planet stats show 0 for buildings
built/deconstructed/destroyed, units created and enemy buildings destroyed. After: placing
3 walls via the player build queue -> `buildingsBuilt == 3`; deconstructing one ->
`buildingsDeconstructed == 1`; a destroyed player wall -> `buildingsDestroyed == 1`; a
destroyed enemy wall -> `destroyedBlockCount["copper-wall"] == 1`; a factory-made unit ->
`unitsCreated == 1`; in a campaign the planet `CampaignStats` maps move the same way.

**Java.** `Logic.java:52-60` (placedBlockCount on build end, default team), `:186-199`
(BlockBuildEndEvent: default team or builder unit on default team -> breaking ?
`buildingsDeconstructed++` : `buildingsBuilt++`; campaign: planet
`buildingsDeconstructed`/`buildingsBuilt.increment(block)`), `:201-216` (BlockDestroyEvent:
default team -> `buildingsDestroyed++` + planet `buildingsDestroyed.increment(block)`; else
`destroyedBlockCount.increment(block)` + planet `enemyBuildingsDestroyed.increment(block)`),
`:218-226` (UnitDestroyEvent: non-default -> `enemyUnitsDestroyed++`; campaign planet
`enemyUnitsDestroyed` or `unitsDestroyed` `.increment(type)`), `:228-235` (UnitCreateEvent:
default team -> `unitsCreated++`, campaign planet `unitsProduced.increment(type)`).
`checkCampaignStats()` = `state.isCampaign() && !net.client()` (`Logic.java:251-253`).
UnitCreateEvent sources: `UnitFactory.java:444`, `Reconstructor.java:359`,
`UnitAssembler.java:574`, `PayloadSource.java:143`, `UnitSpawnAbility.java:54`.

**Port today.** `core/sim/mod.rs:1786-1800` handles only `placed_block_count`;
`core/sim/unit_runtime.rs:530-540` only `enemy_units_destroyed`; `GameStats.buildings_*`,
`destroyed_block_count`, `units_created` never move; `CampaignStats` (`core/game/stats.rs:27`)
only `waves_lasted` (`core/game/runtime.rs:404-408`). `BlockBrokenEvent` (`core/event.rs:51`)
has no `team`.

**Changes.**
1. `core/event.rs` `BlockBrokenEvent`: add `pub team: u8` (fill it at every construction
   site: `grep -rn "BlockBrokenEvent {" client/rust`; the destroy path in
   `core/sim/unit_runtime.rs` ~718 has `team` in scope; break paths read `TeamComp` before
   removal). Check `dump::block_event_record` and golden event logs: if a golden serialises
   the struct, keep the JSON shape (do not add `team` to the dump) so goldens do not move.
2. `core/sim/mod.rs` `flush_events`: next to the existing `placed_block_count` branch add, for
   the live `CampaignRuntime` (`rules.default_team`):
   * `BlockPlacedEvent` (build end, not breaking) on the default team: `stats.buildings_built += 1`;
     campaign (`session.is_campaign()`) -> planet stats `buildings_built` `+1` for the block
     name (`CampaignStats::add(&mut stats.buildings_built, name, 1)`; planet via
     `runtime.campaign.stats_for_mut(planet)` with `planet` from `runtime.session.sector`, as
     `core/game/runtime.rs:405` does).
   * `BlockBrokenEvent { destroyed: false }` (deconstruct finish) on the default team:
     `buildings_deconstructed += 1` + planet `buildings_deconstructed`.
   * `BlockBrokenEvent { destroyed: true }`: default team -> `buildings_destroyed += 1` +
     planet `buildings_destroyed`; other team -> `destroyed_block_count` `+1` by name (add a
     `GameStats::add_destroyed(name, n)` helper next to `add_placed` in `core/game/stats.rs`)
     + planet `enemy_buildings_destroyed`.
   Confirm first that `BlockPlacedEvent` is only emitted on construction completion (not on
   API/instant places used by map load). If it is also emitted for map materialisation, gate
   on the event source (grep emit sites) so loading a save does not count.
3. `core/sim/unit_runtime.rs` at the `enemy_units_destroyed` site: campaign planet stats
   `enemy_units_destroyed` (non-default team) or `units_destroyed` (default team) by unit type
   name.
4. `core/world/blocks/units/behavior.rs:56 spawn_output_unit` (UnitFactory/Reconstructor):
   when the team is the default team, `stats.units_created += 1` and campaign planet
   `units_produced` `+1` by type name. Find the UnitAssembler/PayloadSource/UnitSpawnAbility
   spawn sites (`grep -rn "units_created += 1" core/`) and add the same call there via one
   helper `fn notify_unit_created(world: &mut World, unit: Entity)` in
   `core/game/stats.rs` or `core/sim/unit_runtime.rs`.
5. gdext: in `gdext/campaign.rs` the per-frame mirror (~line 562-571) add
   `self.session.stats = runtime.session.stats.clone();` so `game_stats_json` is live (it is
   small).

**Tests** (`core/sim/production_tests.rs`): `game_stats_count_build_break_destroy`:
campaign session on a `runtime_sim`, player-build 2 walls (use the player build-plan path the
G08/H00 tests use, grep `player_build_plans`), tick until built; break one through the queue;
kill another team-1 wall (set health 0 + `resolve_deaths`), kill a team-2 wall -> assert
`buildings_built 2, buildings_deconstructed 1, buildings_destroyed 1,
destroyed_block_count["copper-wall"] 1`. `units_created` via an existing unit-factory test
(grep `unit_factory_needs_power`): after the unit spawns, `units_created == 1`.

**Verify in-engine.** Ground Zero: queue two `copper-wall` player places, step until
`construct_progress` is done, `$MC.game_stats_json()` -> `"built": 2`. Panic 0.

**Commit.** `Gameplay H09 T10: GameStats and planet CampaignStats follow the Logic event counters`

---

### T11 — Fog persistence: `static-fog-data` chunk and building `visibleFlags`

> **In-engine verification (2026-10-14, session 2):** PARTLY: code confirmed (`map_source.rs:110` passes `false`; `CustomChunk` trait `io/save/state.rs:291`). Decided the open question in the in-engine half: a Ground Zero resume always loses fog in Java too — `Planet.applyRules` -> `CampaignRules.apply` sets `rules.staticFog = rules.fog = fog` from the Serpulo campaign default `false` (`CampaignRules.java:19`, `Planet.java:294-305`); use Onset (`erekir` 10, fog on by its rule setter `Planets.java:74-84`) for the save/resume half (text fixed below).

**Symptom / acceptance.** Saving and resuming an Erekir sector forgets everything explored
(Java keeps `static-fog-data`). After: save -> load restores `FogControl` static buffers bit
for bit; buildings saved with fog on write the v4 `visibleFlags` base layout.

**Java.** `FogControl.java:97` `SaveVersion.addCustomChunk("static-fog-data", this)`;
`write` (`:437-469`: `byte used, short w, short h`, then per team `byte team` + RLE bytes
`(cur ? 0x80 : 0) | run`, run <= 127), `read` (`:471-505`), `shouldWrite` (`:507-510`:
`fog && staticFog && fog != null`). `BuildingComp.java:187` `writeVisibility = state.rules.fog && visibleFlags != 0`.

**Port today.** `core/game/fog.rs:469-547` `should_write/write/read` exist; no
`CustomChunk` is ever registered (`core/io/save/versions/v1.rs:1030` skips unknown chunks;
`core/io/sim_io.rs:818-843 save` sets no `custom_chunks`); `core/world/map_source.rs:110`
`write_all_java(.., false)`.

**Changes.**
1. `core/game/fog.rs`: `pub struct StaticFogChunk(pub std::sync::Mutex<Option<Vec<u8>>>)`
   implementing `crate::io::save::state::CustomChunk` (trait at `core/io/save/state.rs:291`):
   `write` writes the stored bytes verbatim; `read` reads `len` bytes into the mutex;
   `should_write` = bytes present. (Mutex poisoning: map the error to an `IoError`, no unwrap.)
2. `core/io/sim_io.rs` `save`: when a `FogControl` resource exists and
   `fog.should_write(rules.fog, rules.static_fog)`, build an `IndexMap` with
   `"static-fog-data" -> Arc::new(StaticFogChunk(Mutex::new(Some(fog.write()))))` and set
   `ctx.custom_chunks = Some(&map)`. `load`/`decode_save_bytes`: register a fresh
   `StaticFogChunk` in the read state's `custom_chunks` (find where `SaveReadState` is built in
   `decode_save_bytes`), carry the bytes out in `DecodedSave` (new field
   `pub static_fog: Option<Vec<u8>>`), and after the world is rebuilt (end of `load`, after
   `apply_rules_to_world`/`reset_fog` ran), call `FogControl::read(&bytes)` on the resource
   (Java order: WorldLoadEvent first, then the chunk overwrites saved teams). Also find the
   sector-resume path used by `SimHost.load_sector` (it may decode through a different
   function: `grep -n "decode_save_bytes" client/rust/mind-gdext/src`) and apply the same.
3. `core/world/map_source.rs:110`: pass `fog = rules_of(self.ecs).is_some_and(|r| r.fog)`.

**Tests** (`core/io/sim_io.rs` tests, next to `live_sim_save_then_load_keeps_fires_and_puddles`):
`live_save_keeps_static_fog`: campaign sim with fog/static fog, a team-1 unit, tick 3, save to
a temp path, reset the fog, load -> `fog_discovered(1, x, y)` matches the pre-save values on a
5x5 sample grid; and a `FogControl` write/read byte round trip (already tested in fog.rs —
extend it to two teams).

**Verify in-engine.** Use Onset, not Ground Zero: on a Serpulo resume Java itself resets `rules.fog`/`staticFog` to the
campaign default `false` (`CampaignRules.java:19` via `Planet.applyRules`), so a Ground Zero resume
can never show saved fog (verified by reading 2026-10-14). Onset: enter (fresh-sector recipe with
`erekir`, 10), spawn/walk a team-1 unit to reveal a spot away from the core, `$MC.run_exit_save()`,
re-enter with the planet "Go" recipe using `("erekir", 10)`, check
`JSON.parse_string($MC.get_rules_json()).fog` is true, then `fog_debug_json(1, x, y).discovered`
-> true for the revealed spot. If Onset's resume path fails, state "headless only for the resume half".

**Commit.** `Gameplay H09 T11: static-fog-data save chunk and building visibility flags`

---

## 3. Order and hot files

Order: **T1 -> T2 -> T3** (fog chain; T2 and T3 need T1's resource), **T4**, **T5 -> T6**
(T6 change 4 reuses T5's fireball spawn), **T7** (reuses T5's CombatCtx spawn helper),
**T10** (adds `BlockBrokenEvent.team`, which T1 change 9 can then use — if T10 lands after T1,
update T1's broken-event branch in the T10 commit), **T8**, **T9**, **T11**.

Hot files shared with other H plans (keep hunks small, merge `main` often):
* `core/sim/mod.rs` (`tick`, `apply_rules_to_world`, `flush_events`) — H00, H04, H06.
* `core/sim/unit_runtime.rs` (`update_fires_and_puddles`, destroy path, unit death) — H01.
* `core/world/build.rs` `valid_place_at` (T2 change 5: per-tile check only) — H02.
* `core/world/blocks/defense/turrets/mod.rs` (T3 config field, T4 enabled gate) — H01/H02.
* `core/combat/damage/explosion.rs` (T7 owns `dynamic_explosion`) — H01.
* `core/world/blocks/storage/core_block.rs` (T9) — H02.
* `gdext/sim_host.rs` (new debug seams) — every plan; add funcs at the end of the
  `#[godot_api] impl` block to avoid conflicts.

## 4. Ledger changes (coordinator applies; executors never edit `chains.json`)

| Item | Change |
|---|---|
| PAR-0091 | `fixing` -> `verified` after T6 (with T5) in-engine evidence |
| new | "Erekir fog of war is never simulated: nothing is hidden, discovered or revealed by radar" (T1+T3), `seeded` -> `verified` |
| new | "Turrets and units target enemies hidden in fog; players build into undiscovered fog" (T2) |
| new | "Derelict/captured and outside-map-area buildings keep running; disabled turrets still fire" (T4) |
| new | "Fires never throw fireballs, burn units or read building contents" (T5) |
| new | "Destroyed buildings explode too weakly; explosions have no lightning/fireballs" (T7) |
| new | "Enemy cores are never filled (fillItems)" (T8) |
| new | "Erekir cores store non-buildable items" (T9) |
| new | "Game-over/planet stats never count builds, breaks, losses or produced units" (T10) |
| new | "Explored fog is lost on save/resume" (T11) |

## 5. Done criteria

* Every task committed with its tests; `cargo test -p mind-core --lib --features msav-import`
  = baseline + new tests, 0 failed; `mind-headless --lib` per the known-failure list;
  `parity goldens` all PASS (any regenerated golden named with its Java reason); clippy clean.
* In-engine evidence per task under `runs/<date>-s<N>-H09-T<n>/` (or the explicit
  headless-only reason the task allows); 0 panics after each step.
* Summary `REPO/docs/H09-gameplay-sweep_summary.md` per H-environment §6.

## 6. Forwarded findings (for the coordinator) and cross-area seams

Seams this plan provides: `FogControl` resource + `Sim::fog_visible_tile/fog_discovered`,
`crate::game::fog_gate::{unit_in_fog_to, building_in_fog_to, building_discovered}`,
`Building.was_visible/visible_flags` maintained for the default team (T1/T2), SimHost
`fog_debug_json`, `debug_set_fog`, `debug_set_building_team`, `debug_create_fire` (if added),
`crate::world::allow_update::check_allow_update` (T4), `DelayedExplosionActions` +
`dynamic_explosion(ctx, ..)` (T7), `notify_unit_created` (T10).

**For H01 (units/AI/combat)**
* `rules.dragMultiplier` + `floor.dragMultiplier` never applied: no drag integration (`J/entities/comp/UnitComp.java:790`; port `core/entities/comp/unit/environment.rs:10-11`, `core/entities/comp/unit/comp.rs:466` field never read).
* `rules.randomWaveAI` unused (`J/ai/Pathfinder.java:556`, `J/ai/types/FlyingAI.java:49`; port only sets it, `core/game/campaign_rules.rs:226`).
* `TeamRule.prebuildAi` unused (PAR-0156; `J/core/Logic.java:574-588`, `J/ai/types/BuilderAI.java:242`, `J/ai/BlockIndexer.java:123`).
* `Teams.updateTeamStats` not live: `present/units/buildings/unitCount` stale (`J/game/Teams.java:181-240`; port `core/game/teams.rs:423` called only from tests). H09 T1 computes its own team list.
* Do not touch `dynamic_explosion`: H09 T7 rewrites it (lightning, fireballs, stagger); H01's unit-death terms (`UnitComp.java:892`) stay H01's.
* Fog: AI/command targeting already goes through `TargetQueries` — after H09 T2 any new H01 target query must call `fog_gate` (`Units.java:304,326`).
* `rules.unitPayloadsExplode` (`J/entities/comp/PayloadComp.java:71`) unused (default false, low).

**For H02 (blocks)**
* Turret ammo `rangeChange` absent: `BulletDef` has no `range_change`, so `Turret.range()` (`Turret.java:...range() = range + peekAmmo().rangeChange`) and fog radius ignore it (e.g. duo graphite `rangeChange = 16`, `J/content/Blocks.java:3295`; port bullet registry `core/content/registries/bullets.rs`).
* `rules.derelictRepair` never synced into `BuildRules.allow_derelict_repair` (`J/world/Build.java:95,254`, `BuildPlan.java:78`; port `core/world/limits.rs:32,58`).
* `rules.blockLimits` never synced into `BuildRules.block_limits` (`J/world/Block.java:1046`; port `core/world/limits.rs:78`, only tests insert).
* `rules.polygonCoreProtection` (`J/world/Build.java:191`), `rules.placeRangeCheck` (`Build.java:268`), `TeamRule.checkPlacement`, `buildRadius(team)` incl. `extraCoreBuildRadius` (`J/game/Teams.java:64`; port `core/game/rules.rs:160` has no caller) — all placement, unused.
* `Build.beginBreak/beginPlace` do not carry `prevPercent` health into the construct build (`J/world/Build.java:30-36,59`; port `core/sim/runtime.rs:545-597` sets none) — verify.
* `MassDriverBolt` (`J/entities/bullet/MassDriverBolt.java:102`) and `CoreBlock` destroy (`J/world/blocks/storage/CoreBlock.java:681`) `dynamicExplosion` calls: use H09 T7's new signature.
* H09 T9 owns `incinerateNonBuildable`; H09 T4 owns `Building.enabled` gating for turrets.
* `BuildVisibility.fogOnly` (radar) must follow `rules.fog` (`J/world/meta/BuildVisibility.java:21`; port `core/ui/campaign.rs:496` lists it) — picker side is H08.

**For H03 (rendering/HUD)**
* Fog rendering: consume `FogControl` (`read` = dynamic, `static_data` = discovered) via H09 T1's resource/probes; `Building.was_visible` drives `BlockRenderer` (`J/graphics/BlockRenderer.java:125,211,274,723-830`), minimap (`MinimapRenderer.java:158,380`); hide units with `fog_gate::unit_in_fog_to(player team)` (`UnitType.java:1473`).
* `rules.hideSpawns` (`J/graphics/MinimapRenderer.java:296`), `rules.unitLight` (`J/type/UnitType.java:1801`), `rules.cloudColor` (`J/world/blocks/storage/CoreBlock.java:460-474`), `rules.waveSending` (`J/ui/fragments/HudFragment.java:1235`) are unconsumed (port: only set).
* `rules.hideBannedBlocks` (`J/world/Block.java:1032`) in the block picker.

**For H04 (saves + campaign loop)**
* Core-full items still count for `SectorInfo.handleCoreItem` in Java — fixed by H09 T9 (`J/world/blocks/storage/CoreBlock.java:891-893`); H04's SectorInfo production numbers change accordingly.
* `static-fog-data` save chunk is H09 T11; H04's sector save/resume path must pass it through (seam: `DecodedSave.static_fog`).
* Planet `CampaignStats` counters are H09 T10 (H04/H05 only read them).

**For H05 (campaign UI/meta)**
* Planet stats dialog reads `CampaignStats` — populated by H09 T10.
* Generated Erekir sectors set `placeRangeCheck = true` and `hideSpawns = false` (`J/maps/planet/ErekirPlanetGenerator.java:425,432`) — PAR-0112 scope.

**For H06 (logic)**
* `setrule mapArea` must call `World.checkMapArea(prev..)` (`J/logic/LExecutor.java:1862-1882`): with H09 T4 this happens inside `Sim::apply_rules_to_world` when `rules_world_dirty` is drained — H06 must keep routing through `rules_world_dirty` (no direct `grid.limit_area` writes).
* `control enabled` on turrets now works (H09 T4 change 3); `autoResetEnabled` field added by T4.

**For H07 (audio/weather)**
* Construct/destroy sounds are fog-gated in Java (`J/world/blocks/ConstructBlock.java:63,113,148`, `J/world/Build.java:110`; port comment `core/sim/mod.rs` ~1805 "fog is G03"): use `Sim::fog_visible_tile(default_team, x, y)` from H09 T1. `TankComp/MechComp/LegsComp` step sounds/effects also check `inFogTo(player.team())`.
* Fire loop sound (`FireComp.java:49-51`) — H07 if not ported.

**For H08 (desktop input)**
* `rules.onlyDepositCore` (Erekir sets it): `BuildingComp.allowDeposit` (`J/entities/comp/BuildingComp.java:745`) unused — item deposit into non-core blocks must be refused on Erekir.
* `rules.itemDepositCooldown` (`J/input/InputHandler.java:2303-2313`), `rules.possessionAllowed` (`J/input/DesktopInput.java:423`), `rules.schematicsAllowed` (`J/ui/dialogs/SchematicsDialog.java:124,192`), `rules.pauseDisabled` (`J/ui/fragments/HudFragment.java:409-417`, `J/core/Control.java:253,620`) unconsumed.
* Desktop attack cursor uses `cursor.build.inFogTo(player.team())` (`J/input/DesktopInput.java:872`) and unit tap selection skips fogged units (`J/input/InputHandler.java:2193`): use H09 T2's `fog_gate`.
* Radar (`BuildVisibility.fogOnly`) must be hidden from the picker when `!rules.fog`.
