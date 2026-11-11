// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Cargo` dispatch arm: the tether unit shuttles items between its loader and
//! a configured unload point.
//! Upstream: core/src/mindustry/ai/types/CargoAI.java

use super::*;

/// `CargoAI` (`CargoAI.java:26-97`, H01 T13).
pub(in crate::sim::unit_runtime) fn cargo_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    entity: Entity,
    team: u8,
) {
    // `CargoAI` (`CargoAI.java:26-97`, H01 T13): the tether unit
    // shuttles items between its loader and a configured unload point.
    let mut state = world
        .get::<crate::ai::types::cargo::CargoAi>(entity)
        .copied()
        .unwrap_or_default();
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
        let _ = crate::ai::types::cargo::update_cargo(&mut ctx, entity, &mut state);
    }
    if let Some(mut stored) = world.get_mut::<crate::ai::types::cargo::CargoAi>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}
