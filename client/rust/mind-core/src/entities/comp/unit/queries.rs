// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit queries (`entities/Units.java` subset; plan 11 §4.1). M0 ships the
//! deterministic set helpers used by controllers and tests; `best`/target
//! sorting lands with M2.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::comp::{HitboxComp, UnitCore};
use crate::entities::comp::{Health, Pos, TeamComp, Unit};

/// All units in stable entity order.
pub fn all(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, bevy_ecs::query::With<Unit>>();
    query.iter(world).collect()
}

/// Number of live units, optionally filtered by team.
pub fn count(world: &mut World, team: Option<u8>) -> usize {
    let mut query = world.query_filtered::<(Entity, &TeamComp), bevy_ecs::query::With<Unit>>();
    query
        .iter(world)
        .filter(|(_, t)| team.is_none_or(|want| t.team == want))
        .count()
}

/// The unit nearest to `(x, y)` within `radius` (world pixels), if any.
///
/// Ties are broken by entity index so the choice is deterministic.
pub fn closest(world: &mut World, x: f32, y: f32, radius: f32, team: Option<u8>) -> Option<Entity> {
    let radius2 = radius * radius;
    let mut query =
        world.query_filtered::<(Entity, &Pos, &TeamComp), bevy_ecs::query::With<Unit>>();
    let mut best: Option<(f32, Entity)> = None;
    for (entity, pos, t) in query.iter(world) {
        if let Some(want) = team
            && t.team != want
        {
            continue;
        }
        let dx = pos.x - x;
        let dy = pos.y - y;
        let dist2 = dx * dx + dy * dy;
        if dist2 > radius2 {
            continue;
        }
        match best {
            Some((best_dist, best_entity))
                if best_dist < dist2
                    || (best_dist == dist2 && best_entity.index() <= entity.index()) => {}
            _ => best = Some((dist2, entity)),
        }
    }
    best.map(|(_, entity)| entity)
}

/// Units within `radius` of `(x, y)`, in stable entity order.
pub fn in_radius(world: &mut World, x: f32, y: f32, radius: f32, team: Option<u8>) -> Vec<Entity> {
    let radius2 = radius * radius;
    let mut query =
        world.query_filtered::<(Entity, &Pos, &TeamComp), bevy_ecs::query::With<Unit>>();
    let mut out: Vec<Entity> = query
        .iter(world)
        .filter(|(_, pos, t)| {
            let dx = pos.x - x;
            let dy = pos.y - y;
            dx * dx + dy * dy <= radius2 && team.is_none_or(|want| t.team == want)
        })
        .map(|(entity, _, _)| entity)
        .collect();
    out.sort_by_key(|entity| entity.index());
    out
}

/// A read-only snapshot of one unit (tests/dumps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitSnapshot {
    /// Entity handle.
    pub entity: Entity,
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
    /// Facing angle in degrees.
    pub rotation: f32,
    /// Current health.
    pub health: f32,
    /// Owning team.
    pub team: u8,
    /// Entity sequence (stable ordering key).
    pub seq: u64,
    /// Hitbox side.
    pub hit_size: f32,
}

/// Reads `entity` as a unit snapshot, if it is one.
pub fn snapshot(world: &World, entity: Entity) -> Option<UnitSnapshot> {
    let pos = world.get::<Pos>(entity)?;
    let core = world.get::<UnitCore>(entity)?;
    let health = world.get::<Health>(entity)?;
    let team = world.get::<TeamComp>(entity)?;
    let hitbox = world.get::<HitboxComp>(entity)?;
    let seq = world.get::<crate::ecs::EntitySeq>(entity).map(|s| s.0)?;
    Some(UnitSnapshot {
        entity,
        x: pos.x,
        y: pos.y,
        rotation: core.rotation,
        health: health.health,
        team: team.team,
        seq,
        hit_size: hitbox.hit_size,
    })
}

/// The lowest-cost unit within `radius` (`Units.best`), scored by `score`.
///
/// Candidates are collected first so `score` can read the world; ties break by
/// entity index for determinism. `score` is the port of upstream's `Floatf`
/// target-priority function.
pub fn best(
    world: &mut World,
    x: f32,
    y: f32,
    radius: f32,
    team: Option<u8>,
    score: impl Fn(&World, Entity) -> f32,
) -> Option<Entity> {
    let radius2 = radius * radius;
    let candidates: Vec<Entity> = {
        let mut query =
            world.query_filtered::<(Entity, &Pos, &TeamComp), bevy_ecs::query::With<Unit>>();
        query
            .iter(world)
            .filter(|(_, pos, t)| {
                let dx = pos.x - x;
                let dy = pos.y - y;
                dx * dx + dy * dy <= radius2 && team.is_none_or(|want| t.team == want)
            })
            .map(|(entity, _, _)| entity)
            .collect()
    };
    let mut best: Option<(f32, Entity)> = None;
    for entity in candidates {
        let value = score(world, entity);
        match best {
            Some((best_score, best_entity))
                if value > best_score
                    || (value == best_score && best_entity.index() <= entity.index()) => {}
            _ => best = Some((value, entity)),
        }
    }
    best.map(|(_, entity)| entity)
}

/// `Units.getCap(team)`: `None` = uncapped.
///
/// TODO(plan 12): read `Rules.disableUnitCap`, `team.ignoreUnitCap`,
/// `unitCapVariable`/`TeamData.unitCap` and the campaign/PvP flags. Until plan
/// 12 owns `Rules`/`Team`, the default is uncapped.
pub const fn get_cap(_team: u8) -> Option<usize> {
    None
}

/// `Units.canCreate(team, type)`: cap check + ban flag (plan 11 §3.4).
pub fn can_create(world: &mut World, team: u8, cap: Option<usize>, banned: bool) -> bool {
    !banned && cap.is_none_or(|limit| count(world, Some(team)) < limit)
}
