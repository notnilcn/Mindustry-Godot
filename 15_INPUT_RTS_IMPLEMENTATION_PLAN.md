# 15 — INPUT, PLACEMENT & RTS IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | M0–M2 landed on `lane/15-input` (commits `777cf97`, `835d290`, `8707381`). **M3 (mobile parity) complete + M4 (RTS e2e/relay hooks) partial on `lane/f19-15` (commit `ae7448e`)** — M3: gesture/`mobile.rs` autotarget, schematic/rebuild, keyboard branches, plan get/remove/overlap, confirm + the plan-14 mobile callback API and the `input_mobile_parity`/`input_rts_move` scenarios with committed goldens; M4: `command_units_apply` 15→11 adapter, stances/commands + control groups/queue emission, deterministic replay checksum, `input_rts_move`. Remaining M4: applying unit-command `SimCommand`s through `Sim::command` (blocked — see §3.3.1) and the §7c MCP step. M5–M7 queued. 3 items flagged `NEEDS USER DECISION` in §8; none block M0–M3. |
| **Phase** | P5 Logic, UI, input (HLP §3, §5). |
| **Depends on** | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (written; `Build.valid_place*`/`valid_break`, `BuildPlan`, `ConstructBlock`, `BlockView`, `ConfigValue`, `plan_rotation`/`can_replace`/`get_replacement`/`change_placement_path`/`handle_placement_line`/`on_new_plan`, `begin_place`/`begin_break`), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (written; `CommandAI`, `UnitCommand`/`UnitStance`, `UnitGroup`, `TeamData` trees, `BuildQueue`/`BuilderComp` plan storage, `SimCommand` unit variants), `14_UI_IMPLEMENTATION_PLAN.md` (unwritten at authoring time — interfaces proposed in §3.10 and §8 R1). Transitively `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (`MindSimHost`/`MindCamera2D`, settings, scenario format), `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (settings persistence), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`SimCommand`, `Sim::command`, schedule), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (tile accessors/coords), `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (bridge `positions_valid`, `ChainedBuilding`, junction/bridge replacement data), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`Schematic`/`Schematics` — unwritten; trait stub in §3.7/§8 R1). |
| **Blocks** | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (reuses placement planner, preview state, palette/undo contracts), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (consumes command emission, plan snapshots, player-input sync contract), `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (touch specifics, mobile keyboard/native text input). Also consumed by `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (preview/selection rendering), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (screen-shake hook), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (input-log replay oracle). |
| **Sources** | AGENTS: `Mindustry/core/src/mindustry/input/AGENTS.md` (read in full), `core/AGENTS.md` (Control), `ai/AGENTS.md` (RTS/formations), `ui/AGENTS.md` (HUD placement UI/config fragments/minimap), `maps/AGENTS.md` (schematic placement). Java read in full or by region: `input/Binding.java`, `input/InputHandler.java` (2552 lines), `input/DesktopInput.java` (1044), `input/MobileInput.java` (1121), `input/Placement.java` (514), `input/PlaceMode.java`; `core/Control.java` (input selection/`setInput`/update loop), `core/Renderer.java` (camera scale/shake/landing), `core/NetClient.java:775-818` (player input sync + plan snapshots); `game/Schematics.java`/`Schematic.java` (`create`/`toPlans`/`place`/`rotate`/`flip`), `ui/fragments/{PlanConfigFragment,BlockConfigFragment,PlacementFragment}.java`, `ui/Minimap.java` (click/drag/scroll), `world/AGENTS.md` + `world/blocks/distribution/{ItemBridge,DirectionBridge,ChainedBuilding}.java` (`positionsValid`), `content/UnitTypes.java` (command availability), `ai/{UnitCommand,UnitStance}.java` (Binding fields). Addon: `phantom-camera` checkout (`README.md`, `addons/phantom_camera/plugin.cfg` v0.11.0.1) and installed copy `client/addons/phantom_camera/plugin.cfg` v0.11.0.3. Plans: `HIGH_LEVEL_PLAN.md`, `PRELIMINARY_PLAN.md`, `00`, `05`, `06`, `07`, `08`, `11`. |
| **Extends spine** | Adds node `Spine/Input` (`MindInput`, Rust) under the plan-00 `Spine` root; upgrades `MindCamera2D` from the P0 scale/convert stub to the full RTS rig (pan/zoom/follow/detach/cutscene/shake/minimap). Adds `MindSimHost` APIs: `emitted_commands()`, `get_input_state_json()`, `dev_set_block(name)`, `dev_rotate()`, `dev_place_drag(x1,y1,x2,y2)`, `dev_break_rect(x1,y1,x2,y2)`, `dev_select_rect(x1,y1,x2,y2)`, `dev_command_move(x,y[,queue])`, `dev_input_clear()` (all append-only, plan 00 §3.10 rule 4). Adds `mind-headless` drivers `input_replay`/`input_dump` and scenarios `input_*`/`placement_*`. Adds a `Placement` tab to the state inspector (mode, block/rotation, line/select plan counts, selected units, emitted command count). |

## 2. Scope & parity definition

**Delivers (HLP §3 row 15):**

1. **Input plumbing.** Godot `InputEvent` → Rust (`MindInput`); `Binding`/`KeyBind` registry with exact upstream names/defaults/categories, per-key rebinding, axis and bundle-key parity; input-handler selection desktop/mobile; `locked()`/focus guards (dialog/field/keyboard/mouse/scroll/chat/console/cutscene); mobile touch gesture port.
2. **`InputHandler` parity.** `input.block`/`rotation`, `updateState`/`update`, build-plan quadtrees, plan mirror (`lastPlans`), `isBuilding` toggle + `buildautopause`, cursor tile helpers, payload key handling, possession/respawn retry, `input.add()`/`remove()` handler lifecycle.
3. **DesktopInput parity.** Line drag (`updateLine`/`flushPlans`/`flushPlansReverse`), schematic select (`f`), rebuild select (`b`), plan moving (`splan`), plan config (`ctrl`-click), right-click breaking + rect removal, quick rotate, cursor selection, camera pan (WASD/pan key/mouse-move/boost), zoom gating, RTS drag-select/tap-select/queue/control groups, ping, tile tap, possession, pause/screenshot bindings.
4. **MobileInput parity.** Touch tap/long-press/pan/zoom, line mode, schematic/rebuild modes, edge auto-pan, on-screen break/rotate/confirm/cancel/pause/command/queue buttons, select-then-confirm commit, payload targeting, manual shooting/autotarget, camera-follow movement, mobile keyboard setting.
5. **`Placement` parity.** `normalizeLine`/`normalizeRectangle`/`normalizeArea`/`normalizeDrawArea`, conveyor A* (`conveyorpathfinding`), bridge placement DP + config rewrite (`calculateBridges`), power-node spacing (`calculateNodes`), `upgradeLine` over `ChainedBuilding`, side-place detection, per-point rotation via `iterateLine`.
6. **Build-plan queue.** Client mirror + spatial trees; `addBuild` call-through to 11's `BuilderComp`/`BuildQueue`; `selectPlans` commit paths; undo/replace semantics; plan snapshot emission (0.5 s, chunked) for 21; preview state for 16.
7. **RTS.** Command-mode toggle (`commandmodehold`), drag-rect/tap/typed selection, selection sets, queue mode, control groups (create/recall/double-tap center) with `distinctcontrolgroups`, `commandUnits`-equivalent emission (chunk 200, final-batch formation grouping via 11's `UnitGroup`), `setUnitCommand`/`setUnitStance` emission, command/stance binding data for 14's command UI.
8. **Camera.** Pan/zoom/follow/lock-to-unit/detach/cutscene/smooth clamp, minimap pointer mapping, screen-shake hook, landing/launch zoom hook; **Phantom Camera evaluated, custom Rust rig chosen (§3.9)**.
9. **Input → command pipeline.** Exact boundary where client intent becomes a relayed command (D2/21): one `RemoteAction` → `SimCommand` mapping, local deterministic apply, relay handoff, echo dedup, error surfacing.

**Done means (parity):** a player on desktop and a player on touchscreen can, against plan 00 + 07 + 11 + 14: select any block/build a conveyor line with A* and bridges/place by drag-or-tap/rotate/flip/rotate queued plans/break by drag/rect/schematic-cut/rebuild derelicts/queue and move plans; and in RTS mode select units and buildings by rect/tap/type/control-group, issue move/attack/queue commands and toggle stances, with the server-equivalent command stream applied in the same order on every peer. Camera behavior matches `Renderer`+`InputHandler` numbers (§6.6). All §7 oracles pass.

**Explicitly deferred (owner plan):**

| Area | Owner |
|---|---|
| HUD placement palette (`PlacementFragment`), block inventory/config/plan-config dialogs, minimap widget, mobile button layout/scene wiring, KeybindDialog UI, chat/console fragments | `14_UI_IMPLEMENTATION_PLAN.md` |
| `Build.validPlace*`/`validBreak` rules, `BuildPlan` fields, construct progress, block config handlers, rotation/replacement predicates, `ConstructBlock` | `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` |
| `CommandAI`/`CommandController` execution, `UnitGroup` formation math, `UnitCommand`/`UnitStance` registries, unit quad trees, `BuildQueue`/`BuilderComp` storage, `Pathfinder`/`ControlPathfinder` | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` |
| Rendering of preview ghosts, plan config overlays, selection polygons, command lines, breaking overlay, cursors, minimap texture | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| `Schematic`/`Schematics` data (`create`/`toPlans`/`place`/`rotate`), `.msch` IO, schematic browser | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (15 consumes) |
| Relay tables/reducers, ordering, echo dedup, desync correction, plan-snapshot/player-input sync | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Editor tools/renderers, undo stack, objective/wave dialogs | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| Android/iOS export, native keyboard/IME flows, store input polish | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` |
| Screen-shake effect budget/particles; `Fx` effects fired by input | `17_FX_PARTS_IMPLEMENTATION_PLAN.md` |
| Sound events for placement/rotation/errors | `18_AUDIO_IMPLEMENTATION_PLAN.md` |

**Deliberate deviations:**

| # | Deviation | Reason / tracking |
|---|---|---|
| I1 | **Logic split:** the pure input state machine, placement algorithms, plan queue mirror, RTS selection/emission live in `mind-core` (Godot-free); only event translation, Godot node plumbing, settings persistence and camera node live in `mind-gdext`. | D1 + plan 00 §3.3 (`mind-gdext` has no game rules); makes input replayable in `mind-headless` without Godot (the D5 oracle requirement). |
| I2 | Input sampling is **render-frame** (like Arc) but all emitted commands are stamped with the next sim tick and drained at tick start; input never reads or writes sim state mid-tick. | D8/§2.4; keeps command logs replay-safe while preserving responsiveness. |
| I3 | Timing uses the client monotonic clock (tick count + frame delta), not wall clock, for double-tap/hold windows; input state is client-local and never checksummed. | §2.4 "no wall-clock reads" applies to sim only; deterministic replay still possible because the input log records absolute event ticks. |
| I4 | Phantom Camera is **not used** for the gameplay camera; a custom Rust rig is implemented. The addon stays installed but not autoloaded (until an explicit decision to delete). | §3.9 evaluation; D1 requires Rust view drivers; addon has no GDExtension API (GDScript/C# only). No `THIRD_PARTY_NOTICES.md` entry needed while unused (plan 00 §3.9). |
| I5 | `input.keyTap` edge detection (Arc `InputProcessor` semantics: tap/release/axisTap) is reimplemented over Godot events with the same frame-latched behavior; Godot `InputMap`/`[input]` actions are used only for engine/editor defaults, never for `Binding` keys. | Plan 00 OD-R10; full control over rebinding + parity. |
| I6 | `Draw`-side code of `InputHandler`/`DesktopInput`/`MobileInput` (`drawTop`/`drawBottom`/`drawCommanded`/...) is not ported here; it is expressed as `PreviewState` data consumed by 16. | 16 owns all drawing (HLP §3 row 16); avoids duplicating draw code. |
| I7 | `input.update()` runs only while `state.is_game()` (already true upstream), and `updateState()` runs always; the mobile handler's in-frame camera side effects are moved behind the camera API. | Matches `Control.java:686-708`; keeps camera math in one place. |

## 3. Target design

All names are final unless marked. `mind-core` is Godot-free/tokio-free; `bevy_ecs` pinned by plan 00. Input state is not ECS data — it is a `Resource` plus plain structs, because it is client-local and must not enter checksums.

### 3.1 Module layout

```
client/rust/mind-core/src/input/
  mod.rs                 # re-exports; InputPlugin registers nothing in the sim schedule (see §3.2)
  binding.rs             # KeyBindId, KeyKind (Key/Axis), KeyBindTable (names/defaults/categories), BindingState (values)
  place_mode.rs          # PlaceMode { None, Breaking, Placing, SchematicSelect, RebuildSelect }, MobileMode flags
  placement.rs           # normalize_line/rectangle/area/draw_area, pathfind_line (A*), upgrade_line,
                         # calculate_nodes, calculate_bridges (+ BridgePlacer trait), is_side_place
  plan.rs                # PlanRef/PlanCopy helpers, PlanTree (Arc QuadTree port), PlanMirror (lastPlans),
                         # ClientPlan { x, y, rotation, block, config, breaking, anim_scale } (UI copies of 07 BuildPlan)
  line.rs                # iterate_line/push_line/PlaceLine, rotation resolution, blockreplace + handle_placement_line calls
  rts.rs                 # SelectionState, select_units_rect/tap/typed, control groups, command target resolution
  command_emit.rs        # RemoteAction -> SimCommand builders; chunking; ActionBatch; ActionError
  action.rs              # RemoteAction enum (every client mutation; one place that decides relay vs local)
  focus.rs               # InputLocks, FocusState, is_locked/is_placing/is_breaking/can_shoot/can_mine predicates
  input_log.rs           # InputEventRecord/InputLog/InputReplay (serde; test + MCP regression format)
  camera_state.rs        # CameraState { target_scale, camerascale, position, shake... } + pure math
  client_input.rs        # InputState (the InputHandler port: block/rotation/modes/plans/selection/mirror)
  desktop.rs             # DesktopController (platform-neutral port of DesktopInput's decision tree)
  mobile.rs              # MobileController (platform-neutral port of MobileInput's decision tree)
client/rust/mind-gdext/src/
  input/
    mod.rs               # MindInput (Node): _input/_unhandled_input -> RawEvent queue, per-frame pump
    events.rs            # Godot InputEvent -> RawEvent (keycodes, mouse, touch, magnify, scroll, resize)
    bindings.rs          # settings (04) <-> BindingState persistence; Godot keycode <-> KeyCode names
    gesture.rs           # GestureDetector port (long press / tap square / tap count / fling)
    desktop.rs           # event glue + UI focus probe + cursor application
    mobile.rs            # touch glue + mobile UI callbacks (14 buttons)
    camera.rs            # MindCamera2D (extends plan 00): CameraState driver + shake + cutscene + clamp
    api.rs               # #[func] API consumed by GDScript (14/19): select_block, rotate, set_mode,
                         # confirm_plans, cancel, keybind list/rebind, minimap_pan/zoom, state JSON
client/rust/mind-gdext/src/
  input/ui_focus.rs      # UiFocus probe (GDScript-pushed; see §3.10) -> core FocusState
client/scenes/input.tscn # optional: reusable on-screen button roots owned by 14
```

Godot node: `Spine/Input` (`MindInput`) added by the plan-00 scene; `MindCamera2D` stays at `Spine/World/Camera2D`.

### 3.2 State ownership and schedule

- `InputState` is owned by `MindSimHost` (Rust) and updated once per render frame (`MindInput::_process`) before the sim pump's catch-up loop; `updateState()` runs even when not `is_game()` (menu clear rules ported).
- Sim schedule: none of this plan registers ECS systems. Emitted commands are pushed to `SimHost::pending_commands` and drained by plan 05's command application at tick start, in FIFO order, before `TickSet::Frame`. This is the same path as plan 21's relay application.
- Nothing in `input::*` may read `bevy_ecs` world state directly except through the read-side APIs named in §4 (`WorldView` bundle: `BlockView`, `TileView`, unit/building query traits). Writes happen only through `Sim::command`.
- Local-only state that never enters `Sim`/checksums: camera, focus, cursor, `block`/`rotation`, `linePlans`/`selectPlans`/mirrors, selection sets, control groups, gesture state, input locks, `lastSchematic`.
- `player.shooting`, `player.mouseX/Y`, `player.boosting`, `player.selectedBlock/Rotation`, `player.unit().plans` remain sim/player state (05/11/21 sync them); 15 writes them via documented 05/11 APIs (`player_input` helpers), never by direct component mutation in the sim tick.

### 3.3 Input → command pipeline (D2 / plan 21 boundary)

Every client mutation is expressed once as `RemoteAction` in `input/action.rs`; `command_emit.rs` turns it into one or more `SimCommand`s and applies them locally (`Sim::command`) in the same order they are appended to the outgoing batch.

```
Godot InputEvent ──► MindInput ──► RawEvent queue ──► Desktop/MobileController.update(InputState)
                                                        │
                                     placement/plan helpers (core)  ──►  PreviewState (16) + plan mirror
                                                        │
                                              RemoteAction (action.rs)
                                                        │
                              SimCommand builder (command_emit.rs) ──► Sim::command (deterministic, local)
                                                        │
                                          ActionBatch { client_seq, tick, cmds } ──► mind-stdb relay (21)
```

| Upstream `@Remote` (`InputHandler`) | Trigger in 15 | Emitted `SimCommand` | Local-only counterpart |
|---|---|---|---|
| `tileTap` | click/tap on ground | none (21 event `tap`; mods) | cursor/inventory |
| `rotateBlock` | rotate on placed build / `quickRotate` | `Rotate { x, y, direction }` **(new, §3.3.1)** | `Fx`/sound via events |
| `tileConfig` | 14 config fragments call 07 `configure()` | `Configure { x, y, value }` (exists, 05) | plan config UI |
| `deletePlans` | break-rect removes team/queued plans | `DeletePlans { positions }` **(new)** | plan mirror removal |
| `removeQueueBlock` | hold-select removes breaking plan | `DeletePlans { positions }` (one entry) | plan mirror removal |
| `commandUnits` | RTS move/attack, formation batch | `UnitCommand`/`UnitCommandQueue` (11 §6.5) | selection render |
| `setUnitCommand` | 14 command UI / command bindings | `UnitCommand { units, command, x, y }` | command cursor |
| `setUnitStance` | 14 stance UI / stance bindings | `UnitStance` (11 §6.5) | stance icons |
| `commandBuilding` | RTS order with `commandBuildings` | `CommandBuilding { positions, x, y }` **(new)** | building selection |
| `requestItem` | 14 inventory withdraw | `Inventory { kind: Withdraw, ... }` **(new)** | flow window |
| `transferInventory` | drop item on building | `Inventory { kind: Deposit, ... }` **(new)** | cooldown |
| `dropItem` | item drop (timeout/rect) | `Inventory { kind: DropItem, angle }` **(new)** | `droppingItem` state |
| `requestUnitPayload`/`requestBuildPayload`/`requestDropPayload` | payload keys, mobile long-press | `Payload { kind: PickupUnit/PickupBuild/Drop, ... }` **(new)** | payload target marker |
| `pickedUnitPayload`/`pickedBuildPayload`/`payloadDropped`/`unitEnteredPayload` | never client-emitted (host completion) | internal apply in 08/11 | — |
| `unitControl`/`buildingControlSelect` | possession (double-tap / ctrl+select) | `UnitControl { unit: Option<i32> }`, `BuildingControlSelect { x, y }` **(new)** | `controlledType`, recent-respawn timer |
| `unitClear` | `respawn` binding | `UnitClear` **(new)** | player unit clear |
| `pingLocation` | `ping` binding | none (21 event `ping`) | ping marker |
| `clientPlanSnapshot` | 0.5 s timer | none (21 `plan_snapshot` view) | `lastPlans` mirror |
| item/liquid set/take/clear/transfer effects | logic/server only | not emitted by 15 | — |

#### 3.3.1 `SimCommand` extension requests (same mechanism as 11 §6.5)

Plan 05 §6.4 currently has `Place`, `Break`, `Configure`, `UnitCommand`, `SetRules`, `SpawnUnit`, `Custom`. 15 requires these addition **requests** to be executed in plan 05's `determinism/command.rs` at the same time as 11's two additions:

```rust
Rotate { x: i16, y: i16, direction: bool },
DeletePlans { positions: SmallVec<[i32; 32]> },
CommandBuilding { positions: SmallVec<[i32; 32]>, x: f32, y: f32 },
Inventory { kind: u8 /*Withdraw|Deposit|DropItem*/, x: i16, y: i16, item: Option<u16>, amount: i32, angle: f32 },
Payload { kind: u8 /*PickupUnit|PickupBuild|Drop*/ , x: f32, y: f32, target: Option<i32> },
UnitControl { unit: Option<i32> },
UnitClear,
BuildingControlSelect { x: i16, y: i16 },
```

If plan 05's enum is declared closed, the fallback is `Custom { kind, data }` with documented kind constants (`CUSTOM_ROTATE = 101`, …) plus a `mind-core` decode shim; explicit variants are strongly preferred for relay readability (the 11 §6.5 rationale). This is the only place 15 needs 05 changed, and it is additive.

> **Reconciliation (2026-10-03, `lane/f19-15`).** Plan 05's `Sim::command` currently applies only the P0/E1 subset (`Place`/`Break`/`Configure`, via `SimCommand::to_p0`); the eight §3.3.1 variants return `CommandError::Unsupported`. Applying `UnitCommand` to the sim is therefore blocked on plan 05's `command.rs` (not edited here — plan-05-owned). 15 contributes the 15→11 seam instead: `input::rts::command_units_apply(states, target, queue)` applies an implicit move / explicit attack target / queued waypoint to plan-11 `CommandAiState`s. Also, `SimCommand::UnitCommand` carries only `(units, command, x, y)` — no `target`/`queue` field — so an attack order's target id is not yet representable on the wire; `ActionBatcher` maps `CommandTarget::Unit(id)` to `(x = id, y = 0)` until plan 05/11 extends the variant (the `command_units_apply` adapter is lossless because it takes `CommandTarget` directly). Both are recorded for the orchestrator; no other plan file was changed.

#### 3.3.2 What stays local / what is predicted

- **Local-only, never relayed:** camera position/scale, focus/locks, cursor, `input.block`/`rotation`, `linePlans`/`selectPlans`/`lastSchematic`, selection sets, control groups, command rect drawing, plan mirrors, gesture state, keybind values.
- **Relayed:** every row in the `RemoteAction` table above, each stamped `(match_id, client_seq, tick)`; `ActionBatch` is append-only and ordered.
- **Prediction:** every relayed command is applied locally immediately (upstream `@Remote(called = Loc.both)` semantics). There is no rollback. Plan 21 owns the echo rule: a client skips its own `client_seq` rows when they arrive from the relay; foreign rows are applied in relay order. If plan 21's cheap validation drops a locally-applied command, that is a divergence; 21's desync detector/snapshot correction resolves it (flagged in §8 R4).
- **Shooting/aiming is not a command:** `player.shooting`/`mouseX/Y`/`boosting` travel with 21's player-input sync (upstream `NetClient` player snapshot). 15 writes them; 21 transports.

#### 3.3.3 Error surfacing

- `command_emit` returns `Result<ActionBatch, ActionError>`; validation failures (`ActionError::InvalidTarget`, `NoUnits`, `NotBuilder`, `RateLimited`) are logged with `[W]` and surfaced to 14 as a toast only when the action came from an explicit user gesture (click/release), not from a per-frame path.
- `Sim::command` `CommandError` (wrong team, out of range, hash mismatch) is logged once per action, never panics (05 §6.4).
- Plan 21 rejection events (21 § command relay) map to the same user-visible toast through `MindInput::action_rejected`; the sim side is 21's responsibility.

### 3.4 Focus and lock semantics

Port `locked()` and every upstream guard into `input/focus.rs`:

| Upstream guard | Port |
|---|---|
| `inputLocks` (`renderer.isCutscene()`, `logicCutscene`, `addLock`) | `InputLocks { base: Vec<LockId>, mods: Vec<Boolp-equivalent> }`; `is_locked()`; `add_lock`/`remove_lock` (mods via 20) |
| `Core.scene.hasKeyboard()` | `FocusState.has_keyboard` (a text field owns focus) |
| `Core.scene.hasMouse()/hasMouse(x,y)` | `FocusState.has_mouse` / `point_over_ui(x,y)` (Godot `Control` hit test; Rust never queries Godot in core) |
| `Core.scene.hasDialog()` | `FocusState.has_dialog` |
| `Core.scene.hasField()` | `FocusState.has_field` (any focusable text widget, incl. IME) |
| `Core.scene.hasScroll()` | `FocusState.has_scroll` (pointer over a `ScrollContainer`) |
| `ui.chatfrag.shown()` / `consolefrag.shown()` | `FocusState.chat_shown` / `console_shown` |
| `ui.hudfrag.shown()`, `state.isMenu()`, `state.isEditor()` | passed from 14/05 read APIs; `editor-blocks-shown` setting branch preserved |
| Mobile `isRebuildSelecting()` / `isPlacing()` / `isBreaking()` | mode predicates on `InputState` |

Rules ported exactly: `locked()` gates camera/world mutations (block cleared when locked), not UI-only shortcuts; `input.update()` early-returns in menu/dialog for the placement branches; `updateState()` always runs.

### 3.5 `InputHandler` port (`client_input.rs`)

Fields mirror upstream names (snake_case): `block`, `override_line_rotation`, `rotation` (default 1), `dropping_item`, `item_deposit_cooldown`, `is_building` (default true), `build_was_auto_paused`, `was_shooting`, `controlled_type`, `recent_respawn_timer`, `last_schematic`, `line_plans`, `select_plans`, `last_plans`, `last_unit`, `spectating`, `selected_units`, `command_buildings`, `command_mode`, `command_rect`, `tapped_one`, `command_rect_x/y`, `control_groups: [Vec<EntityId>; 10]`, `place_mode`, `line` (`PlaceLine`), `result_plan`, `b_plan` (plan preview scratch).

Behaviors ported line-by-line (owner: `client_input.rs` unless noted):

- `update()`: spectate invalidation; `logicCutscene` camera lerp (delegated to camera); plan mirror sync (`player.unit().plans` ↔ `last_plans`, capacity seed on unit change); `player_plan_tree` rebuild; `player.typing`; `update_building(is_building)` (11's `BuilderComp`); locked → `block = None`; `player.selected_block/rotation`; `was_shooting`; `controlled_type` timer + auto-`unitControl` retry with `control_interval` (70 ticks) — emission via `RemoteAction::UnitControl`.
- `reset()`/`updateState()`: exact clearing rules (`command_buildings`, `selected_units`, control groups, `last_plans`, shooting, ping; menu branches in desktop/mobile subclasses).
- `getSyncedPlans(out)`: skips `breaking`; mobile subclass appends non-breaking `select_plans`.
- `flushPlans`/`flushPlansReverse`/`flushSelectPlans`: `valid_place(..., ignore_units = true)` → `plan.copy()` → `block.on_new_plan(copy)` → 11 `BuildQueue::add_build(copy)` → `player_plan_tree.insert(copy)`. Reverse iteration for `boost`-held releases.
- `removeSelection` (4 overloads): `try_break_block` per tile, `valid_break` select-plan accumulation, rect removal from `player.unit().plans` and `select_plans`, team `BlockPlan` removal + `RemoteAction::DeletePlans`.
- `rebuildArea`/`tryRepairDerelict`/`canRepairDerelict`: derelict repair plan creation (rotation + config copy).
- `getPlan` (size-aware overlap via `((size+1)%2)*tilesize/2` center), `planMatches`, `draw*` state feeds (no drawing here; §3.8).
- `tileX/tileY/rawTileX/rawTileY/tileAt`, `selectedBlock/isPlacing/isBreaking/isRebuildSelecting`, `mouseAngle`, `selectedUnit`/`selectedControlBuild` (`40f` radius + hitbox grow 6), tile-tapped/config/inventory coordination (`check_config_tap`, `tile_tapped`, `try_tap_player`, `can_tap_player`, mining predicates, item drop).
- `canShoot()`/`onConfigurable()`/`isDroppingItem()`/`canDepositItem()`.
- `add()`/`remove()` lifecycle: the Rust equivalent is `MindInput::attach(handler_kind)` rebuilding the 14 UI bridges and trees; `setInput` preserves `block` (mobile toggle).

### 3.6 `Placement` port

`input/placement.rs` is a direct port; all scratch buffers are reusable statics on the planner instance (no per-call allocation in steady state):

```rust
pub fn pathfind_line(world: &WorldView, conveyors: bool, pathfinding: bool, block: Option<BlockId>, from: TilePos, to: TilePos, out: &mut SmallVec<[TilePos; 128]>) -> bool;
pub fn normalize_line(from: TilePos, to: TilePos, out: &mut SmallVec<[TilePos; 128]>);
pub fn normalize_rectangle(from: TilePos, to: TilePos, block_size: i32, out: &mut SmallVec<[TilePos; 128]>);
pub fn upgrade_line(world: &WorldView, from: TilePos, to: TilePos, out: &mut SmallVec<[TilePos; 128]>);
pub fn calculate_nodes(world: &WorldView, team: TeamId, points: &mut SmallVec<[TilePos; 128]>, block: BlockId, rotation: i32, overlapper: impl Fn(TilePos, TilePos) -> bool);
pub fn calculate_bridges(plans: &mut SmallVec<[PlanCopy; 128]>, placer: &dyn BridgePlacer, has_junction: bool, avoid: impl Fn(BlockId) -> bool);
pub fn is_side_place(plans: &[PlanCopy]) -> bool;
pub fn normalize_area(...) -> NormalizeResult;   // x<=x2, y<=y2, rotation inferred
pub fn normalize_draw_area(...) -> NormalizeDrawResult;
```

Port notes (exact parity):

- `normalizeLine` snaps to the dominant axis; `normalizeRectangle` steps by `block_size`.
- `pathfindLine`: if `conveyors && settings.conveyorpathfinding` → `astar`; on failure → `normalizeLine`; else Bresenham no-diagonal.
- `astar`: node limit **1000**; cost/parents/closed maps; `tileHeuristic` = 20 when neighbor not `alwaysReplace`/replaceable or deep floor, 8 on a direction change vs the parent (`parents` lookup), else 1; distance heuristic Manhattan; d4 only; reconstruct and reverse. Needs `block: Option<BlockId>` (upstream reads `control.input.block`; parameterize).
- `upgradeLine`: walk `ChainedBuilding::next()` from the start build while same-axis target not reached; returns `pathfindLine(true, …)` on chain break (08's trait).
- `calculateNodes`: keep first/last + `valid_place` points, greedily jump to the furthest overlapping point (spacing for bridges/power nodes); used by 08/09.
- `calculateBridges`: the upstream DP with `conveyorCost = 3`, `junctionCost = 30`, `bridgeCost = 200`, `bridgeOverEmptyPenalty = 5`, `infCost = i32::MAX/2`, `2*N` arrays; `placeable` predicate incl. same-block tolerance and junction avoidance; backtrack assigns `BridgePlacer::apply_to_plans`; results reversed. `ItemBridgePlacer` sets `cur.block/other.block = bridge` and `other.config = Point2Relative(cur - other)`; `DirectionBridgePlacer` sets blocks only. Bridge spacing validity comes from 08's `ItemBridgeBlock::positions_valid` / `DirectionBridgeBlock::positions_valid`. Skip when `is_side_place`, empty, non-orthogonal, or bridge locked.
- `iterateLine` (`line.rs`): diagonal = `diagonal_placement` key, flipped by `swapdiagonal` when mobile or `block.swapDiagonalPlacement`; dispatch upgrade-line vs pathfind vs rectangle vs straight; end rotation from `ChainedBuilding` neighbor; `block.change_placement_path(points, rotation, diagonal)`; per-point rotation from next point / end rotation / previous point for conveyors; overlapping multiblock suppression via running rect; `overrideLineRotation` semantics.
- `updateLine`: builds `linePlans` with `block.next_config()`; when `blockreplace` setting: `block.get_replacement(plan, line_plans)` if unlocked then `block.handle_placement_line(line_plans)` (07).

### 3.7 Build-plan queue, mirror, preview, snapshots

- **Storage:** the authoritative queue is 11's `BuilderComp.plans` (`BuildQueue`); 15 never appends directly (upstream AGENTS rule). `addBuild` call-through: `BuildQueue::add_build(plan, front)` handles same-position replacement and construct-progress carry-over.
- **Mirror:** `last_plans` is resynced every frame from `player.unit().plans` exactly as `InputHandler.update()` does (including the "new unit with empty queue seeds from `last_plans`" rule).
- **Trees:** `PlanTree` is a direct port of `Arc QuadTree` with insertion-order iteration, used for `player_plan_tree`, `select_plan_tree`; `QueryEachable::find` fallback chain (`tree` → `line_plans` → `select_plans`) preserved for 07/16 (`all_plans`, `all_select_lines`, `all_render_plans_config`). Reuse plan 11's generic spatial port if it exports one; otherwise place `PlanQuadTree` here and note the duplicate.
- **Commit paths:** desktop release flush, mobile confirm button, schematic use (`selectPlans` from 12's `Schematics::to_plans`, `checkHidden`), rebuild area, plan move (`splan` removes + re-adds on release; `ctrl` opens 14's plan config).
- **Undo/replace:** upstream semantics only — same-position replacement, `splan` move, break-rect removal, `clear_building`; no separate undo stack (editor owns undo, 19).
- **Preview state (`PreviewState`, consumed by 16):** `block`, `rotation`, `place_mode`, cursor tiles/world, `line_plans`, `select_plans`, `last_placed`, `splan`, `command_rect`, `selected_units`, `command_buildings`, `target` marker, `valid` flags cached per plan (`cached_valid`), `anim_scale` values. Updated in place; no allocation growth after warmup.
- **Plan snapshot (21):** every 0.5 s (`plan_sync_time`), `get_synced_plans`, truncate to `maxPlayerPreviewPlans = 1000`, batch size `900/12 = 75`, `plan_group_id` increments; empty snapshot sent when the list is empty. Data shape in §6.5.
- **`input.lastPlans` rendering/undo** is 16's consumer; `drawOtherBuildPlans`/`drawBuildPlans` state (other players' previews) comes from 21's player rows, not local input.

### 3.8 Preview/rendering boundary (16)

15 produces `PreviewState` + `PlanMirror` + `SelectionState`; 16 renders `Layer.plans`, breaking overlay, schematic overlays, selection polygons (`drawCommand`/`drawUnitSelection`/rect fill), command target lines/queue markers (`drawCommanded*`), plan config top overlays, cursors (`SystemCursor.hand/target/drill/unload/repair`), and the place arrow (`drawArrow`). Text overlays (`width x height (area)`) use 14's label layer. Input sets `valid`/color inputs only. `17` fires `ShakeEvent`; camera consumes it.

### 3.9 RTS design

- **Selection state:** `selected_units: SmallVec<[EntityId; 128]>`, `command_buildings: SmallVec<[TilePos; 32]>`, `control_groups: [SmallVec<[EntityId; 64]>; 10]`, `last_ctrl_group`, `last_ctrl_group_select_ms`, `command_rect`, `command_rect_x/y`, `tapped_one`, `queue_command_mode`.
- **Queries (11's team trees):** `selected_command_unit(x,y)` = intersect 4-unit box, filter `is_commandable`, min by `dist - hit_size/2`; `selected_enemy_unit` over present non-player teams, filter not-in-fog; `selected_command_units(x,y,w,h,pred)`; `selected_command_buildings`; all reuse a scratch `Seq`.
- **Modes:** `command_mode` toggled by binding (tap or hold per `commandmodehold`, default true), disabled when `block != None`, when `block==None && can_boost && commandMode == boost key` (upstream guard), or when UI is a dialog/field; selected units pruned each frame (`!allow_command || !valid || team != player.team`).
- **Selection actions:** drag rect on select-release (`selectUnitsRect`, `tapped_one`, `multi_unit_select`); tap toggles unit/building (`tapCommandUnit`); double-tap selects all same type on screen (`selectTypedUnits`); `select_all_units` (with `select_across_screen`), `select_all_unit_transport` (payload carriers), `select_all_unit_factories` (commandable buildings).
- **Commands:** `commandTap(screen, queue)` resolves target (enemy build or enemy unit; else position), maps ids, chunks **200**, `final_batch` on the last chunk, emits `UnitCommand`/`UnitCommandQueue`; `command_buildings` emit `CommandBuilding`. Bindings: `command_queue` (middle mouse / axis-tap path), `cancel_orders` and `unit_stance_*`/`unit_command_*` are consumed by 14's command UI through `setUnitCommand`/`setUnitStance` emission (the binds live in `UnitCommand`/`UnitStance` content defs per 02/11).
- **Apply side (11):** `command_units_apply(world, units, target, queue, final_batch)` is the port of `InputHandler.java:310-408`: implicit move, target/queue assignment on `CommandAI`, `lastCommanded`, group `UnitGroup` per physics layer, `calculate_formation`, `queuedCommands` keyed by target — 11 owns this; 15 does not reimplement.
- **Control groups:** create with `create_control_group` held, `distinctcontrolgroups` default true; invalid-unit pruning; recall selects; double-tap (`< 400 ms`) centers camera on the group centroid.

### 3.10 UI/focus/render contract with 14

14 (unwritten) provides, and 15 consumes:

- `UiFocus` values per frame: `has_keyboard`, `has_mouse`, `has_dialog`, `has_field`, `has_scroll`, `chat_shown`, `console_shown`, `hud_shown`, plus editor flags. Delivery: 14's `Ui` autoload calls `MindInput.set_ui_focus(...)` (GDScript/Rust `#[func]`); default if 14 prefers polling: Rust probes `Viewport.gui_get_focus_owner()` + dialog group counter (`NEEDS USER DECISION` §8 U1).
- Widgets that call input APIs: `PlacementFragment`/HUD palette → `MindInput.select_block(name)`, `rotate(delta)`, `toggle_pause_building`, `clear_building`; block config/inventory/plan config fragments → 07 `configure()`/`BlockConfigFragment` equivalents (14) and `check_config_tap`; minimap widget → `MindInput.minimap_pan(tile)` / `minimap_zoom(amount)`; mobile buttons → `set_mobile_mode(...)`, `confirm_plans`, `toggle_command_mode`, `toggle_queue_mode`, `toggle_schematic`, `flip(x)`, `rotate_plans(dir)`, `save_schematic`; KeybindDialog → `MindInput.keybinds()` / `rebind(name, code)`.
- 15 provides setters/getters and never manipulates Godot widgets: `input.block`, `rotation`, modes, plan counts, action results all cross the boundary as data.

### 3.11 Camera

**Decision (HLP §8/OD6): custom Rust rig, not Phantom Camera.** `MindCamera2D` is driven by `camera_state.rs` pure math + `camera.rs` node glue:

- Port `Renderer` camera: `target_scale`/`camerascale` (lerp factor 0.1, snap at 0.001), `scaleCamera(amount)` = `target_scale *= amount/4 + 1`, clamp `[min_scale, max_scale]` (`min_zoom_in_game = min_zoom / minzoomingamemultiplier`, `max_zoom_in_game = maxzoom_multiplier * max_zoom`, base `min = 1.5`, `max = 6`), logical cutscene zoom via `min_zoom/max_zoom`; `camera.width = viewport_width / camerascale` (and height).
- Pan: WASD/axis axis normalized * `camSpeed = (boost ? 15 : 4.5) * frame_delta_60`; `pan` key or `mouseMove` key pans proportionally to cursor offset from center (`panScale = 0.005` clamped ±1); detached camera (`detach-camera` setting) moves the player toward the camera; `smoothcamera` follow lerp `0.08` else snap; game-over/win core pan; spectator follow; mobile drag pan (`camera width / viewport width` scale), clamp to `[-w/4, -h/4, world + w/4, world + h/4]`; mobile edge auto-pan (`edgePan = Scl.scl(60)`, `maxPanSpeed = 1.3`).
- Follow/lock: `panCamera(pos)`, `spectate(unit)`, `center_on_tile`, lock-to-unit mode for 14's unit HUD (double-click control group already centers).
- Zoom input gating ported from desktop/mobile `update()` (chat/console/scroll/rotate-placed/rotatable-placement conflicts).
- Shake: consume 17's `ShakeEvent { intensity, duration }`; port intensity clamp 0..100 * (`screenshake`/4) * 0.75, random direction per frame, reduction.
- Cutscene/landing: `logicCutscene` pan+zoom, launch/land zoom hook from 12's launch animator (`show_landing`/`show_launch` API).
- Minimap: 14 maps click/drag/scroll to `minimap_pan(tile)` (camera center) and `minimap_zoom(amount)`.

## 4. Port map

| Mindustry source | Target Rust module | Notes on adaptation |
|---|---|---|
| `input/Binding.java` | `mind_core::input::binding` + `mind-gdext::input::bindings` | Full constant table preserved (§6.1). `KeyBind.add` → `KeyBindTable` static registry; values live in `BindingState`, persisted by 04 settings under the bind name; `Axis` vs `Key` represented by `KeyKind`; mouse-button defaults allowed. `Binding.init()` forced-init becomes `BindingTable::init()` called at content/client init (mirrors `Vars.java:510`). |
| `input/PlaceMode.java` | `mind_core::input::place_mode` | Enum + mobile-only mode flags (`line_mode`, `schematic_mode`, `rebuild_mode`, `queue_command_mode`) in `MobileMode`. |
| `input/Placement.java` | `mind_core::input::placement` | All static scratch `Seq`s become per-planner buffers; `control.input.block` parameterized as `Option<BlockId>`; `Pools`/`Point2` → `TilePos`; DP arrays reused. 08 supplies `positions_valid`; 07 supplies `valid_place`/`can_replace`. |
| `input/InputHandler.java` | `mind_core::input::{client_input, focus, rts, command_emit, action, plan, line, input_log}` | `@Remote` methods become `RemoteAction` + `SimCommand` builders (§3.3); draw methods become `PreviewState` (§3.8); `ui.*`/`renderer.*`/`Call.*` reads become injected capability traits (`InputCaps`: `focus()`, `is_cutscene()`, `camera()`, `schematics()`, `config_ui()`). |
| `input/DesktopInput.java` | `mind_core::input::desktop` + `mind-gdext::input::desktop` | Decision tree (`pollInputPlayer`/`pollInputNoPlayer`, control-groups, possession, zoom gating, `splan`, cursor selection) in core; Godot key/mouse translation + cursor application in gdext. `Time.delta` → render-frame delta; `Time.millis` windows → monotonic clock. |
| `input/MobileInput.java` | `mind_core::input::mobile` + `mind-gdext::input::mobile` + `gesture.rs` | GestureDetector port; all mode logic in core; touch translation and 14's buttons in gdext. |
| Arc `GestureDetector` (used at `InputHandler.add()`: `new GestureDetector(20, 0.5f, 0.3f, 0.15f, this)`) | `mind-gdext::input::gesture` | Thresholds/constants ported (`tap_square=20`, `long_press=0.5 s`, `tap_count_interval=0.3 s`, `max_fling_delay=0.15 s`); tap/long-press/pan/zoom dispatch into core controllers. |
| `core/Control.java` (input parts) | `mind_core::input::client_input` (`create_player`/`set_input`/`update`/`update_state` order) + `mind-gdext::input::mod` | Handler selection desktop/mobile; `setInput` preserves `block`; menu key/pause/screenshot/fullscreen hooks stay in `MindInput`; `control.saves`/`sound`/`indicators` are other plans' (04/18/14). |
| `core/Renderer.java` (camera parts) | `mind_core::input::camera_state` + `mind-gdext::input::camera` | Scale/lerp/clamp/shake math pure in core; node transform in gdext. `isCutscene()`/`landScale` exposed for locks/UI. |
| `ui/Minimap.java` (widget input) | 14 widget + `MindInput::minimap_pan/zoom` | Click centers camera; drag pans; scroll zooms (upstream `renderer.minimap.zoomBy`). |
| `ui/fragments/PlanConfigFragment.java` | 14 UI + `MindInput` plan-config hooks | Visibility/selected-plan lifecycle in 14; input owns `splan`/`selectPlans` (`ctrl`-click, `isUsingSchematic`, `isDone` hide rule). |
| `ui/fragments/BlockConfigFragment.java` | 14 UI + `InputCaps::check_config_tap` | `isShown`/`getSelected`/`hasConfigMouse`/`showConfig`/`hideConfig` consumed by core predicates. |
| `ui/fragments/PlacementFragment.java` (palette keys) | 14 UI + `MindInput::select_block/rotate` | `Binding.blockSelect*` category/next/prev + number keys set `input.block`/category; input owns `rotation` and plans. |
| `game/Schematics.java` (`toPlans`/`create`/`place`/`rotate`/`flip`) | `12` `Schematics` + `mind_core::input::line` (`rotate_plans`/`flip_plans`/`schem_origin`) | 15 owns plan transform math (`rotatePlans`/`flipPlans`/`schemOriginX/Y`, mobile centroid origin); 12 owns `.msch` data/create/place. |
| `game/Schematic.java` | 12 | Consumed only. |
| `world/blocks/distribution/{ItemBridge,DirectionBridge,ChainedBuilding}.java` | 08 | `positions_valid`, `next()`, `junction_replacement`/`bridge_replacement` consumed by `placement.rs`. |
| `entities/comp/BuilderComp.java` (`addBuild`/`removeBuild`/`clearBuilding`/`updateBuilding`/`isBuilding`) | 11 `BuildQueue`/`BuilderComp` | 15 calls; never mutates plans directly. |
| `player` input sync (`NetClient.java:760-818`) | 21 | `player.shooting/mouse/selectedBlock/plans` + `clientPlanSnapshot`; 15 defines the data, 21 transports. |
| `ai/{UnitCommand,UnitStance}.java` (Binding fields) | 02/11 defs consume `BindingState` ids | Command/stance selection UI in 14; emission in 15. |
| `input/AGENTS.md` recipes | this plan | New keybind procedure preserved: add `KeyBindId` + bundle keys `keybind.<name>.name`/`category.<cat>.name`; new action → `RemoteAction` + deliberate relay choice. |

## 5. Milestones & task breakdown

Each milestone is verifiable headlessly and/or through the MCP rig; evidence goes in the Changelog.

**M0 — Bindings + raw input + focus + replay harness (no placement).**
- `binding.rs` full registry; settings persistence via 04; `MindInput` node; `RawEvent` translation for key/mouse/scroll/touch/magnify; `FocusState`/`InputLocks`; `input_log` format + `mind-headless input_replay` driver; `InputCaps` test double.
- Verify: `cargo test -p mind-core input::binding::tests::registry_matches_upstream`; `mind-headless run input_focus_guards`; rebind round-trip test; MCP: open game, press key, eval `get_input_state_json()` contains the key event, rebind persists across restart.

**M1 — Placement planner + plan queue + headless placement table.**
- `placement.rs`, `line.rs`, `plan.rs`, `client_input.rs` plan/selection state; call-through to 11 `BuildQueue`; `PreviewState` dump.
- Verify: `cargo test -p mind-core input::placement::tests::{normalize_line_axis, normalize_rectangle_steps, astar_detour, astar_limit_1000, upgrade_line_chain, bridge_dp_costs, bridge_side_place_skip, nodes_spacing}`; `mind-headless run placement_validation_table`; scenario `input_place_line_headless`.

**M2 — Desktop parity.**
- `desktop.rs` decision tree; cursor selection; schematic/rebuild select; `splan`; plan config; break rect; zoom gating; possession; control groups; command rect/tap/queue; `deletePlans`/`rebuild`/`tryRepairDerelict`.
- Verify: `cargo test -p mind-core input::desktop::tests::*` + MCP scenario §7c-1..3; state-diff golden for a recorded desktop session.

**M3 — Mobile parity.**
- `gesture.rs`, `mobile.rs`; auto-pan; line/schematic/rebuild; confirm; payload target; manual shooting/autotarget; keyboard-mode branches; mobile UI callback API for 14.
- Verify: `cargo test -p mind-core input::mobile::tests::{longpress_line, tap_commit, pan_shifts_plans, zoom_gesture, edge_pan_clamp, doubletap_mine}`; mobile MCP scenario with synthetic touch events.

**M4 — RTS end-to-end + relay hooks.**
- `rts.rs`, `command_emit.rs`; `SimCommand` extension request merged with 05/11; `command_units_apply` verified against 11; stances/commands emission; control groups; queue mode.
- Verify: `mind-headless run input_rts_move`; `replay input_rts.simlog --checksum-every 60`; MCP scenario §7c-4; relay stub from 21 (or `Sim::command` when 21 absent).

**M5 — Camera rig + minimap + shake.**
- `camera_state.rs`/`camera.rs`; pan/zoom/follow/detach/cutscene/clamp/smooth; minimap pointer; `ShakeEvent` consumer.
- Verify: unit tests for scale/pan/clamp math; MCP pan/zoom/center scenario; screenshot diff against golden framing (tile center + zoom).

**M6 — Plan snapshots + input sync + polish + perf.**
- `getSyncedPlans` + 0.5 s snapshot batches for 21; player-input sync payload; `PreviewState` handoff to 16; cursor types; alloc audit; budgets.
- Verify: §7d benches; `input_replay` checksums; zero steady-state preview allocations; 16 consumes `PreviewState` without shape changes.

**M7 — Hardening + docs.**
- Bundle keys for all keybinds; mod lock API (`addLock`) via 20; cursor/error toasts contract with 14; Android devicelab notes for 22.
- Verify: exit checklist §7e complete.

## 6. Data & formats

### 6.1 Binding registry (parity ABI; names are settings keys and bundle keys)

Defaults below are the upstream `Binding.java` values. `Axis` nodes store key names; unset is allowed. Bundle keys: `keybind.<name>.name`, `category.<category>.name`.

| Category | Names (defaults) |
|---|---|
| `general` | `move_x` (Axis a/d), `move_y` (Axis s/w), `mouse_move` (mouseBack), `pan` (mouseForward), `boost` (shiftLeft), `respawn` (v), `control` (controlLeft), `select` (mouseLeft), `deselect` (mouseRight), `break_block` (mouseRight), `pickupCargo` (leftBracket), `dropCargo` (rightBracket), `clear_building` (q), `pause_building` (e), `rotate` (Axis scroll), `rotateplaced` (r), `diagonal_placement` (controlLeft), `pick` (mouseMiddle), `ping` (p), `rebuild_select` (b), `schematic_select` (f), `schematic_flip_x` (z), `schematic_flip_y` (x), `schematic_menu` (t) |
| `command` | `command_mode` (shiftLeft), `command_queue` (mouseMiddle), `create_control_group` (controlLeft), `select_all_units` (g), `select_all_unit_factories` (h), `select_all_unit_transport` (unset), `select_across_screen` (altLeft), `cancel_orders` (unset), `unit_stance_hold_fire`/`unit_stance_pursue_target`/`unit_stance_patrol`/`unit_stance_ram`/`unit_stance_boost`/`unit_stance_hold_position` (unset), `unit_command_move`/`unit_command_repair`/`unit_command_rebuild`/`unit_command_assist`/`unit_command_mine`/`unit_command_enter_payload`/`unit_command_load_units`/`unit_command_load_blocks`/`unit_command_unload_payload`/`unit_command_loop_payload` (unset) |
| `blocks` | `category_prev` (comma), `category_next` (period), `block_select_left`/`right`/`up`/`down` (arrows), `block_select_01`..`_09` (num1..9), `block_select_10` (num0) |
| `view` | `zoom` (Axis scroll), `detach_camera` (unset), `teleport_cursor` (unset), `menu` (back on Android else escape), `fullscreen` (f11), `pause` (space), `skip_wave` (unset), `minimap` (m), `research` (j), `planet_map` (n), `block_info` (f1), `toggle_menus` (c), `screenshot` (f12), `toggle_power_lines` (f5), `toggle_block_status` (f6) |
| `multiplayer` | `player_list` (tab), `chat` (enter), `chat_history_prev`/`_next` (up/down), `chat_scroll` (Axis scroll), `chat_mode` (tab), `console` (f8) |
| `debug` (no category) | `debug_hitboxes` (unset), `performance_metrics` (unset) |

Settings keys consumed (04 owns persistence; defaults noted where upstream declares them):
`conveyorpathfinding` (default true), `blockreplace` (true), `buildautopause` (false), `doubletapmine` (false), `commandmodehold` (true), `distinctcontrolgroups` (true, read with `true` fallback), `smoothcamera` (true), `detach-camera` (false), `swapdiagonal` (false; mobile default UI-checked), `keyboard` (false; mobile), `hints`, `screenshake`, `minzoomingamemultiplier`, `maxzoomingamemultiplier`, `backgroundpause`, `editor-blocks-shown` (19), `drawhitboxes`, `showperformance`, `blockstatus`, `lasersopacity`, `preferredlaseropacity`.

### 6.2 Input event log (`scenarios/input_*.events.jsonl`; format 1)

```json
{"format":1,"header":{"build":"...","map":"...","seed":123,"mobile":false,"width":1920,"height":1080}}
{"tick":7,"ev":{"t":"KeyDown","code":"a"}}
{"tick":7,"ev":{"t":"MouseMove","x":640.0,"y":360.0}}
{"tick":51,"ev":{"t":"MouseButton","button":"left","down":true}}
{"tick":80,"ev":{"t":"Scroll","x":0.0,"y":1.0}}
{"tick":95,"ev":{"t":"TouchDown","pointer":0,"x":100.0,"y":200.0}}
{"tick":120,"ev":{"t":"Magnify","factor":1.1}}
```

Event types: `KeyDown`, `KeyUp`, `MouseMove`, `MouseButton`, `Scroll`, `TouchDown`, `TouchUp`, `TouchMove`, `Magnify`, `Resize`, `UiFocus`, `Action` (raw semantic action for UI-driven events: `select_block`, `rotate`, `confirm`, ...). Replay: events applied in order before the sim tick with the same number; commands emitted at that tick enter the command log. This format is the deterministic substitute for missing upstream input unit tests (§7a).

### 6.3 Input state dump (`get_input_state_json()` / `mind-headless input_dump`; append-only to plan 00 §6.3)

```json
{
  "format": 1, "tick": 1234, "mobile": false, "mode": "none", "block": "conveyor", "rotation": 0,
  "is_building": true, "command_mode": false, "queue_mode": false,
  "cursor": {"tile": [20, 20], "raw": [21, 21]},
  "line_plans": [{"x":20,"y":20,"rot":0,"block":"conveyor","valid":true}],
  "select_plans": [], "player_plans_mirror": 3,
  "selected_units": [101, 102], "command_buildings": [], "command_rect": null,
  "control_groups": [[], [], []],
  "locks": ["cutscene"], "focus": {"keyboard": false, "mouse": false, "dialog": false, "field": false},
  "emitted_commands": [{"tick":1234,"kind":"Place","x":20,"y":20,"block":"conveyor"}]
}
```

### 6.4 `RemoteAction` / `ActionBatch`

```rust
pub enum RemoteAction {
    Rotate { x: i16, y: i16, direction: bool },
    Configure { x: i16, y: i16, value: ConfigValue },
    DeletePlans { positions: SmallVec<[i32; 32]> },
    CommandUnits { units: SmallVec<[i32; 32]>, target: CommandTarget, queue: bool, final_batch: bool },
    SetUnitCommand { units: SmallVec<[i32; 32]>, command: u16 },
    SetUnitStance { units: SmallVec<[i32; 32]>, stance: u16, enabled: bool },
    CommandBuilding { positions: SmallVec<[i32; 32]>, x: f32, y: f32 },
    Inventory { kind: InventoryKind, x: i16, y: i16, item: Option<ItemId>, amount: i32, angle: f32 },
    Payload { kind: PayloadAction, x: f32, y: f32, target: Option<i32> },
    UnitControl { unit: Option<i32> },
    UnitClear,
    BuildingControlSelect { x: i16, y: i16 },
    Ping { x: f32, y: f32, text: Option<SmolStr> },
}
pub struct ActionBatch { pub match_id: u64, pub client_seq: u32, pub tick: u64, pub commands: SmallVec<[SimCommand; 4]> }
```

Chunk limits ported: unit ids **200** per batch; delete positions passed whole (upstream sends the array; cap at 256 with consecutive emits if larger); plan snapshots **75** plans per chunk, max **1000** plans, 0.5 s cadence.

### 6.5 Plan snapshot payload (15 → 21)

```
PlanSnapshot { plan_group_id: u32, player: Identity/PlayerId, plans: Option<Vec<PlanWire>> }
PlanWire { x: i16, y: i16, rotation: i8, block: u16, config: TypeIOValue (Number|Bool|Content only), breaking: bool }
```

`TypeIO` plan encoding follows 04; only `Number|Boolean|Content` in network plan queues (07 §6.3). `breaking` plans are filtered by `getSyncedPlans`; `selectPlans` are included only on mobile (upstream `MobileInput.getSyncedPlans`).

### 6.6 Ported constants (single `input::constants` module)

`max_length = 100`, `max_schematic_size = 64` (Vars), `max_player_preview_plans = 1000`, `plan_snapshot_interval = 0.5 s`, `plan_snapshot_chunk = 75`, `command_chunk = 200`, `astar_node_limit = 1000`, bridge DP `{conveyor:3, junction:30, bridge:200, empty_penalty:5}`, `player_select_range = 11 desktop / 17 mobile`, `unit_select_rad_scl = 1.0`, `control_group_double_tap_ms = 400`, `mine_double_tap_ms = 500`, payload hold tick `20 ms` / tap guard `200 ms`, control interval `70 ticks`, `pan_scale = 0.005`, `pan_speed = 4.5`, `pan_boost_speed = 15`, `zoom_step = amount/4 + 1`, `camera_lerp = 0.1`, `camera_snap = 0.001`, `smooth_camera_lerp = 0.08`, base zoom `min 1.5 / max 6`, game zoom `min 0.5 / max 6`, desktop build hint margins, mobile `edge_pan = scl(60)`, `max_pan_speed = 1.3`, `line_mode_start_tiles = 3`, `area_break_start_tiles = 2`, gesture `{tap_square:20, long_press:0.5, tap_count:0.3, fling_delay:0.15}`, bridge/hint constants.

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests / deterministic input-log replay

Upstream has **no unit tests covering `mindustry.input`** (the `tests/` tree has `ApplicationTests` placement-adjacent cases only). The substitute oracle is deterministic input-log replay plus focused algorithm tests; each is a `cargo test -p mind-core` name:

| Upstream anchor | Rust test | Notes |
|---|---|---|
| (none; derived from `Placement.normalizeLine/normalizeRectangle/upgradeLine`) | `input::placement::tests::{normalize_line_axis, normalize_rectangle_steps, upgrade_line_chain, upgrade_line_fallback}` | golden point lists |
| (none; `Placement.astar`) | `input::placement::tests::{astar_straight, astar_detour, astar_replaceable_preference, astar_node_limit_1000, astar_unreachable_falls_back}` | fixture grid; asserts exact path + node cap |
| (none; `Placement.calculateBridges`) | `input::placement::tests::{bridge_dp_costs, bridge_side_place_skip, bridge_config_rewrite_point2, direction_bridge_no_config}` | uses 08 stub blocks |
| `ApplicationTests.blockOverlapRemoved` | `input::plan::tests::overlapping_plan_replaced` | same-position `addBuild` replacement |
| `ApplicationTests.multiblock` (placement half) | `input::placement::tests::multiblock_line_suppression` | 2x2/3x3 stepping |
| (none; desktop flow) | `input::replay::tests::{desktop_place_line, desktop_break_rect, desktop_schematic_flip_rotate, desktop_plan_move, desktop_control_groups, desktop_command_rect}` | replay `.events.jsonl` + state diff |
| (none; mobile flow) | `input::replay::tests::{mobile_longpress_line, mobile_confirm_commit, mobile_pan_shift_plans, mobile_zoom, mobile_edge_pan, mobile_payload_target}` | same |
| (none; emission) | `input::command_emit::tests::{chunk_200_final_batch, delete_plans_positions, rotate_block_direction, inventory_kinds, payload_kinds, rotate_plans_math, flip_plans_math}` | |
| (none; focus) | `input::focus::tests::{locked_gates_camera_only, menu_clears_state, dialog_blocks_placement, chat_blocks_zoom}` | |
| `ApplicationTests.inventoryDeposit` (input half) | `input::desktop::tests::deposit_cooldown_gate` | `can_deposit_item` |

### 7b. Headless harness scenarios (`mind-headless`)

| Scenario | Steps / assertions |
|---|---|
| `placement_validation_table` | fixture 64×64 map (flat + water + ore + walls + enemy core); golden JSON table of `(block, x, y, rot, team, floor/env, expected valid_place/valid_break/ignore_units)` for ~120 rows: floor/overlay, deep water, `canPlaceOn`, `requiresWater`, block limits, enemy core radius (`placeRangeCheck`), same-block same-rot, replacement chains, derelict, darkness; asserts exact match (this is 15's placement oracle shared with 07). |
| `input_place_line_headless` | apply synthetic placement `RemoteAction`s (line start→end on both axes + diagonal + rectangle + conveyors with `conveyorpathfinding` on/off) via `input::replay`; assert plan counts, rotations, bridge plans/config, resulting `SimCommand`s, and post-construction checksum after `dev_construct_all`. |
| `input_replay_desktop` / `input_replay_mobile` | run `.events.jsonl` through `InputReplay` with a fixed camera/world; dump `get_input_state_json` + command log; compare to committed goldens; then `Sim` apply and compare checksums. |
| `input_rts_move` | spawn 30 units (11 harness), replay rect-select + move/queue/stance/control-group events; assert selection sets, emitted chunking (`final_batch` on last chunk), formation groups assigned per layer, unit target positions, and 600-tick checksum. |
| `input_schematic_transform` | synthetic 5x5 schematic (12 fixture): rotate cw/ccw, flip x/y, shift; assert plan coordinates/config/rotation vs golden (ports `rotatePlans`/`flipPlans`). |
| `input_plan_snapshot` | 1200 plans + mobile select plans: assert truncation to 1000, 75-plan chunks, empty-snapshot behavior, non-breaking filter. |

### 7c. MCP playtest scenarios (open-godot-mcp)

Preconditions: `tools/build.sh`; repo playtest skill; `godot_health check` (if `BRIDGE_NOT_CONNECTED`, launch editor per skill); always pid-stamp evals. Node paths are the plan-00 spine (`/root/Spine/SimHost`, `/root/Spine/World/Camera2D`) plus `/root/Spine/Input`; adapt if 00 renames.

1. **Place a conveyor line via drag (desktop).**
   - Launch `res://scenes/spine.tscn`; eval `SimHost.select_block("conveyor")` (palette API) and `SimHost.set_paused(false)`.
   - Eval `Camera2D.tile_to_screen(20, 20)` → `A`; `tile_to_screen(30, 20)` → `B`; `godot_input sequence` with `mouse_button left down` at `A`, `mouse_motion` in 5 steps to `B`, screenshot mid-drag.
   - Assert mid-drag: `Input.get_input_state_json().line_plans | length == 11`, all `valid: true`, `mode == "placing"`; capture screenshot `place_line_preview.png`.
   - `mouse_button left up`; assert `SimHost.get_build_plans() >= 11` (all non-breaking), `get_tile_json(20,20).block != "air"` after `dev_step_ticks(240)` (construction), rotation 0 for `x=20..30,y=20`; screenshot `place_line_done.png`.
   - Toggle `conveyorpathfinding` on, drag a diagonal from `(20,22)` to `(30,27)` with `diagonal_placement` held; assert path is axis-aligned (A* result has no diagonal steps) and endpoint reached.
2. **Rotate.**
   - `select_block("copper-wall")`; wheel event (`godot_input mouse_button wheel` / `Scroll`) → `Input.rotation() == (previous+1)%4`; with a rotatable block selected, `r` (`rotateplaced`) over an existing placed block triggers `Rotate` command (assert `emitted_commands` + `get_tile_json.rot`).
   - Hold `diagonal_placement` + wheel while placing → rotation changes instead of zoom (upstream gating); release → wheel zooms (`Camera2D.zoom()` changes).
3. **Break.**
   - Right-click-drag a rect over the line; release; assert tiles are `air` (or construct-deconstructing then `air` after ticks), `get_builds()` decreased, `emitted_commands` contains `Break`s; screenshot `break_rect.png`.
   - `schematic_select` (`f`) + right-drag creates a schematic plan overlay (assert `last_schematic` non-null, `select_plans` non-empty) — full select path with 12 installed.
4. **RTS select + move.**
   - Use 11's `MindUnits.spawn_unit("dagger", 0, x, y)` ×6 around `(40,40)`.
   - Hold `command_mode` (shift; `commandmodehold=true`); left-drag rect over the units; assert `Input.selected_unit_count() == 6`, screenshot selection rings `rts_select.png`.
   - Right-click at tile `(60,45)` (or `command_queue` + click for queue); assert `emitted_commands` has one `UnitCommand`/`UnitCommandQueue` with `final_batch: true`, units' `command` is move, and after `dev_step_ticks(180)` unit positions moved toward target; screenshot `rts_move.png`.
   - Control group: ctrl+`1` creates group; `1` recalls; double-`1` centers camera (assert camera position ≈ centroid).
5. **Camera + minimap + shake.** WASD pan, wheel zoom, `center_on_tile(64,64)`, minimap click (14 widget) pans; eval `17` shake event changes camera offset and returns to baseline; assert zoom clamp at min/max; screenshots.

### 7d. Performance budget & measurement

| Metric | Budget | Method |
|---|---|---|
| Input event → `PreviewState` update, 100-tile straight line | P95 ≤ 0.5 ms | `mind-headless bench input_line --len 100` (replay harness, excludes rendering) |
| Conveyor A* placement line (≤1000 nodes) | P95 ≤ 2.0 ms | `bench input_astar` (worst-case maze) |
| `calculate_bridges` 100-plan line | P95 ≤ 0.5 ms | `bench input_bridges` |
| Large drag-select (1000 units in view) | P95 ≤ 1.5 ms | `bench input_select --units 1000` (11 tree intersect + filter) |
| Command emission 200 units incl. local `Sim::command` | P95 ≤ 1.0 ms | `bench input_command --units 200` |
| Idle per-frame input cost | P95 ≤ 0.2 ms, zero steady-state allocations | `bench input_idle` + alloc audit (plan 00 CI flag) |
| Input-log replay determinism | 10k ticks, identical command log + checksums across runs/processes | `mind-headless replay input_replay_desktop.simlog` ×4 |
| In-engine input+Pump frame slice | ≤ 2 ms P95 at 1000 plans/500 units | `godot_profiler` around `MindInput._process`; screenshots |

Budgets recorded in `bench/baselines.json`; plan 23 owns the CI gate. Regressions block the P5 gate (HLP §5).

### 7e. Exit criteria checklist

- [x] Binding registry matches `Binding.java` exactly (names, defaults, categories, axis-ness); bundle keys exist; rebinding persists via 04 and survives restart. *(M0: 88 binds, `registry_matches_upstream` + `rebind_round_trip_through_settings`; `user://keybinds.json` restart path is MCP/§7c.)*
- [ ] Desktop and mobile handlers both reachable at runtime (`setInput` preserves `block`); `locked()`/focus guards match upstream branch-for-branch.
- [x] Line/rectangle/diagonal/upgrade/conveyor-A*/bridge placement match goldens; `conveyorpathfinding`, `blockreplace`, `swapdiagonal`, `diagonal_placement` settings honored. *(M1: `placement::tests::*` + goldens `placement_validation_table`/`input_place_line_headless`; settings plumbing is `InputCaps`.)*
- [ ] Build queue add/replace/flush/reverse/remove/rebuild/derelict-repair writes only through 11's `BuildQueue`; no direct plan appends. *(M1 seam: `input::queue::BuildQueue`; 11's real `BuilderComp` call-through is the documented open handoff.)*
- [ ] Plan snapshot (0.5 s, 75/chunk, 1000 cap) + mobile select-plan inclusion verified.
- [ ] RTS selection (rect/tap/typed/all/transport/factories), queue mode, control groups, double-tap center, and stance/command emission verified; `command_units_apply` matches 11's `UnitGroup` formation behavior.
- [x] Every `@Remote` action mapped to a `RemoteAction`/`SimCommand` (or documented local-only); `SimCommand` extension request delivered to 05 (or `Custom` fallback + shim). *(M2: `action::RemoteAction` + `command_emit::ActionBatcher`; §3.3.1 variants added to `determinism::command` with codec tests.)*
- [ ] Camera math matches `Renderer`/`InputHandler` numbers; Phantom Camera decision recorded; follow/lock/detach/cutscene/minimap/shake hooks wired.
- [ ] `PreviewState` consumed by 16 with no drawing in `mind-gdext`/`mind-core` input; 14 UI drives input only through the `MindInput` API.
- [ ] All §7a tests green; §7b scenarios pass with goldens; §7c MCP steps recorded with screenshot paths; §7d budgets met.
- [ ] D1 boundary greps clean (`use godot` absent from `mind-core`); ported files carry GPL headers; no C#.

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Needs user? |
|---|---|---|---|
| R1 | `14`, `12`, `16`, `21` were **not on disk** when this plan was authored. Interfaces proposed: `UiFocus`, `Schematic::to_plans`, `PreviewState`, relay payloads. | Implement against §3.10/§3.7; re-verify names at each sibling's kickoff; no sibling edits from 15. | Orchestrator reconcile (not blocking) |
| R2 | `SimCommand` lacks the non-placement player actions (§3.3.1). | Request additive variants in 05 (same as 11 §6.5); fallback `Custom` kinds + shim. | **U2** (which layer owns the additions: 05 enum vs 21 wire type; default 05) |
| R3 | Phantom Camera vs custom rig. | **Custom Rust rig** adopted (§3.11); addon kept installed but unused; revisit only if dev-time cutscene tweens are wanted (non-gameplay). | no (HLP §8/OD6 policy) |
| R4 | D2 has no authoritative sim: locally applied commands that 21's cheap validation drops cause divergence. | Clients apply immediately (upstream parity); 21 owns rejection events + desync correction; 15 surfaces rejection toasts and never retries silently. | Reconcile with 21 at execution |
| R5 | `UiFocus` delivery (GDScript push vs Rust probe). | 14's UI autoload calls `MindInput.set_ui_focus(...)`; Rust probe used only as fallback for editor/standalone. | **U1** (default stated; 14 must agree) |
| R6 | `Arc QuadTree` port fidelity (iteration order feeds `find`). | Port exact Arc semantics with insertion-order iteration; reuse 11's spatial port if it exports a generic variant; golden test. | no |
| R7 | Mobile gesture parity cannot be fully validated on the Windows/WSL dev host (no touch device). | Exact constants ported + synthetic touch replay; real-device check deferred to 22 (OD5). | no |
| R8 | Input timing windows use client monotonic clock (I3). | Accepted: input is local-only; replay uses logged ticks; sim never reads it. | no |
| R9 | `Menu` key/back handling on Android vs desktop, fullscreen/screenshot/pause bindings split between `Control` and input. | `MindInput` owns them, calling 14/22 APIs; parity table in §4. | no |
| R10 | Screen shake source. | 17 fires `ShakeEvent`; camera consumes; no shake math in 15 beyond the `Renderer` port. | no |
| R11 | Cursors (`SystemCursor`) on platforms without system cursors (mobile/web). | Map to `MouseShape`/hidden cursor; mobile ignores system cursor calls (as upstream). | no |
| R12 | Control groups are session-local (not saved). | Reset on world load/`ResetEvent`; matches upstream. | no |
| R13 | `input.update()` per render frame vs sim tick (I2). | Render-frame sampling, tick-stamped commands; deterministic replay via input log. | no |

**U1 — `UiFocus` contract with 14:** default 14 pushes focus flags to `MindInput`; alternative is a Rust probe of Godot focus/`Control` state. Pick before 14 lands.
**U2 — `SimCommand` action-variant ownership:** default extend 05's enum at M4 with the §3.3.1 list; alternative is a 21-owned wire command type plus `SimCommand` translation. Pick before 21's schema freezes.
**U3 — Schematic dependency timing:** if 12 is late, ship `Placement`/desktop flows against a `SchematicProvider` trait and land schematic select/rebuild as the only `#[ignore = "plan 12"]` tests.

## 9. References

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 decisions, §2 arch, §3 row 15, §4 template, §5 gate, §8 addon policy, §9–10), `PRELIMINARY_PLAN.md`.
- Sibling plans read: `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.5 spine classes, §3.10 extension contract, §7c, OD-R10/R12), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3 file list, §6.4 `SimCommand`, §6.6 constants, §7.2), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§3.1 `WorldGrid`/tile accessors, §7c camera step), `07_BLOCKS_BUILD_IMPLEMENTATION_PLAN.md` (§2.4 deferral of input to 15, §3.4/§3.6/§3.9/§3.11), `08_LOGISTICS_IMPLEMENTATION_PLAN.md` (§2.5 ownership of `calculateBridges` to 15, §3.6 bridge validity), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (§3.5/§3.8 RTS runtime, §3.13 Godot/STDB surfaces, §6.5 `SimCommand` additions).
- Mindustry AGENTS: `core/src/mindustry/input/AGENTS.md`, `core/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `ai/AGENTS.md`, `ui/AGENTS.md`, `maps/AGENTS.md`, `core/src/mindustry/AGENTS.md`, `Mindustry/AGENTS.md`, `tests/AGENTS.md`, `net/AGENTS.md` (player sync), `world/blocks/AGENTS.md` (bridges/conveyors).
- Mindustry code: `input/{Binding,InputHandler,DesktopInput,MobileInput,Placement,PlaceMode}.java`; `core/{Control,Renderer,NetClient}.java`; `game/{Schematic,Schematics}.java`; `ui/Minimap.java`, `ui/fragments/{PlanConfigFragment,BlockConfigFragment,PlacementFragment}.java`; `world/blocks/distribution/{ItemBridge,DirectionBridge,ChainedBuilding}.java`; `ai/{UnitCommand,UnitStance}.java`; `entities/comp/{BuilderComp,UnitComp}.java` (skimmed via 11/07); `Vars.java:107,189,203,379,510`; `tests/src/test/java/ApplicationTests.java` (placement-adjacent cases).
- Addon: `C:\Users\Clinton\g\code_examples\phantom-camera\README.md`, `addons/phantom_camera/plugin.cfg` (v0.11.0.1) and `mindustry-godot/client/addons/phantom_camera/plugin.cfg` (v0.11.0.3); no `rts-framework` checkout exists under `code_examples` (evaluated by absence; Mindustry's RTS model is bespoke and ported directly).
- Skills: `godot-compositor-testing`, `playtest` (MCP recipes; plan 00 creates the repo-local analog).

## Changelog

- 2026-10-01 — Plan written (draft v1). No implementation started. Open: U1 (`UiFocus` delivery), U2 (`SimCommand` action-variant ownership), U3 (schematic dependency timing). Reconciliation items R1/R4 for the orchestrator (14/12/16/21).
- 2026-10-03 — **M0–M2 landed on `lane/15-input`** (base `main` @ `c86bb9c`; commits `777cf97`, `835d290`, `8707381`; no conflicts — additive files only).
  - **M0 (`777cf97`)** — `mind-core::input::{binding,place_mode,focus,input_log,caps}`: the full `Binding.java` registry (88 binds, exact names/defaults/categories/axis-ness + `keybind.<name>.name` bundle keys), `BindingState` seeded from defaults and persisted through plan-04 `SettingsStore`, `MobileMode`/`PlaceMode`, `InputLocks`/`FocusState` guards, `format:1` JSONL `InputLog`/`InputReplay`, and the `InputCaps` test double. `mind-gdext::input::{mod,events,bindings}` adds the `MindInput` node at `/root/Spine/Input` (declared in `spine.tscn`), Godot→`RawEvent` translation and `user://keybinds.json` persistence. `mind-headless input dump|replay|scenario` + `input_focus_guards`. Evidence: **mind-core 1345 passed / 2 ignored**; workspace `clippy -D warnings` + `fmt --check` clean; goldens `tests/golden/input/{keybinds,focus_guards,focus_guards_replay}.json` verified by `--golden`.
  - **M1 (`835d290`)** — `mind-core::input::{placement,line,plan,queue}`: `normalize_line/rectangle/area/draw_area`, conveyor A* (node limit 1000, deterministic `(f,pos)` heap), `calculate_nodes` spacing, `calculate_bridges` DP + `RelativeBridgePlacer`/`DirectionBridgePlacer`, `upgrade_line` over the `ChainedBuilding` seam, `iterate_line`/`PlaceLine` + `rotate_plans`/`flip_plans`, `ClientPlan`/`PlanCopy`/`PlanTree`/`PlanMirror`/`PreviewState`, and the minimal `BuildQueue` seam over plan 11. Scenarios `placement_validation_table` (64 rows across 8 blocks) and `input_place_line_headless` (4 planner cases) with committed goldens. Evidence: **mind-core 1363 passed / 2 ignored**; `cargo check -p mind-headless` clean.
  - **M2 (`8707381`)** — `mind-core::input::{action,command_emit,client_input,desktop,rts,replay}`: `RemoteAction` (all §3.3 rows, `relayed()` split), `ActionBatcher` (200-unit batches, 256-position delete cap, monotonic `client_seq`), `InputState`, `DesktopController` (scroll/zoom gating, drag line, break rect, control groups + `<400 ms` double-tap, command rect/tap), pure `rts` selection helpers and the `ReplayHarness`. `determinism::command` gained the eight §3.3.1 `SimCommand` variants (`Rotate`, `DeletePlans`, `CommandBuilding`, `Inventory`, `Payload`, `UnitControl`, `UnitClear`, `BuildingControlSelect`) with binary codec + round-trip tests. Evidence: **mind-core 1385 passed / 2 ignored** (53 `input::*`); `mind-headless 76 lib + 4 integration`; workspace `clippy -D warnings` + `fmt --check` clean.
  - **Deferred (explicit):** M3 mobile (`gesture`/`mobile.rs`), M4 RTS end-to-end/relay + `command_units_apply` against 11, M5 camera rig/minimap/shake, M6 plan snapshots/alloc audit, M7 hardening/bundle keys; real plan-11 `BuilderComp` call-through (the `input::queue::BuildQueue` seam stands in); plan-07 `Block.changePlacementPath`/`next_config`/`get_replacement` line hooks (`LineHooks` seam); plan-12 `Schematic`/`Schematics`; all §7c MCP steps + `tools/mcp-smoke` (single-editor mutex); §7d benches. **Orchestrator reconciliation:** §3.3.1 `SimCommand` variants are additive to plan 05's `command.rs` (U2 default executed); `input::queue` documents the plan-11 handoff; `UiFocus` still defaults to plan-14 push (U1).
- 2026-10-03 — **M3 complete + M4 partial landed on `lane/f19-15`** (commit `ae7448e`; base `main` @ `e1fd7bc`; no conflicts — additive within `mind-core/mind-headless/mind-gdext` input modules).
  - **M3 (mobile parity)** — `mind-core::input::mobile`: added `check_targets` (20 px enemy autotarget + building fallback), `use_schematic`/`confirm_schematic`/`schem_origin` (centroid), `rebuild_area` (caller-supplied plan-07 replacement closure), `get_plan`/`has_plan`/`remove_plan`/`check_overlap_placement`, `update_transitions` (command/line/placing/schematic/rebuild mode transitions), `camera_move`, and `keyboard`-mode gating for `pan`/`zoom`; `handle_gesture` now begins selecting only on an existing plan, starts schematic/rebuild select on touch-down, enables keyboard shoot-on-touch, and removes an existing plan before re-queuing. `InputCaps` gained `mobile_keyboard()`; `GestureEvent::name()`. `mind-gdext::input` wires `MobileInputBridge` into `MindInput` and exposes the plan-14 callback API (`mobile_state_json`/`mobile_mode_json`/`set_mobile_block`/`set_mobile_keyboard`/`mobile_rotate`/`mobile_flip`/`mobile_rotate_plans`/`mobile_toggle_{command,queue,schematic}`/`mobile_clear_select_plans`/`mobile_select_plan_count`/`mobile_touch_{down,drag,up}`/`mobile_tick`/`mobile_camera_move`/`mobile_reset`), removing the WIP dead-code. Tests: `input::mobile::tests::{autotarget,schematic_use_and_origin,plan_remove_and_overlap,rebuild_area_queues_replacements,keyboard_gates_pan_zoom,touch_down_selects_existing_plan,tap_removes_existing_plan}` alongside the §5 names (`longpress_line`, `tap_commit`, `pan_shifts_plans`, `zoom_gesture`, `edge_pan_clamp`, `doubletap_mine`). Scenarios: `input_mobile_parity` (schematic transform/confirm, rebuild, autotarget, confirm commit, keyboard gating) and `input_replay_mobile` with committed goldens; new `mind-headless/tests/input_golden.rs` locks all six input scenarios (`input_focus_guards`, `placement_validation_table`, `input_place_line_headless`, `input_replay_mobile`, `input_mobile_parity`, `input_rts_move`).
  - **M4 (RTS e2e + relay hooks, partial)** — `mind-core::input::rts::command_units_apply(states, target, queue)` is the documented 15→11 seam (the `InputHandler.java:310-408` loop plan 11 did not ship): implicit move / explicit `AttackTarget` / queued `CommandQueueEntry` per `CommandAiState`. `input_rts_move` scenario drives selection → move/attack/queue commands → `ActionBatcher` (200-chunk, `final_batch`) → `SetUnitCommand`/`SetUnitStance` → control groups → `command_units_apply`, with a deterministic `Hasher` checksum; `input::replay::tests::rts_move_replay_checksum` asserts a stable checksum and match-id binding. **Remaining:** applying the unit-command `SimCommand`s through `Sim::command` is blocked because plan 05 applies only the P0 subset (see the §3.3.1 reconciliation); the §7c MCP scenario step stays deferred to the single-editor mutex. **Evidence:** `cargo fmt --all` clean; workspace `cargo clippy --all-targets -- -D warnings` clean; `cargo test -p mind-core` **1456 lib + 3 `blocks_golden` + 5 `combat_golden` + 2 `sim_core_determinism` + 2 `sim_core_meta` + 1 `sim_core_schedule` = 1469 passed / 3 ignored** (74 `input::*`); `cargo test -p mind-headless` **78 lib + 1 `audio_golden` + 2 `fx_golden` + 1 `input_golden` + 1 `logistics_golden` = 83 passed**; golden checksums `input_mobile_parity` → `5be03d592a93bd84`, `input_rts_move` → `c04e75fe90e02c08`.
