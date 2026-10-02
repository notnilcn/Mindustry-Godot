// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Area/splash damage (`core/src/mindustry/entities/Damage.java`).
//!
//! Ports `Damage.damage(...)`, `Damage.completeDamage(...)` and the unit/building
//! falloff rules. Target enumeration is insertion-stable: entities are visited in
//! `EntitySeq` order, so Rust↔Rust replays apply damage deterministically (HLP §9;
//! plan 10 R-10-6).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::ecs::EntitySeq;
use crate::entities::comp::{Building, Health, Pos, TeamComp};

use super::armor::apply_armor_opt;

/// Damage application options (`Damage.damage` parameter bundle).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageOptions {
    /// Whether armor is ignored (`BulletType.pierceArmor`).
    pub pierce_armor: bool,
    /// Multiplier applied to the target's armor (`armorMultiplier`).
    pub armor_multiplier: f32,
    /// Whether distance is measured from the target's hitbox edge (`scaled`).
    pub scaled: bool,
}

impl Default for DamageOptions {
    fn default() -> Self {
        Self {
            pierce_armor: false,
            armor_multiplier: 1.0,
            scaled: false,
        }
    }
}

/// One resolved target (entity + squared distance) in encounter order.
#[derive(Debug, Clone, Copy)]
struct Target {
    entity: Entity,
    seq: u64,
    dst: f32,
}

fn targets_in_radius(
    world: &World,
    team: Option<u8>,
    x: f32,
    y: f32,
    radius: f32,
    scaled: bool,
) -> Vec<Target> {
    let radius2 = radius * radius;
    let mut targets: Vec<Target> = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        if entity_ref.get::<Health>().is_none() {
            continue;
        }
        // `source_team` is the attacker's team: its allies are not damaged
        // (`Damage.damage(team, ...)` hits enemies only).
        if let Some(source) = team
            && entity_ref
                .get::<TeamComp>()
                .is_some_and(|t| t.team == source)
        {
            continue;
        }
        let dx = pos.x - x;
        let dy = pos.y - y;
        let dst2 = dx * dx + dy * dy;
        // `scaled` shrinks the effective distance by the target's hitbox radius.
        let hit_size = entity_ref.get::<Building>().map(|_| 4.0_f32).unwrap_or(0.0);
        let effective = if scaled {
            let dst = dst2.sqrt() - hit_size / 2.0;
            dst.max(0.0)
        } else {
            dst2.sqrt()
        };
        if effective * effective > radius2 {
            continue;
        }
        let seq = entity_ref
            .get::<EntitySeq>()
            .map(|s| s.0)
            .unwrap_or(u64::MAX);
        targets.push(Target {
            entity: entity_ref.id(),
            seq,
            dst: effective,
        });
    }
    targets.sort_by_key(|t| (t.seq, t.entity.index()));
    targets
}

fn armor_of(content: &ContentRegistry, world: &World, entity: Entity) -> f32 {
    let Some(building) = world.get::<Building>(entity) else {
        return 0.0;
    };
    content
        .block(building.block)
        .map(|def| def.armor)
        .unwrap_or(0.0)
}

/// Applies `damage` to every damageable entity within `radius` of `(x, y)`.
///
/// Returns the total damage actually applied (after armor). Units and buildings
/// share the `Health` component, so both are hit (plan 11 supplies unit armor).
#[allow(clippy::too_many_arguments)]
pub fn damage_area(
    world: &mut World,
    content: &ContentRegistry,
    team: Option<u8>,
    x: f32,
    y: f32,
    radius: f32,
    damage: f32,
    opts: DamageOptions,
) -> f32 {
    if radius <= 0.0 || damage <= 0.0 {
        return 0.0;
    }
    let targets = targets_in_radius(world, team, x, y, radius, opts.scaled);
    let mut total = 0.0f32;
    for target in targets {
        // Unit falloff `lerp(1 - dist/radius, 1, 0.4)` (Damage.damage).
        let falloff = {
            let base = (1.0 - target.dst / radius).clamp(0.0, 1.0);
            base + (1.0 - base) * 0.4
        };
        let raw = damage * falloff;
        let armor = armor_of(content, world, target.entity) * opts.armor_multiplier;
        let applied = apply_armor_opt(raw, armor, opts.pierce_armor);
        apply_health(world, target.entity, applied);
        total += applied;
    }
    total
}

/// Square-radius building damage (`Damage.completeDamage`).
pub fn complete_damage(
    world: &mut World,
    content: &ContentRegistry,
    x: f32,
    y: f32,
    radius: f32,
    damage: f32,
) -> f32 {
    if radius <= 0.0 || damage <= 0.0 {
        return 0.0;
    }
    let mut targets: Vec<(u64, Entity)> = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        if entity_ref.get::<Health>().is_none() {
            continue;
        }
        let (bx, by) = crate::world::BuildHarness::tile_center(
            building.tile.x() as i32,
            building.tile.y() as i32,
        );
        if (bx - x).abs() <= radius && (by - y).abs() <= radius {
            let seq = entity_ref
                .get::<EntitySeq>()
                .map(|s| s.0)
                .unwrap_or(u64::MAX);
            targets.push((seq, entity_ref.id()));
        }
    }
    targets.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    let mut total = 0.0f32;
    for (_, entity) in targets {
        let armor = armor_of(content, world, entity);
        let applied = apply_armor_opt(damage, armor, false);
        apply_health(world, entity, applied);
        total += applied;
    }
    total
}

/// Subtracts `amount` from an entity's health, clamping at zero (`damage()` core).
pub fn apply_health(world: &mut World, entity: Entity, amount: f32) {
    let Some(mut health) = world.get_mut::<Health>(entity) else {
        return;
    };
    health.health = (health.health - amount).max(0.0);
    if health.health <= 0.0 {
        health.dead = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn area_falloff_and_armor() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        assert!(harness.place(12, 10, wall, 0, true));
        let center = CombatHarness::tile_center(10, 10);
        let applied = harness.damage_buildings(center.0, center.1, 40.0, 50.0);
        assert!(applied > 0.0, "at least one building damaged");
        // A building outside the radius is untouched.
        let far = harness.damage_buildings(center.0 + 200.0, center.1, 8.0, 50.0);
        assert_eq!(far, 0.0);
    }

    #[test]
    fn complete_damage_is_square() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let center = CombatHarness::tile_center(10, 10);
        let dealt = complete_damage(
            &mut harness.build.world,
            &harness.build.content,
            center.0,
            center.1,
            12.0,
            30.0,
        );
        assert!(dealt > 0.0);
    }
}
