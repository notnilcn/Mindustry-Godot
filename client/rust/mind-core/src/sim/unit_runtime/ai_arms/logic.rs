// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Logic` dispatch arm: logic-controlled targeting, movement and the
//! controller reset on death.
//! Upstream: core/src/mindustry/ai/types/LogicAI.java
//! core/src/mindustry/entities/units/AIController.java (updateUnit)

use super::super::ai_weapons::logic_ai_weapon_pass;
use super::super::targeting::unit_control_cost;
use super::*;

/// `LogicAI` (T1): `AIController.updateUnit` runs `updateTargeting()` (weapons)
/// before `updateMovement()`.
pub(in crate::sim::unit_runtime) fn logic_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
) {
    // `LogicAI` (T1): `AIController.updateUnit` runs
    // `updateTargeting()` (weapons) before `updateMovement()`.
    //
    // A bare `Logic` slot without the component (only reachable through
    // a decoded fixture that sets the kind directly; Java cannot reach
    // this state) is left inert.
    if let Some(mut state) = world
        .get::<crate::ai::types::logic::LogicAi>(entity)
        .cloned()
    {
        let can_boost = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id))
            .is_some_and(|def| def.can_boost);
        let should_shoot = state.shoot && !(can_boost && state.boost);
        let explicit = matches!(
            state.aim_control,
            crate::logic::enums::LUnitControl::Target | crate::logic::enums::LUnitControl::Targetp
        );
        let pos_target = (state.aim_control == crate::logic::enums::LUnitControl::Target)
            .then_some(state.pos_target);
        let main_target = if state.aim_control == crate::logic::enums::LUnitControl::Targetp {
            state.main_target
        } else {
            None
        };
        logic_ai_weapon_pass(
            world,
            &runtime.content,
            &queries,
            entity,
            team,
            main_target,
            pos_target,
            should_shoot,
            explicit,
        );
        let alive = {
            // `controlPath.getPathPosition(unit, ...)` reads
            // `unit.type.pathCostId` (ControlPathfinder.java:1137-1138).
            let cost = unit_control_cost(world, &runtime.content, entity);
            runtime.control_path(cost);
            let UnitRuntime {
                content,
                grid,
                pathfinder,
                control_paths,
                ..
            } = &mut *runtime;
            let control_path = &mut control_paths[usize::from(cost.id())];
            let mut ctx = crate::ai::ai_controller::AiCtx {
                world,
                grid,
                content,
                pathfinder,
                team,
            };
            crate::ai::types::logic::update_logic(&mut ctx, entity, &mut state, control_path)
        };
        if alive {
            if let Some(mut stored) = world.get_mut::<crate::ai::types::logic::LogicAi>(entity) {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
        } else {
            // `unit.resetController()` (the unit loop holds the
            // resolved def; `reset_controller`'s `with_content` cannot
            // see the `UnitRuntime` removed by this system).
            let def = world
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id));
            crate::logic::executor::unit_control::reset_controller_def(world, entity, def, team);
        }
    }
}
