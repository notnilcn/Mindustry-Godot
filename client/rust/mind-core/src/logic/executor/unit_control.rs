// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit binding and control bridge (plan 13 M5).
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java` (`UnitBindI`,
//! `UnitControlI`, `UnitLocateI`, `UnitRadarI`) and
//! `entities/comp/UnitComp.java` (`control`, `checkLogicAI`). Plan 11 owns the
//! `LogicAI` movement body; this module installs the controller kind, applies
//! the instruction-side setters plan 11 exposes on [`ControllerSlot`], and
//! enforces `Rules.logic_unit_control`/`logic_unit_build`/`logic_unit_deconstruct`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ai::{AiKind, ControllerSlot};
use crate::entities::comp::unit::{HitboxComp, UnitCore};
use crate::entities::comp::{Pos, TeamComp, Unit, Vel};
use crate::logic::enums::LUnitControl;
use crate::logic::value::{LVar, LogicObject, VarRef};

/// `LogicAI.control` timeout refresh (10 s at 60 Hz).
pub const UNIT_TIMEOUT_KEEP: f32 = 600.0;
/// `LExecutor` transfer delay (`90` ticks).
pub const TRANSFER_DELAY: f32 = 90.0;

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

/// Ensures a unit carries the plan-11 `LogicAi` state component and refreshes
/// its control timeout.
fn ensure_logic_ai(world: &mut World, unit: Entity) {
    use crate::ai::types::logic::LogicAi;
    if world.get::<LogicAi>(unit).is_none() {
        world.entity_mut(unit).insert(LogicAi::new(unit, i32::MIN));
    }
    if let Some(mut ai) = world.get_mut::<LogicAi>(unit) {
        ai.control();
    }
}

/// `UnitComp.checkLogicAI`: validates a bound unit and installs/reuses a
/// `LogicAI` controller when `control` is set.
///
/// Returns the controlled entity, or `None` if the unit may not be controlled.
pub fn check_logic_ai(
    world: &mut World,
    build_team: u8,
    privileged: bool,
    unit: Entity,
    bound: Option<Entity>,
    control: bool,
) -> Option<Entity> {
    if bound != Some(unit) || world.get::<Unit>(unit).is_none() {
        return None;
    }
    if world.get::<UnitCore>(unit).is_some_and(|c| c.dead) {
        return None;
    }
    if !privileged && world.get::<TeamComp>(unit).map(|t| t.team) != Some(build_team) {
        return None;
    }
    match world.get::<ControllerSlot>(unit).map(|c| c.kind) {
        Some(AiKind::Logic) => {
            ensure_logic_ai(world, unit);
            Some(unit)
        }
        _ if control => {
            {
                let mut slot = world.get_mut::<ControllerSlot>(unit)?;
                slot.kind = AiKind::Logic;
                slot.timer = UNIT_TIMEOUT_KEEP;
                slot.target = None;
            }
            ensure_logic_ai(world, unit);
            Some(unit)
        }
        _ => Some(unit),
    }
}

/// Refreshes a logic-controlled unit's timeout (`LogicAI.control`).
pub fn refresh_control_timer(world: &mut World, unit: Entity) {
    if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
        slot.timer = UNIT_TIMEOUT_KEEP;
    }
    if let Some(mut ai) = world.get_mut::<crate::ai::types::logic::LogicAi>(unit) {
        ai.control();
    }
}

/// `UnitBindI`: returns the next unit of `type_id` in stable slot order for a
/// team, advancing the per-type binding cursor.
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
        matches.push(entity);
    }
    matches.sort_by_key(|e| e.index());
    if matches.is_empty() {
        *cursor = 0;
        return None;
    }
    let index = (*cursor as usize) % matches.len();
    *cursor = (index + 1) as u32;
    Some(matches[index])
}

/// `UnitControlI` setter application.
///
/// Delegates to the plan-11 `LogicAi` state component (owned by plan 11's
/// `LogicAI::apply_control`) so the movement body reads the same fields; units
/// without the component fall back to the legacy controller-slot behavior.
pub fn apply_control(
    world: &mut World,
    unit: Entity,
    control: LUnitControl,
    params: &[LVar],
) -> bool {
    let nums: Vec<f64> = params.iter().map(|v| v.num()).collect();
    if world
        .get::<crate::ai::types::logic::LogicAi>(unit)
        .is_some()
    {
        let keep = world
            .get_mut::<crate::ai::types::logic::LogicAi>(unit)
            .map(|mut ai| ai.apply_control(control, &nums))
            .unwrap_or(true);
        if !keep && let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
            slot.target = None;
        }
        return keep;
    }

    let get = |i: usize| -> f64 { params.get(i).map(|v| v.num()).unwrap_or(0.0) };
    match control {
        LUnitControl::Idle | LUnitControl::Stop => {
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
                slot.target = None;
            }
            if let Some(mut vel) = world.get_mut::<Vel>(unit) {
                vel.x = 0.0;
                vel.y = 0.0;
            }
            true
        }
        LUnitControl::Move
        | LUnitControl::Approach
        | LUnitControl::Pathfind
        | LUnitControl::AutoPathfind => {
            let x = (get(0) as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let y = (get(1) as i32).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
                slot.target = Some(crate::world::TilePos::new(x, y));
            }
            true
        }
        LUnitControl::Boost => {
            let boost = get(0) != 0.0;
            if let Some(mut core) = world.get_mut::<UnitCore>(unit) {
                core.boosting = boost;
            }
            true
        }
        LUnitControl::Flag
        | LUnitControl::Within
        | LUnitControl::Target
        | LUnitControl::Targetp
        | LUnitControl::ItemDrop
        | LUnitControl::ItemTake
        | LUnitControl::PayDrop
        | LUnitControl::PayTake
        | LUnitControl::PayEnter
        | LUnitControl::Mine
        | LUnitControl::Build
        | LUnitControl::Deconstruct
        | LUnitControl::GetBlock => true,
        LUnitControl::Unbind => {
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
                slot.target = None;
            }
            true
        }
    }
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

/// Sets the `ulocate` not-found outputs (`UnitLocateI` else branch).
fn locate_not_found(
    exec: &mut super::Executor,
    out_x: VarRef,
    out_y: VarRef,
    out_found: VarRef,
    out_build: VarRef,
) {
    super::set_output_num(exec, out_x, 0.0);
    super::set_output_num(exec, out_y, 0.0);
    super::set_output_num(exec, out_found, 0.0);
    super::set_output_obj(exec, out_build, None);
}

/// `UnitLocateI` — the ore/building/spawn/damaged scans.
///
/// Plan 06's `WorldGrid` and the plan-13 [`LogicContentIndex`] projection are
/// consulted where installed; a world without them reports "not found" (the
/// pre-M7 behavior). Building selection matches `BlockIndexer.getFlagged`/
/// `getEnemy`; the build output applies upstream's
/// `max(unit.range(), buildingRange)` / same-team gate.
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
    if !exec.privileged && !logic_unit_control_enabled(crate::logic::blocks::rules_ref(world)) {
        locate_not_found(exec, out_x, out_y, out_found, out_build);
        return;
    }
    let unit = match exec.arena.get(exec.unit).value_obj() {
        Some(LogicObject::Unit(unit)) => *unit,
        _ => {
            locate_not_found(exec, out_x, out_y, out_found, out_build);
            return;
        }
    };
    if check_logic_ai(world, exec.team, exec.privileged, unit, Some(unit), true).is_none() {
        locate_not_found(exec, out_x, out_y, out_found, out_build);
        return;
    }
    refresh_control_timer(world, unit);

    let (ux, uy) = unit_pos(world, unit).unwrap_or((0.0, 0.0));
    let team = world.get::<TeamComp>(unit).map(|t| t.team).unwrap_or(0);

    let result: Option<(f32, f32, Option<Entity>)> = match locate {
        crate::logic::enums::LLocate::Ore => {
            let item = match exec.arena.get(ore.id()).value_obj() {
                Some(LogicObject::Content(c)) if c.type_ == crate::content::ContentType::Item => {
                    Some(crate::content::ItemId::new(c.id))
                }
                _ => None,
            };
            item.and_then(|item| find_closest_ore(world, ux, uy, item))
        }
        crate::logic::enums::LLocate::Building => {
            let enemy = exec.arena.get(enemy.id()).as_bool();
            find_closest_building(world, ux, uy, team, enemy, flag)
        }
        crate::logic::enums::LLocate::Damaged => find_closest_damaged(world, ux, uy, team),
        crate::logic::enums::LLocate::Spawn => find_closest_spawn(world, ux, uy),
    };

    let Some((x, y, build)) = result else {
        locate_not_found(exec, out_x, out_y, out_found, out_build);
        return;
    };
    super::set_output_num(exec, out_x, x as f64);
    super::set_output_num(exec, out_y, y as f64);
    super::set_output_num(exec, out_found, 1.0);
    let build = build.filter(|entity| {
        let same_team = world
            .get::<TeamComp>(*entity)
            .is_some_and(|t| t.team == exec.team);
        let in_range = world
            .get::<Pos>(*entity)
            .is_some_and(|p| within(ux, uy, BUILDING_RANGE, p.x, p.y));
        same_team || in_range
    });
    super::set_output_obj(exec, out_build, build.map(LogicObject::Building));
}

/// `Geometry.findClosest` over ore tiles (`BlockIndexer.findClosestOre`).
fn find_closest_ore(
    world: &World,
    x: f32,
    y: f32,
    item: crate::content::ItemId,
) -> Option<(f32, f32, Option<Entity>)> {
    let grid = world.get_resource::<crate::world::WorldGrid>()?;
    let index = world.get_resource::<crate::logic::world::LogicContentIndex>()?;
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
    let grid = world.get_resource::<crate::world::WorldGrid>()?;
    let index = world.get_resource::<crate::logic::world::LogicContentIndex>()?;
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
    use crate::logic::value::LVar;

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
        let controlled = check_logic_ai(&mut world, 0, false, unit, Some(unit), true);
        assert_eq!(controlled, Some(unit));
        assert_eq!(
            world.get::<ControllerSlot>(unit).unwrap().kind,
            AiKind::Logic
        );
    }

    #[test]
    fn apply_move_sets_target_and_idle_clears() {
        let mut world = World::new();
        let unit = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                UnitCore::new(0.0),
                ControllerSlot::new(AiKind::Logic),
            ))
            .id();
        let mut x = LVar::new("x");
        x.set_num(5.0);
        let mut y = LVar::new("y");
        y.set_num(6.0);
        assert!(apply_control(&mut world, unit, LUnitControl::Move, &[x, y]));
        assert!(world.get::<ControllerSlot>(unit).unwrap().target.is_some());
        assert!(apply_control(&mut world, unit, LUnitControl::Stop, &[]));
        assert!(world.get::<ControllerSlot>(unit).unwrap().target.is_none());
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

    #[test]
    fn check_logic_ai_installs_logic_ai_state() {
        let mut world = World::new();
        let unit = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                UnitCore::new(0.0),
                ControllerSlot::new(AiKind::Ground),
            ))
            .id();
        assert_eq!(
            check_logic_ai(&mut world, 0, false, unit, Some(unit), true),
            Some(unit)
        );
        let ai = world
            .get::<crate::ai::types::logic::LogicAi>(unit)
            .expect("logic ai state");
        assert!(ai.controlled);
    }

    #[test]
    fn apply_control_delegates_to_logic_ai() {
        let mut world = World::new();
        let unit = world
            .spawn((
                Unit,
                TeamComp { team: 0 },
                UnitCore::new(0.0),
                ControllerSlot::new(AiKind::Ground),
            ))
            .id();
        assert!(check_logic_ai(&mut world, 0, false, unit, Some(unit), true).is_some());
        let mut x = LVar::new("x");
        x.set_num(5.0);
        let mut y = LVar::new("y");
        y.set_num(6.0);
        assert!(apply_control(&mut world, unit, LUnitControl::Move, &[x, y]));
        let ai = world
            .get::<crate::ai::types::logic::LogicAi>(unit)
            .expect("logic ai state");
        assert_eq!(ai.control, LUnitControl::Move);
        assert_eq!(ai.target_pos, (5.0, 6.0));
    }

    #[test]
    fn ulocate_finds_closest_ore() {
        let content = crate::content::test_support::test_registry();
        let ore = content.block_id("ore-copper").expect("ore-copper");
        let stone = content.block_id("stone").expect("stone");
        let mut grid = crate::world::WorldGrid::new(8, 8);
        for index in 0..64 {
            let tile = grid.tiles.geti_mut(index);
            tile.floor = stone;
            tile.block = crate::content::BlockId::AIR;
        }
        grid.tiles.geti_mut((2 + 3 * 8) as usize).overlay = ore;
        grid.tiles.geti_mut(6 + 8).overlay = ore;
        let mut world = World::new();
        world.insert_resource(grid);
        world.insert_resource(crate::logic::world::LogicContentIndex::from_content(
            &content,
        ));
        let copper = content.item_id("copper").expect("copper");
        let found = find_closest_ore(&world, 0.0, 0.0, copper).expect("ore tile");
        assert_eq!((found.0, found.1), (2.5, 3.5));
    }

    #[test]
    fn ulocate_finds_flagged_enemy_building() {
        use crate::entities::comp::Building;
        let content = crate::content::test_support::test_registry();
        let core = content.block_id("core-shard").expect("core-shard");
        let mut world = World::new();
        world.insert_resource(crate::logic::world::LogicContentIndex::from_content(
            &content,
        ));
        let enemy = world
            .spawn((
                Building::new(crate::world::TilePos::new(4, 4), core, 0),
                TeamComp { team: 1 },
                Pos { x: 36.0, y: 36.0 },
            ))
            .id();
        let found = find_closest_building(
            &world,
            0.0,
            0.0,
            0,
            true,
            crate::logic::enums::BlockFlag::Core,
        )
        .expect("enemy core");
        assert_eq!(found.2, Some(enemy));
    }

    #[test]
    fn ulocate_finds_damaged_friendly_building() {
        use crate::entities::comp::Building;
        use crate::entities::comp::Health;
        let content = crate::content::test_support::test_registry();
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let mut world = World::new();
        world.insert_resource(crate::logic::world::LogicContentIndex::from_content(
            &content,
        ));
        let damaged = world
            .spawn((
                Building::new(crate::world::TilePos::new(3, 3), wall, 0),
                TeamComp { team: 0 },
                Pos { x: 24.0, y: 24.0 },
                Health {
                    health: 1.0,
                    max_health: 10.0,
                    dead: false,
                },
            ))
            .id();
        let found = find_closest_damaged(&world, 0.0, 0.0, 0).expect("damaged building");
        assert_eq!(found.2, Some(damaged));
    }
}
