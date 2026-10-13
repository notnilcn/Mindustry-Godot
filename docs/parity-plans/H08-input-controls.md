<!-- SPDX-License-Identifier: GPL-3.0-only -->
# H08 — In-game input and controls, desktop (2026-10-14)

Area: everything the desktop player does with keyboard and mouse after
entering a campaign sector — placement lines, taps, block hotkeys, RTS
command keys, possession, payload keys, camera, mouse bindings, view toggles.
Java authority: `J/input/DesktopInput.java`, `J/input/InputHandler.java`,
`J/input/Binding.java`, `J/input/Placement.java`,
`J/ui/fragments/PlacementFragment.java` (input half).

**Read `docs/H-environment.md` first, fully** (abbreviations `REPO`, `WT`,
`core/`, `gdext/`, `ui/`, `J/`, `ARC/`; shorthands `$G`, `$H`, `$I`, `$C`,
`$MC`; §4.1 sector entry; §4.2 seams; §6 commit rules). Then
`docs/H-INDEX.md` §3 if it exists. Then this plan.

Start state: H00 is on `main`:
```bash
git log main --oneline | grep -m1 'H00'      # must print a line
```
After H00 the player alpha really spawns on Ground Zero
(`$H.has_player_unit()` is `true`, rules `env` 233) and
`SimCommand::Place{player: Some}` no longer applies instantly. Use the real
player everywhere; do NOT use `debug_spawn_player` unless a step says so.

Drift rule: port line numbers were read on `main` = `ffbe84dd` (2026-10-14).
If a line moved, re-locate it with `grep -n "<quoted function name>"`. If a
function does not exist at all, or the code already does what a task asks,
STOP that task, write down what you saw, and continue with the next task.
Java line numbers are exact (checked 2026-10-14).

Live input path (verified on `ffbe84dd`): Godot event ->
`gdext/input/events.rs` `translate` -> `MindInput::input`
(`gdext/input/mod.rs:111`, UI/dialog/lock gates) -> `DesktopBridge::handle`
(`gdext/input/desktop.rs:465`) -> `key_down_event`/`key_up_event` ->
`dispatch_action(name)` (`desktop.rs:622`); mouse:
`mouse_button` (`desktop.rs:1599`), `mouse_move` (`:2033`), `scroll`
(`:2279`). Commands leave through `emit_action` (`RemoteAction` ->
`ActionBatcher` -> `MindSimHost::enqueue_sim_command`, applied at the next
tick) or direct host calls (`host.bind_mut().<fn>()`). The player unit is
driven by `PlayerInput` (`core/ai/types/player.rs`), uploaded each host
`process` from `MindInput::player_control` (`mod.rs:965`).
`MindInput.push_event(json)` queues a `RawEvent` dispatched in
`MindInput::process` through the same bridge code:
`{"t":"KeyDown","code":"f"}`, `{"t":"KeyUp","code":"f"}`,
`{"t":"MouseMove","x":X,"y":Y}`,
`{"t":"MouseButton","button":"left","down":true,"x":X,"y":Y}`,
`{"t":"Scroll","x":0,"y":1}`. Key codes are registry names (`f`, `z`,
`shiftLeft`, `controlLeft`, `num1`, `escape`, `leftBracket`, `comma`,
`left`, `f1`...). `push_event` skips `MindInput::input`'s UI gates (fine for
world tests). Use `godot_input` (real Godot events) only where a task says.

Screen point of a tile: `$G --eval "$C.center_on_tile(TX, TY); return true"`,
then (next call) `$G --eval "return $C.tile_to_screen(X, Y)"` -> `(SX, SY)`
viewport coords. Use those in `push_event`. While the player is alive the
camera follows the player again on the next frame, so read `tile_to_screen`
right before each push (or pause first: `$H.set_paused(true)` — but note
T10 changes paused-camera behaviour; while paused the camera stays where
it is after T10).
(Verified 2026-10-14, session 3: `$C.center_on_tile` is undone on the next frame while a
player exists; for off-player areas use `$C.set_process(false); $C.center_on_tile(X, Y)` and
re-enable with `$C.set_process(true)`. Free ground on Ground Zero: (132..147, 57..72);
x >= 150 at y 55..80 is static wall and y > 80 is outside `limit_area`. Plans/commands pushed
while paused apply at the next `step`, and `clear_player_build_plans()` before that step is a
no-op.)

---

## 1. Inventory

### 1.1 Binding-by-binding (Java `Binding.java` 88 binds; port registry `core/input/binding.rs` is 1:1 for names/defaults)

"Port handler" = where the action happens today (`desktop.rs` unless noted).
Status: OK = behaves like Java; WRONG/MISSING = task; DEFER = not planned.

| Binding | Default | Java consumer (file:line) | Port handler | Status / task |
|---|---|---|---|---|
| move_x / move_y | A/D, S/W | `DesktopInput.updateMovement` L975+; camera pan when dead/paused L277-283 | `pan_axis` -> `player_control` -> sim; camera follows player | unit OK; **paused/dead camera pan MISSING** T10 |
| mouse_move | mouseBack | updateMovement L994-996; camera L278 | never fires (mouse buttons are not keys) | MISSING T9 |
| pan | mouseForward | L261-265, L292-295 | `pan_pressed` always false; formula unused with a player | MISSING T9+T10 |
| boost | shiftLeft | `player.boosting` L1010; camSpeed L239; flushPlansReverse L785 | boosting OK (`player_control`); camera boost OK | OK; reverse flush DEFER |
| respawn | V | L428-433 `Call.unitClear` | `RemoteAction::UnitClear` | OK |
| control | controlLeft | L424-436 possess unit / control building | `left_press` top; turret routed to `BuildingControlSelect` | **WRONG** T8 |
| select | mouseLeft | everything left | hard-coded `"left"` | OK; rebinding ignored T9 |
| deselect | mouseRight | L608-610 (stop mining), L734-739 (clear block/plans) | `right_press` clears block / select plans; **mine stop MISSING** | T4 |
| break_block | mouseRight | L740-748 | `right_press`/`right_release` | OK (break rect, removeSelection) |
| pickupCargo / dropCargo | [ / ] | updateMovement L1012-1040 (tap + hold repeat) | `payload()` + `update_payload_hold` | code OK; in-engine unverified T8 |
| clear_building | Q | L612-614, L632 | `dispatch_action` | OK |
| pause_building | E | L671-678 (`isBuilding` toggle; `player.shooting = false` when resuming) | `dispatch_action` | shooting reset MISSING T4 |
| rotate | scroll | L837-849 | `scroll()` / `"rotate"` | OK except line-angle snap **WRONG** T1 |
| rotateplaced | R | L898-900 (modifier) | `scroll()` route | OK |
| diagonal_placement | controlLeft | line diagonal; zoom override | `update_place_line` | OK |
| pick | mouseMiddle | `PlacementFragment.updatePick` L132-172 | `pick_block` (any block, no plans/unlock/category) | WRONG T5 |
| ping | P | multiplayer | none | DEFER (multiplayer) |
| rebuild_select | B | L616-640, `rebuildArea` IH:2318-2342 | `finish_rebuild_select` | OK; derelict half T11 |
| schematic_select | F | L616-640 | `finish_schematic_copy`, cut in `right_release` | OK |
| schematic_flip_x / _y | Z / X | L642-650 | `flip_select_plans` | OK |
| schematic_menu | T | L452 | opens `schematics` | OK |
| command_mode | shiftLeft | L300-311 (incl. canBoost guard L302) | `update_command_mode` | **canBoost guard MISSING**, not gated on in_game T7 |
| command_queue | mouseMiddle | `touchDown` L924-934 | `key_down(COMMAND_QUEUE)` never true; middle always picks | WRONG T6b+T9 |
| create_control_group | controlLeft | L360-401 | `control_group` | OK |
| select_all_units | G | L316-329 (command mode only; `controlSelectGlobal`) | auto-enables command mode; no filter | WRONG T6a |
| select_all_unit_factories | H | L346-357 (commandable buildings) | clears selection | WRONG T6a |
| select_all_unit_transport | unset | L331-344 (`Payloadc`) | same as select all | WRONG T6a |
| select_across_screen | altLeft | modifier | `viewport_world_rect` | OK |
| cancel_orders | unset | stance `stop` (PlacementFragment command table) | clears selection | WRONG T6a |
| unit_stance_* (6) | unset | `setUnitStance` | ids off by one (`hold_fire` sends 0 = `stop`) | WRONG T6a |
| unit_command_* (10) | unset | `setUnitCommand` | 5 of 10 mapped; numeric ids | WRONG T6a |
| category_prev / _next | , / . | PF L240-258 (cycle + `input.block = getSelectedBlock`) | fragment cycles only | WRONG T5 |
| block_select_left/right/up/down | arrows | PF L188-207 | none | MISSING T5 |
| block_select_01..10 | 1..0 | PF L208-232 (category/block combos); control groups in command mode | Nth block of visible category | WRONG T5 |
| zoom | scroll | L475-482 | `route_scroll` | OK |
| detach_camera | unset | L248-255 + updateMovement L982-990 | toggles follow only | DEFER (unset by default) |
| teleport_cursor | unset | L243-246 (editor/infinite) | none | DEFER |
| menu | escape | `Control.java` | `ui_root.gd` | OK (PAR-0101) |
| fullscreen | F11 | `Control.java:690-694` (also on menu) | `dispatch_action("fullscreen")` has **no arm** | MISSING T9 |
| pause | space | `Control` | `dispatch_action` | OK |
| skip_wave | unset | `HudFragment.java:461` (`canSkipWave`) | none | DEFER (unset) |
| minimap | M | L449 | effect | OK |
| research / planet_map | J / N | L450-451 campaign only | effect + close keys | OK |
| block_info | F1 | PF L260-271 (unit/content info only) | opens the **config panel** first | WRONG T5 |
| toggle_menus | C | `HudFragment.java:454-457` | `hud_fragment.gd _toggle_menus` | OK (G03) |
| screenshot | F12 | full-map screenshot | viewport capture | DEFER (cosmetic) |
| toggle_power_lines | F5 | L459-466 | **no arm, no handler** | MISSING T9 |
| toggle_block_status | F6 | L455-457 | `hud_fragment.gd` | OK |
| player_list / chat / chat_history_* / chat_scroll / chat_mode | Tab/Enter/Up/Down/scroll/Tab | `ChatFragment.java:65` (`net.active()` only) | none | DEFER: singleplayer chat key is inert in Java too |
| console | F8 | `ConsoleFragment.java:43` (setting gated) | `dispatch_action` gated | OK |
| debug_hitboxes / performance_metrics | unset | debug | none | DEFER |

### 1.2 Non-binding desktop behaviour

| Java | Port | Status / task |
|---|---|---|
| `Tile.relativeTo(x,y,cx,cy)` (`J/world/Tile.java:95-101`) | `core/input/placement.rs:118` `relative` | **WRONG: y flipped** (`(0,-1)` -> 1) and non-adjacent -> 0 (Java -1). A conveyor dragged up points down. T1 |
| `InputHandler.iterateLine` base rotation `((int)((angle+45)/90)) % 4` (IH:2465-2469) | `line.rs:146-152` uses `relative(start,end)` | WRONG for non-adjacent start/end. T1 |
| `updateLine` sets `rotation = l.rotation` (IH:1946) | `client_input.rs:182` never writes `self.rotation` | MISSING T1 |
| Line-angle snap before the wheel step (DI:762-768, then L837) | `scroll()` `desktop.rs:2302-2324`: snap overwrites the step; `atan2` range `(-180,180]` | WRONG T1 |
| Block line flags `conveyorPlacement`, `allowDiagonal`, `swapDiagonalPlacement`, `ignoreLineRotation`, `allowRectanglePlacement` | `line_block_for` `desktop.rs:2120` hard-codes them | WRONG T2 |
| `PlacementWorld.valid_place/build_rotation/is_chained` (`Build.validPlace`, `ChainedBuilding`) | `HostWorld` `desktop.rs:132-175`: bounds only, defaults | WRONG T2 |
| `updateLine` `getReplacement` + `handlePlacementLine`, `changePlacementPath` | `update_line` uses `NoLineHooks`, plain plans | MISSING T3 (PAR-0098 remainder) |
| `Placement.calculateBridges` `plan.placeable(team)` | `placement.rs:487` `plan.placeable()` (block != air) | WRONG T3 |
| `tileTapped` (IH:1973-2030): `interactable` gate, empty tap hides config+inventory, inventory only when `items.total() > 0` and block `synthetic` | `tile_tapped` `desktop.rs:1828`: no team gate, no hide on empty tap | WRONG T4 |
| `checkConfigTap` (IH:1968-1970) tap-to-link | none | H02 owns (§6) |
| `tryRepairDerelict` (IH:2070-2081), rebuildArea derelict loop (IH:2331-2341) | none | T11 (needs H02) |
| `selectedUnit()` ControlBlock branch (IH:2148-2163), `selectedControlBuild()` (IH:2165-2171) | `selected_control_build` treats turrets as buildingControlSelect; Router missing | WRONG T8 |
| `commandTap` (IH:1165-1201): enemy building target, `commandBuildings` | core `DesktopController::command_tap` (`core/input/desktop.rs:172`): units only, unit target only | WRONG T6b |
| `selectedUnits.removeAll(...)` each frame (DI:313) | stale ids stay selected | MISSING T6a |
| `buildautopause` (DI:591-594) | none | MISSING T4 |
| `allowDeposit = alwaysAllowDeposit \|\| !rules.onlyDepositCore` (BuildingComp:745) | `player_items.rs:159` "not ported" — **false**: `only_deposit_core` exists (`core/io/json/rules.rs:580`, set for Erekir in `core/game/planet.rs:403`) | T12 |
| Camera follow `lerpDelta(target, smoothcamera ? 0.08 : 1)` (DI:289) | always smooth | T10 |
| Desktop has no edge panning (edge pan is `MobileInput`) | `camera.rs:147-161` edge-pans whenever the player is absent | WRONG T10 |
| `SetUnitCommand` never moves units (IH:411-444) | `apply_live_unit_command` treats wire `(0,0)` as a move to tile (0,0) | WRONG T6b |
| Synthetic GUI clicks (bridge `godot_input`) | did not reach Controls (G07 T13) | T13 |

### 1.3 Round-G "done" claims spot-checked on `ffbe84dd` (10 checked)

| Claim (G08 summary) | Verdict |
|---|---|
| T1 shooting/aim/facing/boost | TRUE: `core/ai/types/player.rs:112-211` sets mount aim/shoot, `core.boosting`; host uploads `player_control` (`sim_host.rs:415`) |
| T2a item transfer on the real stack | TRUE (`core/sim/player_items.rs`); but its comment "onlyDepositCore not ported" is FALSE -> T12 |
| T3 hotkey gating in game | TRUE (`handle` L478-492, tests `bindings_do_not_fire_outside_game`); F11 on the menu dispatches `"fullscreen"` which has no arm -> T9 |
| T4 even-size offset | TRUE (`core/input/cursor.rs:101` `placement_tile`, used by `placement_cursor_tile`) |
| T6b schematic copy | TRUE in code (`finish_schematic_copy`, `copy_schematic_plans`); paste readback re-verified in V1 |
| T7 rebuild select | TRUE (`Sim::queue_team_rebuild` `core/sim/builder.rs:947`) |
| T9 wheel routing | PARTLY FALSE: `route_scroll` is right, but the line-angle snap (a) discards the wheel step and (b) maps downward drags to rotation 0 (`atan2` negative) -> T1 |
| T13 axis rebind | TRUE (`rebind_axis`, `keybind_dialog.gd` two-step capture + wheel) |
| T12 Ctrl+click building control | FALSE: turrets/build turrets emit `BuildingControlSelect` (Java: `ControlBlock` -> `unitControl(block unit)`; `buildingControlSelect` is for cores/payload blocks); Router missing -> T8 |
| "select-plan flush inserts at `min(1,size)` in Java" (deviation note) | FALSE: `flushPlans` (IH:1823-1832) calls `addBuild(copy)` = tail (`BuilderComp.java:272-299`); the port's tail insert already matches. No task. |

### 1.4 Done — do not redo

Binding registry; WASD player movement (PAR-0044); click mining (PAR-0045);
shooting/aim/facing/boost upload; item withdraw/deposit/drop and tap-unit
deposit; game gate for binds, single M/F8 toggle, Esc pause (PAR-0101);
even-size cursor offset; plan configs to finished blocks; schematic copy /
flip / rotate / paste / cut (PAR-0099); `use_schematic` seam; rebuild
select + area removal + ghost clear; wheel `route_scroll`; axis rebind
(PAR-0102); Ctrl+click unit possession + V respawn input path; payload key
hold repeat (code); hint keys (`take_hint_keys`); command-mode objective
seam (`any_unit_has_command`, `command_mode_objective`); C toggle menus
(G03); RTS drag select / right-click move (PAR-0140 verified paths).

### 1.5 Deferred (with reason)

Multiplayer binds (ping, chat, player list); `detach_camera`/
`teleport_cursor`/`debug_hitboxes`/`performance_metrics` (unset by default);
plan-editing gestures (drag an unstarted plan `splan`, click-delete break
plans `deleting`, Ctrl plan config, Shift `flushPlansReverse`); cursor
shapes; full-map screenshot; `Call.tileTap`/`TapEvent` (no Java consumer in
core: `grep -rln TapEvent J/` lists only `EventType` and `InputHandler`).

---

## 2. Tasks (one commit each, in this order)

Every task: build once (`CARGO_BUILD_JOBS=6 flock /tmp/opencode/mg-build.lock tools/build.sh`),
restart (`tools/session-play.sh N` or `--import` when a task needs the
profile), enter Ground Zero (H-environment §4.1, the Go-button variant),
and after every in-engine step:
`grep -c "panic" $(ls -t $U/logs/* | head -1)` must print `0`.
Evidence dir: `runs/20261014-s<N>-H08-T<k>/` (PNG + text of eval outputs).

---

### T1. Line placement rotation: `relativeTo` y-flip, angle base rotation, rotation carry-over, wheel snap

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (+ a new latch bug). push_event drag (140,60)->(140,64) with `conveyor`: preview and queued plans are 5 conveyors with rotation **3** (Java 1); built belts read `building_debug(140,62).rotation` 3 (`runs/20261014-s3-verify/H08-T1-vertical-drag.png`). Downward drag (145,64)->(145,60): all rotation 1 (Java 3). NEW: `override_line_rotation` is never reset (set at `desktop.rs:1113/:2322` only), so after one wheel step every later drag uses the plain rotation for all steps — FIXED in place (Port today + change 5 made mandatory). CORRECTED the second check: in Java a wheel during a drag gives ALL steps `rotation` (IH:2486-2500), not "last step only"; use an UPWARD drag (expect all 2). Java leg: upward drag preview arrow points up (`runs/20261014-s3-verify/java-06-drag-up.png`); the wheel during a drag zoomed the Java camera instead of rotating (pynput scroll, profile/pause state unclear) — Java not observed for the wheel.

**Symptom / acceptance.** Dragging a conveyor line straight up places
conveyors that point DOWN (rotation 3); Java points them up (rotation 1).
A long non-adjacent drag gets base rotation 0 regardless of direction.
After a drag, the next single placement keeps the old rotation (Java keeps
the last line rotation). Pressing the wheel during a downward drag snaps
the line to east and loses the wheel step.
Acceptance: tests below; in-engine a conveyor drag (140,60)->(140,64) gives
5 plans with rotation 1.

**Java.**
* `J/world/Tile.java:95-101`:
  ```java
  public static byte relativeTo(int x, int y, int cx, int cy){
      if(x == cx && y == cy - 1) return 1;
      if(x == cx && y == cy + 1) return 3;
      if(x == cx - 1 && y == cy) return 0;
      if(x == cx + 1 && y == cy) return 2;
      return -1;
  }
  ```
  i.e. `relativeTo(a, b)`: b one tile EAST of a -> 0, NORTH (+y) -> 1,
  WEST -> 2, SOUTH -> 3, otherwise -1. (Same as port `D4` in
  `core/input/placement.rs:23`: index 1 = `(0, 1)`.)
* `InputHandler.iterateLine` (IH:2463-2490):
  ```java
  float angle = Angles.angle(startX, startY, endX, endY);   // atan2 degrees, +360 when negative: [0,360)
  int baseRotation = rotation;
  if(!overrideLineRotation || diagonal){
      baseRotation = (startX == endX && startY == endY) ? rotation : ((int)((angle + 45) / 90f)) % 4;
  }
  ... int result = baseRotation;
      if(next != null) result = Tile.relativeTo(point.x, point.y, next.x, next.y);
      else if(endRotation != -1) result = endRotation;
      else if(block.conveyorPlacement && i > 0) result = Tile.relativeTo(prev..., point...);
      if(result != -1) line.rotation = result;
  ```
* `InputHandler.updateLine` (IH:1943-1950): `iterateLine(..., l -> { rotation = l.rotation; ... })` — the input rotation follows each line step (ends as the last step's rotation).
* `DesktopInput.pollInputPlayer` L762-768 runs BEFORE L836-849:
  ```java
  if(mode == placing && block != null){
      if(!overrideLineRotation && !Core.input.keyDown(Binding.diagonalPlacement) && (selectX != cursorX || selectY != cursorY) && ((int)Core.input.axisTap(Binding.rotate) != 0)){
          rotation = ((int)((Angles.angle(selectX, selectY, cursorX, cursorY) + 45) / 90f)) % 4;
          overrideLineRotation = true;
      }
  }else{ overrideLineRotation = false; }
  ...
  if(!Core.input.keyDown(Binding.diagonalPlacement) && Math.abs((int)Core.input.axisTap(Binding.rotate)) > 0){
      rotation = Mathf.mod(rotation + (int)Core.input.axisTap(Binding.rotate), 4);   // L837
      ... if(isPlacing() && mode == placing) updateLine(selectX, selectY);
  ```
  So the wheel during a fresh drag = snap to the drag angle, THEN add the wheel step.
* `Angles.angle(x1,y1,x2,y2)` = `atan2(y2-y1, x2-x1)` in degrees, `+360` if `< 0` (ARC `arc/math/Angles.java`).

**Port today.**
* `core/input/placement.rs:118-126` `relative`: `(1,0)=>0, (0,-1)=>1, (-1,0)=>2, (0,1)=>3, _=>0` (y flipped, no -1). Callers: `placement.rs:292` (`upgrade`/chain check), `placement.rs:403` `is_side_place`, `line.rs:150,184,188`.
* `core/input/line.rs:146-152` base rotation `relative(start, end)`.
* `core/input/client_input.rs:182-213` `update_line` never writes `self.rotation`.
* `gdext/input/desktop.rs:2302-2324` (`scroll`): sets `rotation += step` first, then (if not overridden) overwrites it with `(((angle + 45.0) / 90.0) as i32).rem_euclid(4)` where `angle = dy.atan2(dx).to_degrees()` in `(-180, 180]`. Downward drag (-90°) -> `(-45/90) as i32 = 0` -> rotation 0 (Java 3).
* `desktop.rs:1110` `rotate_placement_step` (key `rotate` binding path) has no snap at all, and
  sets `override_line_rotation = true` unconditionally (`:1113`).
* `override_line_rotation` is NEVER reset to false anywhere (only set at `desktop.rs:1113` and
  `:2322`; verified in-engine 2026-10-14): after the first wheel/rotate during a drag, every
  later drag in the session ignores its direction and uses the plain `rotation` for all steps
  (an upward drag then gave rotation 0, later 1, for all 5 plans). Java resets it every frame
  while not placing (DI:762-768 `else{ overrideLineRotation = false; }`).

**Changes.**
1. `core/input/placement.rs` `relative`:
   ```rust
   /// `Tile.relativeTo(x, y, cx, cy)` (`Tile.java:95-101`): direction index of
   /// `b` seen from `a` (`0` east, `1` north/+y, `2` west, `3` south), `-1`
   /// when `b` is not a 4-neighbour.
   pub fn relative(a: TilePos, b: TilePos) -> i32 {
       match (b.x() as i32 - a.x() as i32, b.y() as i32 - a.y() as i32) {
           (1, 0) => 0,
           (0, 1) => 1,
           (-1, 0) => 2,
           (0, -1) => 3,
           _ => -1,
       }
   }
   ```
   Check both other callers: `placement.rs:292` compares two `relative`
   values (still fine with -1); `is_side_place` (`:403`) uses
   `pos_mod(relative(..) - rotation, 2) == 1` — Java `Placement.isSidePlace`
   uses `Tile.relativeTo` the same way (open `J/input/Placement.java`, grep
   `isSidePlace`, confirm, keep).
2. `core/input/placement.rs`: add
   ```rust
   /// `Angles.angle(x1, y1, x2, y2)` in degrees, `[0, 360)`.
   pub fn angle_deg(x1: i32, y1: i32, x2: i32, y2: i32) -> f32 {
       let a = ((y2 - y1) as f32).atan2((x2 - x1) as f32).to_degrees();
       if a < 0.0 { a + 360.0 } else { a }
   }
   /// `((int)((angle + 45) / 90f)) % 4` (IH:2467, DI:764).
   pub fn line_angle_rotation(x1: i32, y1: i32, x2: i32, y2: i32) -> u8 {
       (((angle_deg(x1, y1, x2, y2) + 45.0) / 90.0) as i32 % 4) as u8
   }
   ```
3. `core/input/line.rs` `iterate_line`:
   * base rotation: `if start == end { params.rotation } else { line_angle_rotation(start.x() as i32, start.y() as i32, end.x() as i32, end.y() as i32) }`.
   * per-point resolution: `relative(..)` now returns `i32`; build `resolved: Option<u8>` only when the value is `>= 0` (Java `if(result != -1)`), i.e. `let r = relative(*point, next); if r >= 0 { Some(r as u8) } else { None }` for each branch (next / prev); `end_rotation` stays as is.
4. `core/input/client_input.rs` `update_line`: after `iterate_line`, `if let Some(last) = self.line.last() { self.rotation = last.rotation; }` (Java writes `rotation` per step, so the final value is the last step's). Do NOT change `preview.rotation` semantics beyond using the updated `self.rotation`.
   Check the mobile path (`core/input/mobile.rs`) still compiles and its tests pass; Java `MobileInput` uses the same `updateLine`.
5. `gdext/input/desktop.rs` `scroll()` rotation branch: replace the block
   from `self.controller.state.rotation = (... + step)` down to the end of
   the `if placing {..}` with (Java order: snap, then step):
   ```rust
   if route.rotate_selection {
       let drag = self.drag.filter(|drag| drag.kind == DragKind::Place);
       if placing
           && let Some(drag) = drag
           && !self.controller.state.override_line_rotation
           && !diagonal_held
           && drag.start != drag.last
       {
           self.controller.state.rotation = mind_core::input::placement::line_angle_rotation(
               drag.start.0, drag.start.1, drag.last.0, drag.last.1);
           self.controller.state.override_line_rotation = true;
       }
       self.controller.state.rotation =
           (self.controller.state.rotation as i32 + step).rem_euclid(4) as u8;
       if placing && let Some(drag) = drag {
           self.update_place_line(bindings, drag.start, drag.last);
       } else if !placing && !self.controller.state.select_plans.is_empty() {
           self.rotate_select_plans(step);
       }
   }
   ```
   Reset `override_line_rotation = false` whenever no Place drag is active
   (Java DI:762-768 `else overrideLineRotation = false` when not placing). This is
   MANDATORY: today nothing ever resets it (verified 2026-10-14). Add
   `self.controller.state.override_line_rotation = false;` at the end of
   `commit_place`, in the drag-cancel paths (right-click/deselect while dragging) and when the
   selected block becomes `None`; add a desktop test that a second drag after a wheel step
   gets per-step rotations again.
6. Same snap in `rotate_placement_step` (`desktop.rs:1110`, used by a
   non-wheel `rotate` binding): before adding `step`, apply the identical
   snap block (only when a Place drag is active, not overridden, diagonal
   not held, start != last).

**Tests.**
* `core/input/placement.rs` tests: `relative_matches_tile_relative_to`:
  `relative((5,5),(6,5)) == 0`, `((5,5),(5,6)) == 1`, `((5,5),(4,5)) == 2`,
  `((5,5),(5,4)) == 3`, `((5,5),(7,5)) == -1`, `((5,5),(6,6)) == -1`.
  `line_angle_rotation_quadrants`: `(0,0,5,0)->0`, `(0,0,0,5)->1`,
  `(0,0,-5,0)->2`, `(0,0,0,-5)->3`, `(0,0,5,-1)->0` (angle 348.7: (393.7/90)=4 -> 0), `(0,0,-1,-5)->3` (258.7 -> 3).
* `core/input/line.rs` tests (use the existing test world in the file):
  `vertical_conveyor_drag_points_up`: block with `conveyor_placement: true`,
  rotation 0, drag (5,5)->(5,8), `override_line_rotation false` -> 4 steps,
  every `rotation == 1`. `downward_drag_points_down`: (5,8)->(5,5) -> all 3.
  `single_tile_keeps_rotation`: start == end, rotation 2 -> one step rotation 2.
* `core/input/client_input.rs` test `update_line_carries_last_rotation`:
  after a drag (5,5)->(5,8) `state.rotation == 1`.
* Any pre-existing test that encoded the old y-flipped value: fix its
  expectation and cite `Tile.java:95-101` in the commit body (list each).

**Verify in-engine.** Ground Zero, `$H.debug_add_core_items("copper", 500)`,
`$I.select_block_by_name("conveyor")`. Center the camera on (140,62), read
`tile_to_screen(140,60)` and `(140,64)`. push_event: MouseMove(140,60) ->
MouseButton left down -> MouseMove(140,64) -> MouseButton left up.
`$G --eval "return $H.player_build_plans_json()"` -> 5 conveyor plans at
x=140, y=60..64, all `rotation 1`. Screenshot after ~3 s (built belts
point up). Second check (CORRECTED 2026-10-14): drag UP (145,60)->(145,64) and, while held, push
`{"t":"Scroll","x":0,"y":1}` -> preview (`$I.placement_preview_json()`)
rotations are ALL `(1 + 1) % 4 = 2`: Java snaps to the drag angle (1), adds the wheel
step, sets `overrideLineRotation`, and `iterateLine` then gives EVERY step `line.rotation =
rotation` (IH:2486-2500: the per-point `relativeTo` branch only runs when
`!overrideLineRotation || diagonal`). Release, then do a third plain drag (145,66)->(145,70)
without the wheel -> all rotation 1 again (override reset). Today: the wheel gives all 1, and
every later drag keeps all steps at the latched rotation. (A downward drag + wheel gives all 0
in both Java and the port today, so it cannot show the bug.) Run this check on a fresh
`--play` (the latch survives within a session).

**Pitfalls.** `relative` is `pub` and used by mobile code; run the whole
`input::` test filter. `line.rotation` is `u8`; never cast -1 into it.
Keep the `scroll()` zoom branch unchanged.

**Commit.** `Gameplay H08 T1: line placement uses Java relativeTo, angle base rotation and wheel snap order`

---

### T2. Line block flags from content; `HostWorld` validity, rotation and chains

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (code + in-engine). `line_block_for` (`desktop.rs:2120-2137`) hard-codes the flags. Ctrl-drag `titanium-conveyor` over an L-shaped conveyor chain (132,72)->(134,74) gives a zig-zag pathfind (132,72)r0,(133,72)r3,(133,73)r0,(134,73)r3,(134,74)r3 instead of following the chain (upgradeLine). CORRECTED the recipe: the old straight 3-tile row gives the same plans with either path, so it could not show the bug.

**Symptom / acceptance.** Conduits, plastanium/stack conveyors and overflow
ducts do not get conveyor path placement; bridges and beam nodes accept
diagonal drags; power nodes/illuminators do not swap the diagonal key;
turrets and beam drills are rotated along the line; upgrading a conveyor
line by dragging over it with titanium conveyor never uses `upgradeLine`
(`HostWorld.is_chained` is always false), and the last plan never adopts the
end conveyor's rotation (`build_rotation` is `None`).

**Java (flags set in constructors).**
| Flag | Classes (file:line) |
|---|---|
| `conveyorPlacement = true` | `Conveyor.java:46`, `Conduit.java:52`, `Duct.java:43`, `OverflowDuct.java:27`, `StackConveyor.java:53` |
| `allowDiagonal = false` | `ItemBridge.java:59` (incl. phase/bridge-conduit subclasses), `DirectionBridge.java:43`, `BeamNode.java:44` |
| `swapDiagonalPlacement = true` | `PowerNode.java:49`, `LightBlock.java:33` |
| `ignoreLineRotation = true` | `Turret.java:172`, `BeamDrill.java:72`, `WallCrafter.java:60` |
| `allowRectanglePlacement = true` | `TileableLogicDisplay.java:36` (+ environment/editor blocks) |

`ChainedBuilding` = conveyor-family builds (`Conveyor`, `Duct`, `StackConveyor`
builds implement it; grep `implements ChainedBuilding` in `J/world/blocks`
and use exactly that list). `iterateLine` (IH:2441-2461) uses
`world.build(x,y) instanceof ChainedBuilding` and `end.rotation`.

**Port today.** `desktop.rs:2120-2137` `line_block_for`: `allow_diagonal:
true`, `conveyor_placement` only for `Conveyor | ArmoredConveyor | Duct`,
others false. `HostWorld` (`desktop.rs:132-175`) `valid_place` = in bounds;
`build_rotation`/`is_chained`/`chained_next` use the trait defaults
(`core/input/placement.rs:46-62`).

**Changes.**
1. `core/input/line.rs`: add
   `pub fn line_block_from_def(id: BlockId, def: &crate::content::BlockDef) -> LineBlock`
   that fills the flags from `def.kind` per the table (map Java classes to
   `BlockKind` variants: grep `enum BlockKind` in
   `core/content/registries/blocks/mod.rs`; for each Java class pick the
   variant(s) whose name matches; ItemBridge family = `ItemBridge`,
   `BufferedItemBridge`, `LiquidBridge` (whatever variants exist for
   bridge-conveyor/phase-conveyor/bridge-conduit/phase-conduit — confirm
   with `content_list` names and `def.kind`); turrets = every `*Turret`
   kind incl. `BuildTurret`? NO — `BuildTurret` extends `BaseTurret`, not
   `Turret`; only kinds of `Turret` subclasses). Unknown -> false. Put the
   Java citation per arm.
2. `desktop.rs` `line_block_for`: `Some(mind_core::input::line::line_block_from_def(block, def))`.
3. `HostWorld` (`desktop.rs`): add a field `host: &'a MindSimHost`
   (constructed from the same `guard` where `HostWorld` is built — both
   sites: `update_place_line` and `right_release`; `right_release` only
   needs `block_at`, pass the guard too) and implement:
   * `valid_place(block, x, y, rotation)` -> `self.host.can_place_player(x, y, block, rotation)` (new, change 4).
   * `build_rotation(x, y)` -> rotation of the building at the tile
     (`self.host.building_rotation(x, y)`, new pub non-func: reads the
     building entity's rotation the same way `building_debug` does — grep
     `"rotation"` in `fn building_debug` and reuse the read).
   * `is_chained(x, y)` -> block kind at the tile is a ChainedBuilding kind
     (use a `pub fn is_chained_kind(kind: BlockKind) -> bool` placed in
     `core/input/line.rs` next to change 1).
   * `chained_next(x, y)` -> for a chained build: the tile in front
     (`(x + D4X[rot], y + D4Y[rot])`, `core/world/blocks/autotiler.rs:28-30`)
     if that tile is also chained, else `None` (`Conveyor.next()` = `front()`
     when it is a conveyor; check `J/world/blocks/distribution/Conveyor.java`
     `next()` and mirror it).
4. `core/sim/builder.rs` `impl Sim`: `pub fn can_place_player(&self, x: i32, y: i32, block: BlockId, rotation: u8) -> bool` = the exact `runtime.can_place(&self.ecs.0, &self.grid, x as i16, y as i16, block, rotation, self.live_default_team())` call `queue_player_place_config` makes (`builder.rs:927`; bounds-check `x`,`y` with `i16::try_from` first). `gdext/sim_host.rs`: `pub fn can_place_player(&self, x: i32, y: i32, block: BlockId, rotation: u8) -> bool` forwarding (not `#[func]`), plus `pub fn building_rotation(&self, x: i32, y: i32) -> Option<u8>`.

**Tests.**
* `core/input/line.rs` `line_flags_follow_java`: with `create_base_content()` (grep how other tests build a registry): `conduit.conveyor_placement`, `plastanium-conveyor.conveyor_placement`, `!bridge-conveyor.allow_diagonal`, `!beam-node.allow_diagonal`, `power-node.swap_diagonal_placement`, `duo.ignore_line_rotation`, `!conveyor.ignore_line_rotation`.
* `core/sim/builder.rs` `can_place_player_matches_queue`: `sim_with_player()`
  (grep it) -> `can_place_player(14, 10, copper_wall, 0)` true; on the core
  tile (10,10) false.

**Verify in-engine.** (CORRECTED 2026-10-14: a straight row cannot distinguish
`upgradeLine` from `pathfindLine`; use an L-shaped chain.) Ground Zero:
`place_block_rot` `conveyor` (132,72) rot 0, (133,72) rot 0, (134,72) rot 1, (134,73) rot 1,
(134,74) rot 1; `debug_add_core_items("titanium",200)`; select `titanium-conveyor`
(`$I.select_block_by_name`); `$C.set_process(false); $C.center_on_tile(136,70)`; push
`KeyDown controlLeft`, drag (132,72)->(134,74), read `$I.placement_preview_json()`, release,
`KeyUp controlLeft`. After the fix the preview follows the chain: (132,72) r0, (133,72) r0,
(134,72) r1, (134,73) r1, (134,74) r1. Today (verified) it is a zig-zag pathfind
(132,72) r0, (133,72) r3, (133,73) r0, (134,73) r3, (134,74) r3. Screenshot.

**Pitfalls.** `HostWorld` must stay read-only: only `bind()` the host (the
callers already hold `host.bind()` — pass `&*guard`). `can_place` may be
slower than bounds; previews call it per point (<= 256), fine.

**Commit.** `Gameplay H08 T2: line placement flags from content; HostWorld validPlace, rotation and chains`

---

### T3. Line placement: junction replacement, auto-bridges, node/bridge spacing (PAR-0098 remainder)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (junction). With a built vertical conveyor line at x=140 (rot 3), a horizontal drag (138,62)->(142,62) queues a plain `conveyor` at (140,62) (no `junction`), and building it overwrote the crossing belt (rotation changed). Bridges over walls / node spacing: code-confirmed (`update_line` uses `NoLineHooks`); not run. FIXED in place: the power-node drag (150,66)->(170,66) is static wall → (120,66)->(140,66).

**Symptom / acceptance.** Dragging a conveyor across a perpendicular
conveyor overwrites it instead of placing a junction; dragging a conveyor
over a wall never bridges; power-node and bridge drags place one per tile.
Acceptance: tests below + in-engine junction.

**Java.**
* `InputHandler.updateLine` (IH:1943-1961):
  ```java
  linePlans.clear();
  iterateLine(x1, y1, x2, y2, l -> { rotation = l.rotation; var plan = new BuildPlan(l.x, l.y, l.rotation, block, block.nextConfig()); plan.animScale = 1f; linePlans.add(plan); });
  if(Core.settings.getBool("blockreplace")){
      linePlans.each(plan -> { Block replace = plan.block.getReplacement(plan, linePlans); if(replace.unlockedNow()) plan.block = replace; });
      block.handlePlacementLine(linePlans);
  }
  ```
* `Conveyor.getReplacement` (`J/world/blocks/distribution/Conveyor.java:112-121`):
  ```java
  if(junctionReplacement == null) return this;
  Boolf<Point2> cont = p -> plans.contains(o -> o.x == req.x + p.x && o.y == req.y + p.y && (req.block instanceof Conveyor || req.block instanceof Junction));
  return cont.get(Geometry.d4(req.rotation)) && cont.get(Geometry.d4(req.rotation - 2)) && req.tile() != null &&
      req.tile().block() instanceof Conveyor && Mathf.mod(req.tile().build.rotation - req.rotation, 2) == 1 ? junctionReplacement : this;
  ```
  `Conveyor.init` (L67-68): `junctionReplacement` default `Blocks.junction`; `bridgeReplacement` default `Blocks.itemBridge` (= `bridge-conveyor`).
  `Conduit.getReplacement` / `Duct.getReplacement`: open `J/world/blocks/liquid/Conduit.java` and `J/world/blocks/distribution/Duct.java`, grep `getReplacement`, transcribe the same way (conduit -> `liquid-junction`, bridge `bridge-conduit`; duct: no junction, bridge `duct-bridge`). Paste the exact lines into your commit body.
* `Conveyor.handlePlacementLine` (Conveyor.java:94-99):
  ```java
  if(bridgeReplacement == null) return;
  boolean hasJuntionReplacement = junctionReplacement != null;
  if(bridgeReplacement instanceof DuctBridge bridge) Placement.calculateBridges(plans, bridge, hasJuntionReplacement, b -> b instanceof Duct || b instanceof Conveyor);
  if(bridgeReplacement instanceof ItemBridge bridge) Placement.calculateBridges(plans, bridge, hasJuntionReplacement, b -> b instanceof Conveyor);
  ```
  Conduit/Duct: grep `handlePlacementLine` in their files and transcribe.
* `ItemBridge.handlePlacementLine` (ItemBridge.java:195-203): for consecutive plans `if(positionsValid(cur.x, cur.y, next.x, next.y)) cur.config = new Point2(next.x - cur.x, next.y - cur.y);`.
* `ItemBridge.changePlacementPath` (ItemBridge.java:206-208): `Placement.calculateNodes(points, this, rotation, (point, other) -> Math.max(Math.abs(point.x - other.x), Math.abs(point.y - other.y)) <= range);`
* `PowerNode.changePlacementPath` (PowerNode.java:179-181): `calculateNodes(points, this, rotation, (point, other) -> overlaps(world.tile(point), world.tile(other)))` where `overlaps(src, other)` = circle at `(src.worldx()+offset, src.worldy()+offset)` radius `laserRange*8` overlaps the square `size*8` centred at `(other.worldx()+offset, ...)` (PowerNode.java, grep `protected boolean overlaps(`).
* `BeamNode` / `DirectionBridge` / `LightBlock`: grep `changePlacementPath` in their files and transcribe (BeamNode only when `!diagonal`).
* Ranges: `bridge-conveyor` 4, `phase-conveyor` 12, `bridge-conduit` 4, `phase-conduit` 12, `duct-bridge` 4, `power-node` laserRange 6, `power-node-large` 15, `surge-tower` 40, `beam-node` 10, `beam-tower` 23 (confirm each against the port content: `grep -n "range\|laser_range" core/content/registries/blocks/*.rs` for these names; Java wins).

**Port today.** `client_input.rs:182` `update_line` -> `iterate_line(.., &NoLineHooks, ..)`, plans with `ConfigValue::None`, no replacement. `core/input/placement.rs`: `calculate_nodes` (L410), `calculate_bridges` (L468) + `RelativeBridgePlacer` / `DirectionBridgePlacer` exist, unused live. **Bug**: `calculate_bridges` L487 `let accessible = plan.placeable() || tile_block == plan.block;` — Java `Placement.calculateBridges` uses `plan.placeable(player.team())` = `Build.validPlace(...)` (open `J/input/Placement.java`, grep `calculateBridges`, paste the line). `circle_rect_overlap` exists (`core/world/blocks/power/nodes.rs:57`). Setting `blockreplace` row default true (`core/ui/settings.rs`).

**Changes.**
1. Fix `placement.rs:487`: `let accessible = world.valid_place(plan.block, plan.x, plan.y, plan.rotation) || tile_block == plan.block;` (after T2 `valid_place` is real). Re-run `placement` tests; fix expectations that encoded the old predicate, citing the Java line.
2. New `core/input/line_rules.rs` (header lines per H-environment §3; `pub mod line_rules;` in `core/input/mod.rs`):
   ```rust
   pub enum NodeRule { Chebyshev(i32), PowerLaser { range_tiles: f32, size: i32, offset: f32 }, Beam { range: i32, size: i32 }, Light { radius: f32 } }
   pub struct LineRules {
       pub node: Option<NodeRule>,
       pub junction: Option<BlockId>,
       pub bridge: Option<(BlockId, i32, bool /*direction bridge*/)>,
       pub avoid: fn(BlockKind) -> bool,
       pub link_bridge_range: Option<i32>,   // ItemBridge.handlePlacementLine
   }
   pub fn line_rules_for(content: &ContentRegistry, block: BlockId) -> LineRules
   pub struct NodeHooks<'a> { pub rule: NodeRule, pub world: &'a dyn PlacementWorld, pub block: BlockId }
   impl LineHooks for NodeHooks<'_> { fn change_placement_path(&self, points, rotation, diagonal) { /* calculate_nodes with the rule; Beam: skip when diagonal */ } }
   ```
3. `client_input.rs`: `pub fn update_line_with(&mut self, world, block_meta, start, end, params, hooks: &dyn LineHooks, rules: Option<&LineRules>)`; `update_line` delegates with `&NoLineHooks, None`. After building `line_plans` (and T1's rotation carry), when `rules` is Some: (a) junction replacement per plan using the Java rule (existing tile block via `world.block_at`, its rotation via `world.build_rotation`, "is conveyor" via the kind set of the line block's family); (b) `calculate_bridges` with the rule's placer and `avoid`; (c) for bridge blocks, Point2 link configs per consecutive pair when `max(|dx|,|dy|) <= range` and same row/column (`positionsValid`).
4. `desktop.rs` `update_place_line`: read the `blockreplace` setting once in `MindInput::bootstrap` like `read_command_mode_hold` (`mod.rs:322`) into a new bridge field `block_replace: bool` (default true); when true call `update_line_with(.., &NodeHooks{..} or &NoLineHooks, Some(&line_rules_for(content, block)))`, else `update_line_with(.., &NoLineHooks, None)`.
5. `commit_place` already routes plans with a config through `queue_player_place_config` and uses each plan's own block — confirm by reading (`desktop.rs:2140-2217`); no change unless it does not.

**Tests** (`line_rules.rs`, reuse the grid test world of `placement.rs` tests; for `valid_place` give the test world a set of blocked tiles):
* `conveyor_line_crossing_conveyor_becomes_junction`: existing conveyor at (5,5) rotation 1, drag conveyor (3,5)->(7,5) -> plan (5,5) is `junction`, others conveyor.
* `conveyor_line_over_wall_bridges`: wall at (5,5) (not placeable), drag (3,5)->(8,5): derive the expected plans by hand from `calculateBridges` (costs: read them from `J/input/Placement.java`), write the derivation in the test comment; expect a `bridge-conveyor` pair over the wall with a `Point2` config on the first.
* `power_node_line_spacing`: power-node drag (0,0)->(20,0) -> x = `[0, 6, 12, 18, 20]` (6 tiles: closest square distance 44 < 48; 7 tiles: 52 > 48).
* `bridge_line_spacing`: bridge-conveyor (0,0)->(10,0) -> `[0, 4, 8, 10]`, configs `(4,0),(4,0),(2,0)`.
* `blockreplace_off_keeps_plain_line`.

**Verify in-engine.** Ground Zero: `$H.place_block_rot(140,60,"conveyor",1)`;
select `conveyor`; drag (138,60)->(142,60) -> `player_build_plans_json()`
has `junction` at (140,60). `$H.place_block(146,62,"copper-wall")`; drag
conveyor (144,62)->(149,62) -> bridge-conveyor plans around x=146. Power
node drag (120,66)->(140,66) -> nodes at x = 120, 126, 132, 138, 140 (the original
(150,66)->(170,66) is static wall/outside `limit_area`). Screenshot each.

**Pitfalls.** `calculate_bridges` takes `&mut SmallVec<[PlanCopy; 128]>`;
convert. Unlock check (`replace.unlockedNow()`): use content unlock state if
reachable from the bridge (grep `unlocked` in `gdext/sim_host.rs`), else
`true` and note it. The preview JSON already serialises `line_plans`.

**Commit.** `Fixes PAR-0098: line placement inserts junctions, bridges and spaced nodes`
(only if the in-engine junction check passes; else `PAR-0098: ...`)

---

### T4. Tap and deselect gestures: `tileTapped` gates, empty tap hides panels, right-click stops mining, pause/autopause

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (a,b,c; d/e code only). (a) enemy (team 2) sorter staged at (136,64): a click opens its config panel (`block_config.visible` true, title `block.sorter.name`) — `runs/20261014-s3-verify/H08-T4-enemy-sorter-tap.png`. (b) a click on empty ground (136,59) leaves the panel visible. (c) left-click ore (130,48) → `player_mine_tile()` `[130,48]`; right-click on empty ground → still `[130,48]`. FIXED in place: recipe coordinates ((150,60)/(150,62) are walls) and verified `apply_sim_command_json` place field names.

**Symptom / acceptance.** (a) Tapping an ENEMY building opens its config
panel / inventory (Java: only `interactable` = own team or derelict).
(b) Tapping empty ground leaves an open config/inventory panel open (Java
hides both). (c) Right-click while mining (no block, no plans) does not
stop mining (Java clears `mineTile`). (d) Pressing E to resume building
keeps the player shooting. (e) setting `buildautopause` has no effect.

**Java.**
* `InputHandler.tileTapped` (IH:1973-2030) — paste-ready summary:
  ```java
  planConfig.hide();
  if(build == null){ inv.hide(); config.hideConfig(); commandBuildings.clear(); return false; }
  if(build.isCommandable() && commandMode){ consumed = true; }
  else if(build.block.configurable && build.interactable(player.team())){ consumed = true; if(!config.isShown() && build.shouldShowConfigure(player) || ...) { configureSound; config.showConfig(build); } }
  else if(!config.hasConfigMouse()){ if(config.isShown() && config.getSelected().onConfigureBuildTapped(build)){ consumed = true; config.hideConfig(); } if(config.isShown()) consumed = true; }
  if(!consumed && build.interactable(player.team())) build.tapped();
  if(build.interactable(team) && build.block.consumesTap) consumed = true;
  else if(build.interactable(team) && build.block.synthetic() && (!consumed || build.block.allowConfigInventory)){
      if(build.block.hasItems && build.items.total() > 0){ inv.showFor(build); consumed = true; showedInventory = true; } }
  if(!showedInventory) inv.hide();
  return consumed;
  ```
  `interactable(team)` = `state.teams.canInteract(team, this.team)` = same team or derelict.
* DI:608-610: `if(Core.input.keyTap(Binding.deselect) && !ui.minimapfrag.shown() && !isPlacing() && player.unit().plans.isEmpty() && !commandMode){ player.unit().mineTile = null; }`
* DI:671-678: `if(keyTap(pauseBuilding)){ isBuilding = !isBuilding; buildWasAutoPaused = false; if(isBuilding) player.shooting = false; }`
* DI:591-594: `if(Core.settings.getBool("buildautopause") && isBuilding && !player.unit().isBuilding()){ isBuilding = false; buildWasAutoPaused = true; }` and InputHandler resumes when plans appear: open `J/input/InputHandler.java`, grep `buildWasAutoPaused`, transcribe the resume rule into the commit body and implement it.

**Port today.** `tile_tapped` (`desktop.rs:1828-1904`): switch/door/config/
inventory, no team checks; `config_spec` (`:2407`) has no team check;
`block_items_json` (`gdext/sim_host.rs:5349`) returns items for any team.
Empty tile: returns false, panels untouched. `right_press` (`:1906`) never
touches mining. `"pause_building"` arm (`:687`) does not touch `shooting`.
No `buildautopause`.

**Changes** (`desktop.rs`):
1. Add `fn tile_interactable(&self, x, y) -> bool`: `host.bind().building_team(x, y)` is `Some(t)` with `t == self.player_team() || t == 0`.
2. `tile_tapped`: first, if there is no building at the tile (`host.bind().building_team(x, y).is_none()`): push a new `Effect::HidePanels` and return false. Gate the switch, door, config and inventory branches with `tile_interactable` (non-interactable -> skip to the end; Java then hides inventory). Inventory branch additionally requires the block to be `synthetic` (Java `Block.synthetic()` = `!isStatic`-like; grep `fn synthetic` / `synthetic` on `BlockDef`; if absent use `def.kind != Static environment` = "has a building") — and `block_items_json` already requires `total() > 0`. When a branch other than the inventory one returns true, also push `Effect::HideInventory` (Java `if(!showedInventory) inv.hide()`).
3. `Effect::HidePanels` / `Effect::HideInventory` in `drain_effects` (`mod.rs:573`): call `hide_panel()` on `BLOCK_INVENTORY_PATH` (exists, `block_inventory_fragment.gd:109`) and, for `HidePanels`, set `visible = false` on `BLOCK_CONFIG_PATH` (or call its hide method if one exists: grep `func hide` in `ui/fragments/block_config_fragment.gd`).
4. `right_press`: before the `command_mode` check add (Java order: L608 runs before the select/deselect taps):
   ```rust
   if self.controller.state.block.is_none() && !self.controller.state.command_mode
       && self.host.as_ref().is_some_and(|h| h.bind().player_plan_count() == 0)
   {
       if let Some(mut host) = self.host.clone() { host.bind_mut().set_player_mine(None); }
   }
   ```
   (`player_plan_count`: add `pub fn player_plan_count(&self) -> usize` on `MindSimHost` if no equivalent exists — grep `fn player_build_plans` and reuse its source.) Do NOT return: the break drag still starts (Java continues to the break branch).
5. `"pause_building"`: when `is_building` becomes true set `self.shooting = false`.
6. `buildautopause`: read the setting in `bootstrap` (like `read_command_mode_hold`) into `build_autopause: bool`; in `MindInput::process` call a new `bridge.update_build_autopause()` that implements DI:591-594 plus the Java resume rule, using `player_plan_count()`; keep `set_player_building_paused` in sync exactly like the `"pause_building"` arm.

**Tests.** `desktop.rs` tests cannot reach a host; add a pure helper in
`core/input/desktop.rs`: `pub fn deselect_stops_mining(block_selected: bool, command_mode: bool, plans: usize) -> bool` with a test of the 4 cases, and use it in change 4.

**Verify in-engine.** Ground Zero (real player). (CORRECTED 2026-10-14: (150,60)/(150,62)
are static walls.)
(a) Stage an enemy sorter on free ground:
`apply_sim_command_json('{"op":"place","x":136,"y":64,"block":<sorter id>,"rotation":0,"team":2}')`
(field names verified: `op`, `x`, `y`, `block` = numeric id from `content_list("block").find("sorter")`
(264 on `ffbe84dd`), `rotation`, `team`, `player`; the enqueue pre-check uses the default team
but passes on free ground), `step(1)`, `building_team_at(136,64)` -> 2. Deselect the block
(`$I.select_block_by_name("air")` -> input state `block` null), push MouseMove + left
MouseButton down/up at `tile_to_screen(136,64)` ->
`get_node("/root/Spine/Ui/UiRoot/OverlayLayer/block_config").visible == false` (today: `true`,
panel titled `block.sorter.name`). Derelict: `building_team_at(169,167)` -> 0 (scrap walls are
not configurable).
(b) click an own sorter (opens), then click empty ground (e.g. (136,59)) -> panel hidden
(today: stays visible).
(c) left-click copper ore (130,48) -> `$H.player_mine_tile()` = `[130, 48]`; right
click (press+release, no drag) on empty ground -> `player_mine_tile()` = `[]` (today: stays
`[130, 48]`). Read every screen point with `tile_to_screen` right before pushing (the camera
follows the player). Screenshots.

**Pitfalls.** H02 adds the `checkConfigTap`/`onConfigureBuildTapped`
(tap-to-link) call at the top of `left_press`'s non-block branch (§6); do
not move or remove that call if it is already on `main`; if both edit
`tile_tapped`, merge by keeping H02's link branch before your gates.

**Commit.** `Gameplay H08 T4: tileTapped team/consume gates, empty tap hides panels, deselect stops mining, build autopause`

---

### T5. Block hotkeys: digit category/block combos, arrow navigation, middle-click pick, F1 info (PAR-0214)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED with corrections. (a) push `num1`,`num1` → `selected_block()` = `conveyor` (Nth block of the current category; Java `duo`). (b) `num3` → `plastanium-conveyor`, category unchanged (2). `period` (category next) changed the category 2→3 but kept the block. (c) middle-click pick of a placed `graphite-press` already selects it, but the category stays 3 (Java switches to crafting). (d) F1 over a conveyor ALREADY opens `content` (not a symptom); over an own sorter F1 opens the config panel (`dialog_stack()` `[]`) — `runs/20261014-s3-verify/H08-T5-f1-sorter.png`. FIXED in place: (c) Q does not deselect (it is `clear_building`), use `select_block_by_name("air")`; (d) uses a sorter; acceptance (d) text.

**Symptom / acceptance.** `1`..`0` pick the Nth block of the visible
category (Java: 1st digit = category, 2nd digit within 400 ms = block);
arrows do nothing; `,`/`.` change category without selecting its block;
middle-click picks any block (locked, ignoring queued plans, no category
switch); F1 opens the config panel (Java: content info only).
Acceptance: (a) `1`,`1` -> `duo`; (b) `3` with a block selected switches to
distribution and selects its remembered/first block; (c) middle-click a
placed `graphite-press` selects it and shows `crafting`; (d) F1 over a
sorter (nothing selected) opens `content` for `sorter`, not the config panel.

**Java.** `PlacementFragment.updatePick` (PF:132-172) and `gridUpdate`
(PF:177-273) — read them in full now (`sed -n 132,273p J/ui/fragments/PlacementFragment.java`).
Key rules:
* Pick (PF:134): `keyTap(pick) && player.isBuilder() && !scene.hasDialog()`; `build` null when in fog; `tryBlock = ConstructBuild ? c.current : build.block`; `tryConfig = build.block.copyConfig ? build.config() : null`; any non-breaking player plan whose bounds contain the cursor overrides both; accept only `tryBlock.isVisible() && unlocked(tryBlock)`; then `input.block = tryBlock; tryBlock.lastConfig = tryConfig; currentCategory = block.category`.
* gridUpdate runs only `if(!control.input.commandMode && gridUpdate(...))` (PF:745): in command mode digits are control groups (already ported) and pick does not run.
* Digits/arrows/categories exactly as PF:186-258 (directional math: left `(j-1+n)%n`, right `(j+1)%n`, up `j = j>3 ? j-4 : n - n%4 + j; j -= j<n ? 0 : 4`, down `j < n-4 ? j+4 : j%4`; first digit when `blockSelectEnd || timeSince(seqMillis) > 400`: category `Category.all[i]` if non-empty, `if(input.block != null) input.block = getSelectedBlock(cat)`; 2nd digit: `blockSelectSeq = i+1`; 3rd digit: `i += (blockSelectSeq - (i != 9 ? 0 : 1)) * 10; blockSelectEnd = true`; then `blocks = getByCategory(cat); if(i >= size || !unlocked) return; input.block = blocks[i]; selectedBlocks.put(cat, block); seqMillis = now`).
* `Category.all` order: turret, production, distribution, liquid, power, defense, crafting, units, effect, logic (digits 1..0).
* categoryPrev/Next (PF:240-258): cycle skipping empty categories, then `input.block = getSelectedBlock(currentCategory)` (selects even if nothing was selected).
* blockInfo (PF:260-271): hovered unit with `type.unlockedNow()` -> `ui.content.show(unit.type)`; else `display = menuHoverBlock ?? input.block ?? hovering (construct -> current)`; show if unlocked.

**Port today.** `desktop.rs:697-711` digits -> `select_catalog_block(index)`;
`"category_prev"|"category_next"` -> `fire_hotkey` -> `ui/fragments/placement_fragment.gd:583` `_on_hotkey` (cycles `_selected_category`, no block select; arrows ignored); `"block_info"` -> `block_info_hotkey` (`:1134`, config panel first); `"pick"`/middle -> `pick_block` (`:1162`). The fragment holds `_catalog` (from `MindUi.block_catalog_json()`; confirm keys `categories[].ordinal` / `categories[].blocks[].name` with `grep -n "ordinal\|block_catalog_json" ui/fragments/placement_fragment.gd gdext/ui/ui_host.rs`), `_selected_category`, `_select(index)`, `_select_block(host, name)`.

**Changes.**
1. `desktop.rs` `dispatch_action`: outside command mode, digits, `block_select_left/right/up/down` and `category_prev/next` only `fire_hotkey(name)` (names unchanged). Delete `select_catalog_block` if nothing else uses it (`grep -rn select_catalog_block client/`), else leave it unused-free.
2. `placement_fragment.gd` `_on_hotkey(action)`: implement the Java logic with state `_selected_blocks: Dictionary` (category name -> block name), `_block_select_seq := 0`, `_block_select_end := true`, `_block_select_ms := 0` and `Time.get_ticks_msec()`. Category for digit `i` = the `_catalog.categories` entry with `ordinal == i` (absent = empty -> ignore). `getByCategory`/`getUnlockedByCategory` both = the entry's `blocks` (the catalog lists unlocked blocks only; deviation: Java indexes the full category list in the combo — identical with the imported full profile; note it in the commit). `getSelectedBlock(cat)` = `_selected_blocks.get(cat, first block)`. "input.block != null" = `str(host.call("selected_block")) != "air"`. Select = `_select_block(host, name)` + `_selected_blocks[cat] = name`; category change = `_select(index)`. Also record `_selected_blocks[cat] = name` on palette clicks. On `"pick"`: switch `_selected_category` to the entry containing `selected_block()`.
3. `pick_block` per `updatePick`: require `has_player_unit()` and `!self.ui_dialog`; block = construct `current` when the tile is a construct (grep `fn construct_progress` in `sim_host.rs`; add `pub fn construct_current_block(&self, x, y) -> Option<BlockId>` if needed), else the building block; queued non-breaking player plans whose bounds contain `aim_world()` override (add `pub fn player_plan_at_world(&self, wx: f32, wy: f32) -> Option<(BlockId, ConfigValue)>` on `MindSimHost` reading the player's `BuilderPlans`); accept only blocks in the unlocked catalog (`mind_core::ui::campaign::block_catalog()` names — the same list `load_catalog` reads, `mod.rs:305`); config: if `def.copy_config` (grep; default true) store the building config (`read_config`, `core/world/config.rs:154`) or the plan config into `last_config` for that block when `def.save_config`; then `select_block` + `fire_hotkey("pick")`. Do nothing in command mode (T6b routes middle there).
4. `block_info_hotkey`: never open the config panel. Order: (1) nearest selectable unit (any team) within `UNIT_TAP_RADIUS` of `aim_world()` -> `open_dialog("content", {"content": <unit type name>})` (name via host content `unit(type_id).name`); (2) else selected block if any, else block under cursor (construct -> current) -> `open_dialog("content", {"content": name})`. Confirm the `content` dialog ctx key by reading `ui/dialogs/content_info_dialog.gd` (or whatever `dialogs_manifest.json` maps `content` to).

**Tests.** GDScript has no unit harness; Rust: none new. Parse gate (H-environment §2).

**Verify in-engine** (`--import` profile, Ground Zero). Find the fragment:
`$G --eval 'return get_node("/root/Spine/Ui/UiRoot").find_child("placement*", true, false).get_path()'`.
(a) push KeyDown/KeyUp `num1`, then within 400 ms KeyDown/KeyUp `num1` (one `$G` call with 4 `--eval`s) -> `$H.selected_block()` == `"duo"`.
(b) push `num3` -> fragment category name `distribution` and `selected_block()` = its first block (`conveyor`).
(c) `$H.place_block(140,60,"graphite-press")`, deselect with `$I.select_block_by_name("air")` (NOT `q`: Q is `clear_building` = clear the build plans, it does not deselect), MouseMove over the press centre (`world_to_screen(1124,484)`; 2x2 offset +4), MouseButton `middle` down/up -> `selected_block() == "graphite-press"`, category `crafting` (`_selected_category` 6). Today (verified) the pick already selects `graphite-press` but the category stays where it was (3): only the category switch, plans, unlock and config halves are missing.
(d) clear, MouseMove over an own `sorter` (a CONFIGURABLE block), KeyDown `f1` -> `get_node("/root/MindUi").dialog_stack()` ends with `content` and `block_config.visible == false`. (CORRECTED 2026-10-14: over a conveyor F1 already opens `content` today; over a sorter it opens the config panel and `dialog_stack()` is `[]`.)
Screenshots per step.

**Pitfalls.** Digits must not fire while a text field has focus (already
gated). The 400 ms window is wall-clock ms.

**Commit.** `Fixes PAR-0214: block hotkey combos, arrow navigation, pick and F1 as in PlacementFragment`

---

### T6a. RTS keys: stance/command names, cancel orders, select-all gating and filters, selection pruning; `set_unit_command` seams (PAR-0140)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED + NEW root cause. `g` alone (no Shift) turns command mode ON (`command_mode()` true). With 3 commandable own units (dagger, mega, mono, `selectable_units_json` commandable true) `g` in command mode selects NOTHING (`selection_json()` `[]`): `select_all_units` builds an infinite `SelectRect` whose `x + w` is NaN, so `bounds()` max is -inf (`core/input/rts.rs:51-66`). FIXED in place (Port today + change 4). Stance/command id maps code-confirmed (`desktop.rs:741-752`).

**Symptom / acceptance.** `unit_stance_hold_fire` sends stance 0 (= `stop`),
every stance key is off by one; `cancel_orders` deselects instead of
ordering `stop`; five command keys are unmapped; `G` turns command mode on
by itself (Java: only in command mode) and selects `mono`s (Java excludes
`controlSelectGlobal == false`); transport-select selects everything;
factory-select selects nothing; dead units stay selected.

**Java.**
* `UnitStance` ids (content order, port `core/content/registries/stances.rs:135+`): `stop`, `holdfire`, `pursuetarget`, `patrol`, `ram`, `boost`, `holdposition`, `mineauto`, then item stances.
* `UnitCommand` order (port `core/content/registries/commands.rs`): `move, repair, rebuild, assist, mine, enterPayload, loadUnits, loadBlocks, unloadPayload, loopPayload`.
* Command-table keybinds (PlacementFragment command table, `grep -n "unitStance\|unitCommand\|cancelOrders" J/ui/fragments/PlacementFragment.java`): `Call.setUnitStance(player, ids, stance, !activeStances.get(stance.id))`; `Call.setUnitCommand(player, ids, command)` over `selectedUnits` filtered by `type.allowStance/allowCommand`; `cancelOrders` = stance `stop`.
* DI:313: `selectedUnits.removeAll(u -> !u.allowCommand() || !u.isValid() || u.team != player.team());`
* DI:315-357 (only `if(commandMode && !scene.hasField() && !scene.hasDialog())`): select-all = own units `isCommandable() && type.controlSelectGlobal` (across-screen: rect + same filter); transport = `isCommandable() && unit instanceof Payloadc`; factories = own `build.isCommandable()` buildings (`block.commandable`: `CoreBlock.java:82`, `UnitFactory.java:54`, `Reconstructor.java:48`, `UnitAssembler.java:70`, `PayloadSource.java:42`), across-screen retains those overlapping the camera rect.
* `controlSelectGlobal = false` only for `mono` (`J/content/UnitTypes.java`, grep `controlSelectGlobal`).

**Port today.** `desktop.rs:719-752`: `select_all_*` -> `select_all_units` (`:916`, toggles command mode on, no filters, factories clears). **It also selects NOTHING (verified 2026-10-14, 3 commandable own units present, `selection_json()` = `[]`):** the global rect is `SelectRect { x: -inf, w: +inf, .. }`, `x + w` = NaN, so `SelectRect::bounds` (`core/input/rts.rs:51-60`) yields `max_x = -inf` and `contains` is always false; `"cancel_orders" | "deselect"` clears; stance arms `set_stance(0..5)`; `unit_command_move..mine` -> `command_units(0..4)`. Sim stances are live (`apply_live_unit_stance`, `core/sim/mod.rs:670`). `selectable_units_json` rows (`core/sim/unit_runtime.rs` ~2974): `id,type,x,y,team,commandable,hit_size`. `SelectableUnit` (`core/input/rts.rs`). `MindInput.set_unit_stance(i64, bool)` exists; `set_unit_command`/`set_unit_stance_named` do NOT exist on `ffbe84dd` (H03 T10 defines them).

**Changes.**
1. `desktop.rs`: helpers resolving names through host content
   (`content.unit_stance_by_name(&str)`, `content.unit_command_by_name(&str)`;
   confirm names with grep in `core/content/`):
   `pub fn set_stance_named(&mut self, name: &str, enabled: bool) -> bool`,
   `pub fn command_units_named(&mut self, name: &str) -> bool` (false when
   nothing selected or unknown name). Arms: `unit_stance_hold_fire ->
   "holdfire"`, `_pursue_target -> "pursuetarget"`, `_patrol -> "patrol"`,
   `_ram -> "ram"`, `_boost -> "boost"`, `_hold_position -> "holdposition"`,
   `cancel_orders -> "stop"`; `unit_command_{move,repair,rebuild,assist,mine,
   enter_payload,load_units,load_blocks,unload_payload,loop_payload}` ->
   `move, repair, rebuild, assist, mine, enterPayload, loadUnits,
   loadBlocks, unloadPayload, loopPayload`. All only when `command_mode`.
   Remove `"deselect"` from the clear arm (it is a mouse binding). Stance
   `enabled`: Java toggles `!activeStances.get(id)`; if the selectable row
   (change 3) carries the unit's active stances use the toggle, else send
   `true` and note the deviation.
2. `MindInput` (`mod.rs`): if `grep -n "fn set_unit_command" client/rust/mind-gdext/src/input/mod.rs` finds H03's funcs, make them call the helpers of change 1 (do not redefine); otherwise define:
   ```rust
   /// `Call.setUnitCommand(player, selected ids, command)` by UnitCommand name.
   #[func] pub fn set_unit_command(&mut self, command: GString) -> bool { self.bridge.command_units_named(&command.to_string()) }
   /// `Call.setUnitStance(player, selected ids, stance, enabled)` by stance name.
   #[func] pub fn set_unit_stance_named(&mut self, stance: GString, enabled: bool) -> bool { self.bridge.set_stance_named(&stance.to_string(), enabled) }
   ```
3. `core/sim/unit_runtime.rs` `selectable_units_json`: add row fields
   `"payload": <entity has UnitPayloads>` (component name as used in
   `gdext/sim_host.rs:5131`) and `"select_global": def.control_select_global`
   (`core/content/registries/units/mod.rs:1144`). `SelectableUnit`
   (`core/input/rts.rs`) gains `pub payload: bool`, `pub select_global: bool`
   (default false / true); parse them in `parse_selectable_units`
   (`desktop.rs:196`) with `unwrap_or` defaults.
4. `select_all_units(bindings, kind)` with `kind: SelectAllKind { Units, Transport, Factories }`: return unless `command_mode`; for the non-across-screen case do NOT build an infinite `SelectRect` (NaN bounds, see Port today) — filter `self.selectable` directly (or pass `Option<SelectRect>` = `None`), and add a pure test that select-all returns every own commandable unit; Units filter `commandable && select_global`; Transport filter `commandable && payload`; across-screen uses `viewport_world_rect` with the same filter. Factories: new host seam `pub fn commandable_buildings(&self) -> Vec<(i16, i16)>` on `MindSimHost` -> `Sim::commandable_buildings(team)` scanning building entities in `EntitySeq` order whose block kind is `CoreBlock | UnitFactory | Reconstructor | UnitAssembler | PayloadSource` and team == player team; fill `controller.state.command_buildings` (`TilePos`); across-screen keeps those whose tile centre is inside the viewport rect.
5. Pruning (DI:313): in `MindInput::process` before the objective block, `retain` `selected_units` ids that are present in `bridge.selectable` with `team == player_team && commandable`; emit selection if it changed.

**Tests.** `core/sim/unit_runtime.rs` test (next to the existing
`selectable_units_json` test ~4870): rows carry `payload` (a `mega` true,
a `dagger` false) and `select_global` (`mono` false). `core/input/rts.rs`
pure filter test if you add a filter helper there.

**Verify in-engine.** Ground Zero; spawn player-team units (team from
`$H.default_team()`, unit index from `content_list("unit").find("dagger")`):
two daggers, one mono, one mega near (1040,460) via `spawn_unit` ops.
Press `g` alone (no shift) -> `$I.command_mode()` false and
`selection_json()` `[]`. Hold `shiftLeft` (KeyDown), press `g` -> selection
has dagger+mega ids, no mono. `$I.rebind("unit_stance_hold_fire","k")`,
KeyDown `k` while selected -> `unit_debug_json()` daggers show stance
`holdfire` (check the field name in the JSON). `return $I.set_unit_command("mine")`
-> true. KeyUp `shiftLeft`. `$I.reset_keybinds()`.

**Commit.** `Gameplay H08 T6a: RTS stance/command keys by name, select-all gating and filters, set_unit_command`

---

### T6b. RTS orders: attack buildings, command buildings, middle-click queue, setUnitCommand never moves

> **In-engine verification (2026-10-14, session 3):** NOT CHECKED in-engine this round (code-only: `command_tap` units-only, middle always `pick_block` at `desktop.rs:1605`). Recipe not dry-run.

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED in engine (local H00-T1 env patch for the player, reverted). Two player daggers spawned with `spawn_unit` (`unit` 0, `team` 1) at world (1080,480)/(1096,480); an enemy `copper-wall` placed with `apply_sim_command_json` `{"op":"place","x":140,"y":62,"block":216,"team":2,"rotation":0}` (`building_team_at(140,62)` = 2). Shift held (`push_event KeyDown shiftLeft`), drag-select -> `selection_json()` `[1000000001,1000000002]`; right-click on the wall -> `command_targets_json()` `[{"id":1000000001,"x":1124.0,"y":500.0,"attack":false}, …]` — a MOVE to the wall position (+4 px tile-centre offset, H01 T1), no attack target; the daggers walked onto it (`runs/20261014-s3-verify3/H08-T6b-wall-rightclick.png`). Middle-click on empty ground (135,60) in command mode: `block` stays null, no target (pick_block of nothing). `$I.has_method("set_unit_command")` is **false** (H03 T3 / T6a seam not on main) — the plan's last verify line would raise a SCRIPT ERROR today. Verify recipe FIXED in place (below). NEW, not owned here: units loaded from a save come back with `GroundAI` (`unit_debug_json` `kind`), so `selectable_units_json` reports `commandable: false` and they cannot be selected; Java `UnitComp.afterRead` (l.586-593) recreates the controller (`CommandAI` for non-AI teams) — reported to the coordinator (H04 T9 / H01 area).

**Symptom / acceptance.** Right-click on an enemy building moves units to
it instead of attacking it; selected factories cannot be given a rally
point; middle-click in command mode picks a block instead of queueing an
order; `set_unit_command("move")` sends units to tile (0,0).

**Java.** `InputHandler.commandTap` (IH:1165-1201) pasted in §1.2 sources:
```java
Vec2 target = input.mouseWorld(screenX, screenY).cpy();
if(selectedUnits.size > 0){
    Teamc attack = world.buildWorld(target.x, target.y);
    if(attack == null || attack.team() == player.team()) attack = selectedEnemyUnit(target.x, target.y);
    Call.commandUnits(player, ids, attack instanceof Building b ? b : null, attack instanceof Unit u ? u : null, target, queue, true);
}
if(commandBuildings.size > 0) Call.commandBuilding(player, commandBuildings.mapInt(b -> b.pos()).toArray(), target);
```
`DesktopInput.touchDown` (DI:924-934): `if(scene.hasMouse() || !commandMode) return false; if(button == mouseRight) commandTap(x, y); if(button == Binding.commandQueue.value.key) commandTap(x, y, true);`
`InputHandler.setUnitCommand` (IH:411-444): switches `ai.command(command)`, resets targets when `resetTarget` — never a move order.

**Port today.** `core/input/desktop.rs:172-194` `command_tap` returns `None`
without selected units; target = enemy unit or position. `RemoteAction::CommandBuilding { positions, x, y }` and `CommandTarget::Building` exist (`core/input/action.rs`). `mouse_button` (`desktop.rs:1599`) `("middle", true) => pick_block()`. `core/sim/mod.rs` `apply_live_unit_command` (~L526): `is_move` with the wire `(0,0)` (from `command_emit.rs:143-155`) resolves tile (0,0).

**Changes.**
1. `core/input/desktop.rs` `command_tap(&mut self, units, player_team, x, y, queue, enemy_building: Option<TilePos>) -> SmallVec<[RemoteAction; 2]>`: units part when selection non-empty: target = `CommandTarget::Building(pos)` if `enemy_building` is Some, else enemy unit, else position; buildings part when `state.command_buildings` non-empty: `RemoteAction::CommandBuilding { positions: <command_buildings>, x, y }` (check the exact field types in `action.rs`). Update the one caller (`gdext desktop.rs:1212`) to pass `enemy_building` = tile under the cursor when `building_team(x,y)` is `Some(t)` with `t != player_team` (Java `attack.team() != player.team()`, derelict counts as non-player — keep Java), and emit each returned action.
2. `gdext desktop.rs` `mouse_button`: `("middle", true) => if self.controller.state.command_mode { self.command_tap_queued(bindings) } else { self.pick_block() }` where `command_tap_queued` passes `queue = true`.
3. `core/sim/mod.rs` `apply_live_unit_command`: add `set_only: bool` = `is_move && x == 0.0 && y == 0.0`; when set, `order = None` (command switch without a target). Doc comment: the `SetUnitCommand` wire carries `(0,0)` (`command_emit.rs`); a real move to world (0,0) is indistinguishable — accepted deviation. Tell H01 (they own the file; keep the diff to these lines).

**Tests.**
* `core/input/desktop.rs`: `command_tap_targets_enemy_building` and `command_tap_commands_buildings` (pure).
* `core/sim/tests.rs` (grep where `commanded_dagger_reaches_target` lives and copy its setup): `set_move_command_does_not_move_units`: dagger at (100,100), `SimCommand::UnitCommand { units: [id], command: move, x: 0.0, y: 0.0 }`, 60 ticks -> still within 1 px of (100,100).

**Verify in-engine.** Ground Zero: spawn 2 player daggers, an enemy-team
`copper-wall` via `apply_sim_command_json` place op with team 2 at
(140,62) (numeric block id: `content_list("block").find("copper-wall")` = 216
on `ffbe84dd`, plus `"rotation":0`). Recipe notes (verified 2026-10-14 round
3): keep `shiftLeft` held from the drag-select through the right-click — the
selection is cleared when Shift is released; compute screen points with
`$C.world_to_screen(wx, wy)` right before each push (camera follows the
player); daggers spawned before a save/reload are not commandable (see the
round-3 note above) — spawn fresh ones; `$I.set_unit_command` exists only
after H03 T3 / T6a (check `has_method` first). Shift-hold, drag-select the daggers, right-click the wall ->
`$H.command_targets_json()` shows a building target (read its format
first). Middle-click empty ground at (135,60) while still in command mode
-> a queued order appears (format per `command_targets_json`; if queues are
not exposed, check `unit_debug_json` positions move after the first
order). `$I.set_unit_command("move")` -> positions unchanged over 2 s.

**Commit.** `Gameplay H08 T6b: command taps attack buildings, command factories, queue on middle click; setUnitCommand never moves`
(add `Fixes PAR-0140:` instead if the ledger item is reopened by the coordinator; otherwise `PAR-0140:` slice wording is fine — use `Gameplay H08 T6b`.)

---

### T7. Command mode: canBoost guard and in-game gate; PAR-0137 boosting mech in-engine

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. Spawned an own `nova` (unit index 5), Ctrl+click possessed it (`player_unit_status().type` = `nova`); KeyDown `shiftLeft` → `command_mode()` true while held, false after KeyUp. FIXED in place: `unit_debug_json` has no elevation field (row keys listed).

**Symptom / acceptance.** With a `nova` (canBoost) as the player unit,
holding Shift enters command mode (Java: Shift boosts; command mode is
disabled while the player can boost and both binds share the key).
Command mode can toggle on the main menu via the binding path.

**Java.** DI:300-311:
```java
if(!locked && block == null && !scene.hasField() && !scene.hasDialog() &&
        !(!player.dead() && player.unit().type.canBoost && Binding.commandMode.value.key == Binding.boost.value.key)){
    if(settings.getBool("commandmodehold")) commandMode = input.keyDown(Binding.commandMode);
    else if(input.keyTap(Binding.commandMode)) commandMode = !commandMode;
}else{ commandMode = false; }
```
The whole `DesktopInput.update` runs only in game (`Control.java:690-760`).

**Port today.** `desktop.rs:573-575` `command_mode_allowed` = `block.is_none() && !text_focus && !ui_dialog` (no lock, no canBoost, no in_game).

**Changes.**
1. `gdext/sim_host.rs`: `pub fn player_unit_can_boost(&self) -> bool` (not func; same read as `sim_host.rs:5131-5137`).
2. `command_mode_allowed`:
   ```rust
   let shared = bindings.value(ids::COMMAND_MODE) == bindings.value(ids::BOOST);
   let boost_guard = shared && self.host.as_ref().is_some_and(|h| h.bind().player_unit_can_boost());
   self.in_game && !self.locked && self.controller.state.block.is_none() && !self.text_focus && !self.ui_dialog && !boost_guard
   ```
   (`command_mode_allowed` needs `bindings`: add the parameter and update both callers.) `toggle_command_mode()` (the MCP latch) stays usable in game.
3. Existing desktop tests construct `DesktopBridge::new()` with `in_game false`: set `bridge.in_game = true` in each command-mode test; add `command_mode_off_on_menu` (in_game false, KeyDown shiftLeft -> command mode false).

**Verify in-engine (also the PAR-0137 check).** Ground Zero: spawn a
player-team `nova` next to the core, possess it (Ctrl+click: MouseMove
over it, KeyDown `controlLeft`, MouseButton left down/up, KeyUp) ->
`unit_debug_json()` player row type `nova`. KeyDown `shiftLeft`, wait 1 s
-> `$I.command_mode()` false and the player is boosting (NOTE 2026-10-14: `unit_debug_json`
rows have NO elevation field — keys are `id,type,team,kind,pos,rotation,health,can_shoot,vel,statuses,mounts`;
add an `elevation` key to the row (pub fn in `unit_runtime.rs`, same JSON) or record the
hover visually); move with `d` held 1 s over a building
-> the mech hovers over it; KeyUp `shiftLeft` -> elevation returns to 0
off solid tiles. Screenshot while boosting. Then V (respawn) -> alpha;
Shift -> command mode true again. Record values for PAR-0137.

**Commit.** `Gameplay H08 T7: command mode honours the canBoost guard and the in-game gate`

---

### T8. Possession targets: ControlBlock vs canControlSelect (+ Router); PAR-0138 payload in-engine

> **Coordinator override (2026-10-14).** H01's plan does NOT provide `control_block_unit_id`. Define it in this task: plain `pub fn control_block_unit_id(&mut self, x: i32, y: i32) -> Option<i32>` on `MindSimHost` forwarding to a new `Sim::control_block_unit(&mut self, x: i16, y: i16) -> Option<EntitySeq-id>` in `core/sim/unit_runtime.rs` that returns the block's existing block-unit if the port already models turret block units (`grep -rn "block_unit\|BlockUnit" client/rust/mind-core/src`), and otherwise records the branch as `skipped: no block-unit model in the port` (do not invent one). The `BuildingControlSelect` sim half (core respawn via `CoreBuild.onControlSelect`) stays in this task too if it is under ~40 lines; else list it for H01.

> **In-engine verification (2026-10-14, session 3):** CONFIRMED (turret half) + PAR-0138 works. Ctrl+click an own `duo` at (146,58) (no unit within 40 px): 1 pending command, player stays `alpha` — the press emits `BuildingControlSelect` (`desktop.rs:1621-1626`), Java would control the turret's block unit. PAR-0138: possessed `mega`, `[` over an own dagger → dagger gone from `unit_debug_positions`, `]` → dagger reappears at the mega's position (1060.2,470.0). FIXED in place: Java drops at the carrier (`tryDropPayload`, IH:1057-1061), not the cursor; added the 40 px closest-unit pitfall (my first Ctrl+click on a duo possessed a mega 14 px away). `unit_debug_json` has no payload field: use `unit_debug_positions` disappearance.

**Symptom / acceptance.** Ctrl+click on an own turret sends
`BuildingControlSelect` (Java: control the turret's block unit via
`unitControl`); Ctrl+click on the own core does nothing (Java:
`buildingControlSelect` -> core respawn); routers are not controllable.

**Java.** `selectedUnit()` IH:2148-2163:
```java
Unit unit = Units.closest(player.team(), mouse.x, mouse.y, 40f, u -> u.isAI() && u.playerControllable());
if(unit != null){ unit.hitbox(r1); r1.grow(6f); if(r1.contains(mouse)) return unit; }
Building build = world.buildWorld(mouse.x, mouse.y);
if(build instanceof ControlBlock cont && cont.canControl() && build.team == player.team() && cont.unit() != player.unit() && cont.unit().isAI()) return cont.unit();
return null;
```
`selectedControlBuild()` IH:2165-2171: `build != null && !player.dead() && build.canControlSelect(player.unit()) && build.team == player.team()`.
ControlBlock implementers: `Turret.TurretBuild` (`Turret.java:283`, `canControl() = playerControllable` L382-383, default true L103), `BuildTurret.BuildTurretBuild` (`BuildTurret.java:92`), `Router.RouterBuild` (`Router.java:30`, `canControl() = size == 1` L47-49).
`canControlSelect`: `CoreBlock` L619-621 `player.isPlayer()` (-> `onControlSelect`: clear unit, respawn), `PayloadBlock` L103-108, `PayloadConveyor` L85-87 (unit standing on it, allowed in payloads, not spawnedByCore).

**Port today.** `desktop.rs:1554-1597`: `selected_unit_for_control` (units only), `selected_control_build` (turret kinds + BuildTurret -> `BuildingControlSelect`). Sim: `SimCommand::UnitControl` possesses (`core/sim/mod.rs:1198`), `BuildingControlSelect` only stores `control_building` (`:1238`) — sim half is H01's.

**Changes** (`desktop.rs`):
1. `selected_unit_for_control` second branch: building under the cursor whose kind is a ControlBlock kind (turret kinds that are Java `Turret` subclasses, `BuildTurret`, `Router` with size 1), same team: get its block unit id through `host.bind().control_block_unit_id(x, y)` (H01 seam, §6). If that seam is not on `main` (`grep -n "fn control_block_unit_id" client/rust/mind-gdext/src/sim_host.rs` empty), skip this branch (no action) and record "waiting for H01".
2. `selected_control_build`: building under the cursor, same team, kind `CoreBlock` (any player unit), or a payload block (`PayloadConveyor`, `PayloadRouter`, payload-block kinds — confirm kinds) with the Java unit conditions approximated by: player unit not `spawned_by_core`, standing on that building's tile (`player_unit_position()` tile == building tile area). Emit `BuildingControlSelect` only for these.
3. Remove turret kinds from `selected_control_build`.

**Verify in-engine.** Pitfall (seen 2026-10-14): `selectedUnit()` first picks the closest own AI
unit within 40 world px of the cursor (`Units.closest(.., 40f, ..)`), so a turret with a spawned
unit nearby possesses the unit instead; keep test turrets >= 6 tiles from spawned units.
Ground Zero: Ctrl+click the core (129,55) ->
`pending`/log shows `BuildingControlSelect` applied (`grep -c` the sim log
line if any; otherwise `apply_sim_command_json` result) — the visible
respawn is H01's. Ctrl+click an own `duo` with H01's seam -> player row
type is the turret block unit; without it, nothing happens (not a
BuildingControlSelect).
PAR-0138 payload check (independent of the routing change): spawn a
player-team `mega` and a player-team `dagger` 2 tiles from it; possess the
mega (Ctrl+click); MouseMove onto the dagger; push KeyDown `leftBracket` ->
within 1 s `unit_debug_json()` shows the mega carrying (payload count 1;
field name from the JSON) and the dagger gone from the units list; KeyUp;
MouseMove 3 tiles away; KeyDown `rightBracket` -> dagger back at the MEGA's
position (CORRECTED 2026-10-14: Java `tryDropPayload` calls
`requestDropPayload(player, player.x, player.y)`, IH:1057-1061, so the payload lands at the
carrier, not the cursor; the port already matches — `payload_drop` in `unit_runtime.rs:3425`). Hold `leftBracket` 1 s near two daggers -> both picked (hold
repeat). Screenshots.

**Commit.** `Gameplay H08 T8: possession targets follow ControlBlock/canControlSelect (+Router)`

---

### T9. Mouse buttons as bindings (pan, mouse_move, command_queue, rebinding); F5 power lines; F11 fullscreen (PAR-0213)

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. push `f5` → `settings_get_i32("lasersopacity")` stays 100 and `last_action` does not change (no arm); `{"t":"MouseButton","button":"back"}` held 1 s → `player_unit_pos()` unchanged; `events.rs:117-125` maps X1/X2 to `"other"`. F11 not pressed (would fullscreen the editor game); code-confirmed no `"fullscreen"` arm in `dispatch_action`.

**Symptom / acceptance.** `pan` (mouseForward) and `mouse_move` (mouseBack)
never fire; rebinding `select`/`break_block`/`pick` has no effect; F5 does
nothing; F11 does nothing (game or menu).

**Java.** Arc mouse buttons are `KeyCode`s (`mouseLeft/Right/Middle/Back/Forward`) in the same binding system. DI:261-265 pan; DI:292-295 pan formula (T10); updateMovement DI:994-996: `if(keyDown(mouseMove)) movement.add(input.mouseWorld().sub(player).scl(1f / 25f * speed)).limit(speed);` (after `movement.set(xa, ya).nor().scl(speed)`). F5 DI:459-466:
```java
if(Core.settings.getInt("lasersopacity") == 0) Core.settings.put("lasersopacity", Core.settings.getInt("preferredlaseropacity", 100));
else { Core.settings.put("preferredlaseropacity", Core.settings.getInt("lasersopacity")); Core.settings.put("lasersopacity", 0); }
```
F11 `Control.java:690-694`: `full = settings.getBool("fullscreen"); graphics.setFullscreen/Windowed; settings.put("fullscreen", !full)` (menu too).

**Port today.** `events.rs:126-133` `mouse_button_name`: X1/X2 -> `"other"`. `DesktopBridge.pressed` holds key names only. `MindCamera2D` reads `MindInput.pan_pressed()`. No `toggle_power_lines`/`fullscreen` arms. `MindUi.settings_get_i32` returns 0 for keys without a settings row (`ui_host.rs:664`); `preferredlaseropacity` has no row. `MindPlatform.set_fullscreen(bool)` `#[func]` exists (`gdext/platform/mod.rs:156`). The gmcp addon's `_mouse_button_index` (`client/addons/open_godot_mcp/runtime/runtime_autoload.gd:1052`) maps unknown names to LEFT (no XBUTTON).

**Changes.**
1. `events.rs`: `MouseButton::XBUTTON1 => "back"`, `XBUTTON2 => "forward"`.
2. `desktop.rs` `handle` `MouseButton` arm: map button -> key name (`left->"mouseLeft"`, `right->"mouseRight"`, `middle->"mouseMiddle"`, `back->"mouseBack"`, `forward->"mouseForward"`); insert/remove it in `pressed`. Pointer routing by binding: on a down edge, if `bindings.value(ids::SELECT)` is that key -> `left_press`; `BREAK_BLOCK`/`DESELECT` -> `right_press`; `PICK`/`COMMAND_QUEUE` -> the T6b middle logic; release edges likewise (`left_release`/`right_release`). For the down edge also run `key_down_event` for every OTHER binding bound to that mouse key (excluding select, deselect, break_block, pick, command_queue). Key events whose code equals the `select` binding (a keyboard key rebound to select) must call the same pointer handlers — in `key_down_event`/`key_up_event` add arms for those five names that call the pointer handlers with the current `self.mouse`.
3. `mouse_move` binding: `PlayerInput.mouse_move: bool` (`core/ai/types/player.rs`); `set_player_control` gains the flag (update the tuple in `MindInput::player_control` and both host call sites, `sim_host.rs:415` and `:2514`); in `update_player` after the axis movement vector is formed: `if input.mouse_move { mv += (aim - pos) * (speed / 25.0); limit mv to speed }` (Java order; find where the port normalises the axis and scales by speed).
4. F5: `dispatch_action` arm `"toggle_power_lines" => { self.effects.push(Effect::UiCall("toggle_power_lines".into())); self.fire_hotkey(name); }`; new `Effect::UiCall(String)` resolved in `drain_effects` by `ui.call(name, &[])` on `/root/MindUi`. In `gdext/ui/ui_host.rs` add `#[func] pub fn toggle_power_lines(&mut self)` implementing the Java swap directly on the settings store (read the store API used by `settings_set`/`settings_get_i32`; `preferredlaseropacity` default 100; persist like `settings_set`). Append at the end of the `#[godot_api]` impl.
5. F11: `dispatch_action` arm `"fullscreen" => { self.effects.push(Effect::UiCall("toggle_fullscreen".into())); }`; `ui_host.rs` `#[func] pub fn toggle_fullscreen(&mut self)`: flip bool setting `fullscreen`, persist, then `DisplayServer::singleton().window_set_mode(FULLSCREEN | WINDOWED)` (same calls as `platform/mod.rs:156-165`). The menu path already dispatches `"fullscreen"` (`desktop.rs:487-491`).
6. gmcp addon (needed only for the real-event check): in `runtime_autoload.gd` `_mouse_button_index` add `"MOUSE_BUTTON_XBUTTON1": return MOUSE_BUTTON_XBUTTON1` and `XBUTTON2`; `_mouse_button_mask` add `MOUSE_BUTTON_MASK_MB_XBUTTON1/2`. (Do not touch the addon's uncommitted `AGENTS.md` change in `REPO`.)

**Tests.** `desktop.rs` test `mouse_back_sets_pressed`: `bridge.in_game = true; handle(MouseButton{button:"back", down:true,..})` -> `key_down(&bindings, ids::MOUSE_MOVE)`; `forward` -> `key_down(ids::PAN)`. `player.rs` test `mouse_move_pulls_toward_cursor`: alpha at (100,100), no axis, `mouse_move` true, aim (200,100), 30 ticks -> x increased.

**Verify in-engine.** Ground Zero: push `{"t":"MouseButton","button":"back","down":true,...}` at a point 10 tiles east of the player, wait 1 s, release -> `player_unit_pos()` x increased. `godot_input mouse_button` with `MOUSE_BUTTON_XBUTTON2` (after change 6) pressed at a screen edge -> camera moves (T10 formula). KeyDown `f5` -> `get_node("/root/MindUi").settings_get_i32("lasersopacity")` 0; again -> 100. KeyDown `f11` -> `get_node("/root/Spine/MindPlatform")` window mode (grep its `#[func]` mode getter; `platform/mod.rs:150` dict `mode`) fullscreen; again -> windowed. Rebind `select` to `k` (`$I.rebind("select","k")`), MouseMove over ore, KeyDown/KeyUp `k` -> mining starts; `reset_keybinds()`.

**Commit.** `Fixes PAR-0213: mouse buttons as bindings; power-laser and fullscreen toggles`

---

### T10. Camera: pan key formula and `panning` latch, WASD pans while paused/dead, no desktop edge pan, `smoothcamera`

> **In-engine verification (2026-10-14, session 3):** CONFIRMED. `set_paused(true)`, KeyDown `d` held 1 s → `camera_state_json().position` unchanged (1010,440 = the player).

**Symptom / acceptance.** With the game paused WASD does nothing (Java: pans
the camera); holding the pan button with a live player does not pan; the
camera edge-pans on desktop whenever the player is dead (Java desktop never
edge-pans); camera follow is always smooth (setting `smoothcamera` ignored).

**Java.** DI:237-295:
```java
float camSpeed = (!keyDown(boost) ? panSpeed(4.5) : panBoostSpeed(15)) * Time.delta;
if(!scene.hasField() && !scene.hasDialog()){
    if(input.keyDown(Binding.pan)){ panCam = true; panning = true; }
    if(|axis(moveX)| > 0 || |axis(moveY)| > 0 || keyDown(mouseMove)){ panning = false; }
}
if(!locked){
    if((player.dead() || state.isPaused() || detached) && !chatfrag.shown() && !hasField && !hasDialog){
        if(keyDown(mouseMove)) panCam = true;
        camera.position.add(Tmp.v1.setZero().add(axis(moveX), axis(moveY)).nor().scl(camSpeed));
    }else if((!player.dead() || spectating != null) && !panning){
        camera.position.lerpDelta(panTarget /*player or lastCore on game over*/, settings.getBool("smoothcamera") ? 0.08f : 1f);
    }
    if(panCam){
        camera.position.x += Mathf.clamp((mouseX - width/2f) * panScale(0.005), -1, 1) * camSpeed;
        camera.position.y += Mathf.clamp((mouseY - height/2f) * panScale, -1, 1) * camSpeed;   // Arc mouseY is bottom-up
    }
}
```

**Port today.** `camera.rs:97-160` (`process`): with a player the target is
followed every frame (smooth), pan only clears the follow while held; axis
pan only when there is no player; edge pan (`auto_pan`) whenever there is
no player. `CameraState::pan_mouse` exists (`core/input/camera_state.rs:288`), unused. `follow(.., smooth = true, ..)` always.

**Changes** (`camera.rs`):
1. Fields `panning: bool`, `smooth_camera: bool` (read `smoothcamera` default true from `SettingsStore` in `bootstrap` and whenever `world_loads` changes).
2. Replace the follow/pan section with the Java structure:
   ```rust
   let paused = host.is_paused(); let dead = player.is_none();
   let pan_key = self.pan_pressed(); let mouse_move = self.mouse_move_pressed(); // new MindInput #[func]-free getter, like pan_pressed
   if self.gameplay_active() {
       if pan_key { self.panning = true; }
       if axis_x != 0.0 || axis_y != 0.0 || mouse_move { self.panning = false; }
   }
   let mut pan_cam = pan_key && self.gameplay_active();
   if !locked {
       if (dead || paused) && self.gameplay_active() {
           if mouse_move { pan_cam = true; }
           self.camera.pan_axis(axis_x, axis_y, delta_frames, boost);
           self.follow_target = None;
       } else if let Some((px, py)) = player && !self.panning {
           self.camera.follow(px, py, self.smooth_camera, delta_frames);
           self.follow_target = Some((px, py));
       }
       if pan_cam {
           let (mx, my) = viewport mouse;  // Godot y is top-down: flip
           self.camera.pan_mouse(mx, self.camera.viewport.1 - my, delta_frames, boost);
       }
   }
   ```
   `locked` = the input lock state (`MindInput` exposes `gameplay_input_active`; add a getter for `locks.any()` if needed). Keep cutscene/logic-cutscene code after it unchanged. `MindInput::pan_axis` returns zeros while a dialog/field is open (already). The coreless-no-player case (no core) keeps `pan_axis`.
3. Edge pan: keep `auto_pan` only when `godot::classes::Os::singleton().has_feature("mobile")`.
4. Keep `follow_world` snap on world load.

**Tests.** `camera_state.rs` already tests `pan_mouse`. Add a pure helper test if you move the decision into core (optional).

**Verify in-engine.** Ground Zero: `$H.set_paused(true)`; read
`$C.camera_state_json()` position; push KeyDown `d`, wait 1 s, KeyUp ->
x increased by ~4.5*60 px; `set_paused(false)` -> within 1 s the camera is
back on the player. Hold `MouseButton forward` (push_event, after T9) at
the right edge 1 s -> camera x increases while the player stands still;
release -> camera stays (panning latch) until WASD is pressed. Screenshots.

**Commit.** `Gameplay H08 T10: camera pans like DesktopInput (pan key, paused WASD, smoothcamera, no desktop edge pan)`

---

### T11. Derelict repair: click a derelict building, B-rect repairs derelicts (needs H02)

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE (depends on H02 T3's derelict validPlace branch, not on main). Derelict facts verified: `tile_debug(169,167)` = `scrap-wall`, `building_team_at` 0.

**Dependency.** H02's `Build.validPlace` derelict branch (Build.java:94-100,
254-260). Check: `grep -n "derelict" client/rust/mind-core/src/world/build.rs`
shows a validPlace branch for team-0 tiles AND a test placing over a derelict
building passes. If absent: skip, list "waiting for H02".

**Java.** IH:2070-2081:
```java
boolean tryRepairDerelict(Tile selected){
    if(canRepairDerelict(selected)){ player.unit().addBuild(new BuildPlan(build.tileX(), build.tileY(), build.rotation, selected.block(), build.config())); return true; }
    return false;
}
boolean canRepairDerelict(Tile tile){
    return tile != null && tile.build != null && !player.dead() && !state.rules.editor && player.team() != Team.derelict && tile.build.team == Team.derelict && tile.build.block.unlockedNowHost() &&
        Build.validPlace(tile.block(), player.team(), tile.build.tileX(), tile.build.tileY(), tile.build.rotation);
}
```
Called in the select tap (DI:726: `!checkConfigTap() && selected != null && !tryRepairDerelict(selected)`) and per building in `rebuildArea` (IH:2331-2341, `intSet` dedupe by building tile).

**Changes.**
1. `core/sim/builder.rs`: `pub fn derelict_repair_plan(&self, x: i32, y: i32) -> Option<(i16, i16, BlockId, u8, ConfigValue)>` implementing `canRepairDerelict` (unlocked check: block is in the campaign unlocked set if the sim knows it, else true; note it); `pub fn try_repair_derelict(&mut self, x, y) -> bool` = queue via `queue_player_place_config`.
2. `Sim::queue_team_rebuild`: after the ghost loop, iterate the rect's tiles, dedupe by building origin (`HashSet` is fine here: only membership, iteration order comes from the tile loop), call `try_repair_derelict`.
3. `gdext`: `MindSimHost::try_repair_derelict(x, y) -> bool` pub forward; `desktop.rs` `left_press` non-block branch: after the H02 config-tap call (if present) and before `can_tap_player`: `if host.bind_mut().try_repair_derelict(x, y) { return; }` (bind scope ends before return).

**Tests.** `builder.rs`: `derelict_wall_click_queues_repair` (team-0 copper wall at (14,10) via the block runtime place with team 0; `try_repair_derelict(14,10)` -> true and one plan); `rebuild_rect_repairs_derelicts`.

**Verify in-engine.** Ground Zero derelict scrap walls near (169,167): walk/teleport? The player must be in build range (220 px) — move the camera there and let the builder travel (plans work at any distance; the unit flies to them). Click one -> `player_build_plans_json()` has the scrap wall; after construction `building_team_at(x,y)` == player team. B-drag over 3 walls -> 3 plans. Screenshot.

**Commit.** `Gameplay H08 T11: derelict repair on click and in rebuild select`

---

### T12. `onlyDepositCore` gates player deposits

> **In-engine verification (2026-10-14, session 3):** NOT CHECKABLE in-engine (headless by design). Code CONFIRMED: `sim/player_items.rs:158-159` comment "not ported" is false (`only_deposit_core` at `io/json/rules.rs:580`, set at `game/planet.rs:403`).

**Java.** `BuildingComp.allowDeposit()` (BuildingComp.java:745): `return block.alwaysAllowDeposit || !state.rules.onlyDepositCore;`; `CoreBlock` sets `alwaysAllowDeposit = true` (CoreBlock.java:75); Erekir sets the rule (`Planets.java:82`). Used by `transferInventory` and `tryDropItems` (IH:513-535, 2286-2308).

**Port today.** `core/sim/player_items.rs:159` treats `allowDeposit` as always true with the (false) comment "not ported". The rule is `only_deposit_core` in `core/io/json/rules.rs:580` and is set in `core/game/planet.rs:403`.

**Changes.** In both `apply_player_inventory` (deposit kind) and `player_deposit_accepted`: read the live rules the same way `core/ai/types/player.rs` reads `infinite_resources` (grep `infinite_resources` there and copy the access path); `allow = def.kind == BlockKind::CoreBlock || !rules.only_deposit_core`; when false: deposit returns false / accepted 0. Fix the comment.

**Tests.** `player_items.rs`: `only_deposit_core_blocks_turret_deposit` (rule on: duo deposit returns false, core deposit still works).

**Verify in-engine.** Headless only (Serpulo never sets the rule); say so in the commit.

**Commit.** `Gameplay H08 T12: onlyDepositCore gates player deposits (alwaysAllowDeposit cores)`

---

### T13. Synthetic GUI clicks through the bridge (G07 T13 finding): diagnose, fix or document

> **SKIP: symptom not reproduced** (in-engine verification 2026-10-14, session 3). Synthetic `godot_input` clicks DO reach Controls on `ffbe84dd`: a probe on the planet dialog's bottom-left Button counted `hits` 1 (dialog closed) with motion + press + release at the rect centre, `"coords":"viewport"`; also 1 with NO motion, and 1 with default (window) coords (the embedded game window is 1152x648 with an identity screen transform, so window == viewport); an in-game HUD block button selected `rotary-pump`, a category button switched `_selected_category` to 0, a main-menu Sidebar button opened `mods`, and the mods dialog's bottom button (dialog open over the menu) closed it. Recipe for H-environment §4: `godot_input mouse_button` press then release at the centre of `control.get_global_rect()` (`"coords":"viewport"`, a preceding `mouse_motion` is harmless but not required); check with `get_viewport().gui_get_hovered_control()` after a motion. No code change; nothing to commit. Evidence `runs/20261014-s3-verify/H08-T13-menu.png`, `H08-T13-menu-after.png`.

**Symptom.** `godot_input mouse_button` on a GUI Button did not trigger it
(G07 T13), while world clicks work.

**Facts from the code (read before testing).**
* `MindInput::input` (`mod.rs:111-190`) runs in Godot's `_input` phase,
  BEFORE GUI dispatch. For a press it calls `ui_captures_at(x, y)`
  (`mod.rs:390`): a rect walk over every visible `Control`; STOP filter or a
  STOP ancestor -> "captured" -> the event is left alone for the GUI.
  Otherwise, if a dialog is open (`dialog_open`) or not in game, it calls
  `mark_input_handled()` — which STOPS the GUI from ever seeing the press.
* The addon converts `"coords":"viewport"` points with
  `get_viewport().get_screen_transform()` (`runtime_autoload.gd:484-491`);
  the default is `"window"` — a viewport point passed without
  `"coords":"viewport"` lands elsewhere when the window is not 1152x648.
* Godot `BaseButton` triggers on RELEASE (`ACTION_MODE_BUTTON_RELEASE`) and
  needs a press and a release on it.

**Diagnosis (do in this order, write every output to the evidence dir).**
1. Pick a known button: `$G --eval 'return get_node("/root/MindUi").open_dialog("planet","")'`, then find a Button: `$G --eval 'var b = get_node("/root/Spine/Ui/UiRoot").dialog("planet").find_children("*","Button",true,false)[0]; return [b.get_path(), b.get_global_rect(), b.mouse_filter, b.is_visible_in_tree()]'`. Connect a probe: `$G --eval 'var b = get_node("<path>"); b.set_meta("hits", 0); b.pressed.connect(func(): b.set_meta("hits", b.get_meta("hits") + 1)); return true'`.
2. Send `mouse_motion` to the rect centre (`"coords":"viewport"`), then `mouse_button` press and release at the same point with `"coords":"viewport"`. Read `b.get_meta("hits")`.
3. If 0: add a temporary `log::info!` in `MindInput::input` printing `(x, y, captured, dialog_open, in_game, marked)` for presses (do not commit it), rebuild, repeat; also read `get_viewport().gui_get_hovered_control()` after the motion.
4. Outcomes:
   * `captured == false` but the point IS inside the button rect -> `ui_captures_at` is wrong (e.g. the control sits under a `CanvasLayer` whose transform differs from the viewport, so `get_global_rect()` is not in viewport space). Fix: compare in the control's canvas space: `let p = control.get_global_transform_with_canvas().affine_inverse() * point; control.get_rect()`-local test, i.e. `Rect2::new(Vector2::ZERO, control.get_size()).contains_point(p)`. Add a unit-free in-engine recheck.
   * `marked == true` for a press that should go to the GUI -> make the dialog-open / menu branches NOT mark the event handled when `gui_get_hovered_control()` (after a motion) or the canvas-space test hits a visible Control; only mark when nothing is under the point.
   * `hits == 1` with `"coords":"viewport"` and motion first -> no code bug: document the recipe.
5. Whatever the outcome, document the working recipe (motion first, press +
   release, `"coords":"viewport"`, rect from `get_global_rect()` of a Control
   that is not under a scaled `CanvasLayer`, or `get_global_transform_with_canvas()`)
   in your summary under "Recipe for H-environment §4" so the coordinator
   can copy it.

**Commit.** Only if code changed: `Gameplay H08 T13: GUI clicks are never consumed by the world-input gate` (body: the diagnosis table). Otherwise no commit; the recipe goes into the summary.

**Pitfalls.** `find_children` with `owned=false`; one statement per eval;
disconnecting the probe is unnecessary (dialog is rebuilt).

---

### V1. Re-verify round-G checks that were limited by the missing player (after H00) — evidence only

> **In-engine verification (2026-10-14, session 3):** PARTLY (player via a local H00-T1 env patch). V1.1: with `mechanical-drill` selected and the sim paused, a push_event click at `world_to_screen(1092,484)` queued a plan at (136,60): `player_build_plans_json()` shows it (progress 0.1667 after `step(1)`), `construct_progress(136,60)` → 1.0 after 10 more ticks (Java rate: alpha 0.5 / buildTime 6). V1.2-V1.5 not run. `$C.world_to_screen` exists (`#[func]`).

> **In-engine verification (2026-10-14, session 3, round 3):** V1.2-V1.5 run (local H00-T1 env patch only; H00 T2 NOT applied). **V1.2 PARTLY:** 3 conveyors `place_block_rot(134..136,64,"conveyor",0)`, F-drag (KeyDown `f` at (134,64), move to (136,64), KeyUp) -> `last_action` `schematic_select`, `select_plans` 3; left click at (140,64) -> conveyors at (139,64),(140,64),(141,64) rotation 0 and core copper 500 -> 497, but BUILT INSTANTLY (`player_build_plans_json()` `[]` right after the click) — the pre-H00-T2 instant-plan symptom; re-run after H00 T2. Coordinates FIXED in place: (130..132,62) is outside the verified free ground (132..147,57..72). **V1.3 PARTLY (same caveat):** `place_block(144,60,"copper-wall")` + `debug_kill_building(144,60)` -> ghost `[{x:144,y:60,block:"copper-wall",rotation:0}]`; B-drag (KeyDown `b` (143,59) -> (145,61), KeyUp) -> wall rebuilt and `team_ghost_plans_json()` `[]`, instantly. **V1.4 PASS:** fresh daggers, Shift-drag-select `[1000000003,1000000004]`, objective false; right-click -> `command_mode_objective()` true; Shift release -> false, `command_mode()` false, selection `[]`. **V1.5 PASS:** `$I.transfer_item(129,55,"copper",30,false)` -> stack copper 30 (core 491 -> 461); duo `place_block(133,58)`; press on the alpha (`world_to_screen(1036,444)`), release on the duo (`world_to_screen(1064,464)`) -> stack 15, `building_debug(133,58).turret_total_ammo` 30. 0 panics.

No code unless a check fails (then STOP, describe, and fix in a separate
commit `Gameplay H08 V1: <what>`).
1. **G08 T4 queued plan readback**: `$I.select_block_by_name("mechanical-drill")`; push a click at world (1124,484) (screen via `$C.world_to_screen(1124, 484)`) -> `player_build_plans_json()` has a `mechanical-drill` plan at (140,60) (it must be QUEUED, not instantly built: read right after the click; `construct_progress(140,60)` grows over time).
2. **G08 T6b paste readback**: place 3 conveyors (134..136,64) (`place_block_rot`; the old (130..132,62) is outside the free ground — corrected 2026-10-14 round 3), F-drag them, move to (140,64), left click -> `player_build_plans_json()` has conveyors at (139,64),(140,64),(141,64) rotation 0, queued.
3. **G08 T7 with the real player**: `debug_kill_building` a wall, B-drag over it -> plan queued, built by the alpha (watch `construct_progress`), ghost removed from `team_ghost_plans_json()`.
4. **G08 T15 command_mode objective in-engine**: spawn 2 player daggers, Shift-drag-select, right-click a point -> `$H.command_mode_objective()` true; deselect (Shift release) -> false next frame.
5. **G08 T2b tap-unit deposit with the real alpha**: `transfer_item(129,55,"copper",30,false)`, place a duo next to the core, press on the alpha, release on the duo -> stack 15.
Record every output in the summary.

---

### T14. Escape with a block config / inventory panel open: the panel must stay under the pause menu (added after the 2026-10-14 in-engine verification)

> **In-engine verification (2026-10-14, session 3, round 3):** CONFIRMED in both clients. Port: a `sorter` at (136,60) (`place_block`), real Godot click on it -> `block_config.visible` true; real `godot_input` Escape -> `dialog_stack()` `["paused"]` and the sorter panel is still visible, drawn ON TOP of the pause dialog and its dim, overlapping the `Abandon` button (`runs/20261014-s3-verify3/H08-NEW-config-open.png`, `H08-NEW-config-escape.png`). Java (sandbox custom game, core tapped -> its inventory fragment `100` copper; the inventory lives in the same `hudGroup` table group as the config fragment): Escape opens the pause menu and the inventory stays logically shown but is drawn UNDER the black9 dim (`java-22-core-tap.png`, `java-23-escape-inventory.png`); after `Back` it is still there (`java-24-after-back.png`). Java's sorter config itself was not reachable (the sandbox stayed paused on Xvfb, so the placed sorter never built); the layering comes from the shared group (source below).

**Symptom / acceptance.** With a block config panel (or the block inventory /
plan config panel) open, Escape opens the pause menu but the panel stays drawn
above the pause dialog and its dim, and it stays clickable (its buttons sit
over the dialog; a click on `Clear` would configure the block under a modal
dialog). Java draws those panels inside the HUD group, below every dialog:
the pause dim covers them and they cannot be clicked until the dialog closes;
they are NOT hidden (after closing the pause menu the panel is still open).
Acceptance: Escape with the sorter panel open -> pause menu on top, the panel
dimmed underneath and not clickable (a synthetic click at a panel button's
centre does not change `building_config_json`); `Back` -> panel visible and
interactive again. Same for the core inventory.

**Java.**
* `J/input/InputHandler.java:2258-2263` (`add()`; `group` is the handler's
  `WidgetGroup` field):
  ```java
  group.setFillParent(true);
  Vars.ui.hudGroup.addChildBefore(Core.scene.find("overlaymarker"), group);
  inv.build(group);
  config.build(group);
  planConfig.build(group);
  ```
  (`overlaymarker` is the HUD's `cont`, `HudFragment.java:357`.) Dialogs are
  added to the scene root above `hudGroup`, so they draw over these panels.
* `J/core/Control.java:746-755`: `Binding.menu` -> if chat shown hide it,
  else `if(!ui.paused.isShown() && !scene.hasDialog()) ui.paused.show()` (+ pause).
  Nothing hides `config`/`inv` here.
* `J/ui/fragments/BlockConfigFragment.java:31-33,74-78`: `isShown()` =
  `table.visible && selected != null`; only `hideConfig()`/`forceHide()`
  (ResetEvent) clear it.

**Port today.** `client/ui/dialogs_manifest.json:51-53` registers
`block_config`, `block_inventory`, `plan_config` with `"group": "overlay"`;
`client/scenes/ui/ui_root.tscn:157-159` instances them under `OverlayLayer`,
which is a sibling AFTER `DialogLayer` (l.85 `DialogLayer`, l.94
`OverlayLayer`, l.103 `LoadingLayer`), so they draw above every dialog and
receive clicks first. `fade_in` (l.151) is also in `overlay` and must stay
there (Java's fade covers dialogs). The manifest test
(`client/rust/mind-core/src/ui/manifest.rs:496-510`) checks fragment names
only, and l.138 accepts the groups `menu|hud|overlay|loading`.

**Changes.**
1. Move the three fragments into the HUD layer, after the HUD fragments
   (Java `addChildBefore(overlaymarker)` puts them just below the HUD `cont`;
   the port has no marker — append them as the LAST children of `HudGroup`,
   which still keeps them under `DialogLayer`):
   * `dialogs_manifest.json`: `"group": "hud"` for `block_config`,
     `block_inventory`, `plan_config` (keep `"hidden": true`).
   * `ui_root.tscn`: re-parent the three instances to `HudGroup` (keep their
     node names; scripts find them by path — fix every
     `OverlayLayer/block_config|block_inventory|plan_config` path:
     `grep -rn "OverlayLayer/block_\|OverlayLayer/plan_config" client/ client/rust/mind-gdext/src`
     and update each hit, including `H-environment.md §4.2`'s documented path
     in your summary — do not edit H-environment yourself).
2. Check the HUD-hide paths: if `HudGroup` is hidden while a block is being
   configured (menu toggle / cutscene), Java hides the panels with it (they
   are in `hudGroup`) — accept that; do not add special cases.
3. Input gate: with a dialog open, `MindInput::input` must not route clicks
   to the config panel (it is now under the dialog, so Godot's GUI dispatch
   gives the dialog the click; verify, no code expected).

**Tests.** `cargo test -p mind-core --lib ui::manifest` (manifest still
valid). GDScript parse gate.

**Verify in-engine** (Ground Zero, env §4.1; no player needed):
```bash
$G --eval "return $H.place_block(136, 60, \"sorter\")" --eval "$I.select_block_by_name(\"air\"); return $C.tile_to_screen(136, 60)"
# godot_input left press + release at that point (coords viewport)
$G --eval 'return get_node("/root/Spine/Ui/UiRoot/HudGroup/block_config").visible'        # true (new path)
python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"Escape","pressed":true}}'
python3 REPO/tools/gmcp.py godot_input '{"action":"key","params":{"key":"Escape","pressed":false}}'
$G --screenshot runs/<date>-s<N>-H08-T14/escape.png      # panel dimmed under the pause menu
# click where the panel's first item button is (its global rect centre) -> building_config_json(136,60) unchanged
# close the pause menu (Back) -> panel visible again, a click on an item configures the sorter
```
Compare with `runs/20261014-s3-verify3/java-23-escape-inventory.png`.

**Pitfalls.** Do not hide the panel on Escape (Java keeps it). Keep
`fade_in` in `overlay`. Moving the nodes changes their absolute paths — the
gmcp recipes in other plans use `/root/Spine/Ui/UiRoot/OverlayLayer/block_config`
(H-environment §4.2, H02 T6, H08 T4): list the new path in the summary for the
coordinator.

**Commit.** `Gameplay H08 T14: block config/inventory panels live under dialogs (HUD group) like Java`

---

## 3. Order, dependencies, hot files

| # | Task | Depends on | Notes |
|---|---|---|---|
| 1 | T1 line rotation | H00 | biggest visible bug (vertical belts reversed) |
| 2 | T2 line flags + HostWorld | T1 | |
| 3 | T3 junction/bridge/nodes | T2 | PAR-0098 remainder |
| 4 | T4 tap/deselect gestures | — | coordinate with H02 config_tap (§6) |
| 5 | T6a RTS keys + seams | — | define `set_unit_command` only if H03 has not |
| 6 | T6b RTS orders | T6a | touches `core/sim/mod.rs` (H01 file) |
| 7 | T7 command-mode guard + PAR-0137 | — | |
| 8 | T5 block hotkeys | T6b (middle routing) | GDScript-heavy |
| 9 | T9 mouse bindings, F5, F11 | T6b | |
| 10 | T10 camera | T9 (pan key) | |
| 11 | T8 possession routing + PAR-0138 | H01 seam for the turret half | |
| 12 | T12 onlyDepositCore | — | small |
| 13 | T11 derelict repair | H02 validPlace derelict branch | skip if absent |
| 14 | T13 GUI click diagnosis | — | |
| 15 | V1 re-verification | H00 | evidence only |
| 16 | T14 config panel layering | — | added 2026-10-14 round 3; touches `ui_root.tscn` + manifest only |

Hot shared files (merge `main` often, keep diffs small): `gdext/input/desktop.rs`
(H02 adds the config-tap call; H03 may touch command-panel funcs),
`gdext/input/mod.rs` (H03 `set_unit_command`; H05 calls `use_schematic`/
`take_hint_keys`), `gdext/sim_host.rs` (append new fns at the END of the
relevant impl block), `core/sim/mod.rs` (T6b, H01's file), `core/sim/builder.rs`
(T2, T11; H02/H04), `core/sim/unit_runtime.rs` (T6a selectable rows; H01),
`core/ai/types/player.rs` (T9; H01), `ui/fragments/placement_fragment.gd`
(T5; H03 command panel), `gdext/ui/ui_host.rs` (T9; H05/H07),
`gdext/camera.rs` (T10; H03 cutscene).

## 4. Ledger changes to report (do not edit `chains.json`)

| Item | Change |
|---|---|
| PAR-0098 | -> verified after T3 in-engine junction/bridge + T1/T2 (list commits); remaining: plan drawing (H03) |
| PAR-0213 | -> verified after T9 (mouse back/forward, F5, F11; C was G03) |
| PAR-0214 | -> verified after T5 |
| PAR-0140 | add note: residuals fixed T6a/T6b (stance names, cancel_orders, select-all filters, factories, middle queue, attack buildings, set-command no-move) |
| PAR-0137 | in-engine boosting evidence from T7 (nova possessed; elevation values) |
| PAR-0138 | in-engine payload evidence from T8 (mega picks up/drops a dagger; hold repeat) |
| PAR-0215 / PAR-0189 | V1 evidence (queued plans readback) — H00 owns the fix |
| new | "Line drags point the wrong way vertically (Tile.relativeTo y flipped) and wheel snap drops the step" -> T1 |
| new | "Line placement ignores block line flags; HostWorld validPlace/chains stubbed" -> T2 |
| new | "Tapping enemy buildings opens config/inventory; empty tap keeps panels; right-click does not stop mining; buildautopause missing" -> T4 |
| new | "Ctrl+click turret uses buildingControlSelect; core/payload blocks/router control missing" -> T8 |
| new | "Camera: paused WASD pan, pan key formula, smoothcamera ignored, desktop edge pan" -> T10 |
| new | "onlyDepositCore ignored for player deposits" -> T12 |
| new | "Synthetic GUI clicks via the bridge do not reach Controls" -> T13 (fixed or documented recipe) |
| new | "Block config/inventory panels draw above the pause menu (overlay layer) instead of under dialogs" -> T14 (added 2026-10-14 round 3) |

## 5. Done criteria

* Every task committed with its tests; full `cargo test -p mind-core --lib --features msav-import` green; `cargo test -p mind-gdext --lib` green; `cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` clean; GDScript parse gate clean; no new `mind-headless` failures vs the H00 baseline.
* In-engine evidence under `runs/20261014-s<N>-H08-<task>/` for T1-T10, T13, V1 (T11 if H02 landed; T12 headless).
* 0 `panic` lines after each in-engine step.
* Summary `REPO/docs/H08-input-controls_summary.md` (H-environment §6 shape): per-task table, `git log --oneline main..HEAD`, test commands/results, evidence per task, deviations, skipped-for-dependency list, the T13 recipe, §4 ledger list with hashes.

## 6. Cross-area seams

Provided by H08:
* `MindInput.use_schematic(index: i32) -> bool` (exists; H05 calls it).
* `MindInput.take_hint_keys() -> PackedStringArray` (exists; H05).
* `MindInput.set_unit_command(command: GString) -> bool`, `MindInput.set_unit_stance_named(stance: GString, enabled: bool) -> bool`: H03 T10 DEFINES them; H08 T6a makes the keybindings use the same helpers (`DesktopBridge::command_units_named`, `set_stance_named`). If H03 has not landed when T6a runs, H08 defines them with exactly these signatures and H03 must not redefine (grep first).
* `MindSimHost` (pub, not `#[func]`): `can_place_player`, `building_rotation` (T2), `player_plan_count` (T4), `player_unit_can_boost` (T7), `commandable_buildings` (T6a), `player_plan_at_world`/`construct_current_block` (T5), `try_repair_derelict` (T11).
* `MindUi.toggle_power_lines()`, `MindUi.toggle_fullscreen()` `#[func]` (T9).
* Selectable rows gain `payload`, `select_global` (T6a) — H03's command panel may read them.

Needed from others:
* **H00**: real player spawn (env 233) and no instant `player:Some` placements — prerequisite for all in-engine steps.
* **H02**: `checkConfigTap` / `onConfigureBuildTapped` tap-to-link (`world::config_tap::config_tap`) — H02 owns the call site at the TOP of the non-block branch of `left_press` (`desktop.rs:1657`, before `can_tap_player`) and the `tileTapped` config-selected-agrees branch; H08 T4 adds team gates and panel hides around it. Derelict repair `Build.validPlace` branch (T11 dependency).
* **H01**: `MindSimHost::control_block_unit_id(x: i32, y: i32) -> Option<i32>` (the ControlBlock block unit's `EntitySeq` id, created on demand like `Router/Turret.unit()`), and the `BuildingControlSelect` sim half (core respawn via `CoreBuild.onControlSelect`, payload-block enter). H08 T6b edits `apply_live_unit_command` (move with wire (0,0) = command switch) — H01 informed; keep it if H01 rewrites the function.
* **H03**: drawing of select plans, plans, ghosts, command buildings selection; command panel uses the T6a seams.
* **H09**: none.
