// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlyingFollowAI` (`ai/types/FlyingFollowAI.java`; H01 T9): quell/disrupt
//! wave support flyers follow the healthiest allied unit that is not another
//! follower; non-wave teams fall back to `FlyingAI`
//! (`useFallback()` = `rules.pvp || rules.waveTeam != unit.team`).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ai::controller::{AiKind, ControllerSlot};
use crate::content::UnitTypeId;
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::UnitTypeComp;
use crate::entities::comp::{Health, Pos};

/// `FlyingFollowAI` controller state (`following` + the 30-tick
/// `timer(timerTarget3)` cadence).
#[derive(Debug, Clone, Copy, PartialEq, bevy_ecs::component::Component)]
pub struct FlyingFollowAiState {
    /// `FlyingFollowAI.following`.
    pub following: Option<Entity>,
    /// `AIController.timer(timerTarget3, 30f)` countdown. Seeded 29 so the
    /// first pick runs on the first tick (`GroundAiState` pattern).
    pub follow_timer: f32,
}

impl Default for FlyingFollowAiState {
    fn default() -> Self {
        Self {
            following: None,
            follow_timer: 29.0,
        }
    }
}

/// `FlyingFollowAI.updateMovement`'s follow pick (`FlyingFollowAI.java:26-29`):
///
/// ```java
/// following = Units.closest(unit.team, unit.x, unit.y, Math.max(unit.type.range, 400f),
///     u -> !u.dead() && !(u.controller() instanceof FlyingFollowAI) && u.type != unit.type,
///     (u, tx, ty) -> -u.maxHealth + Mathf.dst2(u.x, u.y, tx, ty) / 6400f);
/// ```
///
/// Deterministic iteration in `(EntitySeq, index)` order; the distance gate is
/// `u.within(x, y, range + u.hitSize/2)`.
pub fn flying_follow_closest(
    world: &mut World,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
    unit_type: UnitTypeId,
) -> Option<Entity> {
    let radius = range.max(400.0);
    let mut candidates: Vec<(u64, Entity)> =
        crate::entities::comp::unit::queries::in_radius(world, x, y, radius, Some(team))
            .into_iter()
            .map(|entity| {
                let seq = world.get::<EntitySeq>(entity).map_or(u64::MAX, |seq| seq.0);
                (seq, entity)
            })
            .collect();
    candidates.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));
    let mut best: Option<(f32, Entity)> = None;
    for (_, other) in candidates {
        if world
            .get::<ControllerSlot>(other)
            .is_some_and(|slot| slot.kind == AiKind::FlyingFollow)
        {
            continue;
        }
        if world
            .get::<UnitTypeComp>(other)
            .is_some_and(|comp| comp.type_id == unit_type)
        {
            continue;
        }
        if world.get::<Health>(other).is_some_and(|health| health.dead) {
            continue;
        }
        let Some(opos) = world.get::<Pos>(other).copied() else {
            continue;
        };
        let dx = opos.x - x;
        let dy = opos.y - y;
        let dist2 = dx * dx + dy * dy;
        let hit = world
            .get::<crate::entities::comp::unit::comp::HitboxComp>(other)
            .map_or(0.0, |hitbox| hitbox.hit_size);
        if dist2 > (radius + hit / 2.0) * (radius + hit / 2.0) {
            continue;
        }
        let max_health = world
            .get::<Health>(other)
            .map_or(0.0, |health| health.max_health);
        let cost = -max_health + dist2 / 6400.0;
        if best.is_none_or(|(best_cost, _)| cost < best_cost) {
            best = Some((cost, other));
        }
    }
    best.map(|(_, entity)| entity)
}
