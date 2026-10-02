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
use crate::logic::value::LVar;

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
        Some(AiKind::Logic) => Some(unit),
        _ if control => {
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
                slot.kind = AiKind::Logic;
                slot.timer = UNIT_TIMEOUT_KEEP;
                slot.target = None;
                return Some(unit);
            }
            None
        }
        _ => Some(unit),
    }
}

/// Refreshes a logic-controlled unit's timeout (`LogicAI.control`).
pub fn refresh_control_timer(world: &mut World, unit: Entity) {
    if let Some(mut slot) = world.get_mut::<ControllerSlot>(unit) {
        slot.timer = UNIT_TIMEOUT_KEEP;
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

/// `UnitControlI` setter application (the subset plan 11 exposes).
///
/// Full `LogicAI` movement/pathfind/build bodies are plan 11; this applies the
/// state changes the controller slot owns and the identity transforms.
pub fn apply_control(
    world: &mut World,
    unit: Entity,
    control: LUnitControl,
    params: &[LVar],
) -> bool {
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
}
