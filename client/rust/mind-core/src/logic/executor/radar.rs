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
    /// `UnitType.targetable(unit, teamer)` (`targetable` default true).
    pub targetable: bool,
    /// `RadarTarget.attacker`: `type.hasWeapons() && !disarmed`.
    pub attacker: bool,
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
        let status = entity_ref.get::<crate::entities::comp::unit::comp::StatusComp>();
        let shield = entity_ref
            .get::<crate::entities::comp::unit::ShieldComp>()
            .map(|s| s.shield)
            .unwrap_or(0.0);
        let (type_armor, has_weapons, targetable) = entity_ref
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
            .and_then(|comp| {
                world
                    .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
                    .and_then(|runtime| {
                        runtime
                            .content
                            .unit(comp.type_id)
                            .map(|def| (def.armor, !def.weapons.is_empty(), def.targetable))
                    })
            })
            .unwrap_or((0.0, false, true));
        let armor = status
            .map(|s| s.armor_override)
            .filter(|v| *v >= 0.0)
            .unwrap_or(type_armor);
        out.push(RadarCandidate {
            entity,
            team: team.team,
            x: pos.x,
            y: pos.y,
            hit_size: hit.map(|h| h.hit_size).unwrap_or(0.0) / 2.0,
            health: health.map(|h| h.health).unwrap_or(0.0),
            max_health: health.map(|h| h.max_health).unwrap_or(0.0),
            shield,
            armor,
            flying: phys.is_some_and(|p| p.flying),
            player,
            dead: core.is_some_and(|c| c.dead),
            targetable,
            attacker: has_weapons && !status.is_some_and(|s| s.disarmed),
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
        // `RadarTarget.attacker`: `u.type.hasWeapons() && !u.disarmed`.
        RadarTarget::Attacker => c.attacker,
        // `RadarTarget.boss`: `u.isBoss()` — the port has no boss flag on
        // `UnitTypeDef` yet, so no candidate matches (documented).
        RadarTarget::Boss => false,
        RadarTarget::Flying => c.flying,
        RadarTarget::Ground => !c.flying,
    }
}

/// `RadarSort.func` value, negated for distance (`-pos.dst2(other)`).
fn sort_key(c: &RadarCandidate, sort: RadarSort, base_x: f32, base_y: f32) -> f32 {
    match sort {
        RadarSort::Distance => -((c.x - base_x).powi(2) + (c.y - base_y).powi(2)),
        RadarSort::Health => c.health,
        RadarSort::Shield => c.shield,
        RadarSort::Armor => c.armor,
        RadarSort::MaxHealth => c.max_health,
    }
}

/// `RadarI.find`: the best candidate matching all three targets.
///
/// Java: `val = sort.func.get(b, u) * sortDir` with `sortDir = order ? 1 : -1`,
/// `best = argmax(val)` (first candidate wins ties), candidates within the
/// source's `range`, `u.targetable(team)` and `b != u`.
#[allow(clippy::too_many_arguments)]
pub fn find(
    candidates: &[RadarCandidate],
    team: u8,
    targets: [RadarTarget; 3],
    sort: RadarSort,
    sort_order: bool,
    base_x: f32,
    base_y: f32,
    range: f32,
    base: Option<Entity>,
) -> Option<Entity> {
    let dir = if sort_order { 1.0 } else { -1.0 };
    let mut best: Option<(Entity, f32)> = None;
    for c in candidates {
        if c.dead || !c.targetable {
            continue;
        }
        if base == Some(c.entity) {
            continue;
        }
        let dx = c.x - base_x;
        let dy = c.y - base_y;
        // `!u.within(b, range)` (`LExecutor.java:815`): `within` is strict.
        if dx * dx + dy * dy >= range * range {
            continue;
        }
        if !targets.iter().all(|t| matches_target(c, team, *t)) {
            continue;
        }
        let val = sort_key(c, sort, base_x, base_y) * dir;
        let replace = match best {
            None => true,
            Some((_, current)) => val > current,
        };
        if replace {
            best = Some((c.entity, val));
        }
    }
    best.map(|(entity, _)| entity)
}

/// `RadarI`'s source gate: `base instanceof Ranged && (privileged ||
/// r.team() == exec.team) && (unit || !building.block.privileged || privileged)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadarSource {
    /// Source entity (building or unit).
    pub entity: Entity,
    /// Whether the source is a unit (`uradar`; needs a `LogicAI`).
    pub unit: bool,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Source team.
    pub team: u8,
    /// `Ranged.range()` (world units).
    pub range: f32,
}

/// Resolves the `RadarI` source and its range, or `None` when a gate fails.
pub fn source(
    world: &World,
    base: &crate::logic::value::LogicObject,
    exec_team: u8,
    privileged: bool,
) -> Option<RadarSource> {
    use crate::logic::value::LogicObject;
    let (entity, is_unit) = match base {
        LogicObject::Building(e) => (*e, false),
        LogicObject::Unit(e) => (*e, true),
        _ => return None,
    };
    let team = world.get::<TeamComp>(entity)?.team;
    if !privileged && team != exec_team {
        return None;
    }
    if !is_unit && crate::logic::blocks::privileged_of(world, entity) && !privileged {
        return None;
    }
    let pos = world.get::<Pos>(entity)?;
    let range = if is_unit {
        // `UnitComp.range()` = `maxRange > 0 ? maxRange : range`.
        world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
            .and_then(|comp| {
                world
                    .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
                    .and_then(|runtime| runtime.content.unit(comp.type_id))
                    .map(|def| {
                        if def.max_range > 0.0 {
                            def.max_range
                        } else {
                            def.range
                        }
                    })
            })
            .unwrap_or(0.0)
    } else if world
        .get::<crate::world::blocks::defense::turrets::TurretState>(entity)
        .is_some()
    {
        // `Turret.range()` includes the loaded ammo's `rangeChange` (I01 T5).
        crate::world::blocks::defense::turrets::turret_range(world, entity)
    } else {
        // Other `Ranged` blocks (processors, cores): the block range.
        crate::logic::blocks::logic_def_of(world, entity)
            .map(|def| def.range)
            .unwrap_or(0.0)
    };
    Some(RadarSource {
        entity,
        unit: is_unit,
        x: pos.x,
        y: pos.y,
        team,
        range,
    })
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
            targetable: true,
            attacker: false,
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
            1000.0,
            None,
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
            1000.0,
            None,
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
            1000.0,
            None,
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
            1000.0,
            None,
        );
        assert_eq!(found, Entity::from_raw_u32(2));
    }
}
