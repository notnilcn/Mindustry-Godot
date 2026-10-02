// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `NoAI` (plan 11 §4.2). An inert controller: the unit keeps its component
//! state but issues no movement, targeting or weapon orders.

use bevy_ecs::entity::Entity;

use super::super::ai_controller::AiCtx;

/// `NoAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoAi {
    /// Bound unit.
    pub unit: Option<Entity>,
}

/// `NoAI.updateUnit`: do nothing.
pub fn update_no_ai(_ctx: &mut AiCtx, _unit: Entity) {}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    use super::*;

    #[test]
    fn no_ai_leaves_the_unit_in_place() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("dummy", 0, 64.0, 64.0, 0.0).expect("dummy");
        let before = harness.snapshot(unit).expect("alive");
        {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_no_ai(&mut ctx, unit);
        }
        let after = harness.snapshot(unit).expect("alive");
        assert_eq!(before.x, after.x);
        assert_eq!(before.y, after.y);
    }
}
