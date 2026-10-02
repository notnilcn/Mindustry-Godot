// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AssemblerAI` (plan 11 §4.2). Assembler drones hover at a fixed spot and
//! face a locked angle; they never pathfind. Ported from
//! `core/src/mindustry/ai/types/AssemblerAI.java`.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Vel;
use crate::entities::comp::unit::comp::UnitCore;

use super::super::ai_controller::AiCtx;
use super::super::ai_controller::normalize_angle;

/// `AssemblerAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct AssemblerAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Locked facing angle in degrees (`AssemblerAI.targetAngle`).
    pub target_angle: f32,
}

/// `AssemblerAI.updateUnit`: hold position, face the locked angle.
pub fn update_assembler(ctx: &mut AiCtx, unit: Entity, state: &AssemblerAi) {
    if let Some(mut vel) = ctx.world.get_mut::<Vel>(unit) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit) {
        core.rotation = normalize_angle(state.target_angle);
    }
    ctx.stop_shooting(unit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;
    use crate::entities::comp::Pos;

    #[test]
    fn assembler_holds_position_and_faces_angle() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness
            .spawn("assembly-drone", 0, 64.0, 64.0, 0.0)
            .expect("assembly-drone");
        let state = AssemblerAi {
            unit: Some(unit),
            target_angle: 90.0,
        };
        {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_assembler(&mut ctx, unit, &state);
        }
        let core = harness.build.world.get::<UnitCore>(unit).unwrap();
        assert!((core.rotation - 90.0).abs() < 0.001);
        let vel = harness.build.world.get::<Vel>(unit).unwrap();
        assert_eq!((vel.x, vel.y), (0.0, 0.0));
        // Still at spawn position.
        let pos = harness.build.world.get::<Pos>(unit).unwrap();
        assert_eq!((pos.x, pos.y), (64.0, 64.0));
    }
}
