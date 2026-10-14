// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AssemblerAI` (plan 11 §4.2, dispatched in H01 T13). Assembler drones hover
//! at an arranged spot and face a locked angle; they never pathfind. Ported
//! from `core/src/mindustry/ai/types/AssemblerAI.java`.
//!
//! The block half (`UnitAssemblerBuild.updateTile` arranging `targetPos`/
//! `targetAngle` per drone) is not wired in the port's `AssemblerBehavior` yet:
//! a dispatched drone with the default zero target holds still and keeps its
//! rotation, matching Java before the first arrangement. See the H01 T13
//! summary for the seam request.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::UnitCore;

use super::super::ai_controller::AiCtx;
use super::super::ai_controller::normalize_angle;

/// `AssemblerAI` controller state (`targetPos`/`targetAngle`, persisted).
#[derive(Debug, Default, Clone, Copy, bevy_ecs::component::Component)]
pub struct AssemblerAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// `AssemblerAI.targetPos.x` (arranged by the assembler block).
    pub target_x: f32,
    /// `AssemblerAI.targetPos.y`.
    pub target_y: f32,
    /// Locked facing angle in degrees (`AssemblerAI.targetAngle`).
    pub target_angle: f32,
}

/// `AssemblerAI.updateMovement`: hold at the arranged spot, face the locked
/// angle once within 5 px.
pub fn update_assembler(ctx: &mut AiCtx, unit: Entity, state: &AssemblerAi) {
    // `if(!targetPos.isZero()){ moveTo(targetPos, 1f, 3f); }`
    if state.target_x != 0.0 || state.target_y != 0.0 {
        crate::ai::types::flying::move_to_point(
            ctx.world,
            ctx.grid,
            ctx.content,
            unit,
            state.target_x,
            state.target_y,
            1.0,
            3.0,
            false,
        );
    }

    // `if(unit.within(targetPos, 5f)){ unit.lookAt(targetAngle); }`
    if in_position_at(ctx, unit, state)
        && let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit)
    {
        core.rotation = normalize_angle(state.target_angle);
    }
}

/// `AssemblerAI.inPosition()`: within 10 px of `targetPos` and facing within
/// 15 degrees of `targetAngle` (the assembler's efficiency term).
pub fn in_position(ctx: &AiCtx, unit: Entity, state: &AssemblerAi) -> bool {
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return false;
    };
    let dx = state.target_x - pos.x;
    let dy = state.target_y - pos.y;
    if dx * dx + dy * dy > 100.0 {
        return false;
    }
    let rotation = ctx
        .world
        .get::<UnitCore>(unit)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    // `Angles.within(rotation, targetAngle, 15f)`.
    let delta = (rotation - state.target_angle).rem_euclid(360.0);
    delta <= 15.0 || delta >= 345.0
}

/// `unit.within(targetPos, 5f)`.
fn in_position_at(ctx: &AiCtx, unit: Entity, state: &AssemblerAi) -> bool {
    ctx.world.get::<Pos>(unit).is_some_and(|pos| {
        let dx = state.target_x - pos.x;
        let dy = state.target_y - pos.y;
        dx * dx + dy * dy <= 25.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn assembler_empty_target_holds_rotation() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness
            .spawn("assembly-drone", 0, 64.0, 64.0, 0.0)
            .expect("assembly-drone");
        let state = AssemblerAi {
            unit: Some(unit),
            target_angle: 90.0,
            ..Default::default()
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
        // `if(!targetPos.isZero())` gate: a zero target never turns the drone.
        let core = harness.build.world.get::<UnitCore>(unit).unwrap();
        assert_eq!(core.rotation, 0.0);
        let pos = harness.build.world.get::<Pos>(unit).unwrap();
        assert_eq!((pos.x, pos.y), (64.0, 64.0));
    }

    #[test]
    fn assembler_reaches_target_and_faces_angle() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness
            .spawn("assembly-drone", 0, 64.0, 64.0, 0.0)
            .expect("assembly-drone");
        let state = AssemblerAi {
            unit: Some(unit),
            target_x: 96.0,
            target_y: 96.0,
            target_angle: 90.0,
        };
        let mut arrived = false;
        for _ in 0..900 {
            harness.vel_update(unit);
            {
                let mut ctx = AiCtx {
                    world: &mut harness.build.world,
                    grid: &harness.build.grid,
                    content: &harness.build.content,
                    pathfinder: &mut harness.pathfinder,
                    team: 0,
                };
                update_assembler(&mut ctx, unit, &state);
                if in_position(&ctx, unit, &state) {
                    arrived = true;
                    break;
                }
            }
        }
        assert!(arrived, "the drone reached targetPos and faces targetAngle");
        let core = harness.build.world.get::<UnitCore>(unit).unwrap();
        assert!((core.rotation - 90.0).abs() < 0.001);
        let pos = harness.build.world.get::<Pos>(unit).unwrap();
        let dst = ((pos.x - 96.0).powi(2) + (pos.y - 96.0).powi(2)).sqrt();
        assert!(dst <= 10.0, "within the inPosition radius ({dst})");
    }
}
