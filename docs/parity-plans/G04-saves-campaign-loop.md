<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G04 — Saves + the campaign gameplay loop (plan, 2026-10-13)

Self-contained execution plan. Read `docs/parity-plans/00-environment.md`
first (session/worktree/build/test/gmcp rules; this plan only references its
sections). Work the tasks in the §3 order, one commit per task. If reality
differs from what a task states (a symbol is missing, a line moved so far that
the quoted function no longer matches, a test value differs from the derived
one), stop that task, write down what you saw, and move to the next task that
does not depend on it.

Precondition line: "Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line)".

Abbreviations: `core/`, `gdext/`, `headless/`, `ui/`, `scenes/`, `J/`, `ARC/`
as defined in `00-environment.md`. `JA/` = `/home/clintonnguyen/g/code_examples/Mindustry/annotations/src/main/`
(Java entity revision manifests, read-only). Line numbers were checked on
2026-10-13 against the integration tree; port lines drift, so search for the
quoted function name with `grep -n` when a line does not match.

---

## §1. Inventory (Java -> port -> status)

Status legend: **W** ported & wired, **NW** ported but not wired into the live
client, **P** partial, **M** missing, **X** obsolete (Java no longer has it).

| Java | Port | Status | Evidence (checked 2026-10-13) | Task |
|---|---|---|---|---|
| `J/io/SaveVersion.java` meta region (`writeMeta`/`readMeta` 113-196) | `core/io/sim_io.rs` `live_meta_tags`; `core/io/save/versions/v1.rs` `read_meta` | P | Written correctly. Read side: only the Load-Game path (`SimIoHandler::load`, `apply_loaded_meta` in `gdext/campaign.rs`) applies `wave/wavetime/tick/stats`. The **sector resume** path (`gdext/sim_host.rs` `load_grid_file`) ignores all tags: `wavetime` resets to 0, `tick` restarts, `stats` lost | T2 |
| `SaveVersion.readRules` (197-211: rules replaced wholesale, `sector.planet.applyRules`) | `gdext/campaign.rs` `merge_map_rules` (~2872) | P | Copies only `spawns, wave_timer, wave_sending, objectives, objective_flags, teams` (+ waves/winWave/attack/spacing for non-preset). Drops `limitMapArea/limitX..`, `borderDarkness`, `weather` cooldowns, `lighting/ambientLight`, `unitCap`, everything else. This is the rendering audit's blocker 1 (`limit_area = null`) | T5 |
| `SaveVersion.writeMap/readMap` (213-387) tile pass | `v1.rs` `write_map`/`read_map_body` | W | RLE + tile data identical | — |
| `readMap` building chunk `tile.build.readAll(in, revision)` (367-371) | `editor/context.rs` `EditorContext::read_building` (240), `world/context.rs` `Context::read_building` (130) | **P (PAR-0161)** | Both decode only `readBase` into `DecodedBase` and DROP the per-kind body; `world::Context` also ignores `set_legacy_entities`, so Java `.msav` slots loaded through Load Game mis-parse even the base (per-kind revision byte used as base version) | T1 |
| `writeMap` building chunk `out.b(build.version()); build.writeAll(out)` (265-267) | `world/map_source.rs` `EcsMapSource::write_building` (87-110) | P | Writes a constant `3` (base version) instead of the per-kind revision and omits the base version byte inside `writeBase` (`world/building_io.rs` `write_base` 96-145). Per-kind revision is lost → future codec bumps cannot be read | T1 |
| `BuildingComp.writeBase/readBase` (185-282) | `world/building_io.rs` `write_base/read_base/read_base_legacy/apply_base` | W | Byte-exact (module bits, efficiency bytes, visibility v4) | — |
| Per-kind `Building.write/read` bodies (68 Java classes) | `BuildingBehavior::write/read` impls | P | 41 behaviors implement IO (inventory table in T1/T3); never invoked on load; several layouts differ from Java (core `commandPos`, PayloadBlock prefix for factories/reconstructors, payload blocks never read their payload); defense projectors, generators/reactors, heat producer, radar, stack conveyor, beam drill, repair turret, shield wall, target dummy have no codec | T1, T3a, T3b |
| `SaveVersion.writeTeamBlocks/readTeamBlocks` (389-420, 455+) | `v1.rs` + `sim_io.rs` `LiveEntitySource::team_plans`, `apply_team_plans` | W | Both save paths carry plans | — |
| `writeWorldEntities` (units: generated `EntityIO` per class) | `sim_io.rs` `SIM_UNIT_CLASS_ID 0xF0` rev 2 (name/team/x/y/rot/health/payloads) | P | Native: no status effects, shield, carried item stack, elevation, `spawnedByCore`, command/target, build plans. Java chunks (class ids 0..49, `JA/resources/classids.properties`) are skipped → every unit of an imported Java save is lost. **Sector resume never reads entities at all** (no sink in `load_grid_file`) → every unit, fire and puddle disappears on every resume, port saves included | T2, T6, T9 |
| `writeWorldEntities` count = `Groups.all.count(serialize) + Groups.unit.size()` | `LiveEntitySource::entity_count` | P | counts `unit_count()` but `write_entities` `continue`s on units lacking `UnitCore/UnitTypeComp/Pos`/content name → count can exceed the chunks written (combat audit T13) | T9 |
| `FireComp`/`PuddleComp` save (class 10/13) | `sim_io.rs` 0xF1/0xF2 | P | Native OK (PAR-0090 verified); Java class 10/13 chunks skipped; resume path drops them (T2) | T2, T6 |
| markers region (`state.markers`) | `MapMarkers` sink | P | Load-Game path reads; sector resume path passes no sink → markers lost | T2 |
| custom chunks | `read_custom_chunks` | W (no vanilla chunk) | — | — |
| `J/game/Saves.java` slots/autosave/`saveSector` | `core/game/saves.rs`, `gdext/campaign.rs` `pump_runtime` | W/P | Autosave on `saveinterval` verified (PAR-0153). Missing: `Logic.sectorCapture` → `control.saves.saveSector` (save on capture, `Logic.java:398-401`). Previews are G05 | T7 |
| `J/core/Logic.java` `update` wave timer (553-565) | `core/game/runtime.rs` `campaign_tick_system` (338-375) | **P (bug)** | `wavetime -= 1/60` per tick although `wavetime` is in ticks (HUD formats it as ticks, `wave_spacing = 2*TIME_TO_MINUTES = 7200`) → wave countdown runs **60x too slowly**. `isWaitingWave` (`waitEnemies` / win-wave hold, 621-624) missing. `runWave` only fires inside the `waveTimer` branch (Java: `wavetime <= 0 && waves` regardless of `waveTimer`) | T3w |
| `Logic.checkGameState` (326-375) | `core/game/play.rs` `check_game_state*` | W | Transcribed 1:1; runs only when `rules.canGameOver` in Java (`runStateCheck`, 521) — port always runs it | T7 |
| `Logic.play` (269-298) | `play.rs` `logic_play` + `apply_launch_loadout` | P | Loadout not capped by `storageCapacity`; cores not healed on start | T7 |
| `Logic.sectorCapture` (378-403) + `SectorCaptureEvent` listeners (`Logic.java:117-150`, `Control.java:176-185`) | `play.rs` `sector_capture`, `gdext/campaign.rs` `apply_capture_bookkeeping` | P | Missing: `waveTeam.data().destroyToDerelict()` (enemy buildings → derelict, 20% destroyed, enemy units killed) — `Teams::destroy_to_derelict` exists (`core/game/teams.rs:565`) but is never called live; missing save-on-capture | T7 |
| `Logic` `BlockDestroyEvent` → `coreDestroyClear` `timeDestroy` (182-186; `Teams.java:365-397`) | — | M | `rules.core_destroy_clear` parsed, never used; serpulo/erekir `ruleSetter` sets it true (`J/content/Planets.java:87,146`) | T7 |
| `Logic` `SaveLoadEvent` (73-91) / `WorldLoadEvent` (105-125) campaign rule overrides | — | M | `coreIncinerates`, `canGameOver`, `allowEditRules`, `infiniteResources=false`, `planet.enemyInfiniteItems` (default **true**) → wave team `infiniteResources/fillItems`, `enemyBuildSpeedMultiplier`, `enemyFactoryActivationDelay` (serpulo 7200) not applied | T5 |
| `Logic.update` `fillItems` / `prebuildAi` (535-559) | `team_ai_system` | P | prebuildAi = PAR-0156 (G01) | cross-area |
| `J/core/Control.java` `playSector` (431-560) | `gdext/campaign.rs` `start_sector_with_loadout`, `play.rs` `play_sector`/`play_new_sector` | P | Lost-sector replay verified (PAR-0084). Missing: `sector.preset.quietUnlock()` (444); `SectorInfo.write()` on resume (via `SaveLoadEvent`) puts `info.items` into the core → offline production/imports never reach the core | T8 |
| `Control` `Trigger.newGame` landing + `prebuildBase` (198-283) | camera landing (PAR-0131) | P | Thrusters/real-core hiding = G03; `prebuildBase` is Erekir-only (deferred) | cross-area / deferred |
| `Control` GameOverEvent → save (188-195) | `pump_runtime` | W | — | — |
| `J/game/Universe.java` `update`/`runTurn` (52-262) | `core/game/universe.rs` `run_turn`, `production_pass`, `invasion_pass` | P | Invasion of the **currently played** sector not applied to live rules (Java 233-243); waveMax uses `state.wave` when played. Turn RNG is `JavaRandom(turn+1)` (accepted deviation) | T8 |
| `J/maps/SectorDamage.java` | `core/maps/sector_damage.rs` `apply` | W | Java only exposes `apply(fraction)`, used by the lost-sector replay (`Control.java:482`). There is no `applyCalculatedDamage` in this Java version | X/— |
| `J/game/SectorInfo.java` fields | `core/io/json/rules.rs` `SectorInfo` (942-1004) | W | Field set identical to Java (old T16 list `wavesSurvived/sumHealth/...` does not exist in this Java version → **T16 obsolete**) | — |
| `SectorInfo.prepare` (227-280) | `core/game/sector.rs` `prepare` + `gdext persist_sector_record` | P | Missing `lastPresetName`, export clear when no active launch pad (264-269) | T8 |
| `SectorInfo.write` (196-225) | `sector.rs` `write` (454) | **wrong + NW** | Port writes params INTO the record (prepare semantics) and is never called; Java applies the record TO the state + core items | T8 |
| `SectorInfo.update/handleCoreItem/handleProduction` | `campaign_tick_system` | W | — | — |
| `handleItemExport/Import`, `importCooldownTimers`, `getImportRate` | `sector.rs` | NW | No producer (launch/landing pads inert) | T10, T11 |
| `J/game/MapObjectives.java` executor (136-296) + 15 objective kinds | `core/game/map_objectives.rs`, `runtime.rs` `objectives_tick_system`, `RuntimeObjectiveEnv` | P | (a) `completed` (and Timer `countup`) is not parsed/written → completion lost on every save/load (Java saves `completed:true`, verified in `runs/20261009-065416-ref-ground-zero/checkpoints/r03..r05`); (b) Timer adds `1/60` per tick vs `duration` in ticks → **60x too slow** (Ground Zero timers 300/180/180 ticks take 5/3/3 minutes); (c) env stubs: `unit_count`→0, `enemy_units_destroyed`→0, `block_at`→`None` (DestroyBlock(s) complete instantly), `command_mode_satisfied`→false | T4 |
| Map markers drawing | — | G03 | — | cross-area |
| `J/world/blocks/campaign/LaunchPad.java` (v8) | `core/world/behavior/campaign.rs` `LaunchPadBehavior` | M (stub) | Only increments `launch_time`; no item accept, no launch, no `LaunchPayload`, no export stat; writes version 0 (Java 1) | T10 |
| `LandingPad.java` | `LandingPadBehavior` | M (stub) | Config + cooldown only; no import timers, landing queue, liquid use, dump | T11 |
| `Accelerator.java` | `AcceleratorBehavior` | M (stub) | heat only | deferred (§2 end) |
| `J/type/SectorPreset.java` `rules = r -> r.winWave = captureWave` | `play.rs` `apply_sector_preset_rules` | W | No custom preset lambdas in `SectorPresets.java`; `startWaveTimeMultiplier` is unused in Java | — |
| `J/game/CampaignRules.java` + `Planet.init` defaults | `core/game/planet.rs` 178-188 | **P (wrong default)** | Port seeds `sector_invasion = allow_sector_invasion` and `legacy_launch_pads = allow_legacy_launch_pads` (both true for serpulo); Java `applyDefaultRules` only copies `campaignRuleDefaults` (fog/hideSpawns/rtsAI for erekir), so both default **false** | T8 |
| `Sector.near()` | `core/game/universe.rs` `Campaign::from_registry` 337-349 builds `planet.neighbors` from `PlanetGrid` | W | Old T11 is **done** (neighbors from the icosphere grid; `EmptyNeighborhood` only for gridless planets/tests) | — |
| Settings hub (PAR-0164) | `MindUi.settings` (`gdext/ui/ui_host.rs:58`), `MindCampaign.settings` (`gdext/campaign.rs:71`), `MindMods.settings` (`gdext/mods.rs:35`) | P | Three stores, stale snapshots; workarounds `live_setting_flag*` in campaign.rs 262-375 | T12 |
| PAR-0162 user schematics / PAR-0163 launch capacity | `gdext/campaign.rs` `load_schematics` (296), `launch_dialog_json` (2352) | W (fixed by `44b0f7b`) | `Schematics::load` walks `<data>/schematics/*.msch` and re-registers loadouts after the sort (`core/game/schematics.rs:521-553`); capacity via `launch_capacity_multiplier` | V1 (verify only) |
| `World.isInMapArea` / `getDarkness` border term / `Build.validPlace` darkness | `core/world/mod.rs` `limit_area` + `is_in_map_area`, `core/world/darkness.rs` `border_darkness` | NW | `limit_area` is never set outside tests; `is_in_map_area` unused; `valid_place` uses wall darkness only | T5 |

### 1.1 Already done / verified (do not redo)

* PAR-0153 autosave (`3a948bb`), PAR-0084 lost-sector replay, PAR-0090 fire/puddle native save, PAR-0081/0082 sector record persistence, PAR-0085 + PAR-0025/26/27 loadout (T1/T2 of campaign-import: `44b0f7b`, `fe9381b`), PAR-0131 zoom/fade/announce (`659146d`; remainder is G03), combat T4 unit payload revision 2 (`4b7067b`: `SIM_UNIT_REVISION = 2`, payload stack; v1 still loads).
* Sector neighbours (old T11): done in `Campaign::from_registry`.
* SectorInfo field set (old T16): identical to Java; nothing to add.
* Content id order (verified with a header parse of `runs/20261009-065416-ref-ground-zero/checkpoints/r05-sector-serpulo-170.msav`, a v13 Java save): items 22/22, blocks 447/447, liquids 11/11, statuses 23/23, unit commands/stances identical to the port order → raw content ids inside per-kind bodies are valid without remapping for vanilla content (T1 adds a guard test).
* Java sector save of the operator profile (`mindustry.save.full.tech.tree.zip` → `saves/sector-serpulo-15.msav`, imported as `sector-serpulo-170.msav`) is **MSAV version 7** (short chunks): 44 buildings (32 scrap-wall, 5 world-processor, 3 conveyor, 2 mechanical-drill, core-shard, scrap-wall-large), 1 entity (class 0 = alpha), meta `wave 1, wavetime 21600`.


---

## §2. Tasks

Task order of this section = §3 execution order. Each task is one commit.

### T1 — PAR-0161: per-kind building bodies survive save/load (native + Java)

**Symptom / acceptance.** Today every per-kind building state is lost on
every load (sector resume, Load Game, Java import): sorter/unloader/item-source
selections reset, bridges and mass drivers unlink, processors lose their code
and links, factories forget their plan and progress, drills/crafters restart
at 0 progress, turrets lose ammo/reload/rotation, construct sites lose
progress, conveyors keep the item count in the module but lose the belt
positions. After: a save → load round trip restores the per-kind fields for
every behavior that has a codec; an imported Java save restores them too.
Acceptance: the fixture test below passes (drill (135,51) progress 420.7119,
warmup 1.0; conveyor (133,55) carries one copper on its belt; core-shard
(129,55) holds copper 4000 and `command_pos == None`), plus the native
round-trip test; in engine a sorter configured to copper keeps copper after
`run_exit_save` + resume.

**Java.**
* `J/io/SaveVersion.java:257-269` (`writeMap`): at a center tile with a
  building: `writeBoolean(true)`, then a chunk containing
  `out.b(tile.build.version()); tile.build.writeAll(out);`.
* `SaveVersion.java:364-379` (`readMap`): `byte revision = in.b(); tile.build.readAll(in, revision);`
  (exception → "Failed to read tile entity of block"); a block without
  `hasBuilding()` skips the chunk.
* `J/entities/comp/BuildingComp.java:284-292`: `writeAll = writeBase(); write();`,
  `readAll(read, revision) = readBase(read); read(read, revision);`.
* `writeBase` (185-221) writes `health f, (rotation|0x80) b, team b, version b (3, or 4 when rules.fog && visibleFlags != 0), enabled b, moduleBitmask b, items?, power?, liquids?, [timeScale f, timeScaleDuration f], [lastDisabler.pos i], efficiency ub, optionalEfficiency ub, [visibleFlags l]`.
  So the Java chunk is: **[per-kind revision][base with embedded base version][per-kind body]**.
* `CoreBlock.java:916-931`: `version()=1`; `write: TypeIO.writeVecNullable(write, commandPos)`
  (= two floats, `NaN,NaN` for null, `TypeIO.java:998-1011`); `read: if(revision >= 1) commandPos = TypeIO.readVecNullable(read)`.

**Port today.**
* `core/world/map_source.rs:87-110` `EcsMapSource::write_building`: writes
  `chunk.b(3)` then `BuildingCodec::write` → `write_base` (no embedded base
  version byte, `core/world/building_io.rs:110-118` comment) + `behavior.write`.
  The per-kind revision is never written.
* `core/editor/context.rs:240-254` `EditorContext::read_building` (used by the
  sector/preset path `gdext/sim_host.rs:3561` `load_grid_file`) and
  `core/world/context.rs:130-141` `Context::read_building` (used by
  `SimIoHandler::load`, `core/io/sim_io.rs:671`) decode only `DecodedBase`; the
  remaining bytes of the length-prefixed chunk are dropped. `world::Context`
  does not override `set_legacy_entities` (`core/io/save/state.rs:185`), so a
  Java `.msav` loaded through Load Game reads the per-kind revision as the base
  version (wrong module bits, items lost).
* `core/sim/runtime.rs:762-835` `SimRuntime::materialize_buildings` applies
  only `building_io::apply_base`.
* `core/world/blocks/storage/core_block.rs:356-392` `CoreBehavior`: writes
  `bool present + f + f` and reads the same — **not** Java's `writeVecNullable`.
* Versions: native saves are `SaveV1` (`core/io/save/versions/v1.rs`,
  `version_array` in `core/io/save/version.rs`).

**Changes.**
1. `core/world/building_io.rs`:
   * Add to `DecodedBase` (keep `#[derive(Debug, Clone, Default, PartialEq)]`):
     ```rust
     /// Per-kind revision byte (`Building.version()`); `None` for native v1
     /// saves, which did not store it.
     pub kind_revision: Option<u8>,
     /// Remaining chunk bytes after `readBase`: the per-kind body (`Building.read`).
     pub body: Vec<u8>,
     ```
   * Add `pub fn write_all_java(world: &World, entity: Entity, w: &mut EntityWriter, fog: bool) -> Result<(), IoError>`:
     writes `w.ub(kind_version)` where `kind_version = behavior_instance(world, entity).map(|i| i.behavior.version(world, entity)).unwrap_or(0)`,
     then the same fields as `write_base` but with the base version byte
     (`3`, or `4` when `fog && visible_flags != 0`) written right after the
     team byte (exactly `BuildingComp.java:191-193`), then
     `inst.behavior.write(world, entity, w)`. Implement by adding a private
     `write_base_impl(world, entity, w, fog, embed_version: bool)` used by both
     `write_base` (embed=false, unchanged bytes) and `write_all_java` (embed=true).
   * Add `pub fn apply_body(world: &mut World, entity: Entity, decoded: &DecodedBase)`:
     if `decoded.body` is empty return; get `behavior_instance`; revision =
     `decoded.kind_revision.unwrap_or_else(|| inst.behavior.version(world, entity))`;
     `let mut r = WireReader::new(&decoded.body); inst.behavior.read(world, entity, &mut r, revision);`
     if `r.remaining() != 0` → `log::debug!("building body `{}` rev {}: {} byte(s) unread", inst.name, revision, r.remaining())`
     (debug, not warn: Java bodies of subclasses the port maps to a parent
     behavior legitimately leave bytes, e.g. Separator `seed`).
2. `core/editor/context.rs:240` and `core/world/context.rs:130` `read_building`:
   after the base read, store `decoded.body = reader.bytes(reader.remaining())?.to_vec();`
   and, on the legacy/Java layout path, `decoded.kind_revision = Some(version)`
   (the `version` argument IS the per-kind revision there). In the native-v1
   path leave `kind_revision = None`. Add `legacy_entities: bool` to
   `world::Context` and implement `set_legacy_entities` + the
   `read_base_legacy` branch exactly like `EditorContext` (copy 245-253).
3. Native format v2 (Java-identical tile-entity chunks):
   * New `core/io/save/versions/v2.rs` with `pub struct SaveV2;` whose
     `version()` returns 2, `write` = `SaveV1`'s write (call a shared
     `pub(super) fn write_regions(w, options)` you extract from `SaveV1::write`),
     and `read` = `SaveV1::read` body with one difference: before the region
     loop call `if let Some(ctx) = state.context.as_deref_mut() { ctx.set_legacy_entities(true); }`
     (the v2 chunk layout equals the legacy one, so the existing legacy branch
     of `read_building` handles it). Extract `SaveV1::read`'s loop into
     `pub(super) fn read_regions(r, state)` and call it from both.
   * `get_meta` identical to v1 (delegate).
   * `core/io/save/versions/mod.rs`: `pub mod v2;`. `version.rs`
     `version_array`: `vec![Arc::new(SaveV1), Arc::new(SaveV2)]`. Update the
     test `version_chain_is_append_only_native_v1` → expect len 2,
     `current_version() == 2`, `get_writer(1).is_some()`, `get_writer(2).is_some()`.
   * `EcsMapSource::write_building`: replace `chunk.b(3); BuildingCodec::write(...)`
     with `crate::world::building_io::write_all_java(self.ecs, entity, chunk, false)`.
     Keep the early `Ok(())` for P0 entities but see Pitfall 2.
   * `core/io/save/versions/v1.rs` `read_map_body`: an entity chunk of length 0
     at a center (P0 entity written empty) must be skipped, not fail: before
     `let version = chunk.ub()?;` add `if chunk_len == 0 { context.on_read_building(i); i += 1; continue; }`
     — careful: the surrounding loop increments `i` at the end; restructure
     with an `if chunk_len > 0 { ... }` block instead of `continue`.
4. `SimRuntime::materialize_buildings` (`core/sim/runtime.rs:762`): right after
   `building_io::apply_base(ecs, entity, decoded, max_health);` call
   `building_io::apply_body(ecs, entity, decoded);`. (It runs before the
   proximity/power pass, like Java where `readAll` runs during the map pass.)
5. `CoreBehavior::write/read` (`core_block.rs:360-392`): write
   `crate::io::typeio::codecs::write_vec_nullable(w, command)`; read
   `if revision >= 1 { if let Ok(v) = read_vec_nullable(r) { core.command_pos = v; } }`.
6. `gdext/sim_host.rs` `building_debug` (775): add keys so in-engine checks can
   see the restored bodies: `drill_progress`/`drill_warmup` (`DrillState`),
   `crafter_progress`/`crafter_warmup` (`CrafterState`), `turret_total_ammo`
   (`TurretState.total_ammo`; check the field name with
   `grep -n "pub total_ammo" client/rust/mind-core/src/world/blocks/defense/turrets/*.rs`).

**Tests.**
* Fixture: copy `REPO/runs/20261009-065416-ref-ground-zero/checkpoints/r05-sector-serpulo-170.msav`
  to `client/rust/mind-core/tests/fixtures/saves/java-gz-r05.msav` (18 933
  bytes, a Java v13 save written by the reference client).
* `client/rust/mind-core/tests/legacy_msav_import.rs` (feature-gated file):
  `java_save_restores_per_kind_bodies`:
  load the fixture through `EditorContext` like `load_map` does, then
  `let mut sim = Sim::new(1, 1, 1, BlockId::AIR, BlockId::AIR); sim.grid = grid; sim.install_block_runtime().unwrap(); sim.materialize_map_buildings(&pending);`
  (check the exact install fn name: `grep -n "pub fn install_block_runtime" client/rust/mind-core/src/sim/runtime.rs`).
  Assert (values decoded from the fixture with an independent parser):
  `DrillState` at tile (135,51): `progress == 420.7119 (±1e-3)`, `warmup == 1.0`;
  conveyor (133,55): `ConveyorBuild.len == 1`, `ids[0] == copper`;
  core (129,55): `ItemModule` copper `== 4000`, `CoreBuild.command_pos == None`;
  router (130,126) has team 0 (derelict) and no body (no panic).
* Same file: `java_save_content_order_is_identity`: read the fixture's content
  header (`SaveReadState.content` mapper is installed during the load; add a
  `SaveReadState::last_mapper: Option<TemporaryMapper>` that
  `SaveIo::load_bytes` fills with `content.temporary_mapper().cloned()` right
  before clearing it) and assert `map(Item, i) == Id(i)` for i in 0..22 and
  `map(Block, i) == Id(i)` for i in 0..447. Expected: identity (verified by a
  header parse on 2026-10-13). If this fails, stop and report — per-kind bodies
  would then need content remapping.
* `core/io/sim_io.rs` tests: `live_save_restores_drill_and_sorter_bodies`:
  `Sim::new(1, 16, 16, ..)`, `install_block_runtime`, place
  `mechanical-drill` at (4,4) and `sorter` at (8,8) via `Command::Place`;
  set `DrillState.progress = 123.5`, `warmup = 0.75`; configure the sorter to
  copper (copy the private helper `configure()` from
  `core/sim/production_tests.rs:150-154` into the sim_io test module; it takes a
  `crate::determinism::ConfigValue`); save, wipe
  (`sim.grid.fill`), load (pattern of `live_runtime_core_save_then_load_roundtrips`);
  assert drill progress 123.5 / warmup 0.75 and the sorter config readback is
  copper. Before the fix both assertions fail (progress 0, config None).
* `building_io.rs` tests: `write_all_java_matches_java_layout`: spawn a smelter
  (`spawn_smelter()` helper), `write_all_java`, assert byte 0 == the crafter
  revision (0), bytes 1..5 = health f32 BE, byte 5 has bit 0x80, byte 7 == 3
  (embedded base version); then `read_base_legacy` + body round trip.
* Keep `version_chain...` updated (above); the old v1 tests stay (v1 still reads).

**Verify in-engine.** Session §4.1 Ground Zero entry after `--import`
(the profile's GZ save is Java v7). Then:
```bash
$G --eval "return $H.building_debug(130, 48)"   # the mechanical drill the tutorial uses (if present), else pick one from hover_info
$G --eval "$H.place_block(140,52,\"sorter\"); return true" \
   --eval 'return get_node("/root/Spine/Input").configure_building(140,52,"content","copper")' \
   --eval "return $H.building_config_json(140,52)"          # expect copper
$G --eval "return $MC.run_exit_save()"; sleep 2
# resume with the Go recipe of §4.1, then:
$G --eval "return $H.building_config_json(140,52)"          # expect copper (before: null)
```
Check the `configure_building` argument shape with
`grep -n "fn configure_building" -A12 client/rust/mind-gdext/src/input/mod.rs` first.
0 panics in the log.

**Pitfalls.**
1. Existing native **v1** saves (every session's autosaved sectors) must still
   load: `kind_revision = None` → current `version()`; body = bytes after the
   v1 base. Do not delete `SaveV1`.
2. `EcsMapSource::write_building` returns `Ok(())` with an EMPTY chunk for P0
   entities without `Building`; the old reader then failed at `chunk.ub()`.
   The zero-length guard in step 3 fixes that latent failure.
3. `apply_body` runs while `grid.generating = true` and before proximity; a
   behavior `read` must not call into the grid (none does today).
4. Logic blocks (processor code, memory, message, switch, display, canvas)
   now restore their bodies too. If a logic `read` panics on Java bytes, that
   is G06's codec; guard with the debug log, never `unwrap`.
5. The UI golden is untouched; `parity/golden_content.json` untouched.
6. `cargo test -p mind-core --lib --features msav-import` is required (the
   fixture test is feature-gated).

**Commit.** `PAR-0161: restore per-kind building bodies on load (native v2 = Java chunk layout)`

---

### T2 — Sector resume restores the whole save (units, fires, puddles, markers, wave timer, tick, stats)

**Symptom / acceptance.** Resuming any sector (planet page "Go", §4.1) loads
only tiles + buildings + team plans: every unit (your polys/monos/daggers, the
enemy wave on the field), fire and puddle disappears, world-processor markers
vanish, `wavetime` is 0 (a wave sector with `waveTimer` fires a wave the
moment you resume; see T3w), `stats` restart at 0 (BuildCount/CoreItem
objectives regress), and the sim tick restarts at 0. After: the resumed world
equals the saved one. Acceptance: spawn two friendly daggers in a sector,
`run_exit_save`, resume → `get_group_count("unit")` is the same as before
minus/plus only the player unit; `get_hud_state().wavetime` equals the saved
value ±1 tick.

**Java.** `Control.playSector` resume branch (`J/core/Control.java:447-455`)
calls `slot.load(world.makeSectorContext(sector))` = `SaveIO.load` = the full
`SaveVersion.read`: `readMeta` (`SaveVersion.java:153-196`: `state.wave =
map.getInt("wave")`, `state.wavetime = map.getFloat("wavetime", state.rules.waveSpacing)`,
`state.tick = map.getFloat("tick")`, `state.stats = JsonIO.read(GameStats, map.get("stats","{}"))`),
`readMap`, `readEntities` (team plans + world entities: units, fires,
puddles, weather states, world labels), `readMarkers` (`state.markers.read`),
custom chunks. `Logic` `SaveLoadEvent` (`Logic.java:73-91`) then runs
`info.write()` (wave/rules from the record — port: `play.rs` `play_sector`
already copies `record.wave/waves/wave_spacing/win_wave/attack`).

**Port today.**
* `gdext/sim_host.rs:3561` `load_grid_file` builds `SaveReadState` with only
  `context` + `content` (no `entities`, no `markers` sink, `tags`/`stats` ignored).
* `gdext/sim_host.rs:1364-1460` `load_sector_world` materializes buildings and
  stores `sector_rules`/`sector_plans`; nothing else.
* The full decode + respawn logic already exists for Load Game in
  `core/io/sim_io.rs:671-818` `SimIoHandler::load` (private `LiveEntitySink`,
  `DecodedUnit/DecodedFire/DecodedPuddle`, unit respawn incl. payload stack,
  fire/puddle spawn) — but it boots two registries per call and uses
  `world::Context`.
* `play.rs:440` `play_sector` calls `session.reset_world()` → `wavetime = 0`,
  `stats` reset, markers cleared.

**Changes.**
1. `core/io/sim_io.rs`: extract a shared decoder:
   ```rust
   /// One decoded save (`SaveIO.load` output before it touches the world).
   pub struct DecodedSave {
       pub grid: WorldGrid,
       pub pending: Vec<(usize, DecodedBase)>,
       pub plans: Vec<(i32, Vec<TeamPlan>)>,
       pub tags: StringMap,
       pub rules: Option<Rules>,
       pub stats: Option<GameStats>,
       pub markers: MapMarkers,
       entities: LiveEntitySink,          // stays private; consumed by apply_decoded_entities
   }
   pub fn decode_save_bytes(bytes: &[u8], registry: &mut ContentRegistry) -> Result<DecodedSave, IoError>
   ```
   Body = today's `load_grid_file` (`WorldGrid::new(0,0)`, `begin_map_load`,
   `EditorContext::new(&mut grid, registry)`, `SaveReadState { context, content: Some(registry), entities: Some(&mut sink), markers: Some(&mut markers), .. }`,
   `SaveIo::load_bytes`, `grid.end_map_load(registry)`), returning all fields
   (`tags = state.tags.clone()`, `rules = state.rules.take()`, `stats = state.stats.take()`,
   `plans = take(&mut state.team_plans)`, `pending = context.take_pending_buildings()`).
   Types: `Rules` = `crate::io::json::Rules`, `GameStats` = `crate::io::json::GameStats`.
2. `impl Sim { pub fn apply_decoded_entities(&mut self, save: &DecodedSave) }`:
   move the unit/fire/puddle respawn block of `SimIoHandler::load`
   (the `for unit in &entities.units` … puddle spawn loop) here verbatim, plus
   the T9 player-unit rule (skip units with `spawned_by_core`; until T9 lands,
   skip any unit on the default team whose type name is one of the core unit
   types `["alpha", "beta", "gamma", "evoke", "incite", "emanate"]`
   (`Blocks.java` `coreShard..coreAcropolis` `unitType`; the port has no
   core-unit-type field — `spawn_player_unit` hardcodes `"alpha"`).
   Rationale: `UnitComp.java:868-871` despawns a `spawnedByCore` unit that is
   not the player's, and the host spawns a fresh player unit (`spawn_player_unit`).
3. Tick parse bug (same root cause): `core/io/sim_io.rs:712` parses the meta
   `tick` with `parse::<u64>()`, which fails on every Java save (float text) and
   silently keeps the old tick. Add `pub fn meta_tick(tags: &StringMap) -> Option<u64>`
   (`parse::<f64>().ok().filter(|t| t.is_finite()).map(|t| t.max(0.0).floor() as u64)`)
   in `sim_io.rs` and use it in both places (here and step 4). Test
   `meta_tick_parses_java_float`: `"74802.1339981854"` → `Some(74802)`, `"12"` → `Some(12)`, `"x"` → `None`.
4. Rewrite `SimIoHandler::load` on top of `decode_save_bytes` (boot ONE
   registry — the decode needs `&mut ContentRegistry`; `materialize` uses the
   live runtime's content) + `apply_decoded_entities`. Behaviour otherwise
   unchanged (tick/rules/stats/wave/wavetime/markers/plans onto the runtime).
5. `gdext/sim_host.rs`:
   * `LoadedGrid` gains `save: Option<DecodedSave>` (the grid/pending/plans/rules
     move out of it as today). `load_grid_file` calls
     `mind_core::io::sim_io::decode_save_bytes(bytes.as_slice(), registry)`.
   * New field on `MindSimHost`: `sector_resume: Option<SectorResumeMeta>` with
     ```rust
     pub struct SectorResumeMeta { pub wavetime: Option<f32>, pub tick: Option<u64>, pub stats: Option<GameStats>, pub markers: MapMarkers }
     ```
     (define it in `core/io/sim_io.rs` so the campaign facade can name it).
   * In `load_sector_world`, after `materialize_map_buildings`: if a save was
     decoded, `self.sim.apply_decoded_entities(&save)`; then, only when
     `use_save && the save file was the source` (the resume branch), fill
     `sector_resume` from `save.tags` (`wavetime` parse f32; `tick` is a Java
     **float** — `SaveVersion.java:135` writes `state.tick` (float),
     `:158` reads `map.getFloat("tick")`, e.g. `74802.1339981854` in the r05
     fixture — so parse it with `parse::<f64>()` and convert with
     `.max(0.0).floor() as u64`) and `save.stats`/`save.markers`, and set
     `self.sim.state.tick` from the tag. Clear `sector_resume` at the start of
     every `load_sector_world`.
   * Rust-only accessor `pub fn take_sector_resume(&mut self) -> Option<SectorResumeMeta>`.
6. `gdext/campaign.rs` `start_sector_with_loadout`: in the resume branch
   (`has_save && !lost_replay`), right after `play_sector(...)` and BEFORE the
   runtime is installed, take `host.bind_mut().take_sector_resume()` (own
   statement; do not hold the bind across other calls) and apply:
   `self.session.wavetime = meta.wavetime.unwrap_or(self.session.rules.wave_spacing)`
   (Java default `map.getFloat("wavetime", state.rules.waveSpacing)`),
   `self.session.stats = stats` when present, `self.session.markers = markers`.
   Then re-read the host tick into `self.last_pump_tick` (already done at the
   end of the function — keep it after this).

**Tests.**
* `core/io/sim_io.rs`: `decode_save_bytes_carries_units_tags_and_markers`:
  build a `Sim` with the block runtime + unit runtime (see the existing
  `live_sim_save_then_load_restores_entities_and_meta` test at ~978 for the
  setup), spawn 2 daggers team 1 at (40,40) and (48,40), write the save, then
  `decode_save_bytes` on the bytes and assert `save.entities.units.len() == 2`
  (test is inside the module, so the private field is visible),
  `save.tags["wavetime"]` equals the session wavetime you set (e.g. `1234.5`).
* `apply_decoded_entities_skips_core_units`: a save with one `alpha` team 1 and
  one `dagger` team 1 → after `apply_decoded_entities` only the dagger exists.
* Existing `live_sim_save_*` tests must still pass (they now go through the
  shared decoder).

**Verify in-engine.** Fresh-profile Ground Zero (§4.1 Go recipe), then:
```bash
$G --eval "return $H.content_list(\"unit\").find(\"dagger\")"             # -> index N
$G --eval "return $H.apply_sim_command_json('{\"op\":\"spawn_unit\",\"unit\":N,\"team\":1,\"px\":1040,\"py\":460}')" \
   --eval "return $H.get_group_count(\"unit\")"                          # e.g. 2 (player + dagger)
$G --eval "return $MC.run_exit_save()"; sleep 2
# Go recipe again, then
$G --eval "return $H.get_group_count(\"unit\")"                          # same as before (before fix: 1)
$G --eval "return $MC.get_hud_state().wavetime"                          # equals the pre-save value, not 0
```
0 panics.

**Pitfalls.**
* `decode_save_bytes` takes `&mut ContentRegistry`; in `load_sector_world` the
  registry is `self.content_snapshot.take()` — keep the existing take/return.
* Legacy Java chunks (class ids 0..49) are still skipped by `LiveEntitySink`
  until T6; that is expected here.
* `Sim::apply_decoded_entities` must run AFTER `install_live_block_runtime`
  (the unit runtime must exist) — it does, since materialize runs after it.
* Do not apply `sector_resume` on the lost-sector replay or fresh launches.

**Commit.** `Gameplay G04 T2: sector resume restores units, fires, puddles, markers, wave timer and stats`

---

### T3w — Wave countdown runs 60x too slowly; `isWaitingWave` and `runWave` gate

**Symptom / acceptance.** On a wave sector with `waveTimer` (e.g. frozenForest
64) the HUD countdown and the next wave take 60 times longer than Java
(2-minute spacing → 2 hours). Enemies still alive at the win wave / with
`waitEnemies` do not hold the timer. After: `wavetime` drops by exactly 1 per
sim tick; the hold matches Java. Acceptance: in engine, two reads of
`$MC.get_hud_state().wavetime` 600 ticks apart (paused + `step(600)`) differ
by 600 (±1); before the fix they differ by 10.
UNVERIFIED in engine (code-read only): executor confirms the 10-vs-600 numbers
with the commands below before changing code.

**Java.** `J/core/Logic.java:560-569` (inside `if(!state.isPaused())`):
```java
if(state.rules.waves && state.rules.waveTimer && !state.gameOver){
    if(!isWaitingWave()){
        state.wavetime = Math.max(state.wavetime - Time.delta, 0);   // Time.delta = 1 per 60 Hz tick
    }
}
if(!net.client() && state.wavetime <= 0 && state.rules.waves){
    runWave();
}
// Logic.java:621-624
public boolean isWaitingWave(){
    return (state.rules.waitEnemies || (state.wave >= state.rules.winWave && state.rules.winWave > 0)) && state.enemies > 0;
}
// runWave (Logic.java:318-324): spawner.spawnEnemies(); state.wave++;
// state.wavetime = state.rules.waveSpacing * (campaign ? difficulty.waveTimeMultiplier : 1f);
```
Defaults: `Rules.waveSpacing = 2 * Time.toMinutes = 7200` ticks,
`waitEnemies = false`.

**Port today.** `core/game/runtime.rs:354-375` `campaign_tick_system`:
`runtime.session.wavetime = (runtime.session.wavetime - 1.0 / TICKS_PER_SECOND).max(0.0)`
(`TICKS_PER_SECOND = 60.0` at line 45), and `run_wave_campaign` is called only
inside the `waves && wave_timer && !game_over` block. No waiting check.
`session.enemies` is refreshed by `enemy_count_system` (`TickSet::EnemyCount`,
runs before `TickSet::Campaign` — `core/sim/schedule.rs:71-77`).
`session.wavetime` is in ticks everywhere else (`play.rs:496,514`, HUD
`core/ui/hud_text.rs:49 wavetime_ticks`).

**Changes.** In `campaign_tick_system`:
1. Replace the block with:
   ```rust
   let s = &mut runtime.session;
   if s.rules.waves && s.rules.wave_timer && !s.game_over && !is_waiting_wave(s) {
       s.wavetime = (s.wavetime - 1.0).max(0.0);
   }
   if s.wavetime <= 0.0 && s.rules.waves {
       // existing body: run_wave_campaign + pending_wave + waves_lasted++
   }
   ```
2. Add to `core/game/play.rs`:
   ```rust
   /// `Logic.isWaitingWave` (`Logic.java:621-624`).
   pub fn is_waiting_wave(session: &PlaySession) -> bool {
       (session.rules.wait_enemies
           || (session.wave >= session.rules.win_wave && session.rules.win_wave > 0))
           && session.enemies > 0
   }
   ```
3. `core/game/runtime.rs` test `wave_timer_reaches_win_wave_and_captures`
   (1587) sets `wavetime = 0.01`; it still fires on the first tick. Keep it.
4. Also expose `is_waiting_wave` in the HUD dict (`gdext/campaign.rs`
   `get_hud_state`, key `"waitingWave"`), G03 uses it for `wave.waveInProgress`.
   (Only the key; no UI change here.)

**Tests** (`core/game/runtime.rs` tests):
* `wave_timer_counts_ticks`: session with `waves: true, wave_timer: true,
  wave_spacing: 7200.0`, `wavetime = 120.0`, `spawn_count = 1`,
  `difficulty_wave_time_multiplier = 1.0`, phase Playing, sector None
  (non-campaign multiplier 1.0 — check `is_campaign()` is false with
  `sector: None`). Run `campaign_tick_system` 119 times → `wave == 0`,
  `wavetime == 1.0`; once more → `wave == 1`, `wavetime == 7200.0`.
* `waiting_wave_holds_timer`: `wait_enemies = true`, `enemies = 3`,
  `wavetime = 50.0` → after 10 ticks `wavetime == 50.0`; set `enemies = 0` →
  after 10 ticks `40.0`.
* `wave_runs_without_timer_when_due`: `wave_timer = false`, `wavetime = 0.0`,
  `waves = true` → one tick → `wave == 1` (Java runs the wave regardless of
  `waveTimer`).

**Verify in-engine.** frozenForest (64) fresh (§4.1 "fresh, never-played"
recipe with 64):
```bash
$G --eval "$H.set_paused(true); return $MC.get_hud_state().wavetime" \
   --eval "$H.step(600); return $MC.get_hud_state().wavetime"
```
Expected difference 600 (before: 10). Then `$H.set_paused(false)`.

**`logic_play` check (coordinator item from G07, verified 2026-10-13).**
Java `Logic.play()` (`Logic.java:269-298`) resets `wavetime`, `stats`, fires
`PlayEvent` (weather cooldown shuffle + `state.tick = 0`, `Logic.java:93-101`),
adds the loadout and heals cores; it is called ONLY from `Control.playMap` and
`Control.playNewSector` (`Control.java:415,589`), never on a save load (a load
fires `SaveLoadEvent`, not `PlayEvent`). The port matches: `logic_play`
(`core/game/play.rs:484`) is called only from `play_map` (253) and
`play_new_sector` (403) (plus a test at `runtime.rs:1414`); the resume path
`play_sector` does not call it, and T2 restores `wavetime`/`stats`/`tick`
from the save. Only fix: the doc comment at `play.rs:494` ("including save
loads") is wrong — reword to "on every new play (`playMap`/`playNewSector`);
save loads keep the saved weather cooldowns (`rules.weather` is in the saved
rules)". A fresh launch builds a new `Sim`, so `state.tick = 0` holds; no code change.

**Pitfalls.** The `Universe` turn counter in the same system uses its own
`advance_time(1)` — do not touch it. `hud_text.rs` already formats ticks; no
UI change. Ground Zero has `waveTimer:false` (tutorial) so it never showed the
bug; its waves are triggered by the `spawnWave` objective flag/world processor.

**Commit.** `Gameplay G04 T3w: wave countdown in ticks, isWaitingWave hold, runWave gate`

---

### T4 — Map objectives: completion persistence, Timer in ticks, DestroyUnits/UnitCount/DestroyBlock(s) inputs

**Symptom / acceptance.** Ground Zero tutorial (17 objectives, see the r05
fixture: Item, BuildCount, CoreItem×4, Timer×5, BuildCount×4, DestroyUnits×2,
Flag×2): (a) Timer objectives (`duration` 300/180/180 ticks) take 60× longer
(5 min instead of 5 s); (b) `DestroyUnits` never completes (the port's env
returns 0 kills) → the tutorial stalls; (c) after any save/load every
objective is incomplete again (completion is not saved), so the HUD restarts
the tutorial chain; (d) `DestroyBlock(s)` objectives complete instantly
(`block_at` returns `None`). After: identical to Java.
Acceptance: unit tests below; in engine on Ground Zero the objective index
advances past the Timer within ~5 s of sim time, and a resume keeps the
completed objectives.

**Java.**
* `J/game/MapObjectives.java:136-143` `update`: for each qualified objective
  (`!completed && dependencyFinished()`), `if(obj.update()) Call.completeObjective(index)`.
* `MapObjective` fields (180-201): `hidden, details, completionLogicCode,
  flagsAdded, flagsRemoved, markers` + `private boolean completed` (serialized
  by `JsonIO`, it is not `transient`); `done()` (213-219) removes/adds flags,
  sets `completed = true`, runs the logic script.
* `TimerObjective` (516-540): `protected float countup` (serialized),
  `update(){ return (countup += Time.delta) >= duration * state.rules.objectiveTimerMultiplier; }`,
  `duration` default `60f * 30f` ticks.
* `UnitCountObjective.update` (470): `state.rules.defaultTeam.data().countType(unit) >= count`.
* `DestroyUnitsObjective.update` (501): `state.stats.enemyUnitsDestroyed >= count`;
  the counter is `Logic` `UnitDestroyEvent` (`Logic.java:218-226`):
  `if(e.unit.team != state.rules.defaultTeam) state.stats.enemyUnitsDestroyed++`.
* `DestroyBlockObjective.update` (604): `build = world.build(pos.x, pos.y); return build == null || build.team != team || build.block != block;`
  `DestroyBlocksObjective` (650): all positions satisfy the same predicate.
* Java save evidence: `runs/20261009-065416-ref-ground-zero/checkpoints/r03..r05`
  meta `rules` contain `objectives:[{class:Item,…,completed:true,amount:15,…}`.

**Port today.**
* `core/io/json/objectives.rs:79-108` `ObjectiveCommon` has no `completed`;
  `common_from_value` (596) ignores it; `serialize_common` (318) does not write it.
  `TimerObjective.countup` exists in the data struct (line ~150) but the runtime
  keeps its own `countup: Vec<f32>` (`core/game/map_objectives.rs:160`) and never
  writes it back.
* `MapObjectivesRuntime::from_data` (170) sets `completed = vec![false; len]`.
  `complete()` (321) never touches `data.all[i]`, so `session.rules.objectives`
  (written into every save by `live_meta_tags`) never records completion.
* `objectives_tick_system` (`core/game/runtime.rs:520`) passes
  `1.0 / TICKS_PER_SECOND` as `delta_seconds`; `timer_objective` (`map_objectives.rs:487`)
  adds it to `timer` and compares with `duration` (ticks).
* `RuntimeObjectiveEnv` (`runtime.rs:560-641`): `unit_count` → 0,
  `enemy_units_destroyed` → 0, `block_at` → `None`, `command_mode_satisfied` → false.
* `GameStats.enemy_units_destroyed` (`core/io/json/rules.rs:842`) is never
  incremented anywhere live.

**Changes.**
1. `ObjectiveCommon`: add `pub completed: bool` (default false). Parse in
   `common_from_value`: `completed: value.get("completed").and_then(Value::as_bool).unwrap_or(false)`.
   Serialize in `serialize_common` **only when true** (Java JsonIO omits
   default-valued fields; matches the fixture): `if common.completed { map.serialize_entry("completed", &true)?; }`.
   `TimerObjective` parse: read `countup` with `f32_field(value, "countup", 0.0)`
   (check the parser fn for Timer near line 668 and add the field).
2. `MapObjectivesRuntime::from_data`: `completed[i] = data.all[i].common().completed`,
   `countup[i] = if let MapObjective::Timer(t) = &data.all[i] { t.countup } else { 0.0 }`.
   `dep_finished` stays false (recomputed by `dependency_finished`).
3. `MapObjectivesRuntime::complete`: also set `self.data.all[index].common_mut().completed = true`
   (add `common_mut()` next to `common()` in `objectives.rs:201` if missing).
4. Write-back: in `objectives_tick_system`, after the completion loop, copy
   the runtime data into the rules so saves carry it:
   `runtime.session.rules.objectives = runtime.session.objectives.data().clone();`
   but only when something changed this tick (completed non-empty) or a timer
   advanced — simplest: after the loop, for every Timer index copy
   `countup[i]` into `data.all[i]` (`TimerObjective.countup`) and then clone
   `data()` into `rules.objectives` when `completed` is non-empty OR
   `runtime.ticks % 60 == 0` (bounded cost; a save between two copies loses <1 s
   of a timer, acceptable). Add `pub fn sync_timers_into_data(&mut self)` on the
   runtime for the countup copy.
5. Timer units: change `objectives_tick_system` to pass `1.0` (one tick) and
   rename the parameter of `MapObjectivesRuntime::update` / `objective_update`
   / `timer_objective` from `delta_seconds` to `delta_ticks` (doc: `Time.delta`).
   Update every caller (`grep -rn "objectives.update(\|\.update(&env" client/rust/mind-core/src`)
   and existing tests that pass seconds (search `delta_seconds` in
   `map_objectives.rs` tests; scale their expectations ×60).
6. Enemy kill counter: in `Sim::resolve_deaths` (`core/sim/unit_runtime.rs:383`),
   before `kill_unit_with_abilities`, read the unit team; after a successful
   kill, if a `CampaignRuntime` resource exists and `team != session.rules.default_team`,
   increment `session.stats.enemy_units_destroyed` (take the resource with
   `world.get_resource_mut::<CampaignRuntime>()` — `world` here is `&mut self.ecs.0`
   while `runtime` (UnitRuntime) is removed, so no borrow clash).
7. `RuntimeObjectiveEnv`:
   * `enemy_units_destroyed()` → `self.stats.enemy_units_destroyed`.
   * `unit_count(team, unit)` → count ECS entities with `UnitTypeComp` whose
     content name equals `unit` (`ContentIndex` has `item_names`; add
     `unit_names: Vec<String>` to `ContentIndex` if absent — check
     `grep -n "pub struct ContentIndex" -A15 client/rust/mind-core/src`), `TeamComp.team == team`,
     and `UnitCore.dead == false` (`Teams.countType` counts live units).
   * `block_at(x, y)` → find the ECS entity with `Building` + `TeamComp` whose
     footprint covers `(x, y)`: `size = BlockTable.instance(b.block).def.size`,
     `off = (size - 1) / 2`, covers when `b.tile.x()-off <= x < b.tile.x()-off+size`
     and likewise for y (Java `world.build` = the building whose tile area contains
     the tile). Return `(instance.name.as_str(), team)`. The trait returns
     `Option<(&str, u8)>`: borrow the name from the `BlockTable` resource
     (`self.world.get_resource::<BlockTable>()`); if lifetimes fight, change
     the trait method to return `Option<(String, u8)>` and update the two
     callers in `map_objectives.rs` (`destroy_block_objective`, `destroy_blocks_progress`).
     Iterate `iter_entities()` (read-only; order does not matter for a
     predicate). Performance: DestroyBlock objectives are few; fine.
   * `command_mode_satisfied()` → read a new resource, defined in
     `core/game/runtime.rs` and re-exported as `mind_core::game::runtime::ObjectiveInputState`:
     ```rust
     /// Input-side facts the objective executor needs (`CommandModeObjective`,
     /// MapObjectives.java:671-687: `control.input.selectedUnits.contains(u -> u.isCommandable() && u.command().hasCommand())`).
     #[derive(Resource, Debug, Clone, Copy, Default)]
     pub struct ObjectiveInputState { pub command_mode_satisfied: bool }
     impl Sim {
         /// Setter for the input layer (G08): call every frame with the current selection state.
         pub fn set_command_mode_satisfied(&mut self, value: bool)  // inserts/updates the resource
     }
     ```
     gdext exposure for G08: `#[func] MindSimHost::set_command_mode_satisfied(value: bool)`
     forwarding to the `Sim` method. G08 calls it from `MindInput` whenever
     the command-mode selection changes. Default false = today's behaviour.

**Tests** (`core/game/map_objectives.rs` + `core/game/runtime.rs`):
* `timer_objective_counts_ticks`: Timer `duration = 300.0`,
  `objective_timer_multiplier = 1.0`; 299 updates with `delta_ticks = 1.0` →
  not complete; 300th → complete.
* `completed_flag_round_trips`: parse
  `[{"class":"Item","item":"copper","amount":15,"completed":true},{"class":"Timer","duration":300,"countup":120,"parents":[0]}]`
  → `from_data` → `is_completed(0)`, `qualified(1)`; running 180 tick updates
  completes index 1; serialize → contains `"completed":true` for both.
* `destroy_units_counts_enemy_kills`: env stub with `enemy_units_destroyed = 2`
  → `DestroyUnits{count:2}` completes, `{count:3}` does not (existing env test
  struct at `map_objectives.rs:700` — extend it).
* `runtime_block_at_reports_building`: a sim with a `duo` team 2 at (10,10)
  (`CombatHarness` or `runtime_sim()` pattern) → `DestroyBlock{pos:(10,10), team:2, block:"duo"}`
  is not complete; after `break_block(10,10)` it completes.

**Verify in-engine.** Fresh Ground Zero from a clean re-import (§4.3; the GZ
save of the profile is at objective 0). Feed the first objectives with
`debug_add_core_items("copper", 200)` + place drills via `place_block` as in
PAR-0051, then watch the objective index advance:
```bash
$G --eval "return $MC.get_hud_state()"     # inspect the objective keys (grep the dict for "objective")
```
Expected: the Timer objectives complete ~300/180 ticks after qualifying
(before: 60× longer). Then `run_exit_save` + Go: the completed objectives stay
completed (before: back to index 0). Find the objective key names first with
`grep -n "objective" client/rust/mind-gdext/src/campaign.rs | grep dict`.

**Pitfalls.** `rules.objectives` is also the editor's data — the `completed`
field must default false so editor maps do not change. Mirror of
`self.session.objectives` in the facade (`pump_runtime`) is already cloned
every pump. `MapObjectivesRuntime::from_rules` is called in
`start_sector_with_loadout` AFTER the map merge; with step 2 a resumed save's
completed flags now survive that call.

**Commit.** `Gameplay G04 T4: objectives persist completion, Timer in ticks, kill/unit/block inputs`

---

### T5 — Sector rules: adopt the map/save rules wholesale, planet `ruleSetter`, load-event overrides, `limitMapArea` plumbing

**Symptom / acceptance.** (a) Ground Zero loads with `tile_debug(...).limit_area == null`
although its rules carry `limitMapArea:true, limitX:90, limitY:20, limitWidth:70, limitHeight:60`
(Java r05 meta) → the whole 256×256 map is buildable/visible, the minimap shows
no border (rendering audit blocker 1). (b) Every other map/save rule except
spawns/waveTimer/waveSending/objectives/flags/teams is dropped (weather
cooldowns, unit cap, lighting, enemyCoreBuildRadius, …). (c) The planet
`ruleSetter` is not ported: Serpulo never sets `coreDestroyClear`,
`derelictRepair`, `placeRangeCheck=false`, `hideSpawns=true`. (d) The campaign
load overrides are missing: the wave team never gets
`infiniteResources/fillItems` (`Planet.enemyInfiniteItems` defaults **true**),
Serpulo enemy factories have no 2-minute activation delay
(`enemyFactoryActivationDelay = 7200`), Erekir enemy build speed ×0.4 missing.
Acceptance: after entering Ground Zero `$H.tile_debug(100,30).limit_area == [90,20,70,60]`;
placing a `copper-wall` at (85,30) (outside the rect) is rejected while (100,30)
is accepted; `$MC.get_hud_state()` unchanged otherwise.

**Java.**
* Fresh preset launch: `World.loadSector` (`J/core/World.java:265-297`):
  `setSectorRules` → `preset.generator.generate` = `FileMapGenerator` →
  `SaveIO.load(map.file)` → `SaveVersion.readRules` (197-211):
  `state.rules = JsonIO.read(Rules, ruleString)` (**wholesale**), empty spawns →
  default waves, `if sector != null { rules.sector = sector; sector.planet.applyRules(rules); }`
  → `preset.rules.get(state.rules)` (= `winWave = captureWave`) → `setSectorRules`
  again (299-334: `rules.sector`, `planet.generator.addWeather`, resources scan,
  `cloudColor = planet.landCloudColor`, `env = planet.defaultEnv`,
  `planet = planet`, `planet.applyRules(rules, !saveInfo)`).
* `Planet.applyRules` (`J/type/Planet.java:295-306`): `ruleSetter.get(rules)`;
  `attributes = defaultAttributes`; `env`; `planet`; `if(!customGame) campaignRules.apply(planet, rules)`.
* `ruleSetter` values (`J/content/Planets.java`):
  serpulo 82-88: `waveTeam = crux(2); placeRangeCheck = false; hideSpawns = true; derelictRepair = true; coreDestroyClear = true;`
  erekir 74-83: `waveTeam = malis(3); placeRangeCheck = false; hideSpawns = false; fog = true; staticFog = true; lighting = false; coreDestroyClear = true; onlyDepositCore = true;`
  tantros 116-118: empty. Default (`Planet.ruleSetter`) is a no-op.
* Resume: `SaveVersion.readRules` wholesale + `planet.applyRules`, then `Logic`
  `SaveLoadEvent` (`Logic.java:73-91`): `coreIncinerates = true; canGameOver = true; allowEditRules = false;`
  `info.write()`; `planet.applyRules(rules)`.
* `Logic` `WorldLoadEvent` (`Logic.java:105-125`), campaign only:
  ```java
  rules.coreIncinerates = true; rules.infiniteResources = false; rules.allowEditRules = false;
  rules.allowEditWorldProcessors = false; rules.worldProcessorPlayerLink = false;
  if(planet.enemyInfiniteItems){ waveTeam.rules().infiniteResources = true; waveTeam.rules().fillItems = true; }
  waveTeam.rules().buildSpeedMultiplier *= planet.enemyBuildSpeedMultiplier;
  if(planet.enemyFactoryActivationDelay > 0f && waveTeam.rules().unitFactoryActivationDelay == 0f)
      waveTeam.rules().unitFactoryActivationDelay = planet.enemyFactoryActivationDelay;
  ```
  Planet defaults (`J/type/Planet.java:125-133`): `enemyBuildSpeedMultiplier = 1`,
  `enemyFactoryActivationDelay = 0`, `enemyInfiniteItems = true`; erekir
  `enemyBuildSpeedMultiplier = 0.4` (`Planets.java:68`), serpulo
  `enemyFactoryActivationDelay = 7200` (`Planets.java:132`).
* `World.isInMapArea` (337-339), `getDarkness` border term (483-503:
  `edgeBlend = 2`; inside the limit rect `edgeDst = min(x-limitX, y-limitY, limitX+limitWidth-1-x, limitY+limitHeight-1-y)`;
  `if(edgeDst <= 2) dark = max((2 - edgeDst) * 2, dark)`), `Build.validPlace`
  (`J/world/Build.java:229`): `if(!type.ignoreBuildDarkness && world.getDarkness(x, y) >= 3) return false;`
  → with the limit rect every tile outside it and the outer tile ring is unbuildable.

**Port today.**
* `gdext/campaign.rs:2872-2902` `merge_map_rules` (subset copy, see Symptom).
  Called once in `start_sector_with_loadout` (~line 727).
* `core/game/planet.rs:364-387` `Planet::apply_rules`: no `ruleSetter`
  (comment says vanilla setters go through campaign defaults — wrong, see Java).
* `core/content/registries/planets.rs` has `enemy_build_speed_multiplier`
  (309/402/595) and `enemy_factory_activation_delay` (311/403/674) but no
  `enemy_infinite_items` and nothing reads them.
* `core/world/mod.rs:151-152` `WorldGrid.limit_area`, `is_in_map_area` (254)
  — never set/used live; `core/world/darkness.rs:134` `border_darkness(grid, border, limit, x, y)`
  exists (helper from rendering T19) and is unused by placement;
  `core/world/build.rs:195` checks `get_wall_darkness` only.
* `gdext/sim_host.rs:3286-3295` `tile_debug` reports `limit_area`.

**Changes.**
1. `core/content/registries/planets.rs`: add `pub enemy_infinite_items: bool`
   (default `true`), and `pub rule_setter: RuleSetter` where
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum RuleSetter { #[default] None, Serpulo, Erekir }
   ```
   set `Serpulo` on serpulo and `Erekir` on erekir where those planets are built
   (near lines 595/674 — find the planet constructors with `grep -n "\"serpulo\"\|\"erekir\"" planets.rs`).
   This changes `parity/golden_content.json` only if planet fields are dumped
   there — check `cargo run -p mind-headless -- content dump` diff; if it
   changes, regenerate + update the sha256 (§2 of env doc) and name the reason
   in the commit body.
2. `core/game/planet.rs`: copy both fields onto the runtime `Planet` in
   `from_def`; in `Planet::apply_rules`, FIRST apply the setter:
   ```rust
   match self.rule_setter {
       RuleSetter::Serpulo => { rules.wave_team = 2; rules.place_range_check = false; rules.hide_spawns = true;
                                rules.derelict_repair = true; rules.core_destroy_clear = true; }
       RuleSetter::Erekir  => { rules.wave_team = 3; rules.place_range_check = false; rules.hide_spawns = false;
                                rules.fog = true; rules.static_fog = true; rules.lighting = false;
                                rules.core_destroy_clear = true; rules.only_deposit_core = true; }
       RuleSetter::None => {}
   }
   ```
   (check `Team.crux.id == 2`, `Team.malis.id == 3` in `J/game/Team.java`).
3. `core/game/play.rs`: add
   ```rust
   /// `Logic` WorldLoadEvent campaign half (`Logic.java:105-125`).
   pub fn apply_world_load_campaign_rules(rules: &mut Rules, planet: &Planet)
   /// `Logic` SaveLoadEvent rule half (`Logic.java:75-79`).
   pub fn apply_save_load_campaign_rules(rules: &mut Rules)  // coreIncinerates/canGameOver/allowEditRules
   ```
   transcribing the Java above (`rules.team_rule_mut(rules.wave_team)` for the
   wave-team rules).
4. `gdext/campaign.rs`: replace `merge_map_rules` with `adopt_map_rules(session: &mut PlaySession, map: &Rules, planet: &Planet, has_preset: bool, resumed: bool)`:
   ```text
   let kept = session.rules.clone();          // campaign fold already applied by play_*_sector
   let mut rules = map.clone();                // wholesale (SaveVersion.readRules)
   if rules.spawns.is_empty() { rules.spawns = kept.spawns.clone(); }   // default waves
   rules.sector = kept.sector.clone(); rules.loadout = kept.loadout.clone();
   planet.apply_rules(registry, &mut rules, false, true);              // ruleSetter + env/attrs + campaignRules.apply
   if has_preset && !resumed { rules.win_wave = kept.win_wave; rules.waves = kept.waves; rules.attack_mode = kept.attack_mode; }
   if resumed { rules.waves = kept.waves; rules.wave_spacing = kept.wave_spacing;
                rules.win_wave = kept.win_wave; rules.attack_mode = kept.attack_mode; }   // SectorInfo.write (play_sector)
   if kept.lighting == false && preset_no_lighting { rules.lighting = false; }
   apply_world_load_campaign_rules(&mut rules, planet);
   if resumed { apply_save_load_campaign_rules(&mut rules); }
   session.rules = rules;
   ```
   Keep the existing `map.teams` copy semantics (now implicit: wholesale). Keep
   the comment history from `merge_map_rules` as doc. The `registry` and
   `planet` are available in `start_sector_with_loadout` (`campaign.planet(planet_id)`).
   Delete `merge_map_rules` and its test if any (`grep -n merge_map_rules`).
5. World application — ONE function that re-derives every world-side value
   from the rules, in **mind-core** so both the campaign start and G06's
   world-processor `setrule` (G06 T4: `setrule mapArea` writes the live
   session rules; Ground Zero's tutorial expands the area this way) can call it:
   ```rust
   // core/sim/mod.rs (impl Sim)
   /// Re-derives the world state that mirrors `Rules`: `World.isInMapArea`
   /// limit rect + border darkness (World.java:337-339, 483-503) and the build
   /// guard (`sync_build_rules`, G02 T8). Call after any rules change.
   pub fn apply_rules_to_world(&mut self, rules: &Rules) {
       self.grid.limit_area = rules.limit_map_area.then_some((rules.limit_x, rules.limit_y, rules.limit_width, rules.limit_height));
       self.grid.border_darkness = rules.border_darkness;   // new WorldGrid field, default true
       // G02 T8's `sync_build_rules(&mut World, &Rules)` (BuildRules from session rules);
       // call it here once it exists — until then leave a `// G02 T8` comment and skip.
       self.refresh_unit_runtime();                          // unit grid snapshot carries limit_area
   }
   ```
   gdext wrapper `MindSimHost::apply_world_rules(&mut self, rules: &Rules)`
   = `self.sim.apply_rules_to_world(rules); self.world_dirty = true;`.
   Callers: (a) `start_sector_with_loadout` right after the rules are adopted
   (own statement, `host.bind_mut()`); (b) mid-sector rule changes: add
   `pub rules_world_dirty: bool` to `CampaignRuntime` (`core/game/runtime.rs:178`);
   G06's `setrule` handler (and any other rules mutation: `Call.setRules`
   relay, T8 invasion) sets it to true; at the end of `Sim::tick` (after the
   schedule, next to T7's `apply_campaign_world_requests`) check the flag on the
   `CampaignRuntime` resource, clear it and call
   `self.apply_rules_to_world(&rules_clone)`. Tell G06 the names:
   `Sim::apply_rules_to_world` (direct) / `CampaignRuntime.rules_world_dirty = true` (from a system).
   The gdext side detects the change through the existing `world_dirty`/
   `emit_world_changed` path: in `MindSimHost`'s per-frame pump, compare
   `self.sim.grid.limit_area` with a cached copy and set `self.world_dirty = true` when it changed.
6. Placement: `core/world/build.rs:195` → 
   `let dark = (get_wall_darkness(grid, content, x, y) as f32).max(border_darkness(grid, grid.border_darkness, grid.limit_area, x, y)); if !def.ignore_build_darkness && dark >= 3.0 { return false; }`
   (check the `ignore_build_darkness` field exists on `BlockDef`; if not, keep
   the unconditional check and note it). Same for the size-2 corner check if
   the port has it (Java `Build.java:251`).

**Tests.**
* `core/sim/tests.rs`: `rules_world_dirty_rederives_limit_area`: runtime sim
  with a `CampaignRuntime`; set `rules.limit_map_area = true, limit_x = 2, limit_y = 2, limit_width = 4, limit_height = 4`
  and `rules_world_dirty = true`; one `sim.tick()` → `grid.limit_area == Some((2,2,4,4))`
  and the flag is false.
* `core/game/planet.rs`: `serpulo_rule_setter_applies`: `Rules::default()` →
  `serpulo.apply_rules(.., false, true)` → `wave_team == 2`, `core_destroy_clear`,
  `derelict_repair`, `!place_range_check`, `hide_spawns`.
* `core/game/play.rs`: `world_load_rules_give_wave_team_infinite_items`:
  serpulo → `team_rule(2).infinite_resources && fill_items`,
  `unit_factory_activation_delay == 7200.0`; erekir →
  `team_rule(3).build_speed_multiplier == 0.4` (from 1.0).
* `core/world/build.rs`: `limit_area_blocks_placement_outside`: grid 64×64,
  `limit_area = Some((10,10,20,20))`: `valid_place` of a 1×1 wall at (15,15)
  true; at (5,15) false; at (10,15) false (edgeDst 0 → dark 4); at (11,15)
  false (edgeDst 1 → dark 2? → `(2-1)*2 = 2 < 3` → **true**). Derivation:
  `dark = (2 - edgeDst) * (4 / 2)`; edgeDst 0 → 4 (blocked), 1 → 2 (allowed),
  2 → 0. So expected: (10,15) false, (11,15) true.
* `gdext` has no unit tests; covered in engine.

**Verify in-engine.** §4.1 Ground Zero entry:
```bash
$G --eval "return $H.tile_debug(100,30)"                 # limit_area == [90,20,70,60]
$G --eval "$H.apply_sim_command_json('{\"op\":\"place\",\"x\":85,\"y\":30,\"block\":\"copper-wall\",\"player\":0}'); return $H.player_build_plans_json()"
```
(check the `place` op payload keys with `grep -n "\"place\"" -A15 client/rust/mind-gdext/src/sim_host.rs`)
Expected: the plan at (85,30) is rejected/absent, a plan at (100,30) is kept.
Screenshot `$G --screenshot runs/<date>-s<N>-G04-T5/gz-limit.png`: the area
outside the rect is dark (G03 draws it; if not yet drawn, note it, do not fix).

**Pitfalls.** The wholesale copy changes `default_team/wave_team` to the map's
values; Serpulo's setter forces `wave_team = 2` anyway. `rules.planet`/`env`
are re-set by `apply_rules`. Do NOT apply `apply_world_load_campaign_rules`
for custom (non-campaign) games. The `PAR-0059` behaviour (captureWave →
winWave, noLighting) must stay: re-run its in-engine check
(`get_hud_state().winWave == 10` on Ground Zero; key name per campaign.rs).

**Commit.** `Gameplay G04 T5: adopt sector map/save rules wholesale, planet ruleSetter, campaign load overrides, limitMapArea`

---

### T7 — Capture/defeat side effects: enemy base turns derelict, `coreDestroyClear`, save on capture, `canGameOver`, `Logic.play` heal

**Symptom / acceptance.** Capturing an attack sector (enemy core destroyed)
leaves every enemy turret/wall/factory on the wave team: turrets keep shooting
after "Sector captured", enemy units keep attacking. Destroying any AI core
on Serpulo/Erekir does not convert the buildings around it
(`coreDestroyClear`). The capture is not saved immediately. After: on capture
the wave team's buildings become derelict (team 0) except ~20% that are
destroyed, cores are destroyed, units die; destroying an AI core converts all
non-privileged buildings within `enemyCoreBuildRadius` (400 world units = 50
tiles) that are not within that radius of another enemy core; a sector save is
queued on capture. Acceptance: test `capture_converts_wave_team_to_derelict`
(below) and in engine on an attack sector (e.g. frontier) after
`$MC.capture_sector()` every former enemy turret reports team 0 or is gone.

**Java.**
* `Logic` `SectorCaptureEvent` listener (`J/core/Logic.java:117-121`):
  `if(!net.client() && e.sector == state.getSector() && e.sector.isBeingPlayed()) state.rules.waveTeam.data().destroyToDerelict();`
* `TeamData.destroyToDerelict` (`J/game/Teams.java:331-362`):
  ```java
  plans.clear();
  for(var b : buildingTree.getObjects(builds)){          // every building of the team
      if(b.block.privileged) continue;
      if(b instanceof CoreBuild) b.kill(); else scheduleDerelict(b);
  }
  finishScheduleDerelict();                             // Call.setTeams(positions, Team.derelict) in chunks of 1000
  units.each(u -> Time.run(Mathf.random(0f, 60f * 5f), () -> { if(u.team == team) u.kill(); }));
  // scheduleDerelict (399-411): derelictBuffer.add(pos); if(build.getPayload() instanceof UnitPayload) Call.destroyPayload(build);
  //   if(Mathf.chance(0.2)) Time.run(Mathf.random(0f, 60f * 6f), build::kill);  build.addPlan(false, true);
  ```
* `Logic` `BlockDestroyEvent` (182-186):
  `if(e.tile.build instanceof CoreBuild core && core.team.isAI() && state.rules.coreDestroyClear) Core.app.post(() -> core.team.data().timeDestroy(core.x, core.y, state.rules.enemyCoreBuildRadius));`
  `timeDestroy` (`Teams.java:365-397`): for each building of the team within
  `range` of `(x,y)` (circle test `build.within(x, y, range)`), not privileged,
  and **not** within `range` of any remaining core of that team
  (`!cores.contains(c -> c.within(build, range))`) → `scheduleDerelict`; then
  `finishScheduleDerelict`; then the capture-replacement floors in a circle of
  `range/8` tiles (port: `MindSimHost::apply_capture_replacements` applies
  planet-wide; acceptable, record as deviation).
  `Rules.enemyCoreBuildRadius = 400f` (`J/game/Rules.java:125`); `Team.isAI()`
  (`J/game/Team.java:112-114`) = `(rules.waves || rules.attackMode || state.isCampaign()) && team != rules.defaultTeam && !rules.pvp`
  → add `pub fn team_is_ai(&self, team: u8, is_campaign: bool) -> bool` to the
  port `Rules` (`core/game/rules.rs`), `is_campaign = session.is_campaign()`.
* `Logic.sectorCapture` (398-401): `control.saves.saveSector(state.rules.sector)`.
* `Logic.update` (521, 600-603): `checkGameState()` only when `state.rules.canGameOver`.
* `Logic.play` (286-291): heal all cores of every active team (`entity.heal()`),
  loadout added for **every** active team with a core (276-284).

**Port today.**
* `core/game/play.rs:624-672` `sector_capture` returns
  `PlayEvent::SectorCapture`; nothing converts the wave team.
* `core/game/teams.rs:565` `Teams::destroy_to_derelict` works on
  `building_records`, which are only filled by `update_team_stats` (never called
  live — `grep -rn update_team_stats` shows tests only) → cannot be used live.
* Building death: `core/sim/unit_runtime.rs` `Sim::resolve_deaths` (building
  half from ~470: entities with `Building` + `Health.dead || health <= 0`).
  "Kill" in the port = set `Health.dead = true` (or health 0); the next
  `resolve_deaths` pass destroys it.
* `game_state_tick_system` (`runtime.rs:644`) always runs `check_game_state`.
* `gdext/campaign.rs` `pump_runtime`: `SectorCapture` → `apply_capture_bookkeeping`
  only (no save).

**Changes.**
1. `core/sim/runtime.rs` (or a new `core/sim/derelict.rs`, header per env §3):
   ```rust
   /// `Building.changeTeam(Team.derelict)` for `Call.setTeams` (`BuildingComp.changeTeam`).
   pub fn set_building_team(&mut self, world: &mut World, entity: Entity, team: u8)
   ```
   sets `TeamComp.team`, the P0 `BuildingComp.team` (`crate::ecs::BuildingComp`,
   field `team: TeamId`) when present, moves the block in `BlockCounter`
   (`counter.remove(old, block, 1)` + the matching add — find it with
   `grep -n "pub fn add\|pub fn remove" client/rust/mind-core/src/world/limits.rs`),
   and re-merges power (`remove_power_graph` then `merge_power_graph`).
2. `impl Sim`:
   ```rust
   /// `TeamData.destroyToDerelict` (Teams.java:331-362).
   pub fn destroy_team_to_derelict(&mut self, team: u8, rng: &mut SimRng)
   /// `TeamData.timeDestroy` (Teams.java:365-397).
   pub fn time_destroy(&mut self, team: u8, x: f32, y: f32, range: f32, rng: &mut SimRng)
   ```
   Iterate buildings in `entities_by_seq()` order (determinism), filter by
   team, skip `BlockDef.privileged`. Cores → kill. Others →
   `if rng.chance(RngStream::Sim, 0.2) { kill } else { set_building_team(.., 0) }`
   — deviation: Java kills the 20% after a random 0..360-tick delay and sets
   the team of ALL scheduled ones (killed ones die as derelict); the port
   kills them immediately and only converts the survivors (same as the existing
   `Teams::destroy_to_derelict` doc). Units of the team (UnitRuntime units with
   that `TeamComp`) → `Health.dead = true` (Java: random 0..300-tick delay).
   Also clear `session.teams.get(team).plans`. Use the `SimRng` the sim owns
   (`Sim.rng_streams: SimRng`, `core/sim/mod.rs:160`; draw with `rng.chance(RngStream::Sim, 0.2)`).
3. Capture hook: in `game_state_tick_system`, when the events contain
   `SectorCapture` for the current sector, set a flag on the runtime
   (`runtime.pending_derelict_team = Some(wave_team)`); `Sim::tick` cannot be
   re-entered from a system, so apply it right after the schedule in
   `Sim::tick` (after `resolve_deaths`, before `flush_events`) — add
   `fn apply_campaign_world_requests(&mut self)` that takes the flag from the
   `CampaignRuntime` resource and calls `destroy_team_to_derelict`. The same
   function drains a `pending_time_destroy: Vec<(u8, f32, f32, f32)>` queue
   for step 4.
4. `coreDestroyClear`: in `resolve_deaths`' building loop, before a dead
   building is removed: if it has `CoreBuild`, its team `!= rules.default_team`
   and `rules.team_is_ai(team, is_campaign)` and `rules.core_destroy_clear`, push
   `(team, x, y, rules.enemy_core_build_radius)` to the runtime queue
   (Java posts it to the next frame — `Core.app.post`). `time_destroy` checks
   "not within range of a remaining core" using the team's remaining core
   entities (query ECS `CoreBuild` + `TeamComp`).
4b. Captured-sector accessor for G01 ("AI units die on captured sectors",
   `J/entities/comp/UnitComp.java:756-758`:
   `if(team.isOnlyAI() && state.isCampaign() && state.getSector().isCaptured()) kill();`
   with `Sector.isCaptured` (`J/type/Sector.java:181-184`) = for the played sector
   `!state.rules.waves && !state.rules.attackMode`). Add in `core/game/runtime.rs`:
   ```rust
   /// `state.isCampaign() && state.getSector().isCaptured()` for the played sector.
   pub fn current_sector_captured(world: &World) -> bool {
       world.get_resource::<CampaignRuntime>().is_some_and(|r| {
           r.session.is_campaign() && !r.session.rules.waves && !r.session.rules.attack_mode
       })
   }
   ```
   and on the port `Rules` (`core/game/rules.rs`), next to `team_is_ai`:
   `pub fn team_is_only_ai(&self, team: u8, is_campaign: bool) -> bool { self.team_is_ai(team, is_campaign) }`
   (`Team.isOnlyAI = isAI() && data().players.size == 0`, `Team.java:117-119`;
   the port is single-player and the player is always on `default_team`, which
   `isAI` already excludes — document that). G01 calls both from its unit update.
   Test `captured_flag_follows_rules`: campaign runtime, `waves=false, attack_mode=false` → true; `waves=true` → false; no runtime → false.
5. Save on capture: `gdext/campaign.rs` `pump_runtime`, `SectorCapture` arm:
   after the bookkeeping, `self.persist_sector_record()` and queue
   `request_save(current slot file)` exactly like the GameOver arm.
6. `game_state_tick_system`: `if !runtime.session.rules.can_game_over { reinsert; return; }`.
7. `play.rs` `apply_launch_loadout` + `sync_session_with_sim` (`runtime.rs:308`):
   heal every registered core of every team (`Health.health = max_health`)
   when `apply_loadout` is true (fresh play only — `Logic.play` is not called
   on resume).

**Tests.**
* `core/sim/tests.rs` (or combat_tests): `capture_converts_wave_team_to_derelict`:
  runtime sim 64×64, place team-2 `core-shard` at (10,10), 10 team-2
  `copper-wall` in a row (20..30, 20), one team-2 dagger; call
  `destroy_team_to_derelict(2, &mut SimRng::new(7))`, then `sim.tick()` twice.
  Assert: no team-2 building remains; walls remaining all have team 0; wall
  count remaining == 10 − (number of `chance(0.2)` hits for seed 7 — compute by
  replaying `SimRng::new(7)` `chance` 10 times in the test, do not hard-code);
  the dagger is dead; the core is gone.
* `time_destroy_spares_buildings_near_another_core`: two team-2 cores 100 tiles
  apart, walls next to each; `time_destroy` around core A with range 400 →
  walls near A converted, walls near B untouched.
* `game_state_check_respects_can_game_over`: `can_game_over = false`, no player
  core → no `GameOver` event.

**Verify in-engine.** An attack sector of the imported profile (frontier,
preset `frontier`; find its sector id with
`$G --eval "return $MC.get_sector_state()"` or the planet dialog list). Enter
fresh (§4.1 fresh recipe with that id), then
```bash
$G --eval "return $MC.capture_sector()"
$G --eval "$H.step(5); return $H.hover_info(X, Y)"      # X,Y = an enemy turret tile found with hover_info before the capture
```
Expected: team 0 (derelict) or empty. `ls -l $U/saves/sector-serpulo-<id>.msav` mtime moves
at the capture. 0 panics.

**Pitfalls.** `destroy_team_to_derelict(&mut self, team, rng: &mut SimRng)`
borrows `self` mutably, so pass the sim's stream by swapping it out:
`let mut rng = std::mem::replace(&mut self.rng_streams, SimRng::new(0)); self.destroy_team_to_derelict(t, &mut rng); self.rng_streams = rng;`.
Never kill/convert inside a system while iterating the world;
collect first. `set_building_team` must not leave a stale entry in the
session `Teams` core list — after conversion the campaign tick's liveness
sweep only removes despawned cores; converted non-core buildings are not in
`cores`, so fine. Determinism: iterate `entities_by_seq`.

**Commit.** `Gameplay G04 T7: capture converts the enemy base to derelict, coreDestroyClear, save on capture`

---

### T3a — Missing per-kind codecs (projectors, generators/reactors, heat producer, radar, stack conveyor, beam drill, shield walls, crafter legacy warmup)

Depends on T1 (bodies are applied on load).

**Symptom / acceptance.** After T1, these blocks still restart from zero on
every load: mend/overdrive heat + phase heat, force projector buildup/broken
(a broken shield comes back intact), generator `productionEfficiency` and
`generateTime` (a combustion generator re-burns an item), reactor heat
(thorium reactor heat resets — a near-meltdown reactor becomes safe), impact
reactor warmup (must re-spin), heat producer heat, radar reveal progress,
stack-conveyor link/cooldown, beam-drill time/warmup, shield-wall shield.
After: each round-trips and reads Java bodies. Acceptance: one round-trip test
per block family (below), all green; the Java byte layouts are produced by
hand in the tests.

**Java layouts** (field order after `super.write`; revision gating on read):

| Java class (file:line) | `version()` | body |
|---|---|---|
| `PowerGenerator.GeneratorBuild` (`world/blocks/power/PowerGenerator.java:214-231`) | 1 | `f productionEfficiency`, `f generateTime` (read `generateTime` only if rev ≥ 1). Inherited by ConsumeGenerator (combustion/steam/differential/rtg/solar? — Solar and Thermal extend PowerGenerator too) |
| `NuclearReactor` (`NuclearReactor.java:203-212`) | 1 (inherited) | generator body + `f heat` |
| `ImpactReactor` (134-143) | 1 | generator body + `f warmup` |
| `HeaterGenerator` (73-82) | 1 | generator body + `f heat` |
| `VariableReactor` (162-177) | 1 | generator body + `f heat, f instability, f warmup` |
| `ForceProjector` (342-359) | 0 | `bool broken, f buildup, f radscl, f warmup, f phaseHeat` |
| `MendProjector` (167-178) | 0 | `f heat, f phaseHeat` |
| `OverdriveProjector` (162-173) | 0 | `f heat, f phaseHeat` |
| `BaseShield` (150-169) | 1 | `f smoothRadius, bool broken` (rev ≥ 1) |
| `ShieldWall` (120-130) | 0 | `f shield`; read: `if(shield > 0) shieldRadius = 1f` |
| `Radar` (98-109) | 0 | `f progress` |
| `StackConveyor` (389-403) | 0 | `i link, f cooldown`; read: `lastItem = items.first()` |
| `BeamDrill` (415-432) | 1 | `f time, f warmup` (rev ≥ 1) |
| `HeatProducer` (`heat/HeatProducer.java:71-80`) | 0 | GenericCrafter body (`f progress, f warmup`) + `f heat` |
| `GenericCrafter` (378-391) | 0 | `f progress, f warmup` + `f 0` when `legacyReadWarmup` (only `cultivator`, `Blocks.java:2969`) |

**Port state components** (verified): `GeneratorState { production_efficiency, generate_time, .. }`
(`core/world/blocks/power/generator.rs:137`), `NuclearReactorState.heat`
(`power/behavior.rs:618`), `ImpactReactorState.warmup` (528),
`VariableReactorState { heat, instability, warmup }` (807), heater generator
heat → `crate::world::blocks::heat::HeatState.heat` (`heat/mod.rs:28`; confirm
the heater behavior stores it there with `grep -n "HeatState" power/behavior.rs`),
`ForceProjectorState { broken, buildup, radscl, warmup, phase_heat }`
(`defense/shields.rs:69`), `MendProjectorState { heat, phase_heat }` (378),
`OverdriveProjectorState { heat, phase_heat }` (`defense/projectors.rs:29`),
`BaseShieldState { smooth_radius, broken }` (355), `ShieldWallState { shield, shield_radius }`
(`shields.rs:302`), `RadarState.progress` (`entities/comp/building.rs:198`),
`StackConveyorBuild { link, cooldown, last_item }` (`distribution/stack_conveyor.rs:37`),
`BeamDrillState { progress, warmup }` (`building.rs:167`; `progress` is Java `time`),
`CrafterState { progress, warmup }` (`building.rs:129`).
Behaviors (`impl BuildingBehavior for …`): `ConsumeGeneratorBehavior`,
`HeaterGeneratorBehavior`, `ThermalGeneratorBehavior`, `SolarGeneratorBehavior`,
`ImpactReactorBehavior`, `NuclearReactorBehavior`, `VariableReactorBehavior`
(`world/blocks/power/behavior.rs`), `ForceProjectorBehavior`,
`MendProjectorBehavior`, `OverdriveProjectorBehavior`, `BaseShieldBehavior`,
`ShieldWallBehavior` (`world/blocks/defense/behaviors.rs`; note `K::ShieldWall`
maps to `defense::WallBehavior` in `default_behavior` — check which one the
registry installs with `grep -n "ShieldWall" client/rust/mind-core/src/world/blocks/defense/*.rs`),
`RadarBehavior` (`world/behavior/defense.rs`), `StackConveyorBehavior`,
`BeamDrillBehavior` (`world/behavior/production.rs`), `HeatProducerBehavior`
(`world/blocks/heat/behavior.rs:35`), `CrafterBehavior` (`production.rs:323`).

**Changes.** For each behavior add `version()` (only when ≠ 0), `write`, `read`
exactly per the table, reading into the component with `get_mut` (skip when
the component is absent; never `unwrap`). Shared helpers in
`core/world/building_io.rs`:
```rust
pub fn write_generator_body(world: &World, e: Entity, w: &mut EntityWriter)   // f production_efficiency, f generate_time (0.0 when no GeneratorState)
pub fn read_generator_body(world: &mut World, e: Entity, r: &mut EntityReader, revision: u8)
```
Reactors/heater call these first. `CrafterBehavior`: write an extra `0.0`
and read an extra float when the block name is `cultivator` (Java flag
`legacyReadWarmup`; add `legacy_read_warmup: bool` to the crafter kind data if
there is a place for it, else compare `inst.name == "cultivator"` with a
comment citing `Blocks.java:2969`). `HeatProducerBehavior`: crafter body then
`HeatState.heat`. `ShieldWall` read: `if shield > 0.0 { shield_radius = 1.0 }`.
`StackConveyor` read: `last_item = ItemModule.first()` (find the "first item"
helper: `grep -n "pub fn first" client/rust/mind-core/src/world/modules/*.rs`).

**Tests** (new `#[cfg(test)]` cases next to each behavior or one file
`core/world/building_io_kinds_tests.rs` registered with `#[cfg(test)] mod` in
`building_io.rs`): for each family build the Java bytes by hand with a
`WireWriter` (e.g. force projector: `bool true, f 12.5, f 0.75, f 1.0, f 0.25`),
read through `inst.behavior.read(world, e, &mut r, rev)` on a spawned
building (pattern: `building_io.rs` test `spawn_smelter`), assert the fields
and `r.remaining() == 0`; then `write` → bytes equal the hand-built bytes.
Names: `force_projector_body_matches_java`, `mend_projector_body_matches_java`,
`overdrive_projector_body_matches_java`, `base_shield_body_matches_java`,
`shield_wall_body_matches_java`, `radar_body_matches_java`,
`stack_conveyor_body_matches_java`, `beam_drill_body_matches_java`,
`combustion_generator_body_matches_java` (`f 0.5, f 37.0`),
`thorium_reactor_body_matches_java` (`f 1.0, f 10.0, f 0.42`),
`impact_reactor_body_matches_java`, `heat_producer_body_matches_java`,
`cultivator_body_has_legacy_warmup`.

**Verify in-engine.** headless only for the byte layouts; one in-engine
spot check: place a `mend-projector` + power source near the GZ core
(`place_block`), step 600, `run_exit_save`, resume, `building_debug` shows the
same `efficiency` and (add a `mend_heat` key to `building_debug` like T1 step 6)
the same heat ±0.01.

**Pitfalls.** Do not change any already-correct codec. The `version()` you
add changes nothing for old native v1 saves (their bodies were written with
version 0 semantics only for blocks that had a codec — these had none, so the
v1 body is empty and `apply_body` returns early).

**Commit.** `PAR-0161: per-kind codecs for projectors, generators, reactors, heat, radar, stack conveyor, beam drill`

---

### T3b — Codec layout fixes: PayloadBlock prefix (unit factory, reconstructor, payload blocks), payload read

Depends on T1.

**Symptom / acceptance.** Java bodies of unit factories/reconstructors start
with the `PayloadBlock` fields; the port reads `progress` from `payVector.x`
→ imported factories get garbage progress and plan; the payload carried by a
payload conveyor/router/loader/unloader/void/mass-driver is never read back
(`payload_block::read_base` reads `payVector`/`payRotation` only), so every
payload on a payload belt vanishes on load and the following fields are
misaligned. Acceptance: tests below; in engine a dagger sitting on a payload
conveyor survives `run_exit_save` + resume (`building_debug` of the conveyor
reports a payload).

**Java.**
* `PayloadBlock.PayloadBlockBuild` (`world/blocks/payloads/PayloadBlock.java:276-292`):
  write `f payVector.x, f payVector.y, f payRotation, Payload.write(payload)`;
  read the same with `payload = Payload.read(read)`.
* `Payload.write/read` (`Payload.java:87-118`): `bool exists`; if exists:
  `b type` (0 unit, 1 block); block: `s block.id, b build.version(), build.writeAll(...)`
  (a full nested building chunk without length prefix); unit: `b classId, unit.write(...)`
  (the generated unit IO, see T6).
* `UnitFactory` extends `UnitBlock` extends `PayloadBlock`
  (`UnitFactory.java:486-506`): body = PayloadBlock body + `f progress, s currentPlan, vecNullable commandPos (rev≥2), command (rev≥3)`.
* `Reconstructor` (413-436): PayloadBlock body + `f progress (rev≥1), vecNullable (rev≥2), command (rev≥3)`.
* `PayloadConveyor` (314-330): NOT a PayloadBlock: `f progress, f itemRotation, Payload item`.
  `PayloadRouter` extends PayloadConveyor (+ rev≥1: `b ctype, s sorted, b recDir`).
  `PayloadLoader`/`PayloadUnloader` extend PayloadBlock (+ `bool exporting` rev≥1).
  `PayloadMassDriver` (487-512) PayloadBlock + `i link, f turretRotation, b state, [f reload, f charge, bool loaded, bool charging]`.
  `PayloadDeconstructor` (268-296) PayloadBlock + `f progress, s n, f×n accum, Payload deconstructing`.
  `PayloadSource` (172-187) PayloadBlock + `s unit, s configBlock, vecNullable (rev≥1)`.
  `BlockProducer` (182-191) PayloadBlock + `f progress`; `Constructor` (90-99) BlockProducer + `s recipe`.

**Port today.**
* `core/world/blocks/payloads/payload_block.rs:420-434` `write_base` writes
  the payload (`super::payload_write`), `read_base` does not read it.
* `core/world/blocks/units/behavior.rs:238-275` (`UnitFactoryBehavior`) and
  `509-540` (`ReconstructorBehavior`) write/read without the PayloadBlock
  prefix.
* Payload codec entry points: `grep -n "pub fn payload_write\|pub fn payload_read" client/rust/mind-core/src/world/blocks/payloads/*.rs`
  (only the write exists today, verify).

**Changes.**
1. Implement `pub fn payload_read(world: &mut World, r: &mut BuildingReader) -> Option<PayloadRef-or-equivalent>`
   mirroring `Payload.read`: block payload → spawn a detached building of
   `block` (the same way `payload_write`'s counterpart spawns pickup payloads —
   find the constructor used when a building is picked up, e.g.
   `grep -n "fn make_build_payload\|BuildPayload" client/rust/mind-core/src/world/blocks/payloads/*.rs`)
   then `read_base_legacy` + `apply_base` + `apply_body` on it (nested
   `readAll(read, version)` with the version byte just read). Unit payload →
   until T6 exists: log and return `None`, and STOP reading the rest of the
   body (return a flag so the caller bails; the chunk is bounded so the next
   building is safe).
2. `payload_block::read_base`: after `pay_rotation`, `holder.payload = payload_read(...)`.
3. Unit factory / reconstructor: call `payload_block::write_base` first in
   `write` and `payload_block::read_base` first in `read`. Persist the
   reconstructor's `command_pos` and the factory's `command_pos`/`command` if
   the components get those fields (they do not today: keep writing
   `write_vec_nullable(None)` / `write_command(None)`).
4. Audit the payload-family codecs against the table above (field order and
   revision gates) and fix any difference you find; list them in the commit
   body.

**Tests.** `payload_conveyor_keeps_build_payload`: spawn a `payload-conveyor`,
give it a `copper-wall` build payload (via the existing pickup helper),
`write_all_java` → `read_base_legacy` + `apply_body` on a fresh entity →
the payload exists and is a `copper-wall`. `unit_factory_body_matches_java`:
hand-built Java bytes `f 0, f 0, f 0, bool false, f 12.5, s 0, f NaN, f NaN, b -1`
(check `write_command(None)` byte form in `core/io/typeio/codecs.rs:313`) →
progress 12.5, plan 0, `r.remaining() == 0`.

**Verify in-engine.** headless only (payload belts are rare on the imported
profile); optional: place `payload-conveyor` + drop the player's payload.

**Commit.** `PAR-0161: PayloadBlock body prefix and payload read for payload blocks, factories, reconstructors`

---

### T8 — Sector record ↔ world: `SectorInfo.write` on resume, `prepare` gaps, invasion of the played sector, campaign-rule defaults, preset `quietUnlock`

**Symptom / acceptance.**
(a) Items a sector earns while you are away (turn production, legacy imports)
land in `SectorInfo.items` but never reach the core when you resume — the
resumed core shows the save's old inventory (Java writes the record into the
core on load). (b) A sector invaded while you are playing it never turns its
waves back on. (c) New profiles (no `serpulo-campaign-rules` key) get
`sectorInvasion` and `legacyLaunchPads` enabled; Java defaults both to false.
(d) Landing on a preset sector never `quietUnlock`s it. (e) The record keeps
exports although no launch pad is active, and never stores `lastPresetName`
(so `sectorDataMatches` can fail and break the lost-sector replay).
Acceptance: tests below; in engine: put 500 extra copper into GZ's record
items via a turn (or edit the record JSON in a test profile), resume → core
copper equals the record (clamped to capacity 4000).

**Java.**
* `SectorInfo.write()` (`J/game/SectorInfo.java:196-225`), called from the
  `SaveLoadEvent` listener on resume (`Logic.java:84`):
  ```java
  if(state.rules.waveTeam.core() != null){ attack = true; if(!planet.allowWaves) winWave = 0; }
  if(winWave <= 0 && !attack && planet.allowWaves) winWave = 30;
  if(preset != null && preset.captureWave > 0 && !planet.allowWaves) winWave = preset.captureWave;
  state.wave = wave; state.rules.waves = waves; state.rules.waveSpacing = waveSpacing;
  state.rules.winWave = winWave; state.rules.attackMode = attack;
  CoreBuild entity = state.rules.defaultTeam.core();
  if(entity != null){ entity.items.clear(); entity.items.add(items);
      entity.items.each((i, a) -> entity.items.set(i, Mathf.clamp(a, 0, entity.storageCapacity))); }
  ```
* `SectorInfo.prepare` (227-280): also `lastPresetName = preset == null ? null : preset.name`,
  `lastWidth/lastHeight = world size`, `export.clear()` when no
  `BlockFlag.launchPad` building of the default team has `efficiency > 0`
  (264-269), `bestCoreType` = largest core.
* `Universe.runTurn` invasion (`J/game/Universe.java:225-251`) for a sector
  that `isBeingPlayed()`: `waveMax = max(info.winWave, state.wave) + random(2,4)*5`;
  `state.rules.winWave = waveMax; state.rules.waves = true; state.rules.attackMode = false;`
  `planet.campaignRules.apply(planet, state.rules)`; fire `SectorInvasionEvent`.
* `Planet.init` (`J/type/Planet.java:446-451`): `applyDefaultRules(campaignRules)` =
  `JsonIO.copy(campaignRuleDefaults, rules)` — `campaignRuleDefaults` is a plain
  `new CampaignRules()` (all false, difficulty normal) except erekir
  (`fog = true, hideSpawns = false, rtsAI = true`, `Planets.java:84-86`).
  `allowSectorInvasion`/`allowLegacyLaunchPads` only gate the checkboxes of
  `CampaignRulesDialog`.
* `Control.playSector` (`Control.java:443-445`): `if(sector.preset != null) sector.preset.quietUnlock();`.

**Port today.**
* `core/game/sector.rs:454-483` `SectorInfoState::write` writes params INTO
  the record (wrong direction) and has no live caller.
* `play.rs:461-472` `play_sector` copies `record.wave/waves/wave_spacing/win_wave/attack`
  into the session but skips the winWave/attack adjustments and the core items.
* `gdext/campaign.rs:1826-1877` `persist_sector_record` + `sector.rs:419`
  `prepare`: no `last_preset_name`, no export clear.
* `core/game/universe.rs:715-772` `invasion_pass` treats every sector as
  not-played (applies to `info` only; `waveMax` from `info.wave`).
* `core/game/planet.rs:178-188`: `sector_invasion: def.allow_sector_invasion`,
  `legacy_launch_pads: def.allow_legacy_launch_pads`.
* `start_sector_with_loadout` never unlocks the preset.

**Changes.**
1. Replace `SectorInfoState::write` with the Java semantics:
   ```rust
   /// `SectorInfo.write()` (SectorInfo.java:196-225): record → live state.
   pub fn write_to_state(&mut self, wave_team_has_core: bool, planet_allow_waves: bool,
                         preset_capture_wave: i32, session: &mut PlaySession)
   ```
   adjust `self.info.attack/win_wave` as Java does, then set
   `session.wave`, `rules.waves/wave_spacing/win_wave/attack_mode`. Delete the
   old `write` and update its test `prepare_and_write_populate_fields`
   (`sector.rs:823`) to the new direction. `play_sector` (`play.rs:461`)
   keeps its record copy but must call `write_to_state` instead once the world
   is known — since `wave_team_has_core` needs the loaded world, call it from
   `start_sector_with_loadout` after `sync_campaign_session` (the session's
   `wave_core_count()` is then valid), only on the resume branch.
2. Core items: new `MindSimHost` Rust-only
   `pub fn write_record_items_to_core(&mut self, team: u8, items: &IndexMap<String, i32>)`:
   first default-team core (lowest `EntitySeq`), `ItemModule.clear()`, then for
   each item `set(item, amount.clamp(0, CoreBuild.storage_capacity))`. Call it
   on the resume branch right after step 1 (own statement).
3. `prepare`: set `last_preset_name` (pass the preset name in from
   `persist_sector_record`), and clear `export` when the default team has no
   launch pad with `efficiency > 0` (needs a `campaign_core_snapshot` field
   `active_launch_pads: usize` — extend `CoreSnapshot` (`sector.rs:58`
   `core_snapshot_of`) by counting `LaunchPadBehavior` buildings
   (`BlockKind::LaunchPad`) of the team with `Building.efficiency > 0.0`).
4. Invasion of the played sector: `TurnContext` already carries
   `current_sector` and `current_wave` (`gdext/campaign.rs` `pump_runtime`).
   In `invasion_pass`, when `Some((planet.id, sector.id)) == ctx.current_sector`,
   use `base = max(info.win_wave, ctx.current_wave)` and push a
   `CampaignEvent::SectorInvasion { …, played: true }` (add the field); do not
   write `info` for it. In `pump_runtime`, after `run_turn`, for a `played`
   invasion set `runtime.session.rules.win_wave = wave_max; waves = true; attack_mode = false;`
   then `planet.campaign_rules.apply_planet(planet, &mut rules, true)`
   (the port's `CampaignRules.apply`, `game/campaign_rules.rs`).
5. Defaults: `planet.rs` `from_def`: `sector_invasion: false`,
   `legacy_launch_pads: false` (keep `fog/hide_spawns/rts_ai` from
   `campaign_rule_defaults`). Settings-loaded rules still override
   (`loadRules`). Check the golden UI campaign view: if `campaign.json`
   changes, regenerate per env §2 with the reason "Java Planet.applyDefaultRules".
6. `start_sector_with_loadout`: after the lock check, when `preset_id` is
   `Some(p)`: `if let Some(fields) = tech_tree::unlock_fields_mut(&mut registry, ContentRef::of(ContentType::Sector, p)) { fields.quiet_unlock(&name, &mut SettingsUnlockStore::new(&mut self.settings)); }`
   (`content/ctype.rs:217`; `unlock_fields_mut` is used the same way in
   `core/game/tech_tree.rs:303`; it may be private — make it `pub` if needed).

**Tests.**
* `sector.rs`: `write_to_state_applies_record_and_win_wave`: record
  `{wave: 7, waves: true, win_wave: 0, attack: false}`, planet allows waves, no
  enemy core → session `wave == 7`, `win_wave == 30`; with enemy core →
  `attack_mode == true`.
* `universe.rs`: `invasion_of_played_sector_targets_live_rules`: force the
  chance (seed a `JavaRandom` whose first `next_double()` < the chance; compute
  the seed in the test by scanning seeds 0..1000 with the same formula) →
  event `played: true`, `info` unchanged.
* `planet.rs`: `serpulo_campaign_rule_defaults_match_java`: fresh campaign →
  `!sector_invasion && !legacy_launch_pads && !fog`.

**Verify in-engine.** Clean re-import (§4.3), then check the defaults are
loaded from the profile (expect the profile's values, not the defaults, if
`serpulo-campaign-rules` exists — record which). Resume GZ (§4.1 Go) and
compare `core_items_json()` with the record
(`$MC.get_sector_state()` — inspect the `items` key name). 0 panics.

**Commit.** `Gameplay G04 T8: SectorInfo.write on resume, prepare gaps, played-sector invasion, campaign rule defaults, preset quietUnlock`

---

### T10 — Launch pads launch items (v8 export path)

**Symptom / acceptance.** A powered `advanced-launch-pad` (or legacy
`launch-pad`) fed with items never launches: no pod, items stay, the sector
reports no export, the destination never imports. After: when
`launchCounter >= launchTime` and the pad is full, the items leave in a pod
entity that lives 120 ticks; on its removal the sector's export statistic grows
(and, with `legacyLaunchPads`, the destination sector gets the items).
Acceptance: test `launch_pad_launches_full_stack` and, in engine, an
advanced launch pad with 100 copper + oil + power empties within
`launchTime` (1800 ticks) and `get_sector_state()` shows a copper export after
the next `SectorInfo.update` refresh (60 ticks).

**Java** (`J/world/blocks/campaign/LaunchPad.java`):
* Content (`J/content/Blocks.java:6840-6860`): `launch-pad`: size 3,
  `itemCapacity 100`, `launchTime = 60*20 = 1200`, power 4/s, `acceptMultipleItems = true`;
  `advanced-launch-pad`: size 4, `itemCapacity 100`, `launchTime = 60*30 = 1800`,
  `liquidCapacity 40`, consumes oil `9/60` per tick + power 8/s,
  `acceptMultipleItems = false` (default).
* `acceptItem` (159-161): `items.total() < itemCapacity && (acceptMultipleItems || items.total() == 0 || items.first() == item)`.
* `shouldConsume` (93-96): `launchCounter < launchTime`.
* `updateTile` (164-181):
  ```java
  if((launchCounter += edelta()) >= launchTime && items.total() >= itemCapacity){
      consume();
      LaunchPayload entity = LaunchPayload.create();
      items.each((item, amount) -> entity.stacks.add(new ItemStack(item, amount)));
      entity.set(this); entity.lifetime(120f); entity.team(team); entity.add();
      Fx.launchPod.at(this); items.clear(); Effect.shake(3f, 3f, this);
      launchCounter = 0f;
  }
  ```
* `version()` 1; `write: f launchCounter`; `read: if(rev >= 1) launchCounter = read.f()` (199-213).
* `LaunchPayloadComp.remove` (295-312): campaign + not client:
  `destsec = state.rules.sector.info.destination`;
  `if(team == defaultTeam && destsec != null && destsec != state.rules.sector)`
  → for each stack: `info.handleItemExport(stack)`, `Events.fire(LaunchItemEvent)`;
  if `campaignRules.legacyLaunchPads` → `destsec.addItems(dest)`.
* `outputsItems()` false; `display` destination label (UI, G05/G03).

**Port today.** `core/world/behavior/campaign.rs:113-163` `LaunchPadBehavior`:
`update_tile` adds `edelta` to `CampaignState.launch_time` when efficient; no
`accept_item`, no launch; IO writes `launch_time` with version 0.
`LaunchPadDef.launch_time` (`core/world/block_kind_data.rs:341`) defaults to 0
and is never filled from content (`content/registries/blocks/campaign.rs`
has no launch time). `CampaignHookQueues` (`core/game/runtime.rs:64`) is the
shared queue for campaign deltas.

**Changes.**
1. Content: add `launch_time` to the two launch-pad specs (1200, 1800) — add
   the field to `BlockSpec`/`BlockDef` if absent, or fill `LaunchPadDef` in
   `BlockKindData::from_def` (`block_kind_data.rs:623`) from the block name
   (`"launch-pad" => 1200.0, "advanced-launch-pad" => 1800.0`, cite
   `Blocks.java:6844,6854`). Also `accept_multiple_items: bool` (true only for
   `launch-pad`). Golden: content dump may change → regenerate per env §2.
2. `LaunchPadBehavior`:
   * `accept_item` per Java; `handle_item` default add (check the trait
     default handles `ItemModule` add; otherwise add `items.add(item, 1)`).
   * `should_consume` hook if the trait has one (`grep -n "fn should_consume" world/behavior/mod.rs`):
     `launch_time < def.launch_time`.
   * `update_tile`: `state.launch_time += edelta`; if `>= def.launch_time` and
     `items.total >= item_capacity`: collect `(item, amount)` stacks, clear the
     `ItemModule`, spawn `LaunchPayloadState { stacks, team, time: 0.0, lifetime: 120.0 }` +
     `Pos` at the pad center, reset `launch_time = 0`. Consume: the power/oil
     consumers are handled by the generic consume pass (verify that the
     building's `consume()` trigger exists — `grep -n "fn trigger_consume\|fn consume(" world/`; if
     not, skip and note).
   * IO: `version() = 1`, write/read `f launch_time` (rev ≥ 1).
3. New component `LaunchPayloadState` in `core/entities/comp/building.rs`
   (or a new `core/game/launch_payload.rs`) and a system
   `launch_payload_system` in the campaign set (`TickSet::Campaign`,
   `core/sim/schedule.rs:419` — add next to `campaign_tick_system`, after it):
   advance `time += 1`; when `time >= lifetime`: if `CampaignRuntime` exists,
   campaign, `team == default_team`, `info.destination` is `Some` and differs
   from the current sector → for each stack `sector.info.handle_item_export(name, amount)`
   (`core/game/sector.rs:245`) and, when the planet's
   `campaign_rules.legacy_launch_pads`, `dest.add_items(stacks)` + save the
   destination's info (`Sector::save_info` needs settings → instead push the
   destination key into a `pending_info_saves` list on the runtime that
   `pump_runtime` flushes with `persist`); despawn. Iterate payload entities by
   `EntitySeq` (give the payload one when spawning; see how fires get
   `EntitySeq` — they do not; sort by `(time, Pos.x, Pos.y)` instead).
4. Render seam for G03: `MindSimHost.launch_payloads_json()` #[func]
   returning `[{x, y, time, lifetime}]` (cross-area note: G03 draws it).

**Tests.** `campaign.rs` tests: `launch_pad_accepts_single_item_type` (advanced
pad: copper accepted, then lead rejected; legacy pad accepts both);
`launch_pad_launches_full_stack`: spawn an advanced pad with efficiency 1,
100 copper, `launch_time = 1799.0`, one `update_tile` with `edelta = 1` →
items empty, one `LaunchPayloadState` with `[(copper, 100)]`, counter 0.
`runtime.rs`: `launch_payload_exports_on_removal`: a campaign runtime whose
sector has `destination = Some(other)`, a payload with 100 copper at time 119 →
one `launch_payload_system` run → `sector.info.export["copper"].counter == 100`
(check the `ExportStat` field name) and the payload is gone.

**Verify in-engine.** Ground Zero (captured in the imported profile? if not,
any based sector): `place_block` an `advanced-launch-pad`, a `power-source`
next to it and a `liquid-source` configured to oil, `debug_add_building_items(x,y,"copper",100)`;
`step(1800)`; `building_items_json(x,y)` → `{}`; after 60 more ticks
`$MC.get_sector_state()` export section lists copper. Destination must be set
(`info.destination`): set it through the launch pad config if the UI exists,
otherwise via a test-only seam (state which in the commit).

**Commit.** `Gameplay G04 T10: launch pads launch items and record exports`

---

### T11 — Landing pads receive imports (v8 import path)

Depends on T10 (exports must exist for a non-zero import rate).

**Symptom / acceptance.** A configured `landing-pad` never receives anything.
After: with another based sector exporting item X to this sector, a pad
configured to X, with ≥1500 water and power, lands 100 X every
`itemCapacity / importRatePerFrame` frames (min `cooldownTime+arrivalDuration = 300` ticks),
consuming 1500 water over the 150-tick arrival, then dumps the items.
Acceptance: test `landing_pad_lands_when_timer_full`.

**Java** (`J/world/blocks/campaign/LandingPad.java`):
* Content (`Blocks.java:6862-6871`): size 4, `itemCapacity 100`,
  `liquidCapacity 3000`, `consumeLiquidAmount 1500`, `consumeLiquid = water`;
  class defaults `arrivalDuration 150`, `cooldownTime 150`. Efficiency =
  `liquids.get(water) >= 1500 ? 1 : 0` (custom `ConsumeLiquid`, 58-77).
* `updateTimers` (139-175, once per frame for all pads via static `lastUpdateId`):
  for every item: `importedPerFrame = importRate[item]/60`; if > 0:
  `importCooldownTimers[item] += 1 / (itemCapacity / importedPerFrame) * Time.delta`,
  else `importCooldownTimers[item] = 0`. Then for each item with waiting pads:
  drop pads whose config changed, sort by `priority`, `landingPadLanded(first)`,
  swap `first.priority` with `last.priority`, clear the list.
* `handleLanding` (119-131): `cooldown = 1; arriving = config; arrivingTimer = 0; liquidRemoved = 0;`
  campaign & not fake → `importCooldownTimers[config] = 0`.
* `updateTile` (240-345): `updateTimers()`; while `arriving != null`:
  `arrivingTimer += delta/arrivalDuration`; remove
  `min(1500/150*delta, 1500-liquidRemoved)` water; at `arrivingTimer >= 1`:
  remove the remainder, `items.set(arriving, itemCapacity)`, if not fake
  `produced(arriving, 100)` + `info.handleItemImport(arriving, 100)`,
  `arriving = null`. Then `if(items.total() > 0) dumpAccumulate(config == null || items.get(config) != items.total() ? null : config)`.
  If not arriving: `cooldown = clamp(cooldown - delta/cooldownTime)`.
  If `config != null && (isFake() || campaign && !legacyLaunchPads)` and
  `cooldown <= 0 && efficiency > 0 && items.total() == 0 &&
  (isFake() || importRate(config) > 0 && importCooldownTimers[config] >= 1)`:
  fake → land immediately; else add to `waiting[config]`.
  `isFake() = team != defaultTeam || !campaign` (an item source for maps).
* `priority = Mathf.rand.nextInt()` per build; IO (2349-2372): `item config, i priority, f cooldown`, rev ≥ 1: `item arriving, f arrivingTimer, f liquidRemoved`; `version()` 1.
* `acceptItem` false; config `Item` (only when `accessible()` and the item is
  on the planet and unlocked) / clear.

**Port today.** `core/world/behavior/campaign.rs:75-111` `LandingPadBehavior`:
config + cooldown decrement only; no IO; `CampaignState { heat, cooldown, launch_time, item }`.
`SectorInfo` has `import_cooldown_timers` + `get_import_rate(item_index)`
(`core/game/sector.rs:313`) + `refresh_import_rates` (287).

**Changes.**
1. Extend `CampaignState` (or a new `LandingPadState` component — prefer a new
   component to keep launch-pad state separate):
   `config, priority: i32, cooldown, arriving: Option<ItemId>, arriving_timer, liquid_removed, land_particle_timer`.
   `priority` from the sim RNG at `create_state` (`rng.range` over i32 — find
   the int helper on `SimRng`).
2. Per-tile half in `LandingPadBehavior::update_tile` (arrival, liquid drain,
   cooldown, dump, "wants to land" flag `wants_landing: bool` on the state).
   Fake pads (non-default team or non-campaign) land immediately.
3. Global half in a new system `landing_pads_system` in `TickSet::Campaign`
   (after `campaign_tick_system`): update `import_cooldown_timers` of the
   played sector once per tick from `get_import_rate`; collect pads with
   `wants_landing` grouped by config item (BTreeMap by item id), sort by
   `priority`, land the first (call `handle_landing` logic on its state), swap
   priorities with the last, clear flags. Campaign + `!legacy_launch_pads` only.
4. IO per Java (version 1).
5. `handle_item_import` (`sector.rs:251`) on landing; the `produced()` half
   goes through the existing `handle_production` queue.

**Tests.** `landing_pad_lands_when_timer_full`: campaign runtime with
`import_rate(copper) = 1.0/s` (inject through the info cache), pad configured
to copper with 3000 water, efficiency 1 → after the timer reaches 1
(`100 / (1/60) = 6000` ticks → set the timer to 0.999 and run 1 tick:
`0.999 + 1/6000 >= 1`) the pad starts arriving; after 150 more ticks it holds
copper 100 and water `3000 - 1500 = 1500`. `fake_landing_pad_is_an_item_source`:
team 2 pad in a non-campaign sim lands without imports.

**Verify in-engine.** headless only unless two based sectors with exports
exist in the profile (record which).

**Commit.** `Gameplay G04 T11: landing pads import items (v8)`

---

### T9 — Unit save chunk: entity count equals written chunks; revision 3 carries Java's state

**Symptom / acceptance.** (a) `LiveEntitySource::entity_count`
(`core/io/sim_io.rs:305`) counts `unit_count()` (entity-validated) while
`write_entities` (327) skips units lacking `UnitCore/UnitTypeComp/Pos` or with
an unknown content name → the `entities` region announces more chunks than it
writes; the reader then consumes the fire/puddle/next-region bytes as unit
chunks (load fails or garbles). (b) Units lose status effects, shield,
carried items, elevation (boosting mechs land), `spawnedByCore`, their
command/target, and builder plans on every save. Acceptance: test
`entity_count_matches_written_chunks` (unit without `UnitCore` → save/load
succeeds and the other units survive) and `unit_rev3_round_trip`.

**Java.** `SaveVersion.writeWorldEntities` (`J/io/SaveVersion.java:425-439`):
count = `Groups.all.count(serialize) + Groups.unit.size()`, then one chunk per
entity. Unit fields (latest revision manifests `JA/resources/revisions/alpha/5.json`
etc.): `abilities, aimX, aimY, controller, elevation, flag(double), health,
isShooting, mineTile, mounts, plans, rotation, shield, spawnedByCore, stack,
statuses, team, type, updateBuilding, vel, x, y` (+ `baseRotation` for mechs,
`payloads` for payload carriers).

**Port today.** see Symptom; DecodedUnit (`sim_io.rs:101-117`).
Unit components: `UnitCore { rotation, dead, elevation, boosting, spawned_by_core }`
(`core/entities/comp/unit/comp.rs:20-31`), `Health`, `UnitPayloads`. Find the
status/shield/item-stack/command components with
`grep -n "pub struct" client/rust/mind-core/src/entities/comp/unit/*.rs` (names
to look for: status list, `Shield`/`shield`, item stack, `CommandAI`/command
state, build plan queue).

**Changes.**
1. Build the list once: `fn live_unit_rows(&self) -> Vec<(u64, Entity, String)>`
   (seq, entity, content name) with the same filters as `write_entities`;
   `entity_count = rows.len() + fires + puddles`; `write_entities` iterates the
   same rows.
2. `SIM_UNIT_REVISION = 3`: after the rev-2 payload stack append, in this
   order: `bool spawned_by_core`, `f elevation`, `f shield` (0 if no shield
   component), `s stack_item (-1 none), i stack_amount`, `i status_count` +
   per status `s effect_id, f time` (dynamic multipliers not stored — note),
   `b command_id (-1 none)`, `bool has_target_pos` + `f x, f y`, `i plan_count`
   + per plan `typeio::codecs::write_plan` (reuse the Java `writePlan` codec,
   `core/io/typeio/codecs.rs:655`). Reader accepts 1, 2, 3.
3. Restore in `apply_decoded_entities` (T2) / `SimIoHandler::load`: set each
   component when present; `spawned_by_core` units are skipped (T2 rule now
   uses the flag; keep the type-name fallback for rev ≤ 2).
4. Update `live_unit_class_id_is_reserved` if it asserts the revision.

**Tests.** `entity_count_matches_written_chunks`: spawn 2 daggers, remove
`UnitCore` from one (`world.entity_mut(e).remove::<UnitCore>()`), save, load →
`IoStatus::Ok`, 1 dagger restored, fires/puddles (spawn one fire) restored.
`unit_rev3_round_trip`: dagger with elevation 0.5, item stack copper×10, one
status (`burning`, 120 ticks), command `move` with target (100,100) →
identical after load. Old rev-2 bytes (hand-built) still decode.

**Verify in-engine.** T2's unit check plus: `unit_debug_json()` after resume
shows the same carried item for a mono that was mining (if the profile has
one; else headless only).

**Commit.** `Gameplay G04 T9: unit chunk count matches written chunks; unit save revision 3`

---

### T6 — Java save import: units, fires, puddles (class-id chunks)

Depends on T2 (entity sink on the resume path) and T9 (the components restored).

**Symptom / acceptance.** Loading a Java save (imported sector save on its
first resume, or Load Game of a Java `.msav`) drops every unit, fire and puddle
(`LiveEntitySink::supports_class` accepts only 0xF0-0xF2). After: Java unit
chunks of the **latest** revision of each class decode into port units (type,
team, position, rotation, health, shield, elevation, statuses, item stack,
spawnedByCore, command target); fires (class 10) and puddles (class 13)
decode. Older revisions and payload-carrying units are skipped with a debug
log (bounded chunks make that safe). Acceptance: test
`java_unit_chunk_decodes` with bytes hand-built from the manifest layout, and
`java_gz_fixture_skips_core_unit` (the r05 fixture has 1 entity, class 0
alpha, `spawnedByCore` → 0 units restored, no error).

**Java.**
* Entity chunk (`SaveVersion.java:441-448`): `b classId, i id, entity.write(out)`;
  generated `write` (`JA/java/mindustry/annotations/entity/EntityIO.java:83-96`)
  = `s revision` then the latest revision's fields in manifest order.
* Field serializers (`EntityIO.io`, 233-300 + `TypeIO`): primitives as
  themselves (`boolean` → `bool`, `double` → `d`); content types (`UnitType`,
  `Liquid`, …) → `s id`; `Team` → `b id` (`TypeIO.java:1126`); `Tile` → `i pos`
  (`574`); `Vec2` → `f x, f y` (1013); `ItemStack` → `s item, i amount` (1080);
  `Ability[]` → `b len, f data×len` (429-452); `WeaponMount[]` →
  `b len, (b state, f aimX, f aimY)×len` (393-425); `UnitController` →
  `writeController` (776-835: type 0 player `i id`; 3 logic `i pos`; 9 command:
  `bool hasAttack, bool hasPos, [f,f], [b kind, i target], b command, ub queueLen, (b tag + data)…, ub stanceCount, stances`; 5 assembler; else 2);
  `Seq<T>`/`Queue<T>` → `i size` + element serializer (`StatusEntry` →
  `writeStatus` 1031-1056, `BuildPlan` → `writePlan` 649-659, `Payload` →
  `Payload.write`); `Building` → `i pos`.
* Class ids + names: `JA/resources/classids.properties` (alpha 0, atrax 1,
  block 2, flare 3, mace 4, mega 5, FireComp 10, PuddleComp 13, mono 16,
  nova 17, poly 18, pulsar 19, risso 20, spiroct 21, quad 23, corvus 24,
  vela 25, oct 26, arkyid 29, beta 30, gamma 31, quasar 32, toxopid 33,
  manifold 36, timedDef 37, timed 38, missile 39, vanquish 40, stell 43,
  osc 44, elude 45, latum 46, renale 47, dummy 49). Latest revisions: alpha 5,
  beta 5, gamma 5, flare 9, mace 9, mega 7, mono 8, nova 7, poly 7, pulsar 5,
  risso 9, spiroct 8, quad 8, corvus 9, oct 7, arkyid 5, quasar 5, toxopid 5,
  manifold 3, missile 3, vanquish 1, stell 2, osc 0, elude 2, latum 2,
  scuttler 1, dummy 0, block 9, FireComp 1, PuddleComp 1 (directory listing of
  `JA/resources/revisions/`). The class id identifies the entity CLASS, not the
  unit type: the unit type is the `type` field.
* Fire rev 1 fields: `lifetime f, tile i(pos), time f, x f, y f`; Puddle rev 1:
  `amount f, liquid s, tile i, x f, y f`.

**Changes.**
1. New file `core/io/java_entities.rs` (header per env §3) with a static table
   generated by hand from the manifests (one `&[JField]` per (class, revision);
   `enum JField { Abilities, AimX, …, BaseRotation, Payloads, Building, Ammo, Lifetime, Time }`)
   and `pub fn read_java_unit(class_id, r, registry) -> IoResult<Option<DecodedUnit>>`:
   read `revision = r.us()?`; if `(class, revision)` is not the table's latest →
   `Ok(None)`; read fields in order with the serializers above; payload seq with
   count > 0 → `Ok(None)`. Map to `DecodedUnit` (name from `registry.unit(type)`).
   `read_java_fire`, `read_java_puddle` likewise.
2. `LiveEntitySink`: accept the Java class ids when the load is a legacy/Java
   save (`custom_name` is None and class < 0xF0); dispatch to the readers.
   Pass `&ContentRegistry` to the sink (it needs unit/status names; the
   decoder in T2 has the registry — store a cloned name table in the sink to
   avoid borrow conflicts with `SaveReadState.content`).
3. Skip `spawnedByCore` units (T2 rule).

**Tests.** `java_unit_chunk_decodes`: hand-built dagger (class 4 "mace",
revision 9) bytes in manifest order (`abilities b0; aimX f; aimY f; baseRotation f;
controller b2; elevation f0; flag d0; health f130; isShooting bool; mineTile i-1(pack -1,-1);
mounts b0; plans i0; rotation f90; shield f0; spawnedByCore false; stack s-1 i0;
statuses i0; team b2; type s<dagger id>; updateBuilding false; vel f0 f0; x f80; y f96`)
→ `DecodedUnit { name: "dagger", team: 2, x: 80, y: 96, rotation: 90, health: 130 }`;
`java_gz_fixture_skips_core_unit` (T1 fixture).

**Verify in-engine.** Clean re-import; the profile's GZ save has only the
core alpha, so in-engine shows no change — headless only for the decode.

**Commit.** `Gameplay G04 T6: decode Java unit, fire and puddle chunks on import`

---

### T12 — PAR-0164: one shared settings store

**Symptom / acceptance.** `MindUi`, `MindCampaign` and `MindMods` each hold a
`SettingsStore` snapshot (`gdext/ui/ui_host.rs:58`, `gdext/campaign.rs:71`,
`gdext/mods.rs:35`); a value written by one is invisible to the others until
reboot (worked around with `live_setting_flag*`, `campaign.rs:262-375`).
Java has one `Core.settings`. Acceptance: unit test `hub_write_is_visible_to_all`;
in engine toggling `skipcoreanimation` in Settings takes effect on the next
fresh launch without restart, and changing `saveinterval` changes the
autosave cadence immediately.

**Java.** `Core.settings` (Arc `Settings`): one in-memory map; `put*` marks
modified; `manualSave/forceSave` write the file.

**Port today.** see Symptom. `SettingsStore::force_save` merges only changed
keys into the on-disk map (`core/io/settings.rs:379-408`), so concurrent
stores no longer clobber, but reads are stale.

**Changes.**
1. New `gdext/settings_hub.rs` (header per env §3):
   ```rust
   static HUB: std::sync::OnceLock<std::sync::Mutex<SettingsStore>> = OnceLock::new();
   pub fn init(paths: &Paths)                         // load once; idempotent
   pub fn with_settings<R>(f: impl FnOnce(&mut SettingsStore) -> R) -> R   // lock, run, unlock; poisoned → recover with into_inner
   pub fn reload(paths: &Paths)                       // after data import / clear game data
   pub fn save(paths: &Paths) -> Result<(), IoError>  // force_save
   ```
   `mod settings_hub;` in `gdext/lib.rs`.
2. Migrate one facade per commit-slice (still one task; split into T12a/T12b/T12c
   commits if it grows): MindUi first (`ui_host.rs`, 22 uses of
   `self.settings`), then MindCampaign (46 uses), then MindMods (5 uses).
   Replace `self.settings.X(..)` with `settings_hub::with_settings(|s| s.X(..))`;
   where a mind-core API takes `&mut SettingsStore` together with other `self`
   fields (e.g. `campaign.save_all(&mut self.settings)`,
   `self.saves.set_current_by_file(&mut self.settings, ..)`,
   `SettingsUnlockStore::new(&mut self.settings)`), call it inside the closure:
   `with_settings(|s| campaign.save_all(s))`. Remove the `settings` fields.
   `ui_host.rs:895` / `1001` (data import / clear game data) → `settings_hub::reload`.
   `input/mod.rs:291` loads a temporary store for bindings → use the hub.
3. Delete `live_setting_flag`, `live_setting_flag_default`,
   `live_save_interval_seconds` and read the hub directly.

**Tests.** `gdext` has no test harness for nodes; put the hub logic in a
plain function and test it in `settings_hub.rs` `#[cfg(test)]` with a temp
dir: two closures (two "facades") — `put_bool("a", true)` in one,
`get_bool("a", false)` in the other → true; `save` then a fresh
`SettingsStore::load` sees it.

**Verify in-engine.** `tools/session-play.sh N --import`, then
`get_node("/root/MindUi").settings_put_flag("skipcoreanimation", true)` and a
fresh saltFlats launch (§4.1 fresh recipe, 98): no landing zoom
(`$C` zoom stays at the rig scale; check with the camera getter used in
campaign-import T3). Restore the flag afterwards.

**Pitfalls.** Never call into Godot (or another facade) inside
`with_settings` — `std::sync::Mutex` is not re-entrant: a nested
`with_settings` deadlocks the main thread. Take what you need, drop the lock,
then call. Grep for nested uses after the migration:
`grep -n "with_settings" client/rust/mind-gdext/src -r` and read each closure.

**Commit.** `Fixes PAR-0164: one shared settings store for MindUi, MindCampaign and MindMods`

---

### V1 — Verify PAR-0162 / PAR-0163 (no code expected)

Both look fixed by campaign-import T1 (`44b0f7b`): `MindCampaign::load_schematics`
(`gdext/campaign.rs:296-306`) walks `<data>/schematics/*.msch`;
`Schematics::load` re-registers loadouts after the sort
(`core/game/schematics.rs:521-553`); `launch_dialog_json` (2352) computes the
capacity with `launch_capacity_multiplier`.
Check: after `--import`, `ls $U/schematics | head` (expect `.msch` files from
the profile), open the planet page, select a based sector next to an unlocked
one, press Launch → Configure Loadout lists the profile's core schematics
(compare names with `ls $U/schematics`), and "Launching Item Capacity" is
non-zero (e.g. 2000 for a shard launcher at 0.5×4000). Screenshot both. If
both hold, report PAR-0162 and PAR-0163 → verified with the screenshots; if
not, write the exact failing observation in the summary (do not fix — the
dialog is G05's).

---

### Deferred (explicitly out of this plan, with reason)

* **Interplanetary Accelerator** (`J/world/blocks/campaign/Accelerator.java`,
  645 lines): planet launch (`ui.planet.showPlanetLaunch`, `unlockedOnLand`
  unlock, charge/launch cutscene, `launchBlock = coreNucleus` item consumption,
  power buffer 1 000 000). End-game only, needs G05 planet-launch UI and G03
  cutscene; only its IO is touched here (T3a: set `version() = 1`, keep the f32).
* **Erekir `prebuildBase` landing rebuild** (`Control.java:223-283`): Erekir
  campaign only; requires `Time.run` scheduling of building placement with
  FX. File a ledger item.
* **Java unit payload stacks / older unit revisions** on import (T6 skips
  them): rare in the operator profile (its only Java sector save has no units
  besides the core alpha).
* **`BlockUnit`/`timed`/`LaunchCoreComp`/`WeatherStateComp`/`WorldLabelComp`
  Java chunks**: weather states are re-rolled by `PlayEvent` (`Logic.java:93-101`),
  world labels are G06; skip.
* **`TargetDummy`, `RepairTurret` rotation, `UnitAssembler`, cargo units
  codecs**: no port state to restore (RepairTurret has no rotation field;
  dummy is sandbox) — Java bodies are skipped harmlessly (bounded chunk).
* **Turn RNG**: `Universe.runTurn` uses `Mathf` global RNG; the port uses
  `JavaRandom(turn+1)` (accepted deviation, deterministic).
* **`timeDestroy` capture-replacement circle**: port re-themes planet-wide
  (`apply_capture_replacements`); acceptable deviation, noted in T7.

---

## §3. Execution order, dependencies, shared files

Order (player impact first; dependencies respected):

1. **T1** PAR-0161 bodies (foundation for T3a/T3b).
2. **T2** sector resume full restore (uses T1's decode path; introduces `decode_save_bytes`).
3. **T3w** wave timer (independent; small).
4. **T4** objectives (independent; Ground Zero tutorial).
5. **T7** capture/derelict/coreDestroyClear (independent).
6. **T5** rules wholesale + limit area (touches `start_sector_with_loadout`, after T2 edited it).
7. **T3a** missing codecs (after T1).
8. **T3b** payload prefix (after T1).
9. **T8** SectorInfo.write / defaults / invasion / quietUnlock (after T2, T5 — same function).
10. **T10** launch pads, then **T11** landing pads.
11. **T9** unit chunk rev 3 (after T2).
12. **T6** Java entity import (after T2, T9).
13. **T12** settings hub (touches many gdext files; do last to avoid conflicts).
14. **V1** verification.

Shared files with other areas (merge carefully, small hunks):
* `gdext/campaign.rs` (G05 planet/launch dialogs, G03 HUD dict) — T2, T4, T5, T7, T8, T12.
* `gdext/sim_host.rs` (everyone) — T1 (`building_debug`), T2, T5, T8, T10.
* `core/game/runtime.rs` (G01 team AI, G03 weather) — T3w, T4, T7, T10, T11.
* `core/sim/unit_runtime.rs` `resolve_deaths` (G01) — T4 step 6, T7 step 4.
* `core/world/blocks/**` behaviors (G02 owns behaviour; this plan only adds
  `version/write/read` and the payload read) — T1, T3a, T3b, T10, T11.
* `core/world/build.rs` `valid_place` (G02/G08) — T5 step 6.
* `parity/golden_content.json` + `parity/golden_manifest.json` — T5 (planet
  fields), T10 (launch time), T8 (UI campaign golden). Regenerate only with
  the Java reason in the commit body.

---

## §4. Ledger changes to report (do not edit chains.json)

* PAR-0161 → `fixing` after T1, `verified` after T1+T3a+T3b with the fixture
  test + the in-engine sorter check. Note: "port v2 native format = Java tile
  chunk layout; v1 saves still load; core commandPos codec fixed; payload
  prefix fixed".
* PAR-0164 → `verified` after T12 (screens/evals in the commit).
* PAR-0162, PAR-0163 → `verified` if V1 holds (fixed by `44b0f7b`), else note.
* PAR-0153 stays verified; PAR-0084 stays verified (re-run its replay check
  after T2/T5: lost-sector replay must still restore survivors).
* PAR-0085 → `verified` (fixed by `44b0f7b` + `fe9381b`, per campaign-import summary).
* PAR-0131 → stays `fixing`; remainder (thrusters, hiding the real core) is G03.
* New items to file:
  * "Sector resume drops all units, fires, puddles, markers, wave timer and stats" (T2).
  * "Wave countdown runs 60x too slowly; isWaitingWave missing" (T3w).
  * "Map objectives: completion not saved, Timer 60x slow, DestroyUnits never completes, DestroyBlock completes instantly" (T4).
  * "Capturing a sector leaves the enemy base active (no destroyToDerelict); coreDestroyClear missing" (T7).
  * "Sector map/save rules dropped (limitMapArea, weather, …); planet ruleSetter and campaign load overrides missing" (T5).
  * "Launch pads never launch; landing pads never land" (T10/T11).
  * "Offline production never reaches the core on resume (SectorInfo.write)" (T8).
  * "Java save import drops units/fires/puddles" (T6).
  * "Unit save omits statuses/shield/items/elevation/commands; entity count can mismatch written chunks" (T9).
  * "Interplanetary Accelerator is a stub" (deferred).

---

## §5. Done criteria

* All tasks committed (one commit each) on the session branch with the test
  names and in-engine eval outputs in the commit bodies.
* `cargo test -p mind-core --lib --features msav-import` green (baseline from
  `G00-integrate-sessions_summary.md` + the new tests); `cargo test -p mind-core --test legacy_msav_import --features msav-import` green;
  `cargo test -p mind-headless --lib` has no failure outside the G00 known list;
  clippy `-D warnings` clean; GDScript parse gate clean.
* In engine after a clean re-import (§4.3): Ground Zero resume shows the core,
  `limit_area == [90,20,70,60]`, a sorter config and a spawned dagger survive
  `run_exit_save` + Go, the GZ tutorial Timer objectives advance in seconds,
  frozenForest wave timer drops 600 per 600 ticks, `capture_sector()` on an
  attack sector turns enemy turrets derelict; 0 `panic` in the log.
* Summary file `docs/parity-plans/G04-saves-campaign-loop_summary.md` per env §6.

---

## §6. Cross-area findings (not tasks here)

1. **G03** — Map border darkness/limit rect must be drawn once T5 sets
   `grid.limit_area` (`minimap.rs::tile_color` helper from rendering T19 exists);
   launch pod (`LaunchPayload`) drawing from T10's `launch_payloads_json()`;
   landing pad pod/arrival drawing (`LandingPad.draw`); PAR-0131 thrusters +
   hiding the real core during landing.
2. **G01** — Unit map-bounds clamp for non-AI teams under `limitMapArea`
   (`J/entities/comp/UnitComp.java:672-690`): the port has no bounds clamp at
   all (no `bounded` handling found in `core/sim/unit_runtime.rs`). PAR-0156
   `prebuildAi` core defenders (`Logic.java:541-556`) unchanged.
3. **G08** — `CommandModeObjective` needs the input side to call
   `MindSimHost::set_command_mode_satisfied(bool)` (→ `Sim::set_command_mode_satisfied`,
   resource `mind_core::game::runtime::ObjectiveInputState`; T4 adds them;
   Java: `control.input.selectedUnits.contains(u -> u.isCommandable() && u.command().hasCommand())`).
4. **G02** — `Separator` uses `CrafterBehavior` (no `seed`, version 0 vs Java
   1, `J/world/blocks/production/Separator.java:183-200`); `BufferedItemBridge`
   shares `ItemBridgeBehavior` without the `ItemBuffer` body; `BuildRules` is
   never synced from session rules except `default_team`/`env`
   (`core/game/runtime.rs:283-288`), so `infiniteResources`, `placeRangeCheck`,
   `derelictRepair` rule changes never reach placement.
5. **G06** — Logic block bodies (processor code/links, memory, message,
   switch, display, canvas) start being restored by T1; any Java-layout
   mismatch in `core/logic/blocks/*.rs` codecs is G06's to fix (T1 logs unread
   bytes at debug level — run with `RUST_LOG=debug` once on a save with
   processors, e.g. Ground Zero's 5 world processors, and hand the log lines to G06).
7. **G06** — after world-processor `setrule` mutates the session rules, set
   `CampaignRuntime.rules_world_dirty = true` (or call `Sim::apply_rules_to_world`)
   so `grid.limit_area` follows (T5 step 5). **G01** — use
   `game::runtime::current_sector_captured(world)` + `Rules::team_is_only_ai` (T7 step 4b).
6. **G05** — Sector record `lastPresetName`/`sectorDataMatches` feed the
   planet dialog; launch-pad destination selection UI (`LaunchPad.buildConfiguration`,
   `ui.planet.showSelect`) and landing-pad "redirect exports" button are G05.
