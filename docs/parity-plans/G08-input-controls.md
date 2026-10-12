<!-- SPDX-License-Identifier: GPL-3.0-only -->
# G08 — In-game input and controls, desktop (2026-10-13)

Self-contained execution plan. Read `00-environment.md` (same directory) first
and fully; this plan uses its abbreviations (`core/`, `gdext/`, `ui/`,
`scenes/`, `J/`, `ARC/`) and its shorthands (`$G`, `$H`, `$I`, `$C`, `$MC`,
§4.1 sector entry, §4.2 seams). Work the tasks in the §3 order, one commit per
task. If what you see in the code differs from what a task says (a symbol is
missing, a line moved by more than ~50 lines and grep cannot find it, a test
value differs from the derived one), STOP that task, write down what you saw
and continue with the next independent task.

Start state: `main` contains the G00 integration merge
(`git log main --oneline | grep -m1 'G00: integrate'` prints a line).

Audit base: the read-only integration tree (= `main` + sessions 2-5). Port
line numbers below were read there on 2026-10-13; Java line numbers are exact.

Live input architecture (verified): Godot events -> `gdext/input/events.rs`
`translate` -> `MindInput::input` (`gdext/input/mod.rs`, gating for UI
captures/dialogs/locks) -> `DesktopBridge::handle` (`gdext/input/desktop.rs`)
-> key edges `key_down_event` -> `dispatch_action(name)`; mouse
`mouse_button`/`mouse_move`/`scroll`. Commands go out through
`DesktopBridge::emit_action` (`RemoteAction` -> `ActionBatcher` ->
`MindSimHost::enqueue_sim_command`, applied at the next tick) or through
direct host calls (`host.bind_mut().<fn>()`, applied immediately between
ticks). The player unit is driven by `PlayerInput` (`core/ai/types/player.rs`),
uploaded every host `process` by `MindSimHost::process` ->
`self.sim.set_player_input(axis)` (`gdext/sim_host.rs` ~line 226).
`MindInput.push_event(json)` queues a `RawEvent` (serde tag `t`):
`{"t":"KeyDown","code":"f"}`, `{"t":"KeyUp","code":"f"}`,
`{"t":"MouseMove","x":X,"y":Y}`,
`{"t":"MouseButton","button":"left","down":true,"x":X,"y":Y}`,
`{"t":"Scroll","x":0,"y":1}` — these are dispatched in `MindInput::process`
with the same bridge code as real events (key codes are the registry names:
`f`, `z`, `x`, `b`, `shiftLeft`, `controlLeft`, `num1`, `escape`, ...). Use
`push_event` for in-engine checks unless a task says `godot_input`.

---

## 1. Inventory

### 1.1 Java files -> port

| Java | Port | Status | Evidence | Task |
|---|---|---|---|---|
| `J/input/Binding.java` (88 binds) | `core/input/binding.rs` `BINDS` | ported 1:1 (names, defaults, categories, axis-ness; test `registry_matches_upstream`) | compared every entry by hand | — (§1.2 sweep for behavior) |
| `J/input/DesktopInput.java` `update` (L230-529) | `gdext/input/desktop.rs` `handle`/`dispatch_action`, `gdext/input/mod.rs` `input`/`process` | partial | §1.2 | T1,T3,T9,T10,T11,T12,T14 |
| `DesktopInput.pollInputPlayer` (L582-933) | `desktop.rs` `left_press`/`left_release`/`right_press`/`right_release`/`mouse_move`/`scroll` | partial: placing line, break rect, mine tap, config tap work; schematic select, rebuild select, shooting, item drop, splan move, plan-delete click, Shift reverse flush missing | read both | T1,T2,T4,T6,T7,T9 |
| `DesktopInput.updateMovement` (L975-1043) | `core/ai/types/player.rs` `update_player` + `sim_host.rs` `player_axis` | partial: axis movement only. No aim, no `lookAt`/facing for omni units, no `controlWeapons` (player never shoots), no `player.boosting`; payload key repeat-hold missing | `update_player` sets `Pos/Vel/UnitCore.rotation` only for non-omni; `set_mount_targets` never called for `AiKind::Player` (`core/sim/unit_runtime.rs` `unit_update_system` ~L1149) so `mount.shoot` stays false | T1 |
| `DesktopInput.tap/touchDown` (command mode) | `desktop.rs` `left_press` command branch, `right_press` -> `command_tap` | ported (PAR-0140 verified); `command_queue` (middle) never true (mouse buttons never enter the `pressed` key set) | `events.rs` emits `RawEvent::MouseButton{button:"middle"}`, never a `KeyDown "mouseMiddle"` | T11,T14 |
| `J/input/InputHandler.java` `tileTapped` (L1973) | `desktop.rs` `tile_tapped` | partial (G02/G03 own config/inventory UIs); returns nothing (Java returns `consumed`, needed for shooting) | read | T1 |
| `InputHandler.tryTapPlayer/canTapPlayer/tryDropItems/canDepositItem` (L2032-2316) | none | missing | grep `droppingItem|tap_player` -> nothing | T2b |
| `InputHandler.requestItem/transferInventory/dropItem/takeItems/transferItemTo` (L167-260, 497-535, 677) | `core/sim/world_apply.rs` `apply_inventory_entity` (live) | wrong: the player's stack is a relay-only field `WorldApplyRuntime.player_item`, not the player unit's `ItemsComp`; no range / `interactable` / `acceptStack` checks | `world_apply.rs` L44, L230-320 | T2a |
| `InputHandler.tryBeginMine/tryStopMine/canMine` (L2045-2095) | `desktop.rs` `try_mine_at`, `Sim::player_can_mine` | ported (PAR-0045 verified) — order vs `tileTapped` differs | read | T1 (order) |
| `InputHandler.tryPickupPayload/tryDropPayload` (L1039-1063) | `desktop.rs` `payload` + `core/sim/unit_runtime.rs` `apply_payload_command` | ported (commit `4b7067b`: carrier-centred search, drop at carrier). Hold-repeat (20 ms / 200 ms) missing. In-engine unverified (no payload player unit) | `git show 4b7067b` | T12 verify |
| `InputHandler.unitControl/unitClear/buildingControlSelect` (L745-877) | `SimCommand::UnitControl/UnitClear/BuildingControlSelect` in `core/sim/mod.rs` ~L786-818 | stub: only writes `controlled_unit`/`control_building`; `player_unit` never changes. `respawn` (V) recenters the camera instead | read | T12 (input) + G01 (sim) |
| `InputHandler.rotatePlans/flipPlans` (L1558-1623) | `core/input/line.rs` `rotate_plans`/`flip_plans` | wrong math (rotates about 0,0, ignores origin, block offset, `planRotation`, `flipRotation`, point configs) | `line.rs` L206-231 | T6a |
| `InputHandler.removeSelection` (L1865-1941) | `desktop.rs` `right_release` | partial: breaks non-air tiles; does not remove queued player plans in the rect, does not remove team ghost plans, no `maxLength` 100 clamp | read | T7 |
| `InputHandler.rebuildArea` (L2318-2342), `Logic.checkOverlappingPlans` (Logic.java L53-58, 255-266) | ghosts are created (`68080c3`, `unit_runtime.rs` `resolve_deaths`), nothing consumes or clears them | missing | grep `overlapping_plans` -> nothing | T7 |
| `InputHandler.updateLine` (L1943-1966) incl. `getReplacement`/`handlePlacementLine`/`changePlacementPath` | `core/input/client_input.rs` `update_line` (`NoLineHooks`), `core/input/placement.rs` `calculate_bridges`/`calculate_nodes` (ported, unused live) | partial: no junction replacement, no auto-bridge, no node spacing | `update_line` passes `&NoLineHooks`; no replacement step | T8 |
| `InputHandler.tileX/tileY` (L2105-2120, block offset) | `desktop.rs` `cursor_tile` | wrong for even-size blocks (no `block.offset` subtraction) | `cursor_tile` = `camera.screen_to_tile(mouse)` | T4 |
| `InputHandler.flushPlans` + `BuilderComp`/`ConstructBlock.constructFinish` config (ConstructBlock.java L72-118) | `core/sim/builder.rs` `UnitBuildPlan` (no `config`), finish branch ~L707-745 | missing: plan configs are dropped | `UnitBuildPlan` fields L48-65 | T5 |
| `InputHandler.canShoot` (L2269), `UnitComp.prefRotation/lookAt` (UnitComp.java L197-206, 491-501), `WeaponsComp.aim/controlWeapons` (WeaponsComp.java L35-74) | none for the player | missing | see `updateMovement` row | T1 |
| `InputHandler.selectedUnit/selectedControlBuild` (L2148-2172) | `desktop.rs` `tap_unit_at_cursor` (command-mode only) | partial | `left_press` puts the Ctrl+click possession inside the command-mode branch | T12 |
| `J/input/Placement.java` (normalizeLine/Area, calculateNodes/Bridges, astar) | `core/input/placement.rs` | ported (pure) | tests in file | used by T7/T8 |
| `J/ui/fragments/PlacementFragment.java` `updatePick` (L132-172), `gridUpdate` (L175-273: digit combos, arrows, categories, `blockInfo`) | `desktop.rs` `pick_block`, `select_catalog_block`, `block_info_hotkey`; `ui/fragments/placement_fragment.gd` `_on_hotkey` (category prev/next only) | partial/wrong: digits select the Nth block of the visible category (Java: 1st digit = category, 2nd digit = block); arrows unhandled; F1 opens the config panel (Java: content info); pick ignores plans/config/unlock/category | read | T10 |
| `PlacementFragment` command-table keybinds (L655-670), `UnitStance.loadAll` (UnitStance.java L83-99) | `desktop.rs` `dispatch_action` stance/command arms | wrong ids: `unit_stance_hold_fire` sends stance 0 (= `stop`); `cancel_orders` deselects instead of sending `stop` | `content/registries/stances.rs` L136-160 order: stop, holdfire, pursuetarget, patrol, ram, boost, holdposition | T11 |
| `J/core/Control.java` L690-760 (fullscreen, pause, menu, screenshot) | `desktop.rs` `pause`/`screenshot`; `ui/ui_root.gd` `_unhandled_key_input` (Esc/M/F8 hard-coded keycodes) | partial: Esc opens the paused dialog without pausing; F11 unhandled; M and F8 toggled twice per press (UNVERIFIED, see T3) | read | T3,T14 |
| `J/ui/fragments/HudFragment.java` L454 (`toggle_menus`), `DesktopInput` L462-470 (`toggle_power_lines`) | none | missing | grep | T14 |
| `J/ui/fragments/ConsoleFragment.java` L43 (`console` needs setting `console`) | `dispatch_action "console"` + `ui_root.gd` F8 | wrong: no setting gate; fires on the main menu (PAR-0101) | read | T3 |
| `J/ui/dialogs/KeybindDialog.java` L96-145 (axis two-step capture) | `gdext/input/bindings.rs` `rebind` (axis -> `set_axis(None, Some(code))`), `ui/dialogs/keybind_dialog.gd` | wrong (PAR-0102) | read | T13 |
| `J/input/MobileInput.java` | `gdext/input/mobile.rs` | out of scope | — | — |

### 1.2 Binding sweep (Java default = port default for all 88; verified)

"Port handler" = where the binding's action happens today. "OK" = behaves
like Java in the live client (modulo the gate in T3).

| Binding | Default | Java consumer | Port handler | Status / task |
|---|---|---|---|---|
| move_x / move_y | A/D, S/W | `DesktopInput.updateMovement`, camera pan when dead/paused | `MindInput.pan_axis` -> `MindSimHost.player_axis`; camera | OK (PAR-0044) |
| mouse_move | mouseBack | updateMovement (move toward cursor) | never fires (mouse buttons are not keys) | T14 |
| pan | mouseForward | DesktopInput.update (camera pan) | `pan_pressed` always false (same reason) | T14 |
| boost | shiftLeft | `player.boosting`, camera pan speed, flushPlansReverse | camera only (`boost_pressed`) | T1 (unit), reverse flush deferred |
| respawn | V | `Call.unitClear` | `recenter_camera` (wrong) | T12 |
| control | controlLeft | Ctrl+click possess unit / control building | only inside command mode | T12 |
| select | mouseLeft | everything left-click | hard-coded "left" | OK; rebinding ignored (T14) |
| deselect | mouseRight | clear block / select plans / mineTile | hard-coded right; `deselect` key arm clears RTS selection (wrong) | T6b, T11 |
| break_block | mouseRight | break rect | hard-coded right | T7 |
| pickupCargo / dropCargo | [ / ] | tryPickupPayload/tryDropPayload (+hold repeat) | `payload()` | OK sim side; hold repeat T12 |
| clear_building | Q | `unit.clearBuilding()`, clears select plans | `dispatch_action` | OK; select-plan clear T6b |
| pause_building | E | `isBuilding` toggle | `dispatch_action` | OK |
| rotate | scroll (axis) | rotation, line rotation, rotatePlans | `scroll()` / `dispatch_action "rotate"` | partial T9 |
| rotateplaced | R | R held + scroll rotates the building under the cursor | R tap rotates (wrong) | T9 |
| diagonal_placement | controlLeft | diagonal line, zoom-while-placing | `update_place_line` | OK; zoom part T9 |
| pick | mouseMiddle | PlacementFragment.updatePick | hard-coded middle -> `pick_block` | T10 |
| ping | P | multiplayer ping | none | deferred (multiplayer) |
| rebuild_select | B | rebuildArea | none | T7 |
| schematic_select | F | schematic copy | none | T6b (PAR-0099) |
| schematic_flip_x / _y | Z / X | flipPlans | none | T6b |
| schematic_menu | T | ui.schematics.toggle (in game) | opens dialog (also on menu) | T3 |
| command_mode | shiftLeft | hold/tap command mode | `update_command_mode` | OK (canBoost guard missing, T12 note) |
| command_queue | mouseMiddle | queued order | never true | T14 |
| create_control_group | controlLeft | digit + Ctrl | `control_group` | OK |
| select_all_units / _unit_factories / _unit_transport | G / H / unset | command mode only, filters | auto-enables command mode; transport = all units; factories selects nothing | T11 |
| select_across_screen | altLeft | modifier | `select_all_units` | OK |
| cancel_orders | unset | stance `stop` | clears selection (wrong) | T11 |
| unit_stance_* (6) | unset | setUnitStance | off-by-one ids | T11 |
| unit_command_* (10) | unset | setUnitCommand | 5 of 10 mapped (move..mine); enterPayload/loadUnits/loadBlocks/unloadPayload/loopPayload unmapped | T11 |
| category_prev / _next | , / . | category cycle | hotkey -> `placement_fragment.gd` | OK (block re-select missing, T10) |
| block_select_left/right/up/down | arrows | grid navigation | none | T10 |
| block_select_01..10 | 1..0 | category/block combos; control groups in command mode | Nth block of visible category | T10 |
| zoom | scroll (axis) | renderer.scaleCamera | `scroll()` | partial T9 |
| detach_camera / teleport_cursor | unset | camera detach / editor teleport | detach toggles follow; teleport none | deferred (unset by default) |
| menu | escape | pause dialog + pause state | `ui_root.gd` KEY_ESCAPE | partial T3 |
| fullscreen | F11 | toggle fullscreen + setting | none | T14 |
| pause | space | toggle pause | `dispatch_action` | OK |
| skip_wave | unset | (no Java consumer found) | none | — |
| minimap | M | minimapfrag.toggle | both `dispatch_action` and `ui_root.gd` | T3 |
| research / planet_map | J / N | campaign-only toggle | always open | T3 |
| block_info | F1 | content info dialog | config panel or content | T10 |
| toggle_menus | C | hide HUD | none | T14 |
| screenshot | F12 | full-map screenshot | viewport capture | deferred (cosmetic) |
| toggle_power_lines | F5 | lasersopacity toggle | none | T14 |
| toggle_block_status | F6 | blockstatus | HUD fragment | OK (PAR-0097) |
| player_list / chat / chat_history_* / chat_scroll / chat_mode | Tab / Enter / Up / Down / scroll / Tab | multiplayer fragments | not in input bridge | deferred (multiplayer out of scope) |
| console | F8 | console (setting gated) | double handler | T3 |
| debug_hitboxes / performance_metrics | unset | debug toggles | none | deferred |

### 1.3 Already done / verified — do not redo
* Binding registry 1:1 with Java (`core/input/binding.rs`).
* WASD movement (PAR-0044), click mining (PAR-0045), RTS selection + move orders (PAR-0140), Q clear / E pause building + build plans through the player builder (PAR-0098 §2a-2d), F6 block status (PAR-0097), focus/lock guards (PAR-0100), payload key path + building pickup + save (PAR-0138 `4b7067b`, headless), schematic *dialog* placement through plans (`d1ef222`), dead buildings leave ghost `BlockPlan`s (`68080c3`).
* `core/input/placement.rs` `calculate_bridges`, `calculate_nodes`, `normalize_area`, `RelativeBridgePlacer`, `DirectionBridgePlacer` exist with unit tests — reuse them.

### 1.4 Deferred (with reason)
* Multiplayer binds (ping, player_list, chat*, console history): multiplayer out of scope.
* `splan` drag-move of an un-started plan, click-to-delete break plans (`deleting`), `planConfig` Ctrl-release, Shift (`boost`) reverse flush (`flushPlansReverse`): low impact; list as one ledger item "plan editing gestures".
* Cursor shapes (`cursorType`): cosmetic.
* `tryRepairDerelict` (click a derelict building to queue its repair): needs `Build.validPlace`/`Build.beginPlace` derelict-repair branches in `core/world/build.rs` (Build.java L94-100, L254-260) — G02 (cross-area §6).
* Full-map screenshot (`renderer.takeMapScreenshot`), `teleport_cursor`, `debug_hitboxes`, `performance_metrics`.
* Hint/schematic-control UI overlays from `DesktopInput.buildUI` (L84-129): G03 HUD.

---

## 2. Tasks

Tasks are written in execution order (§3 repeats it with dependencies).

### T1. The player unit aims at the cursor, faces its movement and shoots on left-click (+ boost key)

**Symptom / acceptance.** Today the player's alpha never fires: a left
press on empty ground does nothing, and the alpha keeps its spawn facing while
flying around (omni units never turn). Java: left-press on empty ground (no
block selected, not mining/tapping) sets `player.shooting`; the alpha turns to
the cursor and fires `small-basic-weapon` bolts toward it until release; while
moving without shooting it faces its velocity; Shift (`boost`) lifts a
`canBoost` mech (nova) which then cannot shoot.
Acceptance: on Ground Zero (§4.1), pressing and holding left mouse on an empty
tile 6 tiles east of the alpha makes `$H.get_group_count("bullet")` > 0 within
1 s, and the alpha's rotation (from `$H.unit_debug_json()`, player row) is
within 5° of 0 (east). Releasing stops new bullets.

**Java.**
* `J/input/DesktopInput.java:701-721` (select tap, no block):
  ```
  else if(!checkConfigTap() && selected != null && !tryRepairDerelict(selected)){
      if(!tryTapPlayer(mx,my) && !tileTapped(selected.build) && !player.unit().activelyBuilding() && !droppingItem
         && !(tryStopMine(selected) || (!settings.getBool("doubletapmine") || selected == prevSelected && Time.timeSinceMillis(selectMillis) < 500) && tryBeginMine(selected))
         && !Core.scene.hasKeyboard()){
          player.shooting = shouldShoot;
      }
  }else if(!Core.scene.hasKeyboard()){ player.shooting = shouldShoot; }   // out of bounds: shoot
  ```
  `shouldShoot = !scene.hasMouse() && !locked && !state.isEditor()` (L327).
  `doubletapmine` default false -> the `tryBeginMine` clause always runs.
  `DesktopInput.java:507-509`: `if(Core.input.keyRelease(Binding.select)) player.shooting = false;`
  `DesktopInput.java:822-824`: `if(player.shooting && !canShoot()) player.shooting = false;`
  `DesktopInput.java:465-469` (menu/dialog): `if(!keyDown(select)) player.shooting = false`.
* `InputHandler.canShoot` (`InputHandler.java:2269-2272`):
  `block == null && !onConfigurable() && !isDroppingItem() && !unit.activelyBuilding() && !(unit instanceof Mechc && unit.isFlying()) && !unit.mining() && !commandMode`.
* `DesktopInput.updateMovement` (`DesktopInput.java:975-1010`):
  ```
  boolean omni = unit.type.omniMovement;
  boolean boosted = (unit instanceof Mechc && unit.isFlying());
  float mouseAngle = Angles.mouseAngle(unit.x, unit.y);       // angle unit -> cursor (world)
  boolean aimCursor = omni && player.shooting && unit.type.hasWeapons() && unit.type.faceTarget && !boosted;
  if(aimCursor) unit.lookAt(mouseAngle); else unit.lookAt(unit.prefRotation());
  unit.movePref(movement);
  unit.aim(input.mouseWorldX(), input.mouseWorldY(), true);
  unit.controlWeapons(true, player.shooting && !boosted);
  player.boosting = Core.input.keyDown(Binding.boost);
  ```
* `UnitComp.lookAt(angle)` (UnitComp.java:491-493): `rotation = Angles.moveToward(rotation, angle, type.rotateSpeed * Time.delta * speedMultiplier())`.
* `UnitComp.prefRotation` (UnitComp.java:197-206):
  `activelyBuilding() && type.rotateToBuilding -> angleTo(buildPlan()); mineTile != null -> angleTo(mineTile); moving() && type.omniMovement -> vel().angle(); else rotation`.
  `moving() = !vel.isZero(0.01f)` = `vel.len2() >= 0.01` (VelComp.java:57; ARC `Vec2.isZero(margin)` = `len2() < margin`).
  `Vec2.angle()` = `atan2(y, x)` in degrees, `+360` when negative.
* `BuilderComp.activelyBuilding`: `isBuilding() && updateBuilding`, false when the head plan is farther than `type.buildRange` (220 = `Vars.buildingRange`; infinite when `rules.infiniteResources`). `mining() = mineTile != null && !activelyBuilding()`.
* `WeaponsComp.aim(x, y, clearTarget=true)` (WeaponsComp.java:60-74): `v = (x,y) - unit; if(v.len() < type.aimDst) v.setLength(aimDst)` (no-op for a zero vector); for each mount with `weapon.controllable`: `target = null; aimX/aimY = unit + v`.
  `controlWeapons(rotate=true, shoot)` (L39-48): for each controllable mount `mount.rotate = rotate; mount.shoot = shoot`.
* `PlayerComp.update` (PlayerComp.java:229-232): `if(unit.type.canBoost){ shouldBoost = unit.onSolid() || boosting || (unit.isFlying() && !unit.canLand()); elevation approach ... }` — the port's `update_kinematics` already applies this from `UnitCore.boosting` (PAR-0137, `0e984ce`), so the input only has to set `UnitCore.boosting`.
* Constants: alpha `rotateSpeed 15`, `speed 3`, `accel 0.1`, `hitSize 8`, weapon `reload 17`, `x 2.75`, mirrored (2 mounts, alternate); `aimDst = hitSize*2 = 16` (non-rotating weapon; `UnitType.java:1007-1009`, port `content/registries/units/mod.rs:2071`).

**Port today.**
* `core/ai/types/player.rs` `PlayerInput` (L36-49): `axis_x, axis_y, moved, mine_tile, mine_timer, mined`. `update_player` (L53-140) rotates only when `!omni && moving` (`core.rotation = move_toward(...)`), never touches `UnitWeapons`, never sets `UnitCore.boosting`.
* `core/sim/unit_runtime.rs` `unit_update_system` (~L1149): the `AiKind::Player` branch calls only `update_player`; `set_mount_targets` is not called, so `mount.shoot` stays `false` and the weapon pass (`weapons::update_weapons`, run after the unit loop, ~L1472) never fires for the player.
* `Sim::set_player_input(axis_x, axis_y)` (`unit_runtime.rs` ~L809).
* `gdext/sim_host.rs` `process` (~L226) uploads `player_axis()` only; `player_axis` reads `MindInput.pan_axis()`.
* `gdext/input/desktop.rs` `left_press` (~L1026): non-block branch = `try_mine_at` then `tile_tapped` (returns `()`); no shooting state.
* Helpers that exist: `crate::weapons::angles_move_toward(angle, to, speed)` (`weapons/mod.rs:245`, exact Arc `Angles.moveToward`), `UnitWeapons { weapons: Vec<WeaponDef>, mounts: Vec<WeaponMount>, pending }` (`weapons/mod.rs:68`), `WeaponDef.controllable`, `WeaponMount { aim_x, aim_y, shoot, rotate, target, .. }`, `UnitTypeDef { omni_movement, face_target, aim_dst, rotate_speed, build_range, rotate_to_building, has_weapons() }`, `StatusComp.speed_multiplier`, `MechComp` (component marks a mech), `crate::combat::bullet::unit_is_flying(world, e)`, `crate::sim::builder::BuilderPlans { plans, update_building, .. }`, `MinerComp { mine_tile, mining }`, `UnitCore { rotation, boosting, .. }`.

**Changes.**
1. `core/ai/types/player.rs` `PlayerInput`: add
   ```rust
   /// `player.shooting` (left button held on a free tile; DesktopInput.java:701-721).
   pub shooting: bool,
   /// Cursor world position (`input.mouseWorldX/Y`), the aim point.
   pub aim_x: f32,
   pub aim_y: f32,
   /// `player.boosting = keyDown(Binding.boost)` (DesktopInput.java:1010).
   pub boosting: bool,
   ```
   (`Default` derive gives false/0).
2. `core/sim/unit_runtime.rs` next to `set_player_input`: add
   ```rust
   /// Uploads the full desktop control state (`DesktopInput.updateMovement`).
   pub fn set_player_control(&mut self, axis_x: f32, axis_y: f32, shooting: bool, aim_x: f32, aim_y: f32, boosting: bool)
   ```
   (read-modify-insert the `PlayerInput` resource exactly like `set_player_input`). Keep `set_player_input` (tests use it).
3. `update_player` — insert BEFORE the velocity update (Java calls `lookAt` before `movePref`), using the velocity read at function start:
   ```rust
   let def = content.unit(type_id);                       // already resolved for omni/rotate_speed; extend that lookup
   let boosted = world.get::<MechComp>(entity).is_some() && crate::combat::bullet::unit_is_flying(world, entity);
   let building = player_actively_building(world, entity, pos, def.build_range, infinite); // see 4
   let mining = input.mine_tile.is_some() && building.is_none();
   let shoot = input.shooting && !boosted && building.is_none() && !mining;          // canShoot sim half
   let mouse_angle = angle_deg(input.aim_x - pos.x, input.aim_y - pos.y);
   let aim_cursor = omni && shoot && def.has_weapons() && def.face_target && !boosted;
   let pref = if let (Some((bx, by)), true) = (building, def.rotate_to_building) { angle_deg(bx - pos.x, by - pos.y) }
              else if let Some(tile) = input.mine_tile { let (cx, cy) = tile_center(tile.x() as i32, tile.y() as i32); angle_deg(cx - pos.x, cy - pos.y) }
              else if omni && vel.x*vel.x + vel.y*vel.y >= 0.01 { angle_deg(vel.x, vel.y) }
              else { core.rotation };
   let target = if aim_cursor { mouse_angle } else { pref };
   let speed_mult = world.get::<StatusComp>(entity).map(|s| s.speed_multiplier).unwrap_or(1.0);
   core.rotation = crate::weapons::angles_move_toward(core.rotation, target, rotate_speed * speed_mult);
   ```
   where `angle_deg(x, y) = { let a = y.atan2(x).to_degrees(); if a < 0.0 { a + 360.0 } else { a } }` (private fn in player.rs). Keep the existing non-omni `rotateMove` code after it unchanged (Java: `movePref -> rotateMove` for non-omni). `infinite` = `CampaignRuntime.session.rules.infinite_resources` when the resource exists, else `false`.
   After the position/velocity writes, set weapons and boost:
   ```rust
   if let Some(mut weapons) = world.get_mut::<UnitWeapons>(entity) {
       let mut vx = input.aim_x - pos.x; let mut vy = input.aim_y - pos.y;   // pos = position BEFORE this tick's move (Java aims after movePref; use the updated pos)
       let len = (vx*vx + vy*vy).sqrt();
       if len > 0.0 && len < def.aim_dst { let s = def.aim_dst / len; vx *= s; vy *= s; }
       let w = &mut *weapons;
       for (weapon, mount) in w.weapons.iter().zip(w.mounts.iter_mut()) {
           if weapon.controllable { mount.target = None; mount.aim_x = pos.x + vx; mount.aim_y = pos.y + vy; mount.rotate = true; mount.shoot = shoot; }
       }
   }
   if let Some(mut stored) = world.get_mut::<UnitCore>(entity) { stored.boosting = input.boosting; }
   ```
   Use the UPDATED `pos` (after `pos.x += vel.x`) for the aim, matching Java's order (`movePref` then `aim`). The `core` write that already exists must happen before this boosting write, or include `core.boosting = input.boosting;` in the existing `core` copy before it is stored (preferred: set `core.boosting` on the local copy before the existing `*stored = core`).
4. `player_actively_building(world, entity, pos, range, infinite) -> Option<(f32, f32)>` (private, player.rs): `BuilderPlans` of the entity; `None` if missing, `!update_building` or `plans.is_empty()`; head plan tile centre `(x*8, y*8)`; `None` if `!infinite && dist > range`; else `Some(centre)`. (`BuildingComp`'s exact `activelyBuilding` also needs `state.isEditor()`, always false here.)
5. `gdext/input/desktop.rs` `DesktopBridge`: add `pub shooting: bool` (init false).
   * `tile_tapped(&mut self, x, y) -> bool`: return `true` on every branch that currently returns after acting (switch toggle, door toggle, config open, inventory open); `false` when nothing happened.
   * `left_press` non-block branch, replace the body with Java order:
     ```rust
     if self.controller.state.block.is_none() {
         // (T2b inserts tryTapPlayer here)
         if self.tile_tapped(x, y) { return; }
         if self.player_actively_building() { return; }   // host seam, 6
         if self.try_mine_at(x, y) { return; }             // tryStopMine || tryBeginMine
         self.shooting = !self.locked;                      // shouldShoot (scene.hasMouse already gated in MindInput::input)
         return;
     }
     ```
     Out-of-bounds tiles: `tile_tapped`/`try_mine_at` already return false there, so the out-of-bounds Java branch is covered.
   * `left_release`: set `self.shooting = false` first (before `self.drag.take()`).
   * `handle`: when `self.ui_dialog` becomes true on a key/button edge, set `self.shooting = false` (Java L465).
   * `pub fn shooting_allowed(&self) -> bool { self.shooting && self.controller.state.block.is_none() && !self.controller.state.command_mode }` (the input half of `canShoot`; dropping-item term added in T2b).
6. `gdext/sim_host.rs`: add `pub fn player_actively_building(&self) -> bool` (non-`#[func]`) calling a new `Sim::player_actively_building(&self) -> bool` (in `core/sim/builder.rs` `impl Sim`, same rule as change 4, using `self.player_unit` and the unit def's `build_range`). Bridge `fn player_actively_building(&self) -> bool { self.host.as_ref().is_some_and(|h| h.bind().player_actively_building()) }`.
7. `gdext/input/mod.rs` `MindInput`: add (not `#[func]`)
   ```rust
   /// `DesktopInput.updateMovement` inputs for the sim host.
   pub fn player_control(&self) -> (f32, f32, bool, f32, f32, bool)
   ```
   returning `(axis.x, axis.y, bridge.shooting_allowed(), aim.x, aim.y, bridge.key_down(&self.bindings, ids::BOOST) && gameplay_input_active())`, where `aim = camera.bind().screen_to_world(mouse.0, mouse.1)` (`self.bridge.camera`, `self.bridge.mouse`); `aim = (0,0)` without a camera.
   Also add `#[func] pub fn is_shooting(&self) -> bool { self.bridge.shooting }` (probe).
8. `gdext/sim_host.rs` `process`: replace `let (axis_x, axis_y) = self.player_axis(); self.sim.set_player_input(axis_x, axis_y);` with a call to a new private `player_control()` that returns the tuple from change 7 (zeros/false when `input_locked` or no input node) and `self.sim.set_player_control(...)`. Keep `set_input_locked` zeroing via `set_player_control(0.0, 0.0, false, 0.0, 0.0, false)`.

**Tests** (`core/ai/types/player.rs` tests module, reuse the `Sim::new(1, 64, 64, AIR, AIR)` + `install_unit_runtime()` setup of `player_axis_moves_an_omni_unit_east`):
* `shooting_player_turns_to_cursor`: `spawn_player_unit("alpha", 0, 100.0, 100.0, 90.0)`; `set_player_control(0.0, 0.0, true, 150.0, 100.0, false)`; 1 tick -> rotation `75.0` (`90 - 15`); 6 ticks total -> rotation `0.0` (±0.01). (Read `UnitCore.rotation` of `sim.player_unit()` via `sim.ecs.0.get::<UnitCore>(e)`.)
* `shooting_player_fires`: same spawn, `shooting=true`, 40 ticks -> `sim.unit_runtime().unwrap().bullets_created >= 1`; a second sim with `shooting=false` for 40 ticks -> `bullets_created == 0`.
* `omni_player_faces_velocity`: alpha at rotation 0, `set_player_control(0.0, 1.0, false, 0.0, 0.0, false)`, 30 ticks -> rotation `90.0` (±0.01). Derivation: vel reaches len2 ≥ 0.01 on tick 2 (accel 0.1 * 3 = 0.3), then 15°/tick -> 90 by tick 8.
* `boosting_mech_cannot_shoot`: `spawn_player_unit("nova", 0, 100.0, 100.0, 0.0)`, `set_player_control(0,0,true,150,100,true)`, 60 ticks -> elevation `> 0.09` and `bullets_created == 0` after the elevation passes 0.09 (assert at the end: no bullets created in ticks 30..60: record the count at tick 30 and compare).
* `building_player_does_not_shoot`: use `sim_with_player()` shape (copy it locally: block + unit runtime, core-shard at (10,10) team 1, alpha at (80,80)), queue `queue_player_place(14, 10, copper-wall, 0)` with the core empty (plan stays queued, still "actively building" since in range), `shooting=true` aim (150,80) for 30 ticks -> `bullets_created == 0`.

**Verify in-engine.** Build, `tools/session-play.sh N --import`, enter Ground Zero (§4.1, the Go-button variant), then:
```bash
$G --eval "return $H.player_unit_pos()"                      # e.g. [1032, 440]
$G --eval "$C.center_on_tile(129, 55); return true"
$G --eval "return $C.tile_to_screen(135, 55)"                # -> (X, Y)
$G --eval "return $I.push_event('{\"t\":\"MouseMove\",\"x\":X,\"y\":Y}')" \
   --eval "return $I.push_event('{\"t\":\"MouseButton\",\"button\":\"left\",\"down\":true,\"x\":X,\"y\":Y}')"
sleep 1
$G --eval "return [$I.is_shooting(), $H.get_group_count(\"bullet\")]"   # [true, >0]
$G --screenshot runs/<date>-s<N>-G08-T1/shooting.png
$G --eval "return $I.push_event('{\"t\":\"MouseButton\",\"button\":\"left\",\"down\":false,\"x\":X,\"y\":Y}')"
```
Note: `MindSimHost.process` reads the mouse from the bridge, so the push_event mouse position is what the aim uses. 0 panics in the log.

**Pitfalls.**
* `update_player` is called with `&mut World` and the runtime removed; do not call `world.remove_non_send::<UnitRuntime>()` in it.
* `UnitWeapons` is taken out of the entity during the weapon pass (`update_weapons` uses `take`); `update_player` runs before that pass, so `get_mut` works.
* The host reads `MindInput` while bound mutably itself: `player_control` must only `bind()` the camera (no host access) — no re-entrancy.
* Determinism: `shooting/aim` enter the sim only through `PlayerInput` (a resource written between ticks), like the axis.
* Existing tests `player_axis_*`/`no_input_keeps_the_player_still` use `set_player_input` and must still pass (shooting defaults false).
* Mining (`try_mine_at`) previously ran before `tile_tapped`; the reorder can change which action a tap on an ore tile under nothing does: none (ore tiles have no building). PAR-0045 behaviour stays.

**Commit.** `Gameplay G08 T1: player unit aims, faces and shoots from desktop input`

---

### T2a. Item transfer uses the player unit's real stack (withdraw / deposit / drop) with Java's checks

**Symptom / acceptance.** Today `MindInput.transfer_item` (block inventory
fragment) removes items from a building into a hidden relay counter
(`WorldApplyRuntime.player_item`), never into the alpha; nothing can be
deposited from the alpha; there is no range check. Java: withdrawing from the
core puts up to `itemCapacity` (alpha 30) items into the unit stack (drawn on
the unit), only within 220 world units; depositing moves the stack into a
building that accepts it (a duo takes copper as ammo).
Acceptance: the tests below; in-engine, `transfer_item` from the core changes
the core count and `$H.unit_debug_json()` shows the player's stack.

**Java.**
* `InputHandler.requestItem` (L497-511): return unless `build.interactable(player.team())` (`state.teams.canInteract(team, build.team)` = same team or derelict) `&& player.within(build, itemTransferRange=220) && !player.dead() && amount > 0`; then `takeItems(build, item, min(unit.maxAccepted(item), amount), unit)`.
* `InputHandler.takeItems` (L167-178): `removed = build.removeStack(item, min(to.maxAccepted(item), amount)); if(removed == 0) return; to.addItem(item, removed);` (+ transfer FX, not simulated).
* `UnitComp/ItemsComp.maxAccepted(item)`: `stack.item != item && stack.amount > 0 ? 0 : type.itemCapacity - stack.amount`.
* `InputHandler.transferInventory` (L513-535): return unless `player.within(build, 220) && build.items != null && !dead && build.allowDeposit()` (`allowDeposit = block.alwaysAllowDeposit || !rules.onlyDepositCore`; `onlyDepositCore` default false); `accepted = build.acceptStack(unit.item(), unit.stack.amount, unit)`; `transferItemTo(unit, item, accepted, ..., build)` = `unit.stack.amount = max(amount - accepted, 0)`; `if(accepted > 0) build.handleStack(item, accepted, unit)` (L248-260).
* `InputHandler.dropItem` (L677-688): `unit.clearItem()` (+ FX).
* `BuildingComp.acceptStack` default: `acceptItem(self, item) && block.hasItems && (source == null || source.team == team) ? min(getMaximumAccepted(item) - items.get(item), amount) : 0`.
* `CoreBuild` (`J/world/blocks/storage/CoreBlock.java:744-812`): `acceptItem = items.get(item) < getMaximumAccepted(item)` (`coreIncinerates` ignored), `getMaximumAccepted = storageCapacity`; `handleStack: realAmount = min(amount, storageCapacity - items.get(item))` then base add; `removeStack` = base. Base `acceptStack` uses these overrides.
* `ItemTurretBuild` (`J/world/blocks/defense/turrets/ItemTurret.java:133-152`): `acceptStack = type == null ? 0 : min((int)((maxAmmo - totalAmmo) / type.ammoMultiplier), amount)`; `handleStack = repeat amount: handleItem(null, item)`; `removeStack = 0`.
* Constants: `Vars.itemTransferRange = 220`, alpha `itemCapacity 30`, core-shard `itemCapacity 4000`, duo `maxAmmo 30`, copper ammo `ammoMultiplier 2` (UNVERIFIED in the port: executor confirms with `grep -n "ammo_multiplier" core/world/blocks/defense/turrets/mod.rs | head -3` — expect the copper entry `2`).

**Port today.**
* `core/sim/mod.rs` `SimCommand::Inventory` arm (~L882-915): live branch calls `world_apply.apply_inventory_entity(&mut self.ecs.0, entity, kind, item, amount)` (`core/sim/world_apply.rs` ~L230-320), which uses `self.player_item` (relay).
* `core/world/blocks/distribution/transfer.rs`: `dispatch_handle_stack` (L138), `dispatch_remove_stack` (L194), `dispatch_get_maximum_accepted` (L166), `default_accept_stack` (L54) — there is NO `dispatch_accept_stack`. Trait `BuildingBehavior::accept_stack` default returns `0` (`core/world/behavior/mod.rs:284`).
* `CoreBehavior` (`core/world/blocks/storage/core_block.rs:139+`) has `accept_item`, `get_maximum_accepted`, `handle_item` but no `accept_stack`/`handle_stack`/`remove_stack` (so core deposits accept 0).
* `TurretBehavior` (`core/world/blocks/defense/turrets/behavior.rs:45`) has `accept_item`/`handle_item` (-> `turrets::accept_item`/`handle_item`, `mod.rs:1042-1080`) but no stack hooks.
* Player stack: `ItemsComp { item: Option<(ItemId, i32)> }` on the unit (`core/entities/comp/unit/comp.rs:392`, `add_item`, `clear_item`).
* Existing test `block_runtime_inventory_targets_live_item_module` (`core/sim/tests.rs` ~L925-1000) asserts the relay semantics; it must be rewritten.

**Changes.**
1. `transfer.rs`: add `pub fn dispatch_accept_stack(world: &World, target: Entity, item: ItemId, amount: i32, source: Option<Entity>) -> i32` (same lookup shape as `dispatch_get_maximum_accepted`, calls `inst.behavior.accept_stack(...)`, `0` when no instance).
2. `CoreBehavior`: add `accept_stack` = `if self.accept_item(world, e, e, item) && source team check (as default_accept_stack) { (self.get_maximum_accepted(world, e, item) - transfer::item_count(world, e, item)).min(amount).max(0) } else { 0 }`; `handle_stack` = `let real = amount.min(self.get_maximum_accepted(world, e, item) - item_count(world, e, item)).max(0); transfer::default_handle_stack(world, e, item, real)`; `remove_stack` = `transfer::default_remove_stack`. Cite CoreBlock.java lines. Order: G02 T1 (trait-default `accept_stack`) lands before this task; T2a owns the Core/Turret overrides (G02 does not add them).
3. `TurretBehavior`: add `accept_stack` = for an item-ammo turret (`TurretState.config.ammo == TurretAmmo::Item(list)` with an entry for `item`): `(((max_ammo - total_ammo) as f32 / entry.ammo_multiplier as f32) as i32).min(amount).max(0)`, else 0; `handle_stack` = `for _ in 0..amount { turrets::handle_item(world, e, item) }`; `remove_stack` = 0.
4. New file `core/sim/player_items.rs` (header per env §3; `mod player_items;` in `core/sim/mod.rs`) with `impl Sim`:
   ```rust
   pub const ITEM_TRANSFER_RANGE: f32 = 220.0;   // Vars.itemTransferRange
   /// requestItem/transferInventory/dropItem against the live player unit. Returns true when items moved.
   pub fn apply_player_inventory(&mut self, kind: u8, pos: TilePos, item: Option<u16>, amount: i32) -> bool
   /// The player unit's carried stack (`unit.stack`).
   pub fn player_unit_stack(&self) -> Option<(ItemId, i32)>
   ```
   * Resolve `unit = self.player_unit()?` (the `UnitRuntime` player entity), `def` (itemCapacity), unit `Pos`, unit team (`TeamComp`).
   * `kind 2` (drop): `ItemsComp.clear_item()`; return true if it held something. No building needed.
   * Else `entity = self.grid.entity_at(pos)?`; building centre = `Building.tile` world `(x*8 + offset, y*8 + offset)` with `offset` from the block def; `within` = squared distance ≤ 220².
   * `interactable`: building `TeamComp.team == unit team || == 0` (derelict).
   * `kind 0` (withdraw): needs `item`, interactable, within, `amount > 0`; `max_accepted` per Java; `removed = dispatch_remove_stack(world, entity, item, amount.min(max_accepted))`; `if removed > 0 { ItemsComp.add_item(item, removed) }`.
   * `kind 1` (deposit): within; building has `ItemModule`; `allowDeposit` = true (`rules.onlyDepositCore` is not ported — `grep only_deposit_core core/game/rules.rs` is empty — and defaults false in Java; note this in the commit); the unit must hold `item` (ignore the passed `item` if it differs from the held one: Java uses `unit.item()`); `accepted = dispatch_accept_stack(world, entity, held, held_amount, Some(unit))` (team check: source team == building team); `ItemsComp.item = if held_amount - accepted > 0 { Some((held, held_amount - accepted)) } else { None }`; `if accepted > 0 { dispatch_handle_stack(world, entity, held, accepted) }`. `amount` from the command is ignored for deposits (Java transfers the whole stack).
5. `core/sim/mod.rs` Inventory arm: when `self.player_unit().is_some()` call `self.apply_player_inventory(*kind, pos, *item, *amount)` and return `Ok(())` (count `commands_applied`); otherwise keep the existing relay path (headless relay tests).
6. Rewrite `block_runtime_inventory_targets_live_item_module` to the new semantics (it has no player unit -> relay path unchanged; keep it as is if it still passes) and add the new tests below.
7. `gdext/sim_host.rs`: `pub fn player_unit_stack(&self) -> Option<(String, i32)>` (item name via content) and `#[func] pub fn player_stack_json(&self) -> GString` (`{"item":"copper","amount":30}` or `{}`) — playtest seam.

**Tests** (`core/sim/player_items.rs` tests; setup = copy of `sim_with_player()` from `core/sim/builder.rs` tests: 64x64, both runtimes, core-shard at (10,10) team 1, alpha team 1 at world (80,80)):
* `withdraw_from_core_caps_at_item_capacity`: core copper 100 (use the `set_core_copper` pattern); `apply_player_inventory(0, (10,10), copper, 50)` -> true; core copper `70`; `player_unit_stack() == Some((copper, 30))`.
* `withdraw_rejects_other_item_when_holding`: after the above, withdraw lead -> false, stack unchanged.
* `deposit_into_duo_takes_ammo_amount`: place `duo` at (13,10) team 1 (`SimCommand::Place{player: None}`), stack copper 30; `apply_player_inventory(1, (13,10), copper, 30)` -> true; stack `Some((copper, 15))`; duo `TurretState.total_ammo == 30` (15 copper * multiplier 2).
* `deposit_into_core_takes_all`: stack copper 30, core copper 70 -> deposit at (10,10) -> stack `None`, core `100`.
* `transfer_out_of_range_is_ignored`: place `container` at (40,40) team 1 with 10 copper (`debug` via ItemModule add), alpha at (80,80): distance ≈ 330 > 220 -> withdraw returns false.
* `drop_clears_stack`: kind 2 -> `None`.

**Verify in-engine.** Ground Zero; `$H.debug_add_core_items("copper", 100)`;
`$G --eval "$I.transfer_item(129, 55, \"copper\", 50, false); return true"`;
after 1 s: `$G --eval "return [$H.core_items_json(), $H.player_stack_json()]"` -> core copper reduced by 30, stack `{"item":"copper","amount":30}`. Screenshot (the stack sprite drawing is G03's; note whether it shows).

**Pitfalls.** Payload/configure sounds are G07's (sim-side); add none here. `CoreBehavior` items may be shared across cores (`onProximityUpdate`); use the module of the tapped core only. `dispatch_*` need the `BlockTable` resource (present with the block runtime). The relay `player_item` stays for headless relay tests — do not delete it. `ItemsComp` may be absent on a unit: insert `ItemsComp::default()` if missing before `add_item`.

**Commit.** `Gameplay G08 T2a: item withdraw/deposit/drop move the player unit's stack`

---

### T2b. Tap your own unit, release over a building to deposit (tryTapPlayer / tryDropItems)

**Symptom / acceptance.** Java: with no block selected, pressing left within
11 world units of your unit while it carries items starts "dropping"; releasing
over a building that accepts the stack deposits it (`transferInventory`),
releasing elsewhere drops it (`dropItem`, stack lost). Port: nothing.
Acceptance: alpha holding 30 copper next to a duo; press on the alpha, release
on the duo -> duo `total_ammo` 30 and stack 15.

**Java.** `InputHandler.tryTapPlayer/canTapPlayer` (L2032-2043):
`canTapPlayer(x,y) = player.within(x, y, playerSelectRange=11) && !dead && unit.stack.amount > 0 && block == null`; `tryTapPlayer` sets `droppingItem = true`.
`tryDropItems(build, x, y)` (L2286-2308), called on select/break release (`DesktopInput.java:799`) with the build under the cursor:
```
if(player.dead()) return;
if(!droppingItem || stack.amount <= 0 || canTapPlayer(x, y) || state.isPaused()){ droppingItem = false; return; }
droppingItem = false;
if(build != null && build.acceptStack(stack.item, stack.amount, unit) > 0 && build.interactable(team) && build.block.hasItems && stack.amount > 0){
    if(build.allowDeposit() && canDepositItem(build)){ Call.transferInventory(player, build); itemDepositCooldown = rules.itemDepositCooldown; }
}else{ Call.dropItem(player.angleTo(x, y)); }
```
`canDepositItem`: `block.depositCooldown < 0` (default -1) -> `itemDepositCooldown <= 0`; `itemDepositCooldown -= Time.delta/60` every frame (`InputHandler.update` L951); `rules.itemDepositCooldown = 0.5` s. `player.dead() -> droppingItem = false` (L986). `canShoot` is false while dropping.

**Port today.** No `dropping_item` state; `transfer_item(x, y, item, amount, deposit)` emits `RemoteAction::Inventory` (desktop.rs ~L823); after T2a the sim side is correct. `MindSimHost::player_unit_position()` exists (pub, not func); T2a adds `player_unit_stack()`.

**Changes** (`gdext/input/desktop.rs`):
1. Fields: `dropping_item: bool`, `last_deposit_ms: u64` (0).
2. `fn cursor_world_raw(&self) -> Option<(f32, f32)>` = `camera.bind().screen_to_world(mouse)` (exact cursor, not tile-snapped).
3. `fn can_tap_player(&self, wx, wy) -> bool`: block is none, host `player_unit_position()` within 11.0 (squared ≤ 121), host `player_unit_stack()` amount > 0.
4. `left_press` non-block branch (T1 layout): first line `if let Some((wx, wy)) = self.cursor_world_raw() && self.can_tap_player(wx, wy) { self.dropping_item = true; return; }`.
5. `left_release` and `right_release`: before their existing logic call `self.try_drop_items()`:
   ```rust
   fn try_drop_items(&mut self) {
       if !self.dropping_item { return; }
       self.dropping_item = false;
       let Some((wx, wy)) = self.cursor_world_raw() else { return };
       if self.can_tap_player(wx, wy) || paused { return; }
       let (tx, ty) = tile under cursor (WorldGrid::to_tile(wx), to_tile(wy));
       let now = Time::singleton().get_ticks_msec();
       let accepts = host.bind().player_can_deposit_at(tx, ty);   // new seam below
       match accepts {
           Some(true) if now.saturating_sub(self.last_deposit_ms) >= 500 => { emit Inventory{kind: Deposit, x: tx, y: ty, item: held id, amount: held amount}; self.last_deposit_ms = now; }
           Some(_) => {}                       // accepting building but on cooldown: nothing (Java: no drop either)
           None => emit Inventory{kind: Drop(=2) ...}
       }
   }
   ```
   `MindSimHost::player_can_deposit_at(x, y) -> Option<bool>`: `None` when no building at the tile or `dispatch_accept_stack(...) == 0` or not interactable or no `has_items` (=> Java else-branch: drop); `Some(true)` otherwise (allowDeposit true by default). Implement via a new `Sim::player_deposit_accepted(pos) -> i32` in `player_items.rs` (reuse T2a logic without mutation).
   The drop action is `InventoryKind::DropItem` (exists, `core/input/action.rs:22`, code 2); `transfer_item` only builds Withdraw/Deposit, so emit `RemoteAction::Inventory { kind: InventoryKind::DropItem, x, y, item: None, amount: 0, angle: 0.0 }` directly.
6. `shooting_allowed()` (T1) gains `&& !self.dropping_item`.

**Tests.** Headless: `core/sim/player_items.rs` `deposit_accepted_matches_apply` (`player_deposit_accepted((13,10))` == 15 for the duo case, 0 for empty ground). In-engine is the acceptance.

**Verify in-engine.** Ground Zero: place a duo next to the core
(`$H.place_block(132, 55, "duo")`), seed the stack via
`$I.transfer_item(129,55,"copper",30,false)`, wait 1 s; `p = $C.tile_to_screen` of the
player tile (`player_unit_pos()/8`), `d = $C.tile_to_screen(132,55)`; push
`MouseMove p`, `MouseButton left down p`, `MouseMove d`, `MouseButton left up d`;
then `$H.player_stack_json()` -> amount 15 and `$H.building_debug(132,55)` shows ammo.

**Pitfalls.** The press on the unit must not also start shooting (return early). `ui_captures_at` gating is in `MindInput::input`; push_event bypasses it (fine).

**Commit.** `Gameplay G08 T2b: tap the player unit to deposit or drop its stack`

---

### T3. PAR-0101: gameplay bindings only in game; M/F8 single toggle; console setting gate; Esc pauses

**Symptom / acceptance.** On the main menu T opens Schematics and F8 opens
the console (PAR-0101); J/N open research/planet even outside a campaign;
Esc in game opens the pause dialog but the sim keeps running. Suspected:
M and F8 in game are toggled twice per press (once by `MindInput` effects,
once by `ui_root.gd`) so nothing visible happens.
Acceptance: (a) main menu: `push_event KeyDown t` and `f8` -> no dialog/console
(`get_node("/root/MindUi").dialog_stack()` stays empty, console invisible);
(b) in game: one M press toggles the minimap exactly once; (c) F8 does nothing
unless setting `console` is true; (d) Esc in game opens `paused` and
`$H.is_paused()` becomes true.

**Java.**
* `Control.update` (Control.java:690-760): `input.update()` (the whole
  `DesktopInput.update`) runs only `if(state.isGame())`; `fullscreen` (F11) is
  handled before that gate (works on the menu). Esc: `if(keyTap(menu) && !ui.restart.isShown() && !ui.minimapfrag.shown()){ if(chatfrag.shown()) chatfrag.hide(); else if(!ui.paused.isShown() && !scene.hasDialog()){ ui.paused.show(); if(!net.active() && !state.rules.pauseDisabled) state.set(State.paused); } }`.
* `DesktopInput.update` L446-461: `if(state.isGame() && !scene.hasDialog() && !scene.hasField()){ minimap -> ui.minimapfrag.toggle(); planetMap && state.isCampaign() -> ui.planet.toggle(); research && state.isCampaign() -> ui.research.toggle(); schematicMenu -> ui.schematics.toggle(); ... }`.
* `ConsoleFragment` (L43): `if(input.keyTap(Binding.console) && settings.getBool("console") && ...) toggle`. Setting `console` default false (`SettingsMenuDialog.java:566`).

**Port today.**
* `gdext/input/desktop.rs` `handle` -> `key_down_event` fires for every binding whenever `!text_focus && !ui_dialog` — no game-state check.
* `dispatch_action` arms `minimap`/`console` push `Effect::ToggleFragment`, `research`/`planet_map`/`schematic_menu` push `Effect::OpenDialog`.
* `ui/ui_root.gd` `_unhandled_key_input` (L75-84) ALSO handles `KEY_ESCAPE`, `KEY_M` (`_on_minimap_key`, gated off on the menu) and `KEY_F8` (`_on_console_key`, gated off on the menu). In game both handlers run for M and F8.
* `MindInput::input` never marks key events handled, so `ui_root`'s `_unhandled_key_input` always also receives them.
* In-game detection available: `UiRoot.menu_group.visible` (`ui/ui_root.gd:16`, `set_menu_visible` L206); node path from `MindInput`: `../Ui/UiRoot/MenuGroup`.
* Campaign detection: `MindCampaign.get_hud_state()` has `"hasCore"`; for "is campaign" use `$MC` — UNVERIFIED which field; executor checks `grep -n "fn is_campaign\|\"campaign\"" gdext/campaign.rs` and uses an existing `#[func]`/pub fn returning whether a campaign sector is active (if none exists, add `pub fn campaign_active(&self) -> bool` on `MindCampaign` returning whether a live sector is loaded — the same condition `get_hud_state` uses for `hasCore`/`sectorName`).

**Changes.**
1. `DesktopBridge`: field `pub in_game: bool` (false). `MindInput::input` and `process` set it each call: `in_game = !MenuGroup.is_visible_in_tree()` (`try_get_node_as::<Control>("../Ui/UiRoot/MenuGroup")`; missing node -> `true` so tests without UI keep working).
2. `handle` KeyDown: `if fresh && !self.text_focus && !self.ui_dialog { if self.in_game { self.key_down_event(..) } else if bindings matches FULLSCREEN { dispatch fullscreen (T14) } }`. Mouse world presses: in `MindInput::input`, skip `bridge.handle` for left/right/middle presses when `!in_game` (the menu has no world).
3. Remove the M and F8 branches from `ui_root.gd` `_unhandled_key_input` (keep Esc there; the binding path owns M/F8 so rebinding works). Keep `_on_minimap_key`/`_on_console_key` functions unused-free: delete them.
4. `dispatch_action "console"`: only push the toggle when the setting `console` is true (read via `SettingsStore::load(&NativeFs, &Paths::resolve(None)).get_bool("console", false)` like `read_command_mode_hold` in mod.rs, or cache it and refresh on F8 press).
5. `research` / `planet_map`: only when the campaign is active (seam from "Port today").
6. Esc (`ui_root.gd` `_on_escape`, the last branch that opens `paused`): after a successful `open_dialog("paused")`, when not in a menu, call `get_node("/root/Spine/SimHost").set_paused(true)` (Java `state.set(paused)`; `pauseDisabled`/net ignored — no multiplayer). Do not unpause on close (Java does not).

7. Close-by-key (ResearchDialog.java:145-149 `keyDown(key == Binding.research.value.key) -> hide()`; PlanetDialog.java:92 `key == escape || back || Binding.planetMap.value.key -> hide`): in `DesktopBridge::handle` KeyDown, when `self.ui_dialog` is true and the code matches the `research` binding while the top of `MindUi.dialog_stack()` is `research`, or matches `planet_map` while the top is `planet`, push a new `Effect::CloseTopDialog` that `drain_effects` resolves with `/root/MindUi` `close_top_dialog()` (`gdext/ui/ui_host.rs:268`). Pass the top dialog name into the bridge as `pub top_dialog: String` refreshed with `ui_dialog` (last element of `dialog_stack()`). So J and N toggle their dialogs as in Java (open via change 5 when no dialog is open; close via this change).

**Tests.** GDScript parse gate (env §2). Rust: `desktop.rs` unit test `bindings_do_not_fire_outside_game` (construct `DesktopBridge::new()`, `in_game = false`, `handle(KeyDown "t")` -> `take_effects()` empty; `in_game = true` -> contains `OpenDialog{name:"schematics"}`).

**Verify in-engine.** Fresh `--play` (menu): push `KeyDown t`, `KeyUp t`, `KeyDown f8`, `KeyUp f8` (push_event) -> `get_node("/root/MindUi").dialog_stack()` empty and `get_node("/root/Spine/Ui/UiRoot/HudGroup/console").visible == false`. Then Ground Zero: `godot_input` key `M` press/release (real event path, both handlers) -> `.../HudGroup/minimap.visible` flips once (check before/after). Esc via `godot_input` -> `dialog_stack()` contains `paused` and `$H.is_paused()` true. In the campaign: push `KeyDown j` -> `research` open; `KeyDown j` again -> closed; same with `n` / `planet`. First, BEFORE changing code, record the current M behaviour with the same `godot_input` sequence (expected UNVERIFIED: visibility unchanged = double toggle) and put it in the commit body.

**Pitfalls.** `push_event` events are dispatched in `process` with `in_game` refreshed there too. `ui_root.gd` changes need only `--play`. The keybind dialog captures keys in its own `_unhandled_input`; it is a dialog so `ui_dialog` gates the bridge.

**Commit.** `Fixes PAR-0101: gameplay hotkeys only in game; single M/F8 toggle; Esc pauses`

---

### T4. Even-size blocks are placed centred on the cursor (`tileX/tileY` block offset)

**Symptom / acceptance.** A 2x2 block (e.g. `graphite-press`, `mechanical-drill`) is placed one tile up/right of where Java places it for the same cursor position; the ghost is not centred under the cursor. Acceptance: cursor at world (100, 100) with `mechanical-drill` selected -> plan anchor tile (12, 12) (Java), not (13, 13).

**Java.** `InputHandler.tileX/tileY` (L2105-2120):
```
Vec2 vec = Core.input.mouseWorld(cursorX, 0);
if(selectedBlock()){ vec.sub(block.offset, block.offset); }   // selectedBlock = isPlacing() && mode != breaking
return World.toTile(vec.x);                                     // Math.round(v / 8)
```
`Block.offset = ((size + 1) % 2) * tilesize / 2f` -> 4 for even sizes, 0 for odd. Raw (`rawTileX`) is used for schematics/break selection (`schemX`, `rawCursorX`); `tileX` (offset) for `selectX`, `cursorX` and the line.
Derivation: 2x2 at world 100: `round((100-4)/8) = round(12.0) = 12`; 1x1: `round(12.5) = 13`.

**Port today.** `desktop.rs` `cursor_tile()` = `camera.screen_to_tile(mouse)` = `floor((world + 4) / 8)` with no offset; used by `left_press` (start of the Place drag), `mouse_move` (drag end), `commit_place` (fallback single plan). `BlockDef.offset` exists (`content/registries/blocks/mod.rs` ~L2116).

**Changes.**
1. `core/input/cursor.rs`: `pub fn placement_tile(world_x: f32, world_y: f32, block_offset: f32) -> (i32, i32)` = `(((world_x - block_offset) / 8.0 + 0.5).floor() as i32, same for y)` (Java `Math.round` = floor(x + 0.5)). Doc-cite InputHandler.java:2105-2120.
2. `desktop.rs`: `fn placement_cursor_tile(&self) -> Option<(i32, i32)>`: if `controller.state.block` is `Some(id)` and `place_mode != Breaking`, `offset` = host content `block(id).offset`, world = `camera.screen_to_world(mouse)` -> `placement_tile`; else `cursor_tile()`. Use it in `left_press` (block branch only, both `start` and the first `update_place_line`), in `mouse_move` when `drag.kind == Place`, and in `commit_place`'s single-plan fallback (`drag.last` already holds it).

**Tests.** `cursor.rs`: `placement_tile_offsets_even_blocks`: `(100,100,4.0) -> (12,12)`, `(100,100,0.0) -> (13,13)`, `(99,99,0.0) -> (12,12)`, `(99,99,4.0) -> (12,12)`, `(108,92,4.0) -> (13,11)`.

**Verify in-engine.** Ground Zero, `$I.select_block_by_name("mechanical-drill")` (2x2, offset 4). Target the world point (1124, 484) (4 px right/up of tile (140,60)'s centre): `$G --eval "return $C.get_canvas_transform() * Vector2(1124, 484)"` -> viewport (X, Y). Push `MouseMove`, `MouseButton left down`, `left up` at (X, Y); `$H.player_build_plans_json()` -> mechanical-drill plan anchored at (140, 60) (Java: `round((1124-4)/8) = 140`, `round((484-4)/8) = 60`). Before the fix the same sequence gives (141, 61) (`floor((1124+4)/8)`); record both in the commit body. Clear plans with `$H.clear_player_build_plans()` between runs.

**Pitfalls.** `screen_to_world` is a `#[func]` on the camera — call through `camera.bind()`. Do not change `screen_to_tile` (other callers rely on raw tiles).

**Commit.** `PAR-0098: placement cursor honours the block offset for even-size blocks`

---

### T5. Build plans carry their config into the finished block

**Symptom / acceptance.** Configs on queued plans (schematic copies, rebuild ghosts, auto-linked bridges, `nextConfig` of sorters) are dropped: a pasted sorter comes out unconfigured. Acceptance: test `player_plan_config_applies_on_finish`.

**Java.** `BuilderComp` (BuilderComp.java:173, 217): `Call.beginPlace(..., current.block.instantBuild ? current.config : null)`; `entity.construct(self(), core, bs, current.config)`; `ConstructBuild.construct` stores `lastConfig = config` (ConstructBlock.java:278-288) and on completion `constructed(tile, current, builder, rotation, team, config)` -> `constructFinish` (L72-118): after `tile.setBlock(...)`, `if(config != null) tile.build.configured(builder, config)`.
`Block.nextConfig()` (Block.java:827-832): `saveConfig && lastConfig != null ? lastConfig : null`; `BuildingComp.configure(value)` sets `block.lastConfig = value` (L388-391); `updateLine` creates each line plan with `block.nextConfig()` (InputHandler.java:1946).

**Port today.** `core/sim/builder.rs` `UnitBuildPlan` (L48-65, `#[derive(Debug, Clone, PartialEq)]`) has no config. `queue_player_place(x, y, block, rotation)` (L777). Finish branch (~L720-745) calls `runtime.set_block_raw` then `placed_hooks` and `chain_bridge`. `ConstructState.last_config` exists (`core/world/construct.rs:33`) but is never written. Configure API: `crate::world::config::configure(world: &mut World, entity, player: Option<Entity>, value: crate::world::config::ConfigValue) -> bool` (`core/world/config.rs:131`).

**Changes.**
1. `UnitBuildPlan`: `pub config: crate::world::config::ConfigValue` (`ConfigValue::None` in `place`/`break_plan`).
2. `Sim::queue_player_place_config(&mut self, x, y, block, rotation, config: ConfigValue) -> bool` = body of `queue_player_place` with `plan.config = config`; make `queue_player_place` call it with `ConfigValue::None`.
3. Construct path: when the builder begins/advances a construct for the head plan, copy `current.config` into `ConstructState.last_config` (where `world.entity_mut(entity).insert(state.clone())` happens, set `state.last_config = current.config.clone()` before inserting, only for non-breaking plans).
4. Finish branch, after `runtime.placed_hooks(...)` and before `chain_bridge`: `if !matches!(current.config, ConfigValue::None) { let _ = crate::world::config::configure(world, placed, Some(unit), current.config.clone()); }`.
5. `gdext/sim_host.rs`: `pub fn queue_player_place_config(&mut self, x: i16, y: i16, block: BlockId, rot: u8, config: ConfigValue) -> bool` forwarding (used by T6b/T7).
6. `nextConfig` for line placement: `DesktopBridge` field `last_config: HashMap<u16 /*block raw*/, ConfigValue>`; `MindInput.configure_building` records `last_config[block_at(x,y)] = value` (the world `ConfigValue` it builds) when the block def has `save_config`; `commit_place` uses `queue_player_place_config` (direct host call) instead of `SimCommand::Place` when `last_config` has an entry for the block and `def.save_config`, else keeps `SimCommand::Place`.

**Tests** (`core/sim/builder.rs` tests, `sim_with_player()`): `player_plan_config_applies_on_finish`: core copper/lead 500 each (extend `set_core_copper` pattern for lead), `queue_player_place_config(14, 10, sorter, 0, ConfigValue::Item(copper))`, tick 120 -> sorter exists and its config reads copper (`crate::world::config::read_config(&sim.ecs.0, entity)`, `core/world/config.rs:154`, returns `ConfigValue::Item(copper)`). `plan_without_config_keeps_default`: copper-wall plan, no config -> finishes (existing test still passes).

**Verify in-engine.** Headless only for the core (no config-bearing plan entry point until T6b); T6b's in-engine check covers it.

**Pitfalls.** `UnitBuildPlan` is cloned into `current` each tick; `ConfigValue` clone is cheap (SmallVec). `configure` may reject values for blocks without handlers — ignore the result. Do not apply config on deconstruction.

**Commit.** `PAR-0098: build plans carry their config to the finished block`

---

### T6a. Core: schematic copy plans, faithful rotate/flip of select plans

**Java.**
* `Schematics.create(x, y, x2, y2)` (Schematics.java:389-450) — ported as `Schematics::create(&self, world: &dyn SchematicWorld, registry, x, y, x2, y2, max_size)` (`core/game/schematics.rs:733`; uses `normalize_area` with `maxSchematicSize = 64`).
* `Schematics.toPlans(schem, x, y, checkHidden)` (L299-302):
  `tiles.map(t -> new BuildPlan(t.x + x - width/2, t.y + y - height/2, t.rotation, t.block, t.config)).removeAll(checkHidden && !visible && !core || !unlockedNow).sort(comparingInt(-schematicPriority))` (stable sort).
* `schematicPriority` values: default 0; `PowerNode` class -10 (`PowerNode.java:50`, incl. `LongPowerNode`), `surge-tower` -15 (Blocks.java:2503), `LogicBlock` 5, `NuclearReactor` -5, `PowerDiode` 10, `plastanium-wall` / `plastanium-wall-large` 10 (Blocks.java:1736,1745).
* `InputHandler.rotatePlans(plans, direction)` (L1558-1594), origin `ox,oy = rawTileX/Y()` (cursor tile):
  ```
  skip breaking
  off = block.size % 2 == 0 ? -0.5 : 0                // Point2 configs (relative links)
  pointConfig p: cx = p.x + off, cy = p.y + off; (cx,cy) = dir >= 0 ? (-cy, cx) : (cy, -cx); p = (floor(cx - off), floor(cy - off))
  wx = (plan.x - ox) * 8 + block.offset; wy = (plan.y - oy) * 8 + block.offset
  (wx,wy) = dir >= 0 ? (-wy, wx) : (wy, -wx)
  plan.x = World.toTile(wx - block.offset) + ox; plan.y = World.toTile(wy - block.offset) + oy     // toTile = round(v/8)
  plan.rotation = block.planRotation(mod(plan.rotation + direction, 4))      // planRotation: (!rotate && lockRotation(true)) ? 0 : rot
  ```
* `InputHandler.flipPlans(plans, x)` (L1596-1623):
  ```
  origin = (x ? ox : oy) * 8
  value = -((x ? plan.x : plan.y) * 8 - origin + block.offset) + origin
  if x: plan.x = (int)((value - block.offset) / 8) else plan.y = ...     // (int) truncates toward zero
  pointConfig p: if size % 2 == 0: p.x-- (or p.y--); p.x = -p.x (or p.y = -p.y)
  block.flipRotation(plan, x): if((x == (rotation % 2 == 0)) != invertFlip) rotation = planRotation(mod(rotation + 2, 4))
  ```
  `invertFlip` true only for `electrolyzer` (Blocks.java:1363).
* Point2 configs: `ConfigValue::Point2(x, y)` and `ConfigValue::Point2Array(packed)` (pack/unpack: `core/io/typeio/mod.rs` `pack_point2`, `unpack_point2_x/_y`).

**Port today.** `Schematics::to_plans(&self, index, ...)` needs the schematic in `self.all` and sorts by `block.raw()` (wrong order). `core/input/line.rs` `rotate_plans`/`flip_plans` (L206-231) rotate around (0,0) ignoring origin/offset/planRotation and flip with `x = -x - 1` — wrong; they are used by `gdext/input/mobile.rs` (`mobile_rotate_plans`/`mobile_flip`). `Block.rotate` is `BlockInstance.rotate` (`core/world/block.rs:83`), readable with `crate::world::blocks::payloads::payload_block::block_rotates(world, block)`.

**Changes.**
1. `core/game/schematics.rs`:
   * `pub fn schematic_priority(def: &BlockDef) -> i32` with the table above (`name == "surge-tower"` -> -15 first, plastanium walls 10, then kinds `PowerNode | LongPowerNode` -10, `LogicBlock` 5, `NuclearReactor` -5, `PowerDiode` 10, else 0).
   * `pub fn plans_for(schem: &Schematic, x: i32, y: i32, check_hidden: bool, registry: &ContentRegistry) -> Vec<BuildPlan>`: the body of `to_plans` but sorted with `plans.sort_by_key(|p| -schematic_priority(def))` (Vec::sort_by_key is stable). `to_plans` calls it. Keep the unlock filter as is (the port has none; copied blocks are placed blocks).
   * Update the existing to_plans test expectations only if they depended on id order (run the schematics tests; if one fails on order, fix the expectation and cite Schematics.java:301 in the commit).
2. `core/input/line.rs`: add
   ```rust
   #[derive(Debug, Clone, Copy)]
   pub struct PlanBlockInfo { pub size: i32, pub offset: f32, pub rotate: bool, pub invert_flip: bool }
   pub fn rotate_plans_about(plans: &mut [ClientPlan], direction: i32, ox: i32, oy: i32, info: &dyn Fn(BlockId) -> PlanBlockInfo)
   pub fn flip_plans_about(plans: &mut [ClientPlan], flip_x: bool, ox: i32, oy: i32, info: &dyn Fn(BlockId) -> PlanBlockInfo)
   ```
   implementing the Java math exactly (`World.toTile` = `(v / 8.0).round()` -> use `(v / 8.0 + 0.5).floor()` for Java `Math.round`; flip uses `as i32` truncation). Leave the old functions for mobile (mark them `/// Mobile legacy; desktop uses *_about`).

**Tests** (`line.rs`): with `info = |_| PlanBlockInfo{size:1, offset:0.0, rotate:true, invert_flip:false}` unless stated:
* `rotate_about_quarter_turn`: plan (6,5) rot 0, origin (5,5), dir 1 -> (5,6) rot 1. Derivation: wx=8, wy=0 -> (-0, 8) -> x = round(0)+5 = 5, y = round(1)+5 = 6.
* `rotate_about_four_times_identity`: (7,3) rot 2 around (5,5), 4x dir 1 -> (7,3) rot 2.
* `rotate_about_2x2`: size 2 offset 4, plan (6,5), origin (5,5), dir 1 -> (4,6). Derivation: wx=12, wy=4 -> (-4, 12) -> x = round(-8/8)+5 = 4, y = round(8/8)+5 = 6.
* `rotate_nonrotating_block_resets_rotation`: `rotate:false`, rot 3, dir 1 -> rot 0.
* `flip_x_about`: plan (6,5) rot 0, origin x 5 -> x 4, rot 2 (value = -(48-40)+40 = 32 -> 4; x-flip with rot%2==0 flips). Rot 1 stays 1.
* `flip_y_about`: plan (5,7) rot 1, origin y 5 -> y 3, rot 3.
* `flip_point_config`: bridge plan (size 1) config `Point2(3,0)`, flip_x -> `Point2(-3,0)`; rotate dir 1 -> `Point2(0,3)`.
* `schematics.rs`: `plans_for_sorts_by_priority`: schematic with a power-node at (0,0) and a copper-wall at (1,0) -> order [wall, node].

**Pitfalls.** `ClientPlan.config` and `BuildPlan.config` (`core/world/plan.rs:25`) are the same `crate::world::config::ConfigValue`; convert with `ClientPlan::from_build_plan`.

**Commit.** `PAR-0099: schematic plans helper and faithful rotate/flip of select plans`

---

### T6b. PAR-0099: F-drag copies an area into select plans; Z/X flip, wheel rotate, left-click places, right-click/Q cancels

**Symptom / acceptance.** Today F does nothing. Acceptance (Ground Zero): place three conveyors (130..132, 60) rotation 0 via `$H.place_block` + rotate (or `apply_sim_command_json` place ops), cursor on (130,60), push `KeyDown f`, move to (132,60), `KeyUp f` -> `$I.get_input_state_json()` `"select_plans": 3`; move the cursor 5 tiles east and push left down/up at (135,60) area -> `$H.player_build_plans_json()` lists 3 conveyor plans at (135..137, 60) rotation 0 (or they finish building). With `KeyDown x`... see Verify.

**Java.** `DesktopInput.pollInputPlayer` L596-657, 703-705, 726-731, 848-858 (verbatim logic):
```
if(!selectPlans.isEmpty()){ shiftX = rawCursorX - schematicX; shiftY = rawCursorY - schematicY; every plan += shift; schematicX += shiftX; schematicY += shiftY; }
if((keyTap(schematicSelect) || keyTap(rebuildSelect)) && !scene.hasKeyboard() && mode != breaking){ schemX = rawCursorX; schemY = rawCursorY; }
if(keyTap(clearBuilding) || isPlacing()){ lastSchematic = null; selectPlans.clear(); }
if(!scene.hasKeyboard() && selectX == -1 && selectY == -1 && schemX != -1 && schemY != -1){
    if(keyRelease(schematicSelect)){
        lastSchematic = schematics.create(schemX, schemY, rawCursorX, rawCursorY);
        useSchematic(lastSchematic);                       // block = null; schematicX/Y = cursor tile; selectPlans = toPlans(schem, schematicX, schematicY, true); mode = none
        if(selectPlans.isEmpty()) lastSchematic = null;
        if(!keyDown(rebuildSelect)){ schemX = -1; schemY = -1; }
    } else if(keyRelease(rebuildSelect)){ ... T7 ... }
}
if(!selectPlans.isEmpty()){ if(keyTap(schematicFlipX)) flipPlans(selectPlans, true); if(keyTap(schematicFlipY)) flipPlans(selectPlans, false); }
select tap: ... else if(!selectPlans.isEmpty()){ flushPlans(selectPlans); movedPlan = true; }     // plans stay selected (paste again)
deselect tap: ... else if(keyTap(deselect) && !selectPlans.isEmpty()){ selectPlans.clear(); lastSchematic = null; }
rotate axis (no diagonal key): if(!(isPlacing() && mode == placing) && !selectPlans.isEmpty() && !chatfrag.shown()) rotatePlans(selectPlans, sign(axisTap(rotate)));
breaking + F held: keyRelease(breakBlock) && keyDown(schematicSelect) && mode == breaking -> lastSchematic = create(schemX, schemY, rawCursor); then on release: removeSelection(..., maxSchematicSize) and useSchematic(lastSchematic) (cut = copy + break).   [implement]
```
`flushPlans` (InputHandler.java:1823-1832): each plan with `validPlace(x, y, block, rotation, null, ignoreUnits=true)` -> `unit.addBuild(copy)`. `schemOriginX/Y()` = `rawTileX/Y()` for flips/rotates.

**Port today.** `DesktopBridge` has no schematic state; `controller.state.select_plans: Vec<ClientPlan>` exists (`core/input/client_input.rs:40`) and is reported by `get_input_state_json` (`"select_plans": len`). `EcsSchematicWorld { grid, world, content, team }` (`core/game/schematics.rs:115`). `dispatch_action` has no arms for `schematic_select`/`schematic_flip_x`/`schematic_flip_y`; `KeyUp` is only used for the command-mode edge.

**Changes** (`gdext/input/desktop.rs` unless noted):
1. Fields: `schem_start: Option<(i32, i32)>`, `schematic_origin: (i32, i32)`.
2. `fn raw_cursor_tile(&self) -> Option<(i32,i32)>` = `cursor_tile()` (raw, no offset).
3. Key down arms: `"schematic_select" | "rebuild_select"`: if no `Break` drag active -> `schem_start = raw_cursor_tile()`. `"schematic_flip_x"` / `"schematic_flip_y"`: if `select_plans` non-empty -> `flip_plans_about(&mut select_plans, true/false, origin = raw cursor tile, info)` then `refresh_select_preview()`. `"clear_building"`: also clear `select_plans` (+ preview).
4. `handle` KeyUp: if `code` matches the `schematic_select` binding and `schem_start` is `Some` and no Place/Break drag active: `let plans = host.bind().copy_schematic_plans(start, raw_cursor, raw_cursor)`; `controller.state.select_block(None)` (+ host `select_block("air")`); `select_plans = plans`; `schematic_origin = raw cursor`; `schem_start = None` unless the `rebuild_select` key is held. (`rebuild_select` KeyUp is T7.)
5. `MindSimHost::copy_schematic_plans(&self, (x1,y1), (x2,y2), (cx,cy)) -> Vec<ClientPlan>` (pub, not func): build `EcsSchematicWorld { grid: &self.sim.grid, world: &self.sim.ecs.0, content, team: Some(self.player_team()) }`, `let schem = Schematics::new().create(&world, content, x1, y1, x2, y2, 64)`, `plans_for(&schem, cx, cy, true, content)` -> `ClientPlan::from_build_plan`. Content registry: the same one `content_registry()` returns.
6. `mouse_move`: if `select_plans` non-empty: shift all plans by `raw_cursor - schematic_origin`, update origin, `refresh_select_preview()`.
7. `refresh_select_preview()`: `preview_json` = select plans in the same JSON shape as `update_place_line` writes (`{x,y,rotation,block}`).
8. `left_press`: before the block/no-block branches (and after the command-mode branch): if `select_plans` non-empty -> for each plan `host.queue_player_place_config(x, y, block, rotation, config)` (T5; `can_place` inside rejects invalid ones = `validPlace`); keep `select_plans` (Java keeps them); return.
9. `right_press`: if `select_plans` non-empty and no block selected -> clear them (+ preview) and return (Java deselect branch precedes break).
10. `scroll` (and the `rotate` dispatch arm): if no Place drag and `select_plans` non-empty and the diagonal key is not held -> `rotate_plans_about(select_plans, sign(delta), raw cursor, info)`; return (no zoom: Java zoom condition requires `selectPlans.isEmpty()` unless diagonal held).
11. Selecting a block (`select_block_by_name`, `select_catalog_block`, `pick_block`) clears `select_plans` (`isPlacing()` branch).
12. Cut: in `right_release` when the `schematic_select` key is held and `schem_start` is Some: compute the plans with `copy_schematic_plans(schem_start, raw cursor, raw cursor)` BEFORE enqueuing the breaks, then set them as `select_plans` after enqueueing (Java order: create on release, removeSelection, then useSchematic). In `right_press` also set `schem_start = raw cursor` (Java L746-748).
13. `MindInput.use_schematic` seam for the schematics dialog (G05 T10b calls it). Java `SchematicsDialog` click in game -> `control.input.useSchematic(s)` -> `DesktopInput.useSchematic` (DesktopInput.java:520-528): `block = null; schematicX = tileX(mouseX); schematicY = tileY(mouseY)` (raw cursor tile, block is null); `selectPlans.clear(); selectPlans.addAll(schematics.toPlans(schem, schematicX, schematicY, checkHidden=true)); mode = none`.
    * `gdext/campaign.rs` (`MindCampaign`): `pub fn schematic_at(&self, index: i32) -> Option<mind_core::game::schematics::Schematic>` (not `#[func]`): `load_schematics(registry).all.get(index as usize).cloned()` — same indices as `list_schematics()` (`campaign.rs` ~L1605).
    * `gdext/input/mod.rs`: `#[func] pub fn use_schematic(&mut self, index: i32) -> bool`: get `/root/Spine/MindCampaign` as `Gd<MindCampaign>`, `schematic_at(index)` (drop that bind before touching the host), raw cursor tile from the bridge, `plans_for(&schem, cx, cy, true, content)` (T6a) -> `ClientPlan::from_build_plan`; clear the selected block (`controller.state.select_block(None)` + host `select_block("air")`); set `select_plans`, `schematic_origin = (cx, cy)`, `refresh_select_preview()`; return `!select_plans.is_empty()`. Returns false outside a game (`!in_game`, T3) or for a bad index.
    * Verify: Ground Zero, `$G --eval 'return get_node("/root/Spine/MindCampaign").list_schematics().size()'` (> 0, vanilla loadouts), `$I.use_schematic(0)` -> true and `get_input_state_json().select_plans > 0`; a left click places them (change 8).

`info` closure: from host content: `PlanBlockInfo { size: def.size, offset: def.offset, rotate: host.block_rotates(id), invert_flip: def.name == "electrolyzer" }`; add `MindSimHost::block_rotates(&self, id: BlockId) -> bool` calling `payload_block::block_rotates(&self.sim.ecs.0, id)` (path `mind_core::world::blocks::payloads::payload_block::block_rotates`, both modules are `pub`).

**Tests.** Rust (gdext has no harness): put the copy logic's core in `core` so it is testable: `core/game/schematics.rs` test `create_then_plans_for_copies_three_conveyors` — `EcsSchematicWorld` over a block-runtime `Sim` (`runtime_sim()` + `place(&mut sim, "conveyor", x, 10)` for x=10..12 from `core/sim/production_tests.rs`), `create(.., 10,10,12,10, 64)` -> width 3 height 1; `plans_for(&schem, 20, 20, ..)` -> plans at x = 19,20,21 (`t.x + 20 - 3/2` = t.x + 19), y = 20, rotation 0.

**Verify in-engine.** Ground Zero; seed copper (`debug_add_core_items("copper", 500)`); place 3 conveyors: `$H.place_block(130,62,"conveyor")` ... (131,62), (132,62). Cursor screen points via `$C.tile_to_screen`. Sequence (push_event): MouseMove(130,62) -> KeyDown f -> MouseMove(132,62) -> KeyUp f -> `get_input_state_json().select_plans == 3` -> MouseMove(136,62) -> KeyDown x / KeyUp x (flip y: rotations of 0 stay 0) -> MouseButton left down/up at (136,62) -> `$H.player_build_plans_json()` contains conveyors at (135,62),(136,62),(137,62) (centre = cursor, origin = 136: tiles 135..137). Screenshot the ghosts (drawn if G03 renders `placement_preview_json`; note if not). MouseButton right down/up -> `select_plans == 0`.

**Pitfalls.** KeyUp for F must be matched against the binding value (rebinding), not the literal `f`. `Schematics::create` with `team: Some(team)` only copies visible buildings (`was_visible`); player-team buildings always count. The cursor tile in `push_event` mode comes from the last pushed MouseMove (`bridge.mouse`).

**Commit.** `Fixes PAR-0099: schematic copy mode (F drag, Z/X flip, wheel rotate, click to place)`

---

### T7. Rebuild select (B), right-drag removes queued plans and ghosts, ghosts clear when rebuilt

**Symptom / acceptance.** Ghost plans of destroyed blocks accumulate (`68080c3`) but B does nothing, a right-drag over a queued (unstarted) plan on empty ground does not cancel it, and ghosts are never removed when the block is rebuilt. Acceptance: tests below; in-engine: destroy a wall, B-drag over it -> one player plan; it rebuilds and the team ghost list no longer contains it.

**Java.**
* `DesktopInput` L616-640: B tap sets `schemX/schemY` (shared with F, T6b); `keyRelease(rebuildSelect)` -> `rebuildArea(schemX, schemY, rawCursorX, rawCursorY)`; `if(!keyDown(schematicSelect)) schemX = schemY = -1`.
* `InputHandler.rebuildArea` (L2318-2342):
  ```
  r = Placement.normalizeArea(x1, y1, x2, y2, rotation, false, 999999999)      // min/max only
  r1 = Rect(r.x*8, r.y*8, (r.x2 - r.x)*8, (r.y2 - r.y)*8)
  for plan in player.team().data().plans (front to back):
      if block.bounds(plan.x, plan.y).overlaps(r1): player.unit().addBuild(new BuildPlan(plan.x, plan.y, plan.rotation, plan.block, plan.config))
  then for every building tile in the rect: tryRepairDerelict(tile)     // deferred, §1.4
  ```
  `Block.bounds(x, y)` = rect centred at `(x*8 + offset, y*8 + offset)` with side `size*8`. ARC `Rect.overlaps(r)`: `x < r.x + r.width && x + width > r.x && y < r.y + r.height && y + height > r.y` (strict).
* `InputHandler.removeSelection(x1, y1, x2, y2, flush=false, maxLength)` (L1880-1941), maxLength = 100 (`InputHandler.maxLength`) or 64 when F is held (`DesktopInput.java:789`):
  ```
  r = normalizeArea(x1, y1, x2, y2, rotation, false, maxLength)
  for each tile in the clamped rect: tile = world.tileBuilding(wx, wy); if tile != null: tryBreakBlock(wx, wy)   // validBreak -> unit.addBuild(BuildPlan(tile.build.tile))
  r1 = Rect(r.x*8, r.y*8, (r.x2 - r.x)*8, (r.y2 - r.y)*8)
  remove every non-breaking player plan whose bounds overlap r1
  remove every team ghost plan (data().plans) whose block bounds overlap r1 (plan.removed = true)
  ```
* `Logic.checkOverlappingPlans(team, tile)` on `BlockBuildEndEvent` (non-breaking) (Logic.java:53-58, 255-266): remove every team ghost whose bounds overlap the new block's bounds.

**Port today.** Ghosts: `CampaignRuntime.session.teams.get_or_null(TeamId(team))?.plans: VecDeque<BlockPlan>` (`core/game/teams.rs:129`; `BlockPlan { x: i16, y: i16, rotation: i8, block, config: Option<ConfigValue>, removed }`), written by `resolve_deaths` (`core/sim/unit_runtime.rs` ~L503-560). Mutable access: `self.ecs.0.get_resource_mut::<CampaignRuntime>()` then `.session.teams.get(TeamId(team))` (`teams.rs:318` returns `&mut TeamData`). `right_release` (desktop.rs ~L1213) emits `SimCommand::Break{player: Some(0)}` for non-air tiles only (cap 256, no length clamp). `Sim::queue_player_break` (builder.rs:802) removes a queued placement covering a non-air tile only. `core/input/placement.rs` `normalize_area(tilex, tiley, endx, endy, rotation, snap, max_length) -> NormalizeResult {x, y, x2, y2, rotation}`.

**Changes.**
1. `core/sim/builder.rs` `impl Sim` (doc-cite each Java method):
   * `fn block_bounds(&self, x: i32, y: i32, block: BlockId) -> (f32, f32, f32, f32)` = `(cx - s/2, cy - s/2, s, s)` with `cx = x*8 + offset`, `s = size*8` (content from `block_runtime`).
   * `fn rects_overlap(a, b) -> bool` per ARC.
   * `pub fn queue_team_rebuild(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> usize`: team = `live_default_team()`; r1 from `normalize_area(.., 0, false, 999_999_999)`; collect matching ghosts (clone) in deque order; for each call `queue_player_place_config(x, y, block, rotation.rem_euclid(4) as u8, config.unwrap_or(ConfigValue::None))` (T5); return the count queued.
   * `pub fn remove_plans_in(&mut self, x1, y1, x2, y2, max_length: i32)`: r from `normalize_area(.., 0, false, max_length)`; remove non-breaking player plans overlapping r1 (`with_player_plans`); remove + mark team ghosts overlapping r1.
   * `fn check_overlapping_plans(&mut self / world, team, anchor, block)`: called in the finish branch of `update_builder` for non-breaking completions (needs `&mut World`: implement as a free fn `check_overlapping_ghosts(world: &mut World, content, team: u8, x: i32, y: i32, block: BlockId)` operating on the `CampaignRuntime` resource; call it right after `out.events.push(BlockPlacedEvent ..)`).
2. `gdext/sim_host.rs`: pub forwards `queue_team_rebuild`, `remove_plans_in`; `#[func] pub fn team_ghost_plans_json(&self) -> GString` (`[{x,y,block,rotation}]` of the player team's ghosts; playtest seam).
3. `desktop.rs`:
   * KeyUp of the `rebuild_select` binding with `schem_start` Some and no Place/Break drag: `host.queue_team_rebuild(start, raw cursor)`; reset `schem_start` unless the `schematic_select` key is held.
   * `right_release`: before emitting breaks clamp the rect with `normalize_area(start, last, 0, false, if F held {64} else {100})` and iterate that rect; after enqueuing, call `host.remove_plans_in(start.0, start.1, last.0, last.1, max_len)`.

**Tests** (`core/sim/builder.rs`, `sim_with_player()` + a `CampaignRuntime` with `default_team: 1`, `phase = Playing` — copy the setup of `sim::combat_tests::destroyed_building_leaves_rebuild_plan`):
* `rebuild_select_queues_ghost`: set core copper 100; place copper-wall at (14,10) team 1; set its `Health.health = 0`, tick -> ghost exists; `queue_team_rebuild(13, 9, 15, 11)` == 1; `player_build_plans()` has (14,10) copper-wall; tick 120 -> wall rebuilt and the team ghost deque no longer has (14,10).
* `rebuild_select_single_tile_rect`: `queue_team_rebuild(14, 10, 14, 10)` == 1 (zero-width rect still overlaps, derivation in Java block above).
* `remove_plans_in_cancels_queued_and_ghosts`: queue a wall plan at (20,20) with an empty core (stays queued) and create a ghost at (22,20); `remove_plans_in(19, 19, 23, 21, 100)` -> no player plans, no ghosts.
* `adjacent_block_does_not_clear_ghost`: ghost at (14,10); a wall finished at (15,10) leaves it.

**Verify in-engine.** There is no building-damage seam (`grep -n "fn debug_damage\|fn damage_building" gdext/sim_host.rs` is empty). Add `#[func] pub fn debug_kill_building(&mut self, x: i32, y: i32) -> bool` to `gdext/sim_host.rs` that sets the building's `Health.health = 0.0` (the next tick's `resolve_deaths` destroys it and pushes the ghost). Then on Ground Zero: `debug_add_core_items("copper", 200)`; `place_block(135, 60, "copper-wall")`; `debug_kill_building(135, 60)`; `step` a few ticks (unpause); `team_ghost_plans_json()` contains (135,60); push MouseMove(134,59 screen) -> KeyDown b -> MouseMove(136,61) -> KeyUp b -> `player_build_plans_json()` has the wall; wait 3 s -> `building_debug(135,60)` shows the wall and `team_ghost_plans_json()` no longer lists it. Second check: queue a wall plan with copper 0 (`debug_add_core_items("copper", -<all>)`) at (140,60), right-drag (139,59)->(141,61) -> `player_build_plans_json()` empty.

**Pitfalls.** `CampaignRuntime` may be absent (editor/custom tests): every function must no-op then. Ghost `rotation` is `i8`. Do not iterate a `HashMap`. The ghost removal in `remove_plans_in` must happen even when no building is in the rect.

**Commit.** `PAR-0098: rebuild select, area removal of queued plans and ghosts, ghosts clear on rebuild`

---

### T8. Line placement: junction replacement, auto-bridges over obstacles, node spacing

**Symptom / acceptance.** Dragging a conveyor across an existing perpendicular conveyor overwrites it instead of placing a junction; dragging across walls/buildings stops nothing and does not insert bridges; dragging power nodes / bridges places one per tile instead of spacing them by range. Acceptance tests below + in-engine conveyor-over-conveyor junction.

**Java.** `InputHandler.updateLine` (L1943-1962): build `linePlans` from `iterateLine` (each plan `config = block.nextConfig()`); `if(settings.getBool("blockreplace", true)){ each plan: replace = plan.block.getReplacement(plan, linePlans); if(replace.unlockedNow()) plan.block = replace; block.handlePlacementLine(linePlans); }`. `iterateLine` calls `block.changePlacementPath(points, rotation, diagonal)` (L2465) before resolving rotations.
* `Conveyor.getReplacement` (Conveyor.java:111-121; same shape for `Duct` L112-121 needs the junction unlocked and not hidden, and `Conduit` L123-132):
  `cont(p) = plans contains o with o.x == req.x + p.x && o.y == req.y + p.y && (req.block is Conveyor || req.block is Junction)`;
  `return cont(d4(req.rotation)) && cont(d4(req.rotation - 2)) && req.tile().block() is Conveyor && mod(req.tile().build.rotation - req.rotation, 2) == 1 ? junctionReplacement : this`.
  Replacements: conveyor / titanium-conveyor / armored-conveyor -> `junction`, bridge `bridge-conveyor` (`Conveyor.init` L67-68); conduits -> `liquid-junction`, bridge `bridge-conduit` (Conduit L64-65); duct -> no junction, bridge `duct-bridge` (Duct L69-70).
* `handlePlacementLine`:
  * Conveyor (L94-99): `calculateBridges(plans, bridgeReplacement(ItemBridge), hasJunction = junctionReplacement != null, avoid = b instanceof Conveyor)`.
  * Conduit (L140-148): `calculateBridges(plans, bridge-conduit, true, b instanceof Conduit)`.
  * Duct (L124-129): `calculateBridges(plans, duct-bridge (DirectionBridge), hasJunction=false, b instanceof Duct || StackConveyor || Conveyor)`.
  * ItemBridge (ItemBridge.java:195-203): for consecutive plans `if(positionsValid(cur, next)) cur.config = Point2(next.x - cur.x, next.y - cur.y)`.
* `changePlacementPath` -> `Placement.calculateNodes(points, block, rotation, overlapper)` (Placement.java:152-185) with overlapper:
  * ItemBridge family (`bridge-conveyor` 4, `phase-conveyor` 12, `bridge-conduit` 4, `phase-conduit` 12): `max(|dx|, |dy|) <= range`.
  * DirectionBridge (`duct-bridge` 4, `reinforced-bridge-conduit` 4): same rule.
  * PowerNode (`power-node` laserRange 6, `power-node-large` 15, `surge-tower` 40, `beam-link` 500): `circle(src.worldx + offset, src.worldy + offset, laserRange*8)` overlaps the square of side `size*8` centred at `(other.worldx + offset, other.worldy + offset)` (PowerNode.java:216-219; ARC circle/rect: closest-point distance² < r²).
  * BeamNode (`beam-node` 10, `beam-tower` 23): only when `!diagonal`: `max(|dx|,|dy|) <= range + size - 1`.
  * LightBlock (`illuminator` radius 140): `dst2(point, other) <= (radius * 0.7 / 8)^2 * 3`.

**Port today.** `InputState::update_line` (`core/input/client_input.rs` ~L182) calls `iterate_line(.., &NoLineHooks, ..)` and builds `line_plans` with `ConfigValue::None`; no replacement or `handlePlacementLine`. `iterate_line` already calls `hooks.change_placement_path(&mut points, rotation, diagonal)` (`core/input/line.rs` ~L140). `calculate_nodes(world, team, points, block, rotation, overlapper)` and `calculate_bridges(world, plans, placer, has_junction, avoid)` + `RelativeBridgePlacer { block, unlocked, range, use_extra }` / `DirectionBridgePlacer` exist (`core/input/placement.rs` ~L410-720). `HostWorld::valid_place` (desktop.rs ~L146) only checks bounds; `build_rotation`/`is_chained` are default `None`/false. `circle_rect_overlap(cx, cy, radius, [x, y, w, h])` exists (`core/world/blocks/power/nodes.rs:57`). Setting `blockreplace` default true (`core/ui/settings.rs:187`).

**Changes.**
1. `core/input/client_input.rs`: `pub fn update_line_with(&mut self, world, block_meta, start, end, params, hooks: &dyn LineHooks, line_rules: Option<&LineRules>)`; `update_line` delegates with `&NoLineHooks, None`. After building `line_plans`, when `line_rules` is Some: (a) junction replacement per plan using the Java `getReplacement` rule (needs `world.block_at` + `world.build_rotation` of the existing tile and the existing tile's block "is conveyor family"); (b) `handlePlacementLine`: conveyor/conduit/duct -> `calculate_bridges` with the right placer and `avoid`; bridge blocks -> Point2 link configs per consecutive pair (`positions_valid`).
2. New `core/input/line_rules.rs` (header): 
   ```rust
   pub enum NodeRule { Chebyshev(i32), PowerLaser { range_tiles: f32, size: i32, offset: f32 }, Beam { range: i32, size: i32 }, Light { radius: f32 } }
   pub struct LineRules { pub node: Option<NodeRule>, pub junction: Option<BlockId>, pub bridge: Option<(BlockId, i32 /*range*/, bool /*direction bridge*/)>, pub avoid: fn(BlockKind) -> bool, pub is_bridge_self: Option<i32 /*range for Point2 linking*/> }
   pub fn line_rules_for(content: &ContentRegistry, block: BlockId) -> LineRules   // name table above (Java citations per entry)
   pub struct NodeHooks<'a> { rule: NodeRule, world: &'a dyn PlacementWorld, block: BlockId, team: u8 }
   impl LineHooks for NodeHooks<'_> { fn change_placement_path(..) { calculate_nodes(..) with the rule } }   // BeamNode: skip when diagonal
   ```
   Kind predicates: Conveyor family = `Conveyor | ArmoredConveyor`; Conduit family = `Conduit | ArmoredConduit`; Duct = `Duct`; Junction = `Junction`.
3. `HostWorld` (desktop.rs): implement `valid_place` through a new host seam `MindSimHost::can_place_player(x, y, block, rotation) -> bool` (calls the block runtime `can_place(&ecs, &grid, x, y, block, rot, player_team)` — the same call `queue_player_place` makes, `core/sim/builder.rs:785`; add `Sim::can_place_player`), and `build_rotation` from the grid building's rotation. Keep the struct read-only.
4. `update_place_line`: when the `blockreplace` setting is true (read once at bootstrap like `read_command_mode_hold`, default true), call `update_line_with` with `NodeHooks` (if the block has a node rule) and `line_rules_for`; else `update_line_with(.., &NoLineHooks, None)`.
5. `commit_place`: plans whose `config` is not `None` (bridge links) go through `host.queue_player_place_config` (T5); plans whose block changed (junction/bridge) use their own block id.

**Tests** (`line_rules.rs` / `client_input.rs`, with `placement.rs`'s test `GridWorld`):
* `conveyor_line_crossing_conveyor_becomes_junction`: existing conveyor at (5,5) rotation 1 (north), drag conveyor (3,5)->(7,5) rotation 0 -> plan at (5,5) is `junction`, others conveyor.
* `conveyor_line_over_wall_bridges`: copper-wall at (5,5), drag conveyor (3,5)->(8,5) rotation 0 (6 plans, index 2 = the wall tile, not placeable, not a junction case). Expected output (derived from the DP, costs conveyor 3 / junction 30 / bridge 200 / over-empty penalty 5): `[conveyor@(3,5), bridge-conveyor@(4,5) config Point2(2,0), bridge-conveyor@(6,5), conveyor@(7,5), conveyor@(8,5)]`, no plan at (5,5). Derivation: dp[0]=0, dp[1]=3, index 2 skipped; at i=3 the bridge from j=1 costs dp[n+1]+200 = 203+200 = 403 (penalty 0 because plan 2 is not placeable), beating j=0 (405); i=4/5 extend with conveyors (406, 409); backtrack gives the bridge pair (4,5)->(6,5).
  **Port bug to fix first**: `calculate_bridges`' `placeable` closure (`core/input/placement.rs` ~L480) uses `plan.placeable()` (block != air), but Java uses `plan.placeable(team)` = `Build.validPlace` (Placement.java `calculateBridges`: `plan.placeable(player.team()) || plan.tile().block() == plan.block`). Replace with `world.valid_place(plan.block, plan.x, plan.y, plan.rotation) || tile_block == plan.block`; otherwise no bridge is ever inserted. Re-run the existing `placement.rs` tests; if one encoded the old behaviour, fix its expectation and cite the Java line.
* `power_node_line_spacing`: power-node (laser 6, size 1) drag (0,0)->(20,0) on an empty world -> points `[0, 6, 12, 18, 20]` x-coordinates (circle radius 48 from tile centre reaches the square of the node 6 tiles away: closest distance 6*8-4 = 44 < 48; 7 tiles: 52 > 48).
* `bridge_line_spacing`: bridge-conveyor drag (0,0)->(10,0) -> `[0, 4, 8, 10]` and Point2 configs (4,0),(4,0),(2,0).
* `blockreplace_off_keeps_plain_line`.

**Verify in-engine.** Ground Zero: `$H.place_block(140,60,"conveyor")` then `configure`/rotate it to north (rotation 1: `apply_sim_command_json` place op with rotation 1 if supported, else place via the player plan path with rotation 1). Select conveyor, drag (138,60)->(142,60) with push_event -> `player_build_plans_json()` has a `junction` at (140,60). Screenshot.

**Pitfalls.** `calculate_bridges` takes `&mut SmallVec<[PlanCopy; 128]>`; convert `line_plans`. Unlock checks: `RelativeBridgePlacer.unlocked` = the bridge is in the unlocked catalog (use `true` in campaign with the imported full profile; read the unlock from content `block.unlock.unlocked()` to be exact). The preview JSON must show the replaced blocks (it already serialises `line_plans`).

**Commit.** `PAR-0098: line placement inserts junctions, bridges and spaced nodes`

---

### T9. Wheel / rotate / zoom parity (R + wheel rotates a placed block; wheel zooms unless placing a rotatable block)

**Symptom / acceptance.** Port: with any block selected the wheel only rotates (Java zooms when the selected block cannot rotate, e.g. a drill, and zooms with Ctrl held); R alone rotates the building under the cursor (Java: R alone does nothing, R held + wheel rotates it; zoom is suppressed while R is held).
Acceptance: unit test `scroll_routing_matches_java` (below) and in-engine: drill selected + wheel -> camera zoom changes; conveyor placed, R held + wheel up -> its rotation +1.

**Java.**
* Zoom (`DesktopInput.java:475-480`):
  ```
  if((!scene.hasScroll() || keyDown(diagonalPlacement)) && !chatfrag.shown() && !consolefrag.shown() && |axisTap(zoom)| > 0
     && !keyDown(rotatePlaced)
     && (keyDown(diagonalPlacement) || !Binding.zoom.value.equals(Binding.rotate.value) || ((!player.isBuilder() || !isPlacing() || !block.rotate) && selectPlans.isEmpty())))
      renderer.scaleCamera(axisTap(zoom));
  ```
* Rotation (`pollInputPlayer` L843-858): `if(!keyDown(diagonalPlacement) && |axisTap(rotate)| > 0){ rotation = mod(rotation + axisTap(rotate), 4); if(splan) ...; if(isPlacing() && mode == placing) updateLine(selectX, selectY); else if(!selectPlans.isEmpty() && !chatfrag.shown()) rotatePlans(selectPlans, sign(axisTap(rotate))); }`
  Line-angle override (L766-773): `if(mode == placing && block != null){ if(!overrideLineRotation && !keyDown(diagonal) && (selectX != cursorX || selectY != cursorY) && axisTap(rotate) != 0){ rotation = ((int)((Angles.angle(selectX, selectY, cursorX, cursorY) + 45) / 90f)) % 4; overrideLineRotation = true; } } else overrideLineRotation = false;`
* Rotate placed (L924-926): `if(cursor.build != null && cursor.interactable(team) && !isPlacing() && |axisTap(rotate)| > 0 && keyDown(rotatePlaced) && cursor.block().rotate && cursor.block().quickRotate) Call.rotateBlock(player, cursor.build, axisTap(rotate) > 0);`
  `InputHandler.rotateBlock` (L690-707): `build.rotation = mod(build.rotation + (direction ? 1 : -1), 4)` + `updateProximity`/`noSleep` (the port's `RemoteAction::Rotate` path already does this).

**Port today.** `desktop.rs` `scroll()` (~L1440): rotate when `rotate_on_scroll && is_building && block.is_some()`, else zoom; no rotatable check, no diagonal-key zoom, no R handling. `dispatch_action "rotateplaced"` -> `rotate_under_cursor` (rotates on tap). `dispatch_action "rotate"`/`"zoom"` handle non-wheel axis bindings. `BlockDef.quick_rotate` exists; `Block.rotate` via `MindSimHost::block_rotates` (added in T6b; add it here if T6b is not done yet).

**Changes.**
1. Pure routing helper in `core/input/desktop.rs` (core):
   ```rust
   pub struct ScrollCtx { pub diagonal_held: bool, pub rotate_placed_held: bool, pub zoom_is_rotate: bool, pub placing: bool, pub block_rotates: bool, pub select_plans: bool, pub cursor_build_quick_rotates: bool }
   pub struct ScrollRoute { pub zoom: bool, pub rotate_selection: bool, pub rotate_placed: bool }
   pub fn route_scroll(ctx: &ScrollCtx) -> ScrollRoute
   ```
   `zoom = !rotate_placed_held && (diagonal_held || !zoom_is_rotate || ((!placing || !block_rotates) && !select_plans))`;
   `rotate_selection = !diagonal_held` (rotation/line/select-plans per Java L843-858; applies even when zoom also happens, as in Java);
   `rotate_placed = !placing && rotate_placed_held && cursor_build_quick_rotates`.
2. `desktop.rs` `scroll()`: build `ScrollCtx` (`placing` = block selected and `is_building`; `block_rotates` via host; `cursor_build_quick_rotates` = building under the cursor is interactable (same team) and `block_rotates && def.quick_rotate`), then: if `rotate_selection` -> existing rotation step (placement rotation, line update, or T6b select-plan rotation); if `rotate_placed` -> `emit_action(RemoteAction::Rotate { x, y, direction: delta > 0 })`; if `zoom` -> `camera.zoom_by(delta)`.
3. `dispatch_action "rotateplaced"`: remove the rotate-on-tap behaviour (the key is now only a modifier). Keep `MindInput.rotate_placed()` `#[func]` for tests but document it as a test seam.
4. Line-angle override (Java L766-773) inside the rotation step when a Place drag is active and `!override_line_rotation`: `rotation = (((angle(select, cursor) + 45) / 90) as i32).rem_euclid(4)` with `angle` in degrees `[0,360)`.

**Tests** (core): `scroll_routing_matches_java`: (a) placing conveyor (rotates) -> zoom false, rotate true; (b) placing drill (no rotate) -> zoom true, rotate true; (c) diagonal held while placing conveyor -> zoom true, rotate false; (d) R held, nothing selected, quick-rotatable build -> zoom false, rotate_placed true; (e) nothing selected -> zoom true.

**Verify in-engine.** Ground Zero: `$I.select_block_by_name("mechanical-drill")`, read `$C.camera_state_json()` zoom, push `{"t":"Scroll","x":0,"y":1}`, read again -> changed. `place_block(140,60,"conveyor")`, `clear_building`, MouseMove over (140,60), KeyDown `r`, Scroll y 1, KeyUp `r` -> `building_debug(140,60)` rotation 1.

**Pitfalls.** The wheel event's `y` sign: `events.rs` maps WHEEL_UP to `+factor`; Java `axisTap(scroll)` is +1 for wheel up as well — keep the sign. `MindInput::input` already drops scroll over UI scroll containers (PAR-0100).

**Commit.** `Gameplay G08 T9: wheel zoom/rotate routing and R+wheel rotate-placed as in DesktopInput`

---

### T10. Block hotkeys: digit category/block combos, arrow-key grid navigation, middle-click pick, F1 info

**Symptom / acceptance.** Port: `1`..`0` pick the Nth block of the currently shown category; arrows do nothing; F1 opens a config panel; middle-click picks any block (locked, enemy, ignoring queued plans) and does not switch the palette category; `,`/`.` change the category without selecting its block. Java (below). Acceptance: (a) `2` then within 400 ms `3` selects the 3rd unlocked block of `production`... exactly: digit 1 = category `Category.all[0]` = turret, so `1`,`1` -> `duo`; `3` alone switches the palette to `distribution` (and, if a block was selected, selects distribution's remembered/first block); (b) middle-click on a placed `graphite-press` selects it and shows the `crafting` category; (c) F1 over a placed conveyor opens the `content` dialog for `conveyor`.

**Java.** `PlacementFragment.gridUpdate` (PlacementFragment.java:175-273) and `updatePick` (L132-172), transcribed:
```
updatePick: tile under cursor; if(keyTap(pick) && player.isBuilder() && !scene.hasDialog()){
   build = tile.build (null if build.inFogTo(team))
   tryBlock = build == null ? null : build instanceof ConstructBuild c ? c.current : build.block
   tryConfig = build == null || !build.block.copyConfig ? null : build.config()          // copyConfig default true
   for(req : player.unit().plans()) if(!req.breaking && req.block.bounds(req.x, req.y).contains(mouseWorld)){ tryBlock = req.block; tryConfig = req.config; break; }
   if(tryBlock != null && build == null && tryConfig == null) tryConfig = tryBlock.getConfig(tile)   // editor-only cases; skip
   if(tryBlock != null && tryBlock.isVisible() && unlocked(tryBlock)){ input.block = tryBlock; tryBlock.lastConfig = tryConfig; currentCategory = input.block.category; return true; }
}
gridUpdate: if(updatePick) return; if(chat/console shown || scene.hasKeyboard()) return;
for i in 0..14 (blockSelect01..10, Left, Right, Up, Down) if keyTap(blockSelect[i]):
   if(i > 9){ blocks = getUnlockedByCategory(currentCategory); cur = getSelectedBlock(currentCategory); j = index of cur;
       left: j = (j - 1 + n) % n; right: j = (j + 1) % n;
       up: j = (j > 3 ? j - 4 : n - n % 4 + j); j -= (j < n ? 0 : 4);
       down: j = (j < n - 4 ? j + 4 : j % 4);
       input.block = blocks[j]; selectedBlocks.put(cat, input.block) }          // only when cur is found in blocks
   else if(blockSelectEnd || Time.timeSinceMillis(blockSelectSeqMillis) > 400){       // first digit: category
       if(!getUnlockedByCategory(Category.all[i]).isEmpty()){ currentCategory = Category.all[i]; if(input.block != null) input.block = getSelectedBlock(currentCategory);
           blockSelectEnd = false; blockSelectSeq = 0; blockSelectSeqMillis = Time.millis(); } }
   else {                                                                               // 2nd / 3rd digit: block
       if(blockSelectSeq == 0) blockSelectSeq = i + 1;
       else { i += (blockSelectSeq - (i != 9 ? 0 : 1)) * 10; blockSelectEnd = true; }
       blocks = getByCategory(currentCategory); if(i >= blocks.size || !unlocked(blocks[i])) return true;
       input.block = blocks[i]; selectedBlocks.put(cat, input.block); blockSelectSeqMillis = Time.millis(); }
   return true
categoryPrev/Next: cycle currentCategory skipping empty ones; input.block = getSelectedBlock(currentCategory)
blockInfo: if(hovered() instanceof Unit u && u.type.unlockedNow()) ui.content.show(u.type)
   else { hovering = build under cursor (ConstructBuild -> current); display = menuHoverBlock ?? input.block ?? hovering; if(display != null && unlockedNow) ui.content.show(display) }
getSelectedBlock(cat) = selectedBlocks.get(cat) ?? first unlocked block of cat
```
`Category.all` order: turret, production, distribution, liquid, power, defense, crafting, units, effect, logic (digit 1..0). In command mode the digits are control groups (`DesktopInput.java:360`) — already ported.

**Port today.** `desktop.rs` digits -> `select_catalog_block(index)` (bridge catalog from `block_catalog()` without unlock filter); `category_prev/next` -> `fire_hotkey` -> `ui/fragments/placement_fragment.gd` `_on_hotkey` (cycles `_selected_category`, no block select); `block_info` -> `block_info_hotkey` (config panel first); `pick_block` (middle button, hard-coded in `mouse_button`). The fragment's `_catalog` (`MindUi.block_catalog_json()`) holds only non-empty, unlocked categories, each with `ordinal` (`Category.ordinal`) and `blocks[{name, localized, size}]`; `_selected_category` is an index into that list; `_select(index)`, `_select_block(host, name)` exist.

**Changes.**
1. `desktop.rs`: digits outside command mode, `block_select_left/right/up/down`, `category_prev/next`: only `fire_hotkey(name)` (remove `select_catalog_block` from the digit path; keep the method for `MindInput` callers if any — grep; delete if unused).
2. `ui/fragments/placement_fragment.gd` `_on_hotkey(action)`: implement the Java combo above with fragment state `_selected_blocks: Dictionary` (category name -> block name), `_block_select_seq: int`, `_block_select_end: bool = true`, `_block_select_ms: int`; `Time.get_ticks_msec()`. Category for digit i = the `_catalog.categories` entry whose `ordinal == i` (absent = empty -> ignore). Block lists: use the entry's `blocks` (unlocked list) for both `getByCategory` and `getUnlockedByCategory` (deviation: Java indexes the full category list; identical when every earlier block is unlocked, true for the imported full profile — note in the commit). `getSelectedBlock` = `_selected_blocks.get(cat, first block)`. "input.block != null" = `str(host.call("selected_block")) != "air"`. Selecting = `_select_block(host, name)`; category change = `_select(index)`. `category_prev/next`: after cycling, select `getSelectedBlock` of the new category. Record `_selected_blocks[cat] = name` whenever a block is selected through the palette click as well.
3. `pick_block` (desktop.rs) rewritten per `updatePick`: require a player unit (`has_player_unit`) and `!ui_dialog`; block = construct's `current` if the building is a construct (`MindSimHost::construct_progress(x,y)` returns `(current, progress, ..)`; use it when `Some`), else the building block; queued player plans under the cursor override (`player_build_plans()` bounds contain the cursor world point); the block must be in the unlocked catalog (ask the fragment? no: use `MindUi.block_catalog_json` names — cache a `HashSet<String>` of unlocked names in the bridge, refreshed on every pick from `/root/MindUi` `block_catalog_json()`); config: `read_config` of the building (host seam `building_config_value(x, y) -> ConfigValue`) stored into the T5 `last_config` map for that block when `def.save_config`; then `select_block` + `fire_hotkey("pick")`; the fragment, on `"pick"`, switches `_selected_category` to the category containing the selected block (`selected_block()` name lookup in `_catalog`).
4. `block_info_hotkey`: Java order: (1) hovered unit = `select_unit_tap(&self.selectable, team_any..)` — use the selectable list (all teams) closest to the raw cursor within `UNIT_TAP_RADIUS` -> `open_dialog("content", {"content": <unit type name>})` (type name via host content `unit(type_id).name`); (2) else `display = selected block (if any) else block under cursor (construct -> current)` -> `open_dialog("content", {"content": name})`. Never open the config panel here.

**Tests.** GDScript logic has no unit harness: verify in-engine. Rust: none new (pick/info touch Godot nodes).

**Verify in-engine.** Ground Zero with the imported profile. (a) push KeyDown/KeyUp `num1` then within 400 ms `num1` again -> `$H.selected_block()` == `"duo"`; (b) push `num3` -> placement fragment shows distribution (`get_node("/root/Spine/Ui/UiRoot/HudGroup/placement")._selected_category` maps to name `distribution`; find the node path with `$G --eval 'return get_node("/root/Spine/Ui/UiRoot").find_child("placement*", true, false)'`); (c) `place_block(140,60,"graphite-press")`, clear selection, MouseMove over it, `MouseButton middle down/up` -> `selected_block() == "graphite-press"` and category `crafting`; (d) KeyDown `f1` over a conveyor -> `get_node("/root/MindUi").dialog_stack()` ends with `content`. Screenshots per step.

**Pitfalls.** The fragment listens to the `hotkey` signal; digits are dispatched only in game (T3). The 400 ms window uses wall-clock ms (Java `Time.millis()`), not sim ticks. Keep `set_catalog_category` sync to the bridge (control groups do not need it any more, but `MindInput` callers may).

**Commit.** `Gameplay G08 T10: block hotkey combos, arrow navigation, pick and F1 as in PlacementFragment`

---

### T11. RTS residuals: stance/command ids, cancel_orders, select-all filters, middle-click queue, `set_unit_command` for the command panel (G03)

**Symptom / acceptance.** `unit_stance_hold_fire` sends stance 0 (= `stop`), every stance key is off by one; `cancel_orders` deselects instead of ordering `stop`; five unit commands have no key; `G` toggles command mode on by itself (Java: only works inside command mode); `select_all_unit_transport` selects everything; middle-click in command mode picks a block instead of queueing an order; G03's command panel has no `#[func]` to set a command; `SetUnitCommand` for `move` sends units to tile (0,0).

**Java.** `UnitStance.loadAll` (UnitStance.java:83-99) ids in order: `stop`(cancelOrders), `holdfire`, `pursuetarget`, `patrol`, `ram`, `boost`, `holdposition`, `mineauto`, then item stances. `UnitCommand` (UnitCommand.java:76-115): `move, repair, rebuild, assist, mine, enterPayload, loadUnits, loadBlocks, unloadPayload, loopPayload` with keybinds `unit_command_*`. Keys apply to `control.input.selectedUnits` filtered by `type.allowStance/allowCommand` (PlacementFragment.java:655-668): `Call.setUnitStance(player, ids, stance, !activeStances.get(stance.id))`, `Call.setUnitCommand(player, ids, command)`.
`InputHandler.setUnitCommand` (L411-444): for each own-team unit with `CommandAI` and `type.allowCommand(command)`: `reset = command.resetTarget || ai.currentCommand().resetTarget; ai.command(command); if(reset){ ai.targetPos = null; ai.attackTarget = null; }` — never a move order.
Select all (DesktopInput.java:330-357): only `if(commandMode && !scene.hasField() && !scene.hasDialog())`; `selectAllUnits`: own-team `isCommandable() && type.controlSelectGlobal` (or the on-screen subset with `controlSelectGlobal`); `selectAllUnitTransport`: `unit instanceof Payloadc`.
Middle click (`DesktopInput.touchDown` L944-956): `if(scene.hasMouse() || !commandMode) return false; ... if(button == Binding.commandQueue.value.key) commandTap(x, y, true)`. In command mode the block palette is replaced by the command table, so the pick (`PlacementFragment.gridUpdate`) does not run: middle = queued order only.

**Port today.** `desktop.rs` `dispatch_action` (~L494-640): `unit_stance_hold_fire => set_stance(0, true)` ... `unit_stance_hold_position => set_stance(5, true)`; `"cancel_orders" | "deselect"` clear the selection; `unit_command_move..mine => command_units(0..4)`; `select_all_units` auto-toggles command mode; `select_all_unit_transport` = all units; `select_all_unit_factories` clears. `mouse_button ("middle", true) => pick_block()`. `command_tap` reads `key_down(COMMAND_QUEUE)` (never true). `MindInput.set_unit_stance(stance: i64, enabled)` `#[func]` exists. Sim: `SimCommand::UnitCommand { units, command, x: 0.0, y: 0.0 }` from `SetUnitCommand` reaches `Sim::apply_live_unit_command` (`core/sim/mod.rs` ~L389-498), where `is_move` with target `(0,0)` resolves to tile (0,0) and sets `slot.target = Some((0,0))`.
Content lookups: `content.unit_command_by_name(&str) -> Option<UnitCommandId>`, `content.unit_stance_by_name(&str) -> Option<UnitStanceId>` (used in `core/ai/unit_command_runtime.rs:63`, `core/sim/tests.rs:234`).

**Changes.**
1. `desktop.rs`: replace numeric stance/command ids with name lookups through host content: stances `hold_fire->"holdfire"`, `pursue_target->"pursuetarget"`, `patrol`, `ram`, `boost`, `hold_position->"holdposition"`, `cancel_orders->"stop"`; commands `unit_command_<x>` -> `move, repair, rebuild, assist, mine, enterPayload, loadUnits, loadBlocks, unloadPayload, loopPayload`. These keys act only in command mode (Java: the command table is only shown then). `"deselect"` arm: remove (the right button is hard-coded; the binding name `deselect` is a mouse button and never arrives as a key). Stance `enabled`: always `true` (toggle needs per-unit stance state that is relay-only today — G01; note deviation).
2. `select_all_units`: return early unless `command_mode`; transport variant filters units whose selectable row has `"payload": true` — add `payload: bool` to `Sim::selectable_units_json` rows (`unit_runtime.rs` ~L1813: true when the unit entity has `PayloadComp` (`core/entities/comp/unit/comp.rs:251`)) and to `SelectableUnit` (`core/input/rts.rs`, `#[serde(default)]`-style parse in `parse_selectable_units`). `controlSelectGlobal`: Java sets it false only for `mono` (`J/content/UnitTypes.java:1357`); exclude units whose type name is `mono` from G-select-all (add `type_name` to the selectable row or compare against the content id of `mono`).
3. `mouse_button ("middle", true)`: `if command_mode { self.command_tap_queue() } else { self.pick_block() }` where `command_tap_queue` = `command_tap` with `queue = true`.
4. `MindInput` `#[func]`s for G03's command panel. G03 T10 (wave 1) DEFINES these two
   with exactly these signatures; if `grep -n "fn set_unit_command" client/rust/mind-gdext/src/input/mod.rs`
   finds them, do not redefine — only make the keybinding arms in change 1 call the same
   name-resolution helper. If G03 T10 is not on `main`, define them here as written:
   ```rust
   /// `Call.setUnitCommand(player, selected ids, command)`; `command` = UnitCommand name ("move", "mine", ...). False when nothing is selected or the name is unknown.
   #[func] pub fn set_unit_command(&mut self, command: GString) -> bool
   /// `Call.setUnitStance(player, selected ids, stance, enabled)` by stance name ("stop", "holdfire", ...).
   #[func] pub fn set_unit_stance_named(&mut self, stance: GString, enabled: bool) -> bool
   ```
   (`set_unit_stance(i64, bool)` stays.)
5. `core/sim/mod.rs` `apply_live_unit_command`: treat `is_move && x == 0.0 && y == 0.0` as a command switch without a target (`order = None`), matching `setUnitCommand` (Java never moves on a command switch). Doc comment: the `SetUnitCommand` wire carries `(0,0)`; a real move order to world (0,0) is impossible to distinguish — deviation accepted (world (0,0) is the corner tile's centre). Tell G01 in your summary (they own command sim).

**Tests.** `core/sim/tests.rs` (or wherever `commanded_dagger_reaches_target` lives — grep): `set_move_command_does_not_move_units`: spawn a commandable dagger at (100,100) team default, apply `SimCommand::UnitCommand { units: [id], command: move id, x: 0.0, y: 0.0 }`, tick 60 -> position within 1 px of (100,100). Bridge: unit test in `desktop.rs` tests: `stance_keys_resolve_names` (needs content: skip if the bridge cannot get content without a host; then verify in-engine only).

**Verify in-engine.** Ground Zero: spawn two daggers for the player team (`apply_sim_command_json {"op":"spawn_unit","unit":<dagger index>,"team":1,"px":1040,"py":460}` — check the team id via `$H.default_team()`), Shift-hold (`KeyDown shiftLeft`) + drag-select them, then `$G --eval "return $I.set_unit_command(\"mine\")"` -> true; `unit_debug_json()` shows the command; `return $I.set_unit_command(\"move\")` -> units do not move toward (0,0) over 2 s. Middle click in command mode on the ground -> units get a queued order (positions change toward it).

**Pitfalls.** `rebind` of command keys: they are unset by default; tests must `rebind("unit_command_mine","k")` first if exercising keys. Keep PAR-0140's verified paths untouched.

**Commit.** `Gameplay G08 T11: RTS stance/command keys by name, select-all filters, queue click, set_unit_command func`

---

### T12. Ctrl+click possession and V respawn — input path (sim half is G01)

**Symptom / acceptance.** Java: holding Ctrl and left-clicking an own-team AI unit (or a controllable turret) takes control of it; V returns to the core unit. Port: Ctrl+click only works in command mode and only writes a dead field; V recenters the camera. Acceptance (after G01's sim half): Ctrl+click a friendly `mega` -> `$H.unit_debug_json()` player row type `mega`; `[` near a dagger picks it up (PAR-0138 in-engine proof); V -> player type `alpha` at the core.

**Java.** `DesktopInput.update` L408-421:
```
if(!scene.hasMouse() && !locked && state.rules.possessionAllowed){
   if(keyDown(Binding.control) && keyTap(Binding.select)){
      Unit on = selectedUnit(); var build = selectedControlBuild();
      if(on != null){ Call.unitControl(player, on); shouldShoot = false; recentRespawnTimer = 1f; }
      else if(build != null){ Call.buildingControlSelect(player, build); recentRespawnTimer = 1f; } } }
```
`selectedUnit()` (InputHandler.java:2148-2163): `Units.closest(player.team(), mouseX, mouseY, 40, u -> u.isAI() && u.playerControllable())`, accepted only if its hitbox grown by 6 (each side) contains the cursor; else a `ControlBlock` building under the cursor with `canControl()`, same team, `unit != player.unit()`, `unit.isAI()` -> its unit. `selectedControlBuild()` (L2165-2171): building under the cursor with `canControlSelect(player.unit())` and same team.
Respawn (L423-431): `if(!player.dead() && !state.isPaused() && !scene.hasField() && !locked){ ...; if(keyTap(respawn)){ controlledType = null; recentRespawnTimer = 1f; droppingItem = false; Call.unitClear(player); } }`. `unitClear`/`unitControl` sim semantics: `InputHandler.java:775-877` (G01).
Payload key hold-repeat (`updateMovement` L1012-1040): tap -> try + `lastPayloadKeyTapMillis = now`; while held, every >20 ms after the tap is >200 ms old -> try again.

**Port today.** `left_press` command-mode branch (desktop.rs ~L1030) emits `RemoteAction::UnitControl { unit: Some(id) }` when `key_down(CONTROL)`; `SimCommand::UnitControl` only sets `Sim.controlled_unit` (`core/sim/mod.rs` ~L786). `RemoteAction::UnitClear` and `BuildingControlSelect` exist. `dispatch_action "respawn" => recenter_camera()`.

**Changes.**
1. Move the Ctrl+click check to the top of `left_press` (before the command-mode branch, after the `locked` gate): `if self.key_down(bindings, ids::CONTROL) { if let Some(id) = self.selected_unit_for_control() { emit UnitControl{Some(id)}; self.shooting = false; return; } if let Some((bx,by)) = self.selected_control_build() { emit BuildingControlSelect{x,y}; return; } }` (Ctrl also = diagonal placement and create-control-group; Java checks possession first and still continues to the select handling — Java does NOT return; to match, do not `return` after emitting: fall through to the normal press).
   `selected_unit_for_control`: from `self.selectable` (all rows), own team (`player_team()`), `commandable` (= AI-controlled, the port's proxy for `isAI && playerControllable`), closest to the raw cursor within 40; accept if `|dx| <= hit/2 + 6 && |dy| <= hit/2 + 6` — add `hit_size: f32` to the selectable row (same mechanism as T11's `payload`). `selected_control_build`: building under the cursor whose kind is a turret with `playerControllable` (Java `ControlBlock`: `Turret` with `playerControllable = true`, default true for turrets) and same team — use `BlockKind::ItemTurret | PowerTurret | LiquidTurret | ...` turret kinds.
2. `dispatch_action "respawn"`: `self.dropping_item = false; self.emit_action(RemoteAction::UnitClear);` (camera recentering stays in the camera follow logic).
3. Payload keys: track `payload_tap_ms` / `payload_hold_ms`; in `MindInput::process` (per frame) if `pickupCargo`/`dropCargo` key is held and `now - tap > 200 && now - hold > 20` -> repeat the action.
4. The sim half (`SimCommand::UnitControl` switching `UnitRuntime.player_unit`, the old core unit despawn, `UnitClear` respawn at the core with `CoreBlock.unitType`, `BuildingControlSelect`, player death respawn) is G01's. If G01's change is not on `main` when you reach T12, do changes 1-3 only, keep the commit headless ("input path only"), and record the dependency in your summary.

**Tests.** `desktop.rs` cannot be unit-tested without a host; verify in-engine.

**Verify in-engine** (only with G01's sim half): Ground Zero, spawn a player-team `mega` near the core (`spawn_unit` op), MouseMove over it, `KeyDown controlLeft`, `MouseButton left down/up`, `KeyUp controlLeft` -> `unit_debug_json()` player type `mega`; spawn a dagger next to it, KeyDown `leftBracket` -> dagger carried (PAR-0138 in-engine evidence); KeyDown `v` -> player type `alpha`.

**Commit.** `Gameplay G08 T12: Ctrl+click possession, V respawn and payload key repeat (input path)`

---

### T13. PAR-0102: rebinding an axis captures both directions (or a wheel)

**Symptom / acceptance.** Rebinding `move_x` captures one key and stores it as the positive key, unbinding the negative direction. Java: the rebind dialog asks twice (`keybind.press.axis` then `keybind.press`) for min then max; a wheel scroll captures `scroll` as a single-key axis. Acceptance: rebind `move_x` to `j` then `l` -> `keybinds_json()` row `move_x` shows `negativeDisplay` J and `display` L; `A`/`D` no longer move, `J`/`L` do.

**Java.** `KeybindDialog` (KeybindDialog.java:96-145): axis rows show `axis.key != null ? key : min + " [red]/[] " + max`; rebind: `rebindAxis = true; rebindMin = true; openDialog`. `rebind(bind, newKey)`: `if(isAxis){ if(newKey.axis || !rebindMin) bind.value = newKey.axis ? new Axis(newKey) : new Axis(minKey, newKey); } else bind.value = new Axis(newKey); save(); if(rebindAxis && isAxis && rebindMin && !newKey.axis){ rebindMin = false; minKey = newKey; openDialog(again) } else done`. Scroll in the capture dialog (`scrolled`) -> `rebind(bind, KeyCode.scroll)` only when `rebindAxis`. Bundle: `keybind.press.axis` = "Press an axis or key..." (bundle.properties:1565).

**Port today.** `gdext/input/bindings.rs` `rebind()` (L98-115): axis -> `state.set_axis(None, Some(code))`. `MindInput.rebind`/`rebind_key` `#[func]`s. `ui/dialogs/keybind_dialog.gd`: `_capture_name`, `_apply_capture(code, from_godot)`, `_on_capture_gui_input` (mouse buttons), `_unhandled_input` (keys), prompt `@keybind.press`. `keybinds_json` already reports `axis`, `negativeDisplay`, `display`.

**Changes.**
1. `MindInput`: `#[func] pub fn rebind_axis(&mut self, name: GString, negative: GString, positive: GString) -> bool` — `set_axis(Some(neg), Some(pos))` (empty string -> `None`), normalising Godot key text with `events::normalize_key_name` like `rebind_key`; `scroll` -> `set_axis(None, Some("scroll"))`; persist with `bindings::save`.
2. `keybind_dialog.gd`: when the row is an axis (`entry.axis`), start capture with prompt `@keybind.press.axis` and `_axis_min = ""`; first key -> store `_axis_min`, change prompt to `@keybind.press`, keep capturing; second key -> `input.rebind_axis(name, _axis_min, key)`; a mouse wheel event (`InputEventMouseButton` WHEEL_UP/DOWN, pressed) during an axis capture -> `rebind_axis(name, "", "scroll")` immediately. Axis row label: `display` when `negativeDisplay` is null, else `negativeDisplay + " [red]/[] " + display` via the existing markup helper.

**Tests.** `gdext/input/bindings.rs` tests: `axis_rebind_keeps_both` (call the new helper used by `rebind_axis` on a `BindingState`: `move_x` -> negative `j`, positive `l`; `axis_of(MOVE_X, "j") == Some(-1.0)`, `axis_of(MOVE_X, "l") == Some(1.0)`); `axis_rebind_scroll` -> `Axis{None, Some("scroll")}`.

**Verify in-engine.** Menu -> Settings -> Controls (keybind dialog is registered as `controls`: `get_node("/root/MindUi").open_dialog("controls", "")`); click Rebind on `move_x`, `godot_input` key `J`, then `L`; `return $I.keybinds_json()` row move_x. In game, hold `L` -> `player_unit_pos()` x increases. Restore with `$I.reset_keybinds()`.

**Commit.** `Fixes PAR-0102: axis rebinding captures both directions or the wheel`

---

### T14. Mouse buttons as bindings (pan, mouse_move, command_queue, rebinding) and view toggles (C, F5, F11)

**Symptom / acceptance.** `pan` (mouseForward) and `mouse_move` (mouseBack) never fire; rebinding `select`/`break_block`/`pick` to a key has no effect; C (hide HUD), F5 (power lasers) and F11 (fullscreen) do nothing. Acceptance: in-engine checks below.

**Java.** Arc treats mouse buttons as `KeyCode`s (`mouseLeft/Right/Middle/Back/Forward`) in the same binding system. `DesktopInput.update` L255-262: `if(keyDown(pan)){ panCam = true; panning = true; }`; L282-289: `if(panCam){ camera.x += clamp((mouseX - width/2) * panScale(0.005), -1, 1) * camSpeed; same for y }` (`camSpeed = (boost held ? 15 : 4.5) * delta`). `updateMovement` L991-993: `if(keyDown(mouseMove)) movement.add(mouseWorld().sub(player).scl(1/25f * speed)).limit(speed)`.
`HudFragment.java:454-456`: `if(keyTap(toggleMenus) && !chatfrag.shown() && !hasDialog && !hasField) toggleMenus()` (hides all HUD fragments; `ui.hudfrag.shown` false).
`DesktopInput.java:462-470` (F5): `if(settings.getInt("lasersopacity") == 0) settings.put("lasersopacity", settings.getInt("preferredlaseropacity", 100)); else { settings.put("preferredlaseropacity", lasersopacity); settings.put("lasersopacity", 0); }`.
`Control.java:690-694` (F11): `full = settings.getBool("fullscreen"); graphics.setFullscreen(!full); settings.put("fullscreen", !full)` (works on the menu too).

**Port today.** `events.rs` `translate`: mouse buttons -> `RawEvent::MouseButton{button:"left"|"right"|"middle"|"other"}`; X1/X2 = `"other"`. `DesktopBridge.pressed` holds key names only. `MindCamera2D` reads `MindInput.pan_pressed()` (camera.rs ~L340). No handlers for `toggle_menus`, `toggle_power_lines`, `fullscreen`.

**Changes.**
1. `events.rs` `mouse_button_name`: add `MouseButton::XBUTTON1 => "back"`, `XBUTTON2 => "forward"`.
2. `DesktopBridge::handle` `MouseButton`: also insert/remove the binding key name in `pressed` (`left->"mouseLeft"`, `right->"mouseRight"`, `middle->"mouseMiddle"`, `back->"mouseBack"`, `forward->"mouseForward"`) and, on a down edge, run `key_down_event(bindings, name)` for bindings other than `select`, `deselect`, `break_block`, `pick`, `command_queue` (those keep their pointer handlers). This makes `pan_pressed`, `key_down(COMMAND_QUEUE)` (T11 queue modifier) and `mouse_move` work.
3. `mouse_move` binding: `MindInput::player_control` (T1) adds `(aim - player)/25 * speed` to the axis when `key_down(MOUSE_MOVE)`: compute in the sim instead — add `PlayerInput.mouse_move: bool` and in `update_player` `movement += (aim - pos) * (speed / 25)` then limit to `speed` (Java order: `movement.set(axis).nor().scl(speed)` then add, then `limit(speed)`).
4. Camera pan: verify `MindCamera2D` applies the Java pan formula when `pan_pressed()`; if it only pans on edge-scroll, add the formula (camera.rs `process`).
5. `dispatch_action`: `"toggle_menus"` -> `Effect::Hotkey` consumed by `hud_fragment.gd` `_on_hotkey`: toggle visibility of `UiRoot/HudGroup` children that Java hides with `toggleMenus` (the HUD fragment root; keep dialogs); `"toggle_power_lines"` -> `hud_fragment.gd` performs the setting swap via `MindUi.settings_get_i32(key)` / `settings_set(key, value)` (`gdext/ui/ui_host.rs:661,632`); `"fullscreen"` -> GDScript `DisplayServer.window_set_mode(FULLSCREEN or WINDOWED)` + `settings_set("fullscreen", ..)` (handled in `ui_root.gd` via the hotkey signal; allowed on the menu per T3 change 2).

**Tests.** `desktop.rs` unit test `mouse_back_sets_pressed`: `handle(MouseButton{button:"back", down:true})` -> `key_down(bindings, ids::MOUSE_MOVE)`.

**Verify in-engine.** `godot_input mouse_button` with `MOUSE_BUTTON_XBUTTON2` held at a screen edge point -> `$C.camera_state_json()` position moves; push KeyDown `c` -> HUD hidden (screenshot), again -> shown; KeyDown `f5` -> `MindUi.settings_get_i32("lasersopacity")` toggles 0/100.

**Commit.** `Gameplay G08 T14: mouse buttons as bindings; HUD, power-laser and fullscreen toggles`

---

### T15. Input seams for hints (G05 T4b) and the command-mode objective (G04 T4)

**Why.** G05 T4b (HintsFragment completions) and G04 T4 (`CommandModeObjective`) read input state that only `MindInput` has.

**Java.**
* `HintsFragment` completions (from G05 T4b's table): `desktopShoot` -> `player.shooting`; `schematicSelect` -> `keyRelease(schematicSelect) || keyTap(pick)`; `conveyorPathfind` -> `keyRelease(diagonalPlacement)`; `boost` -> `keyDown(boost)`; `rebuildSelect` -> `input.isRebuildSelecting()` = `input.keyDown(Binding.rebuildSelect)` (InputHandler.java:2140-2142).
* `MapObjectives.CommandModeObjective.update` (MapObjectives.java:671-675): `control.input.selectedUnits.contains(u -> u.isCommandable() && u.command().hasCommand())`; `CommandAI.hasCommand()` = `targetPos != null` (CommandAI.java:562-564).

**Port today.** Neither seam exists. G04 T4 defines `#[derive(Resource, Default)] pub struct ObjectiveInputState { pub command_mode_satisfied: bool }` in `core/game/runtime.rs` (read by its objective env); nothing sets it. Port "has command": the unit's `ControllerSlot { kind: AiKind::Command, target: Some(_) }` (`core/sim/mod.rs` `apply_live_unit_command` writes `slot.target`).

**Changes.**
1. `DesktopBridge`: `hint_keys: Vec<&'static str>`; push (deduplicated per drain) at these points: `"shoot"` when `left_press` sets `shooting = true` (T1); `"boost_down"` on the KeyDown edge of the `boost` binding (and once per `process` frame while held, so a hint shown after the press still completes: in `MindInput::process`, `if key_down(BOOST) { push "boost_down" }`); `"schematic_select_release"` on the KeyUp of `schematic_select` (T6b handler); `"pick_tap"` in `pick_block` when it selects; `"diagonal_release"` on the KeyUp of `diagonal_placement`; `"rebuild_selecting"` every `process` frame while the `rebuild_select` key is held.
2. `MindInput`: `#[func] pub fn take_hint_keys(&mut self) -> PackedStringArray` — returns and clears `bridge.hint_keys` (order of first occurrence).
3. `core/sim` (`impl Sim`, `core/sim/unit_runtime.rs` next to `selectable_units_json`): `pub fn any_unit_has_command(&self, ids: &[i32]) -> bool`: true when any live unit whose `EntitySeq` is in `ids`, on `live_default_team()`, has `ControllerSlot.kind == AiKind::Command && slot.target.is_some()`.
   Do NOT add a setter: G04 T4 defines `Sim::set_command_mode_satisfied(&mut self, value: bool)` (inserts/updates `ObjectiveInputState`) and its `#[func] MindSimHost::set_command_mode_satisfied(value: bool)`; use those (read the resource first and only write when the value changes).
4. `gdext/sim_host.rs`: pub forward of `any_unit_has_command` (the setter forward already exists from G04 T4). `MindInput::process`, after the bridge update: `let ids = selected_units; let sat = host.bind().any_unit_has_command(&ids); host.bind_mut().set_command_mode_satisfied(sat);` (separate bind scopes; not inside a `bind()` of the bridge's host clone). Java evaluates the objective every update, so per frame is correct.
5. If G04 T4 is not on `main` yet (`grep -n "struct ObjectiveInputState" client/rust/mind-core/src/game/runtime.rs` prints nothing), do changes 1-2 only and record the dependency.

**Tests.** `core/sim/tests.rs`: `any_unit_has_command_needs_target`: spawn a commandable dagger (copy the setup of `commanded_dagger_reaches_target`), `any_unit_has_command(&[id]) == false`; apply a move `SimCommand::UnitCommand` to (200, 200) -> `true`. `set_command_mode_satisfied(true)` (G04's) -> resource reads true.

**Verify in-engine.** Ground Zero: push `KeyDown f`, `KeyUp f` -> `$I.take_hint_keys()` contains `"schematic_select_release"`; second call returns `[]`. The objective half is headless only (no objective-input readback seam; G04 T4 verifies the objective itself in-engine): add `#[func] pub fn command_mode_objective(&self) -> bool` to `gdext/sim_host.rs` (reads the resource) and check it turns true after Shift-drag-selecting two spawned player-team daggers and right-clicking a point.

**Pitfalls.** Payload/configure sounds are G07's (emitted sim-side): do not add audio here. Do not hold the `MindInput` bind while calling the host from `process` beyond the existing pattern.

**Commit.** `Gameplay G08 T15: hint key edges and command-mode objective input seams`

---

## 3. Execution order, dependencies, shared files

Order (player impact first, dependencies respected):

| # | Task | Depends on | Notes |
|---|---|---|---|
| 1 | T1 shooting/aim/facing/boost | — | biggest gap: the player cannot fight |
| 2 | T2a item stack sim | — | |
| 3 | T2b tap-unit deposit | T1, T2a | |
| 4 | T3 PAR-0101 game gate, M/F8, Esc pause | — | |
| 5 | T4 even-size cursor offset | — | |
| 6 | T5 plan config | — | needed by T6b, T7, T8, T10 |
| 7 | T6a schematic core helpers | — | |
| 8 | T6b PAR-0099 copy mode | T5, T6a | adds `MindSimHost::block_rotates` |
| 9 | T7 rebuild select / area removal / ghost clear | T5 | uses T6b's `schem_start` (if T6b is not done, add the field here) |
| 10 | T8 line junction/bridge/nodes | T5 | |
| 11 | T9 wheel routing | T6b (select plans, `block_rotates`) | if T6b is blocked, add `block_rotates` here and skip the select-plan branch |
| 12 | T10 block hotkeys/pick/F1 | T3, T5 | |
| 13 | T11 RTS residuals + `set_unit_command` | — | tell G03 the two func signatures |
| 14 | T12 possession / respawn input | G01 sim half | headless-only commit if G01 has not landed |
| 15 | T13 PAR-0102 axis rebind | — | |
| 16 | T14 mouse bindings, view toggles | T1 (`PlayerInput`) | |
| 17 | T15 hint keys + command-mode objective | T1, T6b, T10; G04 T4 (resource) | G05 T4b and G04 T4 consume it |

Build once per task (env §1), restart the editor after every Rust build.

Files shared with other areas (pull `main` before editing, keep commits small):
* `gdext/input/desktop.rs`, `gdext/input/mod.rs` — G08 owns; G05 T4b calls `take_hint_keys()` (T15), G05 T10b calls `use_schematic(index)` (T6b change 13); G03 T16/T17 read `command_mode()`/`selection_json()` and will call `set_unit_command` / `set_unit_stance_named` (T11).
* `gdext/sim_host.rs` — every area adds seams; add new functions at the end of the `#[godot_api] impl` block.
* `core/ai/types/player.rs`, `core/sim/unit_runtime.rs` (T1, T11, T12) — G01 (player possession, commands).
* `core/sim/builder.rs` (T5, T7) — G02/G04 may touch construction.
* `core/world/blocks/storage/core_block.rs`, `core/world/blocks/defense/turrets/behavior.rs`, `core/world/blocks/distribution/transfer.rs` (T2a) — G02.
* `core/sim/mod.rs` (T2a Inventory arm, T11 `apply_live_unit_command`) — G01.
* `core/input/placement.rs` (T8 bug fix) — G08.
* `ui/fragments/placement_fragment.gd` (T10) — G03 T17 adds the command panel in the same file; `ui/fragments/hud_fragment.gd`, `ui/ui_root.gd` (T3, T14) — G03/G05.
* `core/game/schematics.rs` (T6a) — G05 (schematics dialog PAR-0143) may touch it.

## 4. Ledger changes to report (do not edit `chains.json`)

| Item | Change |
|---|---|
| PAR-0099 | -> verified after T6b in-engine check (commit hash + run dir). |
| PAR-0101 | -> verified after T3 (menu T/F8 inert; in-game single toggles). |
| PAR-0102 | -> verified after T13. |
| PAR-0098 | stays `fixing`; note slices T4 (cursor offset), T5 (plan config), T7 (rebuild select/area removal/ghost clear), T8 (line replacement). Remaining: plan/ghost/beam drawing (G03), derelict repair (G02). |
| PAR-0138 | in-engine proof via T12 (Ctrl+click a `mega`, `[` picks up a dagger) once G01's possession lands; until then stays `fixing` with note "key path headless-verified (4b7067b); in-engine needs possession". |
| PAR-0140 | add note: residuals fixed in T11 (stance id off-by-one, cancel_orders, select-all filters, queued middle click, set-command no longer moves to (0,0)). |
| new | "Player unit never shoots, never faces the cursor/movement (desktop updateMovement aim/controlWeapons missing)" -> fixed by T1. |
| new | "Item withdraw/deposit use a phantom relay stack; no range/acceptStack; no tap-unit deposit" -> T2a/T2b. |
| new | "M and F8 toggle twice per press in game (binding dispatch + ui_root.gd)" — file only if T3's before-check confirms it. |
| new | "Even-size blocks placed one tile off the cursor (InputHandler.tileX block offset)" -> T4. |
| new | "Build plans drop their config (schematic/rebuild/sorter nextConfig)" -> T5. |
| new | "Ghost rebuild plans never clear; B rebuild select missing; right-drag does not cancel queued plans" -> T7. |
| new | "Line placement: no junction replacement, auto-bridge or node spacing; calculate_bridges placeable predicate ignores validPlace" -> T8. |
| new | "Wheel always rotates while a block is selected; R taps rotate (Java: R+wheel)" -> T9. |
| new | "Digit hotkeys select the Nth block instead of category/block combos; arrows/pick/F1 differ" -> T10. |
| new | "Ctrl+click possession and V respawn are inert" -> T12 + G01. |
| new | "Mouse back/forward bindings (pan, mouse_move) never fire; C/F5/F11 unhandled" -> T14. |
| new (deferred) | "Plan editing gestures: drag an unstarted plan, click to delete break plans, Shift reverse flush, Ctrl plan config" — not planned (§1.4). |

## 5. Done criteria

* Every task committed with its tests passing: `cargo test -p mind-core --lib` filters named in each task, full `cargo test -p mind-core --lib --features msav-import` green, `cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` clean, GDScript parse gate clean (env §2), no new `mind-headless` failures beyond G00's list.
* In-engine acceptance evidence for T1, T2a, T2b, T3, T4, T6b, T7, T8, T9, T10, T11, T13, T14 under `runs/<date>-s<N>-G08-T<k>/` (PNG + eval JSON); T5/T6a headless; T12 as available.
* 0 `panic` lines in the session log after each in-engine step.
* `docs/parity-plans/G08-input-controls_summary.md` written and committed (env §6 shape), including the §4 ledger list with commit hashes, G01/G03 hand-offs (T11 func signatures, T12 dependency), and every UNVERIFIED item's observed value.

## 6. Cross-area findings (not tasks here)

1. **G01 — player death/respawn missing:** `UnitRuntime.player_unit` is set once (`core/sim/unit_runtime.rs:890`) and never cleared; there is no `PlayerComp.update` death timer / `CoreBuild.requestSpawn` (PlayerComp.java:233-240, `deathDelay = 60`). After the alpha dies the player can never build or move again.
2. **G01 — possession sim half:** `SimCommand::UnitControl/UnitClear/BuildingControlSelect` only write `controlled_unit`/`control_building` (`core/sim/mod.rs` ~L786-818). Needed: switch `player_unit` (+ `AiKind::Player`), despawn the old `spawnedByCore` unit, `unitClear` docking respawn (InputHandler.java:775-877). T12 calls these.
3. **G01/G04 — core unit type:** `MindSimHost::spawn_player_unit` always spawns `alpha` (`gdext/sim_host.rs` ~L1928); Java uses `((CoreBlock) core.block).unitType` (foundation -> beta, nucleus -> gamma).
4. **G01 — stances are relay-only:** `SimCommand::UnitStance` -> `ensure_unit_commands().set_unit_stance` (`core/sim/mod.rs` ~L820) never reaches live units; stance keys/panel (T11) have no gameplay effect until fixed. Also the stance toggle (`!activeStances`) needs a per-unit stance readback.
5. **G01 — `SetUnitCommand` move to (0,0):** fixed defensively in T11 change 5 (`apply_live_unit_command`); G01 should know (they own the file).
6. **G02 — derelict repair:** `Build.validPlace`/`beginPlace` derelict branches (Build.java:94-100, 254-260) are missing, so `tryRepairDerelict` (click a derelict block / rebuild-select over derelicts, InputHandler.java:2070-2081, 2331-2341) cannot be ported. Ground Zero has derelict scrap walls.
7. **G02 — stack hooks:** G02 T1 lands the trait-default `accept_stack` first; T2a then adds `CoreBehavior`/`TurretBehavior` `accept_stack`/`handle_stack`/`remove_stack` and `transfer::dispatch_accept_stack` (other item-accepting blocks keep the default `accept_stack = 0`, so player deposits into e.g. crafters/containers need their own overrides: `StorageBehavior` already has one).
8. **G03 — drawing:** select-plan ghosts (T6b `placement_preview_json`), team ghost plans (T7 `team_ghost_plans_json`), the carried item stack on the player unit (T2a), shooting bolts are sim bullets (already drawn), schematic-flip hint label (`DesktopInput.buildUI` L111-128), building/respawn hints (L84-109).
9. **G03 — block inventory fragment:** `ui/fragments/block_inventory_fragment.gd` withdraw amounts should follow `BlockInventoryFragment.takeItem` (click = whole amount of that item capped by `maxAccepted`, hold = 1 per `holdWithdraw` ticks) and disable buttons outside `itemTransferRange`; after T2a the sim enforces range/capacity.
10. **G05 — Esc pause dialog:** T3 makes Esc pause the sim; the paused dialog's own buttons (G05/G03) are unchanged.
