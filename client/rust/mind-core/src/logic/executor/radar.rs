// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RadarI` target selection (plan 13 M5).
//!
//! Ported from `core/src/mindustry/logic/LExecutor.java` (`RadarI`) plus the
//! `RadarTarget`/`RadarSort` filters. Upstream keeps the last outcome on the
//! instruction instance and only refreshes every 30 ticks for buildings / on the
//! unit's target timer for `uradar`; this port evaluates the candidate set each
//! run (deterministic, no wall clock) which is behavior-identical for a single
//! query and cheaper than the cache. Candidate iteration is by entity slot
//! order so results are stable.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::{HitboxComp, PhysicsComp, UnitCore};
use crate::entities::comp::{Health, Pos, TeamComp, Unit};
use crate::logic::enums::{RadarSort, RadarTarget};

/// One radar-relevant unit snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadarCandidate {
    /// Entity handle.
    pub entity: Entity,
    /// Team id.
    pub team: u8,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Half hitbox (`hitSize / 2`).
    pub hit_size: f32,
    /// Current health.
    pub health: f32,
    /// Max health.
    pub max_health: f32,
    /// Shield amount.
    pub shield: f32,
    /// Armor.
    pub armor: f32,
    /// Whether the unit flies.
    pub flying: bool,
    /// Whether the unit is controlled by a player.
    pub player: bool,
    /// Whether the unit is dead.
    pub dead: bool,
}

impl RadarCandidate {
    /// Distance to a world-space point.
    pub fn distance_to(&self, x: f32, y: f32) -> f32 {
        ((self.x - x).powi(2) + (self.y - y).powi(2)).sqrt()
    }
}

/// Collects radar candidates from the world in stable entity-slot order.
pub fn candidates(world: &World) -> Vec<RadarCandidate> {
    let mut out: Vec<RadarCandidate> = Vec::new();
    for entity_ref in world.iter_entities() {
        let entity = entity_ref.id();
        if entity_ref.get::<Unit>().is_none() {
            continue;
        }
        let (Some(pos), Some(team)) = (entity_ref.get::<Pos>(), entity_ref.get::<TeamComp>())
        else {
            continue;
        };
        let health = entity_ref.get::<Health>().copied();
        let phys = entity_ref.get::<PhysicsComp>();
        let hit = entity_ref.get::<HitboxComp>();
        let core = entity_ref.get::<UnitCore>();
        let player = entity_ref.get::<crate::entities::comp::Player>().is_some();
        out.push(RadarCandidate {
            entity,
            team: team.team,
            x: pos.x,
            y: pos.y,
            hit_size: hit.map(|h| h.hit_size).unwrap_or(0.0) / 2.0,
            health: health.map(|h| h.health).unwrap_or(0.0),
            max_health: health.map(|h| h.max_health).unwrap_or(0.0),
            shield: 0.0,
            armor: 0.0,
            flying: phys.is_some_and(|p| p.flying),
            player,
            dead: core.is_some_and(|c| c.dead),
        });
    }
    out.sort_by_key(|c| c.entity.index());
    out
}

/// `RadarTarget` predicate against a base team.
pub fn matches_target(c: &RadarCandidate, team: u8, target: RadarTarget) -> bool {
    let derelict = crate::game::team::DERELICT.0;
    match target {
        RadarTarget::Any => true,
        RadarTarget::Enemy => c.team != team && c.team != derelict,
        RadarTarget::Ally => c.team == team,
        RadarTarget::Player => c.player,
        // Attacker/boss need combat/type data owned by plans 10/11.
        RadarTarget::Attacker | RadarTarget::Boss => false,
        RadarTarget::Flying => c.flying,
        RadarTarget::Ground => !c.flying,
    }
}

fn sort_key(c: &RadarCandidate, sort: RadarSort, base_x: f32, base_y: f32) -> f32 {
    match sort {
        RadarSort::Distance => c.distance_to(base_x, base_y),
        RadarSort::Health => c.health,
        RadarSort::Shield => c.shield,
        RadarSort::Armor => c.armor,
        RadarSort::MaxHealth => c.max_health,
    }
}

/// `RadarI.find`: returns the best candidate matching all three targets.
///
/// `targets` filters are applied in order (`target1`, `target2`, `target3`);
/// `sort_order` is the upstream boolean (`true` = ascending / nearest first).
pub fn find(
    candidates: &[RadarCandidate],
    team: u8,
    targets: [RadarTarget; 3],
    sort: RadarSort,
    sort_order: bool,
    base_x: f32,
    base_y: f32,
) -> Option<Entity> {
    let mut best: Option<(Entity, f32)> = None;
    for c in candidates {
        if c.dead {
            continue;
        }
        if !targets.iter().all(|t| matches_target(c, team, *t)) {
            continue;
        }
        let key = sort_key(c, sort, base_x, base_y);
        let replace = match best {
            None => true,
            Some((_, current)) => {
                if sort_order {
                    key < current
                } else {
                    key > current
                }
            }
        };
        if replace {
            best = Some((c.entity, key));
        }
    }
    best.map(|(entity, _)| entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(entity: u32, team: u8, x: f32, health: f32, flying: bool) -> RadarCandidate {
        RadarCandidate {
            entity: Entity::from_raw_u32(entity).expect("entity"),
            team,
            x,
            y: 0.0,
            hit_size: 1.0,
            health,
            max_health: health,
            shield: 0.0,
            armor: 0.0,
            flying,
            player: false,
            dead: false,
        }
    }

    #[test]
    fn enemy_filter_excludes_derelict_and_self() {
        let derelict = crate::game::team::DERELICT.0;
        let list = vec![
            candidate(1, 0, 0.0, 10.0, false),
            candidate(2, derelict, 0.0, 10.0, false),
            candidate(3, 1, 0.0, 10.0, false),
        ];
        let found = find(
            &list,
            0,
            [RadarTarget::Enemy, RadarTarget::Any, RadarTarget::Any],
            RadarSort::Distance,
            true,
            0.0,
            0.0,
        );
        assert_eq!(found, Entity::from_raw_u32(3));
    }

    #[test]
    fn sort_order_picks_nearest_or_farthest() {
        let list = vec![
            candidate(1, 1, 10.0, 5.0, false),
            candidate(2, 1, 30.0, 5.0, false),
            candidate(3, 1, 20.0, 5.0, false),
        ];
        let nearest = find(
            &list,
            0,
            [RadarTarget::Enemy, RadarTarget::Any, RadarTarget::Any],
            RadarSort::Distance,
            true,
            0.0,
            0.0,
        );
        assert_eq!(nearest, Entity::from_raw_u32(1));
        let farthest = find(
            &list,
            0,
            [RadarTarget::Enemy, RadarTarget::Any, RadarTarget::Any],
            RadarSort::Distance,
            false,
            0.0,
            0.0,
        );
        assert_eq!(farthest, Entity::from_raw_u32(2));
    }

    #[test]
    fn ground_filter_excludes_flying() {
        let list = vec![
            candidate(1, 1, 0.0, 5.0, true),
            candidate(2, 1, 0.0, 5.0, false),
        ];
        let found = find(
            &list,
            0,
            [RadarTarget::Enemy, RadarTarget::Ground, RadarTarget::Any],
            RadarSort::Health,
            true,
            0.0,
            0.0,
        );
        assert_eq!(found, Entity::from_raw_u32(2));
    }
}
