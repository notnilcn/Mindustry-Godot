// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SuicideAI` (plan 11 §4.2). Rams the nearest hostile in a straight line,
//! ignoring terrain. Ported from `core/src/mindustry/ai/types/SuicideAI.java`.
//! The `suicide` stance forces an immediate launch even without a target.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Pos;
use crate::entities::comp::unit::HitboxComp;

use super::super::ai_controller::AiCtx;

/// `SuicideAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct SuicideAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current locked target.
    pub target: Option<Entity>,
    /// Force-launch flag (`suicide` stance).
    pub suicidal: bool,
}

/// `SuicideAI.updateUnit`: ram the target; return `true` on contact.
pub fn update_suicide(ctx: &mut AiCtx, unit: Entity, state: &mut SuicideAi) -> bool {
    let (range, target_air, target_ground) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| (def.range.max(400.0), def.target_air, def.target_ground))
        .unwrap_or((400.0, true, true));
    let target = state.target.or_else(|| {
        if state.suicidal {
            ctx.find_target(unit, range, true, true)
        } else {
            ctx.find_target(unit, range, target_air, target_ground)
        }
    });
    let Some(target) = target else {
        state.target = None;
        ctx.stop_shooting(unit);
        return false;
    };
    state.target = Some(target);
    let Some(tpos) = ctx.world.get::<Pos>(target).copied() else {
        state.target = None;
        return false;
    };
    let contact = ctx
        .world
        .get::<HitboxComp>(target)
        .map(|hitbox| hitbox.hit_size.max(4.0))
        .unwrap_or(4.0);
    let arrived = ctx.move_direct(unit, tpos.x, tpos.y, contact);
    if arrived {
        state.target = None;
    }
    arrived
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn suicide_ignores_walls() {
        let mut harness = UnitHarness::new(64, 16, 1);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        for y in 0..16 {
            assert!(harness.build.place(32, y, wall, 0, true));
        }
        let a = harness
            .spawn("crawler", 0, 40.0, 40.0, 0.0)
            .expect("crawler");
        let b = harness.spawn("dagger", 1, 400.0, 40.0, 0.0).expect("b");
        let mut state = SuicideAi {
            unit: Some(a),
            target: Some(b),
            suicidal: true,
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_suicide(&mut ctx, a, &mut state);
        let snap = harness.snapshot(a).expect("alive");
        assert!(snap.x > 40.0, "unit advanced through the wall line");
    }
}
