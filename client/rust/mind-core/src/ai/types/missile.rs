// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MissileAI` (plan 11 §4.2). Homing missiles steer at the target, keep facing
//! it, and detonate when the `TimedKill` lifetime expires. Ported from
//! `core/src/mindustry/ai/types/MissileAI.java`.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::TimedKillComp;
use crate::entities::comp::unit::lifecycle::kill_unit;

use super::super::ai_controller::AiCtx;

/// `MissileAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct MissileAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current target.
    pub target: Option<Entity>,
}

/// `MissileAI.updateUnit`: home toward the target and detonate on timeout.
///
/// Returns `false` once the missile has been killed.
pub fn update_missile(ctx: &mut AiCtx, unit: Entity, state: &mut MissileAi) -> bool {
    let expired = ctx
        .world
        .get::<TimedKillComp>(unit)
        .map(|timed| timed.time <= 0.0)
        .unwrap_or(false);
    if expired {
        kill_unit(ctx.world, unit);
        return false;
    }
    let target = state.target.or_else(|| {
        let range = ctx
            .unit_type(unit)
            .and_then(|id| ctx.content.unit(id))
            .map(|def| def.range.max(600.0))
            .unwrap_or(600.0);
        ctx.find_target(unit, range, true, true)
    });
    let Some(target) = target else {
        return true;
    };
    state.target = Some(target);
    if let Some(tpos) = ctx.world.get::<Pos>(target).copied() {
        ctx.face_target(unit, tpos.x, tpos.y);
        ctx.move_direct(unit, tpos.x, tpos.y, 4.0);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn missile_homes_toward_target() {
        let mut harness = UnitHarness::new(64, 64, 1);
        // `missile` is a codegen-only def; attach the marker to a flyer.
        let a = harness.spawn("flare", 0, 40.0, 40.0, 0.0).expect("flare");
        harness.build.world.entity_mut(a).insert(TimedKillComp {
            time: 100.0,
            lifetime: 100.0,
        });
        let b = harness.spawn("dagger", 1, 400.0, 40.0, 0.0).expect("b");
        let mut state = MissileAi {
            unit: Some(a),
            target: Some(b),
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_missile(&mut ctx, a, &mut state);
        let snap = harness.snapshot(a).expect("alive");
        assert!(snap.x > 40.0);
    }
}
