<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I01 — Simulation residuals: bugs and deviations left by round H (units, blocks, world, objectives)

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan.

Round H's H01 (units/AI/combat), H02 (blocks), H04 (saves/campaign loop) and H09 (gameplay
sweep) are complete. Their executors listed deviations and blockers they did not fix; the plan
writer probed the live client on 2026-10-15 and found two more bugs. This plan fixes all of them.
Each task is independent unless it says otherwise.

Probe results that need NO task (recorded so nobody re-investigates):
* "Left-click on ore latches the mine tile but the running sim clears it" (H08 pass 2): NOT
  reproduced on 2026-10-15. A real click on copper ore (133,50) at Ground Zero latched
  `player_mine_tile() = [133,50]`, the mining beam drew, and copper reached the core. H08 most
  likely clicked ore that already had a drill on it, or probed while paused.
* "Clicking a drill onto ore queues no plan" (H08 V1.1): NOT reproduced. A real click placed a
  `mechanical-drill` at (124,46) (log: the first click queued it; a second click on the occupied
  tile logged `player plan rejected`).
* "Units spawned through `apply_sim_command_json` vanish within a tick" (H09 pass 1/2): NOT
  reproduced. Team-1 and team-2 daggers spawned at (1100,480)/(1150,560) lived and fought.
  If you see it again, check the map-area kill boundary (units outside `limit_area` die at once;
  Ground Zero's limit area is x 90..160, y 20..80) before filing anything.
* Duo range: H03 quoted "Java 110" for the duo. Java is `range = 160` (`J/content/Blocks.java:3337`),
  and the port matches. No bug.

---

## 1. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5 -> T6 -> T7 -> T8 -> T9 -> T10 -> T11 -> T12 -> T13 -> T14 -> T15 -> T16.

### T1 — The CoreItem objective counts items delivered to the core, not items stored

**Evidence (plan writer, 2026-10-15):** the Ground Zero objective panel reads
`Move into Core: 1643/5 Copper` while the objective stays incomplete. Java shows
`stats.coreItemCount` (copper delivered by conveyors), which is 0 there, so the panel should read `0/5`.

**Java.** `CoreItemObjective` (`J/game/MapObjectives.java:392-411`): `update()` =
`state.stats.coreItemCount.get(item) >= amount`; `text()` =
`Core.bundle.format("objective.coreitem", state.stats.coreItemCount.get(item), amount, item.emoji() + " ", item.localizedName)`.
`coreItemCount` is incremented ONLY by `CoreBuild.handleItem` (`J/world/blocks/storage/CoreBlock.java:886-890`,
team == defaultTeam). `handleStack` (player/unit deposits, mining offload) does NOT increment it (`:787-801`).

**Port today.** `core/game/map_objectives.rs:590-600`: the text uses `locale.item_count(item)` (the
stored core count), with a stale comment "until `stats.coreItemCount` lands". The update already
uses `env.core_item_count(item)` (`:492-496`, `core/game/runtime.rs:802`), which is correct.

**Changes.** Add `fn core_item_count(&self, item) -> i32` to the text locale trait (default 0) next
to `item_count` (`map_objectives.rs:63`), implement it where `item_count` is implemented for the live
runtime (read `stats.core_item_count`), and use it in the `CoreItem` text. Keep `item_count` for the
`Item` objective (Java `ItemObjective.text` uses `state.rules.defaultTeam.items().get(item)`; check
`J/game/MapObjectives.java` `ItemObjective`). Delete the stale comment.

**Tests.** `core_item_objective_text_uses_delivered_count` (stats 3, stored 1643 -> text has `3/5`).

**Verify.** Ground Zero with drills not connected by conveyor: the panel reads `0/5`; connect a
conveyor from a drill to the core -> the count rises (screenshot).

**Commit.** `Gameplay I01 T1: CoreItem objective text shows the delivered-item count`

---

### T2 — Closed doors are solid (`Building.checkSolid`)

**Java.** `Tile.solid()` (`J/world/Tile.java:462-464`) =
`block.solid || floor.solid || (build != null && build.checkSolid())`. `Door`
(`J/world/blocks/defense/Door.java`): `solid = false; solidifes = true;` and
`DoorBuild.checkSolid()` = `!open` (`:143-145`). `BuildingComp.checkSolid()` default false (`:1739`).
`solidifes` matters for placement under units (`Build.validPlace` checks units only when
`type.solid || type.solidifes`).

**Port today.** `core/world/tile.rs:131-135` `solid_full` documents that `checkSolid` has no override;
the door spec (`core/content/registries/blocks/defense.rs:~195-203`) has no `solid`, so a closed
door never blocks units or ground pathing.

**Changes.**
1. A per-tile "dynamic solid" bit: the door behaviour knows `open`. Add `fn check_solid(&self, world, e) -> bool`
   to `BuildingBehavior` (default false) and implement it for doors (`!open`). Because `WorldGrid`
   tiles do not see ECS state, mirror it into the grid: when a door's `open` changes (its configure
   path and its creation), set a `Tile` flag `dynamic_solid` (add the field or reuse a spare flag;
   `grep -n "pub struct Tile" core/world/tile.rs`), and make `Tile::solid`/`solid_full` honour it.
2. Pathfinder packing (`core/ai/pathfinder/path_tile.rs`) and EntityCollisions (the velocity
   integration's solid test, `entities::comp::unit::velocity`) read `solid_full`, so they pick it up;
   check every `fn solid` caller with `grep -rn "\.solid(\|solid_full(" core/` and confirm each one
   wants Java's `Tile.solid()`. Bump the pathfinder's tile revision when a door toggles so flow fields
   refresh (`grep -n "fn update_tile\|revision" core/ai/pathfinder/mod.rs`).
3. `solidifes`: add `BlockDef.solidifies` (true for doors) and use it in the placement unit-overlap
   check (`sim/builder.rs:348` comments the Java short-circuit).

**Tests.** `closed_door_blocks_ground_unit`, `open_door_lets_unit_pass`,
`door_toggle_updates_flowfield`.

**Verify.** Ground Zero: a row of walls with a door between them; a team-2 dagger spawned on one side
with the core on the other: door closed -> it attacks the door; open (`configure_building(x,y,"bool","true")`)
-> it walks through.

**Commit.** `Gameplay I01 T2: closed doors are solid for units and pathing (Building.checkSolid)`

---

### T3 — Line damage uses Java tile rounding (`World.toTile`)

**Java.** `Damage.collideLine`/`linecast` (`J/entities/Damage.java:149,170,189`) use
`World.toTile(v) = Math.round(v / 8)` (`J/core/World.java:174-176`).
**Port today.** `core/combat/damage/line.rs:21` `to_tile` floors (`(value / 8).floor()`), so lasers and
line damage pick the wrong tile at tile boundaries (H01 T1 deviation 1).
**Changes.** Use `crate::world::WorldGrid::to_tile` (or `f32::round`) exactly like H01 T1 did for
`collide_bullet`; fix the doc comment ("tile centers are `x.5`" is wrong since H01 T1).
**Tests.** `collide_line_rounds_like_world_to_tile` (a laser ending at x = 8*10 + 4.1 hits tile 11).
**Commit.** `Gameplay I01 T3: Damage.collideLine uses World.toTile rounding`

---

### T4 — Pathfinding costs: live building health, checkSolid, weighted control paths

**Java.** `Pathfinder` costs (`J/ai/Pathfinder.java`, `costGround` etc.) read
`PathTile.health` packed from the LIVE `build.health` (`Pathfinder.java:270`), not the max.
`ControlPathfinder.costGround` (`J/ai/ControlPathfinder.java:33-42`) is a weighted cost
(`otherTeam solid -> wallImpassableCap`, deep water, near-solid penalties) that the A* uses.
**Port today.** `core/ai/pathfinder/path_tile.rs` packs the block def's max health (H01 T15 deviation 4;
the packing has no ECS access); `core/ai/control_pathfinder.rs` only computes passability (booleans),
not the weights (deviation 7).
**Changes.**
1. Packing: give the packer a health lookup. When a building takes damage or heals, push the new
   health into the grid snapshot the pathfinder reads (a `building_health` field on `Tile` updated by
   the damage/heal helpers, or a side map keyed by tile index that `path_tile` reads). Re-pack the
   tile and bump the revision on change, rate-limited to whole-percent changes so a turret firing at a
   wall does not rebuild flow fields every tick (Java repacks on `BuildDamageEvent`? check
   `grep -n "updateTile\|BuildDamageEvent\|Events.on" J/ai/Pathfinder.java` and follow Java exactly).
2. `ControlPathfinder`: make the A* take costs (`astar.rs` already exists in `core/ai/`; check whether it
   accepts a cost closure) and pass Java's `costGround`/`costLegs`/`costNaval`.
**Tests.** `damaged_wall_is_cheaper_to_path_through`; `control_path_prefers_shallow_over_deep`.
**Commit.** `Gameplay I01 T4: pathfinder packs live building health; control paths use Java costs`

---

### T5 — `BulletType.rangeChange` (ammo changes turret range)

**Java.** `Turret.TurretBuild.range()` (`J/world/blocks/defense/turrets/Turret.java:348-353`):
`range + peekAmmo().rangeChange` when ammo is loaded. Used by targeting, the range circle, fog radius
(`Turret.java:426-428`) and `estimateStats`. Example: duo graphite ammo `rangeChange = 16f`
(`J/content/Blocks.java:3295`).
**Port today.** `BulletDef` has no `range_change` (`core/sim/fog.rs:527` comment; H01 T10 dev 1;
H09 dev 7).
**Changes.** `BulletDef.range_change: f32` (content data for every bullet that sets it:
`grep -n "rangeChange" J/content/Blocks.java J/content/UnitTypes.java`), a `fn turret_range(world, e) -> f32`
helper returning `config.range + peek_ammo.range_change`, and use it in turret targeting, the
selection range circle (I02's renderer reads the same helper), turret fog radius, logic `@range`
(I03 T3) and `estimateStats` DPS rows. Regenerate `parity/golden_content.json` if it dumps bullets
(env §2).
**Tests.** `duo_graphite_range_is_176`.
**Commit.** `Gameplay I01 T5: BulletType.rangeChange extends turret range per loaded ammo`

---

### T6 — Area damage honours air/ground filters

**Java.** `Damage.damage(team, x, y, radius, damage, complete, air, ground, scaled, source)`
(`J/entities/Damage.java`, grep `public static void damage(`): units are hit only when
`(unit.isFlying() && air) || (unit.isGrounded() && ground)`; buildings only when `ground`.
Bullet splash passes `collidesAir`/`collidesGround`; the logic `explosion` instruction passes its own
flags.
**Port today.** `core/combat/damage/area.rs:136` `damage_area` / `DamageOptions` has no air/ground
filters (H06 T8 deviation: the logic explosion ignores them).
**Changes.** Add `air: bool, ground: bool` to `DamageOptions` (default true/true), filter in
`damage_area`, and pass the bullet's flags from every splash call site and the logic explosion's flags.
**Tests.** `ground_only_splash_spares_flyers`, `logic_explosion_air_only`.
**Commit.** `Gameplay I01 T6: area damage honours air/ground filters`

---

### T7 — Erekir cores require the core zone (`requiresCoreZone`)

**Java.** `core-bastion`, `core-citadel`, `core-acropolis` set `requiresCoreZone = true`
(`J/content/Blocks.java:3198,3218,3236`); `CoreBlock.canPlaceOn` (`J/world/blocks/storage/CoreBlock.java`,
grep `requiresCoreZone`) then requires the core-zone floor under the whole footprint (in addition to the
upgrade rules).
**Port today.** H02 T4 treated `requiresCoreZone` as false, believing the port had no Erekir cores;
`core-bastion` exists (`core/content/registries/blocks/storage.rs:124`).
**Changes.** `BlockDef.requires_core_zone` (true for the three), and the `canPlaceOn` rule in the H02 T4
code path (`grep -n "fn can_place_on" core/world/`).
**Tests.** `bastion_requires_core_zone`.
**Commit.** `Gameplay I01 T7: Erekir cores require the core zone`

---

### T8 — `ignoreBuildDarkness`, dead `BeamDrillState`, map markers decision (was H04 T12)

1. `ignoreBuildDarkness` (`J/world/Block.java:377`; true in `J/world/blocks/environment/Floor.java:116`,
   `StaticWall.java:35`, `RemoveWall.java:19`, `RemoveOre.java:19`; used in `J/world/Build.java:229`
   and `:257` `!check.floor().placeableOn && !type.ignoreBuildDarkness`): add
   `BlockDef.ignore_build_darkness` (true for floors, static walls, ores, remove-wall, remove-ore) and
   skip the darkness and solid-floor checks in `valid_place` (`core/world/build.rs`) when set.
   Test `env_blocks_ignore_build_darkness`. Regenerate the content golden if it changes.
2. `BeamDrillState` (`core/entities/comp/building.rs`) is never inserted (the beam-drill codec uses
   `DrillState`). Delete it and its re-export (`core/entities/comp/mod.rs`) after
   `grep -rn "BeamDrillState" client/rust` shows only the definition, the re-export and a comment.
3. Map markers region: measure first. Add a test that loads every `assets/maps/serpulo/*.msav` through
   the legacy reader with a counting hook in `skip_legacy_region(REGION_MARKERS)` and prints/asserts
   the region byte length. If every region is an empty UBJson map, keep the skip with the test as a
   guard ("Java presets ship no markers; objective markers live in rules objectives") and stop.
   Otherwise decode with `core/io/json/ubjson.rs::read_ubjson`, convert the `IntMap` entries to
   `ObjectiveMarker` JSON (`marker_name_to_type`, `core/game/map_markers.rs`) and feed `MapMarkers`.
**Commit.** `Gameplay I01 T8: ignoreBuildDarkness, drop dead BeamDrillState, markers region decision`

---

### T9 — Coolant and heat consumers read the current liquid

**Java.** `ConsumeCoolant` / `ConsumeLiquidFilter` (`J/world/consumers/ConsumeLiquidFilter.java`,
`getConsumed(build)` = `build.liquids.current()` when it matches the filter, else the first matching
liquid... read it exactly) and the Erekir heat-coolant pick.
**Port today.** `core/world/update.rs:615` `first_available_liquid` (the generic coolant
optional-efficiency path) and the Erekir heat-coolant pick (`core/world/blocks/power/behavior.rs`, grep
`first_available_liquid` or the H02 T10 note) still take the first non-zero slot. H02 T10 added
`LiquidModule::current_liquid()` / `current_liquid_amount()`.
**Changes.** Port `ConsumeLiquidFilter.getConsumed` exactly and use it in both places.
**Tests.** `coolant_uses_current_liquid` (a turret holding water then cryofluid uses the current one).
**Commit.** `Gameplay I01 T9: liquid-filter consumers use the current liquid`

---

### T10 — Unit ids survive save/load; cargo loader save body

**Java.** `UnitCargoLoader.read` (`J/world/blocks/units/UnitCargoLoader.java:174-185`) stores
`readUnitId` and resolves it in `updateTile` with `Groups.unit.getByID(readUnitId)` (`:84-103`).
Java unit ids are saved with the unit entity (`EntityComp.id`) and restored on load.
**Port today.** `LiveEntitySink::read_entity` discards the saved entity id (`core/io/sim_io.rs`) and
`Sim::apply_decoded_entities` respawns units with fresh `EntitySeq`s, so no id->entity map exists;
the cargo loader body is unimplemented (H02 T13 blocker), so a save with a cargo loader misreads the
following building.
**Changes.**
1. Save the unit's `EntitySeq` id in the unit chunk the way Java writes `id` (check the port's unit
   chunk writer and the Java field order in the generated `write` for unit classes; H04 T10 checked
   these tables), restore it on load (`EntitySeq` must then never collide: bump the sequence counter
   past the max restored id), and add `pub fn unit_by_id(world, id) -> Option<Entity>`.
2. The cargo loader body: `read_unit_id` stored on read, resolved in `update_tile` via `unit_by_id`.
**Tests.** `unit_ids_round_trip_through_save`, `cargo_loader_relinks_unit_after_load`,
`save_with_cargo_loader_reads_next_building`.
**Commit.** `Gameplay I01 T10: unit ids persist across saves; cargo loader save body`

---

### T11 — Unit assembler: save body and drone arrangement

**Java.** `UnitAssembler` (`J/world/blocks/units/UnitAssembler.java`): save body (`:756-789`: `progress`,
the assembled `units` id list, the `blocks` `PayloadSeq`, `commandPos`); drone arrangement
(`:509-513`: each drone's `UnitAssemblerAI` target position/angle around the block, and the
efficiency term that requires every drone `inPosition`).
**Port today.** The `UnitAssembler` state has no PayloadSeq/units/commandPos model (H02 T13 blocker);
`AssemblerAI` dispatch landed (H01 T13) but drones hold the zero target (H01 T13 dev 5: "seam
request: write `AssemblerAi.target_x/target_y/target_angle` per drone and use
`ai::types::assembler::in_position` in the efficiency term").
**Changes.** A `PayloadSeq` type (ordered (content, amount) list with Java's `write`/`read`), the
assembled units list (needs T10's ids), `command_pos`; the save body; the drone arrangement and the
efficiency term per Java.
**Tests.** `assembler_save_round_trip`, `assembler_drones_take_positions`,
`assembler_waits_for_drones_in_position`.
**Commit.** `Gameplay I01 T11: unit assembler save body and drone arrangement`

---

### T12 — Turret block units and building control select (PAR-0138 turret half)

**Java.** `Turret.TurretBuild implements ControlBlock` (`J/world/blocks/defense/turrets/Turret.java:283`):
`public BlockUnitc unit = (BlockUnitc)UnitTypes.block.create(team);` positioned on the turret every
update; `canControl()` = `playerControllable` (default true); `isControlled()` = `unit.isPlayer()`;
while controlled the turret aims at `unit.aimX/aimY` and shoots when `unit.isShooting()`
(`updateTile`, the `isControlled()` branches). `InputHandler.selectedUnit()` (IH:2148-2163) returns
`cont.unit()` for an own ControlBlock under the cursor. `BuildTurret`, `Router` (`size == 1`) are also
ControlBlocks. `CoreBuild.canControlSelect(player)` / `onControlSelect` (`CoreBlock.java:619-...`):
Ctrl-click on the own core respawns the player at it.
**Port today.** A `BlockUnit` unit component exists (`core/entities/comp/unit/defs.rs:86`,
`lifecycle.rs:298`) but turrets never create one. `desktop.rs` sends `BuildingControlSelect` for turrets,
and the sim only stores `control_building`. `MindSimHost.control_block_unit_id` does not exist.
**Changes.**
1. Turrets (and BuildTurret, size-1 Router) create a `block` unit on placement (team, position each
   tick, removed with the building; never saved as a separate unit: check `UnitTypes.block`
   `J/content/UnitTypes.java` for its flags).
2. `#[func]`-free `pub fn control_block_unit_id(&mut self, x, y) -> Option<i32>` on `MindSimHost` and
   the possession path: `UnitControl` on that unit makes it the player unit; the turret's
   `is_controlled` branch aims at the player's aim point and shoots with the player's shoot flag
   (`gdext/input/desktop.rs` `selected_unit_for_control` second branch, H08 T8's skipped half).
3. Core control select: Ctrl-click the own core -> `onControlSelect` respawn (clear the player unit,
   respawn at the core).
**Tests.** `turret_has_block_unit`, `controlled_turret_shoots_at_aim`, `core_control_select_respawns`.
**Verify.** Ground Zero: Ctrl-click an own duo -> the player becomes the turret (HUD player type `block`),
mouse aim rotates the duo, LMB fires; Ctrl-click the core -> respawn. Screenshots.
**Commit.** `Fixes PAR-0138: turrets carry block units the player can control; core control select respawns`

---

### T13 — Middle-click command queue reaches the sim (`CommandAI.commandQueue`)

**Java.** `InputHandler.commandTap`/`commandUnits(..., queue)` with `Binding.commandQueue` (middle mouse
by default) appends to `CommandAI.commandQueue` instead of replacing the target; the AI pops the next
position when it arrives (`J/ai/types/CommandAI.java`, grep `commandQueue`).
**Port today.** The input recognises the queue modifier (`desktop.rs:1595 command_tap_with_queue`), but
the wire drops `queue`, so a queued order applies like a direct one (H08 T6b deviation). The sim has a
`command_queue` in its state (`core/sim/command.rs:281`).
**Changes.** Carry `queue: bool` on `SimCommand::UnitCommand` (append-only binary field; bump the
command log format if one is versioned) and in `apply_sim_command_json`; apply as Java (append to the
queue; `CommandAI` pops in order).
**Tests.** `queued_orders_execute_in_sequence`.
**Commit.** `Gameplay I01 T13: middle-click queued unit orders execute in sequence`

---

### T14 — Power graph keeps its links when a building is replaced

**Evidence (H03 pass 2, 2026-10-15):** replacing a sandbox `power-source` (or breaking/replacing a
multiblock near one) can leave a consumer on a separate graph with `satisfaction 0`. Staging order that
works: machine -> `power-node` -> `power-source`.
**Java.** `PowerGraph`/`PowerNode` linking on `placed()`/`onProximityUpdate`/`onRemoved`
(`J/world/blocks/power/PowerNode.java` `getNodeLinks`, `placed()`, `J/world/blocks/power/PowerGraph.java`
`reflow`/`remove`).
**Changes.** First write a failing test reproducing the report:
`power_source_replacement_keeps_consumer_powered` (consumer + node + source; break and re-place the
source; after 2 ticks the consumer's satisfaction is 1). Then find where the port rebuilds graphs on
removal/placement (`core/world/blocks/power/`) and fix it to match Java's `PowerNode.placed()` auto-link
and graph reflow.
**Commit.** `Gameplay I01 T14: power graphs relink when a building is replaced`

---

### T15 — `fog_debug_json` stalls the game on Ground Zero

**Evidence (plan writer, 2026-10-15):** `$H.fog_debug_json()` on Ground Zero (fog off) did not
return within the 15 s eval budget and stalled the frame loop (recovered with `--play`).
**Changes.** Make it O(1)-ish: return counts and the controller state, not per-tile arrays, unless
called with an explicit small rect (`fog_debug_json_rect(x, y, w, h)`). Check `debug_set_fog`
and `fog_static_bytes` for the same problem.
**Commit.** `Gameplay I01 T15: fog debug readbacks stay cheap on large maps`

---

### T16 — Verification sweep (no code unless a check fails)

Run each check; file a new task in your summary for each failure (with the evidence).
1. Ground Zero wave 2 (one dagger + two flares): drill-fed duo at (132,48) plus a second duo at (134,48);
   `run_wave()` twice (clean `--import` first): by +6000 ticks the core is alive and the enemies are dead.
   (H00 T3 recorded a single flare destroying the duo, drill and core before H02 T16's turret lead and
   H01's AI work landed.)
2. Sandstorm wind (H07 T13, covered by H01 T4): `debug_create_weather("sandstorm", ...)`; a dagger drifts
   ~8.8 px per 600 ticks (H01 pass-2 evidence `runs/20261014-s5-H01-T4/probe.json`).
3. BuilderAI `updateBoosting`/`moving` tail (`J/ai/types/BuilderAI.java:231-233`): a poly building far
   away boosts like Java (headless test if it does not).
4. PAR-0168 ("Enemy base-builder AI queues no plans (units_base_build golden: 0 of 28)", still `seeded`;
   its sibling PAR-0169 was verified 2026-10-13): run the `units_base_build` golden/scenario
   (`cargo run -q -p mind-headless -- list | grep -i base`) and report the plan count; if it is still 0,
   compare `BaseBuilderAI.update` (`J/ai/types/BaseBuilderAI.java`) with `core/ai/base_builder_ai.rs` and fix.
**Commit.** none unless a fix is needed (`Gameplay I01 T16: <fix>`).

---

## 2. Hot files

`core/sim/unit_runtime.rs`, `core/world/tile.rs` (T2), `core/ai/pathfinder/*` (T2, T4),
`core/combat/damage/*` (T3, T6), `core/world/blocks/defense/turrets/mod.rs` (T5, T12),
`core/io/sim_io.rs` (T10), `gdext/input/desktop.rs` (T12, T13). Merge `main` before each of these tasks.

## 3. Ledger changes to report

| Item | After |
|---|---|
| PAR-0138 | verified (T12 turret half; the payload half was verified by H08) |
| new | "CoreItem objective shows stored items" -> verified (T1) |
| new | "Closed doors are not solid" -> verified (T2) |
| new | "Ammo rangeChange ignored" -> verified (T5) |
| new | "Area damage ignores air/ground" -> verified (T6) |
| new | "Erekir cores ignore core zone" -> verified (T7) |
| new | "Unit ids lost on save; cargo loader/assembler bodies" -> verified (T10, T11) |
| new | "Queued unit orders apply as direct orders" -> verified (T13) |
| new | "Power graph loses links on re-placement" -> verified (T14) |

## 4. Done criteria

All tasks committed with their tests; suites green; summary in `REPO/docs/I01-sim-residuals_summary.md`.
