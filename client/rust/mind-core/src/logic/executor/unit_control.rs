// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit binding and control bridge (`LExecutor.UnitBindI`/`UnitControlI`/
//! `UnitLocateI`).
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java:180-551` and
//! `entities/comp/UnitComp.checkLogicAI` (`UnitComp.java:562-580`). The
//! movement body is `ai/types/logic.rs` (`LogicAI.updateMovement`); this module
//! applies the instruction-side setters and enforces
//! `Rules.logic_unit_control`/`logic_unit_build`/`logic_unit_deconstruct`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ai::{AiKind, ControllerSlot};
use crate::entities::comp::unit::{HitboxComp, UnitCore};
use crate::entities::comp::{Pos, TeamComp, Unit};
use crate::logic::enums::LUnitControl;
use crate::logic::value::{LogicObject, VarRef};

/// `LogicAI.logicControlTimeout` (10 s at 60 Hz).
pub const UNIT_TIMEOUT_KEEP: f32 = 600.0;
/// `LogicAI.transferDelay` (1.5 s at 60 Hz).
pub const TRANSFER_DELAY: f32 = 90.0;
/// `Vars.logicItemTransferRange` (`Vars.java:125`).
pub const LOGIC_ITEM_TRANSFER_RANGE: f32 = 45.0;

/// `Rules.logicUnitControl` gate for `ubind`/`ucontrol`.
pub fn logic_unit_control_enabled(rules: &dyn crate::logic::blocks::LogicRulesApi) -> bool {
    rules.logic_unit_control()
}

/// `Rules.logicUnitBuild` gate for `ucontrol build`.
pub fn logic_unit_build_enabled(rules: &dyn crate::logic::blocks::LogicRulesApi) -> bool {
    rules.logic_unit_build()
}

/// `Rules.logicUnitDeconstruct` gate for `ucontrol deconstruct`.
pub fn logic_unit_deconstruct_enabled(rules: &dyn crate::logic::blocks::LogicRulesApi) -> bool {
    rules.logic_unit_deconstruct()
}

/// Borrows the content registry from the live unit runtime (the VM runs with
/// only a `&mut World`; `UnitRuntime` holds the registry + grid snapshot).
pub(crate) fn with_content<R>(
    world: &World,
    f: impl FnOnce(&crate::content::ContentRegistry) -> R,
) -> Option<R> {
    world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .map(|runtime| f(&runtime.content))
}

/// Borrows the live tile grid: the editor/generation `WorldGrid` resource
/// first, the unit runtime's snapshot second (same order as `run_get_block`).
pub(crate) fn with_grid<R>(
    world: &World,
    f: impl FnOnce(&crate::world::WorldGrid) -> R,
) -> Option<R> {
    if let Some(grid) = world.get_resource::<crate::world::WorldGrid>() {
        return Some(f(grid));
    }
    world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .map(|runtime| f(&runtime.grid))
}

/// A tile read (`block`, `build`, `floor`, `overlay`) at world coordinates
/// (`World.tileWorld`, rounding).
pub(crate) fn tile_at_world(world: &World, x: f32, y: f32) -> Option<TileSnapshot> {
    let tx = crate::world::WorldGrid::to_tile(x);
    let ty = crate::world::WorldGrid::to_tile(y);
    with_grid(world, |grid| {
        if !grid.tiles.in_bounds(tx, ty) {
            return None;
        }
        let tile = grid.tile(tx, ty);
        Some(TileSnapshot {
            block: tile.block,
            build: tile.build,
            floor: tile.floor,
            overlay: tile.overlay,
        })
    })
    .flatten()
}

/// One tile's layers (`World.tileWorld` read).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TileSnapshot {
    /// `tile.block()`.
    pub block: crate::content::BlockId,
    /// `tile.build`.
    pub build: Option<Entity>,
    /// `tile.floor()`.
    pub floor: crate::content::BlockId,
    /// `tile.overlay()`.
    pub overlay: crate::content::BlockId,
}

/// World-pixel centre of a building (`Building.x/y`: tile centre + offset).
pub(crate) fn building_center(world: &World, e: Entity) -> Option<(f32, f32)> {
    let tile = world.get::<crate::entities::comp::Building>(e)?.tile;
    let block = world.get::<crate::entities::comp::Building>(e)?.block;
    let offset = with_content(world, |content| {
        content.block(block).map(|def| def.offset).unwrap_or(0.0)
    })
    .unwrap_or(0.0);
    let ts = crate::config::TILESIZE as f32;
    Some((tile.x() as f32 * ts + offset, tile.y() as f32 * ts + offset))
}

/// `UnitComp.clearBuilding()` -> `BuilderComp.clearBuilding` (`plans.clear()`).
pub(crate) fn clear_building(world: &mut World, unit: Entity) {
    if let Some(mut plans) = world.get_mut::<crate::sim::builder::BuilderPlans>(unit) {
        plans.plans.clear();
    }
}

/// `unit.canBuild()`: `type.buildSpeed > 0` (`UnitComp.java:862`).
pub(crate) fn unit_can_build(world: &World, unit: Entity) -> bool {
    world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
        .and_then(|comp| {
            with_content(world, |content| {
                content.unit(comp.type_id).map(|def| def.build_speed > 0.0)
            })
        })
        .flatten()
        .unwrap_or(false)
}

/// `UnitComp.canMine()` / `validMine(tile)`.
fn unit_valid_mine(world: &World, unit: Entity, tile: crate::world::TilePos) -> bool {
    let Some(runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
        return false;
    };
    crate::entities::comp::unit::miner::valid_mine(
        world,
        &runtime.grid,
        &runtime.content,
        unit,
        Some(tile),
        true,
    )
}

/// `UnitComp.maxAccepted(item)` (`UnitComp.java`): one item type at a time,
/// bounded by `type.itemCapacity`.
fn unit_max_accepted(world: &World, unit: Entity, item: crate::content::ItemId) -> i32 {
    let capacity = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
        .and_then(|comp| {
            with_content(world, |content| {
                content
                    .unit(comp.type_id)
                    .map(|def| def.item_capacity.max(0))
            })
        })
        .flatten()
        .unwrap_or(0);
    match world
        .get::<crate::entities::comp::unit::comp::ItemsComp>(unit)
        .and_then(|items| items.item)
    {
        Some((held, amount)) if held != item && amount > 0 => 0,
        Some((_, amount)) => (capacity - amount).max(0),
        None => capacity,
    }
}

/// `BuildingComp.allowDeposit()` (`BuildingComp.java:745`).
fn building_allows_deposit(world: &World, build: Entity) -> bool {
    let only_core =
        crate::game::runtime::rules_of(world).is_some_and(|rules| rules.only_deposit_core);
    if !only_core {
        return true;
    }
    world
        .get::<crate::entities::comp::Building>(build)
        .is_some_and(|building| {
            with_content(world, |content| {
                content
                    .block(building.block)
                    .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock)
            })
            .unwrap_or(false)
        })
}

/// `Building.isValid()`: alive (`Health.dead` false).
fn building_is_valid(world: &World, build: Entity) -> bool {
    world
        .get::<crate::entities::comp::Health>(build)
        .is_some_and(|health| !health.dead)
}

/// Resolves the executor's current instruction index (`@counter - 1`).
fn instruction_key(exec: &super::Executor) -> u32 {
    let counter = exec.arena.get(exec.counter).num;
    (counter - 1.0).max(0.0) as u32
}

/// `exec.timeoutDone(unit, delay)` (`LExecutor.java:97-99`) in ticks.
fn timeout_done(exec: &super::Executor, world: &World, unit: Entity, delay: f32) -> bool {
    let tick = world
        .get_resource::<crate::logic::world::LogicWorldState>()
        .map(|state| state.tick as f32)
        .unwrap_or(0.0);
    tick >= exec.unit_timeouts.get(&unit).copied().unwrap_or(0.0) + delay
}

/// `exec.updateTimeout(unit)`.
fn update_timeout(exec: &mut super::Executor, world: &World, unit: Entity) {
    let tick = world
        .get_resource::<crate::logic::world::LogicWorldState>()
        .map(|state| state.tick as f32)
        .unwrap_or(0.0);
    exec.unit_timeouts.insert(unit, tick);
}

/// `LogicAI.controller = exec.thisv.building()` refresh / install.
pub fn check_logic_ai(
    world: &mut World,
    build_team: u8,
    privileged: bool,
    unit: Entity,
    bound: Option<Entity>,
    control: bool,
    controller_building: Option<Entity>,
) -> Option<Entity> {
    use crate::ai::types::logic::LogicAi;
    if bound != Some(unit) || world.get::<Unit>(unit).is_none() {
        return None;
    }
    if world.get::<UnitCore>(unit).is_some_and(|c| c.dead) {
        return None;
    }
    if !privileged && world.get::<TeamComp>(unit).map(|t| t.team) != Some(build_team) {
        return None;
    }
    if !crate::ai::controller_registry::unit_is_logic_controllable(world, unit) {
        return None;
    }
    match world.get::<ControllerSlot>(unit).map(|c| c.kind) {
        Some(AiKind::Logic) => {
            let has_component = world.get::<LogicAi>(unit).is_some();
            if has_component {
                if let Some(mut ai) = world.get_mut::<LogicAi>(unit) {
                    ai.controller = controller_building;
                }
            } else {
                world
                    .entity_mut(unit)
                    .insert(LogicAi::new(controller_building));
            }
            Some(unit)
        }
        _ if control => {
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
                slot.kind = AiKind::Logic;
                slot.timer = UNIT_TIMEOUT_KEEP;
                slot.target = None;
            }
            world
                .entity_mut(unit)
                .insert(LogicAi::new(controller_building));
            // `unit.mineTile = null; unit.clearBuilding();`
            crate::entities::comp::unit::miner::clear_mine_tile(world, unit);
            clear_building(world, unit);
            Some(unit)
        }
        _ => None,
    }
}

/// Refreshes a logic-controlled unit's `controlTimer`.
pub fn refresh_control_timer(world: &mut World, unit: Entity) {
    if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
        ai.control_timer = crate::ai::types::logic::LOGIC_TIMEOUT_TICKS;
    }
}

/// Resets a unit back to its type's default controller
/// (`Unit.resetController()` -> `type.createController(unit)`).
pub fn reset_controller(world: &mut World, unit: Entity) {
    let def = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
        .and_then(|comp| with_content(world, |content| content.unit(comp.type_id).cloned()))
        .flatten();
    let team = world.get::<TeamComp>(unit).map(|t| t.team).unwrap_or(0);
    reset_controller_def(world, unit, def.as_ref(), team);
}

/// [`reset_controller`] with the already-resolved type (the unit loop holds
/// `UnitRuntime` outside the world, so it cannot use [`with_content`]).
pub fn reset_controller_def(
    world: &mut World,
    unit: Entity,
    def: Option<&crate::content::registries::units::UnitTypeDef>,
    team: u8,
) {
    world
        .entity_mut(unit)
        .remove::<crate::ai::types::logic::LogicAi>();
    let Some(def) = def else {
        return;
    };
    let kind = crate::entities::comp::unit::lifecycle::select_team_controller(world, def, team);
    if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
        slot.kind = kind;
        slot.timer = 0.0;
        slot.target = None;
    }
}

/// `UnitBindI`: returns the next unit of `type_id` in stable spawn order for a
/// team, advancing the per-type binding cursor (`%=`/`++` semantics).
pub fn bind_next(world: &mut World, type_id: u16, team: u8, cursor: &mut u32) -> Option<Entity> {
    let mut matches: Vec<Entity> = Vec::new();
    for entity_ref in world.iter_entities() {
        let entity = entity_ref.id();
        if entity_ref.get::<Unit>().is_none() {
            continue;
        }
        if entity_ref
            .get::<crate::entities::comp::unit::UnitTypeComp>()
            .map(|t| t.type_id.raw())
            != Some(type_id)
        {
            continue;
        }
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
            continue;
        }
        if entity_ref.get::<UnitCore>().is_some_and(|core| core.dead) {
            continue;
        }
        matches.push(entity);
    }
    matches.sort_by_key(|e| {
        (
            world
                .get::<crate::ecs::EntitySeq>(*e)
                .map(|s| s.0)
                .unwrap_or(u64::MAX),
            e.index(),
        )
    });
    if matches.is_empty() {
        return None;
    }
    // `exec.binds[type.id] %= seq.size; ... exec.binds[type.id]++`.
    let index = (*cursor as usize) % matches.len();
    *cursor = (index as u32).wrapping_add(1);
    Some(matches[index])
}

/// `LogicObject::Unit` binding gate: `(u.team == exec.team || privileged) &&
/// u.type.logicControllable` and the unit is alive.
pub fn bind_object_ok(world: &World, team: u8, privileged: bool, unit: Entity) -> bool {
    if world.get::<Unit>(unit).is_none() {
        return false;
    }
    if world.get::<UnitCore>(unit).is_some_and(|core| core.dead) {
        return false;
    }
    if !privileged && world.get::<TeamComp>(unit).map(|t| t.team) != Some(team) {
        return false;
    }
    world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
        .and_then(|comp| {
            with_content(world, |content| {
                content
                    .unit(comp.type_id)
                    .map(|def| def.logic_controllable)
                    .unwrap_or(true)
            })
        })
        .unwrap_or(true)
}

/// `UnitControlI`: applies one command to the bound unit. The full Java
/// `switch(type)` (`LExecutor.java:355-548`).
///
/// Outputs (`within` p4, `getBlock` p3/p4/p5) are written back through the
/// executor arena.
#[allow(clippy::too_many_arguments)]
pub fn apply_control(
    exec: &mut super::Executor,
    world: &mut World,
    unit: Entity,
    type_: LUnitControl,
    p1: VarRef,
    p2: VarRef,
    p3: VarRef,
    p4: VarRef,
    p5: VarRef,
) {
    // `float x1 = World.unconv(p1.numf()), y1 = ...; d1 = ...` (`*8`; the
    // port's `value::conv` is Java's `World.unconv`).
    let x1 = crate::logic::value::conv(exec.arena.get(p1.id()).numf());
    let y1 = crate::logic::value::conv(exec.arena.get(p2.id()).numf());
    let d1 = crate::logic::value::conv(exec.arena.get(p3.id()).numf());

    let control = type_ != LUnitControl::Unbind && type_ != LUnitControl::Within;
    let ai = check_logic_ai(
        world,
        exec.team,
        exec.privileged,
        unit,
        Some(unit),
        control,
        exec.build,
    );
    if ai.is_none() && control {
        // `if(unitObj instanceof Unit unit && (ai != null || !control))` fails.
        return;
    }
    if ai.is_some() {
        refresh_control_timer(world, unit);
    }

    match type_ {
        LUnitControl::Idle | LUnitControl::AutoPathfind => {
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                ai.control = type_;
            }
        }
        LUnitControl::Move
        | LUnitControl::Stop
        | LUnitControl::Approach
        | LUnitControl::Pathfind => {
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                ai.control = type_;
                ai.move_x = x1;
                ai.move_y = y1;
                if type_ == LUnitControl::Approach {
                    ai.move_rad = d1;
                }
            }
            if type_ == LUnitControl::Stop {
                crate::entities::comp::unit::miner::clear_mine_tile(world, unit);
                clear_building(world, unit);
            }
        }
        LUnitControl::Unbind => {
            if world.get::<ControllerSlot>(unit).map(|s| s.kind) == Some(AiKind::Logic) {
                reset_controller(world, unit);
            }
        }
        LUnitControl::Within => {
            let inside = world.get::<Pos>(unit).is_some_and(|pos| {
                let dx = x1 - pos.x;
                let dy = y1 - pos.y;
                dx * dx + dy * dy <= d1 * d1
            });
            super::set_output_num(exec, p4, if inside { 1.0 } else { 0.0 });
        }
        LUnitControl::Target => {
            let shoot = exec.arena.get(p3.id()).as_bool();
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                ai.pos_target = (x1, y1);
                ai.aim_control = type_;
                ai.main_target = None;
                ai.shoot = shoot;
            }
        }
        LUnitControl::Targetp => {
            let target = match exec.arena.get(p1.id()).value_obj() {
                Some(LogicObject::Unit(e)) | Some(LogicObject::Building(e)) => Some(*e),
                _ => None,
            };
            let shoot = exec.arena.get(p2.id()).as_bool();
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                ai.aim_control = type_;
                ai.main_target = target;
                ai.shoot = shoot;
            }
        }
        LUnitControl::Boost => {
            let boost = exec.arena.get(p1.id()).as_bool();
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                ai.boost = boost;
            }
        }
        LUnitControl::Flag => {
            let flag = exec.arena.get(p1.id()).num();
            if let Some(mut core) = world.get_mut::<UnitCore>(unit) {
                core.flag = flag;
            }
        }
        LUnitControl::Mine => {
            let tile_pos = crate::world::TilePos::new(
                crate::world::WorldGrid::to_tile(x1) as i16,
                crate::world::WorldGrid::to_tile(y1) as i16,
            );
            let can_mine = with_content(world, |content| {
                crate::entities::comp::unit::miner::can_mine(world, content, unit)
            })
            .unwrap_or(false);
            if can_mine {
                let valid = unit_valid_mine(world, unit, tile_pos);
                if let Some(mut comp) =
                    world.get_mut::<crate::entities::comp::unit::comp::MinerComp>(unit)
                {
                    comp.mine_tile = if valid { Some(tile_pos) } else { None };
                }
            }
        }
        LUnitControl::PayDrop => {
            if !timeout_done(exec, world, unit, TRANSFER_DELAY) {
                return;
            }
            let has_payload = world
                .get::<crate::entities::comp::unit::comp::UnitPayloads>(unit)
                .is_some_and(|payloads| payloads.has_payload());
            if has_payload {
                emit_payload_event(world, unit, 2, x1, y1, false);
                update_timeout(exec, world, unit);
            }
        }
        LUnitControl::PayTake => {
            if !timeout_done(exec, world, unit, TRANSFER_DELAY) {
                return;
            }
            let units = exec.arena.get(p1.id()).as_bool();
            emit_payload_event(world, unit, if units { 0 } else { 1 }, x1, y1, units);
            update_timeout(exec, world, unit);
        }
        LUnitControl::PayEnter => {
            // `Call.unitBuildingControlSelect` (unit enters a controllable
            // building) has no port path yet: it needs the building-unit
            // possession seam (I01 T12). Deferred, recorded in the summary.
        }
        LUnitControl::Build => {
            let enabled =
                exec.privileged || logic_unit_build_enabled(crate::logic::blocks::rules_ref(world));
            let block_id = match exec.arena.get(p3.id()).value_obj() {
                Some(LogicObject::Content(c)) if c.type_ == crate::content::ContentType::Block => {
                    Some(crate::content::BlockId::new(c.id))
                }
                _ => None,
            };
            let Some(block_id) = block_id else {
                return;
            };
            if !enabled || !unit_can_build(world, unit) {
                return;
            }
            let block_ok = with_content(world, |content| {
                content.block(block_id).is_some_and(|def| {
                    def.build_visibility
                        != crate::content::registries::blocks::BuildVisibility::Hidden
                })
            })
            .unwrap_or(false);
            if !block_ok {
                return;
            }
            let ts = crate::config::TILESIZE as f32;
            let offset = with_content(world, |content| {
                content.block(block_id).map(|def| def.offset).unwrap_or(0.0)
            })
            .unwrap_or(0.0);
            let x = crate::world::WorldGrid::to_tile(x1 - offset / ts);
            let y = crate::world::WorldGrid::to_tile(y1 - offset / ts);
            let rot = exec.arena.get(p4.id()).numi().rem_euclid(4) as u8;
            let conf = exec.arena.get(p5.id()).value_obj().cloned();
            let conf = match conf {
                Some(LogicObject::Content(c)) => {
                    crate::world::config::ConfigValue::Content(c.type_, c.id)
                }
                Some(LogicObject::Building(e)) => crate::world::config::ConfigValue::Building(e),
                _ => crate::world::config::ConfigValue::None,
            };
            let plans_empty = world
                .get::<crate::sim::builder::BuilderPlans>(unit)
                .is_none_or(|plans| plans.plans.is_empty());
            let changed = world
                .get::<crate::ai::types::logic::LogicAi>(unit)
                .is_some_and(|ai| {
                    ai.plan.x as i32 != x || ai.plan.y as i32 != y || ai.plan.block != block_id
                });
            let tile_blocks = with_grid(world, |grid| {
                grid.tiles.getn(x, y).map(|tile| (tile.block, tile.build))
            })
            .flatten();
            let already = match tile_blocks {
                Some((block, Some(build))) => {
                    block == block_id
                        && world
                            .get::<crate::entities::comp::Building>(build)
                            .is_some_and(|b| b.rotation == rot)
                }
                _ => false,
            };
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                if changed || plans_empty {
                    ai.plan.progress = 0.0;
                    ai.plan.initialized = false;
                    ai.plan.stuck = false;
                }
                ai.plan.x = x as i16;
                ai.plan.y = y as i16;
                ai.plan.rotation = rot;
                ai.plan.block = block_id;
                ai.plan.breaking = false;
                ai.plan.config = conf;
            }
            clear_building(world, unit);
            if tile_blocks.is_some()
                && !already
                && let Some(plan) = world
                    .get::<crate::ai::types::logic::LogicAi>(unit)
                    .map(|ai| ai.plan.clone())
                && let Some(mut plans) = world.get_mut::<crate::sim::builder::BuilderPlans>(unit)
            {
                plans.update_building = true;
                plans.add_build(plan, true);
            }
        }
        LUnitControl::Deconstruct => {
            let enabled = exec.privileged
                || logic_unit_deconstruct_enabled(crate::logic::blocks::rules_ref(world));
            if !enabled || !unit_can_build(world, unit) {
                return;
            }
            let x = crate::world::WorldGrid::to_tile(x1);
            let y = crate::world::WorldGrid::to_tile(y1);
            let plans_empty = world
                .get::<crate::sim::builder::BuilderPlans>(unit)
                .is_none_or(|plans| plans.plans.is_empty());
            let changed = world
                .get::<crate::ai::types::logic::LogicAi>(unit)
                .is_some_and(|ai| {
                    ai.plan.x as i32 != x || ai.plan.y as i32 != y || !ai.plan.breaking
                });
            if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
                if changed || plans_empty {
                    ai.plan.progress = 0.0;
                    ai.plan.initialized = false;
                    ai.plan.stuck = false;
                }
                ai.plan.x = x as i16;
                ai.plan.y = y as i16;
                ai.plan.breaking = true;
            }
            clear_building(world, unit);
            let valid = valid_break_at(world, exec.team, x, y);
            if valid
                && let Some(plan) = world
                    .get::<crate::ai::types::logic::LogicAi>(unit)
                    .map(|ai| ai.plan.clone())
                && let Some(mut plans) = world.get_mut::<crate::sim::builder::BuilderPlans>(unit)
            {
                plans.update_building = true;
                plans.add_build(plan, true);
            }
        }
        LUnitControl::GetBlock => {
            let range = world
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
                .and_then(|comp| {
                    with_content(world, |content| {
                        content.unit(comp.type_id).map(|def| {
                            if def.max_range > 0.0 {
                                def.max_range
                            } else {
                                def.range
                            }
                        })
                    })
                })
                .flatten()
                .unwrap_or(0.0)
                .max(
                    world
                        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
                        .and_then(|comp| {
                            with_content(world, |content| {
                                content.unit(comp.type_id).map(|def| def.build_range)
                            })
                        })
                        .flatten()
                        .unwrap_or(0.0),
                );
            let within = world.get::<Pos>(unit).is_some_and(|pos| {
                let dx = x1 - pos.x;
                let dy = y1 - pos.y;
                dx * dx + dy * dy <= range * range
            });
            if !within {
                super::set_output_obj(exec, p3, None);
                super::set_output_obj(exec, p4, None);
                super::set_output_obj(exec, p5, None);
                return;
            }
            match tile_at_world(world, x1, y1) {
                Some(tile) => {
                    use crate::content::ContentRef;
                    super::set_output_obj(
                        exec,
                        p3,
                        Some(LogicObject::Content(ContentRef::block(tile.block))),
                    );
                    super::set_output_obj(exec, p4, tile.build.map(LogicObject::Building));
                    let layer = if tile.overlay == crate::content::BlockId::AIR {
                        tile.floor
                    } else {
                        tile.overlay
                    };
                    super::set_output_obj(
                        exec,
                        p5,
                        Some(LogicObject::Content(ContentRef::block(layer))),
                    );
                }
                None => {
                    super::set_output_obj(exec, p3, None);
                    super::set_output_obj(exec, p4, None);
                    super::set_output_obj(exec, p5, None);
                }
            }
        }
        LUnitControl::ItemDrop => {
            let p1_obj = exec.arena.get(p1.id()).value_obj().cloned();
            let clear_item = matches!(
                &p1_obj,
                Some(LogicObject::Content(c))
                    if c.type_ == crate::content::ContentType::Block && c.id == 0
            );
            if clear_item {
                if let Some(mut items) =
                    world.get_mut::<crate::entities::comp::unit::comp::ItemsComp>(unit)
                {
                    items.clear_item();
                }
                return;
            }
            if !timeout_done(exec, world, unit, TRANSFER_DELAY) {
                return;
            }
            let Some(LogicObject::Building(build)) = p1_obj else {
                return;
            };
            let amount = exec.arena.get(p2.id()).numi();
            let Some((item, held)) = world
                .get::<crate::entities::comp::unit::comp::ItemsComp>(unit)
                .and_then(|items| items.item)
            else {
                return;
            };
            let dropped = held.min(amount);
            let team = world.get::<TeamComp>(unit).map(|t| t.team).unwrap_or(0);
            let build_team = world.get::<TeamComp>(build).map(|t| t.team).unwrap_or(0);
            let within = building_center(world, build).is_some_and(|(bx, by)| {
                world.get::<Pos>(unit).is_some_and(|pos| {
                    let size = with_content(world, |content| {
                        world
                            .get::<crate::entities::comp::Building>(build)
                            .and_then(|b| content.block(b.block))
                            .map(|def| def.size as f32)
                            .unwrap_or(1.0)
                    })
                    .unwrap_or(1.0);
                    let range =
                        LOGIC_ITEM_TRANSFER_RANGE + size * crate::config::TILESIZE as f32 / 2.0;
                    let dx = bx - pos.x;
                    let dy = by - pos.y;
                    dx * dx + dy * dy <= range * range
                })
            });
            if build_team != team
                || !building_is_valid(world, build)
                || !building_allows_deposit(world, build)
                || dropped <= 0
                || !within
            {
                return;
            }
            let accepted = crate::world::blocks::distribution::transfer::dispatch_accept_stack(
                world,
                build,
                item,
                dropped,
                Some(unit),
            );
            if accepted > 0 {
                if let Some(mut items) =
                    world.get_mut::<crate::entities::comp::unit::comp::ItemsComp>(unit)
                {
                    let left = held - accepted;
                    items.item = if left > 0 { Some((item, left)) } else { None };
                }
                crate::world::blocks::distribution::transfer::dispatch_handle_stack(
                    world, build, item, accepted,
                );
                update_timeout(exec, world, unit);
            }
        }
        LUnitControl::ItemTake => {
            if !timeout_done(exec, world, unit, TRANSFER_DELAY) {
                return;
            }
            let Some(LogicObject::Building(build)) = exec.arena.get(p1.id()).value_obj().cloned()
            else {
                return;
            };
            let amount = exec.arena.get(p3.id()).numi();
            let item = match exec.arena.get(p2.id()).value_obj() {
                Some(LogicObject::Content(c)) if c.type_ == crate::content::ContentType::Item => {
                    Some(crate::content::ItemId::new(c.id))
                }
                _ => None,
            };
            let Some(item) = item else {
                return;
            };
            let team = world.get::<TeamComp>(unit).map(|t| t.team).unwrap_or(0);
            let build_team = world.get::<TeamComp>(build).map(|t| t.team).unwrap_or(0);
            if build_team != team
                || !building_is_valid(world, build)
                || world
                    .get::<crate::world::modules::ItemModule>(build)
                    .is_none()
            {
                return;
            }
            let within = building_center(world, build).is_some_and(|(bx, by)| {
                world.get::<Pos>(unit).is_some_and(|pos| {
                    let size = with_content(world, |content| {
                        world
                            .get::<crate::entities::comp::Building>(build)
                            .and_then(|b| content.block(b.block))
                            .map(|def| def.size as f32)
                            .unwrap_or(1.0)
                    })
                    .unwrap_or(1.0);
                    let range =
                        LOGIC_ITEM_TRANSFER_RANGE + size * crate::config::TILESIZE as f32 / 2.0;
                    let dx = bx - pos.x;
                    let dy = by - pos.y;
                    dx * dx + dy * dy <= range * range
                })
            });
            if !within {
                return;
            }
            let stored =
                crate::world::blocks::distribution::transfer::item_count(world, build, item);
            let taken = stored.min(amount.min(unit_max_accepted(world, unit, item)));
            if taken > 0 {
                let removed = crate::world::blocks::distribution::transfer::dispatch_remove_stack(
                    world, build, item, taken,
                );
                if removed > 0 {
                    let mut items = world
                        .get::<crate::entities::comp::unit::comp::ItemsComp>(unit)
                        .copied()
                        .unwrap_or_default();
                    items.add_item(item, removed);
                    world.entity_mut(unit).insert(items);
                    update_timeout(exec, world, unit);
                }
            }
        }
    }
}

/// `Build.validBreak` against the live grid.
fn valid_break_at(world: &World, team: u8, x: i32, y: i32) -> bool {
    if let Some(runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() {
        let rules = world
            .get_resource::<crate::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        return crate::world::build::valid_break(
            &runtime.content,
            &runtime.grid,
            Some(world),
            &rules,
            team,
            x,
            y,
        );
    }
    false
}

/// Emits the payload command as a [`crate::logic::world::LogicWorldEvent`]; the
/// owning `Sim` applies it through the same helpers the player input uses
/// (`payload_pickup_unit`/`payload_pickup_build`/`payload_drop`), because the
/// block runtime is not reachable from the logic VM's `World`.
fn emit_payload_event(world: &mut World, carrier: Entity, kind: u8, x: f32, y: f32, units: bool) {
    if let Some(mut state) = world.get_resource_mut::<crate::logic::world::LogicWorldState>() {
        state.emit(crate::logic::world::LogicWorldEvent::Payload {
            carrier: carrier.to_bits(),
            kind,
            x,
            y,
            units,
        });
    }
}

/// `UnitLocateI` — the ore/building/spawn/damaged scans, with the per-
/// instruction 40-tick cache (`ai.exec_cache` + `checkTargetTimer`).
#[allow(clippy::too_many_arguments)]
pub fn run_ulocate(
    exec: &mut super::Executor,
    world: &mut World,
    locate: crate::logic::enums::LLocate,
    flag: crate::logic::enums::BlockFlag,
    enemy: VarRef,
    ore: VarRef,
    out_x: VarRef,
    out_y: VarRef,
    out_found: VarRef,
    out_build: VarRef,
) {
    // `if(!exec.privileged && !state.rules.logicUnitControl) return;` — Java
    // leaves every output untouched on the gate.
    if !exec.privileged && !logic_unit_control_enabled(crate::logic::blocks::rules_ref(world)) {
        return;
    }
    let unit = match exec.arena.get(exec.unit).value_obj() {
        Some(LogicObject::Unit(unit)) => *unit,
        _ => {
            // `else { outFound.setbool(false); }`.
            super::set_output_num(exec, out_found, 0.0);
            return;
        }
    };
    if check_logic_ai(
        world,
        exec.team,
        exec.privileged,
        unit,
        Some(unit),
        true,
        exec.build,
    )
    .is_none()
    {
        super::set_output_num(exec, out_found, 0.0);
        return;
    }
    refresh_control_timer(world, unit);

    let key = instruction_key(exec);
    // `if(ai.checkTargetTimer(this))` -> recompute; else replay the cache.
    let recompute = world
        .get_mut::<crate::ai::types::logic::LogicAi>(unit)
        .is_some_and(|mut ai| ai.check_target_timer(key));
    if !recompute {
        let cache = world
            .get::<crate::ai::types::logic::LogicAi>(unit)
            .and_then(|ai| ai.exec_cache.get(&key).copied())
            .unwrap_or_default();
        super::set_output_num(exec, out_x, cache.x as f64);
        super::set_output_num(exec, out_y, cache.y as f64);
        super::set_output_num(exec, out_found, if cache.found { 1.0 } else { 0.0 });
        super::set_output_obj(exec, out_build, cache.build.map(LogicObject::Building));
        return;
    }

    let (ux, uy) = unit_pos(world, unit).unwrap_or((0.0, 0.0));
    let team = world.get::<TeamComp>(unit).map(|t| t.team).unwrap_or(0);

    let result: Option<(f32, f32, Option<Entity>, bool)> = match locate {
        crate::logic::enums::LLocate::Ore => {
            let item = match exec.arena.get(ore.id()).value_obj() {
                Some(LogicObject::Content(c)) if c.type_ == crate::content::ContentType::Item => {
                    Some(crate::content::ItemId::new(c.id))
                }
                _ => None,
            };
            item.and_then(|item| find_closest_ore(world, ux, uy, item))
                .map(|(x, y, build)| (x, y, build, false))
        }
        crate::logic::enums::LLocate::Building => {
            let enemy = exec.arena.get(enemy.id()).as_bool();
            find_closest_building(world, ux, uy, team, enemy, flag)
                .map(|(x, y, build)| (x, y, build, true))
        }
        crate::logic::enums::LLocate::Damaged => {
            find_closest_damaged(world, ux, uy, team).map(|(x, y, build)| (x, y, build, true))
        }
        crate::logic::enums::LLocate::Spawn => {
            find_closest_spawn(world, ux, uy).map(|(x, y, build)| (x, y, build, false))
        }
    };

    // `if(res != null && (!build || res.build != null))`.
    let found = match &result {
        Some((_, _, build, is_build_scan)) => !is_build_scan || build.is_some(),
        None => false,
    };
    if found {
        let (x, y, build, _) = result.unwrap_or((0.0, 0.0, None, false));
        if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
            ai.exec_cache.insert(
                key,
                crate::ai::types::logic::LocateCache {
                    x,
                    y,
                    found: true,
                    build,
                },
            );
        }
        super::set_output_num(exec, out_x, x as f64);
        super::set_output_num(exec, out_y, y as f64);
        super::set_output_num(exec, out_found, 1.0);
    } else {
        if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
            ai.exec_cache.insert(
                key,
                crate::ai::types::logic::LocateCache {
                    found: false,
                    ..Default::default()
                },
            );
        }
        super::set_output_num(exec, out_found, 0.0);
    }

    // `outBuild` gate: `res.build != null && (within(max(unit.range(),
    // buildingRange)) || same team)`.
    let out_build_entity = result.as_ref().and_then(|(_, _, build, _)| *build);
    let range = {
        let def_range = with_content(world, |content| {
            world
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
                .and_then(|comp| content.unit(comp.type_id))
                .map(|def| {
                    if def.max_range > 0.0 {
                        def.max_range
                    } else {
                        def.range
                    }
                })
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0);
        def_range.max(BUILDING_RANGE)
    };
    let keep = out_build_entity.filter(|build| {
        let same_team = world
            .get::<TeamComp>(*build)
            .is_some_and(|t| t.team == exec.team);
        let in_range =
            building_center(world, *build).is_some_and(|(bx, by)| within(ux, uy, range, bx, by));
        same_team || in_range
    });
    super::set_output_obj(exec, out_build, keep.map(LogicObject::Building));
}

/// Reads a unit's position (`Pos`).
pub fn unit_pos(world: &World, unit: Entity) -> Option<(f32, f32)> {
    world.get::<Pos>(unit).map(|p| (p.x, p.y))
}

/// Reads a unit's hit size (`HitboxComp.hitSize`).
pub fn unit_hit_size(world: &World, unit: Entity) -> f32 {
    world
        .get::<HitboxComp>(unit)
        .map(|h| h.hit_size)
        .unwrap_or(0.0)
}

/// `Vars.buildingRange` (the floor of the `ulocate` build-output range check).
pub const BUILDING_RANGE: f32 = 220.0;

/// `World.tileSize` as `f32`.
const TILE_PX: f32 = crate::config::TILESIZE as f32;

/// Maps the logic `BlockFlag` subset onto the content registry's flags.
fn content_flag(
    flag: crate::logic::enums::BlockFlag,
) -> crate::content::registries::blocks::BlockFlag {
    use crate::content::registries::blocks::BlockFlag as C;
    use crate::logic::enums::BlockFlag as L;
    match flag {
        L::Core => C::Core,
        L::Storage => C::Storage,
        L::Generator => C::Generator,
        L::Turret => C::Turret,
        L::Factory => C::Factory,
        L::Repair => C::Repair,
        L::Battery => C::Battery,
        L::Reactor => C::Reactor,
        L::Drill => C::Drill,
        L::Shield => C::Shield,
    }
}

/// `Geometry.findClosest` over ore tiles (`BlockIndexer.findClosestOre`).
fn find_closest_ore(
    world: &World,
    x: f32,
    y: f32,
    item: crate::content::ItemId,
) -> Option<(f32, f32, Option<Entity>)> {
    let index = world.get_resource::<crate::logic::world::LogicContentIndex>()?;
    with_grid(world, |grid| {
        let mut best: Option<(f32, i32, i32)> = None;
        for (pos, tile_index) in grid.iter_row_major() {
            let tile = grid.tile_ref(tile_index);
            if tile.block != crate::content::BlockId::AIR {
                continue;
            }
            let drop = index
                .ore_drop_for(tile.overlay)
                .or_else(|| index.ore_drop_for(tile.floor));
            if drop != Some(item) {
                continue;
            }
            let tx = pos.x() as i32;
            let ty = pos.y() as i32;
            let wx = (tx as f32 + 0.5) * TILE_PX;
            let wy = (ty as f32 + 0.5) * TILE_PX;
            let dist2 = (wx - x) * (wx - x) + (wy - y) * (wy - y);
            if best.is_none_or(|(best_dist, _, _)| dist2 < best_dist) {
                best = Some((dist2, tx, ty));
            }
        }
        best.map(|(_, tx, ty)| (tx as f32 + 0.5, ty as f32 + 0.5, None))
    })
    .flatten()
}

/// `Geometry.findClosest` over flagged buildings (`BlockIndexer.getFlagged`).
fn find_closest_building(
    world: &World,
    x: f32,
    y: f32,
    team: u8,
    enemy: bool,
    flag: crate::logic::enums::BlockFlag,
) -> Option<(f32, f32, Option<Entity>)> {
    let index = world.get_resource::<crate::logic::world::LogicContentIndex>()?;
    let want = content_flag(flag);
    let mut best: Option<(f32, Entity, f32, f32)> = None;
    for entity_ref in world.iter_entities() {
        let Some(building) = entity_ref.get::<crate::entities::comp::Building>() else {
            continue;
        };
        if !index.flags_of(building.block).contains(&want) {
            continue;
        }
        let Some(other_team) = entity_ref.get::<TeamComp>().map(|t| t.team) else {
            continue;
        };
        if (enemy && other_team == team) || (!enemy && other_team != team) {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dist2 = (pos.x - x) * (pos.x - x) + (pos.y - y) * (pos.y - y);
        if best.is_none_or(|(best_dist, _, _, _)| dist2 < best_dist) {
            best = Some((dist2, entity_ref.id(), pos.x, pos.y));
        }
    }
    best.map(|(_, entity, bx, by)| (bx / TILE_PX, by / TILE_PX, Some(entity)))
}

/// `Units.findDamagedTile`: closest damaged building of `team`.
fn find_closest_damaged(
    world: &World,
    x: f32,
    y: f32,
    team: u8,
) -> Option<(f32, f32, Option<Entity>)> {
    let mut best: Option<(f32, Entity, f32, f32)> = None;
    for entity_ref in world.iter_entities() {
        if entity_ref
            .get::<crate::entities::comp::Building>()
            .is_none()
        {
            continue;
        }
        let Some(health) = entity_ref.get::<crate::entities::comp::Health>() else {
            continue;
        };
        if !health.damaged() {
            continue;
        }
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dist2 = (pos.x - x) * (pos.x - x) + (pos.y - y) * (pos.y - y);
        if best.is_none_or(|(best_dist, _, _, _)| dist2 < best_dist) {
            best = Some((dist2, entity_ref.id(), pos.x, pos.y));
        }
    }
    best.map(|(_, entity, bx, by)| (bx / TILE_PX, by / TILE_PX, Some(entity)))
}

/// `Geometry.findClosest` over spawn overlays (`Spawner.getSpawns`).
fn find_closest_spawn(world: &World, x: f32, y: f32) -> Option<(f32, f32, Option<Entity>)> {
    let index = world.get_resource::<crate::logic::world::LogicContentIndex>()?;
    with_grid(world, |grid| {
        let mut best: Option<(f32, i32, i32)> = None;
        for (pos, tile_index) in grid.iter_row_major() {
            let tile = grid.tile_ref(tile_index);
            if !index.is_spawn_overlay(tile.overlay) {
                continue;
            }
            let tx = pos.x() as i32;
            let ty = pos.y() as i32;
            let wx = (tx as f32 + 0.5) * TILE_PX;
            let wy = (ty as f32 + 0.5) * TILE_PX;
            let dist2 = (wx - x) * (wx - x) + (wy - y) * (wy - y);
            if best.is_none_or(|(best_dist, _, _)| dist2 < best_dist) {
                best = Some((dist2, tx, ty));
            }
        }
        best.map(|(_, tx, ty)| (tx as f32 + 0.5, ty as f32 + 0.5, None))
    })
    .flatten()
}

/// `Mathf.within`.
fn within(ax: f32, ay: f32, radius: f32, bx: f32, by: f32) -> bool {
    let dx = ax - bx;
    let dy = ay - by;
    dx * dx + dy * dy <= radius * radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::blocks::DefaultLogicRules;

    #[test]
    fn gating_defaults_match_upstream() {
        let rules = DefaultLogicRules;
        assert!(logic_unit_control_enabled(&rules));
        assert!(logic_unit_build_enabled(&rules));
        // Upstream `logicUnitDeconstruct` defaults false.
        assert!(!logic_unit_deconstruct_enabled(&rules));
    }

    #[test]
    fn check_logic_ai_installs_logic_controller() {
        let mut world = World::new();
        let unit = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                UnitCore::new(0.0),
                ControllerSlot::new(AiKind::Ground),
            ))
            .id();
        let controlled = check_logic_ai(&mut world, 0, false, unit, Some(unit), true, None);
        assert_eq!(controlled, Some(unit));
        assert_eq!(
            world.get::<ControllerSlot>(unit).unwrap().kind,
            AiKind::Logic
        );
    }

    #[test]
    fn commanded_unit_is_not_logic_controllable() {
        let mut world = World::new();
        let unit = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                UnitCore::new(0.0),
                ControllerSlot::new(AiKind::Command),
            ))
            .id();
        // With no order the CommandAI is controllable; with one it is not
        // (`CommandAI.isLogicControllable` = `!hasCommand()`, the live
        // `ControllerSlot.target`).
        assert!(check_logic_ai(&mut world, 0, false, unit, Some(unit), true, None).is_some());
        world.get_mut::<ControllerSlot>(unit).unwrap().kind = AiKind::Command;
        world
            .entity_mut(unit)
            .remove::<crate::ai::types::logic::LogicAi>();
        world.get_mut::<ControllerSlot>(unit).unwrap().target =
            Some(crate::world::TilePos::new(4, 4));
        assert!(check_logic_ai(&mut world, 0, false, unit, Some(unit), true, None).is_none());
    }

    #[test]
    fn bind_next_wraps_cursor() {
        let mut world = World::new();
        let a = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                crate::entities::comp::unit::UnitTypeComp {
                    type_id: crate::content::UnitTypeId::new(1),
                },
            ))
            .id();
        let b = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                crate::entities::comp::unit::UnitTypeComp {
                    type_id: crate::content::UnitTypeId::new(1),
                },
            ))
            .id();
        let mut cursor = 0u32;
        assert_eq!(bind_next(&mut world, 1, 0, &mut cursor), Some(a));
        assert_eq!(bind_next(&mut world, 1, 0, &mut cursor), Some(b));
        assert_eq!(bind_next(&mut world, 1, 0, &mut cursor), Some(a));
    }
}
