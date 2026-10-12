# Parity plan: building, production/logistics and block config (session 4)

This is an execution plan for the building, production/logistics and block-config
workstream of the Mindustry 1:1 port (Godot 4.7 + Rust). It is self-contained.
Do the work items in the order of the "Execution order" section. Every path is
relative to the worktree root `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session4`
unless it starts with `Mindustry/` (the read-only Java reference at
`/home/clintonnguyen/g/code_examples/Mindustry`).

Abbreviations: `core/` = `client/rust/mind-core/src/`, `gdext/` =
`client/rust/mind-gdext/src/`, `J/` = `Mindustry/core/src/mindustry/`.

---

## 0. Environment and workflow

- **Worktree:** `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session4`, branch
  `parity/session-4`. Work and commit only here. Do not push, do not merge. Do not
  edit `chains.json` or `parity-ledger.json`. Do not edit other `-sessionN`
  worktrees or the main checkout.
- **Read first:** `client/AGENTS.md` and `client/rust/AGENTS.md`. The invariants
  are: `mind-core` has no godot/tokio; GDScript is UI-only (it reads JSON and
  calls `MindInput`/`MindSimHost` funcs, and never mutates sim state); every new
  file starts with
  `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.` and
  `// SPDX-License-Identifier: GPL-3.0-only` (GDScript: `##` comments); workspace
  lints deny `unwrap`/`expect` outside tests; the sim must stay deterministic
  (no wall clock, no HashMap iteration order in sim logic; use `Vec`/`BTreeMap`/`IndexMap`).
- **RAM is tight** (several agents share 14 GB). Run one cargo job at a time and
  always set `CARGO_BUILD_JOBS=6`.
- **Unit tests (fast loop):**
  ```bash
  cd client/rust
  CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib <filter>        # e.g. sim::builder, production_tests
  CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib                 # expect "1858 passed; 0 failed" or more
  CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   # expect no output
  cargo fmt -p mind-core -p mind-gdext
  git checkout client/rust/mind-core/src/io/settings.rs   # cargo fmt reformats this unrelated file; never commit it
  ```
  Put a `#[cfg(test)] mod tests` block at the **end** of a file. Clippy
  rejects items that come after a test module (`items_after_test_module`).
- **Full build for the editor:** run `CARGO_BUILD_JOBS=6 tools/build.sh` from the worktree root.
  It takes about 1 min and prints `Finished ... sync_scenarios: synced 11 file(s)`. The
  `.so` is mmap'd, so after every build restart the editor and game with
  `tools/session-play.sh 4`, which takes about 60 s. To only replay the scene, use
  `tools/session-play.sh 4 --play`.
- **Driving the game:** use `export OPEN_GODOT_MCP_PORT=7000` and
  `python3 tools/gmcp.py --eval '<expr>'`. Every eval must be a single
  `return ...` statement. A bad eval, such as one that calls a non-`#[func]` method
  like `player_unit_position()`, pins the frame loop. When that happens,
  `get_tick()` stops advancing. Recover with `tools/session-play.sh 4 --play`.
  For screenshots, use `python3 tools/gmcp.py --screenshot runs/20261012-session4/<name>.png`
  and view the PNG. Note: on display :13 the world currently renders mostly
  white (software GL, which is the rendering agent's domain). Assert through
  state accessors, not pixels.
- **Enter Ground Zero (sector 15):**
  ```bash
  H='get_node("/root/Spine/SimHost")'
  python3 tools/gmcp.py --eval "$H.load_sector(\"serpulo\", 15); return true" \
    --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 15)' \
    --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
  ```
  The player (alpha) spawns at world (1032, 440) = tile (129, 55). The core starts
  with **0 items**, which is correct: the tutorial objective is "obtain 15 copper".
- **Playtest seams added in this workstream** (all on `/root/Spine/SimHost`):
  - `debug_add_core_items("copper", 200)` adds items to the first player core and
    returns the amount accepted. A negative amount removes items.
  - `core_items_json()` returns `{"copper":194,...}`.
  - `apply_sim_command_json('{"op":"place","x":133,"y":55,"block":216,"player":0}')`
    places a block. The `"player":N` field routes the command through the player
    unit's build queue, exactly like mouse input. Without it the placement is instant
    (API path). Use `"op":"break"` the same way. The block id is the index in
    `content_list("block")`, for example `return $H.content_list("block").find("copper-wall")`
    returns 216 and `mechanical-drill` returns 325.
  - `player_build_plans_json()` returns
    `[{x,y,rotation,block,breaking,progress,stuck}]` (the head entry is the active plan).
  - `construct_progress(x, y)` returns `{block, progress, deconstructing}`, or `{}`
    when the tile is not under construction.
  - `place_block(x, y, "name")` and `break_block(x, y)` are instant API placement.
  - Other funcs: `power_debug(x,y)`, `liquid_debug(x,y)`, `get_tick()`,
    `selectable_units_json()`, `has_player_unit()`.
- **Headless:** `client/bin/rust/debug/mind-headless --help` (subcommands `blocks`,
  `sim`, `run`, ...). Most verification in this plan is done with `mind-core` unit tests
  that build a `Sim` with `install_block_runtime()` (see `core/sim/production_tests.rs`
  and `core/sim/builder.rs` tests for the pattern: `Sim::new(seed, w, h, BlockId::AIR, BlockId::AIR)`,
  `sim.grid.fill(stone, AIR)`, `sim.install_block_runtime()`, place via
  `sim.command(SimCommand::Place{.., player: None})`, `sim.tick()`, and set a floor with
  `sim.grid.tiles.get_mut(x, y).floor = id`).
- **Evidence:** save screenshots/JSON under `runs/20261012-session4/` (shared `runs/`).
- **Commits:** commit each item separately as `Fixes PAR-XXXX: <summary>`, add a
  body that explains the change, and end with the line
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. For a partial slice use
  `PAR-XXXX: <summary>`. Run the full `mind-core` tests and clippy before every commit.

---

## 1. Already done (do not redo)

| Commit | Item | What landed |
|---|---|---|
| `c2941ae` | PAR-0098 (core) | Builder-unit construction. `core/sim/builder.rs` (new): `BuilderPlans` component on the player unit, `UnitBuildPlan`, a port of `BuilderComp.updateBuildLogic` (`update_builder`), `validatePlans`, `shouldSkip`, range/rotation of the queue, `Build.beginPlace`/`beginBreak` via `SimRuntime::begin_place`/`begin_break` (`core/sim/runtime.rs`), and `ConstructState::{set_construct,set_deconstruct,construct_step,deconstruct_step}` with the `checkRequired` item-accumulator economy (`core/world/construct.rs`, `BuildEconomy`, `CoreInventory`). `Sim::tick` calls `update_builders()` after the schedule. `SimCommand::Place/Break` with `player: Some(_)` queues plans when `player_can_build()` (`core/sim/mod.rs`); `player: None` stays instant. Desktop input (`gdext/input/desktop.rs` `commit_place`, `right_release`) now sends `player: Some(0)`. The block renderer (`gdext/render/blocks.rs`) draws a construct block as a tinted ghost of the target block (`MindSimHost::construct_at`). 4 unit tests in `sim::builder::tests`. |
| `a2451de` | PAR-0098 (seams) | `debug_add_core_items`, `player_build_plans_json`, `construct_progress`, and the `player` field on `apply_sim_command_json`. Verified in-engine on Ground Zero: a wall plan went through `build1`, completed in about 0.2 s and drew 6 copper (200 to 194). A player break refunded 3. |
| `5a79e4d` | PAR-0075 | `ConsumeGeneratorBehavior`/`HeaterGeneratorBehavior` (`core/world/blocks/power/behavior.rs`) port `acceptItem`/`handleItem`/`acceptStack`/`handleStack`/`acceptLiquid`/`handleLiquid` with the filter consumers folded into `itemFilter`/`liquidFilter`. Shared helpers in `core/world/behavior/helpers.rs`: `block_consumes_item`, `block_consumes_liquid`, `liquid_capacity`, `default_handle_liquid`. Test: `sim::production_tests::combustion_generator_accepts_coal_and_generates`. |
| `217124e` | PAR-0078 | Pumps read the floor (`PumpState.liquid_drop/amount`, `refresh_pump_floor` called from `refresh_drill_ore`), pump `min(cap - stored, amount*pumpAmount*edelta)`, and `dumpLiquid` every tick. `Pump.canPlaceOn` lives in `core/world/build.rs::pump_can_place_on`. `Block.supportsEnv(rules.env)` uses the new `BuildRules.env` (Java bits, default 233, synced from session rules in `core/game/runtime.rs`), so water-extractor is placeable. Liquid movement (`core/world/blocks/liquid/movement.rs`) now works for non-`LiquidNode` buildings: `node_capacity()`, and behavior-dispatched `accept_liquid`/`handle_liquid`. Crafters accept and handle consumed liquids and dump output liquids. SolidPump/Fracker accept consumed items/liquids and dump. Tests: `pumps_read_the_floor_liquid_and_feed_conduits`, `conduit_feeds_a_liquid_consumer`. |
| `4ca8c66` | PAR-0077 (WIP) | Only data: `ItemSource.itemsPerSecond = 100` and `powerSource.powerProduction = 1e6/60` in `core/world/block_kind_data.rs::from_def`, plus a new `SandboxState.liquid: Option<LiquidId>` (`core/entities/comp/building.rs`). **The behavior rewrite is NOT done** (see §3). |

Already working, no action needed: the item/liquid picker for sorters, unloaders
and duct routers (the config fragment works for `ConfigKind::Item/Liquid` blocks).

---

## 2. PAR-0098 remainder: construction polish

The core loop is done. The following items remain, in priority order.

### 2a. Delete-plans and clear-building act on the player queue
- **Symptom:** `SimCommand::DeletePlans` (sent by input when the player right-clicks a
  queued plan) and the `clear_building` key (Java `Binding.clearBuilding`, default `q`)
  do nothing to the real queue. `DeletePlans` edits the unrelated relay `BuildQueue` in
  `core/sim/world_apply.rs:313` (`apply_delete_plans`).
- **Java:** `J/input/InputHandler.java` `removeSelection` / `deletePlans` and
  `J/input/DesktopInput.java:612` (`if(Core.input.keyTap(Binding.clearBuilding) && !player.dead()) player.unit().clearBuilding();`).
- **Change:** in `core/sim/mod.rs`, in the `SimCommand::DeletePlans { positions }` arm,
  also remove matching plans from the player unit's `BuilderPlans`. Add
  `pub fn delete_player_plans(&mut self, positions: &[i32])` to `core/sim/builder.rs`
  that does `plans.plans.retain(|p| !positions.contains(&TilePos::new(p.x, p.y).pack()))`.
  In `gdext/input/desktop.rs` `key_down_event`, the existing arm
  `"clear_building" => { self.controller.state.select_block(None); ... }` (around line 503)
  must additionally call `host.bind_mut().clear_player_build_plans()`. Expose that as a
  `#[func] clear_player_build_plans()` on `MindSimHost` that calls
  `self.sim.clear_player_build_plans()` (which already exists in `core/sim/builder.rs`).
  The binding is named `"clear_building"` in `core/input/binding.rs:361`.
- **Test:** `sim::builder::tests::delete_plans_removes_player_plan`: queue 2 plans, apply
  `SimCommand::DeletePlans{positions: smallvec![TilePos::new(14,10).pack()]}`, then
  assert 1 plan remains.

### 2b. Pause building (`E`)
- **Java:** `DesktopInput.java:675` toggles `isBuilding`, and
  `player.unit().updateBuilding = isBuilding` (`InputHandler.update`).
  `BuilderComp.updateBuildLogic` returns early when `!updateBuilding`
  (`J/entities/comp/BuilderComp.java:89`).
- **Change:** add `pub update_building: bool` (default `true`) to `BuilderPlans`, and in
  `update_builder` return right after `validate_plans` when it is false. Add
  `#[func] set_player_building_paused(paused: bool)` on `MindSimHost` (it sets
  `BuilderPlans.update_building = !paused` on the player unit). Call it from the existing
  `"pause_building"` arm in `gdext/input/desktop.rs` (around line 507), which already
  toggles `controller.state.is_building`: pass `!is_building`.
- **Test:** pausing keeps `construct_progress` constant over 30 ticks.

### 2c. Unit overlap check and the "stuck" skip
- **Java:** `BuilderComp.java:150` `Build.checkNoUnitOverlap(block, x, y)`, where a
  ground unit standing on the footprint rotates the plan to the back of the queue.
- **Change:** in `update_builder` (in `core/sim/builder.rs`), before `has_all`, query the
  unit entities (`UnitTypeComp` + `Pos` + `PhysicsComp.flying == false`) whose hitbox
  overlaps the block footprint rectangle. If any is found, call
  `plans.plans.rotate_left(1); continue;`. Use `core/entities/comp/unit/comp.rs` `PhysicsComp`.
- **Test:** a dagger spawned on the target tile delays construction until it is moved.

### 2d. Save/load of an in-progress ConstructBlock
- **Gap:** `ConstructBlock` has no behavior `write`/`read`
  (`core/world/behavior/mod.rs:241-249` defaults). Java `ConstructBuild.write/read`
  (`J/world/blocks/ConstructBlock.java`, search `public void write(Writes write)`) writes
  `progress` (f32), `previous.id` (short), `current.id` (short),
  `prevBuild` count + positions (revision dependent), and accumulators.
- **Change:** create `core/world/behavior/construct.rs` with a
  `ConstructBehavior: BuildingBehavior` that implements `write`/`read` exactly like Java
  (read the Java method and mirror the field order). Map it in
  `core/world/behavior/mod.rs::default_behavior` with
  `K::ConstructBlock => Arc::new(construct::ConstructBehavior)`. On read, insert a
  `ConstructState` and call `set_construct`/`set_deconstruct` with the block's
  `build_time`/`requirements` (cost multiplier 1.0, or the BuildRules value if known).
- **Test:** an IO round-trip of a sim mid-construction (look at existing tests in
  `core/world/building_io.rs` for the pattern) keeps `progress` and `current`.

### 2e. Plan/queue drawing and build beam (coordinate with the rendering agent)
- **Gap:** queued plans (`player_build_plans_json`) and the placement preview
  (`MindInput.placement_preview_json`, `gdext/input/mod.rs:748`) are not drawn
  anywhere. The construct ghost is static (Java uses the `blockbuild` shader with
  `progress`; `J/world/blocks/ConstructBlock.java:251` `draw()`). The build beam
  (`BuilderComp.drawBuildingBeam`, `BuilderComp.java:358`) is absent.
- **Do not implement the renderer.** It is owned by the rendering session (session 3).
  Only keep the data seams stable. Document them in the final report.

### 2f. Rebuild of destroyed blocks (ConstructBlock "ghosts") and `rebuild_select` (`B`)
- **Java:** `J/game/Teams.java` `TeamData.plans` (`BlockPlan` added on building death
  when `state.rules.ghostBlocks`), `J/input/InputHandler.java` `rebuildArea`, and
  `RebuildAI`. The port has `crate::game::teams::BlockPlan` and `TeamData.plans`
  (used by the campaign AI for PAR-0139).
- **Change (later; low priority):** on the `rebuild_select` key release, convert the
  team's `BlockPlan`s inside the rectangle into `UnitBuildPlan`s on the player queue
  (`queue_player_place`).

### 2g. Schematic placement through plans
- `gdext/campaign.rs:1471` `place_schematic_base64` places instantly through
  `MindSimHost::place_block_at`, and drops rotation. Change it to call
  `self.sim.queue_player_place(x, y, block, rotation)` for each tile when
  `sim.player_can_build()`, keeping the instant path otherwise. This depends on §8
  (PAR-0099).

**Pitfalls for all of §2:**
- `UnitBuildPlan.x/y` is the block **centre** tile (Java convention). For breaks,
  `queue_player_break` normalises to `Building.tile` (the anchor).
- `Sim::refresh_unit_runtime()` clones the grid and rebuilds the pathfinder. Only call
  it when the grid changed (`BuilderOutcome.changed`).
- The ECS `Pos` of a building is `(tile + 0.5) * 8` (`core/world/update.rs:96`), while unit
  `within()` checks in `builder.rs` use `tile * 8` (Java `Tile.worldx()`). Do not
  "fix" one without the other.
- `BuildRuleView::read` prefers `CampaignRuntime.session.rules`. Tests without a
  campaign fall back to `BuildRules` (where `infinite_resources = false`).

---

## 3. PAR-0077: sandbox sources and voids do nothing

- **Symptom:** an item source never outputs, an item/liquid void never accepts, a liquid
  source never outputs, and a power source gives no power.
- **Acceptance:** item source (configured to copper) to conveyor to item void: the conveyor
  carries copper and the void consumes it. Liquid source (water) to conduit to liquid
  void: water flows. A power source linked to a lancer by a node: `power_debug` shows
  satisfaction 1. Unconfigured sources emit nothing (Java default `outputItem = null`).
- **Java:**
  - `J/world/blocks/sandbox/ItemSource.java:34-35` (config Item / configClear),
    `:90-103` `updateTile`: `counter += edelta(); limit = 60f/itemsPerSecond; while(counter >= limit){ items.set(outputItem,1); dump(outputItem); produced(outputItem); items.set(outputItem,0); counter -= limit; }`,
    and `:120` `acceptItem` returns `false`.
  - `J/world/blocks/sandbox/ItemVoid.java:28` `handleItem` does nothing, and `:33`
    `acceptItem` returns `enabled`.
  - `J/world/blocks/sandbox/LiquidSource.java:37-38` config Liquid/clear, and `:62`
    `updateTile`: `if(source == null) liquids.clear(); else { liquids.set(source, liquidCapacity); dumpLiquid(source); }`.
  - `J/world/blocks/sandbox/LiquidVoid.java:34` `acceptLiquid` returns `enabled`, and `:39`
    `handleLiquid` does nothing.
  - `J/world/blocks/sandbox/PowerSource.java` (a PowerNode with `powerProduction`,
    `J/content/Blocks.java:6747` = 1000000/60) and `PowerVoid.java`
    (`consumePower(Float.MAX_VALUE)`).
- **Current state:** `core/world/behavior/sandbox.rs` `SandboxBehavior` is used for all six
  kinds (`core/world/behavior/mod.rs` family `Sandbox` fallback, around line 600). It only
  adds `round(per_tick)` items to its own module, defaults to COPPER, never dumps, and
  does not override `accept_*`. `core/world/blocks/power/sandbox.rs` has unused helpers
  `update_power_source`, `update_power_void`, `update_liquid_source`, `update_liquid_void`.
- **Changes:**
  1. In `core/world/behavior/sandbox.rs`, `update_tile` must read
     `instance(world,e).kind_data` (copy the `instance()` helper pattern from
     `core/world/behavior/production.rs:44`) and dispatch:
     - `BlockKindData::ItemSource(def)`: if `SandboxState.item` is `None`, return.
       Otherwise `state.accumulator += edelta(world,e)` (`crate::world::update::edelta`),
       set `limit = 60.0 / def.items_per_second.max(0.0001)`, and
       `while state.accumulator >= limit { set module[item]=1 (ItemModule: remove all then add 1); transfer::dump(world, e, Some(item)); set module[item]=0; state.accumulator -= limit; }`.
       Write `SandboxState` back after each loop iteration (take/insert pattern).
       Ensure an `ItemModule` exists: in `create_state`, if missing, insert
       `ItemModule::with_items(n)`, where `n` comes from the `ModuleDims` resource
       (`core/world/modules/mod.rs:282`; `world.get_resource::<ModuleDims>()`).
     - `BlockKindData::LiquidSource(_)`: ensure a `LiquidModule` exists (same pattern). If
       `state.liquid` is `None`, zero the module. Otherwise set
       `module.liquids[liquid] = liquid_capacity` (use `helpers::liquid_capacity`, or
       `crate::world::blocks::power::sandbox::update_liquid_source(world, e, liquid, cap)`),
       then call `crate::world::blocks::liquid::movement::dump_liquid_proximity(world, e, liquid, 2.0, -1)`.
     - `BlockKindData::PowerSource(def)`: call
       `crate::world::blocks::power::sandbox::update_power_source(world, e, def.power_production)`.
     - `BlockKindData::PowerVoid`: call `update_power_void(world, e)`.
     - `ItemVoid`/`LiquidVoid`: nothing.
  2. Add overrides to `SandboxBehavior`:
     - `accept_item`: `true` only for `ItemVoid`, and only when `Building.enabled`. Return `false` for others.
     - `handle_item`: no-op (items vanish).
     - `accept_stack`: for ItemVoid return `amount`, else `0`.
     - `accept_liquid`: `true` only for `LiquidVoid` when enabled.
     - `handle_liquid`: no-op.
     - `configured`: `ConfigValue::Item(i)` sets `item`; `ConfigValue::Liquid(l)` sets
       `liquid`; `ConfigValue::None` clears both.
     - `config`: return `Item` for ItemSource and `Liquid` for LiquidSource.
     - `config_kinds`: return `&[ConfigKind::Item]` for item source. Because the method
       has no `&World`, create **two behaviors**: `ItemSourceBehavior` and
       `LiquidSourceBehavior`. The current single struct cannot return per-kind kinds.
       Recommended: split `SandboxBehavior` into `ItemSourceBehavior`, `ItemVoidBehavior`,
       `LiquidSourceBehavior`, `LiquidVoidBehavior`, `PowerSourceBehavior` and
       `PowerVoidBehavior`, and map them by `BlockKind` in
       `core/world/behavior/mod.rs::default_behavior` (add explicit
       `K::ItemSource => Arc::new(sandbox::ItemSourceBehavior)` arms before the family
       fallback).
  3. Remove the `unwrap_or(ItemId::COPPER)` default (Java has no default).
  4. Power: confirm `power-source`/`power-void` defs have `has_power` set, that
     `read_power_info` (`core/world/blocks/power/module.rs:60-110`) classifies the source
     as an output (`outputs_power`) and the void as a consumer, and that
     `PowerProduction` is read by `PowerGraph::get_power_produced`
     (`core/world/blocks/power/graph.rs:161`). If the void lowers
     `Consume::Power{usage: f32::MAX}`, the graph's satisfaction math must not produce NaN
     (check `get_satisfaction`). Clamp the usage to `f32::MAX/1e6` if needed.
- **Tests** (`core/sim/production_tests.rs`):
  - `item_source_feeds_conveyor_into_void`: place `item-source` at (5,5), configure it
    through `SimCommand::Configure{x:5,y:5,value: ConfigValue::Content("copper".into())}`,
    place `conveyor` at (6,5) with rotation 0 and `item-void` at (7,5). Tick 120. Assert
    the void received items: count `dispatch_handle_item` calls through a debug counter,
    or assert the conveyor is not backed up (conveyor item count less than capacity)
    and the source emitted copper (add a `SandboxState.emitted: u32` debug counter;
    `#[derive(Default)]` keeps IO unaffected because SandboxState is not serialized
    by behavior `write`).
  - `unconfigured_item_source_emits_nothing`.
  - `liquid_source_fills_conduit` (configure `Content("water")`).
  - `power_source_powers_consumer`: power-source next to a `laser-drill` (it consumes
    power; place ore under it) and assert `power_debug`-equivalent satisfaction > 0 via
    `crate::world::blocks::power` graph APIs, or that `Building.efficiency > 0`.
- **In-engine:** sandbox blocks are `BuildVisibility::SandboxOnly`, so on Ground Zero
  `placement_allowed` rejects them through the live runtime `placement_gate`. Verify
  through headless tests. In-engine, use a custom sandbox game if the Custom Game dialog
  works (`MindCampaign.start_map(name)` + `set_rules_json` with
  `"infiniteResources":true`); that is unverified.

**Status 2026-10-12:** done — six per-kind behaviors (`ItemSourceBehavior`, `ItemVoidBehavior`, `LiquidSourceBehavior`, `LiquidVoidBehavior`, `PowerSourceBehavior`, `PowerVoidBehavior`) replace the single `SandboxBehavior`; `power-void` gained its `consumePower(f32::MAX)` and `item-void` its Java `solid/update` flags; ItemSource/LiquidSource gained Java `write`/`read` (LiquidSource revision 1); the `parity/golden_content.json` item-void fields were regenerated and audit-verified. In-engine on Ground Zero 170 (the host's `placement_gate` is off, so sandbox blocks are placeable there): the item source emitted exactly 1000 items in 600 stepped ticks (100/s) into the core; an adjacent item void halved that to exactly 500; the liquid source filled to 9986.67 and fed a conduit (6.67); the power source reported graph satisfaction 1.0 with the battery at status 1.0. Evidence: `runs/20261012-session4/PAR-0077-evidence.json`, `runs/20261012-session4/PAR-0077-sandbox-sources.png`.

---

## 4. PAR-0076: reconstructors cannot be supplied and upgrade for free

- **Symptom:** belts cannot feed silicon/graphite into the additive reconstructor, and a
  unit entering it is upgraded after a fixed 120 ticks without consuming items or power.
- **Acceptance:** an additive reconstructor accepts silicon and graphite up to its
  per-item capacity (80 each = 2x requirement) and rejects other items. It upgrades a
  dagger to a mace only with power, 40 silicon + 40 graphite present and after
  `constructTime` = 600 ticks (scaled by efficiency and `rules.unitBuildSpeed(team)`),
  and consumes those items on completion.
- **Java:** `J/world/blocks/units/Reconstructor.java`
  - `init()` lines 140-165: `capacities[item] = max(capacities[item], stack.amount*2)`,
    `itemCapacity = max(itemCapacity, stack.amount*2)` (start at 10), over every
    `ConsumeItems` stack.
  - `getMaximumAccepted` line 275: `round(capacities[item.id] * state.rules.unitCost(team))`.
  - `updateTile` lines 326-367: progress `+= edelta() * state.rules.unitBuildSpeed(team)`
    only when `efficiency > 0`. When `progress >= constructTime`: upgrade, `progress %= 1f`,
    `consume()`. Command and commandPos are applied.
  - `shouldConsume` line 377: `constructing && enabled && team.activateUnitFactories()`.
  - `config(UnitCommand)` line 51 and `configClear` line 52.
  - `J/content/Blocks.java` (around 6336): additive `constructTime = 60*10`, items silicon 40,
    graphite 40, power 3. Multiplicative (around 6358) `60*30`. Exponential (around 6380)
    `60*60*1.5`. Tetrative (around 6400) `60*60*4`. Erekir reconstructors are around 6490-6549.
- **Current state:** `core/world/blocks/units/behavior.rs:196-277` `ReconstructorBehavior`
  is built with `Reconstructor::new(upgrades, 120.0)` (line 206). `update_tile` (line 223)
  calls `state.update(&self.reconstructor, 1.0)` (`core/world/blocks/units/mod.rs:236`),
  which ignores efficiency and items. There is no `accept_item`. The specs in
  `core/content/registries/blocks/units.rs` (around 121-200) and `units_erekir.rs` omit
  `has_items`, so no `ItemModule` exists, and the `Consume::Items` gate in
  `core/world/update.rs:517` is permanently 0. There is no `constructTime` field in the
  spec (check `BlockSpec` in `core/content/registries/blocks/mod.rs` for
  `construct_time`; add `pub construct_time: Option<f32>` to `BlockSpec` and `BlockDef`
  if absent).
- **Changes:**
  1. Content: in `core/content/registries/blocks/mod.rs`, during `BlockDef` init (the
     `init()` block around line 2655), set `has_items = true` when any
     `Consume::Items` consumer exists (Java `ConsumeItems.apply` sets `hasItems=true`,
     `J/world/consumers/ConsumeItems.java:24`). This also fixes other blocks with
     missing flags. Run the full test suite: goldens may change. If a golden test fails,
     inspect it. Only update a golden if the new value matches Java.
  2. Add `construct_time` to the reconstructor specs with the Java values above
     (Serpulo 4, Erekir 4). Plumb `BlockDef.construct_time` into
     `ReconstructorBehavior::from_def` instead of `120.0`.
  3. `ReconstructorBehavior`: add `capacities: Vec<i32>` computed like Java `init()` from
     `def.consumes` `Consume::Items` stacks. Implement:
     - `accept_item`: `capacities[item] > 0 && count(item) < capacities[item]` (Java
       `Reconstructor.acceptItem` is the default `consumesItem && items < getMaximumAccepted`).
     - `get_maximum_accepted`: `capacities[item]`.
     - `handle_item`: `transfer::default_handle_item`.
     - `accept_stack` and `handle_stack`: copy `UnitFactoryBehavior` at
       `core/world/blocks/units/behavior.rs:149-188`.
  4. `update_tile`: read `Building.efficiency`. Only when a payload exists with a valid
     upgrade, and `efficiency > 0`, advance
     `progress += edelta(world,e) * rules.unit_build_speed(team)`.
     `rules.unit_build_speed` exists (`core/game/rules.rs:157`); read it from
     `CampaignRuntime` if present, else 1.0. On completion, remove the consumed item stacks
     from the `ItemModule` (Java `consume()`), then spawn the upgraded unit. Change
     `ReconstructorBuild::update` to take `(delta_progress: f32)` and return the target;
     keep the payload/progress bookkeeping there.
  5. Make the generic consume gate apply. `Building.efficiency` is computed by
     `core/world/update.rs` from `consumer_efficiency`. With an `ItemModule` present, the
     `K::Items` gate works. Make sure `should_consume` for the reconstructor is only true
     while a payload is being upgraded (Java `shouldConsume`). If the update pass has a
     per-behavior hook (`fn should_consume`), override it.
- **Tests** (in `core/world/blocks/units/behavior.rs` tests, or `core/sim/production_tests.rs`):
  - `additive_reconstructor_accepts_only_its_items` (silicon yes, copper no, capacity 80).
  - `reconstructor_needs_items_and_time`: dagger payload plus no items gives no upgrade
    after 700 ticks. With 40/40 items and a power source it upgrades between tick 600
    and 700, and the items drop to 0.
- **Pitfall:** `UnitFactoryBehavior` stores items in its own `UnitFactoryBuild.items` vec
  (`core/world/blocks/units/mod.rs:141`), not in an `ItemModule`, so the inventory UI and
  unloaders cannot see them. Do **not** copy that pattern. Use `ItemModule` for the
  reconstructor. Migrating the unit factory to `ItemModule` is a follow-up (§10).

**Status 2026-10-12:** done — `BlockDef` gained `construct_time` (`Reconstructor.constructTime`:
additive 600, multiplicative 1800, exponential 5400, tetrative 14400; Erekir refabricators
1800/3000/2700/3600) and the reconstructor specs gained Java's `itemCapacity` values
(`initCapacities`: 80/260/1700/2000 and 80/120/100/200); `Consume::Items` consumers now set
`has_items`/`accepts_items` in `init_self` (Java `ConsumeItems.apply`). `ReconstructorBehavior`
computes `capacities` like `initCapacities`, implements `accept_item`/`get_maximum_accepted`
(`capacities[item] * rules.unitCost(team)`)/`handle_item`/`accept_stack`/`handle_stack`, and its
`update_tile` advances `edelta * rules.unitBuildSpeed(team)` only while a valid payload is held,
removes the `ConsumeItems` stacks (`trigger` with the `unitCost` multiplier) and spawns the upgrade;
`ReconstructorBuild::update` takes the delta and keeps `progress %= 1f`. `parity/golden_content.json`
regenerated (9 entries: `has_items` for neoplasia-reactor + the 8 reconstructors, `item_capacity`
for the reconstructors) and audit-verified. Tests:
`sim::production_tests::reconstructor_needs_items_and_time` (no items -> payload waits 700 ticks;
40/40 + power-source -> mace in the 600..700 window and items consumed to 0) and
`world::blocks::units::behavior::tests::additive_reconstructor_accepts_only_its_items`. In-engine
Ground Zero 170: an item source feeding an additive reconstructor is rejected for copper and fills
silicon to exactly 80 (2x the requirement) and stops. Evidence:
`runs/20261012-session4/PAR-0076-evidence.json`. The payload-to-mace half is headless-only (no
in-engine seam loads a unit payload into a reconstructor).

---

## 5. PAR-0079: Erekir plasma-bore and cliff-crusher never produce

- **Symptom:** a plasma bore (BeamDrill) aimed at an ore wall and a cliff crusher
  (WallCrafter) next to a sand wall produce nothing.
- **Java:**
  - `J/world/blocks/production/BeamDrill.java`: `drillTime = 200`, `range = 5` (lines
    39-40). `updateFacing` scans `range` tiles in front of each footprint edge tile for a
    solid wall with `wallDrop()` (lines 140-175). `canPlaceOn` (line 198) requires a
    wall ore in range. `updateTile` (line 237) does
    `lastDrillSpeed = facingAmount*multiplier*timeScale/drillTime*efficiency`; when
    `time >= drillTime` it offloads `lastItem`. The drill multiplier is
    `getDrillTime(item) = drillTime / drillMultipliers.get(item,1)` (line 217).
  - `J/world/blocks/production/WallCrafter.java`: `drillTime=150`,
    `attribute=Attribute.sand`, `output=Items.sand` (lines 33-43). `getEfficiency`
    (line 141) sums `other.block().attributes.get(attribute)` of solid tiles directly in
    front (the facing edge). `canPlaceOn` (line 137) requires `eff > 0`. `updateTile`
    (line 187): `if(cons && (time += edelta()*eff) >= drillTime){ offload(output); time %= drillTime; }`
    then `dump(output)` (line 221). `shouldConsume` (line 227) is `items.get(output) < itemCapacity`.
- **Current state:** `core/world/behavior/mod.rs:579-580` maps `WallCrafter` to `DrillBehavior`
  and `BeamDrill` to `BeamDrillBehavior`, which delegates to `DrillBehavior`
  (`core/world/behavior/production.rs` around line 610). `refresh_drill_ore`
  (`production.rs` around line 457) counts **floor** ore under the footprint for
  BeamDrill (wrong) and returns early for WallCrafter, so `dominant_item` stays `None` and
  `update_tile` returns.
- **Changes:**
  1. Add `facing_amount: f32` and `facing_item: Option<ItemId>` fields to `DrillState`
     (`core/entities/comp/building.rs`; find `pub struct DrillState`). Do not serialize
     them (Java recomputes them on proximity update).
  2. In `refresh_drill_ore`, add branches:
     - `BeamDrill(def)`: for each of the `size` tiles along the front edge (rotation
       0=+x, 1=+y, 2=-x, 3=-y; the front edge is `offset + size` in the facing direction),
       step `j in 0..def.range`. Stop at the first tile whose block `def.solid` is true.
       If that block's `item_drop` (the wall ore: the port's `BlockDef.item_drop` on wall
       ore blocks, check what `wallDrop()` maps to; Java `Tile.wallDrop()` returns
       `block.itemDrop` when the block is solid, else the overlay's wall ore) has
       `hardness <= def.tier` and is not blocked, count it. `facing_amount` is the count
       (times the per-item multiplier) and `facing_item` is the most common item.
       `BeamDrillDef` needs `range` (default 5); add it to `core/world/block_kind_data.rs`
       `BeamDrillDef` if missing, and set plasma-bore = 5 and large-plasma-bore = 6 (see
       `J/content/Blocks.java`, search `plasmaBore = new BeamDrill`).
     - `WallCrafter(def)`: sum `attributes` named `def.attribute` (default "sand") of the
       solid blocks directly in front of the facing edge into `facing_amount`.
  3. Replace `BeamDrillBehavior::update_tile` with a port of `BeamDrill.updateTile`
     (progress uses `facing_amount`; offload `facing_item` with `transfer::offload`; dump
     on the dump timer like `DrillBehavior`). Add `WallCrafterBehavior` with the
     `WallCrafter.updateTile` port, and map
     `K::WallCrafter => Arc::new(production::WallCrafterBehavior)` in `mod.rs:579`.
  4. Placement: add to `core/world/build.rs::valid_place_at` (next to the
     `pump_can_place_on` call) a `BeamDrill` and `WallCrafter` `canPlaceOn` check that uses
     the same scans. Factor the scan into a `pub fn` in `production.rs` that takes
     `(content, grid, x, y, size, rotation, range)`.
  5. Rotation matters: refresh on rotate (the `SimCommand::Rotate` arm in
     `core/sim/mod.rs:749` should re-run `refresh_drill_ore`; it already calls `no_sleep`).
- **Tests:** in `core/sim/production_tests.rs`, `plasma_bore_mines_wall_ore` places a
  `beryllium-wall`-type ore wall (the Erekir wall ores are `wall-ore-beryllium`,
  `wall-ore-tungsten`; check names with `content_list`) 2 tiles in front, gives the
  drill power (power-source after §3, or set `Building.efficiency` via a consumer stub),
  and asserts items appear after `drillTime/facing` ticks.
  `cliff_crusher_makes_sand_from_sand_walls`.
- **In-engine:** blocked. The imported profile has no launchable Erekir sector (ledger
  note). Verify headless only.

---

## 6. PAR-0089: sublimate fires on any trace of fuel and never consumes it

> This is turret code (session 2 owns combat). Do it only if session 2 has not, and
> coordinate. It is listed here because it was assigned to this workstream.
- **Java:** `J/world/blocks/defense/turrets/ContinuousLiquidTurret.java` lines 64-135:
  `activated` latch: `currentAmount >= liquidConsumed*4` sets it, `< liquidConsumed`
  clears it. `shouldConsume = super && activated`.
  `hasAmmo = hasCorrectAmmo && ammoTypes.get(current) != null && currentAmount > 0 && activated`.
  `useAmmo` does not consume. Consumption is the `ConsumeLiquidFilter(ammoTypes, liquidConsumed)`
  consumer (line 42). Sublimate: `liquidConsumed = 18/60` (see `J/content/Blocks.java`,
  search `sublimate = new ContinuousLiquidTurret`).
- **Current state:** `core/world/blocks/defense/turrets/mod.rs:863` `has_ammo_state` for
  `TurretAmmo::Liquid` returns true when `liquids.get(l) >= 1/ammo_multiplier`. There is no
  `activated` latch and no per-tick consumption. The comment at
  `core/world/blocks/defense/turrets/advanced.rs:1576` claims "liquid consumption stays
  with the ConsumeLiquidFilter consumer", but filter consumers are not lowered (see
  `core/world/consumers.rs:23`: `ConsumeInstanceKind` has no filter variant).
- **Changes:** add `liquid_consumed: f32` to `TurretConfig` (sublimate `18.0/60.0`) and
  `activated: bool` to `TurretState`. In the turret tick for `TurretKind::Continuous` with
  `TurretAmmo::Liquid`, update `activated` as in Java (use `LiquidModule.current` and
  `current_amount`). Gate `has_ammo` on `activated && amount > 0`. While
  `activated && efficiency > 0 && is_shooting`, remove `liquid_consumed * delta` of the
  current liquid each tick. Since Java's consumer consumes only when `shouldConsume`
  (shooting), check `ContinuousTurretBuild.shouldConsume`.
- **Test:** sublimate with 0.5 ozone never fires. With 2.0 ozone it fires, and the ozone
  decreases by 18/60 per tick while shooting.

---

## 7. Block config (PAR-0092, PAR-0093, PAR-0108, plus others)

How the config UI works today:
- Tapping a building with no block selected calls `tile_tapped` (`gdext/input/desktop.rs:1069`).
  That calls `config_spec(x,y)` (`gdext/input/desktop.rs:1393`), which builds the JSON
  `{title, block, x, y, text, options:[{name,label}]}` and emits
  `Effect::OpenBlockConfig`. The panel is `client/ui/fragments/block_config_fragment.gd`.
  It builds content buttons (submit kind `"content"`) or a text field (submit kind
  `"string"`, only for `BlockKind::LogicBlock`).
- Submit goes to `MindInput.configure_building(x,y,kind,value)` (`gdext/input/mod.rs:891`):
  `"none"` maps to `ConfigValue::None`, `"number"` to `Number`, `"bool"` to `Bool`, and
  **everything else to `ConfigValue::String`**. Then `RemoteAction::Configure` calls
  `to_sim_config` (`core/input/command_emit.rs:248`), where `String(s)` becomes the
  **wire `Content(s)`**. Then `SimRuntime::resolve_config` (`core/sim/runtime.rs` around
  line 197) resolves the content name to `Item/Liquid/Block/Unit`. An unknown name is parsed
  as a raw `u16` block id, else `Err(UnknownContent)`. **Arbitrary text therefore can
  never reach a building as `String`.**
- The options come from `block_config_kinds(block)` (the behavior's `config_kinds()`).
  Only `sorter.rs:74` declares kinds. Undeclared configurable blocks fall back to the item
  list if `has_items`, else the liquid list. That is why unit factories show an unrelated
  item list.

### 7.0 Shared prerequisite: a text wire value
1. `core/determinism/command.rs`: add the variant `Text(String)` to `pub enum ConfigValue`
   (around line 17). Encode it as tag `6` in the encoder (around line 636, next to
   `ConfigValue::Bytes`): `put_u8(out, 6); put_string(out, value)` (mirror the `Content`
   arm). Decode it as `6 => Ok(ConfigValue::Text(cur.string()?))` (around line 893). Add a
   round-trip test next to the existing ones (around line 1009).
2. `core/sim/runtime.rs::resolve_config`: add `Wire::Text(s) => WorldConfigValue::String(s.clone())`.
3. `core/input/command_emit.rs::to_sim_config` stays as is (`String` to `Content`). This
   keeps the content picker working.
4. `gdext/input/mod.rs::configure_building`: for `kind == "string"`, bypass
   `RemoteAction`. Get the host and call `host.bind_mut().enqueue_sim_command(SimCommand::Configure { x, y, value: mind_core::determinism::ConfigValue::Text(value) })`
   (`MindSimHost::enqueue_sim_command` is at `gdext/sim_host.rs:393`). For `kind == "int"`,
   use `ConfigValue::Int(value.parse()?)` through the same direct path (needed for the
   illuminator color and the unit-factory plan index).
5. Test: `core/sim` test: configure a `message` block with `Text("hello\nworld")` and
   assert `MessageBehavior::config` returns `String("hello\nworld")`.

### 7.1 PAR-0108: switch toggle
- **Java:** `J/world/blocks/logic/SwitchBlock.java:28` `config(Boolean.class, ...)` and
  `:58-62` `configTapped(){ configure(!enabled); clickSound.at(this); return false; }`.
  Returning false means no panel opens.
- **Port:** `core/logic/blocks/switch.rs` already handles `ConfigValue::Bool` in `configured`.
- **Change:** in `gdext/input/desktop.rs::tile_tapped`, before `config_spec`, check the block
  kind at (x,y) through the host content (`BlockKind::SwitchBlock`). If it matches, read the
  current enabled flag (add `#[func]`/Rust `MindSimHost::building_enabled(x,y) -> Option<bool>`
  that reads `Building.enabled`), and emit
  `RemoteAction::Configure{x,y,value: ConfigValue::Bool(!enabled)}` through
  `self.emit_action`. Then return without opening a panel. Also make `config_spec` return
  `None` for switches.
- **Test (in-engine):** place a `switch` with `place_block(130,55,"switch")`, tap it with
  `python3 tools/gmcp.py godot_input '{...left click at the tile screen pos...}'`, or call
  `get_node("/root/Spine/Input").configure_building(130,55,"bool","true")`, then assert
  `building_enabled` flips. Add a Rust unit test for `SwitchBehavior` configured Bool
  (it may already exist).

### 7.2 PAR-0093: message and illuminator
- **Message, Java:** `J/world/blocks/logic/MessageBlock.java:41-60` (`config(String)`:
  length <= `maxTextLength`=400, newlines <= `maxNewlines`=24) and `:109`
  `buildConfiguration` (a text dialog).
- **Port:** `core/logic/blocks/message.rs:56` `configured` handles `ConfigValue::String`
  (correct once §7.0 lands).
- **Changes:** `config_spec` (`gdext/input/desktop.rs:1393`): set
  `"text": true` for `BlockKind::LogicBlock | BlockKind::MessageBlock`. Add
  `"current": <current text>` from a new `MindSimHost::building_config_string(x,y) -> Option<String>`
  that calls the behavior's `config()` and returns the `String` payload. In
  `block_config_fragment.gd::_build_text_editor`, prefill `field.text` from
  `_config.get("current","")`, and use a `TextEdit` (multi-line) when
  `_config.get("multiline", false)`. Set `multiline: true` for messages. Submit `"string"`.
- **Illuminator, Java:** `J/world/blocks/power/LightBlock.java:35`
  `config(Integer.class, (tile, value) -> tile.color = value)`, `:68` default
  `color = Pal.accent.rgba()`, `:109` `buildConfiguration` opens a color picker.
- **Port:** there is no behavior (`BlockKind::LightBlock` falls through to Noop;
  `core/game/schematics.rs:495` already treats its config as a Number).
- **Changes:** create `core/world/behavior/light.rs` with `LightBehavior` and a component
  `LightState { color: u32 }` (default `0xffd37fff`, which is `Pal.accent`). It handles
  `configured(ConfigValue::Number(n))` as `color = n as i64 as u32`, `config()` returns
  `Number(color as f64)`, `config_kinds` returns `&[ConfigKind::Number]`, and
  `write`/`read` mirror Java `LightBuild.write/read` (an `i` color; check the Java file
  for the revision). Map `K::LightBlock => Arc::new(light::LightBehavior)` in
  `core/world/behavior/mod.rs`. Wire `Wire::Int(v)` resolves to `Number`, so it already
  works (`resolve_config`). In `config_spec`, for LightBlock emit `"color": true` and
  `"current": color`. In the fragment, show a `ColorPickerButton` (code-instantiated, add
  the `# code-instantiated:` comment). On change, submit `("int", str(color.to_rgba32()))`.
  Light rendering itself belongs to the render agent; expose
  `MindSimHost::light_color(x,y)`.
- **Tests:** message `Text` round-trip (§7.0); `LightBehavior` configured/config round-trip.

### 7.3 PAR-0092: unit factories, reconstructors and payload blocks
- **Java:**
  - `J/world/blocks/units/UnitFactory.java:58-82`: `config(Integer)` sets the plan index,
    `config(UnitType)` sets the plan by type, `configClear` clears the command.
    `currentPlan = -1` by default (line 228). `:285` `buildConfiguration` shows
    `ItemSelection` over `plans.map(p -> p.unit)` (only unlocked ones).
  - `J/world/blocks/units/Reconstructor.java:51-52`: `config(UnitCommand)` / `configClear`.
    `:215` `buildConfiguration` shows the command buttons of the output unit type.
  - `J/world/blocks/payloads/Constructor.java:67`, `PayloadSource.java:113` and
    `PayloadRouter.java:183` each have a `buildConfiguration` with a block/unit picker.
- **Port:**
  - `core/world/blocks/units/behavior.rs` `UnitFactoryBehavior` has no `configured`,
    `config` or `config_kinds`. `UnitFactoryBuild.current_plan` is a `usize` defaulting
    to 0 (`core/world/blocks/units/mod.rs:143,154`), but Java defaults to -1.
  - `ReconstructorBehavior` has no command config.
  - Payload blocks already implement `configured`/`config`
    (`core/world/blocks/payloads/constructor.rs:115-122`, `payload_source.rs:123-135`,
    `payload_router.rs:258-266`), but declare no `config_kinds`, so the panel falls back to
    items.
- **Changes:**
  1. `UnitFactoryBehavior`: change `current_plan` to `Option<usize>` (or `i32` with -1) to
     match Java, and update `UnitFactoryBuild::update` (mod.rs:165) to return `None` when
     unset. Implement `configured`: `Number(n)` sets the index (out of range means `None`),
     `Unit(id)` finds the plan by unit, and `None` clears. Implement `config`: returns
     `Number(index)` or `None`. `config_kinds`: `&[ConfigKind::Unit]`. Implement
     `write`/`read` if the factory serializes `currentPlan` (Java `UnitFactoryBuild.write`
     writes `progress` and `currentPlan` as a short; check the existing write in
     behavior.rs and keep IO compatible).
  2. `gdext/input/desktop.rs::config_spec`: when `declared.contains(&ConfigKind::Unit)`, list
     options from the block's `def.unit_plans` (unit names; label `unit.<name>.name`) for
     the unit factory, or all units for payload sources. When
     `declared.contains(&ConfigKind::Block)`, list blocks (payload source/constructor:
     filter by `def.size <= block size` as Java `Constructor.canProduce` does; look at
     `constructor.rs` for the existing filter). The fragment's `"content"` submit resolves
     the name to `Unit`/`Block` through `resolve_config` (already done, because unit and
     block names resolve via `content.by_name`).
  3. Reconstructor command: add `command: Option<u16>` to `ReconstructorBuild` and a
     `configured(ConfigValue::Content(ContentType::UnitCommand, id))` handler. Check how
     `UnitCommand` content is represented in the port (`core/ai` or content
     registries; search `UnitCommand`). Apply it to the spawned unit's controller via the
     same path as the PAR-0140 command AI (`core/sim/unit_runtime.rs`, `AiKind::Command`).
     If commands are not content in the port, add `config_kinds = &[]`, return `None` from
     `config_spec` for reconstructors (so no misleading item list appears), and record
     the gap.
  4. Payload blocks: add `config_kinds()` returning `&[ConfigKind::Block]`
     (constructor), `&[ConfigKind::Block, ConfigKind::Unit]` (payload source), and
     `&[ConfigKind::Block, ConfigKind::Unit]` (payload router), matching each `configured`.
- **Tests:** `unit_factory_config_selects_plan` (`Configure Content("flare")` on an
  air-factory selects that plan, and with no config nothing is produced);
  `config_spec_lists_factory_units` (a gdext unit test, if `config_spec` is testable;
  otherwise verify in-engine: tap a placed `ground-factory` and read the
  `block_config` node's `_config` via
  `return get_node("/root/Spine/Ui/UiRoot/OverlayLayer/block_config")._config`).

### 7.4 Other config blocks: sanity sweep
For each block, place it, tap it and configure it, then assert `config()` with a small
`#[func] building_config_json(x,y)` seam (add it: return the behavior `config()` as JSON).
This avoids a seam per block.

| Block | Java config | Port behavior | Status |
|---|---|---|---|
| sorter / inverted sorter | Item | `distribution/sorter.rs` | works (declared kinds) |
| overflow/underflow gate | none (Java `invert` flag only) | `overflow_gate.rs` | must have no config panel; check `def.configurable` is false |
| unloader | Item | `directional_unloader.rs:151` for the duct unloader; check that `unloader` has a behavior with `configured` | verify |
| item bridge / phase conveyor / bridge conduit | Point2 (link), tapping another bridge links them | `item_bridge.rs:326-335` | **unverified**: the link is set by tapping a second building (`onConfigureBuildTapped`). Check that `tile_tapped` supports "configure another building" (it does not today: the panel opens instead). Port `J/world/blocks/distribution/ItemBridge.java` `onConfigureBuildTapped`. This needs a "selected config building" state in `DesktopBridge`: after opening a bridge config, the next tap on a bridge in range emits `Configure{Point2 relative}`. |
| mass driver | Point2 link | `mass_driver.rs:464-473` | same as bridges |
| power node | Point2 laser links, Integer | `nodes.rs` | same, plus auto-link on placement (`PowerNode.getNodeLinks`). Verify that placing a node auto-links in-range consumers. |
| door | Boolean (tap toggles, `configTapped`) | `behavior/defense.rs:188-205` | needs the same tap-toggle as the switch (§7.1). Java `Door.DoorBuild.tapped`/`configTapped` |
| duct router / duct unloader | Item | `duct_router.rs` | works per the ledger |
| item source / liquid source | Item / Liquid | §3 | after §3 |
| launch pad / landing pad | (Erekir landing pad Item) | `behavior/campaign.rs:96-104` | verify |
| logic processor | String code / links | `logic_block.rs:520` | text works once §7.0 lands (logic editor UI is another agent's) |
| canvas | Bytes | `canvas.rs:100` | UI not ported (low priority) |
| memory cell / display | none | | n/a |

---

## 8. PAR-0099: schematic copy mode (F) is dead

- **Symptom:** holding F and dragging selects nothing, and clicking places nothing. Z/X
  flips do nothing.
- **Java:** `J/input/DesktopInput.java:616-657`:
  - On `schematicSelect` tap, record `schemX/schemY` = raw cursor.
  - On release, `lastSchematic = schematics.create(schemX, schemY, rawCursorX, rawCursorY)`,
    then `useSchematic(lastSchematic)` (`:529-536`: `selectPlans = schematics.toPlans(schem, schematicX, schematicY, checkHidden)`).
  - While `selectPlans` is non-empty, the plans follow the cursor (`:596-606`), Z/X call
    `flipPlans(selectPlans, true/false)` (`:647-654`), and the wheel rotates them
    (`rotatePlans`). A left tap commits (`InputHandler.flushPlans(selectPlans)` then
    `selectPlans.clear()`), and right-click or `clearBuilding` clears them (`:620-624`).
  - The selection rectangle is drawn by `drawBreakSelection(..., maxSchematicSize)` (`:135-141`).
- **Port pieces that exist:** `core/game/schematics.rs:724` `Schematics::create(world: &dyn SchematicWorld, registry, x, y, x2, y2, max_size)`
  and `:640` `to_plans`. `EcsSchematicWorld` implements `SchematicWorld`
  (`core/game/schematics.rs:126`). `core/input/line.rs:219` `flip_plans`. The bindings
  exist: `schematic_select` (`f`), `schematic_flip_x` (`z`), `schematic_flip_y` (`x`)
  (`core/input/binding.rs:412-428`, ids 20/21/22).
- **Current state:** `gdext/input/desktop.rs::key_down_event` (the match around line 540)
  has no `schematic_select`/`schematic_flip_*` arms. There is no select-plans state.
- **Changes (all in `gdext/input/desktop.rs` unless stated):**
  1. Add fields to `DesktopBridge`: `schem_start: Option<(i32,i32)>`,
     `select_plans: Vec<ClientPlan>`, `schem_anchor: (i32,i32)`.
  2. On `"schematic_select"` key **down** (only when `!text_focus` and no break drag), set
     `schem_start = cursor_tile()`. A key **up** edge is needed: `RawEvent::KeyUp` is
     handled at around line 380. On KeyUp, if the code matches the `SCHEMATIC_SELECT`
     binding and `schem_start` is set, build the schematic:
     - Get the host. Build an `EcsSchematicWorld` over `host.grid()` + `host.sim.ecs` (check
       the constructor signature at `core/game/schematics.rs` around line 100; it may need
       `&World` and `&WorldGrid`; add a `MindSimHost::schematic_from_area(x1,y1,x2,y2) -> Option<Schematic>`
       in `gdext/sim_host.rs` so the borrow stays inside the host).
     - Use `max_size = 64` (Java `Vars.maxSchematicSize`; check `core/config.rs` or
       `game/schematics.rs` for a constant).
     - Convert the tiles to `select_plans` (`ClientPlan::place(x, y, rotation, block)`
       with `x = cursor.x + tile.x - width/2` etc.; follow `to_plans` centering), set
       `schem_anchor = cursor`, and clear `schem_start`.
  3. In `mouse_move`, when `!select_plans.is_empty()`, shift every plan by the cursor
     delta (Java `:596-606`) and update `preview_json` with the same JSON shape that
     `update_place_line` uses (line 1205), so the renderer can draw it once that exists.
  4. In `key_down_event`, `"schematic_flip_x"`/`"schematic_flip_y"`: when select plans exist,
     call `mind_core::input::line::flip_plans(&mut self.select_plans, true/false)` (check the
     signature: `flip_plans(plans, flip_x, flip_y)`).
  5. In `scroll`: when select plans exist, rotate them around the anchor (Java
     `InputHandler.rotatePlans`; port it into `core/input/line.rs` as
     `pub fn rotate_plans(plans: &mut [ClientPlan], direction: i32)` with a unit test),
     and return.
  6. In `left_press`: when select plans exist, commit them. Emit
     `SimCommand::Place{.., player: Some(0)}` for each plan through `enqueue_commands`
     (the same as `commit_place`), then clear. `right_press` or `clear_building`: clear
     `select_plans`.
  7. Keep the schematic-dialog "save" (`showSchematicSave`) out of scope (the schematics
     dialog belongs to another agent). Expose `MindInput.last_schematic_base64()` if cheap.
- **Tests:** a `core/input/line.rs` unit test for `rotate_plans` (four rotations give the
  identity). In-engine: place 3 conveyors with `place_block`, inject
  `python3 tools/gmcp.py godot_input '{"action":"key","params":{"key":"F","pressed":true}}'`
  (check the exact `godot_input` schema in `.claude/skills/playtest/SKILL.md`), move the
  mouse, release F, click elsewhere, and assert that `player_build_plans_json()` lists 3
  conveyor plans.
- **Dependency:** commit uses the player build queue (done, §1).

---

## 9. PAR-0100: focus and lock guards (wheel over UI zooms the camera)

- **Symptom:** scrolling the mouse wheel over a scroll panel (console, database, any
  `ScrollContainer`) zooms the world. An input lock (cutscene) does not stop world input.
- **Java:** `J/input/DesktopInput.java:237` `boolean locked = locked();` and `:478-482`
  zoom only when `(!scene.hasScroll() || keyDown(diagonalPlacement)) && !chatfrag.shown() && !consolefrag.shown() && !keyDown(rotatePlaced) && ...`.
  `locked()` (`J/input/InputHandler.java`, search `public boolean locked()`) returns
  `inputLocks.contains(Boolp::get)`, which gates movement, shooting and building.
- **Port:** `core/input/desktop.rs:62` `DesktopController::handle_scroll(delta, caps)` is gated
  on `caps.focus().blocks_zoom()`, but it is never called. `gdext/input/desktop.rs:1352`
  `scroll()` only checks `self.ui_dialog`. `gdext/input/mod.rs:577` `set_locks(json)`
  stores `self.locks`, but nothing reads it.
- **Changes:**
  1. `gdext/input/mod.rs::input` (around lines 110-160): for `RawEvent::Scroll { .. }`,
     compute `self.ui_captures_at(mouse_x, mouse_y)` (scroll events carry no position;
     use `self.bridge.mouse`). If the hovered control is inside a `ScrollContainer`, or
     any STOP-filter control captures, skip `bridge.handle` for that event (Java
     `scene.hasScroll()`). Implement `fn ui_scroll_at(x,y) -> bool`, which walks the
     hovered control's ancestors for `ScrollContainer` (`try_cast::<ScrollContainer>()`).
  2. Also skip zoom when the console or chat fragment is visible. Read visibility of
     `../Ui/UiRoot/HudGroup/console` and `.../chat` (check the node names in
     `client/scenes/ui/ui_root.tscn`).
  3. Locks: pass `locked = self.locks.any()` into the bridge (`bridge.locked = ...` each
     `process`). Add `pub fn any(&self) -> bool { self.dialog || self.cutscene || self.logic_cutscene || !self.custom.is_empty() }` to
     `core/input/focus.rs::InputLocks` (it does not exist yet). In `DesktopBridge`, when `locked`: ignore `left_press`, `right_press` and
     `scroll`, and send a zero movement axis (`MindSimHost::player_axis` is read in
     `sim_host.rs` around line 153; add a `MindSimHost.set_input_locked(bool)` that zeroes
     the axis).
  4. Optionally route `scroll` through `DesktopController::handle_scroll` with a `caps`
     adapter so the core-gated path is the live one (the code scan asked for this).
- **Tests:** a core unit test already covers `handle_scroll` blocking
  (`core/input/desktop.rs:222-230`). In-engine: open the console
  (`get_node("/root/Spine/Input")` hotkey or `MindUi` toggle). Record
  `get_node("/root/Spine/World/Camera2D").zoom`, inject a wheel event over the console
  rect via `godot_input`, and assert the zoom is unchanged. With the console closed, the
  same event changes the zoom.

---

## 10. Broad per-family sweep: known broken or unverified

Status key: OK = verified by a test in this workstream; BROKEN = confirmed by reading
code; UNVERIFIED = not checked. For each UNVERIFIED family, write one
`core/sim/production_tests.rs` test (the place-and-tick pattern) before fixing anything.

| Family | Blocks | Status / notes |
|---|---|---|
| Mechanical/pneumatic drills (2x2) | mechanical-drill, pneumatic-drill | Floor ore counting runs at placement (`refresh_drill_ore`). In-engine GZ drills feed the core (test `ground_zero_drill_feeds_the_materialized_core` in `core/sim/runtime.rs`). OK. |
| Laser/airblast drills (3x3, 4x4) | laser-drill, blast-drill | **BROKEN (likely):** `count_ore` (`core/world/behavior/production.rs` around line 530) iterates `x..x+size` from `Building.tile`, the **centre** tile. Java uses `offset = -(size-1)/2`, so for size 3 the footprint is `x-1..x+1`, and for size 4 it is `x-1..x+2`. Fix: in `refresh_drill_ore` pass `tile.x() - (size-1)/2` (and y) into `count_ore`. Check `world/harness.rs` callers too. Add test `laser_drill_counts_centered_footprint` (ore only under the left column must count). Also, laser drills need power plus optional water boost (`optional_efficiency`). After §3, verify with a power source. |
| Erekir drills | plasma-bore, large-plasma-bore, cliff-crusher, impact-drill (BurstDrill) | §5. Impact drill unverified. |
| Pumps | mechanical/rotary/impulse pump | OK (§1, `217124e`). Rotary/impulse need power: unverified. |
| Solid pumps | water-extractor, oil-extractor (Fracker), cultivator? (AttributeCrafter) | water-extractor placeable now. Its output scales with the floor attribute (`SolidPump.attribute`): **BROKEN**, `block_kind_data.rs` around 850-863 forces attribute -1 (per the ledger). Port `SolidPumpBuild.onProximityUpdate` validTiles/boost. Fracker sand input via belts now accepted: UNVERIFIED end-to-end. |
| GenericCrafters | graphite-press, silicon-smelter, kiln, pulverizer, melter, separator, spore-press, cryofluid-mixer, plastanium-compressor, phase-weaver, surge-smelter, pyratite-mixer, blast-mixer, disassembler | Item accept/dump existed. Liquid input/output fixed in `217124e`. Crafter dump loops `amount` times per timer (`production.rs` around line 178), whereas Java `dumpOutputs` calls `dump(item)` once per output per timer. Verify against Java `GenericCrafter.dumpOutputs` and fix if it differs. Separator/disassembler weighted output: UNVERIFIED. |
| Generators | combustion, steam, differential, rtg, solar, large-solar, thermal, impact-reactor, thorium-reactor | Item/liquid intake fixed (`5a79e4d`). Generators with `output_liquid` (pyrolysis, neoplasia) add liquid but **never dump**. Fix: in `core/world/blocks/power/generator.rs` after the output-liquid add (around line 555), call `dump_liquid_proximity(world, entity, liquid, 2.0, -1)`. Thermal needs heat floor: UNVERIFIED. Impact reactor and thorium reactor intake: UNVERIFIED. |
| Power graph | power-node, large node, battery, diode, surge tower | Node auto-linking on placement and laser links via config: UNVERIFIED (§7.4). Battery charge/discharge has tests in `core/world/blocks/power/tests.rs`. |
| Conveyors | conveyor, titanium, plastanium (stack), armored | Covered by existing distribution tests; in-engine GZ conveyor line is OK per ledger chain `ground-zero-conveyor-line`. Plastanium stack conveyor loading: UNVERIFIED. |
| Routing | junction, router, distributor, sorter, inverted sorter, overflow/underflow gate, bridge conveyor, phase conveyor | Unit tests exist per file in `core/world/blocks/distribution/`. Bridge linking UI: §7.4. |
| Unloader, container, vault | unloader, container, vault, core | Unloader config: §7.4. Unloaders pulling from cores, and core-adjacent containers merging storage (`CoreBlock` `linkedCore`): UNVERIFIED. |
| Mass driver | mass-driver | Link config UI missing (§7.4). Shooting logic: UNVERIFIED. |
| Liquids | conduit, pulse/plated conduit, liquid router, junction, bridge conduit, tanks | Existing `core/world/blocks/liquid/tests.rs`. Consumer intake fixed (`217124e`). |
| Unit factories | ground/air/naval factory | Plan selection config missing (§7.3). Items are stored outside `ItemModule` (`UnitFactoryBuild.items`), so unloaders and the inventory panel cannot see them. Follow-up: migrate to `ItemModule`. |
| Reconstructors | additive..tetrative, Erekir | §4. |
| Payload | conveyor, router, constructor, deconstructor, payload source/void, mass driver | Config kinds missing (§7.3). Transport: UNVERIFIED. |
| Launch pad, core upgrade | launch-pad, core-foundation over core-shard | Core upgrade = placing a bigger core over a smaller one (`CoreBlock.canReplace`, `core/world/build.rs::can_replace` plus `CoreBlock.canPlaceOn` which requires an existing core with `size <= this`). With player plans now in place, verify that placing `core-foundation` on `core-shard` queues, constructs and keeps items (Java `CoreBuild` merges storage). UNVERIFIED. Launch pad: Java `LaunchPad.updateTile` launches items when full and powered; campaign-side launch is session 5's domain. UNVERIFIED. |
| Sandbox | sources/voids | §3. |
| Walls / doors | all walls, door | Door tap toggle: §7.4. |
| Logic | switch, message, processor, display, memory | Switch §7.1, message §7.2. |

New gaps found during this work (not in the ledger yet; report them):
1. `count_ore` footprint offset for odd sizes of 3 or more and for size 4 (see table).
2. Generator `output_liquid` never dumped.
3. Unit factory items live outside `ItemModule`.
4. The config `String` wire collapses to `Content`, so free text can never reach a building (§7.0).
5. `MindInput.placement_preview_json` and the player plan queue have no renderer consumer (rendering agent).
6. The core starts with 0 items on a fresh Ground Zero. That matches Java for the
   tutorial, but means construction now really requires mining first. Expect players and
   tests to need `debug_add_core_items`.

**Status 2026-10-12:** done — `refresh_drill_ore` now offsets the ore scan by `-(size-1)/2`
(Java `Tile.getLinkedTilesAs`), `count_ore` applies Java's `Drill.countOre` ordering
(`lowPriority` ASC, amount ASC, id ASC, last wins), and the `ConsumeGenerator` behavior dumps its
`output_liquid` after the add (`dumpLiquid(liquid, 2f, -1)`). Tests:
`sim::production_tests::laser_drill_counts_centered_footprint` (ore only under the left column of a
3x3 laser drill at (8,8) counts 3; a tile the old `x..x+size` scan would have counted is ignored)
and `sim::production_tests::generator_dumps_output_liquid` (a fed pyrolysis generator fills an
adjacent conduit). In-engine on Ground Zero 170: a pyrolysis generator fed by slag + arkycite
liquid sources ran at efficiency 1.0 and its conduit grew water (0.083 -> 0.25). Evidence:
`runs/20261012-session4/section10-drill-generator-evidence.json`. Also found while here: drills have
no `canPlaceOn` ore requirement in the port (`world/build.rs::can_place_on` is a stub), and
`content/registries/blocks/*` never sets `destructible` for blocks that Java marks destructible-only.

---

## 11. Execution order

1. §3 PAR-0077 (small, unblocks power/item sources for other tests).
2. §10 laser-drill footprint fix and generator liquid dump (small, high value).
3. §4 PAR-0076.
4. §7.0 then §7.1, §7.2 and §7.3 (config).
5. §2a, 2b, 2c, 2d (construction polish).
6. §9 PAR-0100, then §8 PAR-0099.
7. §5 PAR-0079 (headless only).
8. §6 PAR-0089 (only after confirming with the combat session).
9. §10 remaining UNVERIFIED rows: write a test per row, fix what fails, and note the rest.

After each item: run `cargo test -p mind-core --lib` (all pass) and clippy (clean), then
`tools/build.sh`, then `tools/session-play.sh 4`. Re-verify in-engine where a recipe is
given, save evidence to `runs/20261012-session4/`, and commit.

## 12. Files likely shared with other sessions (merge carefully)

`gdext/sim_host.rs` (many sessions add funcs), `gdext/input/desktop.rs` and
`gdext/input/mod.rs` (input/HUD), `gdext/render/blocks.rs` (rendering, session 3; only the
construct-ghost lines are ours), `core/sim/mod.rs` (`Sim::tick`, command dispatch),
`core/world/behavior/mod.rs` (`default_behavior` table), `core/world/blocks/defense/turrets/*`
(combat, session 2), `core/game/runtime.rs` (campaign, session 5; one line syncs
`BuildRules.env`).
