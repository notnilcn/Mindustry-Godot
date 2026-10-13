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

/// `Units.getCap(team)`: `None` = uncapped (`Integer.MAX_VALUE` upstream).
///
/// Wired against plan 12's `Rules`/`Team`/`TeamData` (plan 11 §3.9):
/// the wave team is uncapped unless PvP, `disableUnitCap`/`ignoreUnitCap` win,
/// otherwise the cap is `unitCap` (plus `TeamData.unitCap` when variable).
pub fn get_cap(
    rules: &crate::game::rules::Rules,
    team: &crate::game::team::Team,
    is_campaign: bool,
    data_unit_cap: i32,
) -> Option<usize> {
    if (team.id == rules.wave_team && !rules.pvp)
        || (is_campaign && team.id == rules.wave_team)
        || rules.disable_unit_cap
        || team.ignore_unit_cap
    {
        return None;
    }
    let cap = if rules.unit_cap_variable {
        rules.unit_cap + data_unit_cap
    } else {
        rules.unit_cap
    };
    Some(cap.max(0) as usize)
}

/// `Units.canCreate(team, type)`: `!useUnitCap || (count < cap && !banned)`.
pub fn can_create(use_unit_cap: bool, type_count: i32, cap: Option<usize>, banned: bool) -> bool {
    !use_unit_cap || (cap.is_none_or(|limit| type_count < limit as i32) && !banned)
}

/// Sum of `BlockDef.unitCapModifier` over the live buildings of `team`
/// (`TeamData.unitCap`, BlockIndexer.java:216,614): Serpulo shard/foundation/
/// nucleus 8/16/24, Erekir 15).
pub fn team_unit_cap_modifier(world: &bevy_ecs::world::World, team: u8) -> i32 {
    use crate::entities::comp::{Building, TeamComp};

    let Some(table) = world.get_resource::<crate::world::BlockTable>() else {
        return 0;
    };
    let mut total = 0;
    for entity_ref in world.iter_entities() {
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
            continue;
        }
        if let Some(def) = table.get(building.block) {
            total += def.def.unit_cap_modifier;
        }
    }
    total
}

/// `Units.getCap(team)` against the live world: `None` = uncapped. Without a
/// `CampaignRuntime` (harness worlds) the cap is not enforced, matching the
/// pre-T12 behaviour (documented deviation).
pub fn unit_cap(world: &bevy_ecs::world::World, team: u8) -> Option<usize> {
    let campaign = world.get_resource::<crate::game::runtime::CampaignRuntime>()?;
    let rules = &campaign.session.rules;
    let data_cap = team_unit_cap_modifier(world, team);
    get_cap(
        rules,
        crate::game::team::Team::get(team),
        campaign.session.is_campaign(),
        data_cap,
    )
}

/// Live units of `type_id` on `team` (`TeamData.countType`; the runtime
/// snapshot's per-type counts are rebuilt lazily, so count the entities).
pub fn live_type_count(
    world: &bevy_ecs::world::World,
    team: u8,
    type_id: crate::content::UnitTypeId,
) -> i32 {
    use crate::entities::comp::unit::comp::UnitTypeComp;
    use crate::entities::comp::{Health, TeamComp};

    let mut count = 0;
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<UnitTypeComp>().map(|comp| comp.type_id) != Some(type_id) {
            continue;
        }
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
            continue;
        }
        if entity_ref.get::<Health>().is_some_and(|health| health.dead) {
            continue;
        }
        count += 1;
    }
    count
}

/// `Units.canCreate(team, type)` against the live world (`Units.java:115-125`).
pub fn can_create_unit(
    world: &bevy_ecs::world::World,
    team: u8,
    def: &crate::content::registries::units::UnitTypeDef,
) -> bool {
    if !def.use_unit_cap {
        return true;
    }
    let cap = unit_cap(world, team);
    let count = live_type_count(world, team, def.id);
    let banned = world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .is_some_and(|campaign| campaign.session.rules.is_banned_unit(&def.name));
    can_create(true, count, cap, banned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::rules::Rules;
    use crate::game::team::{CRUX, NEOPLASTIC, SHARDED, Team};

    #[test]
    fn wave_team_is_uncapped_outside_pvp() {
        let rules = Rules::default();
        assert!(get_cap(&rules, Team::get(CRUX.0), false, 0).is_none());
        let mut pvp = rules.clone();
        pvp.pvp = true;
        assert!(get_cap(&pvp, Team::get(CRUX.0), false, 0).is_some());
    }

    #[test]
    fn disable_and_ignore_flags_uncap() {
        let rules = Rules {
            unit_cap: 10,
            disable_unit_cap: true,
            ..Rules::default()
        };
        assert!(get_cap(&rules, Team::get(SHARDED.0), false, 0).is_none());

        let rules = Rules {
            unit_cap: 10,
            ..Rules::default()
        };
        // neoplastic ignores the cap.
        assert!(get_cap(&rules, Team::get(NEOPLASTIC.0), false, 0).is_none());
    }

    #[test]
    fn variable_cap_adds_team_data_cap() {
        let rules = Rules {
            unit_cap: 10,
            unit_cap_variable: true,
            ..Rules::default()
        };
        let cap = get_cap(&rules, Team::get(SHARDED.0), false, 5);
        assert_eq!(cap, Some(15));
        let fixed = Rules {
            unit_cap: 10,
            unit_cap_variable: false,
            ..Rules::default()
        };
        assert_eq!(get_cap(&fixed, Team::get(SHARDED.0), false, 5), Some(10));
    }

    #[test]
    fn can_create_honours_cap_and_ban() {
        assert!(can_create(false, 999, Some(1), true), "no cap -> allowed");
        assert!(can_create(true, 0, Some(10), false));
        assert!(!can_create(true, 10, Some(10), false), "at cap");
        assert!(!can_create(true, 0, Some(10), true), "banned");
        assert!(can_create(true, 5000, None, false), "uncapped");
    }
}
