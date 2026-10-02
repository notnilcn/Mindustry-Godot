// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlyingFollowAI` (plan 11 §4.2). Air units escort an allied formation leader;
//! when no leader is in range they fall back to `FlyingAI` flag targeting.
//! Ported from `core/src/mindustry/ai/types/FlyingFollowAI.java`.

use bevy_ecs::entity::Entity;

use crate::entities::comp::unit::comp::PhysicsComp;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::TilePos;

use super::super::ai_controller::AiCtx;
use super::super::controller::ControllerSlot;

/// `FlyingFollowAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlyingFollowAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current follow leader.
    pub follow: Option<Entity>,
}

/// `FlyingFollowAI.updateUnit`: escort the leader, else fall back to the
/// assigned target at `fallback_range`.
pub fn update_flying_follow(ctx: &mut AiCtx, unit: Entity, state: &mut FlyingFollowAi) -> bool {
    let Some(leader) = state.follow.or_else(|| nearest_ally(ctx, unit)) else {
        // Fall back to a direct fly toward the assigned target tile.
        let target = ctx
            .world
            .get::<ControllerSlot>(unit)
            .and_then(|slot| slot.target);
        let Some(target) = target else {
            return false;
        };
        return ctx.pathfind(unit, target, 4.0);
    };
    state.follow = Some(leader);
    let Some(lpos) = ctx.world.get::<Pos>(leader).copied() else {
        state.follow = None;
        return false;
    };
    ctx.move_direct(unit, lpos.x, lpos.y, 16.0)
}

/// Nearest allied flying unit that is not `unit`.
fn nearest_ally(ctx: &mut AiCtx, unit: Entity) -> Option<Entity> {
    let pos = ctx.world.get::<Pos>(unit).copied()?;
    let team = ctx.world.get::<TeamComp>(unit)?.team;
    let candidates =
        crate::entities::comp::unit::queries::in_radius(ctx.world, pos.x, pos.y, 200.0, Some(team));
    let mut best: Option<(f32, Entity)> = None;
    for other in candidates {
        if other == unit {
            continue;
        }
        if !ctx
            .world
            .get::<PhysicsComp>(other)
            .map(|physics| physics.flying)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(opos) = ctx.world.get::<Pos>(other) else {
            continue;
        };
        let dx = opos.x - pos.x;
        let dy = opos.y - pos.y;
        let dist2 = dx * dx + dy * dy;
        match best {
            Some((best_dist, best_entity))
                if best_dist < dist2
                    || (best_dist == dist2 && best_entity.index() <= other.index()) => {}
            _ => best = Some((dist2, other)),
        }
    }
    best.map(|(_, entity)| entity)
}

#[allow(dead_code)]
fn tile_of(pos: &Pos) -> TilePos {
    TilePos::new(
        crate::world::WorldGrid::to_tile(pos.x) as i16,
        crate::world::WorldGrid::to_tile(pos.y) as i16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn follow_moves_toward_leader() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let leader = harness.spawn("flare", 0, 200.0, 200.0, 0.0).expect("flare");
        let follower = harness.spawn("flare", 0, 40.0, 40.0, 0.0).expect("flare");
        let mut state = FlyingFollowAi {
            unit: Some(follower),
            follow: Some(leader),
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_flying_follow(&mut ctx, follower, &mut state);
        let before = harness.snapshot(follower).expect("alive");
        // The follower moved toward the leader's position.
        assert!(before.x > 40.0 || before.y > 40.0);
    }
}
