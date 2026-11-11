// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Boost` dispatch arm: enable boosting without issuing movement.
//! Upstream: core/src/mindustry/ai/types/BoostAI.java

use super::*;

/// `BoostAI.defaultBehavior`: turn boost on; the lift-off itself runs in
/// `update_kinematics` (`UnitComp.updateBoosting`).
pub(in crate::sim::unit_runtime) fn boost_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    entity: Entity,
    team: u8,
) {
    // `BoostAI.defaultBehavior` (`ai/types/boost.rs`): enable boost
    // without issuing movement; the lift-off runs in `update_kinematics`
    // (`UnitComp.updateBoosting`).
    let mut state = crate::ai::types::boost::BoostAi {
        unit: Some(entity),
        boosting: true,
    };
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
        crate::ai::types::boost::update_boost(&mut ctx, entity, &mut state);
    }
}
