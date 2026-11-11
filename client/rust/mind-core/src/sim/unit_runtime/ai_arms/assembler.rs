// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Assembler` dispatch arm: hold at the arranged spot and face the locked
//! angle.
//! Upstream: core/src/mindustry/ai/types/AssemblerAI.java

use super::*;

/// `AssemblerAI` (`AssemblerAI.java:11-24`, H01 T13).
pub(in crate::sim::unit_runtime) fn assembler_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    entity: Entity,
    team: u8,
) {
    // `AssemblerAI` (`AssemblerAI.java:11-24`, H01 T13): hold at the
    // arranged spot and face the locked angle.
    let state = world
        .get::<crate::ai::types::assembler::AssemblerAi>(entity)
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
        crate::ai::types::assembler::update_assembler(&mut ctx, entity, &state);
    }
    if let Some(mut stored) = world.get_mut::<crate::ai::types::assembler::AssemblerAi>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}
