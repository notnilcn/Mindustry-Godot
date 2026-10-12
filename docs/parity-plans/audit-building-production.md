# Audit + execution plan: building, production/logistics and block config

Status: written 2026-10-12 by the session-4 auditor. This file is self-contained.
An executor needs nothing else from earlier plans or conversations. Read §0, then
work the tasks in §4 in the order given by §6.

Path abbreviations:
- `core/` = `client/rust/mind-core/src/`
- `gdext/` = `client/rust/mind-gdext/src/`
- `ui/` = `client/ui/`
- `J/` = the read-only Java reference `/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/`

All other paths are relative to the repo checkout you work in.

---

## 0. Environment and workflow

### 0.1 Checkout and session
- Never work in the main checkout `/home/clintonnguyen/g/code_examples/Mindustry-Godot`
  directly while other agents run. Create or reuse a session worktree:
  ```bash
  cd /home/clintonnguyen/g/code_examples/Mindustry-Godot
  bash tools/parity-session.sh --session N          # N = your assigned session (2..5)
  cd ../Mindustry-Godot-sessionN
  git merge --ff-only main
  ```
  Session N uses bridge port `6970 + 10*(N-1)` (session 4 = 7000) and X display `:(9+N)`.
- **Merge the audit fixes first.** They live on branch `parity/session-4` (§3). If they
  are not yet on `main`, run `git merge parity/session-4` in your worktree before
  starting. Otherwise several tasks below describe code that does not exist yet.
- **Generated assets.** In a fresh worktree run `bash tools/pack.sh pack` once (about 1 min)
  before the first launch. Without it, sprites, fonts and locales are placeholders.
- **Build:** `CARGO_BUILD_JOBS=6 tools/build.sh` (about 1 min incremental). Expect
  `Finished `dev` profile ...` and `sync_scenarios: synced 11 file(s)`.
- **Restart after every build** (the editor mmaps the `.so`): `tools/session-play.sh N`.
  - First launch in a worktree, or after a settings reset: `tools/session-play.sh N --import`.
    It imports the full-tech-tree Java profile and prints `{"nodes": 375, "unlocked": 282}`.
  - Replay the scene only (no rebuild): `tools/session-play.sh N --play`.
- **RAM:** 14 GB is shared by up to 4 parallel sessions. Run one cargo job at a time,
  always with `CARGO_BUILD_JOBS=6`. Never start a second editor. If stray games pile up,
  check with `pgrep -af "godot --path"` (`session-play.sh` kills its own).

### 0.2 Tests
```bash
cd client/rust
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib                 # expect: ok. 1929 passed; 0 failed; 4 ignored (or more passed)
CARGO_BUILD_JOBS=6 cargo test -p mind-core --lib production_tests  # fast loop for this area
CARGO_BUILD_JOBS=6 cargo test -p mind-headless --lib             # expect exactly 2 pre-existing failures:
#   campaign_scenarios::tests::sector_golden_matches, units_scenarios::tests::committed_goldens_match
CARGO_BUILD_JOBS=6 cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   # expect no output
cargo fmt -p mind-core -p mind-gdext                              # then re-run clippy
```
- Put `#[cfg(test)] mod tests` at the **end** of a file. Clippy rejects items after a test
  module (`items_after_test_module`).
- The sim-level test pattern lives in `core/sim/production_tests.rs`, using the helpers
  `runtime_sim()`, `place(&mut sim, "name", x, y)`, `item()`, `liquid()`, `set_floor()`,
  `stored_item()`, `stored_liquid()`, `configure()` and `spawned_unit()`.
  `runtime_sim()` = a 32x32 stone world with `install_block_runtime()`. Advance with
  `sim.tick()`. For instant API placement use
  `sim.command(SimCommand::Place{.., player: None})`. For player plans use
  `player: Some(0)` (this needs a player unit; see `core/sim/builder.rs` tests,
  `sim_with_player()`).
- **Goldens:** `parity/golden_content.json` is a *port-derived* snapshot (its
  `"generator"` key says "source-derived fallback"). It is not a JVM oracle. Change it only
  when the new value matches the Java source, and say so in the commit. After editing it,
  update its sha256 in `parity/golden_manifest.json` (entry `"id": "content_ids"`) with
  `sha256sum parity/golden_content.json`. The tests `content::parity::tests::field_diff`
  and `names_ids_match_golden` compare the two.

### 0.3 Driving the game (gmcp)
```bash
export OPEN_GODOT_MCP_PORT=7000        # your session port
H='get_node("/root/Spine/SimHost")'; I='get_node("/root/Spine/Input")'; C='get_node("/root/Spine/World/Camera2D")'
python3 tools/gmcp.py --eval "return $H.get_tick()"
python3 tools/gmcp.py --screenshot runs/<stamp>-<item>/after.png
```
- Every `--eval` must be one statement that **returns** a value, for example
  `return X` or `X; return true`.
  - Calling a method that is not a `#[func]` pins the frame loop. Example: `player_unit_position()`
    is Rust-only. When the loop is pinned, `get_tick()` stops advancing.
    Recover with `tools/session-play.sh N --play`.
  - No loops or lambdas.
  - The eval budget is 15 s.
- **Ground Zero (sector 170, not 15).** Run this after `--import`:
  ```bash
  python3 tools/gmcp.py --timeout 140 \
    --eval "$H.load_sector(\"serpulo\", 170); return true" \
    --eval 'return get_node("/root/Spine/MindCampaign").start_sector("serpulo", 170)' \
    --eval 'get_node("/root/Spine/Ui/UiRoot").set_menu_visible(false); return true'
  python3 tools/gmcp.py --eval 'return get_node("/root/Spine/MindCampaign").get_hud_state()'
  #   expect "hasCore": true, "sectorName": "groundZero"
  ```
  - The player alpha spawns at world (1032, 440), which is tile (129, 55).
  - The core starts with `{}` items. That is correct: the tutorial is "obtain 15 copper".
    Seed items with `debug_add_core_items`.
  - Sandbox-only blocks (item/liquid/power source and void) are rejected by the campaign
    placement gate for **player plans**. API `place_block` bypasses the gate, so use
    `place_block` to put sandbox blocks into Ground Zero for testing.
- **Sandbox recipe.** No verified in-engine custom-sandbox launcher exists. For sandbox
  behavior use Ground Zero + `place_block` (instant, ungated) + `debug_add_core_items`,
  or headless `production_tests`.
- **Real mouse input:**
  1. Get the screen position with `$C.tile_to_screen(x, y)`, which returns viewport
     `{x, y}` for an on-screen tile.
  2. Send
     `python3 tools/gmcp.py godot_input '{"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}'`
     and then the same command with `"pressed":false`.
  3. For the wheel, use `MOUSE_BUTTON_WHEEL_UP` / `MOUSE_BUTTON_WHEEL_DOWN`.
  4. Read the zoom with `return $C.zoom` (example values `{x:4,y:-4}` → `{x:6,y:-6}`).

### 0.4 Playtest seams (all `#[func]` on `/root/Spine/SimHost` unless noted)
| Seam | Returns / does |
|---|---|
| `debug_add_core_items("copper", 300)` | adds to the first player core and returns the amount accepted (a negative amount removes) |
| `core_items_json()` | `{"copper":294,...}` |
| `apply_sim_command_json('{"op":"place","x":133,"y":55,"block":216,"player":0}')` | `"player":N` routes through the player build queue (like the mouse); without it placement is instant. `"op":"break"` works the same way |
| `content_list("block").find("copper-wall")` | the block id for the JSON above (216; `mechanical-drill` = 325) |
| `player_build_plans_json()` | `[{x,y,rotation,block,breaking,progress,stuck}]`, head = active |
| `construct_progress(x,y)` | `{block,progress,deconstructing}` or `{}` |
| `clear_player_build_plans()`, `set_player_building_paused(bool)` | queue control |
| `place_block(x,y,"name")`, `break_block(x,y)` | instant API placement (no gate, no cost) |
| `building_debug(x,y)` | `{block,enabled,efficiency,...}` plus sandbox fields |
| `building_config_json(x,y)` | the behavior `config()` as JSON (`null`, `"2.0"`, `{"type":"string","value":..}`) |
| `building_enabled(x,y)`, `light_color(x,y)`, `building_config_string(x,y)` | readbacks |
| `tile_debug(x,y)`, `power_debug(x,y)`, `liquid_debug(x,y)` | probes |
| `/root/Spine/Input` `configure_building(x,y,kind,value)` | kind: `none`/`bool`/`number`/`int`/`string`/`content` |
| `/root/Spine/Ui/UiRoot/OverlayLayer/block_config` | the config panel: `.visible`, `._config` (spec JSON as a Dictionary) |

### 0.5 Commits
- Commit one task per commit as `Fixes PAR-XXXX: <summary>`. Use `PAR-XXXX: <summary>`
  for a partial slice, and `Audit fix PAR-XXXX: ...` for an audit correction.
  - The body must say what changed, how it matches Java (file:line), how you verified it
    (test names + in-engine eval output), and any deviation.
  - End with `Co-Authored-By: <your model line>`.
- Never claim "Fixes" without the acceptance check from the task, done in-engine where the
  task gives an in-engine recipe. Save evidence (PNG/JSON) under `runs/<stamp>-<item>/`.
- `cargo fmt` also reformats the unrelated `core/io/settings.rs`. Run
  `git checkout client/rust/mind-core/src/io/settings.rs` before committing if it shows up.
- Do not edit `chains.json` or `parity-ledger.json`. Do not push.

---

## 1. Scope of this audit

Cheaper-model commits audited (all on `main`):
`c72b7f6 1a30f0d de8ef16 c94f37d 1ba8984 d5d2b94 c9cb0e8 cca8826 26f6918 c8f1ffa 11204fc 229c5d6`.

Also checked: the merge commit `c79174c` (clean: `git show --cc` has no conflict hunks),
the fmt commit `7c7e5d4`, and the golden refreshes `eec6032` / `8aaa27d`.

Baseline at the time of audit: `main` at `9aa3ef7` plus the audit fixes in §3.

---

## 2. Per-commit verdicts

Verdict key:
- **correct**: matches Java and works in-engine.
- **partial**: works, but Java behavior is missing (see the linked task).
- **wrong**: contradicts Java or regresses behavior (fixed or tasked).
- **risky**: works now but is fragile.

| Commit | Item | Verdict | Evidence |
|---|---|---|---|
| `c72b7f6` | PAR-0077 sandbox sources/voids | **correct** | Per-kind behaviors (`core/world/behavior/sandbox.rs`) port `ItemSource.updateTile` (`J/world/blocks/sandbox/ItemSource.java:90-103`, `limit = 60/itemsPerSecond`, set-1/dump/set-0), ItemVoid/LiquidVoid `accept* = enabled`, `LiquidSource.updateTile` (`LiquidSource.java:62`), PowerSource production, and PowerVoid `consumePower(f32::MAX)` (`PowerVoid.java`). Golden: item-void `solid/update = true` matches the Java ItemVoid constructor. Tests: `item_source_feeds_conveyor_into_void`, `unconfigured_item_source_emits_nothing`, `liquid_source_fills_conduit`, `power_source_powers_consumer` (all pass). In-engine: `building_debug` shows sandbox fields. Sandbox blocks cannot be player-planned in the campaign (gate), which is expected. |
| `1a30f0d` | §10 laser-drill footprint + generator liquid dump | **correct** | `refresh_drill_ore` now offsets by `-(size-1)/2` (Java `getLinkedTilesAs`). The countOre ordering (lowPriority, amount, id; take last) matches `J/world/blocks/production/Drill.java` `countOre`. `ConsumeGenerator` now calls `dumpLiquid(outputLiquid)`. Tests `laser_drill_counts_centered_footprint` and `generator_dumps_output_liquid` pass. |
| `de8ef16` | PAR-0076 reconstructors | **partial** | Correct parts: `capacities = 2x` the stack (`J/world/blocks/units/Reconstructor.java:140-165`), item capacities 80/260/1700/2000 (golden values match Java), the `constructTime` values (`J/content/Blocks.java` additive 600, multiplicative 1800, exponential 5400, tetrative 14400), `progress += edelta*unitBuildSpeed`, `progress %= 1`, and `consume()` on completion. The `has_items` change for every `Consume::Items` block matches `ConsumeItems.apply`. Test `reconstructor_needs_items_and_time` asserts 600-700 ticks plus item consumption (passes). **Missing:** (a) `shouldConsume` (`Reconstructor.java:377`), so power is drawn while idle with items loaded; (b) the configured command is never applied to the upgraded unit (`:346-352`); (c) `moveInPayload` travel before progress (`:336`). See task T3. |
| `c94f37d` | §7.0 text wire value | **correct** | Wire `ConfigValue::Text` uses tag 6 in `core/determinism/command.rs` with a round-trip test, and `resolve_config` maps `Text → String`. `MindInput.configure_building` routes `"string"`/`"int"` directly as `SimCommand::Configure`. In-engine: `configure_building(141,50,"string","hello world")` gives `building_config_json` = `{"type":"string","value":"hello world"}`. |
| `1ba8984` | PAR-0108 switch tap | **correct** | `tile_tapped` toggles through `Configure Bool(!enabled)` and opens no panel (`J/world/blocks/logic/SwitchBlock.java:58-62`). In-engine with a **real mouse click** on a switch at (131,57): `building_enabled` went true → false and `block_config.visible` = false. The Java `clickSound` is not played (minor, T9). |
| `d5d2b94` | PAR-0093 message + illuminator | **correct** (render half pending) | Message: `MessageDef{max_text 400, max_newlines 24}` (`J/world/blocks/logic/MessageBlock.java:29-30,41-60`). The panel spec has `text`/`multiline`/`current`. In-engine, a real tap on a message gives `_config = {"block":"message","current":"audit note","multiline":true,"text":true,...}`. Illuminator: `LightBehavior` stores color (default `Pal.accent`), configured through `"int"`. In-engine, `light_color` = 4278190335 after the config. **Not done:** the light is not drawn with the configured color (rendering area, T10). |
| `c9cb0e8` | PAR-0092 factory/reconstructor/payload config | **wrong → fixed** (default plan) / **partial** | The factory `config(Integer/UnitType)`, IO v3 (`J/world/blocks/units/UnitFactory.java:58-82`, write `progress,currentPlan,commandPos,command`), the payload `config_kinds` and the config panel unit/block lists are fine. **Bug:** the default plan used `unlocked()` from the block runtime's private vanilla registry (which never receives campaign research), so a fresh ground-factory on Ground Zero had config `null` and never produced. This regressed earlier behavior. Fixed in `45163d9` (fallback to plan 0; in-engine config is now `"0.0"`). **Partial:** the reconstructor command panel is suppressed (`config_spec` returns `None`); Java shows the command buttons (`Reconstructor.java:215-240`). See task T4. |
| `cca8826` | PAR-0098 §2a/§2b clear/delete/pause | **correct, but it broke a seam → fixed** | `delete_player_plans`, `clear_building` and `set_player_building_paused` are right (`J/input/DesktopInput.java:612,675`; `BuilderComp.updateBuilding`). In-engine: paused for 1 s, the drill plan stayed queued with no construct block; after resume, the drill was built and 12 copper spent; clear emptied the queue. **Bug:** the edit merged `#[func]` into a doc comment (`/// construction.    #[func]`), so `construct_progress` vanished from GDScript/MCP. Fixed in `28bf3a1`. |
| `26f6918` | PAR-0098 §2c unit overlap | **partial** | Ports `Build.checkNoUnitOverlap` (`J/world/Build.java:178-180`) with `hitboxTile = min(hitSize*0.66, 7.8)`. Test `unit_overlap_delays_construction` passes. **Deviation:** Java counts units with `isGrounded() && !type.allowLegStep` (`J/entities/Units.java:29-35`). The port uses `!PhysicsComp.flying` and ignores `allow_leg_step`, so legged units wrongly block construction. Task T6. |
| `c8f1ffa` | PAR-0098 §2d ConstructBlock IO | **correct** | Field order matches `J/world/blocks/ConstructBlock.java:484-524` (progress f32, previous/current shorts, an acsize byte with -1 meaning none, then per-entry accumulator/total f32 and itemsLeft i32 from revision 1). The test round-trips. Inferring "deconstruct" from `previous == current` only affects `set_*` defaults, which are overwritten by the read values. |
| `11204fc` | PAR-0100 focus/lock guards | **correct / risky** | The wheel over a `ScrollContainer` or with the console/chat shown is swallowed, and locks zero movement and world presses (`J/input/DesktopInput.java:237,478-482`). In-engine: wheel over the open world zooms 4 → 6; wheel over the block palette (1000,500) leaves zoom unchanged. **Risk:** `ui_scroll_at` walks every node in the scene tree on each wheel event (`root.find_children("*")`), which costs O(nodes). Task T8 replaces it with the hovered-control ancestor walk. |
| `229c5d6` | PAR-0079 Erekir drills | **wrong → fixed** (wall ore) / **partial** | The BeamDrill/WallCrafter ports and the sand attributes on walls (`J/content/Blocks.java:597-676`) are right. **Bug:** the commit made the 4 `ore-wall-*` OreBlocks `solid` and scanned only `tile.block`, claiming they are StaticWalls. In Java they are `wallOre` **overlays** (`Blocks.java:1017-1036`), and `Tile.wallDrop()` (`J/world/Tile.java:584-589`) reads the overlay. On real Erekir maps the bore found nothing. Fixed in `f25b443` (the scan ports `wallDrop`, the solid flags and golden lines are reverted, and the test now uses an overlay). **Partial:** facing is cached at placement/rotation, whereas Java `updateFacing()` runs every tick (`BeamDrill.java:244`), so walls mined or replaced later are not seen. `shouldConsume` (`:268-270`) and boost (`optionalBoostIntensity`) need a check. Task T5. |
| `c79174c` | merge session-4 → main | **correct** | `git show --cc c79174c` shows no conflict resolutions. |
| `7c7e5d4` | cargo fmt of 4 files | **correct** | Whitespace-only (`io/map/preview.rs`, `ui/text.rs`, `render/minimap.rs`, `render/objective_markers.rs`). No logic change. |
| `eec6032` | content_ids manifest hash | **correct at the time; superseded** | Hash-only update for the merged golden. `f25b443` changes the golden again (reverts the 4 wrong `solid` flags) and updates this hash to `234fb18c…`. |
| `8aaa27d` | campaign UI golden | **correct** | Only additive fields (`named`, `selectable`, `require_unlock`, `preset_unlocked`, `lock_icon`, `icon`, `content_icon`, `resources`) from the PAR-0154 and loadout work, plus a checksum. No values changed. |

In-engine evidence is in `runs/20261012-session4/`
(`audit-gz.png`, `audit-message-panel.png`). Late in the audit the session-4 game
process disconnected while idle (`No Godot instance connected`). The editor log shows only
X11 `BadWindow` noise and no Rust panic, so the cause is unknown and is not attributed to
this code. If it recurs, check `.opencode/parity-session/session-N/editor-console.log` for
`panicked`.

Area check for panics reachable from input, config or render paths (the class of the
`hover_info` freeze): the new `#[func]`s in the audited commits use `tiles.in_bounds`.
Older accessors (`construct_at`, `construct_draw`, `conveyor_items`, `building_team`,
`tile_debug`) used the panicking `Tiles::get`. Fixed in `8b6ee23`; in-engine,
`construct_progress(-5, 99999)` and `tile_debug(-1,-1)` return `{}`.
No new `unwrap()`/`expect()`/`debug_assert` reachable at runtime was found in this area
(test-only `expect` excluded).

---

## 3. Audit fixes already made (branch `parity/session-4`; merge these first)

| SHA | Fix |
|---|---|
| `28bf3a1` | Restore `#[func]` on `MindSimHost::construct_progress` (swallowed into a doc comment by `cca8826`). |
| `f25b443` | PAR-0079: `beam_facing_scan` ports `Tile.wallDrop()` (solid block `itemDrop`, else the `wallOre` overlay's drop on a non-synthetic wall). Reverts `solid: true` on `ore-wall-thorium/beryllium/graphite/tungsten` in `core/content/registries/blocks/ore.rs` and in `parity/golden_content.json`, and sets the manifest hash to `234fb18c79c50e4c0b910367123529188f87a380cacd32f1259a1aee41764e42`. `plasma_bore_mines_wall_ore` now places the ore as an overlay on `beryllic-stone-wall`. |
| `8b6ee23` | Bounds-check `construct_at`/`construct_draw`/`conveyor_items`/`building_team`/`tile_debug` in `gdext/sim_host.rs` (`tiles.getn`). |
| `45163d9` | PAR-0092: a placed unit factory defaults to plan 0 when the block runtime's registry reports no unlocked plan unit. `unit_factory_config_selects_plan` updated. |
| `90836be` | `cargo fmt` of the test above. |

After these fixes: `cargo test -p mind-core --lib` gives 1929 passed, 0 failed; mind-headless has
only the 2 pre-existing failures; clippy is clean.

---

## 4. Tasks: defects found by the audit (do these first; ordered)

Each task lists: symptom → acceptance → Java → current port → changes → tests →
verify → pitfalls.

### T0. Add a building-inventory playtest seam (prerequisite for in-engine checks)
- **Why:** several tasks need items inside a building (factory, reconstructor, crafter)
  without building belts.
- **Change:** in `gdext/sim_host.rs`, next to `debug_add_core_items`, add:
  ```rust
  /// Debug/playtest seam: adds `amount` of `item` to the building at (x, y)
  /// (clamped to `Block.itemCapacity`); returns the amount accepted.
  #[func]
  pub fn debug_add_building_items(&mut self, x: i32, y: i32, item: GString, amount: i64) -> i64
  ```
  Implementation:
  1. `let Some(tile) = self.sim.grid.tiles.getn(x, y) else { return 0 }`, then the entity
     from `tile.build`.
  2. Resolve the item via `self.content_snapshot.item_id`.
  3. Read the capacity from the `BlockTable` instance `def.item_capacity`.
  4. Call `ItemModule::add(item, amount as i32, capacity)`.
  5. Return 0 when the building has no `ItemModule`.
  Also add `#[func] building_items_json(x,y) -> GString` returning `{"silicon":40,...}`
  (wrap the existing Rust-only `block_items_json(x,y) -> Option<String>`, returning `"{}"` for `None`).
- **Verify:**
  ```bash
  python3 tools/gmcp.py --eval "return $H.place_block(145,50,\"ground-factory\")" \
    --eval "return $H.debug_add_building_items(145,50,\"silicon\",10)" \
    --eval "return $H.building_items_json(145,50)"
  ```
  The output must end with `10.0` and then `{"silicon":10}`.

### T1. `shouldConsume()` is never honoured (systemic)
- **Symptom:** blocks keep consuming and drawing power when Java would idle them.
  Examples: a reconstructor with items loaded but no unit draws power; a crafter whose
  output is full keeps burning inputs (`GenericCrafter.shouldConsume`); a full drill
  keeps requesting power.
- **Acceptance** (headless tests are the oracle; there is no power-demand seam):
  - An additive reconstructor with 40 silicon + 40 graphite and **no payload**, powered by a
    `power-source`, has `Building.should_consume == false` and `Building.efficiency == 0`,
    and the power graph's needed power excludes it (`PowerGraph::get_power_needed` returns 0
    for a graph holding only the source and the reconstructor).
  - With a dagger payload, `should_consume == true` and the graph needs 3/60 per tick
    (Java `consumePower(3f)` is per second; the port stores per tick).
  - A graphite press with 10 graphite stored (`itemCapacity` 10) and 4 coal keeps 4 coal
    over 200 ticks.
- **Java:**
  - `J/entities/comp/BuildingComp.java:1950-2010` `updateConsumption`:
    `update = shouldConsume() && productionValid()`, and `if(!update) efficiency = optionalEfficiency = 0`.
    The no-consumer/cheat branch uses `efficiency = shouldConsume() ? potential : 0`.
  - Power: `J/world/consumers/ConsumePower.java` `requestedPower` returns
    `usage * (entity.shouldConsume() ? 1 : 0)` for non-buffered consumers, and
    `J/world/blocks/power/PowerGraph.java` `getPowerNeeded` only counts consumers with
    `shouldConsumePower`.
  - Per-block overrides (line numbers):
    - `GenericCrafter.java:205-235` (output item `items.get + amount > itemCapacity` → false;
      every output liquid full and `!ignoreLiquidFullness && !dumpExtraLiquid` → false)
    - `Separator.java:92`
    - `Drill.java:247` (`items.total() < itemCapacity && enabled`)
    - `BurstDrill.java:128`
    - `BeamDrill.java:270` (`items.total() < itemCapacity && facingAmount > 0 && enabled`)
    - `WallCrafter.java:226` (`items.get(output) < itemCapacity`)
    - `Pump.java:155`
    - `SolidPump.java:117` (`liquids.get(result) < liquidCapacity - 0.01f`)
    - `UnitFactory.java:454` (`currentPlan != -1 && enabled && payload == null`)
    - `Reconstructor.java:377` (`constructing && enabled`)
    - `HeatCrafter.java:62`
    - `RepairTower.java:89`
    - `RegenProjector.java:181`
    - `ShockwaveTower.java:124`
    - Turrets (`ContinuousTurret.java:65`, `PointDefenseTurret.java:105`) belong to the
      combat area. Do not change turrets here.
- **Port today:** `core/world/update.rs` `update_consumption` (around line 406) has
  `let update = true;` (around line 466), so the hook does not exist. The no-consumer
  and cheat branches (around lines 417-430) set efficiency to `potential` unconditionally.
  The power graph: `core/world/blocks/power/graph.rs:172-195` `get_power_needed` uses
  `params.usage * if params.enabled {1} else {0}` and skips `!params.should_consume_power`.
  `params` comes from `read_power_info` in `core/world/blocks/power/module.rs` (around lines 60-110).
- **Changes:**
  1. In `core/world/behavior/mod.rs`, trait `BuildingBehavior`: add
     ```rust
     /// `Building.shouldConsume()` (default `enabled`; the caller already gates on enabled).
     fn should_consume(&self, world: &World, e: Entity) -> bool { let _ = (world, e); true }
     ```
  2. In `core/world/update.rs` `update_consumption`:
     - compute `let should = inst.behavior.should_consume(world, entity);` once;
     - replace `let update = true;` with `let update = should;`;
     - in the no-consumer and cheat branches set `eff = if should { potential * scale } else { 0.0 }`.
     Keep `potential` unchanged (Java assigns potential before zeroing).
  3. Add `pub should_consume: bool` (default `true`) to `crate::entities::comp::Building`
     (`core/entities/comp/building.rs`), and set it in `set_efficiency` (pass `should`).
     **Pitfall:** `Building` may be serialized. Check `EntityIo`/`building_io.rs`.
     If it is serialized, mark the field `#[entity(skip)]` or keep it out of IO
     (it is transient upstream).
  4. Power: in `read_power_info` expose `should_consume` (read `Building.should_consume`),
     and in `PowerGraph::get_power_needed` and the satisfaction pass around line 341 use
     `params.usage * if params.enabled && params.should_consume {1} else {0}` for
     non-buffered consumers.
  5. Overrides (each a few lines; read the state component the behavior already owns):
     - `CrafterBehavior` (`core/world/behavior/production.rs`): port
       `GenericCrafter.shouldConsume`. Use `crafter.output_items` (`(u16 item, i32 amount)`)
       against `ItemModule` and `inst.def.item_capacity`. For `output_liquids`, use
       `ignore_liquid_fullness` (field on `CrafterDef`). The port has no `dumpExtraLiquid`
       knob, so treat it as `false` and note this in a comment.
     - `Separator`: `GenericCrafter` returns `true` when there are no outputs. Java
       `Separator.shouldConsume`: `items.total() < itemCapacity && enabled`.
     - `DrillBehavior`: `items.total() < itemCapacity`.
     - `BeamDrillBehavior`: `items.total() < itemCapacity && DrillState.facing_amount > 0`.
     - `WallCrafterBehavior`: `items.get(output) < itemCapacity`.
     - `PumpBehavior`: Java `Pump.shouldConsume`
       (`liquidDrop != null && liquids.get(liquidDrop) < liquidCapacity - 0.01f && enabled`).
     - `SolidPumpBehavior`: `liquids.get(result) < liquidCapacity - 0.01`.
     - `UnitFactoryBehavior`: `current_plan >= 0` (no payload model, so skip the payload term).
     - `ReconstructorBehavior`: `state.constructing(&self.reconstructor)`. The method
       exists in `core/world/blocks/units/mod.rs`.
- **Tests** (`core/sim/production_tests.rs`):
  - `reconstructor_idle_requests_no_power`: place a power-source, an additive
    reconstructor and items, tick 10, and assert `Building.efficiency == 0` and that the
    graph's needed power excludes the reconstructor. Use
    `crate::world::blocks::power` APIs, or assert `Building.should_consume == false`.
  - `full_crafter_stops_consuming`: graphite-press with 10 graphite and 4 coal plus a
    power-source: after 200 ticks coal is still 4. Before the fix it drops.
  - `full_drill_stops`: mechanical drill on ore with 10 items → `efficiency == 0`.
- **Verify:** run the full mind-core tests. Some existing generator/crafter tests may
  assume continuous consumption; fix them only if Java agrees, and cite the line.
  In-engine on Ground Zero (requires T0):
  ```bash
  python3 tools/gmcp.py --eval "return $H.place_block(140,60,\"power-source\")" \
    --eval "return $H.place_block(142,60,\"graphite-press\")" \
    --eval "return $H.debug_add_building_items(142,60,\"graphite\",10)" \
    --eval "return $H.debug_add_building_items(142,60,\"coal\",4)"
  sleep 4
  python3 tools/gmcp.py --eval "return $H.building_items_json(142,60)"   # expect coal still 4
  ```
- **Pitfalls:** do not change turret behavior. `should_consume` must be deterministic:
  only read ECS state.

### T2. Unit factories build without power, ignore `unitBuildSpeed`, and reset progress to 0
- **Symptom:** a ground factory with silicon/lead but **no power** still produces daggers.
  The production time ignores `rules.unitBuildSpeed`. Leftover progress is dropped.
- **Acceptance:** an unpowered `ground-factory` with 10 silicon + 10 lead produces nothing
  over 3000 ticks. Powered (power-source adjacent), it produces exactly 1 dagger between
  ticks 900 and 1000 (`J/content/Blocks.java` groundFactory dagger plan time `60*15`).
  The items drop by 10/10.
- **Java:** `J/world/blocks/units/UnitFactory.java:400-450` `updateTile`:
  - `if(efficiency > 0 && currentPlan != -1){ time += edelta()*speedScl*unitBuildSpeed; progress += edelta()*unitBuildSpeed; }`;
  - on `progress >= plan.time`: `progress %= 1f`, spawn the unit, `consume()`.
  - `shouldConsume` is at `:454`.
- **Port today:**
  - `core/world/blocks/units/behavior.rs` `UnitFactoryBehavior::update_tile` (around line 232)
    calls `state.update(&self.factory, 1.0)`.
  - `core/world/blocks/units/mod.rs:165` `UnitFactoryBuild::update` adds `time_scale` (1.0)
    when `should_consume(&self.items, ..)` (items only). It sets `progress = 0.0` and
    decrements its private `items` vec.
  - `update_tile` then also removes the items from `ItemModule` (double bookkeeping).
    The private `UnitFactoryBuild.items` is re-synced from `ItemModule` each tick.
    `accept_item` reads the private copy (stale within a tick).
- **Changes:**
  1. `UnitFactoryBuild::update(&mut self, factory, delta_progress: f32, items: &ItemModule) -> Option<UnitTypeId>`:
     - read requirements from `items`, not `self.items`;
     - `if delta_progress <= 0 { return None }`;
     - `self.progress += delta_progress`;
     - on completion use `self.progress %= 1.0` and return the unit. Do not touch items
       here. Remove the `items: Vec<i32>` field from `UnitFactoryBuild`, plus every use.
       `accept_item`/`accept_stack`/`get_maximum_accepted` read `ItemModule` via
       `transfer::item_count`.
  2. `UnitFactoryBehavior::update_tile`:
     - `let team = TeamComp`;
     - `let delta_progress = crate::world::update::edelta(world, e) * unit_build_speed(world, team)`.
       `edelta` already multiplies by `Building.efficiency`, and the power gate works
       through efficiency. `unit_build_speed` is the helper in the same file, added for
       the reconstructor.
     - On `Some(unit)`: remove the plan requirements from `ItemModule` scaled by
       `unit_cost(world, team)` (Java `consume()` with the `ConsumeItemDynamic` multiplier),
       then spawn.
  3. Requirement gating comes from the consumer. The factory's item consumer is dynamic
     (`ConsumeItemDynamic`, Java `UnitFactory.java:84`). The port's generic `Consume::Items`
     consumer is not present for factories (check `def.consumes` for ground-factory in
     `core/content/registries/blocks/units.rs`; it should hold only power). So keep an
     explicit check in `update`: return `None` while any requirement is missing.
- **Tests:** `unit_factory_needs_power` (unpowered: no unit after 3000 ticks; powered:
  1 dagger in 900..=1000 ticks; items consumed). Update `unit_factory_config_selects_plan`
  if it relied on progress without power: add a power-source next to it.
- **Verify in-engine** (Ground Zero):
  ```bash
  python3 tools/gmcp.py --eval "return $H.place_block(145,50,\"ground-factory\")" \
    --eval "return $H.debug_add_building_items(145,50,\"silicon\",10)" \
    --eval "return $H.debug_add_building_items(145,50,\"lead\",10)"
  sleep 20; python3 tools/gmcp.py --eval "return $H.selectable_units_json()"   # unpowered: no new dagger
  python3 tools/gmcp.py --eval "return $H.place_block(143,50,\"power-source\")"
  sleep 20; python3 tools/gmcp.py --eval "return $H.selectable_units_json()"   # one dagger (type id of dagger) appears
  ```
- **Pitfalls:** factory IO (`write`/`read`, version 3) must stay byte-compatible; it writes
  `progress` and `current_plan` only. Keep `default_plan` (audit fix `45163d9`).

### T3. Reconstructor: apply the configured command and the payload move-in
- **Symptom:** a command configured on a reconstructor (for example `rebuild` for a
  poly) is stored but never given to the upgraded unit. The upgrade also starts the
  instant a payload is handed over.
- **Acceptance:**
  - With `command = rebuild` configured on a multiplicative reconstructor, an upgraded
    `poly` spawns with its command AI command = `rebuild`.
  - Without a configured command, the upgraded unit gets its type's `default_command`.
- **Java:** `J/world/blocks/units/Reconstructor.java:336-352`:
  `if(moveInPayload()){ ... if(progress >= constructTime){ payload.unit = upgrade(...).create(team); if(payload.unit.isCommandable()){ if(commandPos != null) ...commandPosition(commandPos); payload.unit.command().command(command == null && type.defaultCommand != null ? type.defaultCommand : command); } ...`.
  `moveInPayload` is in `J/world/blocks/payloads/PayloadBlock.java:194-215` (the payload
  slides to the centre at `payloadSpeed`; progress only counts once it arrives).
- **Port today:** `ReconstructorBuild.command: Option<u16>` exists
  (`core/world/blocks/units/mod.rs`), and `configured` handles
  `Content(UnitCommand, id)`. `spawn_output_unit` (`core/world/blocks/units/behavior.rs:51`)
  spawns with no command. Command state for live units is `CommandAiState.command`
  (`core/ai/types/command.rs:66`). Find how live player-team units get it: search
  `CommandAiState` in `core/sim/unit_runtime.rs` (PAR-0140 gives player-team units
  `AiKind::Command`).
- **Changes:**
  1. Add `fn spawn_output_unit_with_command(world, e, def, command: Option<UnitCommandId>) -> Option<Entity>`
     next to `spawn_output_unit`. After `spawn_unit_def`, set the unit's command state to
     `command.or(def.default_command)`. Use the same component/field the unit runtime reads
     for `CommandAI`. If live units carry no `CommandAiState` component, insert one built
     like the PAR-0140 path does.
  2. Call it from `ReconstructorBehavior::update_tile` with
     `state.command.map(UnitCommandId::new)`.
  3. Payload travel: add `move_in: f32` (0..1) to `ReconstructorBuild`. On accept set it
     to 0, and advance it by `edelta / 20.0` per tick (an approximation of
     `payloadSpeed = 0.7` px/tick over about 14 px; document the approximation). Progress
     only counts once `move_in >= 1`. Optional: if this complicates IO, skip it and
     record the deviation.
- **Tests:** `reconstructor_applies_command`: configure
  `Content(UnitCommand, <id of "rebuild">)`, run the upgrade (as in
  `reconstructor_needs_items_and_time`) and assert the spawned poly's command.
- **Pitfalls:** `UnitCommandId` raw ids come from `content/registries/commands.rs` order.
  Resolve by name through `ContentRegistry` (`get_by_name(ContentType::UnitCommand, "rebuild")`).

### T4. Reconstructor command panel
- **Symptom:** tapping a reconstructor opens nothing (`config_spec` returns `None`).
  Java shows the command buttons.
- **Acceptance:** tapping an additive reconstructor shows a panel whose options are the
  unit commands. Pressing `rebuild` sets `building_config_json` to
  `{"type":"content","content":"unitCommand","id":<n>}`.
- **Java:** `J/world/blocks/units/Reconstructor.java:215-240` `buildConfiguration`:
  - with `showAllCommands = true` (`:42`), it lists `Vars.content.unitCommands()` when
    no unit is held, else `unit().commands`;
  - `configure(item)`;
  - the checked state is `command == item || (command == null && unit.defaultCommand == item)`.
- **Port today:** `gdext/input/desktop.rs` `config_spec` has
  `if def.kind == BlockKind::Reconstructor { return None; }` (added by `c9cb0e8`).
  `ReconstructorBehavior::config_kinds` returns `&[]`.
- **Changes:**
  1. `ReconstructorBehavior::config_kinds` → `&[ConfigKind::Content]`.
  2. In `config_spec`, replace the early return with: for `BlockKind::Reconstructor`, push
     options for every `content.entries(ContentType::UnitCommand)` name. Use the label
     `unit.command.<name>`, or the bundle key used by the command HUD (search
     `commandmode` or `command.` in `assets` locales), with a raw-name fallback.
     Add `"current"` from `building_config_json`.
  3. The fragment submits `("content", name)`. `resolve_config` resolves unit-command names
     through `by_name` to `WorldConfigValue::Content(ContentType::UnitCommand, id)`.
     Verify that `by_name("rebuild")` is unambiguous. If a block or unit is also named
     `rebuild`, prefix the option name `unitCommand:rebuild` and teach `resolve_config`
     the prefix.
- **Tests:** a gdext-free check: `resolve_config(Content("rebuild"))` returns
  `Content(UnitCommand, _)` (add it to `core/sim/runtime.rs` tests).
- **Verify in-engine:** place `additive-reconstructor` with `place_block` near (131,57),
  tap it with a real click (§0.3), then read `._config.options` and check that it has
  10 entries.

### T5. Plasma bore / cliff crusher: per-tick facing, shouldConsume, boost
- **Symptom:** after placement the bore keeps using the cached facing scan. A wall mined,
  removed, or built in front later is ignored. The water boost is unverified.
- **Acceptance:** remove the ore wall in front (set the tile block to `AIR`) and the bore
  stops producing within 1 tick. Restore it and production resumes. With water supplied
  (optional consumer), the drill speed multiplier equals `optionalBoostIntensity`
  (plasma-bore: check `J/content/Blocks.java`, search `plasmaBore = new BeamDrill`, for
  `optionalBoostIntensity` and the `consumeLiquid(...).boost()`).
- **Java:** `J/world/blocks/production/BeamDrill.java`:
  - `237-266` `updateTile`: `updateFacing()` every tick,
    `multiplier = lerp(1, optionalBoostIntensity, optionalEfficiency)`,
    `time += edelta()*multiplier`, and when `time >= drillTime` adds one `wallDrop()` per
    facing tile if `items.total() < itemCapacity`, then dumps on the timer;
  - `270` `shouldConsume`;
  - `375+` `updateFacing`.
  - `J/world/blocks/production/WallCrafter.java:187-227`: `getEfficiency` is called
    every tick.
- **Port today:** `core/world/behavior/production.rs` `BeamDrillBehavior::update_tile`
  and `WallCrafterBehavior::update_tile` read `DrillState.facing_amount/facing_item/facing_drops`,
  which are filled only by `refresh_drill_ore` (on placement and `SimCommand::Rotate`,
  `core/sim/mod.rs`). The behavior has no grid access.
- **Changes:** the behaviors cannot see `WorldGrid`. Re-run the scan from `Sim::tick`:
  1. Add `pub(crate) fn refresh_facing_drills(&mut self)` in `core/sim/runtime.rs`
     (`impl Sim`). It iterates entities with `DrillState` whose block kind is
     `BeamDrill | WallCrafter`, and calls
     `crate::world::behavior::production::refresh_drill_ore(world, &self.grid, content, e)`.
  2. Call it in `Sim::tick` (`core/sim/mod.rs`) right after `self.schedule.run(...)` and
     before `update_builders()`. To bound the cost, keep a `Vec<Entity>` index of facing
     drills on `SimRuntime`, updated in `set_block_raw` / `remove_block_raw`.
  3. Boost: check `BeamDrillDef` for `optional_boost_intensity`. If it is missing, add it
     (default `2.5` per Java `BeamDrill`; verify the field default in
     `J/world/blocks/production/BeamDrill.java` near line 45) and use
     `Building.optional_efficiency`.
  4. `shouldConsume` comes via T1.
- **Tests:** `plasma_bore_stops_when_ore_removed` (after 400 ticks of production, set the
  overlay to AIR, record the count, tick 400, and assert the count is unchanged).
- **Pitfalls:** the ore is an **overlay on a static wall** (audit fix `f25b443`); the scan
  must keep using the `wallDrop` logic. In-engine verification is blocked: no Erekir
  sector is launchable in the imported profile. Use tests.

### T6. Unit-overlap filter: legged units and grounded state
- **Java:** `J/entities/Units.java:29-35`: a unit counts when
  `unit.isGrounded() && !unit.type.allowLegStep`. `isGrounded()` is `elevation < 0.001`
  (`J/entities/comp/UnitComp.java` / `ElevationMoveComp`).
- **Port:** `core/sim/builder.rs` `no_unit_overlap` skips only `PhysicsComp.flying`.
- **Change:** also skip units whose type has `allow_leg_step()` true (the content
  `UnitTypeDef` accessor at `core/content/registries/units/mod.rs:174`; resolve via
  `UnitTypeComp.type_id`). Use the elevation component if one exists (search
  `ElevationComp`/`elevation` in `core/entities/comp/unit/comp.rs`) instead of `flying`.
- **Test:** `legged_unit_does_not_block_construction` (spawn `atrax` on the tile; the
  plan still builds).

### T7. Solid pumps ignore floor attributes (water extractor, oil extractor)
- **Symptom:** a water extractor pumps the same amount on any floor. An oil extractor on
  sand pumps nothing extra, and on stone it pumps.
- **Acceptance:** on 4 `sand-floor` tiles (water attribute, for example 0? check
  `Blocks.java` floor `attributes.set(Attribute.water, ...)`), the output rate equals
  `pumpAmount * (validTiles + boost)` per tick:
  - water-extractor: `pumpAmount = 0.11`, `attribute = water`, `baseEfficiency = 1`
    (`Blocks.java:2941-2946`);
  - oil-extractor: `pumpAmount = 0.25`, `attribute = oil`, `baseEfficiency = 0`
    (`:2985-2992`), so it pumps only on oil-attribute floors (sand, shale, tar).
- **Java:** `J/world/blocks/production/SolidPump.java`:
  - `80` `canPump = !floor.isLiquid`;
  - `150-160` `onProximityUpdate`: `boost = sumAttribute(attribute)/size²`,
    `validTiles += baseEfficiency/size²` per pumpable tile;
  - `122-135` `updateTile`: `fraction = max(validTiles + boost + attribute.env(), 0)`,
    `maxPump = min(cap - stored, pumpAmount*delta*fraction*efficiency)`;
  - `68-75` `canPlaceOn` for multiblocks: `sum > 0.00001`;
  - `J/world/Block.java:582-588` `sumAttribute`.
- **Port:** `core/world/block_kind_data.rs:129` `SolidPumpDef { pump_amount, attribute: i16 (-1), base_efficiency }`;
  `from_def` forces `attribute = -1`. `core/world/behavior/production.rs`
  `SolidPumpBehavior::update_tile` adds `amount * edelta * warmup`.
- **Changes:**
  1. Change `SolidPumpDef.attribute` to `String` (`""` = none), and set it from the block
     name: `water-extractor → "water"`, `oil-extractor → "oil"`. `Fracker` is the
     oil-extractor class; check which kind data it uses (`FrackerDef`) and add the same
     field. Set `base_efficiency` (water 1.0, oil 0.0).
  2. Add `valid_tiles: f32, boost: f32` to `PumpState` (`core/entities/comp/building.rs`).
     Extend `refresh_drill_ore`'s pump branch (`refresh_pump_floor`, which the
     `BlockKindData::Pump` match calls) to also handle `SolidPump`/`Fracker`: compute both
     values from the footprint floors (`floor.is_liquid`, `floor.attributes` by name).
  3. `SolidPumpBehavior::update_tile`:
     `fraction = (valid_tiles + boost).max(0.0)` (ignore `attribute.env()`, which is 0 on
     Serpulo, and note it);
     `max_pump = (cap - stored).min(pump_amount * delta * fraction * efficiency)`.
     Use warmup only for visuals.
  4. `canPlaceOn` in `core/world/build.rs` `valid_place_at`: for SolidPump/Fracker
     multiblocks, require the footprint sum `> 0.00001`.
- **Tests:** `water_extractor_scales_with_floor`, `oil_extractor_needs_oil_floor`.
- **Pitfall:** the attribute names in the port's `BlockDef.attributes` are strings such as
  `"sand"` and `"water"`. Grep `attributes: vec![("water"` to confirm the floor data
  exists. If it does not, add the Java values from `J/content/Blocks.java` floor
  definitions (search `Attribute.water`, `Attribute.oil`).

### T8. Wheel guard walks the whole scene tree on every scroll
- **Port:** `gdext/input/mod.rs` `ui_scroll_at` calls `root.find_children("*")` per wheel
  event.
- **Change:** use `self.base().get_viewport()?.gui_get_hovered_control()` and walk
  `get_parent()` ancestors for `ScrollContainer`. If no control is hovered, return false.
  Keep the console/chat check.
- **Verify:** use the same in-engine recipe as the audit (§0.3). The wheel at (300,150)
  over the world zooms (`$C.zoom` 4 → 6). At (1000,500) over the block palette the zoom
  is unchanged.

### T9. Switch tap click sound (minor)
- **Java:** `SwitchBlock.java:58-62` `clickSound.at(this)`.
- **Change:** in `gdext/input/desktop.rs` `tile_tapped`'s switch branch, emit the UI click
  through the existing audio path (search `play_ui`/`AudioEvent` in `gdext/input`). Use
  the `click` sound name as used elsewhere in the gdext UI. Optional.

### T10. Illuminator color is not rendered (rendering area: hand off, do not implement here)
- `LightBehavior` stores the color (`light_color(x,y)` seam). The light pass
  (`J/world/blocks/power/LightBlock.java:126-129` `drawLight` with
  `radius 200 * min(smoothTime, 2)` and `brightness 0.9 * efficiency`) belongs to the
  rendering work. Record it in the final report for the rendering owner.

### T11. `Building.placed()` is never called: power nodes and consumers do not auto-link
- **Symptom:** placing a power node next to generators and consumers creates no laser
  links. Placing a consumer inside a node's laser range does not connect it. Java does
  both automatically. `LiquidVoid.placed()` (liquids clear) and any other `placed()`
  override never run.
- **Acceptance:** on Ground Zero:
  1. `debug_add_core_items` 500 copper and 500 lead.
  2. Instantly place `combustion-generator` at (150,60) with `place_block`.
  3. Queue `power-node` at (154,60) through `apply_sim_command_json` with `"player":0`
     (power-node id from `content_list("block").find("power-node")`), and tick until
     `construct_progress(154,60)` is `{}`.
  4. Then `power_debug(154,60)["graph_id"]` must equal `power_debug(150,60)["graph_id"]`
     (`power_debug` returns `graph_id, init, satisfaction, status, stored`).
  5. Next, `place_block(156,60,"laser-drill")` (a consumer inside the node's laser range).
     `power_debug(156,60)["graph_id"]` must equal the node's graph id.
- **Java:**
  - `J/entities/comp/BuildingComp.java:1359-1370` `placed()`: when
    `(consumesPower || outputsPower) && hasPower && connectedPower`, run
    `PowerNode.getNodeLinks(tile, block, team, other -> other.configureAny(pos()))`.
  - `J/world/blocks/power/PowerNode.java:400-411` `placed()`: when it has no links, run
    `getPotentialLinks(tile, team, other -> configureAny(other.pos()))`.
  - `PowerNode.java:278` `getNodeLinks`.
  - `J/world/blocks/ConstructBlock.java:151-156` `constructed()` calls
    `tile.build.placed()` after `constructFinish`.
  - `J/world/blocks/distribution/ItemBridge.java:224-233` `playerPlaced`: link the
    previously placed bridge (`findLink`/`lastBuild`) to the new one.
- **Port today:**
  - `BuildingBehavior::placed` exists (`core/world/behavior/mod.rs:168-169`), but no live
    path calls it (`grep -rn "behavior.placed(" core` only finds a sandbox test).
  - `core/world/blocks/power/nodes.rs:205` `get_potential_links` (node → others) and
    `:317` `configure_link(graphs, world, grid, tile, packed)` exist.
  - There is no port of `getNodeLinks` (consumer → nodes in whose laser range it lies).
  - Placement paths:
    - `SimRuntime::set_block_raw` (`core/sim/runtime.rs` around line 296), used by
      `place`, the builder's construct-finish (`core/sim/builder.rs`) and `begin_place`;
    - `materialize_buildings` (map load, which must **not** call `placed`, as in Java).
- **Changes:**
  1. Add `pub(crate) fn placed_hooks(&mut self, world, grid, entity)` to `SimRuntime`. It:
     - calls `inst.behavior.placed(world, entity)` (look up via `BlockTable`);
     - if the def `has_power && (consumes power || outputs power) && connected_power`
       (check the field names on `BlockDef`; `connected_power` may be `conductive_power`),
       computes the nodes in range: port `PowerNode.getNodeLinks` (`PowerNode.java:278-300`)
       as `pub fn get_node_links(world, grid, block_def, tile_pos, team, out: &mut Vec<Entity>)`
       in `nodes.rs` (the laser range check `overlaps(src, other, range)` and `maxNodes`
       not yet reached), then `configure_link(graphs, world, grid, node, pos.pack())` for each;
     - if the block is a `PowerNode` with no links, runs `get_potential_links` and
       `configure_link(graphs, world, grid, entity, other_pos.pack())` for each.
     `PowerGrids` is an ECS resource; take it out with `world.resource_scope`.
  2. Call `placed_hooks` from `SimRuntime::place` (API placement; Java also calls `placed()`
     for instant builds via `constructed`) and from the builder's construct-finish branch in
     `core/sim/builder.rs` (after `set_block_raw(..., state.current, ...)`). Do **not** call
     it for the construct block itself (`build<size>`) or for map materialization.
  3. Bridge auto-link: in the builder's construct-finish branch, when the block kind is
     `ItemBridge`/`BufferedItemBridge`/`LiquidBridge`, find the nearest same-block bridge
     behind it in range, the way `findLink` does (`ItemBridge.java`, search
     `public Tile findLink`; Java uses `lastBuild`, the last bridge this player placed).
     Store `last_bridge: Option<(BlockId, TilePos)>` on `BuilderPlans` and apply
     `configure` of the old bridge to point at the new one (`Point2` relative). Only for
     player-built bridges, matching `playerPlaced`.
- **Tests:**
  - `node_autolinks_on_place` (generator, then a node in range placed via
    `SimCommand::Place{player:None}` → the node's `PowerModule.links` contains the
    generator's pos);
  - `consumer_autolinks_to_node_in_range`;
  - `bridges_chain_when_player_placed` (`sim_with_player`, two bridge plans 3 tiles apart
    → the first links to the second).
- **Pitfalls:** do not double-link (check `links.contains` first, as Java does). Running
  hooks during `materialize_buildings` would rewrite saved links. Keep the existing
  `merge_power_graph` call.

### T12. Tap-to-link config (bridges, mass drivers, power nodes)
- **Symptom:** tapping a bridge opens a panel (or item options) instead of entering link
  mode. Tapping a second bridge or node does not link them.
- **Acceptance:** tap bridge A, then tap bridge B in range: `building_config_json(A)`
  becomes `{"type":"point",...}` pointing at B. Tapping B again unlinks it. The same works
  for `power-node` (adds/removes a laser link) and `mass-driver`.
- **Java:**
  - `J/input/InputHandler.java:1973-2010` `tileTapped`: when a config is shown, the
    selected building's `onConfigureBuildTapped(other)` decides first; returning false
    keeps the selection and consumes the tap.
  - `J/world/blocks/distribution/ItemBridge.java:299-315`
  - `J/world/blocks/distribution/MassDriver.java:272`
  - `J/world/blocks/power/PowerNode.java:419-440`
- **Port today:** `gdext/input/desktop.rs` `tile_tapped` (around line 1069) always opens the
  tapped building's own config. There is no "selected config building" state.
  `ItemBridgeBehavior::configured` accepts `Point2` (relative) or a packed `Number`
  (`core/world/blocks/distribution/item_bridge.rs:335-353`).
- **Changes:**
  1. Add `config_selected: Option<(i32,i32)>` to `DesktopBridge` (`gdext/input/desktop.rs`).
     Set it when `Effect::OpenBlockConfig` is pushed, and clear it when the panel closes.
     The fragment's close button calls `MindInput.close_block_config()`; add that
     `#[func]` to `gdext/input/mod.rs` and call it from
     `ui/fragments/block_config_fragment.gd` `visible = false` paths.
  2. In `tile_tapped`, before opening a config: if `config_selected = Some(sel)` and the
     selected block kind is a bridge, mass driver or power node, call a new
     `fn on_configure_build_tapped(sel, target) -> bool` that ports the Java method:
     - bridge: if the target is a bridge linking back to sel, configure the target with
       `-1` and sel with `target`; else if in range and in line (`link_valid`, port it from
       `ItemBridge.linkValid`: same axis, distance <= range), toggle the link via
       `configure_building(sel, "int", packed)` (the `Number` path), and return false;
     - power node: `nodes::link_valid` (`core/world/blocks/power/nodes.rs:142`), then
       configure the node with the packed target pos (`configure_link` toggles); return false;
     - mass driver: the same pattern with `MassDriver.linkValid`.
     When it returns false, the tap is consumed and the panel stays bound to sel. When it
     returns true, open the target's config normally.
  3. In `config_spec`, return a minimal spec with no options (title only) for these kinds,
     so the panel only shows a close button.
- **Tests:** a core unit test for a bridge `link_valid` port. The tap logic is gdext, so
  verify it in-engine with two real taps (§0.3).
- **Pitfalls:** configure bridges with **relative** `Point2` or **packed absolute** ints
  consistently. `configured` treats `Number` as packed absolute.

### T13. Door tap toggle
- **Java:** `J/world/blocks/defense/Door.java:148-154` `tapped()`: unless a unit stands in
  an open door or the 60-tick `timerToggle` is pending, `configure(!open)`. `Door.configurable`
  is false in Java (it toggles through `tapped`). Check `consumesTap`.
- **Port:** `DoorBehavior::configured(Bool)` exists (`core/world/behavior/defense.rs:188-205`).
  There is no tap path.
- **Change:** in `gdext/input/desktop.rs` `tile_tapped`, mirror the switch branch for
  `BlockKind::Door` (and `AutoDoor` if present): read `building_config_json` (Bool) and
  emit `Configure Bool(!open)`. Add the 60-tick throttle in core: add `toggle_timer` to
  `DoorState` and ignore `configured` calls within 60 ticks. The unit-in-door check uses
  the same overlap helper as T6.
- **Test:** `door_toggles_and_throttles`.

### T14. PAR-0089: sublimate fires on any trace of fuel and never consumes it
(This is turret code, shared with the combat area: check `git log -- client/rust/mind-core/src/world/blocks/defense/turrets` for an existing fix first. If one exists, skip.)
- **Symptom:** a sublimate with a drop of ozone fires continuously and never drains it.
- **Acceptance:** sublimate with 0.5 ozone never fires. With 2.0 ozone it fires, and the
  ozone falls by `liquidConsumed` (18/60) each tick while shooting. When the ozone drops
  below `liquidConsumed` it stops, and it restarts only once the ozone is `>= 4*liquidConsumed`.
- **Java:**
  - `J/world/blocks/defense/turrets/ContinuousLiquidTurret.java`:
    - `15` `liquidConsumed = 1/60` default;
    - `42` the `ConsumeLiquidFilter(ammoTypes.containsKey, liquidConsumed)` consumer;
    - `83-91` the `activated` latch (`>= liquidConsumed*4` sets it, `< liquidConsumed`
      clears it);
    - `108-110` `shouldConsume = super && activated`;
    - `113-116` `useAmmo` consumes nothing;
    - `124-126` `hasAmmo = hasCorrectAmmo && ammoTypes.get(current) != null && currentAmount > 0 && activated`.
  - The sublimate block in `J/content/Blocks.java` (search `sublimate = new ContinuousLiquidTurret`)
    sets `liquidConsumed = 10f/60f` or similar. Read the exact value there.
- **Port today:**
  - `core/world/blocks/defense/turrets/mod.rs:863` `has_ammo_state`, `TurretAmmo::Liquid`:
    `liquids.get(l) >= 1/ammo_multiplier`;
  - there is no `activated` latch and no per-tick drain;
  - `core/world/blocks/defense/turrets/advanced.rs` (around line 1576) holds the sublimate
    config.
  - Filter consumers are not lowered (`core/world/consumers.rs:23` has no filter variant).
- **Changes:**
  1. Add `liquid_consumed: f32` to `TurretConfig` (default `1.0/60.0`; sublimate = the
     Java value) and `activated: bool` to `TurretState`.
  2. In the continuous-turret tick (`TurretKind::Continuous` with `TurretAmmo::Liquid`),
     find the current liquid as the max stored among the ammo liquids
     (`LiquidModule.current` if it exists), then:
     `if amt >= cfg.liquid_consumed*4 { activated = true } else if amt < cfg.liquid_consumed { activated = false }`.
  3. `has_ammo_state` for a Liquid ammo with `kind == Continuous`:
     `activated && amt > 0 && ammo entry exists`.
  4. While `activated && Building.efficiency > 0 && the turret is shooting`
     (`wasShooting`/`is_shooting` in TurretState), remove `liquid_consumed * delta` of the
     current liquid. This ports `ConsumeLiquidFilter.update` with `shouldConsume`.
- **Tests:** `sublimate_activation_window` (0.5 → no fire; 2.0 → fires and drains 18/60
  per tick, using the Java value; draining below the threshold stops it).
- **Pitfalls:** do not touch item/power turrets. No in-engine Erekir sector exists, so test
  headless only.

### T15. PAR-0099: schematic copy mode (F drag → place)
- **Symptom:** holding F and dragging selects nothing. Z/X do nothing.
- **Acceptance:** with 3 conveyors placed in a row on Ground Zero: hold F at the first,
  release at the last, move the mouse 5 tiles right, and left-click. After the click,
  `player_build_plans_json()` lists 3 conveyor plans 5 tiles right of the originals with
  the same rotations. Pressing X before the click flips them (rotations mirrored).
- **Java:**
  - `J/input/DesktopInput.java:616-657`: `schemX/schemY` on the `schematicSelect` tap;
    on release `lastSchematic = schematics.create(schemX, schemY, rawCursorX, rawCursorY)`
    and `useSchematic`; flips at `647-654`;
  - `529-536` `useSchematic` (`selectPlans = schematics.toPlans(schem, x, y)`);
  - `596-606` plans follow the cursor;
  - `J/input/InputHandler.java:1558` `rotatePlans`, `:1596` `flipPlans`;
  - the commit is `InputHandler.flushPlans(selectPlans)` on a left tap.
- **Port pieces:**
  - `core/game/schematics.rs:724` `Schematics::create(world: &dyn SchematicWorld, registry, x, y, x2, y2, max_size)`
    and `:640` `to_plans(index, x, y, check_hidden, registry)`;
  - `EcsSchematicWorld { grid, world, content }` (`core/game/schematics.rs:115-126`);
  - `core/input/line.rs:219` `flip_plans(plans, flip_x, flip_y)`;
  - bindings `schematic_select`=`f`, `schematic_flip_x`=`z`, `schematic_flip_y`=`x`
    (`core/input/binding.rs:412-428`);
  - `gdext/input/desktop.rs` `key_down_event` has no arm for these;
  - `RawEvent::KeyUp` is handled around line 380.
- **Changes:**
  1. Add `schem_start: Option<(i32,i32)>` and `select_plans: Vec<ClientPlan>` to
     `DesktopBridge`.
  2. On key-down `schematic_select` (not `text_focus`, no break drag): set
     `schem_start = cursor_tile()`.
  3. On KeyUp of the `schematic_select` binding: add the host `#[func]`-free Rust method
     `MindSimHost::schematic_plans(x1,y1,x2,y2, cx, cy) -> Vec<(i32,i32,u8,u16)>`. It
     builds an `EcsSchematicWorld` over `self.sim.grid`/`self.sim.ecs.0`/the runtime
     content, calls `Schematics::create(.., max_size = 64)` (Java `Vars.maxSchematicSize = 64`),
     and converts the tiles to plans centred on the cursor (mirror `to_plans` offset math).
     Store the result in `select_plans`.
  4. `mouse_move`: shift every plan by the cursor delta. Write `preview_json` in the same
     JSON shape `update_place_line` uses, so the renderer can show it.
  5. `schematic_flip_x/y`: call `flip_plans`.
  6. `scroll` with `select_plans` non-empty: add `rotate_plans(plans, direction)` to
     `core/input/line.rs` (port `InputHandler.rotatePlans`, `InputHandler.java:1558-1594`)
     and return.
  7. `left_press` with `select_plans` non-empty: enqueue
     `SimCommand::Place{.., player: Some(0)}` per plan (as `commit_place` does), then clear.
     `right_press`/`clear_building`: clear `select_plans`.
- **Tests:** `core/input/line.rs` `rotate_plans_four_times_is_identity`,
  `rotate_plans_quarter_turn`. In-engine: the acceptance recipe, with keys injected via
  `godot_input` key events (see `.claude/skills/playtest/SKILL.md` for the `key` action
  schema).
- **Pitfalls:** schematic tiles store the block **origin** offsets. Multiblocks need the
  `offset`/size handling that `to_plans` does. Copy that code; do not reinvent it.

### T16. Construction remainder §2e: plan drawing and build beam (rendering handoff)
- **Data seams that exist:** `player_build_plans_json()`, `MindInput.placement_preview_json()`
  (`gdext/input/mod.rs`), `MindSimHost::construct_draw`/`construct_at` (used by
  `gdext/render/blocks.rs` for the construct ghost).
- **Java:**
  - `J/entities/comp/BuilderComp.java:337-386` `drawBuilding`/`drawBuildingBeam`;
  - `J/world/blocks/ConstructBlock.java:251-275` `draw` (the `blockbuild` shader with
    `progress`);
  - `J/input/InputHandler.java` `drawPlan`/`drawBreaking`.
- **Task:** hand this to the rendering owner; do not implement it here. Record it in the
  final report.

### T17. Construction remainder §2f: rebuild ghosts and `rebuild_select` (B)
- **Java:** `J/game/Teams.java` `TeamData.plans` gets a `BlockPlan` on block death when
  `state.rules.ghostBlocks`. Also `J/input/InputHandler.java` `rebuildArea`, and
  `J/input/DesktopInput.java:616,635-640` (`rebuildSelect`).
- **Port:** `crate::game::teams::BlockPlan` and `TeamData.plans` exist (campaign AI uses
  them). Check where they are filled on building death: search `plans.push` in
  `core/game/` and `core/sim/`.
- **Changes:**
  1. Make sure building destruction (not deconstruction) pushes a `BlockPlan` for the team
     when `rules.ghost_blocks`.
  2. Add a `rebuild_select` key edge to `gdext/input/desktop.rs` (as in T15 step 2).
  3. On release, call a new `Sim::queue_team_rebuild(x1,y1,x2,y2)` that converts the
     team's `BlockPlan`s inside the rectangle into `queue_player_place` calls.
- **Test:** destroy a wall (`destroy_building` helper in `core/sim/runtime.rs`), call
  `queue_team_rebuild` over it, and assert 1 player plan.

### T18. Construction remainder §2g: schematic placement goes through plans
- **Port:** `gdext/campaign.rs` `place_schematic_base64` (around line 1471) calls
  `MindSimHost::place_block_at` (instant, rotation dropped).
- **Change:** when `self.sim.player_can_build()` is true, call
  `self.sim.queue_player_place(x, y, block, rotation)` per tile instead (the plan queue
  path). Otherwise keep instant placement. Remove the `let _ = rot;` in `place_block_at`
  and pass rotation through `SimCommand::Place` (the live runtime honours rotation).
- **Test:** a gdext-free core test for `queue_player_place` with rotation 1 → the plan
  rotation is 1 and the built block rotation is 1.

---

## 5. Per-block-family sweep (status and tasks)

Status values:
- OK: verified by a passing test.
- BROKEN: confirmed by code reading or in-engine; a task exists.
- UNVERIFIED: write the listed test first; fix only if it fails against Java.

Each UNVERIFIED row is its own small task. Add the test to `core/sim/production_tests.rs`
using the place-and-tick pattern, cite the Java method in the test doc comment, then fix
or record the result.

| Family | Blocks | Status | Task / test to write |
|---|---|---|---|
| Player construction | all | OK (in-engine): wall built over time, 6 copper spent; drill 12 copper; pause/resume; clear; out-of-range waits; break refunds. | T6, T11, T17, T18 |
| Mechanical/pneumatic drill (2x2) | | OK | (T1 shouldConsume) |
| Laser/blast drill (3x3, 4x4) | | OK footprint (`1a30f0d`) | UNVERIFIED power + water boost: `laser_drill_needs_power_and_boosts` (Java `Drill.java` `updateTile`, `liquidBoostIntensity` 1.6) |
| Erekir drills | plasma-bore, large-plasma-bore, cliff-crusher, impact-drill | partial | T5. Impact drill (BurstDrill): UNVERIFIED `impact_drill_bursts` (`BurstDrill.java` `updateTile`) |
| Pumps | mechanical/rotary/impulse | OK floor + dump | UNVERIFIED rotary/impulse power gating (T1) |
| Solid pumps | water-extractor, oil-extractor | BROKEN (no attribute scaling) | T7 |
| Crafters | graphite-press, silicon-smelter, kiln, pulverizer, melter, separator, disassembler, spore-press, cryofluid-mixer, plastanium-compressor, phase-weaver, surge-smelter, pyratite-mixer, blast-mixer | items + liquids OK; dump count matches Java `GenericCrafter.dumpOutputs` (`GenericCrafter.java:335-351`) | T1 (output-full gating). UNVERIFIED `separator_weighted_results` (`Separator.java` `updateTile` random weights; the port uses a deterministic LCG, so assert only the distribution bounds over 1000 crafts) |
| Generators | combustion, steam, differential, rtg, solar, large-solar, thermal, impact-reactor, thorium-reactor, pyrolysis, neoplasia | intake OK (`5a79e4d`), output liquid dump OK (`1a30f0d`) | UNVERIFIED: `thermal_generator_reads_heat_floor` (`ThermalGenerator` `sumAttribute(heat)`), `thorium_reactor_heats_without_coolant` (`NuclearReactor.java` `updateTile`), `differential_needs_cryofluid` |
| Power graph | nodes, battery, diode, surge tower | BROKEN auto-link (T11), tap-link (T12) | battery tests exist in `core/world/blocks/power/tests.rs` |
| Conveyors | conveyor, titanium, armored, plastanium | OK per the existing distribution tests | UNVERIFIED `plastanium_conveyor_moves_stacks` (`StackConveyor.java` `updateTile`) |
| Routing | junction, router, distributor, sorter (inverted), overflow/underflow gate | OK per tests in `core/world/blocks/distribution/` | sorter config panel OK |
| Bridges | bridge-conveyor, phase-conveyor, bridge-conduit | BROKEN (no tap-link T12, no auto-chain T11) | |
| Unloader / container / vault | | config panel falls back to the item list (`Unloader` declares no `config_kinds`) | add `config_kinds = &[ConfigKind::Item]` in `core/world/blocks/storage/unloader.rs`; UNVERIFIED `unloader_pulls_from_core` (`Unloader.java:165`) |
| Mass driver | | BROKEN link UI (T12) | UNVERIFIED `mass_driver_shoots_to_linked` (`MassDriver.java:139`) |
| Liquids | conduits, router, junction, tanks | OK (`core/world/blocks/liquid/tests.rs`, consumer intake) | |
| Unit factories | ground/air/naval | BROKEN power gating (T2); default plan fixed (`45163d9`) | T2 |
| Reconstructors | additive..tetrative, Erekir | partial (T1, T3, T4) | |
| Payload blocks | conveyor, router, constructor, deconstructor, source, void, payload mass driver | config kinds OK (`c9cb0e8`) | UNVERIFIED `constructor_builds_selected_block` (`Constructor.java`, `BlockProducer.updateTile`) |
| Core upgrade | core-foundation over core-shard | UNVERIFIED | Test `core_upgrade_keeps_items`: place `core-shard` with 100 copper, queue `core-foundation` over it via a player plan, run to completion, assert the block is foundation and copper is still >= 100 minus the cost. Java: `J/world/blocks/storage/CoreBlock.java:180-215` (`canReplace`, `canPlaceOn` requires an existing smaller core, `placeBegan`). The port's `can_replace` (`core/world/build.rs`) has no core rule; add `CoreBlock.canReplace`/`canPlaceOn` if the test fails. |
| Launch pad | | UNVERIFIED | `launch_pad_launches_when_full` (`J/world/blocks/campaign/LaunchPad.java:143`); the campaign side belongs to the campaign area. |
| Sandbox | sources/voids | OK (`c72b7f6`) | |
| Doors | door, door-large | BROKEN tap toggle | T13 |
| Logic | switch, message, processor, display, memory | switch/message OK | |
| Illuminator | | config OK; light render pending (T10) | |

New gaps found (not yet in the ledger):
1. `shouldConsume` is never honoured (T1). This is systemic.
2. `Building.placed()` is never called, so there is no power auto-link and no bridge
   chaining (T11).
3. Unit factories ignore power and unitBuildSpeed, and keep a private item vector (T2).
4. Solid-pump floor attributes are not ported (T7).
5. Door taps do not toggle (T13). Bridge/node/driver tap-linking is missing (T12).
6. The block runtime boots its own vanilla `ContentRegistry` (`load_vanilla_content`), so
   any `unlocked()` read inside behaviors ignores campaign research. Audit fix
   `45163d9` works around this for factories. Any future behavior that checks unlocks
   must not trust that registry.

---

## 6. Execution order and dependencies

1. Merge `parity/session-4` (§3).
2. T0 (seam), then T1, T2 and T3 (T2 and T3 depend on T1's `should_consume` hook).
3. T11, then T12 (T12 uses the link helpers from T11).
4. T7, T5 and T6.
5. T4, T13 and T9.
6. T15, then T18 (schematic placement via plans), then T17.
7. T14 (only after checking the combat area has not already done it).
8. §5 UNVERIFIED rows, one test each.
9. T8 (performance).

Hand T10 and T16 to the rendering owner.

After every task:
1. `cargo test -p mind-core --lib`: all pass.
2. `mind-headless --lib`: only the 2 known failures.
3. clippy: clean.
4. `tools/build.sh`, then `tools/session-play.sh N`.
5. Run the in-engine recipe from the task.
6. Commit with evidence.

## 7. Files shared with other areas (pull `main` before editing; keep commits small)
- `gdext/sim_host.rs`: every area adds `#[func]`s. Watch for doc-comment and attribute
  merges such as the one fixed in `28bf3a1`. After editing, run
  `grep -nE '///.*\S\s{2,}#\[' client/rust/mind-gdext/src/sim_host.rs` and expect no output.
- `gdext/input/desktop.rs` and `gdext/input/mod.rs` (input/HUD).
- `gdext/render/blocks.rs` (rendering; only the construct-ghost lines belong to this area).
- `core/sim/mod.rs` (`Sim::tick` runs `update_builders()` then `resolve_deaths()`; T5 adds
  the facing refresh).
- `core/sim/runtime.rs` (`remove_building` is the shared removal helper behind
  `break_block`, `destroy_building` and `remove_block_raw`).
- `core/world/update.rs` (T1 touches the consumption core used by every block).
- `core/world/behavior/mod.rs` (`default_behavior` table, trait).
- `core/world/blocks/defense/turrets/*` (combat; T14 only).
- `core/game/runtime.rs` (campaign; one line syncs `BuildRules.env`).
- `parity/golden_content.json` + `parity/golden_manifest.json` (content).
