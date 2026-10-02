// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `HugAI` (plan 11 §4.2). Melee units close on the nearest hostile until they
//! touch it. Ported from `core/src/mindustry/ai/types/HugAI.java`: the upstream
//! raycast approach becomes a direct steering (`move_direct`); the melee damage
//! itself is resolved by the plan-10 collision/weapon pass.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Pos;
use crate::entities::comp::unit::HitboxComp;

use super::super::ai_controller::AiCtx;

/// `HugAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct HugAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current locked target.
    pub target: Option<Entity>,
}

/// `HugAI.updateUnit`: pursue the target; return `true` on contact.
pub fn update_hug(ctx: &mut AiCtx, unit: Entity, state: &mut HugAi) -> bool {
    let (range, target_air, target_ground) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| (def.range, def.target_air, def.target_ground))
        .unwrap_or((80.0, false, true));
    let target = state
        .target
        .or_else(|| ctx.find_target(unit, range, target_air, target_ground));
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
    ctx.face_target(unit, tpos.x, tpos.y);
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
    fn hug_closes_on_enemy() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let a = harness.spawn("dagger", 0, 40.0, 40.0, 0.0).expect("a");
        let b = harness.spawn("dagger", 1, 200.0, 40.0, 0.0).expect("b");
        let mut state = HugAi {
            unit: Some(a),
            target: Some(b),
        };
        let mut contacted = false;
        for _ in 0..600 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            if update_hug(&mut ctx, a, &mut state) {
                contacted = true;
                break;
            }
        }
        assert!(contacted, "hug unit reached its target");
    }
}
