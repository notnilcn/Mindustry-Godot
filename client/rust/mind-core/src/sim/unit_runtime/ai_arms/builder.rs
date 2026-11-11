// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Builder` dispatch arm: pull ghost plans from the team queue and move toward
//! the active plan.
//! Upstream: core/src/mindustry/ai/types/BuilderAI.java
//! core/src/mindustry/content/UnitTypes.java (core builder fleet range)

use super::super::ai_weapons::command_ai_weapon_pass;
use super::*;

/// `BuilderAI` (T10c): `Sim::update_builders` executes the plan the unit walks
/// to; the weapons pass runs on the `shouldShoot()` predicate.
pub(in crate::sim::unit_runtime) fn builder_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
) {
    // `BuilderAI` (T10c): pull ghost plans from the team queue and
    // move toward the active plan; `Sim::update_builders` executes it.
    let mut state = world
        .get::<crate::ai::types::builder::BuilderAiState>(entity)
        .copied()
        .unwrap_or_else(|| {
            // AI-team core builders construct with
            // `BuilderAI(true, coreFleeRange)` (`UnitTypes.java:2495`,
            // 4352; the def carries the range).
            let core_flee_range = world
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .and_then(|def| match def.controller {
                    crate::content::registries::units::ControllerKind::Builder {
                        core_flee_range,
                    } => Some(core_flee_range),
                    _ => None,
                })
                .unwrap_or(400.0);
            crate::ai::types::builder::BuilderAiState::with_flee(true, core_flee_range)
        });
    {
        let UnitRuntime {
            content,
            grid,
            pathfinder,
            ..
        } = &mut *runtime;
        let mut ctx = crate::ai::ai_controller::AiCtx {
            world,
            grid,
            content,
            pathfinder,
            team,
        };
        crate::ai::types::builder::update_builder_ai(&mut ctx, entity, &mut state);
    }
    if let Some(mut stored) = world.get_mut::<crate::ai::types::builder::BuilderAiState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
    let _ = command_ai_weapon_pass(world, &runtime.content, queries, entity, team, true);
}
