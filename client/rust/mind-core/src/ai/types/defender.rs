// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DefenderAI` (`ai/types/DefenderAI.java`; H01 T9): oct defenders escort the
//! healthiest player-controllable ally in `max(range, 400)`, falling back to
//! the own core (then the enemy core on wave maps). No weapons pass of its own
//! (only `!controllable && autoTarget` mounts self-target).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, UnitTypeId};
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::UnitTypeComp;
use crate::entities::comp::{Health, Pos};

/// `DefenderAI` controller state (`AIController.target` + the 40/90-tick
/// `timer(timerTarget)` cadence).
#[derive(Debug, Clone, Copy, PartialEq, bevy_ecs::component::Component)]
pub struct DefenderAiState {
    /// `AIController.timer(timerTarget)`: seeded 39 like `GroundAiState` so the
    /// first acquisition runs on the first tick.
    pub retarget: f32,
    /// `AIController.target`.
    pub target: Option<Entity>,
}

impl Default for DefenderAiState {
    fn default() -> Self {
        Self {
            retarget: 39.0,
            target: None,
        }
    }
}

/// `DefenderAI.findTarget` (`DefenderAI.java:26-38`):
///
/// ```java
/// var result = Units.closest(unit.team, x, y, Math.max(range, 400f),
///     u -> !u.dead() && u.type != unit.type && u.targetable(unit.team) && u.playerControllable(),
///     (u, tx, ty) -> -u.maxHealth + Mathf.dst2(u.x, u.y, tx, ty) / 6400f);
/// if(result != null) return result;
/// var core = unit.closestCore(); if(core != null) return core;
/// if(state.rules.waves && unit.team == state.rules.waveTeam) return unit.closestEnemyCore();
/// return null;
/// ```
///
/// The `Units.nearby` distance gate is `u.within(x, y, range + u.hitSize/2)`;
/// iteration is in `(EntitySeq, index)` order so equal-cost ties keep the first
/// spawned ally (Java's quadtree order is not reproducible).
#[allow(clippy::too_many_arguments)]
pub fn defender_find_target(
    world: &mut World,
    content: &ContentRegistry,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
    unit_type: UnitTypeId,
    waves: bool,
    wave_team: u8,
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
            .get::<UnitTypeComp>(other)
            .is_some_and(|comp| comp.type_id == unit_type)
        {
            continue;
        }
        let Some(def) = world
            .get::<UnitTypeComp>(other)
            .and_then(|comp| content.unit(comp.type_id))
        else {
            continue;
        };
        if !def.player_controllable || !def.targetable {
            continue;
        }
        let dead = world.get::<Health>(other).is_some_and(|health| health.dead);
        if dead {
            continue;
        }
        let Some(opos) = world.get::<Pos>(other).copied() else {
            continue;
        };
        let dx = opos.x - x;
        let dy = opos.y - y;
        let dist2 = dx * dx + dy * dy;
        // `Units.nearby`: `u.within(x, y, range + u.hitSize/2)`.
        if dist2 > (radius + def.hit_size / 2.0) * (radius + def.hit_size / 2.0) {
            continue;
        }
        let max_health = world
            .get::<Health>(other)
            .map_or(0.0, |health| health.max_health);
        // `-u.maxHealth + dst2 / 6400`.
        let cost = -max_health + dist2 / 6400.0;
        if best.is_none_or(|(best_cost, _)| cost < best_cost) {
            best = Some((cost, other));
        }
    }
    if let Some((_, target)) = best {
        return Some(target);
    }
    // `unit.closestCore()`.
    if let Some(core) = crate::sim::unit_runtime::closest_core_of_team(world, team, x, y) {
        return Some(core);
    }
    // Enemy core on wave maps for the wave team.
    if waves && team == wave_team {
        return crate::sim::unit_runtime::closest_enemy_core(world, team, x, y);
    }
    None
}
