// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BoostAI` (plan 11 §4.2). The RTS boost controller is a `CommandAI` variant
//! that toggles `unit.isBoosting` from the `boost` stance. Ported from
//! `core/src/mindustry/ai/types/BoostAI.java` (`defaultBehavior` = enable boost).

use bevy_ecs::entity::Entity;

use crate::entities::comp::unit::comp::UnitCore;

use super::super::ai_controller::AiCtx;

/// `BoostAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct BoostAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Whether the boost stance is active.
    pub boosting: bool,
}

/// `BoostAI.defaultBehavior`: enable/disable boost without issuing movement.
pub fn update_boost(ctx: &mut AiCtx, unit: Entity, state: &mut BoostAi) {
    if let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit) {
        core.boosting = state.boosting;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn boost_toggles_unit_flag() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let mut state = BoostAi {
            unit: Some(unit),
            boosting: true,
        };
        {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_boost(&mut ctx, unit, &mut state);
        }
        assert!(harness.build.world.get::<UnitCore>(unit).unwrap().boosting);
        state.boosting = false;
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_boost(&mut ctx, unit, &mut state);
        assert!(!harness.build.world.get::<UnitCore>(unit).unwrap().boosting);
    }
}
