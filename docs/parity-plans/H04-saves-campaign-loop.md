<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H04 — Saves + the campaign loop (sim side of the campaign)

Date: 2026-10-14. Area: everything that turns "playing one sector" into a
Serpulo campaign: the sector record (`SectorInfo`), campaign turns
(`Universe.runTurn`: offline production, imports/exports, invasions), sector
resume/launch bookkeeping (`Control.playSector`/`playNewSector`, `Logic`
event listeners), match statistics (`GameStats`/`CampaignStats`), launch and
landing pads (v8 export/import), unit/entity save chunks and the shared
settings store.

**Read `docs/H-environment.md` first** (session/worktree/build/test/gmcp
rules, §4.1 sector entry, §4.2 seams, §6 commits). Then `docs/H-INDEX.md` §3
if it exists. This plan only references those sections.

Start state: `git log main --oneline | grep -m1 'H00'` must print a line
(H00 fixed `planet.rs::env_bit`, so Serpulo `rules.env == 233` and the player
alpha spawns on Ground Zero; `player:Some` placements queue plans). Assume
both are fixed. If the grep prints nothing, stop.

Drift rule: port line numbers were read on `ffbe84dd` (2026-10-14). Re-locate
every quoted function with `grep -n`. If a function does not exist, or the
code already does what a step asks, skip that step, write what you saw in the
summary, and continue. Java line numbers (`J/` =
`/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/`) were
checked on 2026-10-14 and are stable.

Abbreviations: `core/`, `gdext/`, `headless/`, `ui/`, `scenes/`, `J/`, `ARC/`
as in `H-environment.md`. `JA/` =
`/home/clintonnguyen/g/code_examples/Mindustry/annotations/src/main/` (Java
entity revision manifests). `MC` = `/root/Spine/MindCampaign`
(`gdext/campaign.rs`), `H` = `/root/Spine/SimHost` (`gdext/sim_host.rs`).

---

## §1. Inventory (Java -> port -> status)

Legend: **W** wired and correct, **P** partial, **B** present but wrong, **M**
missing.

| Java | Port | Status | Evidence (2026-10-14) | Task |
|---|---|---|---|---|
| Live campaign state while a sector runs (`Vars.state`, one `Universe`, one set of `Sector` objects) | Two copies: `MindCampaign.campaign` (facade) and `CampaignRuntime.campaign` (inside the sim) | **B** | `pump_runtime` copies runtime -> facade only when a `PlayEvent` landed (`gdext/campaign.rs` ~575). After a turn, `self.persist()` writes the **stale facade** `universe.turn/seconds` over the runtime's values (`Campaign::save_all` -> `Universe::save`, `core/game/universe.rs` ~778). `run_turn` #[func] (~1043) does the same. `set_campaign_rules_json` (~2870) overwrites `runtime.campaign` with the stale facade clone (drops runtime sector-info deltas). On exit (`run_exit_save` ~2420) the runtime is dropped without copying its campaign back, so other sectors' turn production stays only in settings, not in memory | **T1** |
| `Sector.isBeingPlayed` (`J/type/Sector.java:148-151`: `state.isGame() && rules.sector == this && !gameOver`) | `Sector.being_played` flag | **B** | Set true by `play_sector`/`play_new_sector`, never cleared on exit or when another sector starts -> a sector you left keeps `being_played == true`, so `Campaign::production_pass` skips it forever (no offline production) and `is_attacked` reads live rules. `sector_capture` (`core/game/play.rs` ~696) wrongly sets it false while still playing | T1, T4 |
| `SectorInfo.update` refresh every `refreshPeriod = 60` **ticks** (`J/game/SectorInfo.java:23,265`; Arc `Interval.get` counts `Time.time` ticks) | `REFRESH_PERIOD = 60.0` compared with a **seconds** accumulator (`core/game/sector.rs:30`, `update(1.0/60)` from `campaign_tick_system`) | **B** | Statistics refresh every 60 s instead of 1 s: means are items/minute, not items/second (`ExportStat.mean` doc: "per second"). Turn production `mean * 120 s` is 60x too large once production is recorded | **T2** |
| `ExportStat.counter` transient; `handleItemExport/Import` add to `counter`; `updateStats` adds `max(counter,0)` then zeroes it (`SectorInfo.java:123-130, 297-309, 354-365`) | `ExportStat { mean }` only; `handle_item_export/import` add to `mean`; `update_stat_map` samples `mean` | **B** | Exports/imports never decay; one launch counts forever | T2 |
| `BuildingComp.produced(item, amount)` -> `sector.info.handleProduction` + `item.unlock()` (`J/entities/comp/BuildingComp.java:1044-1050`); `offload(item)` calls `produced(item,1)` (1006-1007); drills/crafters/separators/beam drill/item source/landing pad call it | trait `produced` is a no-op (`core/world/behavior/mod.rs` ~378); `transfer::offload` (`core/world/blocks/distribution/transfer.rs` ~392) never records production | **M** | `rawProduction` is always 0 except unit mining (`entities/comp/unit/miner.rs` ~300) -> `production` is capped to 0 (`production = min(production, rawProduction)`) -> **sectors never produce anything while you are away**, and export is capped to 0 | T2 |
| `dumpLiquid` unlocks the liquid (`BuildingComp.java:896`) | — | M | Items/liquids are never unlocked by producing them | T2 |
| `SectorInfo.write()` on resume (`SectorInfo.java:165-196`, called from `Logic` `SaveLoadEvent` 70-88) | `SectorInfoState::write` (`core/game/sector.rs` ~454) writes the wrong direction and has no caller; `play_sector` (`play.rs` ~432) copies only `wave/waves/waveSpacing/winWave/attack` | **B** | Offline production/imports (record `items`) never reach the core on resume; `winWave`/`attack` adjustments missing; `info.hasCore = true` + `saveInfo` missing | **T3** |
| `SectorInfo.prepare` (`SectorInfo.java:199-256`) | `SectorInfoState::prepare` (`sector.rs` ~419) + `persist_sector_record` (`gdext/campaign.rs` ~2458) | P | Missing `lastPresetName` (so `sectorDataMatches` can never be true for presets after the first save), export clear when no active launch pad (244-249) | T3 |
| `Control.playSector` (`J/core/Control.java:431-560`): `preset.quietUnlock()` (443-445), `planet.setLastSector` (448), lost-sector replay only when `!clearSave && spawnPosition != 0 && sectorDataMatches` (466-468), `hadNoCore` | `start_sector_with_loadout` (`gdext/campaign.rs` ~630) | P | No `quietUnlock`, `set_last_sector` never called (`core/game/planet.rs` ~371 has no caller), replay ignores `sectorDataMatches`/`clearSectorOnLose`/`!info.hasCore` | T3 |
| `Control.playNewSector`: `info.origin = origin; info.destination = origin` (573-578) | `play_new_sector(…, origin, …)` | **B** | `start_sector_with_loadout` always passes `origin = None` -> a launched sector never gets a destination (exports go nowhere) | T3 |
| `Universe.runTurn` invasion of the **played** sector (`J/game/Universe.java:231-258`) | `Campaign::invasion_pass` (`universe.rs` ~715) | **B** | Ignores `ctx`; writes `info` of the played sector instead of live rules; never saves the invaded record; `waveMax` uses `info.wave` for the played sector; no `SectorInvasionEvent` consumer (Java toast `sector.attacked`, `J/ui/fragments/HudFragment.java:273-275`) | **T4** |
| `Planet.applyDefaultRules` -> `CampaignRules` all false except erekir fog/hideSpawns/rtsAI (`J/type/Planet.java:308-309,448`, `J/game/CampaignRules.java:7-16`) | `Planet::from_def` (`core/game/planet.rs` ~193-194) | **B** | `sector_invasion = allow_sector_invasion`, `legacy_launch_pads = allow_legacy_launch_pads` (both true for Serpulo); Java default false | T4 |
| `WaveSpawner.countSpawns()` = ground spawn tiles only (`J/ai/WaveSpawner.java:49-51`) | `sync_session_with_sim` sets `spawn_count = map_spawns.max(wave_cores)` (`core/game/runtime.rs` ~333) | B | `hasSpawns`/"no spawns -> waves off" use a value that includes enemy cores; `checkGameState` then adds `wave_core_count()` again | T4 |
| `Logic` listeners: `BlockBuildEndEvent` (187-200), `BlockDestroyEvent` (202-216), `UnitDestroyEvent` (218-226), `UnitCreateEvent` (228-236), `GameOverEvent` (244-248) -> `GameStats` + planet `CampaignStats` | only `placedBlockCount` (`core/sim/mod.rs` ~1799), `enemyUnitsDestroyed` (`unit_runtime.rs` ~539), `wavesLasted` | P | `buildingsBuilt/Deconstructed/Destroyed`, `unitsCreated`, `destroyedBlockCount`, planet `unitsProduced/buildingsBuilt/…` maps never incremented -> Game Over dialog shows zeros; hint `coresDestroyed` hard-coded 0 (`gdext/sim_host.rs` ~5284) | **T5** |
| `LaunchPad` (`J/world/blocks/campaign/LaunchPad.java`) | `LaunchPadBehavior` (`core/world/behavior/campaign.rs` ~117) | M | Only accumulates `launch_time`; `LaunchPadDef.launch_time` is 0 (never filled); no item accept, no launch, no `LaunchPayload`, IO version 0 | **T6** (PAR-0198) |
| `LandingPad` (`LandingPad.java`) | `LandingPadBehavior` (~78) | M | Config + cooldown only; no import timers, landing queue, water use, dump, IO; content spec has no water handling | **T7** (PAR-0198) |
| `Logic.update` wave timer + `runWave` gate | `campaign_tick_system` (`core/game/runtime.rs` ~369) | W (code) | G04 T3w correct (verified by reading: `wavetime - 1.0` per tick, unconditional `wavetime<=0 && waves` gate, `is_waiting_wave`). In-engine proof missing (no fixture) | **T8** (PAR-0193) |
| `SaveVersion.writeWorldEntities` count = written chunks (`J/io/SaveVersion.java:425-439`) | `LiveEntitySource::entity_count` / `write_entities` (`core/io/sim_io.rs` ~305/327) | B | Count includes units that `write_entities` skips -> corrupt entities region. Unit rev 2 lacks statuses/shield/stack/elevation/spawnedByCore/command/plans | **T9** |
| Java unit/fire/puddle chunks on import | `LiveEntitySink::supports_class` (sim_io.rs ~149) accepts 0xF0-0xF2 only | M | Every unit of a Java save is dropped | T10 |
| One `Core.settings` | three `SettingsStore`s (MindUi, MindCampaign, MindMods) | B (PAR-0164) | `live_setting_flag*` workarounds still in `gdext/campaign.rs` | T11 |
| `Block.ignoreBuildDarkness` (`J/world/Block.java:377`, used `J/world/Build.java:229,257`; true for `Floor`, `StaticWall`, `RemoveWall`, `RemoveOre`) | no field; `valid_place` darkness check unconditional (`core/world/build.rs` ~196) | M | editor/env blocks only | T12 |
| `BeamDrillState` component | `core/entities/comp/building.rs` ~171 | dead code | never inserted; codec uses `DrillState` | T12 |
| Markers region (UBJson `IntMap`, `J/game/MapMarkers.java:57-74`) | legacy path skips it (`core/io/save/versions/v1.rs` ~309) | deviation | decide with data (T12 step 3) | T12 |
| `Logic.checkGameState` loss branch | `check_game_state_campaign` (`play.rs` ~580) | W (read) | G03 summary claimed "loss flips rules.waves false"; code only does so when `spawn_count + wave cores <= 0` (Java too). G03 used sector 98 (saltFlats), **already captured** in the profile -> `waves:false` is expected there. Re-verify on Ground Zero | V1 |
| `SectorDamage.apply(fraction)` | `core/maps/sector_damage.rs` | W | Java has no offline "damage while away" simulation in this version (only `apply(1f)` in the lost-sector replay). Sector danger while away = invasion flags only (T4) | — |
| `Accelerator` (interplanetary launch) | stub | M | End-game, needs H05 planet-launch UI: **deferred**, file a ledger item | deferred |

### §1.1 Done — do not redo (spot-checked 2026-10-14 against the code)

All six G04 claims checked hold:
* **T1 per-kind bodies** (PAR-0161): `world/building_io.rs` ~445/549 call `inst.behavior.read(world, entity, r, revision)`.
* **T2 sector resume units/fires/puddles/markers/wavetime/stats**: `Sim::apply_decoded_entities` (`core/io/sim_io.rs` ~560) spawns units, `FireState`, `PuddleState`; `load_sector_world` (`gdext/sim_host.rs` ~1806) calls it and stores `SectorResumeMeta`; `start_sector_with_loadout` applies wavetime/stats/markers.
* **T3w wave timer**: `campaign_tick_system` subtracts `1.0` per tick and runs `runWave` on `wavetime <= 0 && waves`.
* **T4 objectives**: `MapObjectivesRuntime.completed` round-trips (`core/game/map_objectives.rs` ~155-183).
* **T5 rules wholesale**: `adopt_map_rules` (`gdext/campaign.rs` ~3537) + `apply_world_load_campaign_rules` + `apply_save_load_campaign_rules` + `Sim::apply_rules_to_world`.
* **T7 capture -> derelict + coreDestroyClear + save on capture**: `pending_derelict_team` set in `game_state_tick_system`, drained in `core/sim/runtime.rs` ~1364; `pending_time_destroy` queued in `unit_runtime.rs` ~685; capture save queued in `pump_runtime`.

Also done: PAR-0153 autosave, PAR-0081/0082 record persistence, PAR-0090 fire/puddle native save, PAR-0162 and PAR-0163 are already `verified` in the ledger (old V1 is dropped), PAR-0085 loadout, sector neighbours, `SectorInfo` field set.

No false "done" claim was found among the six; the bugs this plan fixes are in
code G04 did not touch (facade/runtime split, statistics, `produced`).

---

## §2. Tasks

Order = §3 order (gameplay impact). One commit per task. Every task ends with
`cargo test -p mind-core --lib --features msav-import`, clippy and the panic
grep (`H-environment.md` §2, §4).

Common in-engine preamble ("GZ resume"; H-environment §4.1 Go recipe, after a
clean re-import §4.3):
```bash
export OPEN_GODOT_MCP_PORT=$((6970 + 10*(N-1)))
G="python3 REPO/tools/gmcp.py --timeout 140"
H='get_node("/root/Spine/SimHost")'; MC='get_node("/root/Spine/MindCampaign")'
$G --eval 'return get_node("/root/MindUi").open_dialog("planet", "")'; sleep 3
$G --eval 'get_node("/root/Spine/Ui/UiRoot").dialog("planet")._sector_action_pressed("serpulo", 170, "go"); return true'; sleep 4
$G --eval "return $MC.get_hud_state().hasCore"          # true
```
After every in-engine step: `grep -c panic $(ls -t $U/logs/* | head -1)` must print `0`.

---

### T1 — One authoritative campaign while a sector is live (turn clock, offline production, being_played)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. GZ resume, settings `turn` = 2140; `$MC.run_turn()` twice (both `true`); `config/settings.bin` still `turn = 2140` (expected 2142) and still 2140 after `run_exit_save()` (`runs/20261014-s3-verify2/H04-evals.txt`). Until T1's `universe_json()` exists, read the persisted value with `python3 -I REPO/runs/20261014-s3-verify2/tools/settings_read.py $U/config/settings.bin '^turn$'`. Extra finding for Change 5: `remove_sector_items` (the PAR-0085 launch cost) returned `true` but the record in settings was unchanged (`items {copper:32}` before and after): it mutates only the facade copy and `persist()` never calls `save_info` for sector records (`Campaign::save_all` saves planet rules/stats + universe only). Java `Sector.removeItems` -> `addItems` -> `saveInfo()` (`J/type/Sector.java:207-231`). When converting it to `with_campaign_mut`, also call `sector.save_info(settings, &planet_name)` after the change (and the same for every other record mutator in the grep list).

**Symptom / acceptance.**
(a) The campaign turn counter and universe clock go backwards: each turn the
runtime writes `turn=N+1`, then `MindCampaign.persist()` writes the facade's
stale `turn=N` over it; after a restart the same turn number (and the same
`JavaRandom(turn+1)` invasion roll) repeats. (b) A sector you left keeps
`being_played == true` in memory, so it never produces offline (production
pass skips it) until the client restarts. (c) Changing campaign rules in-game
(`set_campaign_rules_json`) replaces the runtime's campaign with the stale
facade copy, losing the played sector's statistics. (d) After exiting a
sector, the planet dialog shows other sectors' stale items (turn production
stayed in the runtime copy, which was dropped).
Acceptance: tests below; in engine, two `run_turn()` calls advance the
persisted turn by 2 and `universe_json().turn` survives `run_exit_save`.

**Java.** One object graph: `Vars.universe`, `planet.sectors`, `state.rules.sector`
are the only copies. `Universe.save()` (`J/game/Universe.java:293-296`) writes
`utimei`/`turn` from that single object. `Sector.isBeingPlayed()`
(`J/type/Sector.java:148-151`):
```java
return Vars.state.isGame() && Vars.state.rules.sector == this && !Vars.state.gameOver && !net.client();
```

**Port today.**
* `gdext/campaign.rs`: field `campaign: Option<Campaign>` (facade). The live
  copy is `CampaignRuntime.campaign` (`core/game/runtime.rs` ~184), inside the
  sim world, reached with `host.bind_mut().take_campaign_runtime()` /
  `put_campaign_runtime(runtime)`.
* `pump_runtime` (~400): mirror `self.campaign = Some(runtime.campaign.clone())`
  only `if mirrored` (= any `PlayEvent`), then `if dirty { self.persist() }`.
* `persist` (~385): `campaign.save_all(&mut self.settings)` on the facade.
* `run_turn` #[func] (~1043), `set_campaign_rules_json` (~2870),
  `run_exit_save` (~2420), `clear_campaign_runtime` (`sim_host.rs` ~2070).
* Facade mutators: `grep -n "self.campaign.as_mut()\|self.campaign.take()" client/rust/mind-gdext/src/campaign.rs`
  (about 20 sites: research spending ~1181, remove_sector_items, abandon,
  clear saves, launch resources, loadout).

**Changes.**
1. Add to `impl MindCampaign` (Rust-only, no `#[func]`):
   ```rust
   /// Runs `f` on THE live campaign: the runtime's copy while a sector runs
   /// (then mirrors it into the facade), else the facade. Java has one
   /// object graph (`Vars.universe` / `planet.sectors`).
   fn with_campaign_mut<R>(&mut self, f: impl FnOnce(&mut Campaign, &mut SettingsStore) -> R) -> Option<R> {
       if let Some(mut host) = self.host() {
           let taken = host.bind_mut().take_campaign_runtime();
           if let Some(mut runtime) = taken {
               let out = f(&mut runtime.campaign, &mut self.settings);
               self.campaign = Some(runtime.campaign.clone());
               host.bind_mut().put_campaign_runtime(runtime);
               return Some(out);
           }
       }
       let mut campaign = self.campaign.take()?;
       let out = f(&mut campaign, &mut self.settings);
       self.campaign = Some(campaign);
       Some(out)
   }

   /// Copies the runtime campaign into the facade (no-op without a runtime).
   fn mirror_runtime_campaign(&mut self) {
       let Some(mut host) = self.host() else { return };
       let taken = host.bind_mut().take_campaign_runtime();
       if let Some(runtime) = taken {
           self.campaign = Some(runtime.campaign.clone());
           host.bind_mut().put_campaign_runtime(runtime);
       }
   }
   ```
   Never hold `host.bind_mut()` across `f` (H-environment §3: re-entrant
   borrow). `f` must not call into Godot.
2. `persist()`: call `self.mirror_runtime_campaign()` first, then the existing
   body. (`pump_runtime` holds the runtime taken out while it runs: there,
   instead of `self.persist()`, set `self.campaign = Some(runtime.campaign.clone())`
   right before `host.bind_mut().put_campaign_runtime(runtime)` whenever
   `dirty` is true, and keep `self.persist()` after the put.)
3. `run_turn` #[func]: replace both branches by
   `self.with_campaign_mut(|campaign, settings| campaign.run_turn(&registry, settings, &ctx, &mut rand))`
   where `ctx`/`rand` are computed first (read the runtime's session for
   `ctx` in its own statement). Keep the return `report.turn > 0`.
4. `set_campaign_rules_json`: do the planet-rule write through
   `with_campaign_mut`; then, in a separate statement, update the runtime
   session rules (`runtime.session.rules = …`), never assigning
   `runtime.campaign = campaign_snapshot`.
5. Convert every other facade mutator found by the grep above that changes
   sector records or universe state to `with_campaign_mut` (research spending
   included: Java spends from the live sector core via `Sector.removeItems`;
   the played sector's spending already goes through the live core, keep
   that). Read-only uses of `self.campaign.as_ref()` stay.
6. `being_played`:
   * `start_sector_with_loadout`, before `play_sector`/`play_new_sector`:
     `for planet in campaign.planets.values_mut() { for s in &mut planet.sectors { s.being_played = false; } }`.
   * `run_exit_save` and the `GameOver` arm of `pump_runtime`: set the
     current sector's `being_played = false` in the runtime campaign (Java:
     `!state.gameOver`, and `state.isGame()` false after exit) before the
     mirror/persist.
   * Do NOT change `sector_capture` here (T4 owns that line).
7. `run_exit_save`: call `self.mirror_runtime_campaign()` BEFORE
   `host.bind_mut().clear_campaign_runtime()`.
8. New seams (playtest + H05):
   ```rust
   /// `{turn, seconds}` of the live universe (runtime copy when installed).
   #[func] pub fn universe_json(&self) -> GString
   /// The live record of one sector as JSON (`SectorInfo` serde, camelCase,
   /// plus `"beingPlayed"` and `"hasSave"`); `"{}"` when unknown.
   #[func] pub fn sector_record_json(&self, planet: GString, sector: i64) -> GString
   ```
   Both read the runtime copy when a runtime is installed (take/put, own
   statement), else the facade.

**Tests.** No gdext node harness exists, so keep the logic testable in
`mind-core`:
* `core/game/universe.rs`: `save_all_after_run_turn_keeps_turn`: campaign A,
  clone B = A, `A.run_turn(..)` twice, `A.save_all(&mut s)` -> `s.get_i32("turn",0) == 2`;
  then `B.save_all(&mut s)` writes 0 — document in the test comment that this
  is exactly the facade bug and that the facade now mirrors before saving.
* `core/game/universe.rs`: `left_sector_produces_on_next_turn`: sector with
  base, `being_played = false`, `info.production["copper"].mean = 1.0`,
  `storage_capacity = 4000`, items 0 -> after `run_turn` items copper `== 120`
  (`turnDuration/60 = 120` s, `Universe.java:154,213`). Same with
  `being_played = true` -> 0.

**Verify in-engine.** GZ resume, then:
```bash
$G --eval "return $MC.universe_json()"                       # note turn T0
$G --eval "return $MC.run_turn()" --eval "return $MC.run_turn()"
$G --eval "return $MC.universe_json()"                       # turn T0+2
$G --eval "return $MC.run_exit_save()"
$G --eval "return $MC.universe_json()"                       # still T0+2
$G --eval "return $MC.sector_record_json(\"serpulo\", 170)"   # "beingPlayed": false
```
Restart (`tools/session-play.sh N --play`), `universe_json()` still `T0+2`.

**Pitfalls.** `pump_runtime` already has the runtime taken out: do not call
`with_campaign_mut` from inside it (the take would return `None` and silently
use the facade). Do not mirror facade -> runtime anywhere.

**Commit.** `Gameplay H04 T1: one authoritative campaign while a sector runs (turn clock, being_played, rule edits)`

---

### T2 — Sector statistics: 1-second refresh, export/import counters, `produced()` (offline production)

> **Coordinator note (2026-10-14).** Your `UnlockRef` enum also gets a `Unit(UnitId)` variant (H01 T3 pushes unit unlocks into the same `CampaignRuntime.pending_unlocks`). If H01 T3 already created `UnlockRef`/`pending_unlocks` on `main`, reuse them; `take_unlocked_content` resolves unit names too.

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. GZ resume, `place_block(130,48,"mechanical-drill")` plus four `place_block_rot(130,50..53,"conveyor",1)` into the core: core copper 0 -> 10 (1800 ticks) -> 32 (5400 ticks, i.e. past one 60-s refresh). After `run_exit_save()` the record `serpulo-s-170-info` has `rawProduction.copper.mean 0.0`, `production.copper.mean 0.0` (`runs/20261014-s3-verify2/H04-evals.txt`). **Recipe fixed below:** the drill at (130,48) does not touch the core (core-shard covers 128..130,54..56); the conveyor line is required, a lone drill only fills itself (10 items) and never offloads.

**Symptom / acceptance.** Leave a sector with drills feeding the core, wait
turns: Java adds `production.mean * 120` items per turn to the record; the
port adds nothing (rawProduction 0). Exports/imports never decay. Items are
never unlocked by producing them.
Acceptance: tests below; in engine a mechanical drill on copper raises
`sector_record_json(...).rawProduction.copper.mean` above 0 within 120 ticks.

**Java.**
* `SectorInfo.java:23`: `private static final float refreshPeriod = 60;` ("in ticks"); `:265` `if(time.get(refreshPeriod))` (Arc `Interval.get` = `Time.time - last >= 60` ticks).
* `:123-130` `export.get(item, ExportStat::new).counter += amount;` / `imports…counter += amount`.
* `:297-309`:
  ```java
  map.each((item, stat) -> {
      if(!stat.loaded){ stat.means.fill(stat.mean); stat.loaded = true; }
      stat.means.add(Math.max(stat.counter, 0));
      stat.counter = 0;
      stat.mean = stat.means.rawMean();
  });
  ```
* `:354-365` `ExportStat { transient float counter; transient WindowedMean means = new WindowedMean(60); transient boolean loaded; float mean; }`.
* `BuildingComp.java:1006-1007` `offload(item){ produced(item, 1); …}`;
  `:1044-1050`:
  ```java
  public void produced(Item item, int amount){
      if(Vars.state.rules.sector != null && team == state.rules.defaultTeam){
          Vars.state.rules.sector.info.handleProduction(item, amount);
          if(!net.client()) item.unlock();
      }
  }
  ```
* `BuildingComp.java:896` (in `dumpLiquid`): `if(!net.client() && state.isCampaign() && team == state.rules.defaultTeam) liquid.unlock();`
* `UnlockableContent.unlock()` fires `UnlockEvent` -> `Control` (`J/core/Control.java:160-174`): `hudfrag.showUnlock`, `checkAutoUnlocks()`.

**Port today.**
* `core/game/sector.rs`: `REFRESH_PERIOD = 60.0` (seconds), `update(seconds_delta, …)`,
  `InfoWindows::update_stat_map` samples `stat.mean`, `handle_item_export/import` add to `mean`.
* `core/io/json/rules.rs` ~933 `pub struct ExportStat { pub mean: f32 }` (derives `Copy, Serialize, Deserialize, Default`).
* `core/world/behavior/mod.rs` ~378 `fn produced` no-op; `transfer::offload` ~392.
* Queue: `CampaignHookQueues { is_campaign, default_team, core_deltas, production_deltas }` (`core/game/runtime.rs` ~66), resource `CampaignCoreQueues`; miner pattern `entities/comp/unit/miner.rs` ~300.
* Liquid dump: `core/world/blocks/liquid/movement.rs` ~228 `pub fn dump_liquid(`.

**Changes.**
1. `sector.rs`: `pub const REFRESH_PERIOD: f32 = 1.0;` with doc
   "`SectorInfo.refreshPeriod` = 60 ticks = 1 s (`SectorInfo.java:23`)". Keep
   `update(seconds_delta, …)` (caller passes `1/60` per tick). Fix any test that
   relied on 60 s.
2. `rules.rs` `ExportStat`: add `#[serde(skip)] pub counter: f32` with doc
   "transient (`SectorInfo.ExportStat.counter`)".
3. `sector.rs`: `handle_item_export/import` -> `stat.counter += amount as f32`.
   `update_stat_map`: `window.add(stat.counter.max(0.0)); stat.counter = 0.0; stat.mean = window.raw_mean();`
   (keep the existing fill-once logic).
4. New free fn in `core/world/blocks/distribution/transfer.rs` (next to `offload`):
   ```rust
   /// `BuildingComp.produced(item, amount)` (BuildingComp.java:1044-1050):
   /// default-team production feeds `SectorInfo.handleProduction` and unlocks
   /// the item (campaign only; queued, folded by `campaign_tick_system`).
   pub fn record_produced(world: &World, e: Entity, item: ItemId, amount: i32) {
       let team = world.get::<crate::ecs::BuildingComp>(e).map(|b| b.team.0);
       if let (Some(team), Some(queues)) = (team, world.get_resource::<crate::game::runtime::CampaignCoreQueues>())
           && let Ok(mut state) = queues.0.lock()
           && state.is_campaign && team == state.default_team
       {
           state.production_deltas.push((item, amount));
           state.unlocks.push(crate::game::runtime::UnlockRef::Item(item));
       }
   }
   ```
   (Confirm the building team component with `grep -n "pub team" client/rust/mind-core/src/ecs.rs`.)
   Call it as the first statement of `transfer::offload`, and make the trait
   default `produced` call it.
5. `CampaignHookQueues`: add `pub unlocks: Vec<UnlockRef>` and
   `pub enum UnlockRef { Item(ItemId), Liquid(LiquidId) }`. In `dump_liquid`
   (movement.rs), when the source has more than `0.0001` of the liquid, push
   `UnlockRef::Liquid` under the same campaign/default-team gate (dedupe:
   skip the push if the last entry is equal, to keep the queue small).
6. `campaign_tick_system`: drain `unlocks` into a new
   `CampaignRuntime.pending_unlocks: Vec<UnlockRef>` (dedupe with a
   `BTreeSet` of `(kind, id)` before pushing).
7. gdext `pump_runtime`: take `runtime.pending_unlocks`; for each, resolve the
   name (`registry.item(id).name` / `registry.liquid(id).name`), then
   `tech_tree::unlock_fields_mut(registry, ContentRef::of(ContentType::Item|Liquid, id))`
   and `fields.unlock(&name, &mut SettingsUnlockStore::new(&mut self.settings))`
   (`core/content/ctype.rs` ~206 returns `true` when newly unlocked). For
   each newly unlocked name push it to `self.unlocked_since_last_take: Vec<String>`
   and, if any, run `tech_tree::check_auto_unlocks` exactly like
   `apply_capture_bookkeeping` (~598) and refresh `runtime.set_unlocked_content(...)`.
   New seam for H05's `showUnlock` popup:
   `#[func] pub fn take_unlocked_content(&mut self) -> PackedStringArray`.

**Tests.**
* `sector.rs` `export_counter_decays`: `handle_item_export("copper", 100)`,
  `update(1.0, …)` -> `export["copper"].mean == 100.0/60.0` (fill 0 then one
  sample, `rawMean` over 60); next `update(1.0, …)` without exports ->
  `mean == 100.0/60.0` still (window holds the sample), counter `== 0`.
* `sector.rs` `refresh_every_60_ticks`: 59 calls `update(1.0/60.0)` -> no
  refresh (production map empty); the 60th -> refreshed (`production` has
  every item).
* `production_tests.rs` (`runtime_sim()`/`place()` helpers):
  `drill_offload_records_production`: install campaign hooks
  (`install_campaign_core_hooks`), set the queue `is_campaign = true`,
  `default_team = <drill team>`, place `mechanical-drill` on copper ore
  touching a core, tick until one item is offloaded -> the queue's
  `production_deltas` contains `(copper, 1)` and `unlocks` contains
  `Item(copper)`.

**Verify in-engine.** GZ resume:
```bash
$G --eval "return $H.place_block(130, 48, \"mechanical-drill\")"   # ore (130..131,48..49)
$G --eval "return [$H.place_block_rot(130,50,\"conveyor\",1), $H.place_block_rot(130,51,\"conveyor\",1), $H.place_block_rot(130,52,\"conveyor\",1), $H.place_block_rot(130,53,\"conveyor\",1)]"   # into the core (128..130,54..56)
$G --eval "$H.set_paused(true); $H.step(1200); return $H.core_items_json()"   # copper > 0 (verified 2026-10-14: 10 after 1800 ticks)
$G --eval "return JSON.parse_string($MC.sector_record_json(\"serpulo\", 170)).rawProduction.copper"
```
Expect `mean > 0`. (Corrected 2026-10-14: the drill at (130,48) does NOT touch
the core; without the conveyor line it fills itself to 10 and stops.) 0 panics.

**Pitfalls.** `record_produced` runs inside building updates: only lock the
queue, never touch `CampaignRuntime` there. Golden UI JSON may include
`ExportStat` — `#[serde(skip)]` keeps the serialized shape.

**Commit.** `Gameplay H04 T2: sector statistics refresh per second, export counters, produced() feeds production and unlocks`

---

### T3 — Resume/launch bookkeeping: `SectorInfo.write`, `prepare` gaps, lost-sector conditions, `quietUnlock`, `setLastSector`, launch origin

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. The plan's recipe could not show the bug: the GZ save already holds the same items as the record, so "core == record" passes today. Discriminating recipe (used here): exit GZ (`run_exit_save`), patch the stored record with `python3 -I REPO/runs/20261014-s3-verify2/tools/settings_patch_record.py $U/config/settings.bin serpulo-s-170-info '{"items":{"copper":777,"lead":55}}'`, then `$MC.reload_from_disk()` and the Go recipe: core = `{"copper":40}` (the save's items), record `{copper:777, lead:55}`; Java `SectorInfo.write` would give 777/55. `lastPresetName` is `null` in the stored record after an exit save (symptom c). `set_last_sector`/sector `quiet_unlock` have no live caller (grep). Verify step rewritten below.

**Symptom / acceptance.** (a) Resuming a sector ignores the record's items
(offline production from T2/T1 never reaches the core). (b) No `winWave`
default (30) / attack detection on resume. (c) `lastPresetName` never stored,
so the lost-sector replay can never match Java's `sectorDataMatches`.
(d) Landing on a preset never `quietUnlock`s it; the planet's last sector
never updates. (e) A launched sector has no `origin`/`destination`.
Acceptance: tests below; in engine the GZ core copper equals the record after
resume.

**Java.**
* `SectorInfo.write()` — paste of `J/game/SectorInfo.java:165-196`:
  ```java
  if(state.rules.waveTeam.core() != null){ attack = true; if(!state.rules.sector.planet.allowWaves) winWave = 0; }
  if(winWave <= 0 && !attack && state.rules.sector.planet.allowWaves) winWave = 30;
  if(state.rules.sector != null && state.rules.sector.preset != null && state.rules.sector.preset.captureWave > 0 && !state.rules.sector.planet.allowWaves) winWave = state.rules.sector.preset.captureWave;
  state.wave = wave; state.rules.waves = waves; state.rules.waveSpacing = waveSpacing;
  state.rules.winWave = winWave; state.rules.attackMode = attack;
  CoreBuild entity = state.rules.defaultTeam.core();
  if(entity != null){ entity.items.clear(); entity.items.add(items);
      entity.items.each((i, a) -> entity.items.set(i, Mathf.clamp(a, 0, entity.storageCapacity))); }
  ```
* `Logic` `SaveLoadEvent` (`J/core/Logic.java:70-88`), campaign only, `!e.isMap`:
  `info.write(); planet.applyRules(state.rules); info.hasCore = true; sector.saveInfo();`
* `SectorInfo.prepare` additions (`:226-228`, `:244-249`):
  `lastPresetName = sector.preset == null ? null : sector.preset.name;`
  `var pads = indexer.getFlagged(defaultTeam, BlockFlag.launchPad); if(pads.size == 0 || !pads.contains(t -> t.efficiency > 0)) export.clear();`
* `sectorDataMatches` (`:98-103`): preset map size equals `lastWidth/lastHeight` and `preset.name == lastPresetName`.
* `Control.playSector` (`J/core/Control.java:437-470`): `if(sector.preset != null) sector.preset.quietUnlock();`
  `sector.planet.setLastSector(sector);`
  `boolean clearSave = planet.clearSectorOnLose || planet.campaignRules.clearSectorOnLose;`
  `if(slot != null && !clearSectors && (!clearSave || sector.info.hasCore))` -> load; then
  `if(defaultTeam.cores().isEmpty() || hadNoCore)` -> `if(clearSave || spawnPosition == 0 || !sectorDataMatches) playNewSector else replay`.
* `Control.playNewSector` (`:573-578`): `sector.info.origin = origin; sector.info.destination = origin; attempts++`.

**Port today.**
* `core/game/sector.rs`: `WriteTarget` (~124) + `SectorInfoState::write` (~454, wrong direction, no caller), `prepare` (~419), `sector_data_matches` (~693).
* `core/game/play.rs` `play_sector` (~432) copies the record into the session; `play_new_sector` (~378) sets origin/destination from its `origin` arg.
* `gdext/campaign.rs` `start_sector_with_loadout` (~630): `lost_replay` = `has_save && spawn_position != 0 && player_core_blocks == 0`; always passes `None` origin; `parse_launch_payload` (~3650) reads `schematic`/`stacks` from the loadout JSON.
* `persist_sector_record` (~2458) calls `prepare(&snapshot, true)`; `CoreSnapshot` (`sector.rs` ~34) built by `core_snapshot_of` (~58).
* `core/game/planet.rs` ~371 `set_last_sector(&mut self, settings, sector)` (no caller).
* `core/content/ctype.rs` ~217 `quiet_unlock(&mut self, name, store)`; `tech_tree::unlock_fields_mut` (`core/game/tech_tree.rs` ~541).
* UI: `ui/dialogs/planet_dialog.gd` `_launch` (~1610) builds the loadout JSON with `launcher_id`.

**Changes.**
1. `sector.rs`: delete `WriteTarget` and `SectorInfoState::write`; add
   ```rust
   /// `SectorInfo.write()` (SectorInfo.java:165-196): record -> live session.
   /// Returns the item stacks to put into the default core (clamped by the caller).
   pub fn write_to_state(&mut self, wave_team_has_core: bool, planet_allow_waves: bool,
                         preset_capture_wave: i32, session: &mut crate::game::play::PlaySession) -> IndexMap<String, i32>
   ```
   Body: the Java `attack/winWave` adjustments on `self.info`, then
   `session.wave = info.wave; rules.waves = info.waves; rules.wave_spacing = info.wave_spacing; rules.win_wave = info.win_wave; rules.attack_mode = info.attack;`,
   return `self.info.items.clone()`. Update the test
   `prepare_and_write_populate_fields` (~823) to the new direction.
2. `CoreSnapshot`: add `pub active_launch_pads: usize` and
   `pub any_launch_pads: bool`; in `core_snapshot_of` count default-team
   buildings whose block def has `BlockFlag::LaunchPad` in `def.flags`
   (`any`) and of those the ones with `Building.efficiency > 0.0`
   (`crate::entities::comp::Building`; get it per entity).
   `prepare(&mut self, core, min_production, preset_name: Option<&str>)`:
   `self.info.last_preset_name = preset_name.map(str::to_owned);`
   and `if core.active_launch_pads == 0 { self.info.export.clear(); }`.
   Update callers (`persist_sector_record` passes the sector preset name from
   `registry.sector(preset).name`).
3. `MindSimHost` Rust-only: `pub fn write_record_items_to_core(&mut self, team: u8, items: &IndexMap<String, i32>) -> bool`:
   first default-team core in `entities_by_seq()` order; `ItemModule::clear()`;
   for each `(name, amount)` with a known item id `module.set(item, amount.clamp(0, core.storage_capacity))`
   (`CoreBuild.storage_capacity`; check the `ItemModule` setter name with
   `grep -n "pub fn set" client/rust/mind-core/src/world/modules/item*.rs`).
   Also mirror into the session team inventory if `apply_launch_loadout`
   does so (`session.teams.inventory(...)`) — read `sync_session_with_sim`
   to see which store the HUD reads and keep both equal.
4. `start_sector_with_loadout`, resume branch only (`has_save && !lost_replay`),
   AFTER `sync_campaign_session` (so the session counts wave cores):
   * take the runtime; `let wave_core = runtime.session.wave_core_count() > 0;`
   * `let items = sector.info.write_to_state(wave_core, planet.allow_waves, preset_capture_wave, &mut runtime.session);`
     (on the runtime campaign's sector record, so T1's single copy holds),
   * `planet.apply_rules(&registry, &mut runtime.session.rules, false, true)` (Java `planet.applyRules` after `write`),
   * `sector.info.info.has_core = true;` then `sector.save_info(&mut self.settings, &planet_name)`,
   * put the runtime back; then (own statement) `host.bind_mut().write_record_items_to_core(default_team, &items)`;
   * copy `runtime.session` into `self.session` as the existing code does.
   Remove the now-duplicate record copy in `play_sector` (`play.rs` ~470-480)
   ONLY if `start_sector_with_loadout` is its sole live caller
   (`grep -rn "play_sector(" client/rust`); otherwise keep it and let
   `write_to_state` overwrite.
5. Lost-sector condition (Java 456-468): compute
   `clear_save = planet.clear_sector_on_lose || planet.campaign_rules.clear_sector_on_lose`
   (find the planet field with `grep -n "clear_sector_on_lose" client/rust/mind-core/src/game/planet.rs`),
   `had_no_core = !record.info.info.has_core`. Replay iff
   `has_save && (!clear_save || !had_no_core) && (no live core || had_no_core) && !clear_save && spawn_position != 0 && record.sector_data_matches(preset_name, preset_w, preset_h)`;
   when the no-core branch is taken but the replay conditions fail, take the
   fresh `play_new_sector` path (`has_save` treated false). Preset map size:
   the loaded preset grid size (`load_preset_map_grid`, `sim_host.rs`); if
   it is not cheaply available, use `last_width/last_height` from the record
   and note the deviation.
6. `quietUnlock` + `setLastSector`: after the lock check, with `preset_id = Some(p)`:
   ```rust
   if let Some(fields) = tech_tree::unlock_fields_mut(&mut registry, ContentRef::of(ContentType::Sector, p)) {
       fields.quiet_unlock(&preset_name, &mut SettingsUnlockStore::new(&mut self.settings));
   }
   ```
   (make `unlock_fields_mut` `pub` if needed) and
   `campaign.planet_mut(planet_id).map(|p| p.set_last_sector(&mut self.settings, sector_id));`
7. Origin: `parse_launch_payload` also reads `"origin": <sector id>` (absent
   -> `None`). Pass `origin.map(|s| (planet_id, s))` to `play_new_sector`
   and `play_sector` (the type is `SectorRef`; check it). In
   `ui/dialogs/planet_dialog.gd` `_launch`, add `"origin": launcher_id,` to
   the `JSON.stringify({...})` dictionary (one line; tell H05 in the summary).

**Tests.**
* `sector.rs` `write_to_state_applies_record_and_win_wave`: record
  `{wave: 7, waves: true, win_wave: 0, attack: false, items: {copper: 5000}}`,
  allow waves, no wave core -> session `wave == 7`, `win_wave == 30`,
  `attack_mode == false`, returned items `{copper: 5000}`; with a wave core ->
  `attack_mode == true` and `win_wave` stays 30 (allowWaves true).
* `sector.rs` `prepare_sets_last_preset_and_clears_export_without_pads`:
  snapshot `active_launch_pads: 0`, export copper mean 3 -> `export` empty,
  `last_preset_name == Some("groundZero")`.
* `play.rs` or `universe.rs` `launch_sets_origin_and_destination`:
  `play_new_sector(…, Some((planet, 170)), …)` -> record `origin`/`destination == "serpulo-170"` (`SectorKey::format`).

**Verify in-engine.** (Rewritten 2026-10-14: the old recipe added copper to the
core and exit-saved, so the save and the record held the same items and the
check passed before the fix.) Make the record differ from the save, then resume:
```bash
$G --eval "return $MC.run_exit_save()"
T=REPO/runs/20261014-s3-verify2/tools
python3 -I $T/settings_patch_record.py $U/config/settings.bin serpulo-s-170-info '{"items":{"copper":777,"lead":55}}'
$G --eval "return $MC.reload_from_disk()"
# re-enter (Go recipe), then
$G --eval "return $H.core_items_json()"            # {"copper":777,"lead":55} (before the fix: the save's items, e.g. {"copper":40})
$G --eval "return JSON.parse_string($MC.sector_record_json(\"serpulo\", 170)).lastPresetName"   # "groundZero" (before: null)
```
(After T1+T2 an alternative is `run_turn()` with production while away.)
0 panics.

**Pitfalls.** `write_record_items_to_core` must run after the core entity
exists and after `sync_campaign_session`, or `apply_launch_loadout` may clear
it again. The resume branch must NOT run `logic_play` (Java resume never
calls `Logic.play`).

**Commit.** `Gameplay H04 T3: SectorInfo.write on resume, prepare lastPresetName/export clear, lost-sector conditions, quietUnlock, launch origin`

---

### T4 — Turns and invasions: played-sector invasion, Java rule defaults, spawn count, toast, capture keeps being_played

> **Coordinator override (2026-10-14).** The invasion TOAST and the `sector_invaded` signal belong to H05 T7. In Change 4 do only the rules half (`waves`/`wavesSpawnAtCores`/win wave + `rules_world_dirty`); skip the `push_toast` call and the signal. Keep emitting the `CampaignEvent::SectorInvasion` event H05 T7 reads. If you can emit `CampaignEvent::SectorLose { planet, sector }` / `SectorCapture { planet, sector }` for off-play sectors cheaply in `universe.rs`, do it (H05 T7 consumes them); otherwise list it as a seam in your summary.

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (a). After a clean import the settings hold `serpulo-campaign-rules = {..."sectorInvasion":true,..."legacyLaunchPads":true...}` and the imported Java profile (`mindustry.save.full.tech.tree.zip` `settings.bin`) has no `*-campaign-rules` key, so these are the port's `from_def` defaults (Java: both false; `J/game/CampaignRules.java:7-16`, `J/content/Planets.java:84-86,136-137` set only `allow*` on Serpulo). (b)-(e) confirmed by reading (`invasion_pass` ignores `_ctx`; `runtime.rs:333` `map_spawns.max(wave_cores)`; `play.rs:696`). **Fact fixed:** `get_campaign_rules_json()` takes NO argument (passing one raised a SCRIPT ERROR and wedged the game; `--play` recovered) and reads the current session's planet, so call it inside a sector, or read `serpulo-campaign-rules` from settings with `REPO/runs/20261014-s3-verify2/tools/settings_read.py`. Extra: `from_def` also copies the planet-level `clear_sector_on_lose` into the campaign rules (Erekir stored `"clearSectorOnLose":true`); Java keeps `campaignRules.clearSectorOnLose` false and reads `planet.clearSectorOnLose || campaignRules.clearSectorOnLose` (`J/core/Control.java:451`). If Change 1 also sets it false, `apply_sector_loss` (`play.rs` ~766, reads only `campaign_rules.clear_sector_on_lose`) must OR in the planet field.

**Symptom / acceptance.** (a) Fresh profiles have sector invasions and legacy
launch pads ON (Java: off). (b) An invasion of the sector you are playing
writes the stored record instead of turning waves back on live. (c) Invaded
records are not saved; no "Sector X under attack!" toast. (d) Capturing sets
`being_played = false` while you are still in the sector. (e) `hasSpawns`
counts enemy cores.
Acceptance: tests below.

**Java.** `Universe.java:231-258` (paste):
```java
if(sector.planet.campaignRules.sectorInvasion && sector.info.minutesCaptured > invasionGracePeriod && sector.info.hasSpawns){
    int count = sector.near().count(s -> s.hasEnemyBase() && !s.hasBase() && (s.preset == null || !s.preset.requireUnlock));
    if(count > 0 && Mathf.chance(baseInvasionChance * (0.8f + (count - 1) * 0.3f))){
        int waveMax = Math.max(sector.info.winWave, sector.isBeingPlayed() ? state.wave : sector.info.wave) + Mathf.random(2, 4) * 5;
        if(sector.isBeingPlayed()){
            state.rules.winWave = waveMax; state.rules.waves = true; state.rules.attackMode = false;
            planet.campaignRules.apply(planet, state.rules);
        }else{
            sector.info.winWave = waveMax; sector.info.waves = true; sector.info.attack = false; sector.saveInfo();
        }
        Events.fire(new SectorInvasionEvent(sector));
    }
}
```
This block sits inside `if(!sector.isAttacked())` of the per-planet third
pass (`:193-260`), and planets outside `current.updateGroup` are skipped
(`:160-163`). Toast: `HudFragment.java:273-275`
`showToast(Icon.warning, Core.bundle.format("sector.attacked", e.sector.name()))`
(bundle `sector.attacked = Sector [accent]{0}[white] under attack!`).
Defaults: `J/type/Planet.java:119,121,181,308-309,448`; `J/content/Planets.java:84-86`
(erekir defaults only). `countSpawns` = `spawns.size` (`J/ai/WaveSpawner.java:49-51`).
`sectorCapture` (`Logic.java:394-425`) does not touch `isBeingPlayed`.

**Port today.** `Campaign::invasion_pass(&mut self, registry, _ctx, seconds_passed, rand, events)`
(`universe.rs` ~715) iterates `self.planets.values()` (an `IndexMap`, deterministic);
`CampaignEvent::SectorInvasion { planet, sector, win_wave }` (~283);
`pump_runtime` ignores `report.events`. `planet.rs` ~193-194 defaults.
`runtime.rs` ~333 spawn count. `play.rs` ~696 `sector.being_played = false`.

**Changes.**
1. `planet.rs` `from_def`: `sector_invasion: false, legacy_launch_pads: false`
   (keep `fog/hide_spawns/rts_ai` from `campaign_rule_defaults`; stored
   settings still override via `load_rules`). If the UI campaign golden
   changes, regenerate per H-environment §2 with reason
   "Java Planet.applyDefaultRules".
2. `CampaignEvent::SectorInvasion` gains `played: bool`.
3. `invasion_pass(&mut self, registry, ctx, …)`: skip planets where
   `ctx.current_planet.is_some_and(|c| c != planet.id)`; for each candidate
   compute `played = ctx.current_sector == Some((planet.id, sector.id))`;
   the attacked check uses `sector.is_attacked(if played { ctx.playing_rules } else { None })`;
   `base = info.win_wave.max(if played { ctx.current_wave } else { info.wave })`.
   For `!played` write the record as today AND push the sector into a
   `to_save` list; for `played` write nothing. After the loop
   `sector.save_info(settings, &planet_name)` for `to_save` (add `settings:
   &mut SettingsStore` to the signature; `run_turn` has it). Keep the RNG
   draw order: one `next_double()` per eligible sector, then
   `next_int_bound(3)` only on success.
4. `pump_runtime` (and T1's `run_turn` #[func]): for each
   `SectorInvasion { played: true, win_wave, .. }`:
   `runtime.session.rules.win_wave = win_wave; waves = true; attack_mode = false;`
   then `planet.campaign_rules.apply_planet(planet, &mut runtime.session.rules, true)`
   (`core/game/planet.rs` calls it from `apply_rules`; check the name), and
   `runtime.rules_world_dirty = true`. For every invasion event (played or
   not): text = `self.bundle.format("sector.attacked", &[sector_display_name])`
   (use the same name helper the planet views use; grep `fn sector_name` in
   `gdext/campaign.rs`), then
   `if let Some(mut hud) = self.base().get_node_or_null("/root/MindHud") { hud.call("push_toast", &[text.to_variant(), "warning".to_variant()]); }`
   and emit a new signal `#[signal] fn sector_invaded(planet: GString, sector: i64);`.
5. `runtime.rs` `sync_session_with_sim`: `sync.spawn_count = map_spawns;`
   (Java `countSpawns`). Fix the test asserting `spawn_count == 1` (~2198) by
   giving it a spawn tile, or change the expectation with the Java reason.
6. `play.rs` `sector_capture`: delete `sector.being_played = false;` (Java
   keeps playing the captured sector; T1 clears it on exit). Update
   `sector_capture_flags` if it asserts it.

**Tests.**
* `planet.rs` `serpulo_campaign_rule_defaults_match_java`: fresh campaign ->
  `!sector_invasion && !legacy_launch_pads && !fog`.
* `universe.rs` `invasion_of_played_sector_targets_live_rules`: serpulo
  campaign, `campaign_rules.sector_invasion = true`, sector S with base,
  `minutes_captured = 30.0`, `has_spawns = true`, one neighbour with an enemy
  base and no player base; `ctx.current_sector = Some(S)`,
  `current_wave = 12`, `playing_rules = Some((false,false))`. Find a seed by
  scanning `JavaRandom::new(seed)` for `seed in 0..10_000` whose first
  `next_double() < 0.01*0.8` (compute in the test). Expect one
  `SectorInvasion { played: true, win_wave: w }` with
  `w == max(info.win_wave, 12) + (2 + r)*5` (`r` = that RNG's next
  `next_int_bound(3)`), and `info` unchanged.
* Same setup with `current_sector = None` -> `played: false`, record
  `win_wave == w`, `waves == true`, and the settings key
  `serpulo-s-<id>-info` contains `"waves":true`.

**Verify in-engine.** Headless only for the roll (1% chance per turn). In
engine: after a clean re-import check what the profile stores (inside a
sector — `get_campaign_rules_json()` takes no argument and reads the current
session's planet): `$G --eval "return $MC.get_campaign_rules_json()"`, or
outside a sector `python3 -I REPO/runs/20261014-s3-verify2/tools/settings_read.py $U/config/settings.bin '^serpulo-campaign-rules$'`.
Before the fix: `sectorInvasion true, legacyLaunchPads true` (verified
2026-10-14; the imported Java profile stores no campaign rules, so these are
port defaults). After: both false. 0 panics.

**Commit.** `Gameplay H04 T4: played-sector invasions hit live rules, Java campaign-rule defaults, countSpawns, invasion toast`

---

### T5 — Match and planet statistics (`Logic` event listeners)

> **Coordinator override (2026-10-14): WITHDRAWN — do not execute.** H09 T10 implements the `Logic` stat listeners (it adds `team` to `BlockBrokenEvent` and counts in `flush_events`). The `cores_destroyed` hint value is H05 T12 Change 2. Skip this task; write `skipped: moved to H09 T10` in your summary.

> **In-engine verification (2026-10-14, session 3):** withdrawn marker confirmed present; not executed or probed.

**Symptom / acceptance.** The Game Over dialog shows "Units created 0,
Buildings built 0, destroyed 0, deconstructed 0" no matter what happened;
planet statistics stay empty; the attack-map hint predicate `coresDestroyed`
is always 0. Acceptance: tests + in-engine `game_stats_json()` non-zero.

**Java** (`J/core/Logic.java`):
* `:187-200` `BlockBuildEndEvent`: if `e.team == defaultTeam || e.unit != null && e.unit.team == defaultTeam`:
  `breaking ? stats.buildingsDeconstructed++ : stats.buildingsBuilt++`; campaign:
  planet `stats().buildingsDeconstructed/buildingsBuilt.increment(block)`.
* `:202-216` `BlockDestroyEvent`: default team -> `stats.buildingsDestroyed++` (+ planet `buildingsDestroyed.increment(block)`); else `stats.destroyedBlockCount.increment(block)` (+ planet `enemyBuildingsDestroyed.increment(block)`).
* `:218-226` `UnitDestroyEvent`: non-default team -> `stats.enemyUnitsDestroyed++`; campaign: `(enemy ? enemyUnitsDestroyed : unitsDestroyed).increment(type)`.
* `:228-236` `UnitCreateEvent`: default team -> `stats.unitsCreated++`; campaign: `unitsProduced.increment(type)`.
* `:244-248` `GameOverEvent` -> planet `sectorsLost++` (port already does in `apply_sector_loss`).
* Hint (`J/ui/fragments/HintsFragment.java:285`): `stats.getDestroyed(coreShard)+getDestroyed(coreFoundation)+getDestroyed(coreNucleus) > 0`.

**Port today.** `GameStats` (`core/io/json/rules.rs` ~839) and
`CampaignStats` helpers (`core/game/stats.rs`: `add_building_built`,
`add_building_destroyed`, `add_enemy_unit_destroyed`, …; `GameStats::add_destroyed`).
Increment sites: `core/sim/mod.rs` ~1789 (`SimEvent::BlockPlacedEvent` ->
`add_placed`), `unit_runtime.rs` ~539 (enemy kill). Building death:
`unit_runtime.rs` ~600-700 (the `BlockDestroyEvent` comment block, has
`team`, `plan` with block id). Deconstruct/build completion: `core/sim/builder.rs`
(~865, ~1944 `BlockBuildEndEvent` comments). Unit creation:
`runtime.units_created += …` in `unit_runtime.rs` (~223, 242, 553, 2535, 2930, 3466).
`game_stats_json` (`gdext/campaign.rs` ~3429) reads the facade session
(mirrored on events only).

**Changes.**
1. Add `impl GameStats`/`CampaignStats` nothing new; add one Rust helper in
   `core/game/runtime.rs`:
   ```rust
   /// `Logic` stat listeners (Logic.java:187-236). `block`/`unit` are content names.
   pub enum StatEvent<'a> { Built { team: u8, block: &'a str, breaking: bool },
                            BlockDestroyed { team: u8, block: &'a str },
                            UnitDestroyed { team: u8, unit: &'a str },
                            UnitCreated { team: u8, unit: &'a str } }
   pub fn record_stat(runtime: &mut CampaignRuntime, ev: StatEvent<'_>)
   ```
   implementing the Java branches against `runtime.session.stats` and, when
   `runtime.session.is_campaign()`, `runtime.campaign.stats_for_mut(planet)`.
2. Call it: build/deconstruct completion in `builder.rs` (the team of the
   builder unit or plan; both places marked `BlockBuildEndEvent`); building
   death in `unit_runtime.rs` next to the `coreDestroyClear` block (use the
   dying building's block name); unit death next to the existing enemy-kill
   increment (replace that increment with `record_stat` so it is not counted
   twice); unit creation at each `units_created += …` site that represents a
   real `UnitCreateEvent` (factory/reconstructor output, wave spawns do NOT
   fire it in Java: `UnitCreateEvent` is fired by `UnitFactory`, `Reconstructor`,
   `UnitAssembler`, core spawn — check each site against
   `grep -rn "new UnitCreateEvent" J/` and only call where Java fires).
   Fetch names via the `ContentIndex` resource / `runtime.content`.
3. `game_stats_json`: read the runtime session stats (take/put) when
   installed.
4. `gdext/sim_host.rs` ~5284 `cores_destroyed`: sum
   `stats.get_destroyed("core-shard") + "core-foundation" + "core-nucleus"`
   from the runtime session.

**Tests.** `core/sim/combat_tests.rs` or `production_tests.rs`:
`building_destroyed_counts_by_team` (kill a team-2 `copper-wall` ->
`destroyed_block_count["copper-wall"] == 1`; kill a team-1 wall ->
`buildings_destroyed == 1`); `deconstruct_counts` via the builder path
(`sim_with_player()` helper); `planet_stats_follow_campaign`.

**Verify in-engine.** GZ resume, `place_block` a team-2 `copper-wall` via
`apply_sim_command_json` place with `"team":2` (check the op supports team;
else `debug_kill_building` on a derelict `scrap-wall` near (169,167) counts as
non-default), then `debug_kill_building(x,y)` ->
`return $MC.game_stats_json()` shows the counters. 0 panics.

**Commit.** `Gameplay H04 T5: Logic stat listeners fill GameStats and planet CampaignStats`

---

### T6 — PAR-0198 (1/2): launch pads launch items (v8 export path)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. The recipe works as written: tiles (120..124,60..64) are snow/ice, `place_block` of `advanced-launch-pad` (121,61), `power-source` (124,61), `liquid-source` (121,64) all `true`; `configure_building(121,64,"content","oil")` reaches the pad (`liquid_debug` current 40, efficiency 1.0). With 100 copper the pad still holds `{"copper":100}` after 2120 ticks and no payload exists (`runs/20261014-s3-verify2/H04-T6-pad-full.png`). `set_launch_destination`/`launch_payloads_json` do not exist yet (they are this task's seams).

**Symptom / acceptance.** A powered launch pad full of items never launches.
After: at `launchCounter >= launchTime` with a full pad the items leave in a
`LaunchPayload` that lives 120 ticks; on removal the sector export counter
grows (and with `legacyLaunchPads` the destination record gets the items).
Acceptance: tests + in-engine pad empties and `export.copper.mean > 0`.

**Java** (`J/world/blocks/campaign/LaunchPad.java`):
* Content (`J/content/Blocks.java:6840-6860`): `launch-pad` size 3,
  `itemCapacity 100`, `launchTime = 60*20 = 1200`, `acceptMultipleItems = true`,
  power 4; `advanced-launch-pad` size 4, `itemCapacity 100`,
  `launchTime = 60*30 = 1800`, `liquidCapacity 40`, oil `9/60`/tick, power 8,
  `acceptMultipleItems = false`.
* `:92-95` `shouldConsume(){ return launchCounter < launchTime; }`
* `:138-140` `acceptItem`: `items.total() < itemCapacity && (acceptMultipleItems || items.total() == 0 || items.first() == item)`.
* `:143-161` `updateTile`:
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
* `:206-222` IO: `version() 1`; write `f launchCounter`; read `if(revision >= 1) launchCounter = read.f()`.
* `:290-311` `LaunchPayloadComp.remove`: campaign only;
  `destsec = state.rules.sector.info.destination;`
  `if(team == defaultTeam && destsec != null && destsec != state.rules.sector)`: for each stack
  `info.handleItemExport(stack)` + `LaunchItemEvent`; `if(planet.campaignRules.legacyLaunchPads) destsec.addItems(dest)`.
* Draw needs (H03): `x, y, fin = time/lifetime, id` (`randomSeedRange(id(), …)`), the pad's `podRegion` (`"<block>-pod"`, fallback `launchpod`).
* `:185-203` destination selection: `ui.planet.showSelect(sector, other -> { prev = info.destination; info.destination = other; if(prev != null) prev.info.refreshImportRates(planet); })` — UI is H05; this task adds the setter seam.

**Port today.** `LaunchPadBehavior` (`core/world/behavior/campaign.rs` ~117);
`CampaignState { heat, cooldown, launch_time, item }` (`core/entities/comp/building.rs` ~246);
`LaunchPadDef { launch_time }` (`core/world/block_kind_data.rs` ~343) built
with `default()` (~628); specs in `core/content/registries/blocks/campaign.rs`
(~20-67: no launch time, no accept flag). Trait hooks exist:
`should_consume`, `accept_item`, `version`, `write`, `read`
(`core/world/behavior/mod.rs`); consume trigger
`crate::world::update::trigger_consumers(world, e)` (`core/world/update.rs` ~654).

**Changes.**
1. `LaunchPadDef`: add `pub accept_multiple_items: bool`. In
   `BlockKindData::from_def` (~628) fill from the block name:
   `"launch-pad" => (1200.0, true)`, `"advanced-launch-pad" => (1800.0, false)`,
   cite `Blocks.java:6844,6846,6854`. If `parity/golden_content.json`
   changes, regenerate per H-environment §2.
2. `LaunchPadBehavior`:
   * `should_consume`: `state.launch_time < def.launch_time`.
   * `accept_item`: Java formula (`ItemModule::total()`, `first()`; add a
     `first()` helper on `ItemModule` if absent = lowest item id with amount > 0,
     matching Arc `ItemModule.first()`).
   * `update_tile`: `state.launch_time += edelta`; if
     `>= def.launch_time && items.total() >= item_capacity`:
     `trigger_consumers(world, e)`; collect `(ItemId, amount)` in item-id order;
     clear the module; spawn
     `(Pos { x, y } /* pad centre */, TeamComp { team }, LaunchPayloadState { id, stacks, time: 0.0, lifetime: 120.0, block })`;
     `launch_time = 0`. `id` from a new resource `LaunchPayloadSeq(u32)`
     (insert lazily, `wrapping_add(1)`).
   * IO: `version = 1`; write `f launch_time`; read `if revision >= 1`.
   * Remove the old `edelta`-only accumulation gated on efficiency (`edelta`
     already includes efficiency).
3. New file `core/game/launch_payload.rs` (header per env §3):
   `#[derive(Component)] pub struct LaunchPayloadState { pub id: u32, pub stacks: Vec<(ItemId, i32)>, pub time: f32, pub lifetime: f32, pub block: BlockId }`
   and `pub fn launch_payload_system(world: &mut World)`, registered in the
   campaign schedule set right after `campaign_tick_system`
   (`core/sim/schedule.rs`; do not reorder existing sets). Each tick:
   collect payload entities sorted by `id`; `time += 1.0` (Java `Timedc` uses
   `Time.delta`); when `time >= lifetime`: if a `CampaignRuntime` exists,
   `session.is_campaign()`, payload team == `rules.default_team`, the played
   sector's `info.destination` resolves (`SectorKey::resolve(registry)`) to a
   different sector: `sector.info.handle_item_export(name, amount)` per stack;
   if the planet's `campaign_rules.legacy_launch_pads`: add the stacks to the
   destination sector record (`Sector::add_items`) and push its key into a
   new `CampaignRuntime.pending_info_saves: Vec<(PlanetId, u16)>` that
   `pump_runtime` flushes with `save_info`. Despawn the payload in all cases.
   Payloads outside a campaign just expire.
4. Seams (`gdext/sim_host.rs`, `#[func]`):
   `launch_payloads_json() -> GString` =
   `[{"id":u32,"x":f32,"y":f32,"time":f32,"lifetime":f32,"block":"advanced-launch-pad","team":u8}]`
   (H03 draws pods from it). `gdext/campaign.rs`:
   `set_launch_destination(planet: GString, sector: i64) -> bool` (via T1
   `with_campaign_mut`: set the played sector's `info.destination =
   SectorKey::format(planet, sector)`, refresh import rates of the previous
   destination with `Campaign::refresh_sector_import_rates`, `save_info`);
   `-1` clears. H05 wires the pad's config button to it.
5. Entity save: payloads are not serialized by Java either? They are
   (`@EntityDef` -> class id `LaunchPayload`); with a 120-tick life, skipping
   them is an accepted deviation — note it in the commit body.

**Tests.** In `campaign.rs` tests (use the existing
`landing_pad_config_roundtrips` scaffolding):
`launch_pad_accepts_single_item_type` (advanced: copper yes, then lead no;
legacy: both yes); `launch_pad_launches_full_stack` (advanced pad, efficiency
1, 100 copper, `launch_time = 1799.0`, one `update_tile` with edelta 1 ->
module empty, one `LaunchPayloadState { stacks: [(copper,100)], lifetime: 120 }`, counter 0);
`launch_pad_io_v1_roundtrip`. In `launch_payload.rs`:
`payload_exports_on_removal` (campaign runtime, played sector destination =
another sector, payload copper 100 at `time 119` -> after one system run the
payload is gone and `export["copper"].counter == 100.0`);
`payload_without_destination_expires_silently`.

**Verify in-engine.** GZ resume (GZ is captured in some profiles; any based
sector works). Find a free 4x4 spot near the core (e.g. tiles 120..123,60..63;
check with `tile_debug`), then:
```bash
$G --eval "return $H.place_block(121, 61, \"advanced-launch-pad\")"
$G --eval "return $H.place_block(124, 61, \"power-source\")"
$G --eval "return $H.place_block(121, 64, \"liquid-source\")"
$G --eval "return get_node(\"/root/Spine/Input\").configure_building(121, 64, \"content\", \"oil\")"
$G --eval "return $MC.set_launch_destination(\"serpulo\", 98)"
$G --eval "return $H.debug_add_building_items(121, 61, \"copper\", 100)"
$G --eval "$H.set_paused(true); $H.step(900); return $H.launch_payloads_json()"
$G --eval "$H.step(1000); return [$H.building_items_json(121, 61), $H.launch_payloads_json()]"
$G --eval "$H.step(200); return JSON.parse_string($MC.sector_record_json(\"serpulo\", 170)).export"
```
Expect: items `{}` after <= 1800 ticks, one payload for 120 ticks, then
`export.copper.mean > 0`. If `configure_building` content kind differs, read
`gdext/input/mod.rs` first. Screenshot the pad. 0 panics.

**Commit.** `PAR-0198: launch pads launch items and record exports (H04 T6)`

---

### T7 — PAR-0198 (2/2): landing pads receive imports (v8 import path)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (fake half). `apply_sim_command_json` `place` needs the NUMERIC block id (`"block":428` for `landing-pad`, from `content_list("block").find("landing-pad")`; a name string returns false) and `"team":2` works. Team-2 `landing-pad` (136,66) + team-2 `liquid-source` (139,66) configured `water`, pad configured `copper` (`building_config_json` = `{"type":"item","id":0}`): after 400 ticks the pad holds no liquid (no water consumer/accept) and no items (`runs/20261014-s3-verify2/H04-T7-fake-pad.png`). Recipe below corrected (numeric id).

Depends on T6 (exports) and T2 (counters).

**Symptom / acceptance.** A configured landing pad never receives anything.
After: when another based sector exports item X here, a pad configured to X,
holding >= 1500 water, lands 100 X every `itemCapacity / (importRate/60)`
frames (never faster than cooldown 150 + arrival 150 ticks), removes 1500
water over the 150-tick arrival, then dumps the items. Non-campaign or
non-default-team pads are item sources ("fake"). Acceptance: tests; in engine
the fake-pad half.

**Java** (`J/world/blocks/campaign/LandingPad.java`; summary of lines):
* Fields `:44-47` `arrivalDuration 150, cooldownTime 150, consumeLiquidAmount 100 (content: 1500), consumeLiquid water`;
  content `Blocks.java:6862-6871`: size 4, `itemCapacity 100`, `liquidCapacity 3000`, `consumeLiquidAmount 1500`. **No power.**
  `acceptsItems = false`, `canOverdrive = false`.
* `:88-104` efficiency = `liquids.get(water) >= 1500 ? 1 : 0` (consumer never drains per tick: `.update(false)`).
* `:139-140` `priority = Mathf.rand.nextInt()`; `cooldown`, `landParticleTimer`, `arrivingTimer`, `arriving`, `liquidRemoved`.
* `:146-158` `handleLanding`: `if(config == null) return; cooldown = 1; arriving = config; arrivingTimer = 0; liquidRemoved = 0;` campaign & `!isFake()` -> `importCooldownTimers.put(config, 0)`.
* `:165-201` `updateTimers` (once per `state.updateId`): for each item `perFrame = importRate[item]/60`; `>0` -> `timers[item] += 1/(itemCapacity/perFrame) * Time.delta` else `timers[item] = 0`; then for each `waiting` list: drop pads whose `config != item`, sort by `priority`, `landingPadLanded(first)`, swap `first.priority` with `last.priority`, clear.
* `:273-345` `updateTile`: `updateTimers()`; arriving: `arrivingTimer += delta/arrivalDuration`; `toRemove = min(1500/150*delta, 1500 - liquidRemoved)`; at `arrivingTimer >= 1`: remove the remainder, `items.set(arriving, itemCapacity)`, if `!isFake()`: `produced(arriving, 100)` + `info.handleItemImport(arriving, 100)`; `arriving = null`. Then `if(items.total() > 0) dumpAccumulate(config == null || items.get(config) != items.total() ? null : config)`. Not arriving: `cooldown = clamp(cooldown - delta/cooldownTime)`. Then if `config != null && (isFake() || campaign && !legacyLaunchPads)` and `cooldown <= 0 && efficiency > 0 && items.total() == 0 && (isFake() || importRate(config) > 0 && timers[config] >= 1)`: fake -> land now, else `waiting[config].add(this)`.
* `:348-350` `isFake() = team != defaultTeam || !state.isCampaign()`.
* `:442-481` `acceptItem false`; `config()` item; IO `version 1`: `item config, i priority, f cooldown`, rev>=1: `item arriving, f arrivingTimer, f liquidRemoved` (`TypeIO.writeItem` = `s id` or `-1`).
* `Control.java` `Trigger.newGame` (`:212-216`): `if(info.importRateCache != null) info.refreshImportRates(planet)`; `Universe.runTurn` refreshes every base sector's cache each turn (`:195-197`).

**Port today.** `LandingPadBehavior` (`campaign.rs` ~78, config + cooldown);
`SectorInfoState::get_import_rate(item_index)`, `import_cooldown_timers:
IndexMap<String,f32>` (`rules.rs` ~1004), `refresh_import_rates`,
`Campaign::refresh_sector_import_rates(registry, planet, id)` (`universe.rs`
~486, called only for non-played sectors in `production_pass`). Spec has no
water consumer (`blocks/campaign.rs` ~70-86). Dump: `transfer::dump_accumulate`.

**Changes.**
1. New component `LandingPadState` in `core/entities/comp/building.rs`:
   `pub struct LandingPadState { pub config: Option<ItemId>, pub priority: i32, pub cooldown: f32, pub arriving: Option<ItemId>, pub arriving_timer: f32, pub liquid_removed: f32, pub wants_landing: bool }`.
   `create_state` inserts it with `priority = building tile packed` (deviation:
   Java `Mathf.rand.nextInt()`; the port has no world RNG for behaviors;
   deterministic ties only). Move config from `CampaignState.item` to
   `LandingPadState.config` (keep `landing_item()` working).
2. `LandingPadDef`: add `arrival_duration 150.0, cooldown_time 150.0, consume_liquid_amount 1500.0, consume_liquid: LiquidId (water)`; fill in `from_def` for `"landing-pad"`.
3. `LandingPadBehavior`:
   * `accept_item -> false`; `accept_liquid`: `liquid == water` (capacity is
     enforced by the mover).
   * local efficiency: `eff = liquids(water) >= 1500`.
   * `update_tile`: arrival/cooldown/dump/wants-landing per Java (use `delta`
     = 1 tick; `canOverdrive=false`). On arrival complete and not fake:
     `transfer::record_produced(world, e, item, 100)` (T2) and queue
     `handle_item_import(name, 100)` through a new
     `CampaignHookQueues.imports: Vec<(ItemId, i32)>` that
     `campaign_tick_system` folds into the played sector's info. Fake pads
     land immediately (`handle_landing`).
   * `is_fake`: `team != queues.default_team || !queues.is_campaign`
     (read `CampaignCoreQueues`).
   * IO version 1 per Java (item = `s` id, `-1` none).
4. New system `landing_pads_system` (campaign schedule set, after
   `launch_payload_system`): only with a `CampaignRuntime`, campaign, and
   `!planet.campaign_rules.legacy_launch_pads`: (a) advance
   `import_cooldown_timers` of the played sector for every item from
   `get_import_rate(item)`; (b) collect pads with `wants_landing` grouped by
   config item in a `BTreeMap<u16, Vec<(priority, Entity)>>`, drop pads whose
   config changed, sort by priority, land the first (`handle_landing`: sets
   state + resets `import_cooldown_timers[item] = 0`), swap its priority with
   the last, clear every `wants_landing`.
5. Import cache for the played sector: in `start_sector_with_loadout` after
   install, call `campaign.refresh_sector_import_rates(&registry, planet_id, sector_id)`
   on the runtime campaign (Java `Trigger.newGame`), and in `run_turn`
   refresh it for every base sector including the played one (Java
   `:195-197`; move the call out of the `is_attacked || being_played` skip).
6. Seam for H03: `#[func] landing_pads_json() -> GString` on `MindSimHost`:
   `[{"x","y","block","config":name|null,"arriving":name|null,"arriving_timer","cooldown","id":entity index}]`.

**Tests.** `landing_pad_lands_when_timer_full`: campaign runtime, played
sector import rate copper = 1.0 item/s (set `windows.import_rate_cache`),
pad configured to copper, 3000 water, `import_cooldown_timers["copper"] = 0.999`;
one tick -> timer `>= 1` (`0.999 + 1/6000`), pad `arriving == copper`; after
150 more ticks pad holds copper 100 and water 1500.
`fake_landing_pad_is_an_item_source`: non-campaign sim, team 2 pad with water
-> lands without imports. `landing_pad_io_v1_roundtrip`.

**Verify in-engine.** Fake half only (two based exporting sectors do not exist
in the imported profile — record that): GZ resume, then (verified 2026-10-14;
`place` needs the numeric block id):
```bash
LP=$($G --eval "return $H.content_list(\"block\").find(\"landing-pad\")" | python3 -c 'import json,sys;print(int(json.load(sys.stdin)["result"]))')   # 428
LS=$($G --eval "return $H.content_list(\"block\").find(\"liquid-source\")" | python3 -c 'import json,sys;print(int(json.load(sys.stdin)["result"]))')
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":136,\"y\":66,\"block\":$LP,\"rotation\":0,\"team\":2}')"
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":139,\"y\":66,\"block\":$LS,\"rotation\":0,\"team\":2}')"
$G --eval "return [$I.configure_building(139,66,\"content\",\"water\"), $I.configure_building(136,66,\"content\",\"copper\")]"
$G --eval "$H.set_paused(true); $H.step(400); return [$H.liquid_debug(136,66), $H.building_items_json(136,66)]"
```
Expect water in the pad and copper landing (before the fix: no liquid, `{}`).
Screenshot. 0 panics.

**Commit.** `Fixes PAR-0198: landing pads import items (H04 T7)`

---

### T8 — PAR-0193: in-engine wave countdown fixture; fresh-launch core check

> **SKIP: symptom not reproduced** (2026-10-14, session 3) — the live countdown and `runWave` already work in engine, and a fixture exists without a new seam: GZ resume, `$H.logic_set_flag("allowWaves", true)` (GZ-2 runs `setrule waveSending/waveTimer true`), `step(30)` -> `waveTimer true`, `wavetime 21571`; `step(300)` -> 21271 (exactly -300); 4x `step(4000)` + `step(1400)` -> `wave 2`, `wavetime 7071` (reset to 7200), units 1 -> 4 (wave 1 = dagger + 2 flares: the H00 T2b off-by-one). `get_hud_state().wavetime` tracks it live (6828 -> 6519 over 4 s unpaused). Screenshots `runs/20261014-s3-verify2/H04-T8-countdown-a.png`/`-b.png`. Fresh frozenForest (64) launch: `hasCore true`, `best_core_position` (320,560) (with H00 T1's env fix applied locally), `wave 0`. PAR-0193 can be closed with this evidence; `debug_set_wave_rules` is only a convenience (coordinator decides whether to keep it).

**Symptom / acceptance.** G04's tick-based countdown has no in-engine proof
because no profile sector has `waveTimer:true` and a live core (Ground Zero's
save has `waveTimer:false`; Java's world processors flip it during the
tutorial). Provide a debug seam and prove the countdown and `runWave` live.

**Java.** `Logic.java:560-569`:
```java
if(state.rules.waves && state.rules.waveTimer && !state.gameOver){
    if(!isWaitingWave()) state.wavetime = Math.max(state.wavetime - Time.delta, 0);
}
if(!net.client() && state.wavetime <= 0 && state.rules.waves) runWave();
```
`runWave` (`:319-325`): `wave++; wavetime = waveSpacing * difficulty.waveTimeMultiplier`.

**Port today.** `campaign_tick_system` (`core/game/runtime.rs` ~386-410) does
this. `logic_set_rule` on SimHost is a stub returning false
(`gdext/sim_host.rs` ~1512); `set_rules_json` edits only the facade and is
rejected in campaign (`allowEditRules = false`). Readback exists:
`$H.logic_rules_probe()` (`waveTimer`, `waves`, `wave`, `wavetime`).

**Changes.**
1. `gdext/campaign.rs`:
   ```rust
   /// Playtest seam (debug only): sets the live runtime's `rules.waveTimer`,
   /// `rules.waves` and `state.wavetime` (ticks). Returns false without a runtime.
   #[func] pub fn debug_set_wave_rules(&mut self, wave_timer: bool, waves: bool, wavetime: f64) -> bool
   ```
   (take runtime, set, put back, mirror the three into `self.session`).

**Tests.** None new (G04's `wave_timer_counts_ticks`,
`waiting_wave_holds_timer`, `wave_runs_without_timer_when_due` cover the
logic); run them by name.

**Verify in-engine.** GZ resume:
```bash
$G --eval "$H.set_paused(true); return $MC.debug_set_wave_rules(true, true, 600.0)"
$G --eval "$H.step(300); return JSON.parse_string($H.logic_rules_probe())"   # wavetime 300, wave W
$G --eval "$H.step(301); return JSON.parse_string($H.logic_rules_probe())"   # wave W+1, wavetime ~7200*mult
$G --eval "$H.step(120); return $H.get_group_count(\"unit\")"               # > 1 (wave units spawned)
$G --eval "return $MC.get_hud_state()"                                      # wave text/countdown agrees
$G --screenshot runs/<date>-s<N>-H04-T8/countdown.png
```
Second check (fresh launch, G04 saw `hasCore:false`): H-environment §4.1
fresh recipe with sector **64** (frozenForest): `load_sector`, `start_sector`,
then `return $MC.get_hud_state().hasCore`. Java frozenForest is a survival
preset with a player core: expect `true`. If false, record
`$H.best_core_position()` and `$MC.get_sector_state()` in the summary and file
it for the coordinator (do not fix here; likely env/placement, H00/H02).
0 panics.

**Commit.** `Fixes PAR-0193: live wave countdown proven in engine; debug_set_wave_rules seam (H04 T8)`

---

### T9 — Unit save chunk: count equals written chunks; revision 3

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Team-2 dagger via `apply_sim_command_json` `spawn_unit` (`"unit":0`), `debug_apply_status(<id>,"wet",6000)` -> `statuses ["wet"]`; `run_exit_save()` + Go -> the dagger is restored with `statuses []`. Use a harmless status (`wet`) — `burning` 600 ticks may kill the dagger before the save. Unit ids come from `unit_debug_json()` (`id` field, e.g. 1000000001).

**Symptom / acceptance.** `entity_count` counts units that `write_entities`
skips -> the reader eats fire/puddle bytes as unit chunks. Units lose
statuses, shield, carried items, elevation, `spawnedByCore`, command and
build plans on every save. Acceptance: tests.

**Java.** `SaveVersion.java:425-439`: count = number of chunks written. Unit
latest-revision fields (e.g. `JA/resources/revisions/alpha/5.json`):
`abilities, aimX, aimY, controller, elevation, flag, health, isShooting,
mineTile, mounts, plans, rotation, shield, spawnedByCore, stack, statuses,
team, type, updateBuilding, vel, x, y` (+ `baseRotation`, `payloads`).

**Port today.** `core/io/sim_io.rs` ~305 `entity_count`, ~327
`write_entities`, `SIM_UNIT_REVISION = 2` (~56), reader ~166 accepts 1 and 2,
`DecodedUnit` (~101), `apply_decoded_entities` (~560: skips default-team core
unit types by name). Components (`core/entities/comp/unit/comp.rs`):
`UnitCore { rotation, dead, elevation, boosting, spawned_by_core }` (~20),
`ItemsComp { item: Option<(ItemId,i32)> }` (~392), `ShieldComp { shield }`
(~419), `StatusComp { statuses: Vec<StatusEntry{effect, duration}>, … }`
(~450; mutate only through its methods — find the apply method with
`grep -n "pub fn " client/rust/mind-core/src/entities/comp/unit/comp.rs | sed -n '/impl StatusComp/,$p'`),
`UnitCommandComp(u16)` (`core/ai/controller.rs` ~120), `CommandAiState`
(`core/ai/types/command.rs` ~66), builder plans: find with
`grep -rn "struct .*Plan.*Queue\|plans:" client/rust/mind-core/src/entities/comp/unit/ client/rust/mind-core/src/sim/builder.rs`.

**Changes.**
1. `fn live_unit_rows(&self) -> Vec<(u64, Entity, String)>` with exactly the
   `write_entities` filters (valid entity, `UnitCore`, `UnitTypeComp`, `Pos`,
   known name); `entity_count = rows.len() + fires + puddles`;
   `write_entities` iterates the rows.
2. `SIM_UNIT_REVISION = 3`; append after the rev-2 payload block:
   `bool spawned_by_core`, `f elevation`, `f shield` (0 without component),
   `s stack_item (-1 none), i stack_amount`, `i status_count` + per status
   `s effect_id, f duration`, `s command_id (-1 none)`, `bool has_target` +
   `f x, f y`, `i plan_count` + per plan the existing Java plan codec
   (`grep -n "fn write_plan\|fn read_plan" client/rust/mind-core/src/io/typeio/codecs.rs`).
   Reader accepts 1, 2, 3.
3. `apply_decoded_entities`: restore each field; skip a unit iff
   `spawned_by_core && team == default_team` (rev 3), keep the name fallback
   for rev <= 2.
4. Update `live_unit_class_id_is_reserved` (~1298) to 3.

**Tests.** `entity_count_matches_written_chunks` (2 daggers, remove
`UnitCore` from one, plus one fire -> save, load OK, 1 dagger + 1 fire);
`unit_rev3_round_trip` (dagger elevation 0.5, copper x10, `burning` 120
ticks, shield 20, command `move` target (100,100) -> identical);
`unit_rev2_bytes_still_decode` (hand-built rev-2 chunk).

**Verify in-engine.** GZ resume, spawn a team-2 dagger with
`apply_sim_command_json` spawn_unit, `debug_apply_status(seq,"burning",600)`,
`run_exit_save()`, Go -> `unit_debug_json()` shows the dagger with the
status. 0 panics.

**Commit.** `Gameplay H04 T9: unit chunk count matches written chunks; unit save revision 3`

---

### T10 — Java save import: units, fires, puddles (class-id chunks)

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in engine (headless task). Code confirmed: `LiveEntitySink::supports_class` (`core/io/sim_io.rs:149`) accepts only 0xF0-0xF2; the profile's Java GZ save (`saves/sector-serpulo-15.msav` in the zip) is the only Java sector save.

Depends on T9. Low in-engine impact (the profile's only Java sector save
holds just the core alpha) but needed for any Java `.msav` with units.

**Java.** Entity chunk (`SaveVersion.java:441-448`): `b classId, i id,
entity.write(out)`; generated write = `s revision` + fields in manifest order
(`JA/java/mindustry/annotations/entity/EntityIO.java:83-96`). Serializers
(`J/io/TypeIO.java`): content -> `s id`; `Team` -> `b`; `Tile` -> `i pos`;
`Vec2` -> `f,f`; `ItemStack` -> `s item, i amount`; `Ability[]` -> `b len, f×len`;
`WeaponMount[]` -> `b len, (b state, f aimX, f aimY)×len`; `UnitController`
-> `writeController` (776-835); `Seq<StatusEntry>` -> `i size` + `writeStatus`
(1031-1056); `Queue<BuildPlan>` -> `i size` + `writePlan` (649-659). Class ids
in `JA/resources/classids.properties`; latest revisions = highest file in
`JA/resources/revisions/<name>/`. Fire rev 1: `lifetime f, tile i, time f, x f, y f`;
Puddle rev 1: `amount f, liquid s, tile i, x f, y f`.

**Port today.** `LiveEntitySink::supports_class` (`sim_io.rs` ~149) only
0xF0-0xF2.

**Changes.** New `core/io/java_entities.rs` with a hand-built field table per
(class id, latest revision) — build it by reading every
`JA/resources/revisions/<class>/<latest>.json` for the Serpulo unit classes
(alpha, beta, gamma, dagger-family `mace`/`fortress` classes, flare, horizon,
zenith, mono, poly, mega, nova…, plus FireComp 10 and PuddleComp 13; list
them with `ls JA/resources/revisions/`). `read_java_unit(class_id, r, names) -> IoResult<Option<DecodedUnit>>`:
read `us` revision; unknown (class, revision) or non-empty payloads ->
`Ok(None)` (chunks are length-bounded, so skipping is safe). Map
`type` -> unit name. `LiveEntitySink` accepts class ids `< 0xF0` when the
load is a Java/legacy save and dispatches. Skip `spawnedByCore` units.

**Tests.** `java_unit_chunk_decodes` (hand-built chunk from the manifest of one
class, e.g. `dagger`'s class — read its manifest file to get the exact field
order and build the bytes in that order); `java_gz_fixture_skips_core_unit`
(the existing legacy fixture: 0 units restored, no error).

**Verify in-engine.** Headless only (state why in the commit).

**Commit.** `Gameplay H04 T10: decode Java unit, fire and puddle chunks on import`

---

### T11 — PAR-0164: one shared settings store

> **In-engine verification (2026-10-14, session 3):** PARTLY. The three stores exist (code), but the user-visible symptom is masked today: `MindUi.settings_put_flag("skipcoreanimation", true)` then a fresh sector 97 -> `is_cutscene()` false right after `start_sector`, because `start_sector` reads the flag through the `live_setting_flag` workaround (`gdext/campaign.rs:267`). The plan's verify step therefore passes BEFORE the change; keep it as a regression check and add the discriminating half: after migration `grep -n "live_setting_flag\|live_save_interval_seconds" client/rust/mind-gdext/src/campaign.rs` prints nothing and the same check still passes.

Unchanged from the round-G design (it was never started). Java: one
`Core.settings`. Port: `SettingsStore` fields in `gdext/ui/ui_host.rs`
(`MindUi`), `gdext/campaign.rs` (`MindCampaign`), `gdext/mods.rs`
(`MindMods`), plus a temporary store in `gdext/input/mod.rs`; workarounds
`live_setting_flag`, `live_setting_flag_default`, `live_save_interval_seconds`
in `gdext/campaign.rs`.

**Changes.**
1. New `gdext/settings_hub.rs`:
   ```rust
   static HUB: std::sync::OnceLock<std::sync::Mutex<SettingsStore>> = std::sync::OnceLock::new();
   pub fn init(paths: &Paths)                                          // idempotent load
   pub fn with_settings<R>(f: impl FnOnce(&mut SettingsStore) -> R) -> R // poisoned -> into_inner
   pub fn reload(paths: &Paths)                                        // after data import / clear
   pub fn save(paths: &Paths) -> Result<(), IoError>                   // force_save
   ```
   `mod settings_hub;` in `gdext/lib.rs`.
2. Migrate MindUi, then MindCampaign (T1's `with_campaign_mut` passes
   `&mut SettingsStore`: give it the hub inside the closure — call
   `settings_hub::with_settings(|s| …)` around the campaign closure, never
   nested), then MindMods, then the input bindings store. Data import / clear
   game data -> `settings_hub::reload`. Delete the `live_setting_*` helpers.

**Tests.** `settings_hub.rs` `#[cfg(test)]` with a temp dir: two closures
share a write; `save` then a fresh `SettingsStore::load` sees it.

**Verify in-engine.** `get_node("/root/MindUi").settings_put_flag("skipcoreanimation", true)`
then a fresh saltFlats-style launch of an unplayed preset (H-environment §4.1
fresh list, e.g. 97) -> `get_node("/root/Spine/World/Camera2D").is_cutscene()`
is false right after `start_sector`. Restore the flag.

**Pitfalls.** `std::sync::Mutex` is not re-entrant: never call Godot or
another facade inside `with_settings`; grep every closure after migration.

**Commit.** `Fixes PAR-0164: one shared settings store for MindUi, MindCampaign and MindMods (H04 T11)`

---

### T12 — Small parity leftovers: `ignoreBuildDarkness`, dead `BeamDrillState`, markers decision

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in engine (editor/env blocks, dead code, data measurement). Code confirmed: no `ignore_build_darkness` anywhere; `BeamDrillState` only in its definition, the `comp/mod.rs:16` re-export and a comment (`world/behavior/production.rs:1592`); legacy reader skips `REGION_MARKERS` (`v1.rs:313`).

1. `ignoreBuildDarkness` (`J/world/Block.java:377`; true in
   `J/world/blocks/environment/Floor.java:116`, `StaticWall.java:35`,
   `RemoveWall.java:19`, `RemoveOre.java:19`; used `J/world/Build.java:229`
   and `:257` `!check.floor().placeableOn && !type.ignoreBuildDarkness`):
   add `pub ignore_build_darkness: bool` to `BlockDef` (default false; set
   true for every block whose kind is floor/static wall/ore/remove-wall/
   remove-ore in the content builder), and in `valid_place`
   (`core/world/build.rs` ~196) skip the darkness check and the solid-floor
   check when set. Test `env_blocks_ignore_build_darkness`. Golden content
   dump may change (regenerate per §2 with the Java reason).
2. `BeamDrillState` (`core/entities/comp/building.rs` ~171): never inserted;
   the beam-drill codec uses `DrillState`. Delete the struct and its re-export
   (`core/entities/comp/mod.rs` ~16) after
   `grep -rn "BeamDrillState" client/rust` shows no other use than the codec
   comment (update that comment).
3. Markers (Java UBJson `IntMap`, `J/game/MapMarkers.java:57-74`): measure
   first. Add a test that loads every `assets/maps/serpulo/*.msav` through the
   legacy reader with a counting hook in `skip_legacy_region(REGION_MARKERS)`
   and asserts/prints the region byte length. If every region decodes to an
   empty map (UBJson `{}` / count 0), keep the skip, add the test as a guard
   ("Java presets ship no markers; objective markers live in rules
   objectives") and stop. If any is non-empty, decode with the existing
   `core/io/json/ubjson.rs::read_ubjson`, convert the `IntMap` entries to the
   port's `ObjectiveMarker` JSON (`JsonIo::read` on `serde_json` text; class
   tags resolved by `marker_name_to_type`, `core/game/map_markers.rs` ~52)
   and feed `MapMarkers`. Ground Zero has 3 `ShapeText` byte matches, which
   may be inside rules objectives — the measurement decides.

**Commit.** `Gameplay H04 T12: ignoreBuildDarkness, drop dead BeamDrillState, markers region decision`

---

### V1 — Verify (no code expected): PAR-0084 after T3; loss keeps `rules.waves`

> **In-engine verification (2026-10-14, session 3), run before T3:** V1.1 — the lost-sector replay works today: GZ resume, `debug_kill_building(129,55)`, `step(120)` -> `gameOver true`, `hasCore false`; close the dialog, Go -> `hasCore true`, core at (1032,440), drill (130,48) restored as team 0 (derelict), conveyor (130,51) gone, launch pad team 0 (`runs/20261014-s3-verify2/H04-V1-loss.png`, `H04-V1-replay.png`). The stored record after the loss had `hasCore true`, `lastPresetName null`, `spawnPosition 8454199`. V1.2 — `get_sector_state().waves` stays `true` on the loss (Java-correct). After T3, `lastPresetName` must be `"groundZero"` before the kill or the replay will (correctly) fall back to a fresh sector.

1. PAR-0084 (lost-sector replay) after T3 changed its conditions: clean
   re-import, fresh Ground Zero is not possible (GZ has a save) — use the
   PAR-0084 ledger recipe: GZ resume -> `run_exit_save` -> resume ->
   `debug_kill_building` on the core (129,55) -> loss -> re-enter -> base
   restored as derelicts + new core at the spawn position. Expect
   `get_hud_state().hasCore == true` and derelict walls present
   (`building_team_at`). Note: T3 now also requires `sectorDataMatches`, i.e.
   `lastPresetName == "groundZero"` in the record — the exit save of step 1
   writes it.
2. G03's "loss flips rules.waves": on the GZ loss of step 1, before
   re-entering: `return [$MC.get_sector_state().waves, JSON.parse_string($MC.game_stats_json()).waves]`.
   Java keeps `waves` true on a loss (only `checkGameState`'s "no spawns"
   branch clears it). GZ has ground spawns, so expect `true`. If false,
   find the writer with the §1 grep list and file it.

Report both in the summary; no commit unless a defect is fixed.

---

### T13 — Loaded units get Java's controller again (`UnitComp.afterRead` -> `type.createController`) (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** After resuming a sector, the player team's own units (e.g. a
dagger made by a ground factory before saving) come back with `GroundAI` and cannot be
selected/commanded in command mode (round-3 verification, `verify-s3-round3.md`). Java:
every unit read from a save resets its controller with `type.createController(unit)` unless
the controller keeps state, so a player-team dagger is `CommandAI` again. After: on resume,
player-team commandable units are `AiKind::Command` (appear in `selectable_units_json()`),
wave-team units get their AI, LogicAI/CommandAI units keep theirs.

**Java.**
* `J/entities/comp/UnitComp.java:585-593`:
  ```java
  @Override
  public void afterRead(){
      setType(this.type);
      controller.unit(self());
      //reset controller state
      if(!(controller instanceof AIController ai && ai.keepState())){
          controller(type.createController(self()));
      }
  }
  ```
* `J/type/UnitType.java:281-283`: `aiController = () -> !flying ? new GroundAI() : new FlyingAI();`
  `controller = u -> !playerControllable || (u.team.isAI() && !u.team.rules().rtsAi) ? aiController.get() : new CommandAI();`
  `:552-554` `createController(unit) = controller.get(unit)`. `Team.isAI()` reads the LOADED
  rules (`state.rules` is set before entities are read: `SaveVersion` reads the rules/meta
  region first).

**Port today.**
* Resume path: `core/io/sim_io.rs` `apply_decoded_entities` (~550-620) re-creates units
  through `Sim::spawn_live_unit` -> `UnitRuntime::spawn` which chooses the controller via
  `crate::entities::comp::unit::lifecycle::select_team_controller(world, def, team)`
  (`lifecycle.rs:~130`), whose `team_ai_flags` reads the `CampaignRuntime` session rules —
  or, when no `CampaignRuntime` is installed yet, falls back to `BuildRules.default_team`,
  which "starts at derelict before a session sets it" (doc comment in `lifecycle.rs`), making
  team 1 look like an AI team -> `select_ai` -> GroundAI.
* `ai/controller_registry.rs` `select_controller` and `keep_state(kind)` (Logic, Command)
  are faithful.

**Changes.**
1. Diagnose first (5 min): add a temporary `log::debug!` in `apply_decoded_entities` printing
   whether `world.get_resource::<CampaignRuntime>()` is `Some` when units are re-spawned on the
   GZ resume path (gdext `campaign.rs` resume -> `load_sector`/`apply_decoded_entities` order:
   `grep -n "apply_decoded_entities\|install_campaign_runtime\|put_campaign_runtime" client/rust/mind-gdext/src/campaign.rs client/rust/mind-gdext/src/sim_host.rs`).
   Remove the log after.
2. Fix faithfully, independent of the order: add `pub fn reset_loaded_controllers(&mut self)` on
   `Sim` (in `core/io/sim_io.rs` next to `apply_decoded_entities`) that, for every unit entity
   restored by the load (keep their entity ids in a `Vec` from `apply_decoded_entities`), sets the
   controller slot to `select_team_controller(world, def, team)` UNLESS the saved controller kind
   satisfies `keep_state` (the port's unit save carries the controller kind only from revision 3
   — H04 T9; before T9 treat every restored unit as "no kept state"). Skip the player unit
   (`AiKind::Player`). Call it at the END of the resume path, after the session rules are
   installed into the world (the same place that sets `rules_world_dirty`/`apply_rules_to_world`
   on resume). Use the helper that `G01 T9 possession` uses to switch a unit's controller
   (`grep -n "fn set_unit_controller\|slot.kind =" client/rust/mind-core/src/sim/*.rs`) so
   per-AI state is reset too.
3. If step 1 shows the runtime IS installed before units are re-spawned, find why
   `team_ai_flags` still returns AI for team 1 (e.g. `session.rules.default_team` not yet
   copied) and fix that instead; still keep step 2's call (it is Java's `afterRead` semantics).

**Tests** (`core/io/sim_io.rs` tests, next to `apply_decoded_entities_skips_core_units`):
`loaded_player_team_units_get_command_ai`: campaign session, `default_team = 1`, a team-1
dagger and a team-2 dagger in the decoded entities; after apply + `reset_loaded_controllers`:
team-1 dagger `AiKind::Command`, team-2 dagger `AiKind::Ground`. `loaded_logic_unit_keeps_state`
(after T9, revision 3 with a Logic controller kind): stays `AiKind::Logic`.

**Verify in-engine.** Ground Zero via "Go"; spawn a team-1 dagger
(`apply_sim_command_json` `spawn_unit` with the numeric dagger index and `"team":1`), confirm it
is in `selectable_units_json()`; `$MC.run_exit_save()`; re-enter with "Go";
`selectable_units_json()` still lists the dagger and `unit_debug_json()` shows its controller as
Command. Panic 0.

**Commit.** `Gameplay H04 T13: loaded units reset to type.createController (player-team units commandable after resume)`

---

### Deferred (out of this plan)

* **Interplanetary Accelerator** (`J/world/blocks/campaign/Accelerator.java`):
  planet launch to Erekir; needs H05 planet-launch UI + H03 cutscene. File a
  ledger item "Interplanetary Accelerator is a stub".
* **Erekir `prebuildBase`** (`Control.java:223-283`): Erekir only.
* **Legacy-launch-pad export cap** (`Universe.java:215-223`, marked TODO in
  Java): only with `legacyLaunchPads` on (now default off).
* **`Sector.addItems` while the destination is being played**
  (`Sector.java:219-230` adds to the live core): only reachable with legacy
  pads; note.
* **Turn RNG** `JavaRandom(turn+1)` vs `Mathf` global: accepted deviation.

---

## §3. Execution order, dependencies, hot files

1. **T1** (foundation: every later task writes the runtime campaign).
2. **T2** (offline production; needed by T3's in-engine check and T6/T7).
3. **T3** (resume/launch bookkeeping; uses T1 + T2).
4. **T4** (turns/invasions; uses T1).
5. ~~**T5**~~ withdrawn (H09 T10 owns the stat listeners).
6. **T6** then **T7** (PAR-0198; T7 needs T6 + T2).
7. **T8**: SKIP (verified 2026-10-14: the live countdown works; PAR-0193 closes).
8. **T9**, then **T13** (loaded-unit controllers; uses T9's revision-3 controller kind when present), then **T10**.
9. **T11** (touches many gdext files; last to limit conflicts).
10. **T12**, then **V1**.

Hot files shared with other H plans (small commits, merge often):
* `gdext/campaign.rs` — H05 (planet/launch dialogs), H03 (HUD dict): T1, T3, T4, T5, T6, T8, T11.
* `gdext/sim_host.rs` — everyone: T3, T5, T6, T7.
* `core/game/runtime.rs` — H01 (team AI), H07 (weather): T2, T4, T5, T6, T7.
* `core/sim/unit_runtime.rs`, `core/sim/builder.rs` — H01/H02/H08: T5 (call sites only).
* `core/world/behavior/campaign.rs`, `core/world/blocks/distribution/transfer.rs`, `core/world/build.rs` — H02: T2, T6, T7, T12.
* `ui/dialogs/planet_dialog.gd` — H05: T3 step 7 (one line).
* `parity/golden_content.json` / `golden_manifest.json` / UI campaign golden — T4, T6, T12.

---

## §4. Ledger changes to report (coordinator applies; never edit chains.json)

| Item | Change |
|---|---|
| PAR-0198 | `fixing` after T6, `verified` after T7 (+ in-engine T6 evidence; T7 fake-pad half) |
| PAR-0193 | `verified` after T8 (countdown eval output + screenshot) |
| PAR-0164 | `verified` after T11 |
| PAR-0084 | note: re-verified after T3 (V1), or reopened with the observation |
| new | "Campaign turn clock and offline production lost: facade campaign overwrites the live runtime copy; left sectors stay being_played" -> `verified` (T1) |
| new | "Sectors never produce while away: produced() is a no-op, statistics refresh every 60 s, exports never decay" -> `verified` (T2) |
| new | "Resume ignores the sector record (items, winWave/attack), no lastPresetName/quietUnlock/setLastSector, launches have no origin" -> `verified` (T3) |
| new | "Invasions of the played sector never restart waves; campaign rules default invasions/legacy pads on" -> `verified` headless (T4) |
| new | "Game Over and planet statistics stay zero (Logic stat listeners missing)" -> `verified` (T5) |
| new | "Unit save: entity count can exceed written chunks; statuses/items/shield/commands lost" -> `verified` (T9) |
| new | "Java save import drops units/fires/puddles" -> `verified` headless (T10) |
| new | "Interplanetary Accelerator is a stub" -> `seeded` (deferred) |

---

## §5. Done criteria

* Every task committed (one commit each) with test names and in-engine eval
  output in the body; T7/T10 marked "headless only" where stated.
* `cargo test -p mind-core --lib --features msav-import` green (H00 baseline +
  new tests); `cargo test -p mind-core --test legacy_msav_import --features msav-import`
  green; `mind-headless --lib` no new failure; clippy `-D warnings` clean;
  `parity goldens` PASS; GDScript parse gate clean.
* In engine after a clean re-import: `universe_json().turn` survives exit and
  restart; a drill raises `rawProduction`; resumed GZ core equals the record;
  a launch pad empties and records an export; the wave countdown drops 300 per
  300 ticks and runs a wave; 0 `panic` lines.
* Summary `REPO/docs/H04-saves-campaign-loop_summary.md` per H-environment §6.

---

## §6. Cross-area seams (exact names)

Provided by H04:
| Seam | Task | Consumer |
|---|---|---|
| `#[func] MindSimHost.launch_payloads_json() -> GString` `[{id,x,y,time,lifetime,block,team}]` | T6 | **H03** launch-pod drawing (`LaunchPayloadComp.draw`: fin = time/lifetime, `randomSeedRange(id,…)`, region `<block>-pod`, fallback `launchpod`) |
| `#[func] MindSimHost.landing_pads_json() -> GString`; component `LandingPadState { config, priority, cooldown, arriving, arriving_timer, liquid_removed, wants_landing }` in `core/entities/comp/building.rs` | T7 | **H03** landing-pad pod/cooldown drawing (`LandingPad.draw` 205-265, light 268-270) |
| `#[func] MindCampaign.set_launch_destination(planet: GString, sector: i64) -> bool` (`-1` clears) | T6 | **H05** launch-pad config button (`LaunchPad.buildConfiguration` -> `ui.planet.showSelect`) |
| loadout JSON key `"origin": <launcher sector id>` for `start_sector_with_loadout` (H04 edits the one line in `planet_dialog.gd` `_launch`) | T3 | **H05** (keep the key when editing `_launch`) |
| `#[func] MindCampaign.sector_record_json(planet, sector) -> GString`, `universe_json() -> GString` | T1 | H05 sector panel (exports/imports/production), playtests |
| `#[func] MindCampaign.take_unlocked_content() -> PackedStringArray` | T2 | **H05** `hudfrag.showUnlock` popup (`Control.java:160-174`) |
| `#[signal] MindCampaign.sector_invaded(planet, sector)` + toast via `/root/MindHud.push_toast` (`sector.attacked`) | T4 | H03 (toast styling only) |
| `#[func] MindCampaign.debug_set_wave_rules(wave_timer, waves, wavetime) -> bool` | T8 | any plan needing a live wave timer (H01 wave smoke, H07 wave sounds) |
| `transfer::record_produced(world, e, item, amount)` (`core/world/blocks/distribution/transfer.rs`) | T2 | **H02**: any behavior that produces items without `offload` (separator, item source, beam drill already call trait `produced`) must call it |
| `CampaignRuntime.rules_world_dirty = true` on played-sector invasion | T4 | (existing drain, `Sim::apply_rules_to_world`) |

Needed from others:
* **H02** owns block behaviors: T6/T7 add only the campaign pads' state/IO/accept;
  if H02 changes `ItemModule` APIs (`first`, `set`, `total`) re-check.
* **H06** owns logic codecs; nothing new here.
* **H01**: T5 calls `record_stat` at unit death/creation sites in
  `unit_runtime.rs` (one-line calls; coordinate if H01 moves them). T9 needs
  the `StatusComp` apply method and `CommandAiState` fields as they are on
  `main` when T9 runs.
* **H05**: `showUnlock` popup, launch destination UI, landing-pad
  "redirect exports" button (`LandingPad.java:388-399`: set every exporting
  sector's `info.destination` to this sector) — H05 can implement it with
  `with_campaign_mut` semantics through a new seam it owns, or ask H04's
  coordinator; not in this plan.
