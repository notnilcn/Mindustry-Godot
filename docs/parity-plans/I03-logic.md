<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I03 — Logic: unit control, sensors, turrets, radar, processor links, the logic editor, displays, canvases

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan. Also read
`WT/client/rust/mind-core/src/logic/AGENTS.md` once.

This plan carries everything H06 (round H logic) did not reach: H06 T9-T19.
H06's task text pointed at "raw material" in round-G plans that are now
archived and must not be opened. This plan replaces that material with the
Java transcriptions you need, so it is self-contained. Everything not quoted
here, read straight from `J/` at the line numbers given.

Ledger items closed by this plan: PAR-0221 (ubind/ucontrol), PAR-0224
(completionLogicCode), PAR-0218 (sensor/fetch), PAR-0223 (turret control),
PAR-0222 (radar), PAR-0220 (processor links), PAR-0103 (logic editor),
PAR-0105 (displays), PAR-0225 (canvas).

---

## 1. State on `main` (verified 2026-10-15 @ `b560e9b1`) — done, do not redo

| H06 task | Commit | What exists now |
|---|---|---|
| T1 live processor seams | `0248a281` | `#[func] SimHost.logic_set_code(x,y,code)`, `logic_get_code(x,y)`, `logic_get_state(x,y)` (vars, links, counter), `logic_last_error()` |
| T20 map processors resolve constants | `32621e87` | globals installed before buildings load |
| T2 `spawnwave` live | `7ef440cb` | `game::runtime::run_wave_now` |
| T3 cutscenes | `b73cc911` | `MindInput.set_logic_cutscene_lock`, `SimHost.logic_cutscene_json` |
| T5 `setrule` tracking | `9a2b77dd` | `run_set_rule` marks `rules_world_dirty` |
| T4 messages + `localeprint` | `47917b7c` | `message` instr -> HUD; `LogicText` resource (bundle, map locales, `iconc`) |
| T6 `setprop` | `c6af4808` | set (not add) semantics |
| T7 conformance suite | `0d8a92c5` | `core/sim/logic_campaign_tests.rs` (GZ/Frozen Forest/Frontier programs); item-turret `maxAmmo` default 30 |
| T8 world side effects | `3c4284c2` | spawn/status/explosion/effect/sound/setblock/getblock/weather apply live |

Facts you will need (all re-verified on `main`):

* `core/logic/executor/mod.rs`: `Instruction::UnitBind` arm at ~1307,
  `Instruction::UnitControl` ~1329, `Instruction::UnitLocate` ~1363,
  `Instruction::Sensor` ~1175, `Instruction::DrawFlush` ~1165. `exec.unit` is the
  `@unit` var, created with `put_obj_const` (`core/logic/assembler.rs:46`), so
  `exec.is_constant(unit_var)` is true and today `ubind` NEVER writes it.
  `LVar::set_const` exists (`core/logic/value.rs`).
* `core/logic/executor/unit_control.rs` (679 lines): `logic_unit_control_enabled`
  :28, `logic_unit_build_enabled` :33, `logic_unit_deconstruct_enabled` :38,
  `check_logic_ai` :58, `refresh_control_timer` :95, `bind_next` :106 (sorts by
  `Entity::index`), `apply_control` :140 (only Idle/Stop/Move/Approach/Pathfind/
  AutoPathfind/Boost/Unbind do anything; every other command returns `true` and
  does nothing, lines 191-203), `run_ulocate` :274.
* `core/ai/types/logic.rs`: `LogicAi` component (fields `unit, building, timeout,
  controlled, control, target_pos, target_id, item_target, item_drop_target,
  item_amount, flag, boost, moving, ...`), `update_logic(ctx, unit, &mut state) -> bool` :213.
* `core/sim/unit_runtime.rs` `unit_update_system`: the controller chain
  (`slot.kind == AiKind::Player` ~2521 ... `AiKind::NoAi` ~3093, then the generic
  `else`). There is NO `AiKind::Logic` branch, so logic units run the generic AI.
* `core/game/map_objectives.rs:357`:
  `let _ = run_logic_script(code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false);`
  (a throwaway `World`). `core/logic/script.rs:50`:
  `pub fn run_logic_script_in(code: &str, max_instructions: u32, loop_: bool, world: &mut bevy_ecs::world::World) -> Option<Executor>`.
* Turrets (`core/world/blocks/defense/turrets/mod.rs`): `TurretState.logic_control_time`
  :354 (init `-1.0` at :455), `logic_shooting` :356. Line ~1433:
  `state.is_shooting = config.always_shooting || state.target.is_some() || state.logic_shooting;`
  (wrong per Java). ~1547: `let can_shoot = if state.logic_control_time > 0.0 { ... state.logic_shooting ...`.
  Nothing ever sets `logic_control_time`. `core/logic/access.rs` knows
  `LAccess::Shoot`/`Shootp` (:95-115) but `control` never routes them to the turret.
* `core/logic/executor/radar.rs` `find`: `key < current` when `sort_order == true`
  (minimizes; Java maximizes), no range filter, `Attacker`/`Boss` -> false, no
  cache, no permission check.
* `core/logic/blocks/logic_block.rs` `configured` (:652-711):
  `ConfigValue::Number(n)` sets the ICON TAG (Java: `Integer` = link toggle,
  `Character` = icon tag); `ConfigValue::Point2`/`Building` toggle links
  (Java has no such config; keep them as the internal form of a toggle).
  `toggle_link` :239 does not re-enable the unlinked target.
* `gdext/input/desktop.rs:3203`: `let text = matches!(def.kind, BlockKind::LogicBlock | BlockKind::MessageBlock);`
  gives processors a one-line text-field config panel (Java: one pencil button).
* `trait BuildingBehavior` (`core/world/behavior/mod.rs`): `logic_sense` :194,
  `logic_control` :206, `on_configure_build_tapped` :227 (H02 T6 landed it; the
  generic `world::config_tap::config_tap` entry and `SimCommand::ConfigTap` exist).
* Displays: `core/logic/blocks/display.rs` `LogicDisplayState { root_display: Some(e) always, color, stroke, commands: VecDeque<u64>, transform, operations, index, processing, display_size }`;
  `core/logic/blocks/io.rs:319` `flush_draw` appends to the TARGET's own queue
  (Java: tileable displays forward to `rootDisplay`) and nothing ever drains it, so
  after 1024 commands every further `drawflush` is dropped. `core/logic/executor/draw.rs`
  has `pack_cmd`/`cmd_*` unpackers, `DISPLAY_DRAW_TYPE = 30`, and the image path
  packs only `30` without `rootDisplay.index << 5` (:252-255).
  `#[func] SimHost.logic_display_commands(x,y)` (`gdext/sim_host.rs:1868`) is a
  placeholder that returns an empty array. There is no display renderer in `gdext/render/`.
* Canvas: `core/logic/blocks/canvas.rs` `CanvasBlockState::new(canvas_size, bits_per_pixel)`,
  `get_pixel(x, y)`, `set_pixel(x, y, color)` (x/y form; Java uses a linear `pos`);
  `core/logic/blocks/io.rs` has no `CanvasBlock` arm, so `read`/`write` on a canvas do nothing.
* Overlays: `gdext/sim_host.rs:6740` `enum ConfigOverlay` has a `Logic` variant with
  no fields; `gdext/render/overlay_top.rs:203` draws nothing for it.
  `enum SelectOverlay` (:6768) has `MessageText` and `LogicTag` with no fields;
  `overlay_top.rs:273` draws nothing.
* Editor: `MindLogic` (`/root/Spine/MindLogic`, `gdext/logic.rs`) `#[func]`s:
  `statement_count`, `statement_names`, `statement_fields(name)`,
  `statement_meta(name)`, `metadata_handoff`, `parse(source, privileged)`,
  `compile(source, privileged)`, `run(...)`, `default_program`.
  `core/logic/statement.rs` `StatementMeta { registered_name, rust_variant, fields, category, privileged, use_wrapping, hidden }`,
  `StatementField { name, kind: FieldKind }`. No `logic` dialog exists in
  `ui/dialogs/`; `ui/dialogs_manifest.json` has no `logic` entry; but
  `core/ui/manifest.rs:405` already lists `"logic"` among pausing dialogs.
* Styles that exist in `ui/styles_manifest.json`: text buttons `logict`, `flatt`,
  `logicTogglet`; image buttons `logici`, `cleari`; fields `nodeField`, `nodeArea`.

---

## 2. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5 -> T6 -> T12 -> T13 -> T14 -> T7 -> T8 -> T9 -> T10 -> T11.
(Sim tasks first, since they are short and verifiable. The editor is the largest
block, and its tasks build on each other.)

### T1 — `ubind` / `ucontrol` / `ulocate` drive live units through LogicAI (Fixes PAR-0221)

**Symptom.** In-engine 2026-10-14: a world processor
`ubind @poly` / `set u @unit` / `ucontrol move 100 60 0 0 0` leaves `u` null and the
poly does not move (its controller stays `CommandAI`).

**Acceptance.** (a) `ubind @poly` binds the team's polys in spawn order, one per
call, wrapping around. (b) `ubind <unit object>` binds that unit. (c)
`ucontrol move 100 60 0 0 0` swaps the unit's controller to LogicAI and moves it to
world (800,480). (d) 600 ticks after the processor stops issuing unit
commands, the unit returns to its type's default controller. (e) `within`,
`getBlock`, `itemTake`, `itemDrop`, `payTake`, `payDrop`, `mine`, `build`,
`deconstruct`, `target`, `targetp`, `boost`, `flag`, `stop`, `unbind` behave as in
the Java below.

**Java.**
* `UnitBindI.run` (`J/logic/LExecutor.java:180-208`):
  ```java
  if(!exec.privileged && !state.rules.logicUnitControl) return;
  if(exec.binds == null || exec.binds.length != content.units().size) exec.binds = new int[content.units().size];
  if(type.obj() instanceof UnitType type && type.logicControllable){
      Seq<Unit> seq = exec.team.data().unitCache(type);
      if(seq != null && seq.any()){
          exec.binds[type.id] %= seq.size;
          if(exec.binds[type.id] < seq.size) exec.unit.setconst(seq.get(exec.binds[type.id]));
          exec.binds[type.id] ++;
      }else exec.unit.setconst(null);
  }else if(type.obj() instanceof Unit u && (u.team == exec.team || exec.privileged) && u.type.logicControllable){
      exec.unit.setconst(u);
  }else exec.unit.setconst(null);
  ```
  `unitCache(type)` is the team's units of that type in insertion order
  (= spawn order = port `EntitySeq` order).
* `UnitControlI.checkLogicAI` (`:323-341`): valid unit, `exec.unit.obj() == unit`,
  `(unit.team == exec.team || exec.privileged)`, `unit.controller().isLogicControllable()`.
  If the controller is already a `LogicAI`, set `la.controller = exec.thisv.building()`
  and return it. Else, if `control` is true, install a NEW `LogicAI` with
  `controller = exec.thisv.building()`, then `unit.mineTile = null; unit.clearBuilding();`.
  Otherwise return null.
* `UnitControlI.run` (`:344-551`), structure:
  ```java
  if(!exec.privileged && !state.rules.logicUnitControl) return;
  boolean control = type != unbind && type != within;
  LogicAI ai = checkLogicAI(exec, unitObj, control);
  if(unitObj instanceof Unit unit && (ai != null || !control)){
      if(ai != null) ai.controlTimer = LogicAI.logicControlTimeout;   // 600
      float x1 = World.unconv(p1.numf()), y1 = World.unconv(p2.numf()), d1 = World.unconv(p3.numf());  // *8
      switch(type){ ... }
  }
  ```
  Per command (read `:355-548` for the full text):
  * `idle, autoPathfind`: `ai.control = type`.
  * `move, stop, approach, pathfind`: `ai.control = type; ai.moveX = x1; ai.moveY = y1;`
    `approach` also sets `ai.moveRad = d1`. `stop` also clears `mineTile` and building.
  * `unbind`: if the controller is a `LogicAI`, `unit.resetController()`.
  * `within`: `p4 = unit.within(x1, y1, d1) ? 1 : 0`.
  * `target`: `ai.posTarget.set(x1, y1); ai.aimControl = target; ai.mainTarget = null; ai.shoot = p3.bool();`
  * `targetp`: `ai.aimControl = targetp; ai.mainTarget = p1.obj() instanceof Teamc t ? t : null; ai.shoot = p2.bool();`
  * `boost`: `ai.boost = p1.bool()`. `flag`: `unit.flag = p1.num()`.
  * `mine`: `tile = world.tileWorld(x1, y1); if(unit.canMine()) unit.mineTile = unit.validMine(tile) ? tile : null;`
  * `payDrop`: gated by `exec.timeoutDone(unit, LogicAI.transferDelay)` (90 ticks);
    if the unit has a payload, drop it at the unit position; `exec.updateTimeout(unit)`.
  * `payTake`: same timeout gate. `p1.bool()` true: closest own-team unit within
    `unit.type.hitSize * 2` with `u != unit && u.isAI() && u.isGrounded() && pay.canPickup(u) && u.within(unit, u.hitSize + unit.hitSize * 1.2f)` -> pick it up.
    False: the building under the unit (`world.buildWorld(unit.x, unit.y)`), own team:
    if it holds a payload the unit can take, take that payload; else if
    `buildVisibility != hidden && build.canPickup() && pay.canPickup(build)` pick up
    the whole building. Then `exec.updateTimeout(unit)`.
  * `payEnter`: building under the unit, same team, `build.canControlSelect(unit)` ->
    the unit enters it (`unitBuildingControlSelect`).
  * `build`: `(rules.logicUnitBuild || privileged) && unit.canBuild() && p3 is Block && block.canBeBuilt() && (block.unlockedNow() || unit.team.isAI())`;
    `x = World.toTile(x1 - block.offset/8)`, `y` likewise, `rot = Mathf.mod(p4.numi(), 4)`;
    reset `ai.plan` progress when x/y/block changed or `unit.plans` is empty; set the plan
    (config = p5 if Content/Building); `unit.clearBuilding()`; add the plan unless the tile
    already holds that block with that rotation (`unit.updateBuilding = true; unit.addBuild(ai.plan)`).
  * `deconstruct`: `(rules.logicUnitDeconstruct || privileged) && unit.canBuild()`; a breaking
    plan at `World.toTile(x1), World.toTile(y1)`, added when `Build.validBreak(unit.team, x, y)`.
  * `getBlock`: `range = max(unit.range(), unit.type.buildRange)`; out of range or no tile ->
    p3/p4/p5 null; else p3 = tile block, p4 = building or null, p5 = overlay if not air else floor.
  * `itemDrop`: `p1 == @air` clears the unit's item (no timeout gate). Otherwise
    timeout-gated; `dropped = min(stack.amount, p2)`; the target building must be
    own team, valid, `allowDeposit()`, within `logicItemTransferRange + size*8/2`
    (`logicItemTransferRange = 45f`, `J/Vars.java`, grep it); `accepted = build.acceptStack(item, dropped, unit)`;
    transfer `accepted`; `exec.updateTimeout(unit)`.
  * `itemTake`: timeout-gated; own team, valid, has items, `p2` is an Item, same range;
    `taken = min(build.items.get(item), min(p3, unit.maxAccepted(item)))`; take it;
    `exec.updateTimeout(unit)`.
* `UnitLocateI.run` (`:220-302`): gated like ubind; uses `checkLogicAI(exec, unitObj, true)`;
  sets `ai.controlTimer`; per-instruction cache `ai.execCache.get(this)`; recomputes only when
  `ai.checkTargetTimer(this)` (the radars set is cleared every 40 ticks); outputs are in TILES
  (`World.conv` = /8); `building` uses `indexer.getEnemy`/`getFlagged`; `ore` uses
  `indexer.findClosestOre(unit, item)`; `spawn` the closest spawner; `damaged` uses
  `Units.findDamagedTile`; `outBuild` is set only when the building is within
  `max(unit.range(), buildingRange)` or is own team.
* `LogicAI` (`J/ai/types/LogicAI.java`, 159 lines, read it whole):
  `transferDelay = 90`, `logicControlTimeout = 600`. `updateMovement()`:
  ```java
  if(targetTimer > 0f) targetTimer -= Time.delta; else { radars.clear(); targetTimer = 40f; }
  if(controlTimer > 0 && controller != null && controller.isValid()) controlTimer -= Time.delta;
  else { unit.resetController(); return; }
  switch(control){
      case move -> moveTo(Tmp.v1.set(moveX, moveY), 1f, 30f);
      case approach -> moveTo(Tmp.v1.set(moveX, moveY), moveRad - 7f, 7, true, null);
      case pathfind -> flying ? moveTo(.., 1f, 30f) : controlPath.getPathPosition(unit, target) then moveTo(result.dest, 1f, epsilonEquals(dest, 4.1f) ? 30f : 0f);
      case autoPathfind -> (see :84-107: move toward the closest enemy core/spawner, stop inside dropZoneRadius+120 of a spawner for the wave team)
      case stop -> unit.clearBuilding();
  }
  if(unit.type.canBoost && !unit.type.flying){ shouldBoost = boost || unit.onSolid() || (unit.isFlying() && !unit.canLand()); unit.elevation = approachDelta(elevation, num(shouldBoost), shouldBoost ? riseSpeed : descentSpeed); }
  if(!shoot || !unit.type.omniMovement) unit.lookAt(unit.prefRotation());
  else if(unit.hasWeapons() && mounts.length > 0 && !mounts[0].weapon.ignoreRotation) unit.lookAt(mounts[0].aimX, mounts[0].aimY);
  ```
  `shouldShoot() = shoot && !(unit.type.canBoost && boost)`; `target(...)` returns
  `posTarget` for `aimControl == target`, `mainTarget` for `targetp`, else null;
  `retarget()` true, `invalid()` false, `checkTarget()` false, `keepState()` true.
  `resetController()` sets the controller back to `type.createController(unit)`.

**Port today.** See §1. In addition, `LogicAi` stores `target_id: i32` (an object id)
instead of an entity, and has no `exec_cache`, `radars`, `plan`, `main_target`,
`pos_target`, `aim_control`, `shoot` or `move_rad` fields.

**Changes (in this order).**
1. `core/ai/types/logic.rs`: reshape `LogicAi` to Java's fields (keep the
   `#[derive(Component)]`):
   `control: LUnitControl` (default `Idle`), `move_x, move_y, move_rad: f32`,
   `control_timer: f32` (init `LOGIC_TIMEOUT_TICKS` = 600), `target_timer: f32`,
   `controller: Option<Entity>` (the processor building), `plan: BuildPlan`,
   `exec_cache: IndexMap<u32, LocateCache>` (key = the instruction's index in its
   executor's program; `LocateCache { x: f32, y: f32, found: bool, build: Option<Entity> }`),
   `radars: IndexSet<u32>` (instruction keys seen since the last 40-tick clear),
   `aim_control: LUnitControl` (default `Stop`), `boost: bool`,
   `main_target: Option<Entity>`, `shoot: bool`, `pos_target: (f32, f32)`.
   Delete fields Java does not have. Fix every compile error by mapping old uses to
   the new names (grep `LogicAi` and `logic::LogicAi` across `core/`). Keep
   `check_target_timer(key) -> bool` (`radars.insert(key)`). Add
   `pub const TRANSFER_DELAY_TICKS: f32 = 90.0;`.
2. `core/ai/types/logic.rs` `update_logic(ctx, unit, &mut state) -> bool`: rewrite
   to the Java `updateMovement` above. Return `false` when the timeout or the
   controller check fails (the caller resets the controller). Use the AI movement
   helpers the other controllers use (`grep -n "fn move_to" core/ai/`; `ctx.pathfind`
   for `autoPathfind`; `ControlPathfinder` `get_path_position` for `pathfind`:
   `grep -rn "fn get_path_position\|fn path_position" core/ai/`). For targeting, set
   the unit's weapon targets the way `update_ai_weapons_for` does (H01 T2), with
   `should_shoot = state.shoot && !(def.can_boost && state.boost)` and the target
   from `aim_control`. If `update_ai_weapons_for` cannot take an explicit target,
   add a parameter `explicit_target: Option<AimTarget>` rather than a second copy.
3. `core/sim/unit_runtime.rs` `unit_update_system`: add exactly one branch
   `} else if slot.kind == AiKind::Logic {` before the generic `else`. Copy the shape of
   the `AiKind::Boost` branch (`AiCtx` construction). Take the `LogicAi` component
   out of the entity, call `crate::ai::types::logic::update_logic(&mut ctx, entity, &mut state)`,
   and re-insert it. On `false`: remove `LogicAi` and set
   `slot.kind = crate::ai::controller_registry::select_controller(<the unit's def>, ...)`
   (`grep -n "pub fn select_controller" core/ai/controller_registry.rs` for its signature;
   it is Java's `type.createController`). If `AiKind::Logic` does not exist yet, add
   it to the `AiKind` enum (append it; never reorder) and to any save codec of
   controller kinds (`grep -rn "AiKind::Command =>" core/io core/sim` and add the next
   free byte; H04 T9 made the kind byte append-only).
4. `core/logic/executor/unit_control.rs`:
   * `bind_next(world, type_id, team, cursor)`: sort by `(EntitySeq, entity.index())`
     (`grep -rn "struct EntitySeq" core/` for the component), filter
     `team == exec.team && def.logic_controllable.unwrap_or(true)` (check the
     default in `J/type/UnitType.java`: `logicControllable = true`) and alive. Apply
     Java's `%=` / `++` exactly.
   * `check_logic_ai(world, team, privileged, unit, bound, control, controller_building)`:
     add the `controller_building: Option<Entity>` argument (the executor's
     `exec.build`). Implement Java's `isLogicControllable()` exactly:
     `UnitController` default false (`J/entities/units/UnitController.java:19-21`);
     every `AIController` true (`J/entities/units/AIController.java:82-84`);
     `CommandAI` = `!hasCommand()` = `targetPos == null`
     (`J/ai/types/CommandAI.java:108-110,562-564`), i.e. a commandable unit with a
     move/attack order is NOT logic-controllable until its order is cleared; the player
     (`PlayerComp.java:174-176`) false. Install a
     fresh `LogicAi` with `controller = controller_building` and clear the unit's
     mine tile and build plans when `control` is true.
   * `apply_control(world, unit, type, params: &[LVar], exec: &mut Executor) -> ()`:
     implement EVERY command from the Java list above. Outputs (`within` -> p4;
     `getBlock` -> p3/p4/p5) are written through the executor arena
     (`exec.arena.get_mut(var_id).set_num(..)` / `.set_obj(..)`); pass the `VarRef`s
     in, not cloned values. For the item and payload commands use the same core
     functions the player input uses:
     `grep -rn "pub fn transfer_item\|pub fn take_items\|pub fn accept_stack\|pub fn pickup_payload\|pub fn drop_payload\|fn picked_build_payload\|fn picked_unit_payload" core/`.
     If a payload helper takes a player instead of a unit entity, add a
     unit-entity variant next to it and make the player one call it. The
     timeout gate is the executor's per-unit timeout map: Java `exec.timeoutDone(unit, delay)` /
     `exec.updateTimeout(unit)` (`grep -n "timeoutDone\|updateTimeout" J/logic/LExecutor.java`
     for the 6-line bodies). Add `unit_timeouts: IndexMap<Entity, u64>` (tick of the
     last transfer) to `Executor` if no equivalent exists (`grep -n "timeout" core/logic/executor/mod.rs`).
   * `run_ulocate`: outputs in tiles (`/ 8.0`), cache keyed by instruction index,
     recompute only when `check_target_timer(key)` is true, and the `outBuild` range rule.
5. `core/logic/executor/mod.rs`:
   * `Instruction::UnitBind`: the Java gate first
     (`if !exec.privileged && !logic_unit_control_enabled(rules_ref(world)) { return; }`),
     resize `exec.binds` to the unit content count on every run like Java, then write
     with `exec.arena.get_mut(exec.unit).set_const(value)` (NOT `set_obj`, and do not skip
     constants). `LogicObject::Unit(u)` binds when `(team == exec.team || privileged) && controllable`.
   * `Instruction::UnitControl`: Java gate (`!exec.privileged && !logic_unit_control` -> return,
     replacing the current gate that ignores `privileged`); `control = type != Unbind && type != Within`;
     `check_logic_ai(.., control, exec.build)`; then `apply_control`.
   * `Instruction::UnitLocate`: same gate.

**Tests** (`core/sim/logic_campaign_tests.rs`, world-processor programs over a live sim;
copy the T7/T8 helpers there):
* `ubind_type_cycles_in_spawn_order`: spawn 3 team-1 polys; a world processor runs
  `ubind @poly` + `sensor id @unit @id` four times; the ids are p1, p2, p3, p1.
* `ubind_unit_object`: `spawn @dagger 10 10 0 @sharded u` then `ubind u` -> `@unit` is `u`.
* `ubind_gate`: a non-privileged micro processor with `logicUnitControl = false` -> `@unit` null.
* `ucontrol_move_reaches_target`: dagger spawned at world (40,40) with no command;
  program `ubind @dagger` + `ucontrol move 10 10 0 0 0` (ucontrol takes TILE units, so the
  target is world (80,80)); after 300 ticks the dagger is within 16 world units of (80,80)
  and its `ControllerSlot.kind` is `AiKind::Logic`.
* `commanded_unit_not_logic_controllable`: give the dagger a CommandAI move order first
  (`targetPos` set) -> `ucontrol move` does nothing (Java `CommandAI.isLogicControllable`).
* `logic_ai_times_out`: as above, then replace the code with `stop`; after 601 ticks the
  controller kind is the type default and `LogicAi` is gone.
* `ucontrol_within_and_getblock`: `ucontrol within 10 10 2 r` -> `r == 1` near (10,10);
  `ucontrol getBlock 10 10 t b f` -> `t` is the block at (10,10), `f` its floor/ore.
* `ucontrol_item_take_and_drop`: a mono-type unit next to a container with copper:
  `itemTake c @copper 10` -> unit stack copper 10 (or its capacity); `itemDrop core 10`
  -> the core gains it; a second call within 90 ticks does nothing.
* `ucontrol_build_places_plan`: a poly; `ucontrol build 12 12 @copper-wall 0 0` with
  `logicUnitBuild = true` -> after enough ticks a team wall at (12,12).
* `ulocate_building_core`: `ulocate building core false @copper x y f b` -> tile coords
  of the own core, `f == 1`.

**Verify in-engine.** Ground Zero (`I-environment` §4.1, `--import` boot). Then:
```bash
$G --eval 'return get_node("/root/Spine/SimHost").content_list("unit").find("poly")'      # id P
$G --eval 'return get_node("/root/Spine/SimHost").apply_sim_command_json("{\"op\":\"spawn_unit\",\"unit\":P,\"team\":1,\"px\":1080,\"py\":480}")'
$G --eval 'return get_node("/root/Spine/SimHost").place_block(118, 50, "world-processor")'
$G --eval 'return get_node("/root/Spine/SimHost").logic_set_code(118, 50, "ubind @poly\nucontrol move 100 60 0 0 0\nset u @unit")'
$G --eval 'get_node("/root/Spine/SimHost").set_paused(true); get_node("/root/Spine/SimHost").step(300); return get_node("/root/Spine/SimHost").unit_debug_json()'
```
Expect the poly's kind `Logic` and its position within 16 px of (800,480).
`logic_get_state(118,50)` shows `u` = the poly. Then `logic_set_code(118,50,"stop")`,
`step(601)` -> kind back to the poly's default. Screenshot
`runs/<date>-s<N>-I03-T1/poly.png`.

**Pitfalls.** `unit_update_system` is the hottest shared file: add exactly one branch.
`@unit` must stay a constant for `set`/`op`; only `ubind` writes it (with `set_const`).
Never iterate a `HashMap` for the bind order.

**Commit.** `Fixes PAR-0221: ubind/ucontrol/ulocate bind and drive live units through LogicAI`

---

### T2 — Objective `completionLogicCode` runs in the live world (Fixes PAR-0224)

**Acceptance.** An objective with `completionLogicCode = "setflag \"x\" true"` sets
flag `x` in the live world when it completes. No shipped map uses it, so this is
headless only.

**Java.** `J/game/MapObjectives.java:213` (`completed()` path) calls
`LExecutor.runLogicScript(completionLogicCode)`; `J/logic/LExecutor.java:73-94`
`runLogicScript`: a privileged executor over the LIVE world, at most 100000
instructions, no loop.

**Changes.**
1. `core/game/map_objectives.rs`: add `pub pending_logic_scripts: Vec<String>` to the
   objectives runtime struct that owns `complete` (grep the struct around :357). In the
   completion path replace `let _ = run_logic_script(code, ...)` with
   `self.pending_logic_scripts.push(code.clone())`.
2. `core/sim/mod.rs` `Sim::drain_logic_events` (grep it): after the existing event loop,
   take the pending scripts out of `CampaignRuntime.session.objectives`
   (`std::mem::take`), DROP the resource borrow, then for each run
   `crate::logic::script::run_logic_script_in(&code, SCRIPT_DEFAULT_MAX_INSTRUCTIONS, false, &mut self.ecs.0)`.
   Then run the event drain once more (a script may emit world events).
3. Update the existing map_objectives test that asserted the old behaviour
   (`grep -n "completion_logic_code\|completionLogicCode" core/game/map_objectives.rs`).

**Tests.** `objective_completion_script_runs_live` (`core/sim/logic_campaign_tests.rs`):
live sim + campaign session + one objective whose completion is forced, with the code
above -> after one tick `logic_flag("x")` (or the flags set in the rules) contains `x`.

**Commit.** `Fixes PAR-0224: objective completionLogicCode runs against the live world`

---

### T3 — Sensor / fetch / printchar tables match Java (Fixes PAR-0218)

**In-engine evidence (2026-10-14):** world processor `fetch core c @sharded 0` -> null;
`fetch coreCount` -> 0; `fetch player` -> null; `sensor @dagger @health` -> 1;
`sensor @copper @name` -> 1; a core's `@name` -> "core-shard" and `@id` -> 19 (Java: null for both).

**Java.** Read these whole before coding:
* `BuildingComp.sense(LAccess)` / `senseObject(LAccess)` (`J/entities/comp/BuildingComp.java:2099-2170`).
  Building `@name` and `@id` are NOT in the table (they return null / NaN through the default).
* `UnitComp.sense` / `senseObject` (`J/entities/comp/UnitComp.java:266-353`).
* `Block.sense` (`J/world/Block.java:1674-1708`), `UnitType.sense` (`J/type/UnitType.java:1430-1449`),
  `Item.sense`/`Liquid.sense` (`grep -n "public double sense\|senseObject" J/type/Item.java J/type/Liquid.java`).
* Family overrides: `grep -rln "public double sense(LAccess" J/world/blocks` and
  `grep -rln "public Object senseObject(LAccess" J/world/blocks` list every block class that
  overrides; transcribe each one that exists in the port.
* `FetchI` (`J/logic/LExecutor.java`, `grep -n "class FetchI" -A60`): `player` ->
  `data.players.get(i).unit()` (or the unit's block tile building if it is a `BlockUnitc`);
  `playerCount` -> players size; `core` -> `data.cores.get(i)`; `coreCount`; `unit`/`unitCount`;
  `build`/`buildCount` filtered by block.
* `PrintCharI` (`grep -n "class PrintCharI" -A25 J/logic/LExecutor.java`):
  `if(exec.textBuffer.length() >= maxTextBuffer) return;` non-content objects append
  nothing; content appends `(char)content.emojiChar()` (the icon code from
  `assets/icons/icons.properties`; 0 when absent, and Java then appends U+0000).
* `GlobalVars.update` (`J/logic/GlobalVars.java`, `grep -n "clientLocale\|clientName"`):
  `@clientLocale = player.locale()`, `@clientName = player.name()`.

**Port today / rows to fix.**
1. Content sense (`core/logic/access.rs`, the `LogicObject::Content(c)` arm near :268/:306)
   returns `Sensed::Num(1.0)` for every property except `@id`. Implement the Block,
   UnitType, Item and Liquid tables: `@name` -> `Str(name)`, `@color`, `@health`/`@maxHealth`,
   `@armor`, `@size`, `@itemCapacity`, `@liquidCapacity`, `@powerCapacity`, `@solid`,
   `@flying`, `@speed`, `@range`, `@payloadCapacity` (only the rows each Java `sense`
   has; everything else NaN -> the port's "null" sensed value). Values come from the
   `LogicContentInfo` resource (`core/logic/globals.rs:586`); extend it at install time
   with the unit/item/liquid scalars you need. `sense(Block, Item)` returns
   `requirement * rules.buildCostMultiplier` (0 with `infiniteResources`).
2. Unit rows: `@range` (`range()/8`), `@shooting`, `@shootX`/`@shootY` (aim / 8),
   `@armor` (type armor, or the override set by `setprop`), `@buildX`/`@buildY`
   (`plan.x/y` while building, else -1), `@payloadCount`, `@totalPayload`,
   `@payloadCapacity`, `@speed` (`type.speed * 60 / 8 * speedMultiplier`), `@flag`;
   object rows `@name` (the player name for the player unit, else null), `@controller`
   (a LogicAI unit -> its processor building, else the unit itself), `@payloadType`,
   `@building`. T1's `LogicAi.controller` feeds `@controller`.
3. Building rows: remove `@name` and `@id` (Java: null). Add `@config`
   (`configSenseable`), `@payloadCount`, `@payloadType`. Family overrides go through the
   existing `BuildingBehavior::logic_sense` hook, each in its own behaviour file:
   GenericCrafter/Drill/Separator/UnitFactory/Reconstructor/ConstructBlock/MassDriver/
   LaunchPad/MendProjector `@progress`; heat families `@heat`; ForceProjector
   `@shield`/`@heat`; Door `@enabled`; Conveyor `@progress`/`@firstItem`; Core
   `@itemCapacity`/`@maxUnits` and `sense(UnitType)`; LightBlock `@color`. Skip (and
   list in your summary) any family whose port state lacks the field.
4. Liquid turrets: `LAccess::Ammo` for a liquid turret returns `state.total_ammo`
   (Java `Turret.sense` -> `totalAmmo`, which liquid turrets never increment -> 0), not the
   liquid amount (`core/world/blocks/defense/turrets/behavior.rs`). `LAccess::Heat` only for
   power turrets (`PowerTurret.sense`); item and liquid turrets fall through to the base table.
5. `run_fetch` (`core/logic/world.rs`, grep `pub fn run_fetch`) `Core/CoreCount`: the team's
   cores in registration order from `CampaignRuntime.session.teams` (grep how `teams` stores
   cores: `grep -n "cores" core/game/teams.rs`); fall back to buildings of a core block kind
   sorted by `EntitySeq` when there is no campaign runtime. `Player/PlayerCount`: the player
   unit, index 0 only, for its own team. `update_logic_globals` already reads
   `self.player_unit()`; store that entity in `LogicWorldState.player_unit` there and read it in `run_fetch`.
6. `printchar` content: `LogicText.iconc` -> `iconc.code(name).unwrap_or(0)`; append
   `char::from_u32(code)`, and U+0000 when the code is 0, exactly like Java.
7. `@clientLocale`/`@clientName`: `core/sim/mod.rs:1180` hardcodes `client_locale: Some("en")`.
   Add a `LogicClientInfo { locale: String, name: String }` resource. `install_sim_seams`
   (gdext) fills it from the settings store (keys `locale` and `name`; see
   `core/io/settings.rs` for the key constants) and `update_logic_globals` reads it,
   falling back to `"en"` and `""`.

**Tests** (one per row group): `content_sense_table` (`sensor h @dagger @health` == the
dagger def health; `sensor n @copper @name` == "copper"; `sensor c @copper @color` == the
Java packed color bits); `building_name_and_id_are_null`; `unit_range_and_armor`;
`liquid_turret_ammo_is_total_ammo`; `fetch_core_and_count`; `fetch_player`;
`printchar_content_emoji` (`printchar @copper` -> the char whose code `icons.properties`
lists for `copper`).

**Verify in-engine.** GZ: world processor at (118,50) (`place_block` + `logic_set_code`):
`fetch core c @sharded 0\nsensor n c @type\nfetch coreCount k @sharded 0 @air\nsensor h @dagger @health\nstop`
-> `logic_get_state(118,50)` shows `n` = the core-shard content, `k` = 1, `h` = 130.

**Commit.** `Fixes PAR-0218: sensor/fetch/printchar tables match Java (content, unit, family overrides, cores)`

---

### T4 — Logic-controlled turrets: `control shoot` / `shootp` (Fixes PAR-0223)

**In-engine evidence (2026-10-14):** duo (132,48) fed by a drill, world processor
`getblock building d 132 48` / `control shoot d 137 48 1` -> after 65 ticks
`@rotation 90`, `@shooting 0`, 0 bullets. Java: it turns to 0° and fires.

**Java** (`J/world/blocks/defense/turrets/Turret.java`; grep each name):
* `logicControlCooldown = 60 * 2` (120 ticks).
* `TurretBuild.control(LAccess type, double p1, double p2, double p3, double p4)`:
  `if(type == LAccess.shoot && !unit.isPlayer()){ targetPos.set(World.unconv((float)p1), World.unconv((float)p2)); logicControlTime = logicControlCooldown; logicShooting = !Mathf.zero(p3); }`
  then `super.control(...)`.
* `control(LAccess type, Object p1, double p2, double p3, double p4)`:
  `if(type == LAccess.shootp && (unit == null || !unit.isPlayer())){ logicControlTime = logicControlCooldown; logicShooting = !Mathf.zero(p2); if(p1 instanceof Posc pos) targetPosition(pos); }`
  then super.
* `logicControlled()` = `logicControlTime > 0`.
* `updateTile`: `if(!validateTarget()) target = null;` ...
  `isShooting = alwaysShooting || (isControlled() ? unit.isShooting() : logicControlled() ? logicShooting : target != null);`
  ... `if(logicControlTime > 0) logicControlTime -= Time.delta;`
* `validateTarget()` = `!Units.invalidateTarget(target, team, x, y) || isControlled() || logicControlled()`.
* In the "has ammo" block: `if(logicControlled()) canShoot = logicShooting;` and the
  `findTarget`/`targetPosition` retarget is skipped while `logicControlled()`, so `targetPos`
  stays where logic put it.

**Changes.**
1. Turret `BuildingBehavior::logic_control` (`core/world/blocks/defense/turrets/behavior.rs`):
   implement `Shoot` (numeric) and `Shootp` (object) exactly as above, with
   `LOGIC_CONTROL_COOLDOWN = 120.0`. `Shootp` with a unit or building target sets the
   target position through the same lead/`targetPosition` helper H02 T16 added
   (`grep -n "fn target_position\|fn intercept" core/world/blocks/defense/turrets/`).
2. `core/logic/access.rs` `control(...)`: after the link gate, route `LAccess::Shoot` and
   `LAccess::Shootp` to the behaviour hook and pass object params through (today it
   ignores them). Look at how `Enabled`/`Config` are routed and follow the same shape.
3. `core/world/blocks/defense/turrets/mod.rs` `update_turret`: change ONLY these lines:
   * the `is_shooting` line (~1433) to Java's three-way form (the port has no
     `isControlled()` player-turret control unless `control_block_unit_id` exists; use
     the existing controlled flag if there is one, `grep -n "controlled\|is_controlled" turrets/mod.rs`);
   * decrement `logic_control_time` by the tick delta when > 0;
   * the target validation keeps the target while logic-controlled;
   * skip the retarget/aim-at-target step while logic-controlled (keep `target_pos`).
   The `can_shoot` line (~1547) already reads `logic_shooting`; check it matches Java.

**Tests.** `control_shoot_aims_duo` (`core/sim/logic_campaign_tests.rs`): duo with copper
ammo, no enemies, world processor `getblock building d X Y` + `control shoot d X+5 Y 1`
-> after 30 ticks `rotation` within `shootCone` of 0°, bullets > 0; after the code is
replaced by `stop`, 200 ticks later no new bullets. `control_shootp_tracks_unit`: shootp at
an enemy dagger -> the turret's target position follows the dagger.

**Verify in-engine.** GZ: drill (130,48) + duo (132,48), `step(1500)` (ammo > 0), world
processor at (118,50):
`getblock building d 132 48\ncontrol shoot d 137 48 1\nsensor r d @rotation\nsensor s d @shooting`
-> after 60 ticks `r` near 0, `s` 1, `get_group_count("bullet") > 0`. Replace the code with
`stop`; 200 ticks later no new bullets. (Before: `r` 90, `s` 0, 0 bullets.)

**Commit.** `Fixes PAR-0223: logic-controlled turrets (control shoot/shootp)`

---

### T5 — `radar` / `uradar` correctness (Fixes PAR-0222)

**In-engine evidence (2026-10-14):** with two crux daggers (health 50/150) in range,
`radar enemy any any health d 1 hi` returns the 50-health one (Java: 150).

**Java** (`J/logic/LExecutor.java`, `grep -n "class RadarI" -A80`):
```java
Object base = radar.building();   // or the unit for uradar
if(base instanceof Ranged r && (exec.privileged || r.team() == exec.team) &&
   (base instanceof Building b && (!b.block.privileged || exec.privileged) || base instanceof Unit)){ ... }
```
* Range is `r.range()`. Unit targets come from `Units.nearby`/`nearbyEnemies` (enemy teams or
  own team per the filters) and must satisfy every `RadarTarget` filter
  (`J/logic/RadarTarget.java`: `any`, `enemy`, `ally`, `player`, `attacker` (`u.type.hasWeapons() && !u.disarmed`), `flying`, `boss` (`u.isBoss()`), `ground`).
* Sorting (`J/logic/RadarSort.java`): `distance` = `-dst2`, `health`, `shield`, `armor`,
  `maxHealth`; the best is `argmax(sort.func(pos, unit) * sortDir)` with `sortDir = order != 0 ? 1 : -1`.
* A 30-tick cache: `if(timer.get(30f))` recompute (`Interval` per instruction); otherwise
  output the last unit.

**Changes** (`core/logic/executor/radar.rs` `find` and the `Instruction::Radar` arm):
`best = argmax(value * dir)`; `distance` value `-(dx*dx+dy*dy)`; range = the source's
range (the `Ranged` trait equivalent: processors use `def.range`, turrets their range,
units `unit.range()`); filters per `RadarTarget.java`; `shield` from `ShieldComp`; `armor`
from the type (or the `setprop` override); a per-instruction cache keyed by instruction
index holding `(tick_computed, Option<Entity>)` refreshed when `tick - tick_computed >= 30`;
the permission gate above (a building source is not privileged unless the executor is).

**Tests.** `radar_health_order` (health 50/100: order 1 -> the 100 one, 0 -> the 50 one);
`radar_range` (a unit beyond a micro processor's 10-tile range -> null);
`radar_cache_holds_29_ticks`; `radar_attacker_filter`.

**Commit.** `Fixes PAR-0222: radar sort direction, range, filters, permissions and 30-tick cache`

---

### T6 — Processor links: Integer config toggles links, Java link lifecycle, pencil config panel, tap-to-link (Fixes PAR-0220)

**In-engine evidence (2026-10-14):** a real click on a micro processor opens
`block_config` with `{"text": true, "multiline": false}` (a one-line text field + OK/X),
no pencil (`runs/20261014-s3-verify2/H06-T14-processor-tap.png`).

**Java** (`J/world/blocks/logic/LogicBlock.java`):
* Config registrations (`:65-115`):
  * `byte[]` -> `if(!accessible()) return; build.readCompressed(data, true);`
  * `String` -> `if(!accessible() || !privileged) return; if(data != null && data.length() < maxNameLength) build.tag = data;`
  * `Character` -> `if(!accessible() || !privileged) return; build.iconTag = data;`
  * `Integer pos` (`:85-114`): `if(!accessible()) return; if(!entity.validLink(world.build(pos))) return;`
    remove every link whose building is `lbuild` (calling `l.trySet(executor, null)`); if any
    were removed and `lbuild.block.autoResetEnabled && lbuild.lastDisabler == entity` then
    `lbuild.enabled = true`; otherwise add `new LogicLink(x, y, entity.findLinkName(lbuild.block), true)`
    and `trySet` it. Then `entity.updateLinks()`.
* `accessible()` (`:120-122`) = `!privileged || state.rules.editor || state.playtestingMap != null || state.rules.allowEditWorldProcessors`.
  `canBreak(tile)` = `accessible()`. `checkForceDark` = `!accessible()`.
* `LogicBuild.collide(Bullet)` = `!privileged || destructible`; `damage(float)` only when
  `!privileged || destructible`; `removeFromProximity()` re-enables every link this
  processor disabled (`if(!link.enabled && link.lastDisabler == this) link.enabled = true;`
  — read `:454-462`; note it walks `executor.links` buildings); `canPickup()` = false;
  `displayable()` = `accessible()`; `shouldShowConfigure(player)` = `accessible()`.
* `validLink(other)` (`:727-729`):
  `other != null && other.isValid() && (privileged || (!other.block.privileged && other.team == team && other.within(this, range + other.block.size*tilesize/2f))) && !(privileged && !state.rules.worldProcessorPlayerLink && other.team == state.rules.defaultTeam) && !(other instanceof ConstructBuild)`.
* `buildConfiguration(table)` (`:737-739`): ONE button, `Icon.pencil`, style `Styles.cleari`,
  size 40, calling `showEditDialog()`.
* `showEditDialog(forceEditor)` (`:745-759`): `ui.logic.show(code, executor, privileged, code -> {...})`;
  the callback compresses `code` with `relativeConnections()`; if the bytes exceed
  `maxCompressedLen` it shows `logic.error.toolong`, else `configure(bytes)`.
* `onConfigureBuildTapped(other)` (`:761-773`): `this == other || !accessible()` -> `deselect()`,
  return false; `validLink(other)` -> `configure(other.pos())`, return false; else super.

**Port today.** §1. In addition, `LogicLink` (grep `pub struct LogicLink` in
`core/logic/blocks/logic_block.rs`) has `x, y, name, active`; the building's
`last_disabler` field: `grep -rn "last_disabler" core/` (it exists if H09 T4 /
`control enabled` added it; if not, add `last_disabler: Option<Entity>` to the
building component that holds `enabled`).

**Changes.**
1. `core/logic/blocks/logic_block.rs` `configured`:
   * `ConfigValue::Number(n)` (and `Int`, if the wire has a separate int variant: check
     `enum ConfigValue` in `core/world/config.rs` or wherever `grep -rn "pub enum ConfigValue" core/` finds it)
     with an integral `n` -> link toggle at `TilePos::from_pack(n as i32)` (grep `fn from_pack`
     / `fn unpack` for the Java `Point2.unpack` equivalent). Gate: `accessible`, then `validLink`.
   * A new `ConfigValue::Char(c)` is the icon tag if a char variant exists; otherwise
     drop icon-tag configuration from the input path (it is editor-only in Java) and list it
     as deferred in your summary.
   * `Point2`/`Building` stay as internal toggle forms but must go through the same
     `validLink` gate.
2. `toggle_link`: Java's remove-then-re-enable rule (`auto_reset_enabled` = the target block
   kind is not `SwitchBlock`; `J/world/Block.java` `autoResetEnabled = true` default,
   `SwitchBlock` sets false — grep it) and `last_disabler == Some(processor)`.
3. `LogicBlockBehavior::on_removed`: re-enable every linked building this processor disabled.
4. Guards: privileged and non-destructible processors take no damage (one check in the
   building damage helper, `grep -rn "pub fn damage_building\|fn apply_building_damage" core/`;
   cite `LogicBlock.java:437-451`), bullets pass through them (`collide`), `can_pickup` is
   false for every logic processor, and breaking is blocked when `!accessible`
   (the `validBreak` path H02 T2 wrote: `grep -n "fn valid_break" core/world/`).
5. `on_configure_build_tapped` for `LogicBlockBehavior`: Java's three branches. Return the
   `ConfigTapResult` variants H02 T6 defined (`grep -n "enum ConfigTapResult" -A10 core/world/config_tap.rs`).
6. Config panel: in `gdext/input/desktop.rs` near :3203, give `BlockKind::LogicBlock` the spec
   `{"logic": true, "x": X, "y": Y}` instead of the text field (keep `MessageBlock`'s text field).
   No panel at all when `!accessible` (Java `shouldShowConfigure`). In
   `ui/fragments/block_config_fragment.gd` `_rebuild()`, add a `logic` branch: one 40x40
   image button with the `pencil` icon, style `cleari`, that calls
   `get_node("/root/MindUi").open_dialog("logic", JSON.stringify({"x": x, "y": y}))` and hides the panel.
   Until T7 lands, the `logic` dialog does not exist: guard with `has_dialog` (check the
   `MindUi` API name with grep) and do nothing.

**Tests.** `link_toggle_via_number_config`, `unlink_reenables_disabled`,
`processor_removed_reenables`, `world_processor_not_accessible` (configure/break rejected
when not accessible), `privileged_processor_immune` (no damage, bullets pass),
`processor_cannot_be_picked_up`, `tap_links_valid_target` (`config_tap` from a micro
processor onto a switch in range adds a link; tapping again removes it; tapping out of
range falls through).

**Verify in-engine (real mouse clicks).** GZ, micro processor at (135,60) and switch at
(137,60) placed with `place_block`. Get the screen positions with `$C.tile_to_screen(...)`.
Click the processor (`godot_input` LMB press+release) -> `config_selected_json()` shows it and
the panel holds one pencil button (screenshot). Click the switch -> `logic_get_state(135,60).links`
lists `switch1`. Click the switch again -> the link is gone. Screenshots in
`runs/<date>-s<N>-I03-T6/`.

**Commit.** `Fixes PAR-0220: processor link config and lifecycle, pencil config panel, tap-to-link`

---

### T12 — Logic displays, sim half: tileable linking, root routing, image root index, a CPU rasterizer (PAR-0105 part 1)

**In-engine evidence (2026-10-14):** `logic-display` (124,50) + world processor
`draw clear 255 0 0` / `draw rect` / `drawflush disp` -> `logic_display_commands` `[]` and the
display draws nothing (`runs/20261014-s3-verify2/H06-T18-display.png`).

**Design decision (binding).** Java renders each display into a GPU `FrameBuffer`.
The port renders into a CPU RGBA buffer in `mind-core` (deterministic, testable
headless), and gdext uploads that buffer into an `ImageTexture` (T13). The buffer
lives in a view-side resource, never in the checksummed sim state.

**Java.**
* `LogicDisplay` (`J/world/blocks/logic/LogicDisplay.java`, 352 lines; read `:1-260`):
  command bytes `clear 0, color 1, colorPack 2 (virtual), stroke 3, line 4, rect 5,
  lineRect 6, poly 7, linePoly 8, triangle 9, image 10, print 11, translate 12, scale 13,
  rotate 14, resetTransform 15`; `displayDrawType = 30`; `scaleStep = 0.05`; `maxSides = 25`;
  `backgroundColor = Pal.darkerMetal`; `LogicDisplayBuild.draw(LongSeq graphicsBuffer)`:
  `added = min(buffer.size, maxDisplayBuffer - commands.size)`, append, `operations++`.
  `processCommands()` drains the whole queue into the frame buffer in order (switch at
  `:184-230`), with the `transform` matrix applied (`translate`/`scale` (`x*scaleStep`)/
  `rotate`/`idt`). `sense`: `displayWidth/Height` = `displaySize`, `bufferSize` =
  `rootDisplay.commands.size`, `operations` = `rootDisplay.operations`.
  `drawable(exec)` = `isValid() && (exec.privileged || (team == exec.team && !privileged))`.
  The display quad is drawn at the building centre with size
  `buffer.width * scaleFactor * Draw.scl` (`Draw.scl` = 1/4, so a 176-px large display covers
  44 world units = 5.5 tiles... check `displaySize`/`scaleFactor` per block in `J/content/Blocks.java`:
  `grep -n "LogicDisplay(\"\|TileableLogicDisplay(\"" -A8 J/content/Blocks.java`).
* `TileableLogicDisplay` (`J/world/blocks/logic/TileableLogicDisplay.java`, 324 lines; read all):
  `displaySize = 32`, `frameSize = 6`, `maxDisplayDimensions = 16`. `linkDisplays(start)`
  floods through same-block proximity, picks the bottom-left root
  (`next.tile.x < root.tile.x || next.tile.y < root.tile.y`), computes the bounding box,
  `rectangular = w*h == count`, and gives every member `rootDisplay = root`, `tilesWidth/Height`,
  `originX/Y`; old buffers are merged into the new root buffer at their old origins.
  `draw(LongSeq)` forwards to `rootDisplay` unless this is the root. The root buffer is
  `(32*tilesWidth - 12) x (32*tilesHeight - 12)` px. `sense displayWidth` =
  `tilesWidth*32 - 12`. `onProximityAdded`/`onProximityRemoved` set `needsUpdate`, and
  `updateTile` re-links when `needsUpdate`. `bits` = the 8-neighbour same-block mask
  (`TileBitmask` picks the frame sprite).
* Image command: the executor packs `(rootDisplay.index << 5) | 30` for a display
  source (`J/logic/LExecutor.java`, `grep -n "displayDrawType" J/logic/LExecutor.java`), and content
  icons pack `(content.id << 5) | contentType.ordinal()`.

**Changes.**
1. `core/logic/blocks/display.rs`: add tileable fields to `LogicDisplayState`
   (`tiles_width, tiles_height, origin_x, origin_y: i32`, `bits: u8`, `needs_update: bool`,
   `rectangular: bool`), a `pub fn link_displays(world, start: Entity)` that transcribes
   `linkDisplays` (flood in `EntitySeq` order through the proximity list; grep how proximity
   is stored: `grep -rn "struct Proximity" core/world/`), and hook `on_proximity_added`/
   `on_proximity_removed`/`update_tile` (or the port's equivalents, grep `fn on_proximity`
   in `core/world/behavior/mod.rs`) to set `needs_update` and re-link. `bits` from the
   8 neighbours on proximity update.
2. `core/logic/blocks/io.rs` `flush_draw`: append to the ROOT display's queue (Java
   `TileableLogicDisplayBuild.draw` forwards), using `drawable(exec)`'s rule for the gate.
3. `core/logic/executor/draw.rs` image packing (:252-255): `(root.index << 5) | 30` for a
   display source, and `(content.id << 5) | content_type_ordinal` for content.
4. New `core/logic/display_raster.rs` (pure Rust, no Godot): a `DisplayRaster { w, h, pixels: Vec<u32> /* RGBA8888 */, color: u32, stroke: f32, transform: [f32; 6] }` and
   `pub fn process_commands(raster: &mut DisplayRaster, commands: &mut VecDeque<u64>, ctx: &dyn DisplayDrawContext)`
   that drains the queue exactly in Java's order and implements every command:
   `clear` (fill RGB, alpha 255), `color` (`Color.toFloatBits(r,g,b,a)` -> RGBA), `stroke`,
   `line` (a quad of width `stroke` like `Lines.line`), `rect` (`Fill.crect(x,y,w,h)`: lower-left
   corner at x,y), `lineRect` (`Lines.rect`), `poly`/`linePoly` (`sides = min(p1, 25)`,
   radius p2, rotation p3), `triangle`, `image` (`ctx.icon(type, id) -> Option<IconPixels>`
   drawn rotated/scaled at x,y size p2, plus display sources via `ctx.display(index)`),
   `print` (`ctx.glyph(char) -> Option<Glyph>` from the logic font, placed per `LogicDisplay.java:207-216`),
   `translate`/`scale`/`rotate`/`resetTransform` (a 2-D affine matrix). Coordinates are
   display pixels with (0,0) at the bottom-left (the buffer is drawn with a negative height in Java,
   so row 0 of the texture is the TOP of the image: store pixels top-row-first and flip y
   on write). Use a scanline polygon fill with pixel-centre sampling (no anti-aliasing;
   Java frame buffers draw without MSAA). Clip to the buffer.
   `trait DisplayDrawContext { fn icon(&self, content_type: u8, id: u16) -> Option<RgbaImage>; fn glyph(&self, c: char) -> Option<Glyph>; fn display(&self, index: i32) -> Option<(u32, u32, &[u32])>; }`
   with a `NoIcons` test impl.
5. A `LogicDisplayRasters` view resource (NOT `ChecksumPart`) mapping root `Entity` ->
   `DisplayRaster`, created on demand with the background colour `Pal.darkerMetal`
   (`grep -rn "DARKER_METAL\|darker_metal" core/render/` for the constant), resized when a
   tileable root's dimensions change (copy old pixels at their old origin, like `MergeBuffer`).
   `pub fn process_display(world, root) -> bool` drains the root's queue into its raster and
   returns whether pixels changed. The VIEW calls it (T13); the sim never does, matching Java
   (Java only processes when the display is drawn).
6. Sense rows: `displayWidth/Height`, `bufferSize`, `operations` per Java (tileable widths above).

**Tests** (`core/logic/display_raster.rs` and `core/logic/blocks/display.rs`):
`clear_fills_buffer`, `rect_covers_expected_pixels` (`rect 10 10 20 20` on 80x80 -> exactly
pixels x 10..29, y 10..29 bottom-origin), `line_with_stroke`, `poly_hexagon_area_within_2pct`,
`transform_translate_then_rect`, `tileable_2x2_links_root_bottom_left` (four displays ->
one root, width 2, height 2, `rectangular`), `flush_draw_forwards_to_root`,
`image_packs_root_index`, `queue_cap_1024`.

**Commit.** `PAR-0105: tileable display linking, root routing and a CPU display rasterizer`

---

### T13 — Logic displays, render half (Fixes PAR-0105)

**Acceptance.** A `logic-display` driven by `draw clear 255 0 0` / `draw color 0 255 0 255` /
`draw rect 10 10 30 30` / `drawflush` shows a red square with a green rectangle, at the
display's position, in the live client; a 2x2 group of `tile-logic-display`s shows one
merged picture; `drawflush` with no rendering pass never grows the queue past 1024.

**Java.** `LogicDisplayBuild.draw()` (`LogicDisplay.java:83-97`): base sprite, then (if
`renderer.drawDisplays`) process commands and draw the buffer at the building centre, size
`buffer.width * scaleFactor * Draw.scl` by `-buffer.height * ...` (y-flipped), blending disabled
(opaque). Tileable: `drawFull` (one quad for the whole rectangular group, drawn once per frame
by the first tile) or `drawTile` (each tile draws its 32x32 slice, offset by the frame), plus
the frame sprite `tileRegion[TileBitmask.values[bits]]` when `bits != 255` at `Layer.block + 0.02`.
`renderer.drawDisplays` is the `displays` setting? Check `grep -rn "drawDisplays" J/` (it is a
setting-driven flag).

**Changes.**
1. New `gdext/render/logic_displays.rs`: a `Node2D` pass in the block band (attach it in
   `gdext/render.rs` next to the other block-band passes: add ONE child, nothing else). Each frame:
   collect visible display buildings (root + members) from the sim, call
   `mind_core::logic::blocks::display::process_display(world, root)` for each visible root
   (take care with `bind_mut` rules), and keep one `ImageTexture` per root, updated with
   `Image::create_from_data(w, h, false, FORMAT_RGBA8, bytes)` + `ImageTexture::update` only when
   `process_display` returned true. Draw with `canvas::draw_region` at the Java rect.
   Implement `DisplayDrawContext` in gdext: `icon` reads the content's `fullIcon` region from
   the atlas `Image` (`MindAssets`), `glyph` reads the logic font (`assets/fonts/logic.ttf`;
   rasterize glyphs once into a cache with Godot's `Font.draw_char` into an `Image`, or use
   `TextServer` glyph bitmaps), `display` reads another root's raster.
2. Replace the placeholder `#[func] SimHost.logic_display_commands(x,y)` with
   `#[func] SimHost.logic_display_state(x, y) -> Dictionary`
   (`{root_x, root_y, width, height, queued, operations, rectangular}`) and
   `#[func] SimHost.logic_display_pixel(x, y, px, py) -> i64` (RGBA of the root raster).
   Keep the old name returning the queued commands of the root (a real read) so existing
   probes keep working.
3. Frame sprites for tileable displays: draw `tile-logic-display-<n>` per
   `TileBitmask.values[bits]` (`grep -n "class TileBitmask" -A30 J/world/blocks/TileBitmask.java`
   or wherever it is; copy its 256-entry table into `core/render/` as a const array).

**Tests.** `gdext` unit tests are limited; put the packing/bitmask table tests in core.

**Verify in-engine.** GZ: `place_block(124,50,"logic-display")`, world processor at (118,50):
`getblock building d 124 50\ndraw clear 255 0 0\ndraw color 0 255 0 255\ndraw rect 10 10 30 30\ndrawflush d\nstop`
-> `logic_display_pixel(124,50,20,20)` is green, `(5,5)` red; screenshot shows the display.
Then four `tile-logic-display`s at (140..141, 60..61) and the same program drawing a
40x40 rect -> one merged picture across the four tiles (screenshot).

**Commit.** `Fixes PAR-0105: logic displays render drawflush output (incl. tileable displays)`

---

### T14 — Canvas blocks: logic read/write, render, config bytes, edit dialog (Fixes PAR-0225)

**In-engine evidence (2026-10-14):** a placed `canvas` (128,46) draws only its base sprite.

**Java** (`J/world/blocks/logic/CanvasBlock.java`, 350 lines; read all):
`canvasSize = 8` (default; check `Blocks.java` `canvas`), palette of 8 colours
`{0x362944, 0xc45d9f, 0xe39aac, 0xf0dab1, 0x6461c2, 0x2ba9b4, 0x93d4b5, 0xf0f6e8}`,
`bitsPerPixel = log2(nextPowerOfTwo(palette.length))` = 3, `data` length
`ceil(size*size*bpp/8)` = 24 bytes. Pixel `pos` is LINEAR (`pos < size*size`), stored at bit
offset `pos*bpp` LSB-first (`getByte`/`setByte`). `read(position, output)` ->
`getPixel(pos)` (NaN when out of range); `write(position, value)` -> `setPixel(pos, index)`
(ignored when out of range or index >= palette length). `readable/writable` =
`isValid() && (exec.privileged || team == exec.team)`. `sense displayWidth/Height` = `canvasSize`.
`config()` = `data`; `config(byte[])` copies when lengths match. Save: `i(len)` + bytes; read
skips when the length differs. Draw: base sprite only when `blending == 0`; the texture
(`makePixmap`: pixel (i % size, i / size)) drawn at `size*8 - pad` (`pad = padding` when not
blending); `side1/side2/corner1/corner2` edges where no neighbouring canvas blends;
`blending` = 4-bit mask of same-block neighbours whose origin is exactly `size` tiles away.
`buildConfiguration`: one pencil button (`Styles.cleari`, 40) opening `CanvasEditDialog`
(`J/ui/dialogs/CanvasEditDialog.java`; read it for the paint UI: palette buttons, a grid of
pixels, brush/fill tools, `@ok` applies via `configure(packPixmap(...))`).
`onConfigureBuildTapped(other)`: `this == other` -> deselect, false; else true.

**Changes.**
1. `core/logic/blocks/canvas.rs`: switch the state to Java's linear form
   (`get_pixel(pos) -> f64` NaN out of range, `set_pixel(pos, index)` with the guards,
   LSB-first bit packing over `data: Vec<u8>`). Keep an `invalidated` flag for the view.
   Fix the save body to `i(len)` + bytes with the length check.
2. `core/logic/blocks/io.rs`: `CanvasBlock` arms in the read and write paths with the
   permission rule above.
3. `configured(ConfigValue::Bytes(b))` copies when the length matches; `config()` returns the bytes.
4. Render: `gdext/render/logic_displays.rs` (T13's pass) also draws canvases: an 8x8
   `ImageTexture` per canvas updated when `invalidated`, the edge sprites
   (`canvas-side1/side2/corner1/corner2`) per `blending`.
5. Config panel: `{"canvas": true, "x":..,"y":..}` spec -> one pencil button in
   `block_config_fragment.gd` that opens a new `canvas_edit` dialog
   (`ui/dialogs/canvas_edit_dialog.gd` + `.tscn` + manifest + `core/ui/manifest.rs` entry;
   `EXPECTED_*_DIALOGS`). The dialog reads `building_config_json(x,y)` (the bytes as an int
   array; add that field if missing), paints with the palette and applies through
   `$I.configure_building(x, y, "bytes", <base64>)` (add a `bytes` kind to `configure_building`
   if it does not exist; grep its kinds).

**Tests.** `canvas_linear_pixels_roundtrip`, `canvas_write_out_of_range_ignored`,
`canvas_logic_read_write` (world processor `write 5 c 10` then `read r c 10` -> 5),
`canvas_save_roundtrip`.

**Verify in-engine.** GZ: `place_block(128,46,"canvas")`, world processor
`getblock building c 128 46\nwrite 1 c 0\nwrite 2 c 9\nstop` -> screenshot shows two pixels
painted; open the pencil dialog, paint, OK -> the building updates.

**Commit.** `Fixes PAR-0225: canvas blocks render, accept logic read/write and edit dialog`

---

### T7 — Logic editor, part 1: the `logic` dialog shell, statement list, load/save (PAR-0103)

**In-engine evidence (2026-10-14):** `UiRoot.dialog("logic") == null`; tapping a processor
opens only the generic text field.

**Java** (`J/logic/LogicDialog.java`, 371 lines; read all):
* `LogicDialog` is a `BaseDialog("logic")` with `clearChildren()`; children are the
  `LCanvas canvas` (grow) and the `buttons` row (growX). `shouldPause = true`;
  `addCloseListener()`; `hidden(() -> consumer.get(canvas.save()))`.
* `setup()` (`:76-...`): buttons size 160x64: `@back` (Icon.left, hide), `@edit`
  (Icon.edit; opens a sub-dialog "@editor.export" with: `@editor.worldprocessors.editname`
  (privileged only, outside the map editor), `@clear` (confirm `@logic.clear.confirm`),
  `@copy.clipboard`, `@load.clipboard` (disabled when the clipboard is empty),
  `@logic.restart` (forceRestart = true, hide)), then (portrait: new row) `@variables`
  (Icon.menu; shows the variables dialog, or the globals dialog when there is no executor
  or in the menu; the button text switches to `@logic.globals` in that case) and `@add`
  (Icon.add; `showAddDialog()`; disabled at `LExecutor.maxInstructions` statements).
* `show(code, executor, privileged, modified)`: loads `code` into the canvas (empty on a
  parse error); on hide the consumer calls `modified(result)` when `forceRestart || !result.equals(code)`.
* Shift+Enter opens the add dialog.

**Port design.** The canvas edits a list of statements in GDScript. The Rust side already
parses/serializes: `MindLogic.parse(source, privileged)` returns statements (check the
returned dictionary shape: `grep -n "fn parse" -A25 gdext/logic.rs`). Add
`#[func] MindLogic.write_statements(statements: Array) -> GString` (inverse of `parse`, using
`Statement::write`) and `#[func] MindLogic.default_statement(name) -> Dictionary` (a default
instance's fields, from `core/logic/statement.rs` `default_statement`). Saving goes
through the processor's configure path: `#[func] SimHost.logic_configure_code(x, y, code) -> GString`
that compresses with `relativeConnections` (the port's `compressed_config`/`compress`
helpers in `logic_block.rs`), returns `"toolong:<len>"` when it exceeds `maxCompressedLen`
(`grep -n "maxCompressedLen" J/world/blocks/logic/LogicBlock.java`), and otherwise queues a
`Bytes` configure as the player (so the campaign configure gate applies, unlike the
`logic_set_code` test seam).

**Changes.**
1. `ui/dialogs/logic_dialog.gd` + `scenes/ui/dialogs/logic_dialog.tscn` (copy the shell of
   `full_text_dialog` and set `full_dialog = true`, `should_pause = true`); manifest entry
   `{"name": "logic", "scene": "res://scenes/ui/dialogs/logic_dialog.tscn", "pause": true, "menu_openable": false, "args": ["x:Int", "y:Int"]}`;
   a static instance under the dialog layer in `scenes/ui/ui_root.tscn`; and confirm
   `core/ui/manifest.rs:405`'s `logic` entry matches.
2. `shown()`: read `x`,`y` from `_context`; fetch the code with `SimHost.logic_get_code(x,y)`,
   `privileged` from `building_debug(x,y)` (add the field if missing), parse it with
   `MindLogic.parse`, and build one statement element per statement (T8 builds the fields;
   in T7 each element shows only the header row: category colour, localized name
   (`lst.<name>` bundle key; check `bundle.properties`), index label, and add/copy/delete
   buttons per `LCanvas.StatementElem` (`J/logic/LCanvas.java:417-510`): header height 38,
   margin 6, `Styles.logici` 24x24 buttons, `Tex.whitePane` background tinted with the
   category colour (`LCategory` colours: `J/logic/LCategory.java:16-22`, `Pal.logic*` in
   `J/graphics/Pal.java`)).
3. The buttons row exactly as Java's `setup()` (bundle keys above, icons by name through
   `MindWidgets.icon_button`). `@edit` opens the sub-dialog (a second small dialog scene,
   or a popup panel built inside the logic dialog; if you add a scene it also needs the
   manifest/ui_root/manifest.rs triple). Clipboard via `DisplayServer.clipboard_get/set`.
4. `hidden()`: serialize with `MindLogic.write_statements`; when changed (or restart),
   call `SimHost.logic_configure_code(x,y,code)`; on `toolong:<len>` show
   `@logic.error.toolong` formatted (`MindAssets.bundle_format`).
5. Wire the T6 pencil button to `open_dialog("logic", ...)` (remove T6's guard).
6. In the variables button handler, call T10's dialog when it exists; until then disable the button.

**Tests.** `core/ui/manifest.rs` test passes with the new entry. `gdext` tests for
`write_statements(parse(x)) == x` round trip over the T7 conformance programs
(`core/sim/logic_campaign_tests.rs` has the shipped map programs).

**Verify in-engine.** Micro processor at (135,60); click it, click the pencil ->
`$ROOT.dialog("logic").visible == true`, screenshot. Programmatically add statements
via the add dialog (T9) or by editing through the dialog's API, hide, `step(5)` ->
`logic_get_code(135,60)` holds the new text and `logic_get_state(135,60).vars` reflects it.

**Commit.** `PAR-0103: logic editor dialog shell (statement list, edit menu, save through configure)`

---

### T8 — Logic editor, part 2: per-statement field layouts (PAR-0103)

**Java.** Each statement's `build(Table)` in `J/logic/LStatements.java` (2598 lines). List
the classes and their line numbers with
`grep -n "@RegisterStatement\|public static class\|public void build(Table" J/logic/LStatements.java`.
The shared helpers are in `J/logic/LStatement.java:233-380`:
* `field(table, value, setter)`: `Styles.nodeField`, size 180x40, pad 2, value sanitized by
  `sanitize` (`:94-150`: one-char `" ; space \n \t #` -> `invalid`; quoted strings keep
  `\n \" \\ \uXXXX` escapes and escape raw `" \ newline`; unquoted: `; -> s`, `" -> '`,
  whitespace and `#` -> `_`). Port `sanitize` into `core/logic/statement.rs` as
  `pub fn sanitize(value: &str) -> String` with Java's tests, and expose
  `#[func] MindLogic.sanitize(value)`.
* `fields(table, desc, value, setter)`: label `bundle(desc)` (`name.token.<desc>` when
  `logiclocalization` is on) then the field (or field then label in compact mode).
* `showSelect(button, values, current, getter, cols=4)`: a popup table of toggle buttons
  (`Styles.logicTogglet`) above the button; `showSelectTable` (`:288-...`).
* `fieldAlignSelect`, `col` (colour pencil -> colour picker; value `%rrggbbaa`).
* `row(table)` in wrapping mode.
`StatementElem` uses a `WrapTable` when `st.useWrapping()`.

**Changes.**
1. Port `sanitize` (above) with tests copied from Java behaviour.
2. In `logic_dialog.gd`, a generic field builder that renders each `StatementField` from
   `MindLogic.statement_fields(name)` by its `FieldKind` (`grep -n "enum FieldKind" -A20 core/logic/statement.rs`):
   text fields (180x40, `nodeField`), enum selects (a `logict` button that opens a popup of
   `logicTogglet` buttons, 4 columns), and the colour pencil. Field labels use
   `name.token.<field>`.
3. Then, statement by statement, match Java's `build(Table)` layouts. Do them in category
   batches, one commit each: (a) io + block (`read write draw print printchar format
   drawflush printflush getlink control radar sensor`), (b) operation + flow (`set op select
   wait stop lookup packcolor unpackcolor end jump`), (c) unit (`ubind ucontrol uradar
   ulocate`), (d) world (`getblock setblock spawn bullet status weathersense weatherset
   spawnwave setrule message cutscene effect explosion setrate fetch sync clientdata getflag
   setflag setprop playsound playmusic setmarker makemarker localeprint`). Statements whose
   visible fields depend on another field (`draw` by type, `ucontrol` by command, `setrule`
   by rule, `op` unary vs binary, `jump` condition, `control` by access, `effect` by effect
   params, `setmarker` by property) must rebuild their row when that field changes, exactly
   as Java calls `rebuild(table)`.
4. Each edit writes back into the statement dictionary so `write_statements` serializes it.

**Tests.** `sanitize_*` (Rust). Round-trip tests from T7 stay green.

**Verify in-engine.** Open the editor on a processor holding the GZ world-processor program
(`logic_get_code` of the shipped world processor); screenshot every category; compare
against Java (if a Java capture of the logic editor exists under `REPO/runs/*ref*`, use it;
otherwise follow I-environment §5 to capture one with the same program).

**Commits.** `PAR-0103: logic editor field layouts (io/block)`, `... (operation/flow)`,
`... (unit)`, `... (world)`.

---

### T9 — Logic editor, part 3: jump arrows, drag reorder, copy/delete, add dialog (PAR-0103)

**Java.**
* `LCanvas` (`J/logic/LCanvas.java`, 742 lines): `DragLayout` (`:197-415`: vertical layout,
  space 10 between elements, the dragged element follows the pointer and the others make
  room; `finishLayout` re-inserts it at the drop index; `setJumpHeights` `:274-355` assigns
  each `JumpCurve` a lane so arrows do not overlap), `StatementElem` (`:417-550`: drag on the
  header, middle-click copies, `copy()` inserts after this element and bumps a copied
  `jump`'s target if it pointed below), `JumpButton` (`:551-625`: drag from the jump
  statement's button onto a target element), `JumpCurve` (`:626-742`: a bezier from the
  button to the target's left edge, colour `Pal.place`, drawn in the margin left of the list).
* `showAddDialog(position)` (`LogicDialog.java`, `:208-277`): a search field (auto-focus on
  desktop, Enter adds the first match), then statements grouped by `LCategory` with a
  category header (icon, localized name, a 5-px line), buttons 130x50 in rows of 3 with the
  category colour as font colour; filters: hidden, `privileged && !privileged`,
  `nonPrivileged && privileged`, unit category hidden when `!privileged && !rules.logicUnitControl`.

**Changes.** Implement drag-reorder (index updates and `jump` target remapping like Java),
copy/delete, the add dialog (as a popup/sub-dialog of the logic dialog), and the jump curves
(`_draw()` on an overlay `Control` beside the list, cubic bezier per `drawCurve`).

**Verify in-engine.** Add 3 statements through the add dialog (search "set", Enter), drag the
last above the first, add a jump and drag its button onto statement 0; hide; the saved code
matches the expected text.

**Commit.** `PAR-0103: logic editor drag, copy/delete, jump arrows and add dialog`

---

### T10 — Logic editor, part 4: variables and globals dialogs (PAR-0103)

**Java.** The variables dialog inside `LogicDialog.setup()` (`:139-197`): for each non-constant
var a row of: an 8-px stub, the name on a gray label (`Pal.accent` text), a value label
(140 wide, refreshed every 15 ticks, flashes `Pal.accent` -> white on change; numbers print as
integers when within 0.00001 of an integer), the type (`typeName`/`typeColor`, `:46-73`). The
dialog un-pauses the game while shown and restores pause on hide. `@logic.globals` opens
`GlobalVarsDialog` (`J/logic/GlobalVarsDialog.java`, 61 lines: the built-in `@` constants
with descriptions from `lglobal.<name>` bundle keys).

**Changes.** Two small dialogs (or sub-panels) fed by `SimHost.logic_get_state(x,y).vars`
(polled every 15 frames while shown) and `MindLogic.metadata_handoff()` / the globals list
(add `#[func] MindLogic.global_names()` if needed).

**Commit.** `Fixes PAR-0103: logic variables and globals dialogs` (only this last editor
commit says `Fixes`).

---

### T11 — Logic overlays: processor range/links/link names, LogicAI unit squares, tags, message hover text (PAR-0103 slice)

**Java.** `LogicBuild.drawConfigure()` (`LogicBlock.java:661-683`): `Drawf.circles(x, y, range)`
(non-privileged only); `Drawf.square(build.x, build.y, size*4 + 1, Pal.place)` per valid link;
`build.block.drawPlaceText(l.name, tileX, tileY, true)` per link (on top).
`drawSelect()` (`:685-725`): `Drawf.square(unit.x, unit.y, unit.hitSize, unit.rotation + 45)` for
every unit whose LogicAI controller is this processor; the world-processor `tag` text
(outline font, scale 1/4, white alpha 0.5, wrap width 90, at `y + 8 + 2 - 1`) unless
pixelate; the `iconTag` large icon at alpha 0.5 (8x8 world units).
`MessageBlock.drawSelect` (`J/world/blocks/logic/MessageBlock.java`, grep `drawSelect`): the
message text above the block.

**Changes.**
1. Give `ConfigOverlay::Logic` fields `{ x, y, range: Option<f32>, links: Vec<(f32, f32, i32 /*size*/, String /*name*/)> }`
   and `SelectOverlay::LogicTag { x, y, tag: Option<String>, icon: Option<String>, units: Vec<(f32,f32,f32,f32)> }`,
   `SelectOverlay::MessageText { x, y, text: String }`. Fill them in the place that builds
   these enums (`grep -n "ConfigOverlay::Logic\|SelectOverlay::MessageText\|SelectOverlay::LogicTag" gdext/`)
   from a pure core helper `logic_block::overlay_links(world, e) -> Vec<(f32, f32, i32, String)>`
   (+ test: a 2x2 memory bank link gives half-size 9.0).
2. Draw them in `gdext/render/overlay_top.rs` (:203 and :273) with the existing `canvas`
   helpers (`circles`, `square`, text through the same path `drawPlaceText` uses elsewhere:
   `grep -rn "draw_place_text\|place_text" gdext/render/`).

**Commit.** `Gameplay I03 T11: processor link/range overlay, LogicAI unit squares, tags and message hover text`

---

## 3. Order and hot files

Order (one commit each, as listed above):
T1, T2, T3, T4, T5, T6, T12, T13, T14, T7, T8 (four commits), T9, T10, T11.

Hot shared files: `core/sim/unit_runtime.rs` (T1: one branch only),
`core/world/blocks/defense/turrets/mod.rs` (T4: the listed lines only),
`gdext/sim_host.rs` (T6, T11, T13: additive `#[func]`s), `gdext/render.rs` (T13: one child),
`gdext/input/desktop.rs` (T6: the spec line), `ui/fragments/block_config_fragment.gd`
(T6, T14), `scenes/ui/ui_root.tscn` + `ui/dialogs_manifest.json` (T7, T14: one entry each).

## 4. Ledger changes to report (coordinator applies)

| Item | After |
|---|---|
| PAR-0221 | verified (T1 tests + in-engine poly) |
| PAR-0224 | verified headless (T2) |
| PAR-0218 | verified (T3) |
| PAR-0223 | verified (T4 in-engine duo) |
| PAR-0222 | verified headless + in-engine if run (T5) |
| PAR-0220 | verified (T6 real clicks) |
| PAR-0105 | verified (T13 screenshots) |
| PAR-0225 | verified (T14) |
| PAR-0103 | verified when T7-T10 land (T11 is a slice) |

## 5. Done criteria

Every task committed with its tests; `mind-core`, `mind-gdext`, `mind-headless` suites
green; clippy clean; `parity goldens` all PASS; GDScript parse gate clean; summary
written to `REPO/docs/I03-logic_summary.md`.
