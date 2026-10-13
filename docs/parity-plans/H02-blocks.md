<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H02 — Block behavior + block configuration: audit and execution plan

Date: 2026-10-14. Area: H02 (per-block behavior, block configuration and tap-to-link,
placement/break validity and the build rules guard, core placement/upgrade/incineration,
power/liquid/defense block behavior, per-kind save bodies the round-G pass missed).

**Read `docs/H-environment.md` first** (paths `REPO`, `WT`, `core/`, `gdext/`, `ui/`, `J/`;
build/test/gmcp recipes; §4.2 playtest seams; commit rules). Then `docs/H-INDEX.md` §3
(cross-plan seams) if it exists. This plan does not repeat that setup.

Start state: H00 has landed on `main`:
`git log main --oneline | grep -m1 'H00'` prints a line. If it does not, stop and report.
After H00 the Ground Zero player alpha spawns (`rules.env` = 233) and
`SimCommand::Place{player: Some}` queues instead of applying instantly.

**Drift rule.** Port line numbers below were read on `main` @ `ffbe84dd` (2026-10-14) and
drift. Always re-locate with `grep -n "<function name>" <file>`. If a function named here
does not exist, or the code already does what the task asks, or it differs so much that the
task text no longer applies: stop that task, write down exactly what you saw (file, line,
code) in your summary, and go to the next task. Never improvise a different design.

Java line numbers (`J/...:N`) were read on 2026-10-14 and are authoritative; paste them into
doc comments as `File.java:N`.

Test helper facts (verified 2026-10-14):
* `core/sim/production_tests.rs`: `runtime_sim()` (32x32 stone, block runtime, **no unit
  runtime**), `place(&mut sim, "name", x, y) -> Entity` (API `SimCommand::Place`, team 1,
  rotation 0, `player: None`), `item(&sim, "copper")`, `liquid(&sim, "water")`,
  `set_floor(&mut sim, x, y, "deep-water")`, `configure(&mut sim, x, y, wire_value)`,
  `stored_item(&sim, e, item)`, `stored_liquid(...)`.
* `core/sim/combat_tests.rs`: `live_sim(size)` (block + unit runtime, air floor),
  `place(&mut sim, x, y, "name", team) -> Entity`, `sim.spawn_live_unit(name, team, px, py, rot)`,
  `health(&sim, e)`, `feed(&mut sim, e, "copper", n)`.
* `core/sim/builder.rs` `mod tests`: `sim_with_player() -> (Sim, Entity /*unit*/, u16 /*core id*/, BlockId /*copper-wall*/)`
  (64x64, core-shard team 1 at (10,10), alpha at world (80,80)); `team_core(&world, team)`.
* Footprint rule: a size-`s` block at `(x,y)` covers `x-(s-1)/2 ..= x+s/2` (3x3 at 20 covers
  19..21; 2x2 at 5 covers 5..6).
* Derelict team id is `0`; player/sharded team is `1`; crux is `2`.

---

## 1. Inventory

### 1.1 How blocks are wired in the port (read once)

* `BlockKind` = Java class (`core/content/registries/blocks/*.rs`, `spec("<name>", BlockKind::X)`).
* Behavior per block: `core/world/blocks/mod.rs::default_registry` (family `register`
  functions, by name) else `core/world/behavior/mod.rs::default_behavior` (by kind) else
  `NoopBehavior`. Trait `BuildingBehavior` is in `core/world/behavior/mod.rs`.
* Item/liquid intake defaults now follow Java (`helpers::consumer_accept_item`,
  `consumer_accept_liquid`, coolant via the `LiquidTraits` resource inserted in
  `core/sim/runtime.rs` `install_block_runtime_from`).
* Configure path (live): `MindInput.configure_building` (`gdext/input/mod.rs`) → either a
  direct `SimCommand::Configure{value: wire}` (`"string"`, `"int"`) or
  `RemoteAction::Configure{world value}` → `ActionBatcher` → `to_sim_config`
  (`core/input/command_emit.rs:248`) → wire → `SimHost.pending_commands` → `Sim::command`
  (`core/sim/mod.rs`, arm `SimCommand::Configure`) → `SimRuntime::resolve_config`
  (`core/sim/runtime.rs:212`) → `configure_power_node` or `world::config::configure`
  (`core/world/config.rs:131`) → `behavior.configured(...)`.
* Placement: `SimRuntime::can_place` (`core/sim/runtime.rs` ~362) → `world::build::valid_place`
  (`core/world/build.rs:144`) → `valid_place_at` (`:163`). Break: `valid_break` (`:109`).
  `BuildRules` resource (`core/world/limits.rs:18`) is the guard's rule copy; today only
  `default_team` and `env` are synced from the session (`core/game/runtime.rs` ~306-311).

### 1.2 Java class -> port -> status after round G

Status: **OK** (ported + test), **PARTIAL**, **BROKEN**, **MISSING**. "Task" = this plan.

| Java (`J/world/blocks/...`) | Blocks | Port | Status | Task |
|---|---|---|---|---|
| distribution/Conveyor, ArmoredConveyor | conveyor, titanium-, armored- | `ConveyorBehavior` | OK; `unitOn` (belts push units) MISSING | T11 |
| distribution/Junction, Router, Sorter, OverflowGate | junction, router, distributor, sorter, inverted-sorter, overflow/underflow gate | behaviors in `distribution/` | OK (sorter `isSame` 3-chain rule verified) | — |
| distribution/ItemBridge, BufferedItemBridge | bridge-conveyor, phase-conveyor | `ItemBridgeBehavior` | PARTIAL: tap-to-link MISSING (tap opens an item list); chain-on-place only through the player builder and missing Java's `!proximity.contains` guard; G03 saw a hang staging a link (T5 root-causes) | T5, T6 |
| liquid/LiquidBridge | bridge-conduit, phase-conduit | `LiquidBridgeBehavior` (`core/world/blocks/liquid/behavior.rs:376`) | BROKEN: no `config`/`configured`/`link_valid_between`/`write`/`read` → can never be linked, links not saved; no chain-on-place (G02-era commit `b1056f77` wrongly claims Java has none) | T6 |
| distribution/MassDriver, payloads/PayloadMassDriver | mass-driver, payload-mass-driver, large-payload-mass-driver | `MassDriverBehavior`, `PayloadMassDriverBehavior` | PARTIAL: tap-to-link MISSING; shooting UNVERIFIED | T6, T14 |
| power/PowerNode, LongPowerNode | power-node, power-node-large, surge-tower, beam-link | `PowerNodeBehavior` + `Sim::configure_power_node` | OK knobs/Configure (G02 T2); tap-to-link MISSING; self-tap autolink needs a `Point2[]` config that the wire cannot carry | T5, T6 |
| power/PowerDiode | diode | `DiodeBehavior` | OK (G02 T2) | — |
| power/BeamNode | beam-node, beam-tower | `NoopBehavior` | MISSING | T15 |
| storage/CoreBlock | core-shard/foundation/nucleus (+Erekir) | `CoreBehavior` (`storage/core_block.rs`) | BROKEN: campaign cores never incinerate (hooks answer `core_incinerates=false`); `canPlaceOn`/`canReplace`/`placeBegan` upgrade and `canBreak` MISSING | T1, T4 |
| storage/StorageBlock | container, vault | `StorageBehavior` | OK (core link/merge ported) | — |
| storage/Unloader | unloader | `UnloaderBehavior` | OK; reads `allow_core_unloaders` from the same broken hooks | T1 |
| production/Drill | mechanical/pneumatic/laser/blast drill | `DrillBehavior` | PARTIAL: `liquidBoostIntensity` hard-coded 1.6 (blast drill 1.8) and `warmupSpeed` hard-coded 0.015 (blast drill 0.01); warmup ignores `delta` | T8 |
| production/BurstDrill | impact-drill | `DrillBehavior` burst path | PARTIAL: boost 1.75 not applied | T8 |
| production/GenericCrafter, Separator, Pump, SolidPump, Incinerator | Serpulo crafters, pumps, extractors, incinerators | behaviors in `behavior/production.rs` | OK (G02 T1/T5) ; separator UNVERIFIED | T14 |
| power/ConsumeGenerator, SolarGenerator, ThermalGenerator, reactors | generators | `power/behavior.rs` | OK + IO (G04 14b9da7); thermal UNVERIFIED | T14 |
| power/Battery | battery, battery-large | graph | OK | — |
| liquid/Conduit, LiquidRouter | conduits, routers, tanks | `LiquidConduitBehavior`, `LiquidRouterBehavior` | BROKEN: no "current liquid" in `LiquidModule` (`current_amount` is the total), so the one-liquid rule and the mixing reactions cannot exist | T10 |
| defense/Wall | walls | `WallBehavior` | PARTIAL: surge lightning, phase deflection MISSING | T12 |
| defense/Door | door, door-large | `DoorBehavior` | OK sim-side (Configure reaches it — see T5) | T5 (test only) |
| defense/MendProjector | mender, mend-projector | `MendProjectorBehavior` | OK (G02 T4) | — |
| defense/OverdriveProjector | overdrive-projector, -dome | `OverdriveProjectorBehavior` | PARTIAL: `Block.canOverdrive` not ported; `applyBoost` duration rule differs | T9 |
| defense/ForceProjector | force-projector | `ForceProjectorBehavior` (`defense/behaviors.rs:57`) | BROKEN live: ~~efficiency stays 0 in-engine with power (PAR-0129, undiagnosed)~~ not reproduced 2026-10-14 (efficiency 1.0 + dome drawn with an adjacent power source, see T7); live bullets never absorbed; phase boost + coolant consumers missing from the spec; update order wrong; IO OK | T7 |
| defense/ShockMine | shock-mine | `ShockMineBehavior` | NOT WIRED (`shock_mine_unit_on` only called by tests) | T11 |
| units/RepairTurret | repair-point, repair-turret | `RepairTurretBehavior` | OK heal (G02 T12); save body `f rotation` (version 1) MISSING | T13 |
| units/UnitAssembler, UnitCargoLoader, UnitCargoUnloadPoint | Erekir | behaviors | save bodies MISSING | T13 |
| world/Build.java validPlace / validBreak | all | `core/world/build.rs` | BROKEN: team/derelict/deep water/shallows/same-block rotation/floor/visibility/core radius/placeRangeCheck/bounds missing; `valid_break` ignores team; `kind_rotates` wrong | T2, T3 |

### 1.3 Done / verified in round G — do not redo

Spot-checked against the code on 2026-10-14 (all true):
* G02 T1 (PAR-0177): trait defaults `accept_item`/`accept_liquid`/`can_unload`/`accept_stack`
  follow `BuildingComp` (`core/world/behavior/mod.rs:300-388`); coolant filter via
  `LiquidTraits` (`helpers.rs:106`), resource inserted at `core/sim/runtime.rs:1092`.
* G02 T2 (PAR-0178): node knobs no longer overwritten (`runtime.rs:137-148` only inserts the
  default when missing); `DiodeBehavior` registered (`power/behavior.rs:1356`).
* G02 T5 (PAR-0179): `IncineratorBehavior::accept_item` present (`behavior/production.rs` ~1527).
* G02 T6 (PAR-0180): drill time uses hardness (`behavior/production.rs` ~467).
* G02 T4 (PAR-0181): projector scans are same-team (`defense/projectors.rs buildings_in_range`).
* G02 T12 (PAR-0182): `update_repair_turret` heals (`blocks/units/mod.rs:575`).
* G02 T7 (PAR-0165): `place_block` reads the tile back (`gdext/sim_host.rs` `tile_holds_block`).
* G04 (PAR-0161): per-kind save bodies for projectors, generators, reactors, heat producers,
  radar, stack conveyor, beam drill, shield wall, base shield (`core/world/building_io_kinds_tests.rs`).
* G07: `GeneratorEnv` is computed from rules (`core/game/runtime.rs` ~1109) — G02 T8 step 8 is done.

False / vacuous claims found (fix inside the named task):
* `b1056f77` (`core/sim/builder.rs chain_bridge` doc): "Java's `LiquidBridge` has no
  `playerPlaced`/`findLink`" — false; `LiquidBridge extends ItemBridge`
  (`J/world/blocks/liquid/LiquidBridge.java:7,19`) and inherits `playerPlaced`
  (`ItemBridge.java:224-233`). → T6.
* G01 T14 test `force_projector_absorbs_explosion` (`core/sim/combat_tests.rs:3744`) reads
  `BaseShieldState` from a force projector, gets radius 0 and returns early with an
  `eprintln!` — it never asserts anything. → T7.
* G03 T8 / PAR-0190 "Configure may not reach `configured()`": the code path reaches
  `configured()` (G03's own run printed the configured item via `building_config_json`; G04's
  in-engine sorter check persisted it). What really breaks: (a) relative/array/content world
  values are mangled or rejected by `to_sim_config`; (b) the block render pass may not redraw on
  a config change (H03). → T5.

---

## 2. Tasks

One task = one commit. Run after every task: `cargo test -p mind-core --lib <filter>` while
working, the full suite with `--features msav-import` before committing, `mind-headless --lib`,
`mind-gdext --lib`, clippy, fmt, `parity goldens`, build, `tools/session-play.sh N --import`
(only when the task has an in-engine step), the task's recipe, and
`grep -c panic $(ls -t $U/logs/* | head -1)` = 0.

In-engine shorthand (from H-environment §4): `G`, `H` (SimHost), `I` (Input), `C` (Camera2D),
`MC` (MindCampaign). Enter Ground Zero with the §4.1 "Enter like the UI Go button" recipe.
Free ground east of the core (re-mapped in-engine 2026-10-14, session 3): tiles
**(132..147, 57..72)** are all `air` on a fresh import (the shard covers 128..130 x 54..56,
(137,56) is a boulder). Everything at **x >= 150 for y 55..80 is a static wall**
(`ice-wall`/`snow-wall`/`dune-wall`), and the sector's `limit_area` is x 90..160, y 20..80
(nothing at y > 80 is placeable). Rows 73..78 are free up to x 144..147. Confirm a tile is
free with `$H.tile_debug(x,y).name == "air"` before placing.
Camera for screenshots: with a live player the camera re-follows the player every frame, so
`center_on_tile` alone does nothing. Use `$C.set_process(false); $C.center_on_tile(x,y)`
(re-enable with `$C.set_process(true)` before any player-relative click work).
`apply_sim_command_json` place/break with `"player":0` is applied at the NEXT tick: when
paused, `step(1)` before reading `player_build_plans_json()`.

---

### T1. Campaign cores incinerate overflow; core rule hooks read the session rules

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Ground Zero, core seeded to 4000 copper (= shard cap): a west-facing `conveyor` at (131,55) fed by a copper `item-source` at (132,55) jams with `building_items_json(131,55)` = `{"copper":3}` for 600+ ticks, core stays 4000, and switching the source to lead never reaches the core; `get_rules_json().coreIncinerates` is already `true` (the hooks ignore it). FIXED in place: the old recipe (item source directly against the core) cannot show a backlog (sources have no buffer). Evidence `runs/20261014-s3-verify/H02-T1-core-full-belt.png`.

**Symptom / acceptance.** In a campaign sector a full core rejects items, so belts into the
core back up and drills stall. Java: campaign always sets `coreIncinerates = true`, the core
accepts everything and burns what does not fit. After: a core holding `storage_capacity` copper
keeps accepting copper from a belt (the extra is destroyed) and `SectorInfo.handleCoreItem`
still counts it; `rules.allowCoreUnloaders = false` blocks core unloaders.

**Java.**
* `J/core/Logic.java:104-108` (WorldLoadEvent) and `:70-73` (SaveLoadEvent):
  `if(state.isCampaign()){ ... state.rules.coreIncinerates = true; ...`
* `J/game/Rules.java:225`: `public boolean coreIncinerates = true;` (default true everywhere);
  `:135`: `public boolean allowCoreUnloaders = true;`
* `J/world/blocks/storage/CoreBlock.java:744-751`:
  ```java
  public boolean acceptItem(Building source, Item item){
      return state.rules.coreIncinerates || items.get(item) < getMaximumAccepted(item);
  }
  public int getMaximumAccepted(Item item){
      return state.rules.coreIncinerates ? Integer.MAX_VALUE/2 : storageCapacity;
  }
  ```
* `CoreBlock.java:886-913` `handleItem`:
  ```java
  boolean incinerate = incinerateNonBuildable && !item.buildable;
  if(team == state.rules.defaultTeam){ state.stats.coreItemCount.increment(item); }
  if(net.server() || !net.active()){
      if(team == state.rules.defaultTeam && state.isCampaign() && !incinerate){
          state.rules.sector.info.handleCoreItem(item, 1);
      }
      if(items.get(item) >= storageCapacity || incinerate){
          if(!noEffect){ incinerateEffect(this, source); }
          noEffect = false;
      }else{
          super.handleItem(source, item);
      }
  }
  ```
  Note the order: stats and `handleCoreItem` count the item **before** the full check, so an
  incinerated (but buildable) item still counts.

**Port today.**
* `core/world/blocks/storage/core_block.rs:31-55` trait `CoreCampaignHooks` with defaults
  `core_incinerates() -> false`, `allow_core_unloaders() -> true`.
* `core/game/runtime.rs:85-105` `impl CoreCampaignHooks for CampaignCoreHooks` overrides only
  `is_campaign`, `default_team`, `handle_core_item` → `core_incinerates()` is always `false`.
* `core/game/runtime.rs:66-75` `CampaignHookQueues { is_campaign, default_team, core_deltas, production_deltas }`,
  refreshed each campaign tick at `core/game/runtime.rs` ~444
  (`state.is_campaign = runtime.session.is_campaign(); state.default_team = ...`).
* `CoreBehavior::handle_item` (`core_block.rs:249-286`) returns early when full, **before**
  `add_core_item`/`handle_core_item` (Java counts first).
* `Rules` fields exist: `core_incinerates` (`core/io/json/rules.rs:672`, default true at
  `:815`), `allow_core_unloaders` (`:582`, default true).
* Grep: `grep -n "fn core_incinerates\|fn allow_core_unloaders" client/rust/mind-core/src -r`.

**Changes.**
1. `core/game/runtime.rs` `CampaignHookQueues`: add `pub core_incinerates: bool` and
   `pub allow_core_unloaders: bool`. `Default`: derive gives `false` — implement `Default`
   by hand so both start at the Java defaults (`core_incinerates: true`,
   `allow_core_unloaders: true`), or keep derive and set them on install (step 3).
2. Same file, the refresh block (~444): also set
   `state.core_incinerates = runtime.session.rules.core_incinerates;`
   `state.allow_core_unloaders = runtime.session.rules.allow_core_unloaders;`
   And wherever the session is (re)loaded and `state.is_campaign` is first set (grep
   `state.is_campaign =` — `:135` resets to false on clear; leave that, but also reset
   `core_incinerates = true` there).
3. `impl CoreCampaignHooks for CampaignCoreHooks`: add
   ```rust
   fn core_incinerates(&self) -> bool {
       self.queues.lock().map(|q| q.core_incinerates).unwrap_or(true)
   }
   fn allow_core_unloaders(&self) -> bool {
       self.queues.lock().map(|q| q.allow_core_unloaders).unwrap_or(true)
   }
   ```
4. Campaign load: Java forces `coreIncinerates = true` for campaign sessions. Find where the
   port applies campaign load overrides (`grep -n "infinite_resources = false\|allow_edit_rules = false" client/rust/mind-core/src/game/*.rs`;
   PAR-0196 added "campaign load overrides"). If that block exists, add
   `rules.core_incinerates = true;` next to it with a `Logic.java:106` citation. If it does
   not exist, stop this step and record it (do not invent a new load hook).
5. `CoreBehavior::handle_item` (`core_block.rs`): reorder to Java: (a) default-team
   `stats.add_core_item` (existing block), (b) `if team == h.default_team() && h.is_campaign()`
   → `h.handle_core_item(item, 1)`, then (c) the full check (`current >= cap` → effect +
   `no_effect = false`, return) else add. `incinerateNonBuildable` (Erekir) is not modelled:
   leave a `// incinerateNonBuildable: Erekir, not modelled` comment.
6. Non-campaign sims (custom games, tests) have no `CampaignCoreHooks` → `NoopCoreCampaignHooks`
   answers `core_incinerates() = false`, which contradicts Java's default `true`. Change the
   trait default in `core_block.rs:33` to `true` **only if** the existing test
   `core_capacity_unifies_adjacent_storage` (`core_block.rs` ~393, asserts "Deposit above
   capacity is rejected when not incinerating") keeps a way to test the non-incinerating
   branch: give that test an explicit hooks impl returning `false` (copy the `Incinerating`
   pattern at `core_block.rs` ~437-449, name it `NotIncinerating`).

**Tests** (`core/world/blocks/storage/core_block.rs` tests module, harness style used there):
* `campaign_hooks_report_core_incinerates`: build a `CampaignHookQueues` via
  `install_campaign_core_hooks(&mut world)`, set `core_incinerates = true` in the lock;
  `CoreHooks` resource's `core_incinerates()` returns `true`; set false → false.
* `full_core_incinerates_and_counts` (`core/sim/production_tests.rs`, `runtime_sim()`):
  place `core-shard` at (10,10); fill copper to the core's `storage_capacity` (read
  `CoreBuild.storage_capacity`, expected 4000 = `itemCapacity`, `Blocks.java:3152`);
  `transfer::dispatch_accept_item(world, core, core, copper)` is `true` (default hooks now
  incinerate); `dispatch_handle_item` once; copper stays exactly 4000.
* `core_rejects_when_not_incinerating`: same with a hooks resource returning `false` →
  `accept_item` false.

**Verify in-engine.** Ground Zero (Go recipe), then:
```bash
$G --eval "return JSON.parse_string($MC.get_rules_json()).coreIncinerates"     # true
$G --eval "$H.debug_add_core_items(\"copper\", 4000); return $H.core_items_json()"
$G --eval "return $H.tile_debug(131,55).name" --eval "return $H.tile_debug(132,55).name"   # both "air"
$G --eval "return $H.place_block_rot(131,55,\"conveyor\",2)" \
   --eval "return $H.place_block(132,55,\"item-source\")" \
   --eval "return $I.configure_building(132,55,\"content\",\"copper\")"
```
(the shard covers 128..130, so the west-facing belt at (131,55) feeds its east edge; the item
source at (132,55) feeds the belt). An item source placed directly against the core proves
nothing: it has no buffer, so `building_items_json` is `{}` whether the core accepts or not.
Step 300 (`$H.set_paused(true)`, `$H.step(300)`), then `core_items_json()` copper is still 4000
and `building_items_json(131,55)` is `{}` or 1 item (flowing). **Before the fix (verified
2026-10-14)** the belt jams: `building_items_json(131,55)` = `{"copper":3}` and stays 3 after
another 300 ticks, and re-configuring the source to `lead` never reaches the core (core lead
unchanged). Record both JSONs. Panic count 0.

**Pitfalls.** The hooks resource is shared through an `Arc<Mutex<..>>`; never hold the lock
while calling into the world. `unwrap_or(true)` on a poisoned lock matches the Java default.
Do not touch `handle_stack` semantics beyond the order fix (G08 owns `acceptStack` paths).

**Commit.** `Gameplay H02 T1: campaign cores incinerate overflow; core hooks read rules`

---

### T2. `BuildRules` sync from `Rules` + `Build.validBreak` (team, derelict, cores)

> **In-engine verification (2026-10-14, session 3):** PARTLY. Last-core break CONFIRMED and destructive: `{"op":"break","x":129,"y":55,"player":0}` + `step(1)` → break plan progress 0.9998, tile `build3`, `hasCore` false, "Sector Lost" autosaved (needed a clean re-import) — `runs/20261014-s3-verify/H02-T2-core-broken.png`. Enemy-building break NOT reproduced as a visible symptom: a team-2 `copper-wall` break plan is dropped by `update_builders`' interactable check (`core/sim/builder.rs` ~675-706), so only the `valid_break` code gap remains (other callers). Derelict scrap wall (169,167, team 0): break plan queued (Java-correct). FIXED in place: recipe warning + `step(1)` note + staging enemy walls on free ground.

**Symptom / acceptance.** The player can deconstruct enemy buildings; the last core can be
deconstructed; sector bans/whitelists, derelict repair, place-range checks and build radius
never reach the placement guard. After: `world::limits::sync_build_rules(world, &rules)` is the
single entry point that copies every placement-relevant rule into `BuildRules`; `valid_break`
rejects other teams' buildings (derelict allowed) and cores per `CoreBlock.canBreak`.

**Java.**
* `J/world/Build.java:317-320`:
  ```java
  public static boolean validBreak(Team team, int x, int y){
      Tile tile = world.tile(x, y);
      return tile != null && tile.block() != Blocks.air && (tile.block().canBreak(tile) && (tile.breakable() || state.rules.allowEnvironmentDeconstruct)) && tile.interactable(team);
  }
  ```
* `J/world/Tile.java:576-578` `interactable(team)` → `state.teams.canInteract(team, team())`;
  `J/game/Teams.java:125-127`: `return team == other || other == Team.derelict;`
  (`Tile.team()` of a tile without a building is derelict).
* `J/world/blocks/storage/CoreBlock.java:173-177`:
  ```java
  public boolean canBreak(Tile tile){
      //always keep at least 1 core to not lose the save
      return state.isEditor() || (state.rules.coreBuildAndConfig && tile.block() instanceof CoreBlock && state.teams.cores(tile.team()).size > 1);
  }
  ```
* `J/game/Rules.java:294-296`:
  `buildRadius(team) = !teams.get(team).protectCores ? 0f : enemyCoreBuildRadius + teams.get(team).extraCoreBuildRadius;`
* `Rules.isBanned(block)` = `blockWhitelist != bannedBlocks.contains(block)` (search
  `public boolean isBanned` in `J/game/Rules.java`; confirm before writing).

**Port today.**
* `core/world/limits.rs:18-48` `BuildRules` fields: `cheat, infinite_resources, instant_build,
  deconstruct_refund_multiplier, allow_environment_deconstruct, place_range_check,
  allow_derelict_repair, build_cost_multiplier, enemy_core_build_radius,
  polygon_core_protection, default_team, banned_blocks, block_limits, env`;
  `is_banned` = `banned_blocks.contains` (no whitelist).
* Only `core/game/runtime.rs` ~306-311 writes it (`default_team`, `env`).
  `Sim::apply_rules_to_world` (`core/sim/mod.rs:1584`) has the comment
  "G02 T8's `sync_build_rules(&mut World, &Rules)`; call it here once it exists".
* `core/world/build.rs:83 can_break` returns `true`; `valid_break` (`:109-140`) ignores team.
* `Rules` type: `crate::game::rules::Rules` (re-export of `crate::io::json::rules::Rules`,
  struct at `core/io/json/rules.rs:470`). Fields (verified): `banned_blocks: IndexSet<String>`
  (`:636`), `revealed_blocks: IndexSet<String>` (`:640`), `block_whitelist` (`:594`),
  `hide_banned_blocks` (`:588`), `editor` (`:500`), `infinite_resources` (`:474`),
  `core_build_and_config` (`:476`), `derelict_repair` (`:502`), `place_range_check` (`:576`),
  `polygon_core_protection` (`:574`), `enemy_core_build_radius` (`:572`),
  `build_cost_multiplier` (`:564`), `deconstruct_refund_multiplier` (`:568`),
  `allow_environment_deconstruct` (`:590`), `instant_build` (`:592`), `waves` (`:484`),
  `wave_team` (`:664`), `fog` (`:648`), `static_fog` (`:650`), `lighting` (`:656`),
  `teams: TeamRules` (`:478`; `TeamRules::get(team) -> Option<&TeamRule>`, `TeamRule`
  fields `protect_cores`, `check_placement`, `extra_core_build_radius`, `cheat`,
  `infinite_resources` at `:30-115`).
  Check `grep -n "pub block_limits\|pub allow_edit_world_processors" core/io/json/rules.rs`
  for the two fields not verified here.

**Changes.**
1. `core/world/limits.rs` `BuildRules`: add fields (with `Default` = Java defaults):
   `block_whitelist: bool` (false), `editor: bool` (false), `hide_banned_blocks: bool` (false),
   `revealed_blocks: Vec<BlockId>` (empty), `core_build_and_config: bool` (false),
   `waves: bool` (true), `wave_team: u8` (2), `fog: bool` (false), `static_fog: bool` (true),
   `lighting: bool` (false), `allow_edit_world_processors: bool` (false, only if the `Rules`
   field exists), `is_campaign: bool` (false),
   `team_rules: Vec<TeamPlaceRule>` (empty) with
   ```rust
   /// `Rules.TeamRule` placement subset (`Rules.java` TeamRule).
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub struct TeamPlaceRule { pub team: u8, pub protect_cores: bool, pub check_placement: bool, pub extra_core_build_radius: f32 }
   impl Default for TeamPlaceRule { fn default() -> Self { Self { team: 0, protect_cores: true, check_placement: true, extra_core_build_radius: 0.0 } } }
   ```
   and methods
   ```rust
   /// `Rules.isBanned` (`blockWhitelist != bannedBlocks.contains(block)`).
   pub fn is_banned(&self, block: BlockId) -> bool { self.block_whitelist != self.banned_blocks.contains(&block) }
   /// `rules.teams.get(team)` with upstream defaults for absent entries.
   pub fn team_rule(&self, team: u8) -> TeamPlaceRule {
       self.team_rules.iter().copied().find(|r| r.team == team).unwrap_or(TeamPlaceRule { team, ..TeamPlaceRule::default() })
   }
   /// `Rules.buildRadius(team)` (`Rules.java:294-296`).
   pub fn build_radius(&self, team: u8) -> f32 {
       let rule = self.team_rule(team);
       if !rule.protect_cores { 0.0 } else { self.enemy_core_build_radius + rule.extra_core_build_radius }
   }
   ```
   `is_banned` changes behavior for `block_whitelist = true` only; run every test that uses
   `banned_blocks` (`grep -rn banned_blocks client/rust/mind-core/src --include=*.rs`).
2. **The single sync entry point** (H04 and H06 call it by this exact name):
   ```rust
   /// Copies the placement/break rule subset of `Rules` into the `BuildRules`
   /// resource (`World.setRules` + every `Build.validPlace` read of `state.rules`).
   /// No-op when `BuildRules` is absent. Block names resolve through `ContentIndex`.
   pub fn sync_build_rules(world: &mut bevy_ecs::world::World, rules: &crate::game::rules::Rules, is_campaign: bool)
   ```
   (`is_campaign` is an explicit argument because `Rules` alone does not know it; callers pass
   `session.is_campaign()` — if `Rules` has a `sector` field that answers it, use that instead
   and drop the argument; check `grep -n "pub sector" core/io/json/rules.rs`.)
   Body: read `crate::sim::runtime::ContentIndex.block_ids` (a `HashMap<String,u16>` — only
   used for lookups, never iterated, so determinism is fine) to map `banned_blocks` and
   `revealed_blocks` names to `BlockId`s **in the `IndexSet` order** (skip unknown names);
   copy every scalar listed in step 1 plus `infinite_resources`, `instant_build`,
   `deconstruct_refund_multiplier`, `build_cost_multiplier`, `enemy_core_build_radius`,
   `polygon_core_protection`, `place_range_check`, `allow_derelict_repair` (from
   `derelict_repair`), `allow_environment_deconstruct`, `env`, `default_team`
   (`rules.default_team`); `team_rules` from `rules.teams.0` (an `IndexMap`, iterate in order).
   Do not touch `block_limits` unless `Rules` has `block_limits` (then map names → ids).
3. Call sites:
   (a) `core/game/runtime.rs` ~306-311: replace the two-field block with
   `crate::world::limits::sync_build_rules(world, &session.rules, session.is_campaign());`
   (keep the comment citing `World.setRules`). Watch the borrow: `session` may be borrowed from
   a resource — clone the `Rules` first if the borrow checker complains.
   (b) `core/sim/mod.rs:1584 apply_rules_to_world`: replace the "G02 T8" comment with the
   call. `apply_rules_to_world` receives `&Rules` only: get `is_campaign` from
   `self.ecs.0.get_resource::<crate::game::runtime::CampaignRuntime>().map(|r| r.session.is_campaign())`
   (check the method name with grep) defaulting to `false`.
4. `core/world/build.rs`:
   * `can_break(def: &BlockDef) -> bool` becomes
     `can_break(def: &BlockDef, rules: &BuildRules, cores_of_team: usize) -> bool`:
     `if def.kind == BlockKind::CoreBlock { return rules.editor || (rules.core_build_and_config && cores_of_team > 1); } true`.
   * `valid_break` gains a `world: Option<&bevy_ecs::world::World>` parameter (put it right
     after `grid`). With `Some(world)`: building team = `world.get::<TeamComp>(entity).map(|t| t.team)`;
     reject when `team_of_tile != team && team_of_tile != 0` (tiles without a building are
     derelict → allowed); `cores_of_team` = count of entities with `CoreBuild` + `TeamComp.team
     == tile_team` (query `world.iter_entities()` filtered by `contains::<CoreBuild>()` — only
     a count, order irrelevant). With `None` (headless harness/scenarios): today's behavior,
     `cores_of_team = usize::MAX`.
   * Update every caller (verified list): `core/sim/builder.rs:1144`, `core/sim/runtime.rs:562`
     and `:620`, `core/ai/types/builder.rs:119` (pass `Some(ctx.world)`),
     `core/world/harness.rs:301` (None), `gdext/sim_host.rs:4813` (`Some(&self.sim.ecs.0)`),
     `headless/input_scenarios.rs:394` (None). `grep -rn "valid_break(" client/rust --include=*.rs`
     must show no stale call.

**Tests** (`core/sim/production_tests.rs` unless noted):
* `sync_build_rules_copies_rules`: `runtime_sim()`; build a `Rules::default()`, set
  `banned_blocks = ["copper-wall"]`, `block_whitelist = false`, `derelict_repair = false`,
  `place_range_check = true`, `enemy_core_build_radius = 320.0`, team 2 rule
  `extra_core_build_radius = 40.0`; call `sync_build_rules(&mut sim.ecs.0, &rules, true)`;
  assert `BuildRules.is_banned(copper-wall id)`, `!allow_derelict_repair`, `place_range_check`,
  `build_radius(2) == 360.0`, `build_radius(1) == 320.0`, `is_campaign`. Then
  `block_whitelist = true` → `is_banned(copper-wall)` false, `is_banned(titanium-wall)` true.
* `cannot_break_enemy_building`: wall team 2 at (5,5) (`SimCommand::Place{team:2}`),
  `valid_break(content, grid, Some(world), &rules, 1, 5, 5)` false; wall team 0 at (7,5) → true;
  own wall team 1 at (9,5) → true.
* `last_core_unbreakable`: core-shard team 1 at (10,10): `valid_break(.., 1, 10, 10)` false;
  with `core_build_and_config = true` and a second shard at (20,20): true.

**Verify in-engine.** Ground Zero (Go):
```bash
$G --eval "return $H.apply_sim_command_json('{\"op\":\"break\",\"x\":129,\"y\":55,\"player\":0}')" \
   --eval "return $H.player_build_plans_json()"
```
(when paused, `step(1)` between the two evals). Expect no deconstruct plan for the core (the
plan list stays without (129,55)). **Run this only AFTER the fix:** on current main the core
break plan is accepted and the shard becomes `build3` within one tick, `hasCore` goes false,
"Sector Lost" shows and the loss is autosaved — recover with the H-environment §4.3 clean
re-import. Then a derelict scrap wall: `tile_debug(169,167)` to confirm `scrap-wall`
(`building_team_at(169,167)` = 0), `apply_sim_command_json('{"op":"break","x":169,"y":167,"player":0}')`
→ a break plan appears (true today too). Enemy building: stage a team-2 wall on free ground
with `apply_sim_command_json('{"op":"place","x":140,"y":66,"block":<copper-wall id>,"team":2}')`
(`building_team_at` → 2), then a `"player":0` break → no plan (today the plan is also dropped,
by the builder's interactable check in `core/sim/builder.rs` `update_builders` ~675-706, so
this half is a regression check, not a visible symptom).

**Pitfalls.** `ContentIndex` lives in `sim::runtime`; `core/world` must not depend on
`sim` if that creates a cycle — it is the same crate, so a `use crate::sim::runtime::ContentIndex`
is fine. Never iterate the `HashMap`. Keep `BuildRules::default()` matching Java defaults so
tests without a session behave as before.

**Commit.** `Gameplay H02 T2: sync_build_rules, validBreak team and core rules`

---

### T3. `Build.validPlace` completion + `kind_rotates` + player-preview seam

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (enemy replacement). A team-2 `conveyor` staged at (140,66) via `apply_sim_command_json` `"team":2` (works: the pre-check uses the default team but the tile is free) was replaced by a `"player":0` `titanium-conveyor` plan: built, `building_team_at(140,66)` = 1. Same-block/same-rotation conveyor and router-over-router places are accepted by the pre-check (true) but the plans are dropped as "done" by `validate_plans` (no visible damage). `HostWorld::valid_place` = bounds only (`gdext/input/desktop.rs:172`) confirmed. Deep water / enemy core radius: NOT CHECKABLE on Ground Zero (no water in the playable area; x>=150 is rock, y>80 outside `limit_area`) — FIXED the recipe text to headless-only. Free-ground facts in §2 intro FIXED (old "140..158, 55..80" is half static walls).

Depends on T2 (BuildRules fields).

**Symptom / acceptance.** Today the player can replace enemy blocks, build on deep water,
build next to enemy cores, replace a block with the identical block (losing contents), place
banned/hidden blocks through plans, and the placement ghost is always green. After: each case
is rejected exactly as Java; the ghost uses the same function.

**Java.** `J/world/Build.java:163-170` (overloads: `validPlace(type,team,x,y,rot)` =
`checkVisible = true, checkCoreRadius = true`), `:173-175` (`&& checkNoUnitOverlap`), and
`validPlaceIgnoreUnits` `:183-273` (pasted, order matters):
```java
if(type == null || (!state.rules.editor && (checkVisible && (!type.environmentBuildable() || (!type.isPlaceable() && !(state.rules.waves && team == state.rules.waveTeam && type.isVisible())))))){
    return false;
}
if(!state.rules.editor && checkCoreRadius && !(!checkVisible && team == Team.derelict)){
    if(state.rules.polygonCoreProtection){
        float mindst = Float.MAX_VALUE; CoreBuild closest = null;
        for(TeamData data : state.teams.active){
            if(!data.team.rules().protectCores) continue;
            for(CoreBuild tile : data.cores){
                float dst = tile.dst2(x * tilesize + type.offset, y * tilesize + type.offset);
                if(dst < mindst){ closest = tile; mindst = dst; }
            }
        }
        if(closest != null && closest.team != team) return false;
    }else if(state.teams.anyEnemyCoresWithinBuildRadius(team, x * tilesize + type.offset, y * tilesize + type.offset)){
        return false;
    }
}
Tile tile = world.tile(x, y);
if(tile == null) return false;
if(!type.canPlaceOn(tile, team, rotation)) return false;
if(type.isFloor()) return type.isOverlay() ? tile.overlay() != type : tile.floor() != type;
if(!type.ignoreBuildDarkness && world.getDarkness(x, y) >= 3) return false;
if(!type.requiresWater && !contactsShallows(tile.x, tile.y, type) && !type.placeableLiquid) return false;
if(type.isOverPlacementLimit(team)) return false;
int offsetx = -(type.size - 1) / 2; int offsety = -(type.size - 1) / 2;
for(int dx = 0; dx < type.size; dx++){ for(int dy = 0; dy < type.size; dy++){
    int wx = dx + offsetx + tile.x, wy = dy + offsety + tile.y;
    Tile check = world.tile(wx, wy);
    if(
    check == null ||
    (type.size == 2 && world.getDarkness(wx, wy) >= 3) ||
    (state.rules.staticFog && state.rules.fog && !fogControl.isDiscovered(team, wx, wy)) ||
    (check.floor().isDeep() && !type.floating && !type.requiresWater && !type.placeableLiquid) ||
    (!state.rules.derelictRepair && check.team() == Team.derelict && check.build != null) ||
    (type == check.block() && check.build != null && rotation == check.build.rotation && type.rotate && !((type == check.block && team != Team.derelict && check.team() == Team.derelict))) ||
    !check.interactable(team) ||
    !check.floor().placeableOn && !type.ignoreBuildDarkness ||
    (!checkVisible && checkCoreRadius && !check.block().alwaysReplace) ||
        !(((type.canReplace(check.block()) || (check.build != null && check.build.canBeReplaced(type)) || (type == check.block && team != Team.derelict && check.team() == Team.derelict)) ||
            (check.build instanceof ConstructBuild build && build.current == type && check.centerX() == tile.x && check.centerY() == tile.y)) &&
        type.bounds(tile.x, tile.y, Tmp.r1).grow(0.01f).contains(check.block.bounds(check.centerX(), check.centerY(), Tmp.r2))) ||
    (type.requiresWater && check.floor().liquidDrop != Liquids.water)
    ) return false;
}}
if(state.rules.placeRangeCheck && checkCoreRadius && !state.isEditor() && getEnemyOverlap(type, team, x, y) != null) return false;
return true;
```
* `Build.java:275-277` `getEnemyOverlap`:
  `indexer.findEnemyTile(team, x * tilesize + block.size, y * tilesize + block.size, block.placeOverlapRange + 4f, b -> b.team.rules().checkPlacement)`
  (`Block.placeOverlapRange` default 50 — confirm with `grep -n "placeOverlapRange" J/world/Block.java`).
* `Build.java:294-314` `contactsShallows`: multiblock → any `getInsideEdges()` or
  `getEdges()` tile (offset from x,y) whose floor is not deep → true; 1x1 → any d4 neighbour
  not deep → true, else the tile itself not deep. `Floor.isDeep()` = `drownTime > 0`.
* `J/game/Teams.java:60-71` `anyEnemyCoresWithinBuildRadius`: for each active team `!= team`
  with `protectCores`: any core `within(x, y, state.rules.buildRadius(core.team) + tilesize)`.
* `J/world/Block.java:1031-1037`:
  `isVisible() = !isHidden() && (rules.editor || (!rules.hideBannedBlocks || !isBanned()))`;
  `isPlaceable() = isVisible() && (!isBanned() || rules.editor) && supportsEnv(rules.env)`;
  `:1553-1555` `isHidden() = !buildVisibility.visible() && !rules.revealedBlocks.contains(this)`;
  `:1117-1119` `environmentBuildable() = isOnPlanet(state.getPlanet())` (treat as true: the
  port has no per-planet block list in the runtime — note it in the commit).
* `J/world/meta/BuildVisibility.java:10-21` (visible() per kind): hidden false; shown true;
  debugOnly false; editorOnly `rules.editor`; coreZoneOnly
  `indexer.isBlockPresent(Blocks.coreZone) || !state.isGame()`; worldProcessorOnly
  `rules.editor || rules.allowEditWorldProcessors`; sandboxOnly `rules.infiniteResources`;
  campaignOnly `state.isCampaign() || !state.isGame()`; legacyLaunchPadOnly /
  notLegacyLaunchPadOnly (H04 owns launch pads — treat as `true`/`!isCampaign` and note);
  lightingOnly `rules.lighting || isCampaign || rules.infiniteResources`; fogOnly
  `rules.fog || rules.editor`.
* `J/world/Block.java:795-800` `canReplace` (the port's `can_replace` matches; keep it).
* `rotate = true` classes (from `grep -rn "rotate = true\|rotate = false" J/world/blocks`):
  Thruster, Turret (all turrets), Conveyor (+Armored), DirectionalUnloader, DirectionBridge
  (DuctBridge, DirectionLiquidBridge), Duct, DuctRouter (+StackRouter), OverflowDuct,
  StackConveyor, HeatConductor, HeatProducer, Conduit (+ArmoredConduit), BlockProducer
  (Constructor), PayloadConveyor (+PayloadRouter), PayloadLoader (+PayloadUnloader),
  PayloadMassDriver, PayloadSource, HeaterGenerator, PowerDiode, BeamDrill, WallCrafter,
  UnitAssembler, UnitAssemblerModule, UnitBlock (UnitFactory, Reconstructor).
  Explicitly `rotate = false`: PayloadDeconstructor, PayloadVoid. `Block.rotate` defaults
  `false` (`J/world/Block.java:122`). Per-block override in `J/content/Blocks.java`:
  `electrolyzer` `rotate = true` (`:1362`), `cryofluidMixer` `rotate = false` (`:1177`).

**Port today.** `core/world/build.rs:163-310 valid_place_at`: checks placeable_player/in_editor,
`rules.is_banned`, limits, `supports_env_bits`, darkness (wall + border), footprint
`can_replace` with "air or same block → continue", pump/beam drill/wall crafter/solid pump
`canPlaceOn`. `contacts_shallows` (`:436`) and `get_enemy_overlap` (`:472`) are wrong stubs;
`can_place_on` (`:78`) is `true`. `kind_rotates` (`core/world/block.rs:520-571`) lists
Junction, ItemBridge, BufferedItemBridge, Sorter, Router, OverflowGate, MassDriver,
LiquidJunction, LiquidBridge, LiquidRouter, Drill, BurstDrill, Door, AutoDoor, PowerNode,
BeamNode, PayloadDeconstructor as rotating (Java: not) and misses UnitFactory, Reconstructor,
UnitAssembler, UnitAssemblerModule, BeamDrill, HeatProducer, HeatConductor, HeaterGenerator,
PayloadSource. `BlockInstance.rotate` (`core/world/block.rs:83`, set at `:143` from
`kind_rotates`). `SimRuntime::placement_allowed` (`core/sim/runtime.rs:191`) hard-rejects
Hidden/DebugOnly/EditorOnly/SandboxOnly when `config.placement_gate`.
Desktop ghost validity: `gdext/input/desktop.rs:172 fn valid_place` returns `in_bounds` only.
Callers of `valid_place`/`valid_place_at` (verified): `core/sim/runtime.rs:381`,
`core/ai/types/builder.rs:98,221,325`, `core/ai/base_builder_ai.rs:414`,
`core/game/schematics.rs:295`, `core/world/harness.rs:237`, `headless/units_scenarios.rs:552`,
`headless/input_scenarios.rs:324,381`, tests in `build.rs`.

**Changes.**
1. `core/world/build.rs`: add
   ```rust
   /// Flags of `Build.validPlace(type, team, x, y, rot, checkVisible, checkCoreRadius)`.
   #[derive(Debug, Clone, Copy)]
   pub struct PlaceFlags { pub check_visible: bool, pub check_core_radius: bool, pub check_units: bool }
   impl PlaceFlags {
       /// `Build.validPlace(type, team, x, y, rot)` (player, AI, schematics).
       pub const PLAYER: Self = Self { check_visible: true, check_core_radius: true, check_units: true };
       /// `validPlace(.., false, false)` (`Control.java:505,521`): dev/API placement.
       pub const API: Self = Self { check_visible: false, check_core_radius: false, check_units: false };
   }
   pub fn valid_place_query(content, table, rules, counter, grid, world: Option<&World>, block, team, rot, x, y, flags: PlaceFlags) -> bool
   ```
   `valid_place_at(..., check_units)` becomes a wrapper:
   `valid_place_query(.., None, .., PlaceFlags { check_visible: false, check_core_radius: false, check_units })`
   and `valid_place(..)` a wrapper with `None` + `PlaceFlags { check_visible: false, check_core_radius: false, check_units: true }`
   (so harness/headless callers without a world keep today's semantics plus the pure grid
   checks of step 2). Live callers migrate in step 6.
2. Port the transcribed order into `valid_place_query` (keep the existing env/limits/darkness
   and kind-specific `canPlaceOn` code; insert the new checks around them in Java order):
   a. Visibility (only `if !rules.editor && flags.check_visible`):
      `let placeable = build_visible(def, rules) && (!rules.is_banned(block) || rules.editor) && supports_env_bits(def, rules.env);`
      `let visible = build_visible(def, rules) && (rules.editor || !rules.hide_banned_blocks || !rules.is_banned(block));`
      reject when `!placeable && !(rules.waves && team == rules.wave_team && visible)`.
      `fn build_visible(def, rules) -> bool` = `visibility_visible(def.build_visibility, rules) || rules.revealed_blocks.contains(&def.id)`
      with `visibility_visible` per the BuildVisibility table (coreZoneOnly: `true` if any tile
      of `grid` has block `core-zone` — **check** the floor/overlay name with
      `grep -n '"core-zone"' core/content/registries/blocks/*.rs`; precompute nothing, a scan
      is acceptable only when `def.build_visibility == CoreZoneOnly`).
      Remove the old unconditional `is_banned`/`supports_env_bits` early returns only if they
      are now covered (`supports_env_bits` stays unconditional? — Java only checks env inside
      `isPlaceable`, i.e. only with `checkVisible`. Keep Java: env check only under
      `check_visible`). Keep `!def.placeable_player && !def.in_editor` as is (port-only flag).
   b. Core radius (`!rules.editor && flags.check_core_radius && !(!flags.check_visible && team == 0)`),
      needs `world`: collect `(team, x, y)` of every `CoreBuild` entity with `TeamComp` and
      `Pos`. `offset` = `def.offset` (check field: `grep -n "pub offset" core/content/registries/blocks/mod.rs`;
      Java `offset = (size + 1) % 2 * tilesize / 2f`, i.e. 4.0 for even sizes, 0 for odd —
      compute it inline if the field is absent). Polygon branch: closest core by squared
      distance among teams whose `rules.team_rule(t).protect_cores`; tie → first in
      `EntitySeq`/entity-index order (sort the collected list by `Entity::index()` before
      scanning). Else: reject if any other-team protecting core is within
      `rules.build_radius(core_team) + 8.0` world units of `(x*8+offset, y*8+offset)`.
   c. `can_place_on` stays `true` here (T4 adds the core rule).
   d. Darkness (existing), then `contacts_shallows`: rewrite per Java using
      `crate::world::edges::inside_edges(size)` / `edges(size)` (`core/world/edges.rs:89,98`,
      return `&'static [TilePos]` offsets) and `content.block(floor).is_some_and(|f| f.is_deep)`
      (`BlockDef.is_deep`, `content/registries/blocks/mod.rs:2188`). Reject when
      `!requires_water && !contacts_shallows && !def.placeable_liquid`. `requires_water` is not a
      `BlockDef` field: treat as `false` and leave a comment (Java: only `Pump`-family
      `requiresWater` for some Erekir blocks — `grep -n "requiresWater = true" J/content/Blocks.java`;
      if any Serpulo block sets it, add the field instead).
   e. Footprint loop (replace the current loop body; delete the "same block always placeable"
      shortcut): for each check tile in bounds (out of bounds → false):
      * `size == 2 && darkness(wx,wy) >= 3` → false;
      * fog: skip (not modelled; comment `staticFog && fog && !isDiscovered`: H04/H03);
      * `check_floor.is_deep && !def.floating && !def.placeable_liquid` → false;
      * `check_team` = `tile.build.and_then(|e| world?.get::<TeamComp>(e)).map(|t| t.team).unwrap_or(0)`;
        when `world` is `None` treat `check_team = team` (cannot know);
      * `!rules.allow_derelict_repair && check_team == 0 && tile.build.is_some()` → false;
      * same block: `tile.block == block && tile.build.is_some() && rot == building.rotation
        && inst.rotate && !(team != 0 && check_team == 0)` → false (`inst.rotate` = the
        `BlockInstance.rotate` from the `BlockTable`, so per-block overrides apply);
      * `!(check_team == team || check_team == 0)` → false (interactable);
      * `!check_floor.placeable_on` → false (no `ignoreBuildDarkness` field: unconditional);
      * `!flags.check_visible && flags.check_core_radius && !current.always_replace` → false;
      * replaceability: ok when `tile.block == AIR` **or** `can_replace(def, current)` **or**
        `(tile.block == block && team != 0 && check_team == 0)` **or** (construct site:
        `current.kind == ConstructBlock` and its `ConstructState.current == block` and the
        construct's centre tile == (x,y) — read `crate::world::ConstructState` field names with
        grep); `check.build.canBeReplaced(type)` is the ConstructBuild case in Java
        (`ConstructBlock.java` — `grep -n canBeReplaced J/world/blocks/ConstructBlock.java`);
        AND bounds: the replaced building's footprint (centre `building.tile`, size
        `current.size`) lies inside the new footprint. Otherwise → false.
   f. After the loop: `rules.place_range_check && flags.check_core_radius && !rules.editor`
      and `get_enemy_overlap` finds something → false. Rewrite `get_enemy_overlap` to take
      `world: &World` and return `Option<Entity>`: any building entity whose team `!= team`,
      whose team's `team_rule(t).check_placement` is true and whose position `Pos` is within
      `place_overlap_range + 4.0 + hit_size/2` of `(x*8 + size, y*8 + size)` (Java
      `findEnemyTile` uses `within(x, y, range + b.hitSize()/2f)` — confirm in
      `J/ai/BlockIndexer.java findEnemyTile`). Iterate with `world.iter_entities()` but pick
      the first by ascending `Entity::index()` (result only used as a boolean; still sort to
      stay deterministic). `place_overlap_range`: use `def.place_overlap_range` if the field
      exists, else `50.0` (`Block.java` default) and comment.
3. `kind_rotates` (`core/world/block.rs:520`): set exactly the Java list above (map each
   class to its `BlockKind`; check names with `grep -n "pub enum BlockKind" -A200 core/content/mod.rs`
   or wherever `BlockKind` is defined). Then in `BlockInstance::new` (`block.rs:143`) apply
   the per-block override: `rotate: match def.name.as_str() { "electrolyzer" => true, "cryofluid-mixer" => false, _ => kind_rotates(def.kind) }`.
   Callers that use `kind_rotates(def.kind)` directly (`core/sim/builder.rs:285`,
   `core/editor/mod.rs:121`, `core/world/build.rs:100`, `core/ui/campaign.rs:1188`) should read
   the instance flag where a `BlockTable` is at hand; where not, leave them (note in commit).
4. `core/sim/runtime.rs can_place(...)`: add `flags: PlaceFlags` param and call
   `valid_place_query(.., Some(world), .., flags)`. Callers:
   * `SimRuntime::place` (API `SimCommand::Place{player: None}` and `place_for_team` for AI):
     give `place` a `flags` param; `Sim::command` Place arm with `player: None` passes
     `PlaceFlags::API`; `Sim::place_for_team` (AI base placements, `core/sim/mod.rs:827`)
     passes `PlaceFlags::PLAYER` (Java `BaseBuilderAI.java:211` uses the 5-arg form).
   * `begin_place` (builder construct start) → `PlaceFlags::PLAYER`.
   * `Sim::can_place_for_team` (`core/sim/mod.rs:427`) → `PlaceFlags::API` (it backs the API
     readback). Add a sibling
     ```rust
     /// `Build.validPlace(type, player.team(), x, y, rot)` for the placement ghost.
     pub fn valid_place_player(&self, x: i16, y: i16, block: BlockId, rot: u8) -> bool
     ```
     using `self.live_default_team()` and `PlaceFlags::PLAYER`.
5. `gdext/sim_host.rs`: add
   ```rust
   /// Placement ghost validity (`Build.validPlace`, `InputHandler.java:1455`); H08 consumes it.
   #[func]
   pub fn valid_place_preview(&self, x: i32, y: i32, block: GString, rotation: i32) -> bool
   ```
   resolving the name via the content registry; out-of-range ints → false. Also fix
   `apply_sim_command_json` "place": the pre-check uses `live_default_team()`; use the JSON
   `"team"` when present (`value.get("team")`), so enemy staging works.
   Do not change `gdext/input/desktop.rs:172` (H08 owns the ghost; it calls the new seam).
6. AI/schematic callers: `core/ai/types/builder.rs` (`valid_place` helper at `:85`): switch to
   `valid_place_query(.., Some(ctx.world), .., PlaceFlags::PLAYER)`.
   `core/game/schematics.rs:295 valid_place_ctx`: `Some(ctx.ecs)`, `PlaceFlags::PLAYER`
   (Java `Schematics.java:524` 5-arg form). `core/ai/base_builder_ai.rs:414`: check whether
   `input` carries a `&World` (`grep -n "pub struct" -A20 core/ai/base_builder_ai.rs`); if yes
   use `Some(world)` + `PLAYER`, else leave the wrapper and note it in §6 for H01.

**Tests** (`core/world/build.rs` tests need a world → put them in `core/sim/production_tests.rs`):
* `cannot_replace_enemy_conveyor`: conveyor team 2 at (5,5); `sim.can_place_for_team`-style
  call with `PlaceFlags::PLAYER` for `titanium-conveyor` team 1 → false; own conveyor team 1 at
  (7,5) → true (upgrade).
* `cannot_build_on_deep_water`: `set_floor` `deep-water` (name check:
  `grep -n '"deep-water"' core/content/registries/blocks/*.rs`) on (7..9,7..9); `copper-wall`
  at (8,8) → false; `conduit` at (8,8): Java `Conduit` `floating = true` (`Conduit.java` —
  confirm) but `contactsShallows` fails in the middle of deep water → false; at (7,8) (edge,
  neighbour (6,8) stone) → true.
* `same_block_same_rotation_rejected`: conveyor rot 0 at (5,5); conveyor rot 0 → false; rot 1
  → true; `router` (not rotating) over `router` → false (`can_replace` same block, no rotate).
* `derelict_same_block_repair`: copper-wall team 0 at (5,5); copper-wall team 1 → true;
  with `allow_derelict_repair = false` → false.
* `enemy_core_radius_blocks_placement`: 64x64 sim; core-shard team 2 at (40,40); BuildRules
  `enemy_core_build_radius = 400.0` (Java default — check `Rules.java enemyCoreBuildRadius`)
  ; wall team 1 at (40, 40+50) (dist 400 < 408) → false; at (40, 40+52) (416) → true.
  With `PlaceFlags::API` → true at both.
* `banned_block_rejected_for_player_only`: `banned_blocks = [copper-wall]`; PLAYER → false;
  API → true; `block_whitelist = true` → PLAYER true.
* `kind_rotates_matches_java`: assert true for Conveyor, Duct, Conduit, ItemTurret,
  UnitFactory, Reconstructor, HeatProducer, BeamDrill, PowerDiode, PayloadSource; false for
  Junction, ItemBridge, Sorter, Router, OverflowGate, MassDriver, LiquidRouter, Drill, Door,
  PowerNode, BeamNode, PayloadDeconstructor.
* Run `cargo test -p mind-core --lib distribution` and `--lib input` (autotiler and line
  placement read rotation).

**Verify in-engine.** Ground Zero (Go):
```bash
$G --eval "return $H.valid_place_preview(145,60,\"copper-wall\",0)"                          # true
$G --eval "return $H.place_block_rot(146,60,\"conveyor\",0)" \
   --eval "return $H.valid_place_preview(146,60,\"conveyor\",0)" \
   --eval "return $H.valid_place_preview(146,60,\"conveyor\",1)"                              # false, true
$G --eval "return $H.valid_place_preview(129,55,\"copper-wall\",0)"                            # false (core)
```
Deep water: headless only — a scan of the playable area (x 90..160, y 20..80, every 2nd
tile, rows every 4) found no `*water*` floor on Ground Zero. Enemy core radius: headless only
(no enemy core on Ground Zero; (150,75)/(150,95)/(150,105) are static walls or outside the
`limit_area` y <= 80). Enemy-block replacement IS checkable today: stage a team-2 conveyor
`apply_sim_command_json('{"op":"place","x":140,"y":66,"block":<conveyor id>,"team":2}')`, then
`valid_place_preview(140,66,"titanium-conveyor",0)` → false after the fix. **Before the fix
(verified 2026-10-14)** a `"player":0` titanium-conveyor place over it is accepted, queued and
built: `tile_debug(140,66).name` = `titanium-conveyor`, `building_team_at(140,66)` = 1.

**Pitfalls.** This changes what the team AI, schematics and builder accept: run
`cargo test -p mind-headless --lib` and the `parity goldens`; if `units_base_build` or a
campaign golden changes, compare with Java before regenerating (BaseBuilderAI placements near
the player core are now rejected — that is Java behavior). The API path must keep working for
every env §4 recipe (sandbox sources, placing next to the core) — that is why `API` skips
visibility and core radius. Do not model fog discovery.

**Commit.** `Gameplay H02 T3: Build.validPlace team, water, rotation, visibility, core radius; kind_rotates`

---

### T4. Cores: placement only by upgrade, upgrade keeps items, canPlaceOn

> **In-engine verification (2026-10-14, session 3):** CONFIRMED and destructive. `core-foundation` (block id 340) `"player":0` on free ground (140,60) → `true`, plan queued (Java rejects via `canPlaceOn`). Over the shard with 4000 copper/lead/silicon: the shard becomes `build4`, `core_items_json()` = `{}`, `hasCore` false (Sector Lost), plan stuck at progress 0.0001 — `runs/20261014-s3-verify/H02-T4-core-upgrade-lost.png`. FIXED in place: "run only after the fix" warning, block id, clear-after-step note.

Depends on T2 + T3.

**Symptom / acceptance.** A core-foundation can be placed on any free ground and cannot be
placed over a shard. After: foundation/nucleus are placeable only over a smaller own core (or
on a `core-zone` floor); when construction begins over the old core the upgrade completes
instantly and the core keeps its items minus the cost.

**Java.** `J/world/blocks/storage/CoreBlock.java`:
* `:179-183` `canReplace(other)`: `super.canReplace(other) || (other instanceof CoreBlock && size >= other.size && other != this)`.
* `:185-203` `canPlaceOn` (pasted):
  ```java
  if(tile == null) return false;
  if(state.isEditor() || state.rules.coreBuildAndConfig) return true;
  CoreBuild core = team.core();
  tile.getLinkedTilesAs(this, tempTiles);
  if(!tempTiles.contains(o -> !o.floor().allowCorePlacement || o.block() instanceof CoreBlock)){
      return true;
  }
  if(core == null || (!state.rules.infiniteResources && !core.items.has(requirements, state.rules.buildCostMultiplier))) return false;
  return tile.block() instanceof CoreBlock && size > tile.block().size && (!requiresCoreZone || tempTiles.allMatch(o -> o.floor().allowCorePlacement));
  ```
* `:205-226` `placeBegan(tile, previous, builder)`: if `previous instanceof CoreBlock`:
  `tile.setBlock(this, tile.team())`, effects, then `tile.team().core().items.set(nextItems)`;
  fire `BlockBuildEndEvent`.
* `:228-239` `beforePlaceBegan`: `nextItems = tile.build.items.copy(); if(!infiniteResources) items.remove(ItemStack.mult(requirements, buildCostMultiplier));`
* `requiresCoreZone = true` only for Erekir cores (`Blocks.java:3198,3218,3236`).
* Costs: foundation `copper 3000, lead 3000, silicon 2000` (`Blocks.java:3160`), capacity 9000
  (`:3164`); shard capacity 4000 (`:3152`); nucleus `copper 8000, lead 8000, silicon 5000, thorium 4000` (`:3173`).
* `ItemModule.has(stacks, multiplier)`: `items[i] >= Math.round(stack.amount * multiplier)` per
  stack — confirm in `J/world/modules/ItemModule.java` (`public boolean has(ItemStack[] stacks, float multiplier)`).

**Port today.** `core/world/build.rs:92 can_replace` has no core rule; `can_place_on` stub
(`:78`); `core/sim/runtime.rs:497 begin_place` always makes a construct block (previous kept
only when same size).

**Changes.**
1. `can_replace`: append `|| (other.kind == BlockKind::CoreBlock && def.kind == BlockKind::CoreBlock && def.size >= other.size && other.id != def.id)`.
2. `valid_place_query` step c (T3): when `def.kind == CoreBlock` run the Java `canPlaceOn`:
   `rules.editor || rules.core_build_and_config` → ok; footprint tiles: if none has
   `!floor.allow_core_placement || tile_block_is_core` → ok (core zone); else need `world`
   (None → ok, keep old behavior): team core = first `CoreBuild` entity of `team` by entity
   index; its `ItemModule` must hold every requirement `round(amount * rules.build_cost_multiplier)`
   unless `rules.infinite_resources`; and the centre tile's block is a core with
   `def.size > that.size` (and `requires_core_zone` false — add `requires_core_zone: bool` to
   `BlockDef` only if you also set it for the three Erekir cores; otherwise skip and comment).
3. `SimRuntime::begin_place`: before making the construct block, if the centre tile holds a
   `CoreBlock` of the same team and `def.kind == CoreBlock`:
   `next_items` = clone of the old core's `ItemModule`, minus `round(amount * cost_multiplier)`
   per requirement (saturating at 0) unless `BuildRules.infinite_resources`;
   `self.set_block_raw(world, grid, x, y, block, rot, team)`; then set the new core entity's
   `ItemModule` to `next_items` (respecting the new capacity); call `placed_hooks`; push the
   same build-end event the construct-finish path pushes (grep `BlockBuildEndEvent` in
   `core/sim/builder.rs`); return `true` without a construct block. The builder must then drop
   the plan as finished (check `core/sim/builder.rs` around the `begin_place` call at ~647:
   if it expects a construct entity afterwards, make it treat "tile already holds the plan
   block" as done).

**Tests** (`core/sim/builder.rs` tests, `sim_with_player()`):
* `core_upgrade_keeps_items`: seed the shard with copper 4000, lead 4000, silicon 4000 (shard
  cap 4000); queue `core-foundation` at (10,10) via `sim.queue_player_place(10, 10, id, 0)`;
  tick until the plan list is empty (≤ 600 ticks); tile block is `core-foundation`; copper
  1000, lead 1000, silicon 2000.
* `core_needs_existing_core`: foundation on free ground (30,30) → `valid_place_player` false.
* `core_upgrade_needs_items`: shard with copper 100 → foundation over it → false.

**Verify in-engine.** Ground Zero (Go):
```bash
FID=$($G --eval 'return get_node("/root/Spine/SimHost").content_list("block").find("core-foundation")')
$G --eval "$H.debug_add_core_items(\"copper\",4000); $H.debug_add_core_items(\"lead\",4000); $H.debug_add_core_items(\"silicon\",4000); return $H.core_items_json()"
$G --eval "return $H.apply_sim_command_json('{\"op\":\"place\",\"x\":129,\"y\":55,\"block\":$FID,\"player\":0}')"
$G --eval "$H.set_paused(true); return $H.step(300)" --eval "return $H.tile_debug(129,55)" --eval "return $H.core_items_json()"
```
Expect `core-foundation` and copper/lead reduced by 3000, silicon by 2000. Screenshot.
(`content_list("block").find("core-foundation")` = 340 on `ffbe84dd`.) **Run this only AFTER
the fix:** on current main the plan is queued, the shard turns into a `build4` construct site,
`core_items_json()` = `{}`, `hasCore` = false (Sector Lost, autosaved) and the plan is stuck at
progress 0.0001 — recover with the §4.3 clean re-import. Free-ground check (no profile damage):
`apply_sim_command_json('{"op":"place","x":140,"y":60,"block":340,"player":0}')` → today `true`
and a queued `core-foundation` plan (Java: rejected by `canPlaceOn`); then
`clear_player_build_plans()` AFTER a `step(1)` (a clear before the command is applied is a
no-op).
Note: the foundation is 4x4 centred at (129,55) covering 128..131 — the H00 player unit may
stand there; Java's unit-overlap rule ignores non-solid... cores are solid: if the plan never
starts, move the player (`debug_spawn_player`) and record it.

**Pitfalls.** `ItemStack.mult` rounds (`Math.round`); use `(amount as f32 * mult).round() as i32`.
The upgrade must not run `begin_break` on the old core. G04 tracks cores in `Teams`
(`register_core`/`unregister_core`): after `set_block_raw` verify the new core is registered
(grep how `set_block_raw` notifies `Teams`; if not, call the same function the construct-
finish path calls).

**Commit.** `Gameplay H02 T4: core upgrade placement keeps items; core canPlaceOn`

---

### T5. PAR-0190: configure values survive the wire; bridge-link hang root cause

> **In-engine verification (2026-10-14, session 3):** PARTLY. (b) bridge hang NOT REPRODUCED: paused, one call per eval, `configure_building(140,68,"int","9371716")` → pending 1 → `step(1)` → `{"type":"point","x":3,"y":0}` → `step(60)` fine, bridge drawn linked (`runs/20261014-s3-verify/H02-T5-bridge-int.png`); `"number"` kind also links. (c) door `"true"`, sorter `{"type":"item","id":0}` already reach `configured()`; the sorter sprite shows no item icon (render half, H03: `runs/20261014-s3-verify/H02-T5-door-sorter.png`). (a) value mangling CONFIRMED by code only (`to_sim_config`, `core/input/command_emit.rs:248-261`; no gmcp seam emits a world `Point2`/content value today). FIXED in place: coordinates (150,70)/(153,70)/(152,74)/(154,74) were static walls → (140,68)/(143,68)/(146,68)/(148,68), pack value 9371716, unlink readback.

**Symptom / acceptance.** (a) A world `Point2` config (relative link) becomes an *absolute*
packed position; `Point2Array`, `Item`, `Liquid`, `Block`, `Unit`, `Content` world values are
sent as the string `"complex"` and rejected; `Number` is narrowed to `f32` (packed positions
with x ≥ 256 lose bits). (b) G03 T7: `configure_building(x,y,"int",packed)` on a bridge
"hung the eval/session" — root cause unknown. (c) PAR-0190: door/sorter/switch configure must
be proven to reach `configured()`. After: every world config value round-trips through the
wire exactly; tests prove sorter, door, switch, item bridge, mass driver and power node configs
apply; the bridge hang is reproduced or ruled out with evidence.

**Java.** Building configs are `Object`s: `Integer` (absolute packed `Point2.pack`, or a raw
int), `Point2` (relative), `Point2[]` (relative), content (`Item`, `Liquid`, `Block`,
`UnitType`), `Boolean`, `String`, `byte[]`, `Double`... (`J/io/TypeIO.java writeObject`).
`ItemBridge.java:66-69`:
```java
//point2 config is relative
config(Point2.class, (ItemBridgeBuild tile, Point2 i) -> tile.link = Point2.pack(i.x + tile.tileX(), i.y + tile.tileY()));
//integer is not
config(Integer.class, (ItemBridgeBuild tile, Integer i) -> tile.link = i);
```

**Port today.**
* Wire `crate::determinism::ConfigValue` (`core/determinism/command.rs:20-35`): `None, Bool,
  Int(i32), Float(f32), Content(String), Bytes, Text` with codec tags 0..6
  (`encode_config` `:636`, `decode_config` `:897`).
* `core/input/command_emit.rs:248 to_sim_config`: `Number → Float(f32)`;
  `Point2(x,y) → Int(TilePos::new(x,y).pack())` (relative offsets packed as if absolute);
  everything else → `Content("complex")`.
* `core/sim/runtime.rs:212 resolve_config`: `Int → Number(f64)`, `Content(name)` → by-name
  content, numeric strings → `Block`.
* `gdext/input/mod.rs:1185 configure_building`: kinds `string`/`int` go direct; `none`,
  `number`, `bool`, anything else (`content`, `item`, …) → `ConfigValue::String(value)` through
  the batcher (→ `Content(name)` → resolved by name: this path works for content names).
* Doors: `DoorBehavior::configured` (`core/world/behavior/defense.rs` ~195) takes `Bool`;
  `building_config_json` returns `true/false` for doors.

**Changes.**
1. Wire: append (never reorder) to `crate::determinism::ConfigValue`:
   `Point2(i32, i32)` (tag 7), `Point2Array(Vec<i32>)` (tag 8; packed relative points, i32 count
   then i32 each), `ContentId { kind: u8, id: u16 }` (tag 9; `kind` = `ContentType as u8` —
   check `ContentType` has a stable numeric repr: `grep -n "pub enum ContentType" -A30 core/content/*.rs`).
   Encode/decode in `encode_config`/`decode_config`; add a codec round-trip test next to the
   existing ones in `command.rs` tests. Fix every exhaustive `match` the compiler reports
   (including `mind-stdb` if it matches on the wire enum — `grep -rn "ConfigValue::Text" client/rust --include=*.rs`).
2. `to_sim_config`: `Point2(x,y) → Point2(x,y)`; `Point2Array(p) → Point2Array(p.to_vec())`;
   `Item(id) → ContentId{kind: ContentType::Item as u8, id: id.raw()}` (same for Liquid, Block,
   Unit, `Content(t,id)`); `Number(n)`: if `n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64`
   → `Int(n as i32)` else `Float(n as f32)`; `Building(_)` stays unsupported (`Content("complex")`
   is fine, log once at debug).
3. `resolve_config`: `Point2(x,y) → WorldConfigValue::Point2(x,y)`;
   `Point2Array(v) → Point2Array(v.into_iter().collect())`; `ContentId` → the typed world
   variant (Item/Liquid/Block/Unit, else `Content(type, id)`), rejecting out-of-range ids with
   `CommandError::UnknownContent(id)`.
4. `configure_building` (`gdext/input/mod.rs`): add kinds `"item"`, `"liquid"`, `"block"`,
   `"unit"` mapped like `"content"` (by name, `ConfigValue::String`), and `"point"` (`value`
   `"dx,dy"` → `ConfigValue::Point2`). Keep `"int"` direct (absolute packed). Update the
   doc comment and H-environment's seam list in your summary (not the file).
5. Bridge hang: write the reproduction test first (below). If the test hangs (cargo test
   timeout) or loops, find the loop with `RUST_BACKTRACE=1` + a `#[ignore]`-free run under
   `timeout 120`, fix it, keep the test. Known `while` loops on this path:
   `ItemBridgeBehavior::update_transport` (`item_bridge.rs` ~552, `while counter >= self.transport_time`
   — infinite when `transport_time == 0` on the non-buffered path; bridge-conveyor is
   registered buffered so this should not trigger — assert it), `transfer.rs` ~467
   (`dump_accumulate`). If the headless test passes, do the in-engine reproduction below
   **paused**, one call per eval, to isolate whether the hang is in the configure call, the
   next sim step, or the render (`gdext/render/links.rs` `bridge_links` loop — the arrow count
   `drawf::bridge_arrow_count` is `dist*2`; an absolute/relative mix-up linking to a far tile
   gives thousands of commands per frame). Report the stage that stalls. If it is the render,
   add a guard in `MindSimHost::bridge_links` (`gdext/sim_host.rs` ~2989) that skips links
   failing `positions_valid` with the block's range (Java draws only `linkValid` links,
   `ItemBridge.java:384` `draw`) and tell H03.

**Tests.**
* `core/determinism/command.rs` tests: `config_wire_point2_roundtrip`,
  `config_wire_content_id_roundtrip`.
* `core/input/command_emit.rs` tests: `to_sim_config_keeps_relative_point` (`Point2(3,0)` →
  wire `Point2(3,0)`), `to_sim_config_number_integral_is_int` (`Number(9830470.0)` → `Int(9830470)`).
* `core/sim/production_tests.rs`:
  * `configure_reaches_sorter_door_switch`: `runtime_sim()`; `sorter` (5,5) configure wire
    `ContentId{Item, copper}` → `SorterBuild.sort_item == Some(copper)`; `door` (8,5)
    `Bool(true)` → `DoorState.open`; tick 61; `Bool(false)` → closed; `switch` (11,5)
    `Bool(false)` → `Building.enabled == false`.
  * `bridge_configure_int_absolute_links_and_ticks`: `bridge-conveyor` (5,5) and (8,5); feed
    the first 10 copper (`ItemModule`); configure wire `Int(pack(8,5))`; tick 600 (must
    finish); `ItemBridgeBuild.link == pack(8,5)` and copper arrived at (8,5) or beyond.
  * `bridge_configure_point_relative`: wire `Point2(3,0)` on (5,5) → link `pack(8,5)`.
  * `node_configure_point_array`: `power-node` (5,5) + `battery` (8,5) + `battery` (5,8);
    wire `Point2Array([pack(3,0), pack(0,3)])` (relative, packed with `pos::pack`) → node
    `PowerModule.links.len() == 2`.

**Verify in-engine.** Ground Zero (Go), paused:
```bash
$G --eval "$H.set_paused(true); return true"
$G --eval "return $H.place_block(140,68,\"bridge-conveyor\")" --eval "return $H.place_block(143,68,\"bridge-conveyor\")"
$G --eval "return $I.configure_building(140,68,\"int\",\"9371716\")"      # pack(143,68) = 143<<16 | 68
```
(The original (150,70)/(153,70)/(152,74)/(154,74) are static walls.) Then, one eval each:
`return $H.pending_command_count()` (1), `return $H.step(1)`,
`return $H.building_config_json(140,68)` (expect `{"type":"point","x":3,"y":0}`),
`return $H.step(60)`, screenshot (`$C.set_process(false); $C.center_on_tile(141,68)` first).
Unlinking with `("int","-1")` reads back `{"type":"point","x":-141,"y":-69}` (= unpack(-1)
relative to the tile, i.e. "no link"). Repeat with `configure_building(140,68,"point","3,0")`
after unlinking. Door: `place_block(146,68,"door")`, `configure_building(146,68,"bool","true")`,
`step(2)`, `building_config_json(146,68)` → `"true"`. Sorter: `place_block(148,68,"sorter")`,
`configure_building(148,68,"item","copper")` (already works today: unknown kinds fall through
to the by-name content path), `step(1)`, `building_config_json` → `{"type":"item","id":0}`.
Take a screenshot of the sorter and door; if the sprites do not change although the config
JSON did, PAR-0190's remainder is H03's (render invalidation) — write that in the summary with
the screenshot paths. (Verified 2026-10-14 on main: all JSONs above already read back as
stated, the bridge links and draws, the door draws open, the sorter draws WITHOUT its item
icon: `runs/20261014-s3-verify/H02-T5-door-sorter.png`.)

**Pitfalls.** Wire tags are a persisted/networked format: append only, bump nothing else.
`mind-stdb` generated bindings are never hand-edited; if the STDB module encodes configs, stop
and report instead of editing generated code. Keep `bind()` guards short in gdext.

**Commit.** `Fixes PAR-0190: configure values keep their type over the wire; bridge link hang`
(if the hang root cause was not found, subject `PAR-0190: ...` and say so in the body).

---

### T6. Tap-to-link (bridges, bridge conduits, mass drivers, power nodes) + LiquidBridge config/IO/chain

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Real taps (`godot_input` motion + press/release, viewport coords): tapping bridge A opens an item list (22 items incl. Erekir + "Clear"), tapping bridge B does not link (config stays `-1`); tapping a power node opens an empty panel, tapping a battery links nothing (`building_config_json(node)` = `null`) — `runs/20261014-s3-verify/H02-T6-tapA.png`, `H02-T6-tapB.png`, `H02-T6-node-tap.png`. FIXED in place: coordinates (x 150..154 are walls) and B moved north of A (the panel opened by A covers tiles right/below A); `power_debug` reports `graph_id` 0 for unlinked buildings, so the recipe now reads the node config.

Depends on T5 (wire `Point2`/`Point2Array`). Provides the H06 seam.

**Symptom / acceptance.** Tapping a bridge conveyor opens an item list; tapping a second
bridge does nothing; bridge conduits cannot be linked at all and lose links on save; tapping a
node then a consumer does not link; mass drivers cannot be aimed. After (two real taps):
bridge A then B (same row, ≤ range) → A links B; tapping B again (A selected) → unlink;
node → battery in range → link toggles; node tapped twice → autolink (no links) or clear;
mass driver → other driver within range → link; self → unlink (+ deselect when already
unlinked). Player-built bridge conduits chain like item bridges.

**Java.**
* `J/input/InputHandler.java:1973-2006` `tileTapped` (pasted):
  ```java
  if(build == null){ inv.hide(); config.hideConfig(); commandBuildings.clear(); return false; }
  boolean consumed = false, showedInventory = false;
  if(build.isCommandable() && commandMode){ consumed = true; }
  else if(build.block.configurable && build.interactable(player.team())){
      consumed = true;
      if((!config.isShown() && build.shouldShowConfigure(player))
      || (config.isShown() && config.getSelected().onConfigureBuildTapped(build) && build.shouldShowConfigure(player))){
          build.block.configureSound.at(build.x, build.y, 1f, 1f, control.sound.uiBus);
          config.showConfig(build);
      }
  }else if(!config.hasConfigMouse()){
      if(config.isShown() && config.getSelected().onConfigureBuildTapped(build)){ consumed = true; config.hideConfig(); }
      if(config.isShown()){ consumed = true; }
  }
  if(!consumed && build.interactable(player.team())){ build.tapped(); }
  ```
* `J/entities/comp/BuildingComp.java:1696-1706` default:
  ```java
  public boolean onConfigureBuildTapped(Building other){
      if(block.clearOnDoubleTap){
          if(self() == other){ deselect(); configure(null); return false; }
          return true;
      }
      return self() != other;
  }
  ```
  `clearOnDoubleTap = true` in: PayloadRouter, Unloader, UnitFactory, Constructor, ItemSource,
  LiquidSource, Sorter, DuctRouter, UnitCargoUnloadPoint, PayloadSource, DirectionalUnloader.
  `deselect()` (`:400-404`) hides the config if this building is selected.
* `J/world/blocks/distribution/ItemBridge.java:298-316` (also `LiquidBridge`):
  ```java
  public boolean onConfigureBuildTapped(Building other){
      if(other instanceof ItemBridgeBuild b && b.link == pos()){ configure(other.pos()); other.configure(-1); return true; }
      if(linkValid(tile, other.tile)){ if(link == other.pos()){ configure(-1); }else{ configure(other.pos()); } return false; }
      return true;
  }
  ```
  `linkValid(tile, other, checkDouble)` `:152-164`; `positionsValid` `:166-176`.
  `playerPlaced` `:224-233`:
  ```java
  Tile link = findLink(tile.x, tile.y);
  if(linkValid(tile, link) && this.link != link.pos() && !proximity.contains(link.build)){ link.build.configure(tile.pos()); }
  lastBuild = this;
  ```
  `findLink` `:180-186`: `lastBuild != null && linkValid(tile, lastBuild.tile) && lastBuild.tile != tile && lastBuild.link == -1`.
  IO `:523-549`: `i link, f warmup, b incoming.size, i incoming…, bool wasMoved||moved` (rev ≥ 1).
* `J/world/blocks/distribution/MassDriver.java:272-288` (pasted):
  ```java
  if(this == other){ if(link == -1) deselect(); configure(-1); return false; }
  if(link == other.pos()){ configure(-1); return false; }
  else if(other.block == block && other.dst(tile) <= range && other.team == team){ configure(other.pos()); return false; }
  return true;
  ```
* `J/world/blocks/payloads/PayloadMassDriver.java:448-464`: same shape with
  `other.block instanceof PayloadMassDriver` (read it before writing).
* `J/world/blocks/power/PowerNode.java:419-441` (pasted):
  ```java
  if(linkValid(this, other)){ configure(other.pos()); return false; }
  if(this == other){
      if(other.power.links.size == 0){
          Seq<Point2> points = new Seq<>();
          getPotentialLinks(tile, team, link -> { if(!insulated(this, link) && points.size < maxNodes){ points.add(new Point2(link.tileX() - tile.x, link.tileY() - tile.y)); } });
          configure(points.toArray(Point2.class));
      }else{ configure(new Point2[0]); }
      deselect(); return false;
  }
  return true;
  ```

**Port today.**
* No `on_configure_build_tapped`, `config_tap`, or `config_selected` anywhere
  (`grep -rn "on_configure_build_tapped\|config_tap\|config_selected" client/rust` → nothing).
* `gdext/input/desktop.rs:1828 tile_tapped`: switch branch, door branch, then
  `config_spec(x,y)` → `Effect::OpenBlockConfig`, else inventory. `config_spec` (`:2407`) falls
  back to an item list for any configurable block with items (bridges, drivers).
* `gdext/input/mod.rs:600` handles `Effect::OpenBlockConfig` by calling the fragment's
  `configure`. Panel `ui/fragments/block_config_fragment.gd` (close button `:155`
  `visible = false`; `_submit` `:159`).
* `ItemBridgeBehavior` has `config`/`configured`/`link_valid_between`/`write`/`read`
  (`item_bridge.rs:299-505`); `MassDriverBehavior::configured` and
  `PayloadMassDriverBehavior::configured` accept `Point2`/`Number`.
* `LiquidBridgeBehavior` (`liquid/behavior.rs:371-414`) has only `create_state`/`update_tile`;
  it stores an `ItemBridgeBuild`; `liquid/bridge.rs:97 link_valid_world(world, e, other, range)`.
* Chaining: `core/sim/builder.rs:430 chain_bridge` (only `ItemBridge`/`BufferedItemBridge`,
  no proximity guard), called at ~841 on construct finish.

**Changes.**
1. `core/world/behavior/mod.rs`: add
   ```rust
   /// Result of `Building.onConfigureBuildTapped(other)` (`BuildingComp.java:1696-1706`).
   #[derive(Debug, Clone, PartialEq)]
   pub enum ConfigTapResult {
       /// Java `return false`: tap consumed, the panel stays on the selected building;
       /// apply these `(building, value)` configs in order.
       Consume(Vec<(Entity, ConfigValue)>),
       /// Java `deselect(); return false`: apply configs, then close the panel.
       ConsumeAndDeselect(Vec<(Entity, ConfigValue)>),
       /// Java `return true` with configs to apply first (ItemBridge reverse link).
       SwitchAfter(Vec<(Entity, ConfigValue)>),
   }
   ```
   and the trait method (default = Java default; `clear_on_double_tap` is a new `BlockDef`
   bool or a by-kind match — use a by-kind match on the class list above to avoid a content
   golden change):
   ```rust
   fn on_configure_build_tapped(&self, world: &World, grid: &WorldGrid, e: Entity, other: Entity) -> ConfigTapResult {
       let _ = grid;
       if crate::world::config_tap::clear_on_double_tap(world, e) {
           if e == other { return ConfigTapResult::ConsumeAndDeselect(vec![(e, ConfigValue::None)]); }
           return ConfigTapResult::SwitchAfter(Vec::new());
       }
       if e != other { ConfigTapResult::SwitchAfter(Vec::new()) } else { ConfigTapResult::Consume(Vec::new()) }
   }
   ```
   (Java `return self() != other`: tapping the selected block again returns false =
   consumed, panel stays.)
2. New file `core/world/config_tap.rs` (`pub mod config_tap;` in `core/world/mod.rs`; license
   header): `pub fn clear_on_double_tap(world, e) -> bool` (kind match),
   `pub fn config_tap(world: &World, grid: &WorldGrid, selected: Entity, other: Entity) -> ConfigTapResult`
   resolving `selected`'s behavior via `BlockTable` (return `SwitchAfter(vec![])` when missing).
3. Overrides:
   * `ItemBridgeBehavior` (and `LiquidBridgeBehavior`): Java body; values are absolute packed
     `ConfigValue::Number(pack as f64)` and `Number(-1.0)`. Reverse link →
     `SwitchAfter(vec![(e, Number(other_pos)), (other, Number(-1.0))])`.
   * `MassDriverBehavior`: Java body; `other.dst(tile) <= range` with the driver's world range
     (`MassDriver.range`; read the behavior's field name) measured centre-to-centre in world
     units; self with `link == -1` → `ConsumeAndDeselect(vec![(e, Number(-1))])`, else
     `Consume(vec![(e, Number(-1))])`.
   * `PayloadMassDriverBehavior`: same shape per its Java.
   * `PowerNodeBehavior`: `linkValid(this, other)` — reuse the port's node link check (grep
     `fn link_valid` in `core/world/blocks/power/nodes.rs`) → `Consume(vec![(e, Number(other_pos))])`;
     self: no links → `get_potential_links` (needs `grid`: `nodes::get_potential_links(world, grid, e, &mut out)`,
     takes `&mut World`? — if it needs `&mut`, add a `&World` variant or compute candidates
     read-only; never mutate in `config_tap`) filtered by `!insulated` and capped at
     `max_nodes`, as relative `Point2Array` → `ConsumeAndDeselect`; links → `Point2Array([])`
     → `ConsumeAndDeselect`.
4. `LiquidBridgeBehavior`: add `config` (copy `ItemBridgeBehavior::config`), `configured`
   (copy `ItemBridgeBehavior::configured`), `link_valid_between` →
   `bridge::link_valid_world(world, e, other, self.range) && other's ItemBridgeBuild.link != e's pos`,
   `version() = 1`, `write`/`read` = `ItemBridgeBehavior::write/read` without the buffer
   (Java `ItemBridge.java:523-549`). Add an IO test in `core/world/building_io_kinds_tests.rs`
   (`liquid_bridge_body_matches_java`, pattern of `stack_conveyor_body_matches_java`).
5. Chaining (`core/sim/builder.rs chain_bridge`): include `BlockKind::LiquidBridge`; add the
   `!proximity.contains(link.build)` guard (read the placed building's proximity: grep the
   proximity component, `core/world/proximity.rs`); fix the doc comment (remove the false
   LiquidBridge claim, cite `LiquidBridge.java:7`).
6. Sim command: add a wire-free path so the sim applies tap results deterministically:
   `SimCommand::ConfigTap { x: i16, y: i16, other_x: i16, other_y: i16 }` (append-only, next
   free tag in `encode_command`; add `op_name` `"config_tap"`). `Sim::command` arm: resolve both
   entities via `self.grid.entity_at`, call `config_tap`, apply each `(entity, value)` with
   `configure_power_node` first then `world::config::configure` (the same order as the
   `Configure` arm), push nothing else. Return the result kind to the host through a new
   `Sim::last_config_tap: Option<ConfigTapKind>` field (`Consume`/`Deselect`/`Switch`) — the
   gdext tap handler needs it to decide whether to open the tapped block's panel.
   (Rationale: Java computes this client-side and sends `configure`; the port's input layer
   holds no rules, so the sim computes it. Document the deviation in the commit.)
7. gdext (`gdext/input/desktop.rs`): field `config_selected: Option<(i32, i32)>` on the bridge
   struct (`DesktopBridge`, ~`:177`). Set it when pushing `Effect::OpenBlockConfig`; clear it
   on hide. In `tile_tapped`, **before** the switch/door branches:
   ```text
   if let Some((sx, sy)) = self.config_selected and the selected tile still has a building:
       enqueue SimCommand::ConfigTap{sx, sy, x, y} via host.bind_mut().enqueue_sim_command (drop the guard first)
       then decide from Java: tapped building configurable+interactable -> after the tap result
       (needs the result: run `host.bind().preview_config_tap(sx,sy,x,y) -> i32` — a new pub fn on
       MindSimHost that calls `config_tap` read-only and returns 0=Consume,1=Deselect,2=Switch)
         Consume -> keep panel, return true
         Deselect -> close panel (clear config_selected, call fragment hide), return true
         Switch  -> clear selection; fall through to the normal open path for (x,y)
   ```
   Compute `preview_config_tap` with `host.bind()`, drop the guard, then enqueue (re-entrant
   borrow rule, H-environment §3). Interactability: only open configs for buildings whose team
   is the player's team or derelict (`building_team_at(x,y)`).
   Add `#[func] fn close_block_config(&mut self)` on `MindInput` (`gdext/input/mod.rs`) clearing
   `config_selected`; call it from `ui/fragments/block_config_fragment.gd` wherever it sets
   `visible = false` (close button `:155`, and `_submit` if it hides). GDScript only calls the
   node; it does not touch sim state.
8. `config_spec`: for `ItemBridge`, `BufferedItemBridge`, `LiquidBridge`, `MassDriver`,
   `PayloadMassDriver`, `PowerNode`/`LongPowerNode` return a spec with `"link": true` and no
   options; `block_config_fragment.gd _rebuild`: when `link` is true show only the title (bundle
   `block.<name>.name`) and the close button. Item option list: skip items whose
   `build_visibility`/`hidden` flag hides them (check `ItemDef` for a `hidden` field); unlock
   filtering stays a TODO in §6 (the gdext content snapshot has no research state).

**Tests** (`core/world/config_tap.rs` tests module, a `runtime_sim()`-style sim; add a local
helper or use `crate::sim::Sim` directly):
* `bridge_tap_links_then_unlinks`: bridges (5,5),(8,5) → `config_tap(A,B)` =
  `Consume([(A, Number(pack(8,5)))])`; apply; again → `Consume([(A, Number(-1))])`.
* `bridge_reverse_tap`: A links B; `config_tap(B, A)` = `SwitchAfter([(B, Number(pack(5,5))), (A, Number(-1))])`.
* `bridge_out_of_range_switches`: bridge-conveyor range 4 (`ItemBridgeBehavior::BRIDGE_CONVEYOR`);
  (5,5)→(10,5) → `SwitchAfter([])`.
* `sorter_double_tap_clears`: sorter configured copper; `config_tap(S,S)` =
  `ConsumeAndDeselect([(S, None)])`.
* `liquid_bridge_links_and_roundtrips`: bridge-conduit (5,5),(8,5) → links; save body round trip.
* `mass_driver_self_tap_unlinks`, `node_self_tap_autolinks` (node + 2 batteries in range →
  `ConsumeAndDeselect([(node, Point2Array(len 2))])`).
* `sim_config_tap_command_applies` (`production_tests.rs`): `SimCommand::ConfigTap` links two
  bridges; `ItemBridgeBuild.link` updated.

**Verify in-engine.** Ground Zero (Go), not paused. (The original x=150..154 tiles are static
walls; B is placed NORTH of A because the config panel opened by tapping A anchors at A's
screen point and extends right/down — a B east of A sat on the panel's top edge in the
2026-10-14 check.)
```bash
$G --eval "return $H.place_block(140,70,\"bridge-conveyor\")" --eval "return $H.place_block(140,73,\"bridge-conveyor\")"
$G --eval "$C.set_process(false); $C.center_on_tile(140,71); return true"
$G --eval "return $C.tile_to_screen(140,70)" --eval "return $C.tile_to_screen(140,73)"
```
Real taps with the bridge input tool at the printed viewport coordinates (`mouse_motion`,
then `mouse_button` press + release, `"coords":"viewport"`, H-environment §4): tap A, tap B;
`building_config_json(140,70)` → `{"type":"point","x":0,"y":3}`; screenshot (H03 draws the
bridge). Tap A again then B → unlinked (`{"type":"point","x":-141,"y":-71}` = unpack(-1)
relative to the tile is Java's "no link"; check `ItemBridgeBuild.link == -1` via
`building_debug` if exposed, else the JSON). Repeat with `bridge-conduit` at (142,70)/(142,73)
and with `power-node` (140,75) → `battery` (140,77) (`power_debug` reports `graph_id` 0 for
both while unlinked, so compare `building_config_json(140,75)` = `null` before / a link list
after, plus `power_debug(140,77).graph_id` == the node's non-zero id). Player chaining:
`apply_sim_command_json` place `bridge-conduit` at (144,70) then (144,73) with `"player":0`,
step until built, `building_config_json(144,70)` → points at (144,73).
(Before the fix, verified 2026-10-14: tapping a bridge opens an item list of ALL 22 items
incl. Erekir ones plus "Clear"; tapping the second bridge does not link
(`building_config_json` stays unlinked); tapping a node opens an empty option panel and
links nothing: `runs/20261014-s3-verify/H02-T6-tapB.png`, `H02-T6-node-tap.png`.)

**Pitfalls.** Never apply configs inside `config_tap` (read-only). `SimCommand` tags are
append-only. `RemoteAction::Configure` remains for panel submissions. Keep the sorter/item
source "tap twice clears" behavior: that is Java's `clearOnDoubleTap`.

**Commit.** `Gameplay H02 T6: tap-to-link for bridges, conduits, drivers and nodes; liquid bridge config and IO`

---

### T7. Force projector: efficiency-0 diagnosis, live absorption, phase boost, coolant (PAR-0129 sim half)

> **In-engine verification (2026-10-14, session 3):** PARTLY / main symptom NOT REPRODUCED. A `force-projector` at (140,62) with an adjacent `power-source` (138,62): after 120 ticks `building_debug` `efficiency` 1.0, `power_debug` satisfaction 1.0 (shared graph 3), and the full hexagon dome is drawn (`runs/20261014-s3-verify/H02-T7-force-projector.png`). So PAR-0129's "efficiency stays 0" does not occur on `ffbe84dd`; step 1's test will likely pass at once — keep it as a regression test, do NOT hunt for a cause. Live absorption missing CONFIRMED by code: `ForceProjectorBehavior::update_tile` (`behaviors.rs:67-74`) never calls `deflect_bullets` (only `combat/harness.rs:647`). FIXED in place: recipe coordinates and dagger spawn (px 1290 is rock; the dagger never appeared).

**Symptom / acceptance.** In-engine a powered force projector stays at efficiency 0
(G00/G01 notes on PAR-0129), so no dome; independently the live sim never absorbs bullets
(only the combat harness calls `deflect_bullets`), phase fabric never boosts, coolant never
speeds recovery, and the one live test is vacuous. After: powered projector reaches
efficiency 1, `radscl → 1`; enemy bullets in the hexagon are absorbed and add buildup; phase
fabric grows the radius by up to 80 and is consumed every 350 ticks.

**Java.** `J/world/blocks/defense/ForceProjector.java`:
* constructor `:72-87`: `update = solid = true; hasPower = hasLiquids = hasItems = true;`
  `if(consumeCoolant){ consume(coolantConsumer = new ConsumeCoolant(coolantConsumption)).boost().update(false); }`
  defaults `:32` `phaseUseTime = 350f`, `:39-44` `shieldHealth = 700f, cooldownNormal = 1.75f, cooldownLiquid = 1.5f, coolantConsumption = 0.1f, consumeCoolant = true`.
* `J/content/Blocks.java:1966-1978`: `phaseRadiusBoost = 80f; radius = 101.7f; shieldHealth = 750f; cooldownNormal = 1.5f; cooldownLiquid = 1.2f; cooldownBrokenBase = 0.35f; itemConsumer = consumeItem(Items.phaseFabric).boost(); consumePower(4f);`
* `updateTile` `:207-257` (pasted, order matters):
  ```java
  boolean phaseValid = itemConsumer != null && itemConsumer.efficiency(this) > 0;
  phaseHeat = Mathf.lerpDelta(phaseHeat, Mathf.num(phaseValid), 0.1f);
  if(phaseValid && !broken && timer(timerUse, phaseUseTime / timeScale) && efficiency > 0){ consume(); }
  radscl = Mathf.lerpDelta(radscl, broken ? 0f : warmup, 0.05f);
  ... (reactorsmoke effect)
  warmup = Mathf.lerpDelta(warmup, efficiency, 0.1f);
  if(buildup > 0){
      float scale = !broken ? cooldownNormal : cooldownBrokenBase;
      if(coolantConsumer != null){ if(coolantConsumer.efficiency(this) > 0){ coolantConsumer.update(this); scale *= (cooldownLiquid * (1f + (liquids.current().heatCapacity - 0.4f) * 0.9f)); } }
      buildup -= delta() * scale;
  }
  if(broken && buildup <= 0){ broken = false; }
  if(buildup >= shieldHealth + phaseShieldBoost * phaseHeat && !broken){ broken = true; buildup = shieldHealth; ... }
  if(hit > 0f){ hit -= 1f / 5f * Time.delta; }
  deflectBullets();
  ```
* `:60-70` `shieldConsumer`: `bullet.team != team && type.absorbable && !absorbed && isInRegularPolygon(sides, x, y, realRadius(), shieldRotation, bullet.x, bullet.y)` → `absorb(); hit = 1; buildup += bullet.type.shieldDamage(bullet);`
* `ForceBuild` fields `:166-167`: `broken = true; buildup, radscl, hit, warmup, phaseHeat`.

**Port today.**
* Spec `core/content/registries/blocks/defense.rs:528-545`: `consumes: vec![consume_power(4.0f32)]` only.
* `ForceProjectorBehavior::update_tile` (`core/world/blocks/defense/behaviors.rs:67-74`) calls
  `update_force_projector_state(world, e, &mut state, eff, false, 0.0)` — phase false,
  coolant 0, no deflection.
* `update_force_projector_state` (`core/world/blocks/defense/shields.rs:177-226`): updates
  `warmup` **before** `radscl` (Java: radscl first, with the old warmup); `buildup -= scale`
  ignores delta. `deflect_bullets(ctx: &mut CombatCtx, ...)` (`:248`) uses `ctx` only for
  `ctx.world` and `ctx.content.bullet(def)`.
* Behavior state defaults set in `behaviors.rs:493-500` (`shield_health 750, cooldown_normal 1.5,
  cooldown_liquid 1.2`); `ForceProjectorState::default` (`shields.rs:105-125`) radius 101.7,
  `phase_radius_boost 80`, `phase_shield_boost 400`, `cooldown_broken_base 0.35`, `broken: true`.
* Vacuous test `core/sim/combat_tests.rs:3744 force_projector_absorbs_explosion` reads
  `BaseShieldState`.

**Changes.**
1. **Diagnose first.** Test `force_projector_powered_by_adjacent_source`
   (`core/sim/combat_tests.rs`, `live_sim(48)`): `place(&mut sim, 18, 20, "power-source", 1)`,
   `place(&mut sim, 20, 20, "force-projector", 1)` (covers 19..21); tick 60; assert
   `Building.efficiency == 1.0`, `ForceProjectorState.warmup > 0.99`, `radscl > 0.9`
   (`1 - 0.95^60 ≈ 0.954` for radscl — use `> 0.9`). If it fails, print and record
   `Building.efficiency`, `Building.potential_efficiency`, the projector's `PowerModule`
   (status, graph id) and the source's graph id, find the cause (power graph join on
   placement, the consumer list, `has_items` with no item consumer, `should_consume`), fix it
   and say exactly what it was in the commit body. Also run the same in a `runtime_sim()`.
2. Spec (`defense.rs` force-projector): `consumes` in Java order — the constructor's coolant
   first, then Blocks' item and power: `[coolant(0.1, optional/boost, update false), item phase-fabric x1 optional/boost, power 4.0]`.
   Use the port's constructors (`grep -n "fn consume_coolant\|fn consume_optional\|fn consume_items\|fn consume_power" core/content/registries/blocks/*.rs`);
   if `consume_coolant` sets `update: true`, write the `ConsumeDef::Coolant { .. }` literal with
   `update: false`. Regenerate `parity/golden_content.json` (+ manifest sha) — reason:
   `ForceProjector.java:85-86`, `Blocks.java:1976-1977`.
3. `shields.rs update_force_projector_state(world, e, state, eff, phase_valid, coolant_heat_capacity, delta)`:
   add a `delta: f32` param (`crate::world::update::delta(world, e)` at the caller); order:
   phase_heat, radscl (old warmup), warmup, buildup `-= delta * scale`, broken checks,
   `hit -= delta / 5`. Use `lerp_delta` with delta if the helper supports it (check its
   signature; `Mathf.lerpDelta(a,b,p) = lerp(a, b, 1 - pow(1-p, delta))`).
4. `deflect_bullets_world(world: &mut World, content: &ContentRegistry, state, team, x, y)`:
   move the body of `deflect_bullets` there; `deflect_bullets(ctx, …)` calls it. Bullet id
   collection stays "collect first"; iterate the collected `Vec<Entity>` sorted by
   `Entity::index()` (determinism).
5. `ForceProjectorBehavior::update_tile`:
   * `phase_valid` = the item module holds ≥ 1 `phase-fabric` (resolve the id once in the
     registration and store `phase_item: Option<ItemId>` on the behavior; `register` has a
     content reference — check).
   * phase consume: `phase_valid && !state.broken && eff > 0 && run_timer(world, e, 1, 350.0 / time_scale)`
     → remove 1 phase fabric (check the timer slot 1 is unused by this behavior; `Timers` length).
   * coolant: if `state.buildup > 0` and the liquid module has a coolant-eligible current liquid
     with amount ≥ `0.1 * edelta`: remove `0.1 * edelta` of it and pass its heat capacity
     (`LiquidTraits.heat_capacity`). "Current liquid" = T10's `LiquidModule::current_liquid()`
     if T10 landed; otherwise the liquid with the largest amount (comment `TODO(T10)`).
   * then bullets: `if let Some(rt) = world.remove_non_send_resource::<UnitRuntime>()` — copy
     the exact take/put-back calls from `bullet_update_system` (`core/sim/unit_runtime.rs`,
     grep `remove_non_send`) — `deflect_bullets_world(world, &rt.content, ...)`, then put it
     back on every path.
6. `core/sim/combat_tests.rs:3744`: change `BaseShieldState`/`smooth_radius` to
   `ForceProjectorState` and `real_radius()`, and replace the early `return` with an
   `assert!(radius > 90.0)` (after step 1 the projector charges).
7. `gdext/sim_host.rs building_debug`: add `force_radscl`, `force_buildup`, `force_broken`,
   `force_phase_heat` from `ForceProjectorState` (pattern: the `SandboxState` block in the same
   function). Afterwards run `grep -nE '///.*\S\s{2,}#\[' client/rust/mind-gdext/src/sim_host.rs` (must print nothing).

**Tests.** Step 1 test; `force_projector_absorbs_enemy_bullet` (`combat_tests.rs`, `live_sim(48)`):
projector team 1 at (20,20) + power-source (18,20); tick 120; spawn a team-2 `dagger` at world
(20*8+60, 20*8+4) facing 180; tick until a dagger bullet exists (≤ 300 ticks), then 60 more;
assert `ForceProjectorState.buildup > 0.0` or `hit > 0.0`. `force_projector_phase_boost`: add 2
phase fabric to the projector's `ItemModule`; tick 30 → `phase_heat > 0.9`; tick 360 → phase
fabric count 1. Fixed `force_projector_absorbs_explosion` passes.

**Verify in-engine.** Ground Zero (Go). (The original (150,62)/(148,62) are static walls and
the original dagger spawn px 1290 is inside rock beyond `limit_area`: the unit never appeared.)
```bash
$G --eval "return $H.place_block(140,62,\"force-projector\")" --eval "return $H.place_block(138,62,\"power-source\")"
$G --eval "return $H.tile_debug(138,62).name"          # power-source
$G --eval "$H.set_paused(true); return $H.step(120)" --eval "return $H.building_debug(140,62)"
```
Expect `efficiency` 1.0 and `force_radscl` > 0.9. Then
`apply_sim_command_json('{"op":"spawn_unit","unit":<dagger index>,"team":2,"px":1120,"py":640}')`
(dagger index from `content_list("unit").find("dagger")` = 0; (1120,640) = tile (140,80), free,
outside the 101.7 px dome; the dagger walks toward the core and its shots cross the dome),
`step(300)` in chunks, `building_debug(140,62)` → `force_buildup` > 0 or the dagger died.
Screenshot for H03 (dome draw is H03/PAR-0129 render half). Place 2 `lancer`s fed by
adjacent `power-source`s near (126,60) beforehand if you leave the sim unpaused: spawned
team-2 daggers otherwise attack the core.

**Pitfalls.** Never hold `UnitRuntime` across a `return`. `lerp_delta` semantics must match
Java at delta 1. Golden regeneration must be explained in the commit body.

**Commit.** `PAR-0129: force projector charges with power, absorbs live bullets, phase boost and coolant`

---

### T8. Drill per-block boost intensity and warmup speed

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (headless by design). Code CONFIRMED: `production.rs:594` `(1.0 + 0.6 * optional)`, `:621/625` `approach(.., 0.015)` without delta.

**Symptom / acceptance.** Water boost on a blast drill gives ×1.6 instead of ×1.8 (impact
drill ×1.6 instead of ×1.75); blast drill warms up at 0.015/tick instead of 0.01; drill warmup
ignores overdrive/time scale. After: Java values.

**Java.** `J/world/blocks/production/Drill.java:38` `liquidBoostIntensity = 1.6f`; `:40`
`warmupSpeed = 0.015f`; `:301-311`:
```java
float speed = Mathf.lerp(1f, liquidBoostIntensity, optionalEfficiency) * efficiency;
lastDrillSpeed = (speed * dominantItems * warmup) / delay;
warmup = Mathf.approachDelta(warmup, speed, warmupSpeed);
progress += delta() * dominantItems * speed * warmup;
... else { lastDrillSpeed = 0f; warmup = Mathf.approachDelta(warmup, 0f, warmupSpeed); return; }
```
`J/content/Blocks.java`: blast drill (`blastDrill = new Drill("blast-drill")`, ~2918)
`warmupSpeed = 0.01f` (`:2929`), `liquidBoostIntensity = 1.8f` (`:2933`); impact drill
(`:3089`) `liquidBoostIntensity = 1.75f` (`:3103`). `Mathf.approachDelta(from, to, speed)` =
`approach(from, to, speed * Time.delta)`.

**Port today.** `core/world/behavior/production.rs` ~594: `let speed = (1.0 + 0.6 * optional) * efficiency;`
~621/625: `approach(state.warmup, speed, 0.015)` / `approach(..., 0.0, 0.015)` (no delta).
`DrillDef` (`core/world/block_kind_data.rs:81-94`) has no boost/warmup fields.

**Changes.**
1. `DrillDef`: add `liquid_boost_intensity: f32` and `warmup_speed: f32`; every constructor of
   `DrillDef` sets Java defaults 1.6 / 0.015 (`grep -rn "DrillDef {" client/rust/mind-core/src`).
   `apply_vanilla_knobs` (same file): `"blast-drill"` → 1.8 / 0.01; `"impact-drill"` (the
   `BurstDrillDef.drill`) → 1.75 (warmup: read `BurstDrill.java` — it may override
   `warmupSpeed`; use what Java says).
2. `production.rs`: `speed = lerp(1.0, def.liquid_boost_intensity, optional) * efficiency`;
   warmup `approach(state.warmup, target, def.warmup_speed * delta)`. Apply the same to the
   burst path if it uses the hard-coded numbers (grep `0.015` and `0.6 *` in the file).
3. If `parity/golden_content.json` contains drill knobs, regenerate and explain (`Blocks.java:2929,2933,3103`).

**Tests** (`production_tests.rs`): `blast_drill_water_boost_is_1_8`: compute the expected
speed directly from the behavior helper if one exists, otherwise run a blast drill on 16
copper ore tiles with power + water for 600 ticks and with power only for 600 ticks; assert
the ratio of produced items is within ±5% of 1.8 (warmup included: run 1200 ticks each and
compare the last 600). `drill_warmup_scales_with_time_scale`: set `Building.time_scale = 2.0`
each tick; warmup reaches 1.0 in half the ticks.

**Verify in-engine.** Headless only — the in-engine difference needs a blast drill on a
Serpulo ore field with water and minutes of play; the unit test is exact.

**Commit.** `Gameplay H02 T8: drills use per-block liquid boost and warmup speed`

---

### T9. Overdrive: `Block.canOverdrive` and the `applyBoost` duration rule

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (headless by design). Code CONFIRMED: `projectors.rs:80-85 apply_boost` maxes both fields; no `can_overdrive` anywhere.

**Symptom / acceptance.** Overdrive projectors speed up blocks Java excludes (phase conduit,
payload conveyors/routers, world processor, landing pads) and refresh a stronger boost's
duration with a weaker boost. After: Java.

**Java.** `J/world/Block.java:314` `public boolean canOverdrive = true;`; false for
`phase-conduit` (`Blocks.java:2394`), `payload-conveyor` (`:6628`), `payload-router`
(`:6633`), `reinforced-payload-conveyor` (`:6639`), `reinforced-payload-router` (`:6649`),
`world-processor` (`:7001`), and class `LandingPad` (`J/world/blocks/campaign/LandingPad.java:69`).
`J/entities/comp/BuildingComp.java:467-474`:
```java
public void applyBoost(float intensity, float duration){
    if(!block.canOverdrive) return;
    //do not refresh time scale when getting a lower intensity
    if(intensity >= this.timeScale - 0.001f){ timeScaleDuration = Math.max(timeScaleDuration, duration); }
    timeScale = Math.max(timeScale, intensity);
}
```
`OverdriveProjector.java:110`: `indexer.eachBlock(this, realRange, other -> other.block.canOverdrive, other -> other.applyBoost(realBoost(), reload + 1f));`
Also `EmpBulletType.java:24` checks `canOverdrive` (H01).

**Port today.** `core/world/blocks/defense/projectors.rs:80-85 apply_boost` always maxes both;
no `can_overdrive` anywhere.

**Changes.** Add `can_overdrive: bool` to `BlockDef` (default true; in the spec struct
`Option<bool>` like the neighbours at `content/registries/blocks/mod.rs` ~1238-1358) and set
`Some(false)` on the six specs above and every `LandingPad`-kind spec (or derive false for
`BlockKind::LandingPad` in the lowering). `apply_boost` gains the Java guard + duration rule;
`update_overdrive_projector` filters targets with `can_overdrive` (read `BlockTable` def).
Regenerate the content golden (reason: `Block.java:314` + the Blocks lines).

**Tests** (`core/world/blocks/defense/behaviors.rs` tests, next to
`overdrive_projector_boosts_nearby_building`): `overdrive_skips_phase_conduit`;
`apply_boost_keeps_duration_for_weaker_boost` (boost 2.5 dur 100, then 1.5 dur 500 →
`time_scale_duration == 100`, `time_scale == 2.5`).

**Verify in-engine.** Headless only (no overdrive-relevant observable seam besides
`building_debug.time_scale`; optional: place `overdrive-projector` + power-source +
`phase-conduit` on Ground Zero and read `building_debug(x,y).time_scale` == 1).

**Commit.** `Gameplay H02 T9: canOverdrive and the applyBoost duration rule`

---

### T10. Liquids: current liquid, one liquid per conduit/router/bridge, mixing reactions

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (headless by design). Code CONFIRMED: `LiquidModule { liquids, current_amount (total), flow }` (`world/modules/mod.rs:177`).

**Symptom / acceptance.** A conduit holding water accepts oil (Java refuses until the current
amount < 0.2); hot + flammable liquids meeting never damage the blocks, hot + cold never
evaporate. Root cause: `LiquidModule` has no "current liquid" (`current()` returns the total).
After: `LiquidModule` tracks Java's `current`; acceptance and reactions follow Java.

**Java.**
* `J/world/modules/LiquidModule.java:29` `private Liquid current = content.liquid(0);`
  `:89-91` `current()`; `:106-108` `currentAmount() = liquids[current.id]`;
  `:118-125` `add(liquid, amount){ liquids[liquid.id] += amount; current = liquid; ... }`;
  `:133-136` `remove(liquid, amount){ add(liquid, Math.max(-amount, -liquids[liquid.id])); }`
  (so remove also sets current); `:93-97` `reset`; `:99-104` `set(liquid, amount){ if(amount >= liquids[current.id]) current = liquid; liquids[liquid.id] = amount; }`;
  `read` `:182-197`: clears, reads `(id, amount)` pairs, `if(amount > liquids[current.id]) current = liq;`.
* `J/world/blocks/liquid/Conduit.java:241-245`:
  `return (liquids.current() == liquid || liquids.currentAmount() < 0.2f) && (tile == null || source == this || (source.relativeTo(tile.x, tile.y) + 2) % 4 != rotation);`
* `J/world/blocks/liquid/LiquidRouter.java:41-43`: `return (liquids.current() == liquid || liquids.currentAmount() < 0.2f);`
* `J/world/blocks/distribution/ItemBridge.java:447-452`:
  `hasLiquids && team == source.team && (!noAcceptDisabled || enabled) && (liquids.current() == liquid || liquids.get(liquids.current()) < 0.2f) && checkAccept(source, world.tile(link));`
* `J/entities/comp/BuildingComp.java:943-983 moveLiquid` (else-branch, pasted):
  ```java
  }else if(!next.block.consumesLiquid(liquid) && next.liquids.currentAmount() / next.block.liquidCapacity > 0.1f && fract > 0.1f){
      float fx = (x + next.x) / 2f, fy = (y + next.y) / 2f;
      Liquid other = next.liquids.current();
      if(other.blockReactive && liquid.blockReactive){
          if((other.flammability > 0.3f && liquid.temperature > 0.7f) || (liquid.flammability > 0.3f && other.temperature > 0.7f)){
              damageContinuous(1);
              next.damageContinuous(1);
              if(Mathf.chanceDelta(0.1)){ Fx.fire.at(fx, fy); }
          }else if((liquid.temperature > 0.7f && other.temperature < 0.55f) || (other.temperature > 0.7f && liquid.temperature < 0.55f)){
              liquids.remove(liquid, Math.min(liquids.get(liquid), 0.7f * Time.delta));
              if(Mathf.chanceDelta(0.2f)){ Fx.steam.at(fx, fy); }
          }
      }
  }
  ```
  The else-branch is reached when the first `if` (`flow > 0 && ofract <= fract && next.acceptLiquid(...)`)
  fails, still inside `if(next.team == team && next.block.hasLiquids && liquids.get(liquid) > 0f)`.
* `J/entities/comp/HealthComp.java:91-93`: `damageContinuous(amount) = damage(amount * Time.delta, ...)`.
* `J/type/Liquid.java:38-48`: `flammability`, `temperature = 0.5f`, `heatCapacity = 0.5f`, `blockReactive = true`.

**Port today.** `core/world/modules/mod.rs:186-232` `LiquidModule { liquids, current_amount (TOTAL), flow }`;
`add`/`remove` clamp and adjust the total; `current()` returns the total.
`core/world/blocks/liquid/movement.rs:95-116 accept_liquid` (no current rule);
`:345-399 move_liquid_impl` (no else-branch). `LiquidTraits` (`core/world/modules/mod.rs:295`)
lacks `block_reactive` (field exists on `LiquidDef`, `content/registries/liquids.rs:88`).
Save read: `core/world/building_io.rs` ~412 writes slots directly.
`power/sandbox.rs:52-54,63` writes slots directly.

**Changes.**
1. `LiquidModule`: add `pub current_liquid: LiquidId` (default `LiquidId::new(0)`), keep
   `current_amount` (total) and `current()` as they are (many callers use the total — do not
   rename). Add `pub fn current_liquid(&self) -> LiquidId` and
   `pub fn current_liquid_amount(&self) -> f32 { self.get(self.current_liquid) }`.
   `add(..)`: set `current_liquid = liquid` (Java sets it even for 0/negative amounts — do the
   same unconditionally). `remove(..)`: also set `current_liquid = liquid`. Add `set(liquid, amount)`
   per Java if any port code needs it (sandbox source: replace the direct slot write at
   `power/sandbox.rs:52-54` with a call that sets current).
   Save read (`building_io.rs` ~412): after reading each `(id, amount)`, apply
   `if amount > module.get(module.current_liquid) { module.current_liquid = id }`.
   If `LiquidModule` is serialized elsewhere (network state, `world/network_state.rs`), do the
   same there. Check `#[derive]`s: if it derives `PartialEq`/`Hash` used by checksums, adding
   a field changes checksums — run `parity goldens` and explain any regenerated golden.
2. `LiquidTraits`: add `block_reactive: Vec<bool>` filled from `LiquidDef.block_reactive`.
3. `accept_liquid` (`movement.rs:95`): for nodes that are conduits, routers/tanks and bridges
   (`!node.junction`; check which `LiquidNode` flags identify them — junctions forward and
   must not apply the rule), require
   `module.current_liquid() == liquid || module.current_liquid_amount() < 0.2`. Liquid bridge:
   also `team == source team` (already?) — keep `checkAccept` as the port has it.
4. `move_liquid_impl`: add the else-branch exactly as Java (inside the `same_team && stored > 0`
   block, when the transfer condition fails): condition
   `!block_consumes_liquid(next, liquid) && next_module.current_liquid_amount() / next_capacity > 0.1 && fract > 0.1`
   (`helpers::block_consumes_liquid`), `other = next_module.current_liquid()`, traits from
   `LiquidTraits`; damage both with `crate::combat::damage::area::apply_health(world, e, 1.0 * delta)`
   (check that helper damages buildings; grep its body) ; steam branch removes
   `min(get(liquid), 0.7 * delta)` from **self**. Effects (fire/steam FX) are H03's: skip them
   but leave a comment. Note `move_liquid_impl` gets `next == entity` guard; keep it.
5. Turret coolant / repair turret / force projector code that guesses "current" (first
   non-zero slot, e.g. `core/world/blocks/units/mod.rs:587-603`) should switch to
   `current_liquid()`; do it for the ones you touch only (list the rest in the summary).

**Tests** (`core/world/blocks/liquid/tests.rs`): `module_tracks_current_liquid` (add water 5,
add oil 1 → current oil; remove water 1 → current water); `conduit_refuses_second_liquid`
(conduit with water 5 → `accept_liquid(oil)` false; drain to 0.1 → true);
`router_refuses_second_liquid`; `slag_into_oil_conduit_damages` (conduit A with slag flowing
into conduit B holding oil > 10% of capacity; after 60 ticks both lost ≥ 59 health — Java
`damageContinuous(1)` per tick; use the conduits' health 45? read conduit health first and
assert `<` max); `hot_into_cold_evaporates` (slag into water: source loses slag).

**Verify in-engine.** Headless only for reactions (no liquid seam that stages two liquids
cheaply). Optional: `liquid_debug(x,y)` after placing a `liquid-source` of water → conduit and
configuring the source to oil; the conduit keeps water until it drains.

**Pitfalls.** `current_amount` naming is confusing: do not change its meaning (UI and
liquid bars read it). Java's `current` defaults to liquid 0 (water) even when empty.

**Commit.** `Gameplay H02 T10: liquid modules track the current liquid; one liquid per conduit; mixing reactions`

---

### T11. `unitOn`: shock mines trigger, conveyors push ground units

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Shock mine (124,75) + team-2 dagger spawned on it and walking over: health 150 unchanged after 40 ticks; team-1 dagger at (1064,544) on running `titanium-conveyor`s (132..137,68) rot 0: x 1064.0 after 60 ticks. `shock_mine_unit_on` is only called from tests (`shields.rs:808,812`). FIXED in place: conveyor x range (145..150 hits rock) and spawn points.

**Symptom / acceptance.** Enemy ground units walk over shock mines with no effect; conveyors
do not move units. After: a dagger on an enemy shock mine triggers 4 lightning tendrils
(damage 25, length 10) at most every 80 ticks and the mine takes 7 damage; a dagger on a
running conveyor drifts along it.

**Java.**
* `J/entities/comp/UnitComp.java:832-845`:
  ```java
  if(tile != null && tile.build != null){ tile.build.unitOnAny(self()); }
  if(tile != null && isGrounded() && !type.hovering){
      if(tile.build != null){ tile.build.unitOn(self()); }
      if(floor.damageTaken > 0f){ damageContinuous(floor.damageTaken); }
  }
  ```
* `J/world/blocks/defense/ShockMine.java:20` `cooldown = 80f`; `:69-85`:
  ```java
  public void unitOn(Unit unit){ if(enabled && unit.team != team && timer(timerDamage, cooldown)){ triggered(); damage(tileDamage); } }
  public void triggered(){
      for(int i = 0; i < tendrils; i++){ Lightning.create(Bullets.damageLightningGround, team, lightningColor, damage, x, y, Mathf.random(360f), length); }
      if(bullet != null){ ... }
  }
  ```
  `Blocks.java:1980-1988`: shock-mine `health = 50; damage = 25; tileDamage = 7f; length = 10; tendrils = 4;`
  `timer(id, time)` returns true when ≥ `time` ticks passed since the last true (first call true).
* `J/world/blocks/distribution/Conveyor.java:34` `pushUnits = true`; `:227-251`:
  ```java
  if(!pushUnits || clogHeat > 0.5f || !enabled) return;
  noSleep();
  float mspeed = speed * tilesize * 55f; float centerSpeed = 0.1f; float centerDstScl = 3f;
  float tx = Geometry.d4x(rotation), ty = Geometry.d4y(rotation);
  float centerx = 0f, centery = 0f;
  if(Math.abs(tx) > Math.abs(ty)){
      centery = Mathf.clamp((y - unit.y()) / centerDstScl, -centerSpeed, centerSpeed);
      if(Math.abs(y - unit.y()) < 1f) centery = 0f;
  }else{
      centerx = Mathf.clamp((x - unit.x()) / centerDstScl, -centerSpeed, centerSpeed);
      if(Math.abs(x - unit.x()) < 1f) centerx = 0f;
  }
  if(len * itemSpace < 0.9f){ unit.impulse((tx * mspeed + centerx) * delta(), (ty * mspeed + centery) * delta()); }
  ```
  Speeds (`Blocks.java:2073,2081,2095`): conveyor 0.035, titanium 0.0801, armored 0.08;
  `itemSpace = 0.4f`. `PhysicsComp.java:18-25`: `mass() = hitSize² π`; `impulse(x,y)` → `vel += (x,y)/mass`.

**Port today.** `core/world/blocks/defense/shields.rs:520 shock_mine_unit_on(ctx, e, state, team, unit_team)`
(test-only; it *decrements* `timer_damage` on every call — wrong: Java's timer is time-based).
Unit environment pass: `core/entities/comp/unit/environment.rs:84 update_unit_environment(world, grid, content, spawns, entity)`,
called from `core/sim/unit_runtime.rs` ~2451; floor damage at ~281
(`if grounded && floor_damage_taken > 0.0`). `ConveyorBuild` (`distribution/conveyor.rs:35`)
fields `len: u8`, `clog_heat: f32`; `ConveyorBehavior.speed` (`:185-210`). Velocity:
`crate::entities::comp::base::Vel { x, y }`; mass formula used inline at
`core/combat/bullet/mod.rs:1466`.

**Changes.**
1. Trait (`core/world/behavior/mod.rs`): `fn unit_on(&self, world: &mut World, e: Entity, unit: Entity) {}`.
2. `ConveyorBehavior::unit_on`: Java formula; `enabled` from `Building.enabled`;
   `delta` = `crate::world::update::delta(world, e)`; unit position `Pos`, building centre
   `Pos`; hit size from `HitboxComp.hit_size`; `vel += (ix, iy) / (hit_size² π).max(0.0001)`.
   `pushUnits` is always true for vanilla conveyors (armored too — check
   `ArmoredConveyor.java` for an override).
3. Shock mine: keep lightning creation in the unit pass (needs `CombatCtx`). Add a component or
   resource-free approach: `ShockMineBehavior::unit_on` checks `enabled`, team and the timer
   (store `last_trigger: Option<f32>` in `ShockMineState` compared with
   `crate::world::update::build_time(world)`: fire when `None` or `now - last >= cooldown`),
   sets `last_trigger = Some(now)`, applies `tile_damage` to itself
   (`crate::combat::damage::area::apply_health(world, e, 7.0)` — check it handles buildings),
   and pushes `(e, team)` into a new resource `PendingShockMineTriggers(Vec<(Entity, u8)>)`.
   Replace `shock_mine_unit_on`'s decrement logic with a `fn shock_mine_trigger(ctx, e, state_or_knobs, team)`
   that only creates the tendrils; drain `PendingShockMineTriggers` in the unit pass where a
   `CombatCtx` is available (right after the unit loop in `unit_update_system`; find where
   `CombatCtx` is built in `core/sim/unit_runtime.rs`), in vector order.
4. Call site: `update_unit_environment` (`environment.rs`), right before the floor damage
   block: if `grounded && !def.hovering` and the tile under the unit centre has a building in
   `grid` (`grid.entity_at(TilePos)` — `grid` here is the `UnitRuntime` snapshot; the entity ids
   are the live ones), resolve its behavior through `BlockTable` and call `unit_on`. Also make
   the floor-damage branch require `!def.hovering` (Java).

**Tests** (`core/sim/combat_tests.rs`): `shock_mine_zaps_enemy_dagger`: `live_sim(48)`;
`place(&mut sim, 20, 20, "shock-mine", 1)`; dagger team 2 spawned at (164,164) (tile centre);
zero its `Vel` each tick; after 2 ticks the dagger lost health and mine health = 50 − 7 = 43;
after 79 more ticks still 43; after 81 total since the first trigger, 36.
`conveyor_carries_dagger`: titanium conveyors at (10..20, 20) rotation 0 (team 1), dagger
team 1 at world (12*8+4, 164) with no command; tick 60; x increased by > 8.
`hovering_unit_ignores_mine` (any hovering ground type, e.g. `elude`? check
`UnitTypeDef.hovering` users) — optional.

**Verify in-engine.** Ground Zero (Go): `place_block(140,62,"shock-mine")`; spawn a team-2
dagger at px 1120, py 496 (the mine's tile centre; `apply_sim_command_json` spawn_unit, dagger
index 0); `set_paused(true)`, `step(10)`; `unit_debug_json()` shows that dagger's `health`
below 150 (Java dagger health is 150); `building_bars_json(140,62)` health value < 1.0.
Conveyor: place `titanium-conveyor` (138..143, 66) rot 0 with `place_block_rot` (the original
x=150 is a static wall), spawn a team-1 dagger at (1112, 528) (tile 139,66), step 60,
`unit_debug_positions()` x increased by > 8. (Before the fix, verified 2026-10-14 with a mine
at (124,75) and belts at (132..137,68): the team-2 dagger walked over the mine with health
150 unchanged; the team-1 dagger on the running belt stayed at x 1064.0 for 60 ticks.)
Spawned team-2 daggers attack the core when unpaused: keep the sim paused or place two
powered lancers first.

**Pitfalls.** `update_unit_environment` is H01's file area; keep the hunk small and say so in
the commit. Determinism: process units in the existing loop order; drain triggers in push
order; lightning angles from `ctx.rng` (as the existing function does).

**Commit.** `Gameplay H02 T11: unitOn hook for shock mines and conveyors`

---

### T12. Wall bullet reactions: surge lightning, phase deflection

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (headless by design). Code CONFIRMED: `WallDef.lightning_chance` (`block_kind_data.rs:188`) is never read.

**Symptom / acceptance.** Surge walls never arc lightning; phase walls never reflect bullets.
After: surge-wall hit → 5% chance of lightning (damage 20, length 17) back along the bullet;
phase-wall → reflects a reflectable bullet with chance `10 / bullet.damage`.

**Java.** `J/world/blocks/defense/Wall.java:21-31` (`lightningChance = -1f, lightningDamage = 20f, lightningLength = 17, chanceDeflect = -1f, flashHit`),
`:164-208` `collision(bullet)`:
```java
super.collision(bullet);
hit = 1f;
if(lightningChance > 0f){ if(Mathf.chance(lightningChance)){ Lightning.create(team, lightningColor, lightningDamage, x, y, bullet.rotation() + 180f, lightningLength); lightningSound.at(...); } }
if(chanceDeflect > 0f){
    if(bullet.vel.len() <= 0.1f || !bullet.type.reflectable) return true;
    if(!Mathf.chance(chanceDeflect / bullet.damage())) return true;
    deflectSound.at(...);
    bullet.trns(-bullet.vel.x, -bullet.vel.y);
    float penX = Math.abs(x - bullet.x), penY = Math.abs(y - bullet.y);
    if(penX > penY){ bullet.vel.x *= -1; }else{ bullet.vel.y *= -1; }
    bullet.owner = this; bullet.team = team; bullet.time += 1f;
    return false;
}
return true;
```
Caller `J/entities/comp/BulletComp.java:269-293`: `remove = build.collision(self())` (only for
`build.team != team`); `if(remove || type.collidesTeam){ ... hit = true; doRemove = true }`;
`type.hitTile(...)` runs regardless; `if(doRemove) remove();`.
Blocks (`Blocks.java` ~1759-1786): phase-wall / phase-wall-large `chanceDeflect = 10f; flashHit = true;`
surge-wall / surge-wall-large `lightningChance = 0.05f;`.

**Port today.** `WallDef { autotile, lightning_chance, lightning_damage, lightning_length }`
(`core/world/block_kind_data.rs:184-193`), never set or read. Bullet→building:
`core/combat/bullet/mod.rs:516 hit_building`, call sites ~1050 and ~1131 (both followed by
`hit_tile` and removal/pierce logic). `BulletDef.reflectable` exists
(`content/registries/bullets.rs:255`). Lightning: `crate::combat::lightning::create(ctx, team, color, damage, x, y, rotation, length)` (`core/combat/lightning.rs:48`).

**Changes.** Add `chance_deflect: f32` to `WallDef` (default -1); knobs in
`apply_vanilla_knobs` (phase walls 10; surge walls lightning 0.05/20/17). New
`fn wall_collision(ctx, bullet, target) -> bool` (`true` = Java "remove") in
`combat/bullet/mod.rs` next to `hit_building`: reads the target's `BlockKindData::Wall`
(via `BlockTable`), runs the Java branches with `ctx.rng.chance(RngStream::Sim, p)`; deflect
mutates the bullet: position `-= vel`, flip the axis per `penX > penY`, `TeamComp.team = wall
team`, owner = wall entity (check the bullet owner field name), `time += 1`. At both call
sites: `let remove = if !same_team { hit_building(..); wall_collision(ctx, bullet, target) } else { true };`
and when `remove == false` skip the hit/removal branch for this building (still run
`hit_tile` per Java). Read how each call site decides removal before editing.

**Tests** (`combat_tests.rs`): `phase_wall_reflects_low_damage_bullet` (dagger bullet damage
20 → chance 0.5; fire ≥ 40 bullets at a team-1 phase wall from a team-2 dagger; assert at least
one bullet changed team to 1); `surge_wall_arcs` (≥ 1 lightning in 100 hits; count lightning
via the lightning result/bullet group).

**Verify in-engine.** Headless only (staging surge/phase walls under fire needs a sustained
enemy and a lightning/bullet-team seam that does not exist).

**Commit.** `Gameplay H02 T12: surge wall lightning and phase wall deflection`
(Shared file with H01: `combat/bullet/mod.rs`; keep the hunk small.)

---

### T13. Save bodies still missing: RepairTurret (+ Erekir assembler/cargo)

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (headless by design). Code CONFIRMED: `impl BuildingBehavior for RepairTurretBehavior` (`world/blocks/units/behavior.rs:841`) has no `version`/`write`/`read`.

**Symptom.** G04's loader reads per-kind bodies; these behaviors have none, so a save with
them misreads every following building. Java bodies:
* `J/world/blocks/units/RepairTurret.java:236-254`: `version() = 1`; write `f rotation`;
  read `if(revision >= 1) rotation = read.f();`.
* `J/world/blocks/units/UnitCargoLoader.java:174-185`: write `i unit == null ? -1 : unit.id`;
  read `readUnitId = read.i()`.
* `J/world/blocks/units/UnitCargoUnloadPoint.java:92-103`: write `s item id or -1`, `bool stale`.
* `J/world/blocks/units/UnitAssembler.java:756-789`: `version() = 1`; write `f progress`,
  `b units.size`, `i unit.id`…, `blocks.write` (PayloadSeq), `TypeIO.writeVecNullable(commandPos)`;
  read mirrors, `commandPos` only for revision ≥ 1.
(LiquidBridge is T6; LandingPad is H04.) Verified missing by scanning every
`impl BuildingBehavior` without `fn write` (2026-10-14).

**Changes.** Implement `version`/`write`/`read` on `RepairTurretBehavior`
(`core/world/blocks/units/behavior.rs:840`; rotation field on `RepairTurret`),
`CargoLoaderBehavior`, `CargoUnloadPointBehavior`, `AssemblerBehavior` (look for existing
PayloadSeq/`write_vec_nullable` helpers: `grep -rn "fn write_vec_nullable\|PayloadSeq" client/rust/mind-core/src`;
if the assembler's `blocks` PayloadSeq has no port codec, implement RepairTurret and cargo only
and record the assembler as blocked with the reason).

**Tests.** `core/world/building_io_kinds_tests.rs`: `repair_turret_body_matches_java`
(rotation 37.5 round trip, revision 1 bytes = base + 4-byte float), `cargo_unload_point_body_matches_java`,
`cargo_loader_body_matches_java`.

**Verify in-engine.** Headless only (body codecs; no Erekir sector in the profile).

**Commit.** `Gameplay H02 T13: save bodies for repair turrets and cargo blocks`

---

### T14. Verification tests for UNVERIFIED early-game blocks

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (verification tests by design).

One test each in `core/sim/production_tests.rs`; fix only small deviations (< 30 lines);
anything bigger becomes a summary note with the failing value.
* `separator_weighted_results`: separator with slag + power for 600 crafts; outputs within
  {copper, lead, graphite, titanium}; frequencies within ±5 percentage points of the Java
  weights (`Blocks.java` `separator` `results = with(Items.copper, 5, Items.lead, 3, Items.graphite, 2, Items.titanium, 2)` — read and paste the exact line).
* `mass_driver_shoots_to_linked`: two mass drivers 20 tiles apart, linked via T5/T6 config,
  power sources, 50 copper into the first; after 600 ticks the second holds or dumped copper.
* `thermal_generator_reads_heat_floor`: thermal generator (2x2) on 4 `hotrock` tiles (read
  `hotrock` `attributes.set(Attribute.heat, ...)` in `Blocks.java`); production per tick =
  `powerProduction * sum(heat)` (read `ThermalGenerator.java` for the exact formula,
  incl. `displayEfficiencyScale`).
* `steam_generator_needs_water`: `steam-generator` with coal but no water → production 0;
  with water → > 0 (Java `consumeLiquid(water, 0.1f)` — confirm).
* `unloader_pulls_from_graphite_press`: press with graphite output + unloader + conveyor →
  graphite leaves (G02 T1 covered crafters generally; this pins the early-game case).
* `bridge_conveyor_throughput`: bridge-conveyor pair 3 apart fed by a titanium conveyor:
  items/sec within ±10% of Java `BufferedItemBridge` (`speed = 74f`, `bufferCapacity = 14` —
  derive the expected rate from `BufferedItemBridge.java` and `ItemBuffer`; if unclear, assert
  only that items flow).
**Verify in-engine.** Headless only (verification tests).
**Commit.** `Gameplay H02 T14: verification tests for separator, mass driver, thermal, steam, unloader, bridge`

---

### T15. Beam node / beam tower (Erekir power transport)

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (Erekir, headless by design). Code CONFIRMED: no behavior registered for `BlockKind::BeamNode` (only `kind_rotates` mentions it).

Lower priority (Erekir). **Symptom.** Beam nodes connect nothing. After: a beam node links the
first same-team powered, `connectedPower`, non-`PowerNode` building in each of the 4
directions within `range` tiles, stopping at insulated buildings; links rebuild when tiles change.

**Java.** `J/world/blocks/power/BeamNode.java:190-196` (`updateTile`:
`if(lastChange != world.tileChanges){ lastChange = world.tileChanges; updateDirections(); }`),
`:241-293` `updateDirections` (pasted head):
```java
for(int i = 0; i < 4; i ++){
    var prev = links[i]; var dir = Geometry.d4[i];
    links[i] = null; dests[i] = null;
    int offset = size/2;
    for(int j = 1 + offset; j <= range + offset; j++){
        var other = world.build(tile.x + j * dir.x, tile.y + j * dir.y);
        if(other != null && other.isInsulated()){ break; }
        if(other != null && other.block.hasPower && other.block.connectedPower && other.team == team && !(other.block instanceof PowerNode)){
            links[i] = other; dests[i] = world.tile(tile.x + j * dir.x, tile.y + j * dir.y); break;
        }
    }
    var next = links[i];
    if(next != prev){ ... unlink prev (remove both power.links entries, new graphs + reflow) ... link next (add both entries, graph.addGraph) }
}
```
Read `:265-293` for the exact unlink/link graph calls before writing. Ranges and buffers:
`grep -n "beamNode = \|beamTower = " -A12 J/content/Blocks.java`.

**Port today.** `BlockKind::BeamNode` → `NoopBehavior`. `grid.tile_changes` (`core/world/mod.rs:147`).
Graph API: `PowerGrids::add_graph` (`power/graph.rs:572`), `reflow` (`:622`),
`nodes::configure_link` (`power/nodes.rs:423`). Template pass: `SimRuntime::refresh_facing_drills`
(`core/sim/runtime.rs:990`, called from `Sim::tick` `core/sim/mod.rs:899`).

**Changes.** `BeamNodeState { links: [Option<Entity>; 4], last_change: i32 }` on the entity;
`SimRuntime.beam_nodes: Vec<Entity>` registered where buildings are spawned
(`set_block_raw`/materialize — follow how `refresh_facing_drills` finds its drills);
`Sim::refresh_beam_nodes()` right after `refresh_facing_drills()`; run `update_directions`
for each node when `grid.tile_changes != state.last_change`. Ranges via a knob table in the
power `register`. Removed nodes: drop from the list when the entity is gone.

**Tests.** `beam_node_links_battery_in_line` (node (5,5), battery (12,5) → same graph; place
`plastanium-wall` (insulated) at (8,5) → separate graphs).

**Verify in-engine.** Headless only (Erekir).

**Commit.** `Gameplay H02 T15: beam nodes link power along four directions`

---

### T16. Turrets re-aim every tick and lead moving targets (`Turret.targetPosition` + `Predict.intercept`) (added after the 2026-10-14 in-engine verification)

**Symptom / acceptance.** On Ground Zero a drill-fed duo spent 25 ammo in ~900 ticks for
~4 hits on two circling flares (verification, H00 T3 row of
`docs/parity-plans/verify-s2-round1.md`). Two port bugs: (1) `state.target_pos` is only
written in `find_target` (on the retarget timer), so between retargets the turret turns
toward and fires at where the target WAS; (2) it never leads: Java aims at
`Predict.intercept(turret, target, bullet.speed)`. Also missing: Java only fires when the
target is within `range() + target.hitSize/1.9`. After: every tick with a valid target the
aim point is the intercept point for the current ammo's bullet speed, and a duo hits a
flare flying a straight line at >= 80 % of shots.

**Java.** `J/world/blocks/defense/turrets/Turret.java`:
* `:84` `accurateDelay = true`, `:86` `moveWhileCharging = true`, `:111` `predictTarget = true`
  (set false only by `scathe`, `Blocks.java:5248`; lancer sets `moveWhileCharging = false`
  and `accurateDelay = false`, `Blocks.java:3594-3595`).
* `:459-479`:
  ```java
  public void targetPosition(Posc pos){
      if(!hasAmmo() || pos == null) return;
      BulletType bullet = peekAmmo();
      var offset = Tmp.v1.setZero();
      //when delay is accurate, assume unit has moved by chargeTime already
      if(accurateDelay && !moveWhileCharging && pos instanceof Hitboxc h){
          offset.set(h.deltaX(), h.deltaY()).scl(shoot.firstShotDelay / Time.delta);
      }
      if(predictTarget && bullet.speed >= 0.01f){
          targetPos.set(Predict.intercept(this, pos, offset.x, offset.y, bullet.speed));
      }else{
          targetPos.set(pos);
      }
      if(targetPos.isZero()){
          targetPos.set(pos);
      }
  }
  ```
* `updateTile` (`:568-611`), inside `if(hasAmmo())`, after `findTarget` on its timer:
  ```java
  if(validateTarget()){
      boolean canShoot;
      if(controlled()){ targetPos.set(unit.aimX(), unit.aimY()); canShoot = unit.isShooting(); }
      else if(logicControlled()){ canShoot = logicShooting; }
      else{
          targetPosition(target);
          if(Float.isNaN(rotation)) rotation = 0;
          canShoot = within(target, range() + (target instanceof Sized hb ? hb.hitSize()/1.9f : 0f));
      }
      if(!controlled()){ unit.aimX(targetPos.x); unit.aimY(targetPos.y); }
      float targetRot = angleTo(targetPos);
      if(shouldTurn()){ turnToTarget(targetRot); }
      if(!alwaysShooting && Angles.angleDist(rotation, targetRot) < shootCone && canShoot){
          wasShooting = true; updateShooting();
      }
  }else{ target = null; }
  ```
* `J/entities/Predict.java:67-77` (`intercept(Position src, Position dst, offx, offy, useSrcVelocity=true, v)`):
  `ddx/ddy` = target `deltaX/deltaY` minus the source's (a building has none), then
  `intercept(src.x, src.y, dst.x + offx, dst.y + offy, ddx, ddy, v)` (`:27-53`).
  `Hitboxc.deltaX()` = `x - lastX` of the last update (`J/entities/comp/HitboxComp.java:42`).

**Port today.** `core/world/blocks/defense/turrets/mod.rs`:
* `update_turret` (~1370-1515): `find_target` on `target_timer`; then
  `let target_rot = angle_to(x, y, state.target_pos.0, state.target_pos.1);` — `target_pos`
  is set only at `find_target`'s end (~1829: `state.target_pos = position(ctx.world, target)...`).
  No `canShoot` range gate.
* `peek_ammo(world, e) -> Option<BulletId>` (~1033); bullet defs via `ctx.content.bullet(id)`
  (`speed` field). `position(world, e)` (~1026).
* A faithful intercept helper already exists: `core/combat/targeting.rs:190`
  `intercept(src, dst, dst_vel, v)` and `:239` `intercept_positions(src, src_vel, dst, dst_vel, use_src_velocity, v)`
  (used by units, `core/sim/unit_runtime.rs:~4400`).
* Target per-tick displacement: `Vel` component (`core/entities/comp/base.rs:65`). After
  H01 T4 lands the units carry a true velocity; if H01 T4 adds a `last position` / delta
  component (`grep -rn "delta_x\|LastPos" client/rust/mind-core/src/entities`), use that
  instead (it is Java's `deltaX`).
* Hit size: `HitboxComp.hit_size` (`core/entities/comp/unit/comp.rs:113`); buildings use
  `block.size * 8` (Java `Building.hitSize()`), check `grep -rn "fn hit_size" client/rust/mind-core/src`.

**Changes.**
1. `TurretConfig`: add `predict_target: bool` (default true), `accurate_delay: bool`
   (true), `move_while_charging: bool` (true); set `predict_target = false` for `scathe`
   and `move_while_charging = false, accurate_delay = false` for `lancer` where
   `config_for` builds per-block configs (grep `"scathe"` / `"lancer"` in this file).
2. New fn `fn target_position(ctx: &CombatCtx<'_>, e: Entity, state: &mut TurretState, config: &TurretConfig, x: f32, y: f32, target: Entity)`
   transcribing `targetPosition`: return when `!has_ammo_state(..)`; bullet speed from
   `peek_ammo_state(state, ctx.world, e)` -> `ctx.content.bullet(id).speed`; target position
   via `position(ctx.world, target)`; target delta = `Vel` of the target (0 for buildings);
   offset when `accurate_delay && !move_while_charging` = `delta * config.shoot.first_shot_delay`;
   when `predict_target && speed >= 0.01` -> `crate::combat::targeting::intercept_positions((x, y), (0.0, 0.0), (tx + ox, ty + oy), delta, true, speed)`
   else `(tx, ty)`; if the result is exactly `(0.0, 0.0)` use `(tx, ty)`.
3. `update_turret`: after the `find_target` block and `if state.target.is_none() { return; }`:
   if the turret is player-controlled or logic-controlled keep today's code paths (grep
   `logic_shooting` / control — do not change them); otherwise call `target_position(..)`
   every tick, compute `can_shoot = dist((x,y), target_pos_actual) <= config.range + hit_size/1.9`
   (distance to the TARGET'S position, not the intercept point; use the turret's effective
   range — if the port has `range()` with ammo `range_change`, use it), and fire only when
   `in_cone && can_shoot` (keep `always_shooting` as today).
4. Leave `find_target` writing `target_pos` (harmless) or remove that line — then make sure
   nothing else reads `target_pos` before the first `target_position` call.

**Tests** (`core/sim/combat_tests.rs`, helpers `live_sim`, `place`, `spawn_live_unit`):
* `turret_leads_moving_target`: duo (team 1) with copper ammo at (10,10); team-2 flare
  spawned 100 px east moving north at constant velocity (set its `Vel` and disable its AI
  the way existing combat tests freeze units — grep `freeze\|no_ai\|AiKind::None` in the
  tests); after one turret update `state.target_pos.1 > flare_y` (aim is ahead), and the
  value equals `intercept_positions(..)` computed in the test.
* `turret_reaims_every_tick`: move the target 20 px between two updates without a retarget
  (set `target_timer` high): `target_pos` follows.
* `turret_holds_fire_out_of_range`: target at `range + hitSize/1.9 + 2` -> no bullet spawned.
* `scathe_does_not_predict`: `predict_target` false -> `target_pos == target position`.

**Verify in-engine.** After H00 (+ H01 T15 if landed):
```bash
# Ground Zero via "Go"; drill-fed duo as H00 T3 (130,48)/(132,48); step 1800 to load ammo
# spawn a team-2 flare 120 px from the duo (index from content_list("unit").find("flare"))
$G --eval "return $H.apply_sim_command_json('{\"op\":\"spawn_unit\",\"unit\":<flare>,\"team\":2,\"px\":1176,\"py\":384}')"
$G --eval "$H.step(600); return $H.get_group_count(\"unit\")"   # flare dead within 600 ticks
```
Record ammo used (duo `building_debug` ammo before/after, or `turret_total_ammo` if that
seam exists) — expect far fewer than the 25-for-4 seen before.

**Pitfalls.** Do not lead with the SHOOTER's velocity (turrets do not move; `src_vel = 0`).
`intercept` already divides by `Time.delta = 1`. Continuous/laser/point-defense/tractor kinds
return before this code — leave them.

**Commit.** `Gameplay H02 T16: turrets re-aim every tick and lead targets (Turret.targetPosition, Predict.intercept)`

---

### Deferred (not tasks; why)

* D1 Fog-of-war placement check (`staticFog && fog && !isDiscovered`) — fog discovery is
  H03/H04's; T3 leaves a comment.
* D2 Campaign unlock gate inside the sim placement path — the runtime registry has no research
  state (`SimRuntime::placement_allowed` uses `def.unlock`, check whether it is synced; H05/H08).
* D3 `Block.environmentBuildable` (planet block lists) — treated as true (T3).
* D4 Erekir residuals: repair tower heal, `ShieldWall.damage` absorption, `BaseShield` unit
  push, `RegenProjector`/`ShockwaveTower` verification, cargo loader item intake.
* D5 Heal suppression (`checkSuppression`) — needs H01's suppression ability.
* D6 Door throttle placement: the port throttles in `DoorBehavior::configured` (Java throttles
  in `tapped`, so logic/schematic configures are not throttled upstream). Low impact; note.

---

## 3. Execution order, dependencies, shared files

Order (by gameplay impact; one commit each):
1. **T1** core incineration (small, every campaign player hits a full core), then **T16** turret aim/lead (every wave fight depends on it).
2. **T2** `sync_build_rules` + `validBreak` → **T3** `validPlace` + `kind_rotates` + preview
   seam → **T4** core upgrade.
3. **T5** wire config types + PAR-0190 + bridge hang → **T6** tap-to-link (needs T5's wire
   `Point2`/`Point2Array`; provides H06's seam).
4. **T7** force projector (PAR-0129 sim half).
5. **T10** liquids (current liquid) — T7's coolant uses it if landed; otherwise T7's fallback.
6. **T8**, **T9** (small).
7. **T11** unitOn, **T12** walls (both touch H01 files — small hunks).
8. **T13** IO, **T14** verification tests, **T15** beam node.

Hot shared files (pull `main` before each; keep hunks small):
* `core/world/build.rs` (T2-T4) — read by H01 BuilderAI, H06 setrule, H08 preview/rebuild.
* `core/world/limits.rs` (T2) — H04/H06 call `sync_build_rules`.
* `core/sim/runtime.rs` (T3 `can_place` flags, T4 `begin_place`, T15 list).
* `core/sim/mod.rs` (T2 `apply_rules_to_world`, T3 place arm, T5 configure arm, T6 `ConfigTap` arm, T15 tick pass).
* `core/determinism/command.rs` (T5 wire tags 7-9, T6 `ConfigTap` command) — append-only.
* `core/world/behavior/mod.rs` (T6 trait method, T11 hook) — every block family.
* `core/world/modules/mod.rs` (T10 `LiquidModule`) — liquids everywhere, IO, network state.
* `gdext/input/desktop.rs`, `gdext/input/mod.rs`, `ui/fragments/block_config_fragment.gd` (T5, T6) — H08 owns input.
* `gdext/sim_host.rs` (T3 `valid_place_preview`, T6 `preview_config_tap`, T7 `building_debug`) —
  after editing run `grep -nE '///.*\S\s{2,}#\[' client/rust/mind-gdext/src/sim_host.rs` (must print nothing).
* `core/entities/comp/unit/environment.rs`, `core/combat/bullet/mod.rs` (T11, T12) — H01's.
* `parity/golden_content.json` + manifest (T7, T8, T9).

---

## 4. Ledger changes to report (do not edit `chains.json`)

| Item | Change |
|---|---|
| PAR-0190 | T5: wire keeps config types; tests prove configure reaches sorter/door/switch/bridge/node → `fixed` (sim side). If screenshots still show unchanged sorter/door sprites, add a note "render invalidation → H03" and keep the item `fixing` assigned to H03. |
| PAR-0188 | No H02 change; T5 summary gives H03 the evidence (config JSON changed, sprite did not). |
| PAR-0129 | T7: sim half (efficiency, absorption, phase, coolant) → note; render half stays H03. |
| PAR-0098 | T2/T3/T4: validPlace/validBreak completion + core upgrade → slice note; status per the remaining H03/H08 halves. |
| PAR-0244 | T5/T6 unblock the bridge half (links can now be staged by tap or `"point"` configure) → note for H03. |
| PAR-0220 | T6 provides `on_configure_build_tapped` + `world::config_tap::config_tap` + `SimCommand::ConfigTap` → note "unblocked for H06". |
| PAR-0209 | No change (H08). |
| New | "Campaign cores never incinerate overflow: belts into a full core back up" → fixed by T1. |
| New | "Placement guard ignores team, deep water, same-block rotation, derelict repair, core radius, visibility, sector bans; validBreak allows enemy buildings and the last core" → T2/T3. |
| New | "Cores cannot be upgraded; core-foundation placeable on free ground" → T4. |
| New | "Bridges/nodes/drivers cannot be linked by tapping; bridge conduits cannot be linked or saved" → T6. |
| New | "LiquidModule has no current liquid: conduits mix liquids, no hot/flammable reactions" → T10. |
| New | "Drill liquid boost/warmup ignore per-block values" → T8. |
| New | "Overdrive affects canOverdrive=false blocks" → T9. |
| New | "Shock mines and conveyor unit transport inert" → T11. |
| New | "Surge/phase walls have no special reaction" → T12. |
| New | "RepairTurret/cargo blocks have no save body" → T13. |
| New | "Beam nodes connect nothing" → T15. |

---

## 5. Done criteria

* Every task T1-T15 committed, or reported blocked/skipped with the exact failing value or
  missing dependency.
* New tests exist and pass; full `mind-core` (`--features msav-import`), `mind-headless`,
  `mind-gdext` suites pass (only the H00-summary baseline failures); clippy + fmt clean;
  `parity goldens` all PASS (regenerated goldens named with the Java reason); GDScript parse
  gate clean after T6.
* In-engine recipes for T1, T2, T3, T4, T5, T6, T7, T11 recorded under
  `runs/<YYYYMMDD>-s<N>-H02-<task>/` (eval outputs as JSON + screenshots where stated); panic
  count 0 after each.
* Summary `REPO/docs/H02-blocks_summary.md` per H-environment §6.

---

## 6. Cross-area seams

Provided by H02 (exact names; other plans reference these):
* **H04, H06** — `pub fn crate::world::limits::sync_build_rules(world: &mut World, rules: &crate::game::rules::Rules, is_campaign: bool)`
  (T2). H04 calls it after any rules swap/load (T2 already wires `core/game/runtime.rs` and
  `Sim::apply_rules_to_world`); H06's `setrule` handler calls it after mutating `Rules`
  mid-sector (or calls `Sim::apply_rules_to_world`, which calls it).
* **H06** — trait method `BuildingBehavior::on_configure_build_tapped(&self, world: &World, grid: &WorldGrid, e: Entity, other: Entity) -> ConfigTapResult`,
  entry `crate::world::config_tap::config_tap(world, grid, selected, other) -> ConfigTapResult`,
  and `SimCommand::ConfigTap { x, y, other_x, other_y }` (T6). `LogicBlockBehavior` implements
  the method (`LogicBlock.java onConfigureBuildTapped`); no second tap mechanism.
* **H08** — `#[func] MindSimHost::valid_place_preview(x: i32, y: i32, block: GString, rotation: i32) -> bool`
  and `Sim::valid_place_player(x, y, block, rot) -> bool` (T3): `gdext/input/desktop.rs:172 valid_place`
  should call it for ghost colours. `#[func] MindInput::close_block_config()` and the
  `config_selected` state in `DesktopBridge` (T6) — H08 keeps them when refactoring input.
  Derelict repair over the B/break rect (`InputHandler.java:2331-2341`) can now use
  `valid_place_query(.., PlaceFlags::PLAYER)` (T3).
* **H01** — `valid_place_query(..., Some(world), ..., PlaceFlags::PLAYER)` for BuilderAI
  derelict/ally rebuild (T3); `BaseBuilderAI` caller left on the wrapper if its input has no
  `&World` (T3 step 6 note). T11 adds a `unit_on` call inside `update_unit_environment` and
  T12 a wall branch in `combat/bullet/mod.rs` — H01 should not duplicate them.
  `EmpBulletType` should honour `BlockDef.can_overdrive` (T9).
* **H03** — `LiquidModule::current_liquid()` (T10) for conduit/tank liquid colours;
  `building_debug` `force_radscl/force_buildup/force_broken/force_phase_heat` (T7) for the dome;
  config-change render invalidation (PAR-0190 remainder, evidence in T5's summary);
  bridge link draw guard (T5 step 5 if the hang is render-side).
* **H04** — per-kind bodies: LiquidBridge (T6), RepairTurret/cargo/assembler (T13); campaign
  load override `core_incinerates = true` must stay in the campaign load path (T1 step 4).
* **H05/H08** — research/unlock state is not in the runtime registry (D2); the config panel's
  item list cannot filter by unlock until a content snapshot with unlocks is available.
