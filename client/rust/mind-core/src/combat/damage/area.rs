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
use crate::entities::comp::unit::comp::StatusComp;
use crate::entities::comp::unit::{ShieldComp, UnitTypeComp};
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
    /// Whether the damage comes from a bullet (`BuildDamageEvent` emission,
    /// G01 T15).
    pub source_is_bullet: bool,
    /// `Damage.damage(.., air, ground)` (`Damage.java:509-511`): flying units
    /// are hit only when `air`.
    pub air: bool,
    /// Grounded units and every building are hit only when `ground`.
    pub ground: bool,
}

impl Default for DamageOptions {
    fn default() -> Self {
        Self {
            pierce_armor: false,
            armor_multiplier: 1.0,
            scaled: false,
            source_is_bullet: false,
            air: true,
            ground: true,
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

#[allow(clippy::too_many_arguments)]
fn targets_in_radius(
    world: &World,
    team: Option<u8>,
    x: f32,
    y: f32,
    radius: f32,
    scaled: bool,
    air: bool,
    ground: bool,
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
        // `UnitComp.checkTarget(air, ground)` (`UnitComp.java:76-78`):
        // `(isGrounded() && ground) || (isFlying() && air)`, where grounded is
        // `elevation < 0.001` and flying is `elevation >= 0.09`. Buildings are
        // only reached through the `ground` half (`Damage.java:549-556`).
        if entity_ref
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
            .is_some()
        {
            if !(air || ground) {
                continue;
            }
            let elevation = entity_ref
                .get::<crate::entities::comp::unit::comp::UnitCore>()
                .map(|core| core.elevation)
                .unwrap_or(0.0);
            let hit = (elevation >= 0.09 && air) || (elevation < 0.001 && ground);
            if !hit {
                continue;
            }
        } else if !ground {
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

/// Effective armor of an entity: block armor for buildings, unit-type armor
/// for units (`Unit.armor`), with the status armor override winning when set
/// (`UnitComp.armor`: `armorOverride >= 0 ? armorOverride : armor`).
fn armor_of(content: &ContentRegistry, world: &World, entity: Entity) -> f32 {
    if let Some(status) = world.get::<StatusComp>(entity)
        && status.armor_override >= 0.0
    {
        return status.armor_override;
    }
    if let Some(building) = world.get::<Building>(entity) {
        return content
            .block(building.block)
            .map(|def| def.armor)
            .unwrap_or(0.0);
    }
    world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
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
    let targets = targets_in_radius(
        world,
        team,
        x,
        y,
        radius,
        opts.scaled,
        opts.air,
        opts.ground,
    );
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
        // `Damage.damage(.., source)` -> `BuildDamageEvent` for buildings
        // (G01 T15).
        if opts.source_is_bullet
            && let Some(building) = world.get::<crate::entities::comp::Building>(target.entity)
        {
            let team = world
                .get::<crate::entities::comp::TeamComp>(target.entity)
                .map(|team| team.team)
                .unwrap_or(0);
            crate::world::events::push_build_damage(
                world,
                crate::world::events::BuildDamageEvent {
                    x: building.tile.x(),
                    y: building.tile.y(),
                    amount: applied,
                    team,
                },
            );
        }
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

/// Applies a single-target damage instance (armor-aware). Used by line/laser and
/// splash hits where the source is not a bullet entity.
pub fn damage_entity(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    damage: f32,
    pierce_armor: bool,
    armor_multiplier: f32,
) -> f32 {
    let armor = armor_of(content, world, entity) * armor_multiplier;
    let applied = apply_armor_opt(damage, armor, pierce_armor);
    apply_health(world, entity, applied);
    applied
}

/// Subtracts `amount` from an entity's health after shields
/// (`ShieldComp.rawDamage` + `StatusComp.healthMultiplier`).
///
/// Status `healthMultiplier` divides incoming damage (so `invincible`'s
/// infinity negates it), shields absorb first, and the remainder drains
/// health. Armor must already be applied by the caller.
pub fn apply_health(world: &mut World, entity: Entity, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    // `LogicBuild.damage(float)` (`LogicBlock.java:437-444`): a privileged,
    // non-destructible processor ignores every damage instance.
    if crate::world::build::logic_processor_invulnerable(world, entity) {
        return;
    }
    let divisor = world
        .get::<StatusComp>(entity)
        .map(|status| status.health_multiplier)
        .unwrap_or(1.0);
    let mut amount = amount / divisor;
    if !amount.is_finite() || amount <= 0.0 {
        return;
    }
    // `ShieldComp.damage` divides by `rules.unitHealth(team)`; `BuildingComp.
    // damage` divides by `rules.blockHealth(team)` (a zero multiplier turns any
    // hit into a kill, `damage = health + 1`).
    if let Some(rules) = crate::game::runtime::rules_of(world) {
        let team = world.get::<TeamComp>(entity).map_or(0, |t| t.team);
        if world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
            .is_some()
        {
            amount /= rules.unit_health(team);
        } else if world
            .get::<crate::entities::comp::Building>(entity)
            .is_some()
        {
            let dm = rules.block_health(team);
            amount = if dm.abs() <= 1e-6 {
                world
                    .get::<Health>(entity)
                    .map_or(amount, |h| h.health + 1.0)
            } else {
                amount / dm
            };
        }
    }
    if !amount.is_finite() || amount <= 0.0 {
        return;
    }
    if let Some(mut shield) = world.get_mut::<ShieldComp>(entity) {
        // `ShieldComp.rawDamage` (`ShieldComp.java:57-63`): `hadShields =
        // shield > 0.0001f; if(hadShields) shieldAlpha = 1f;` — set on every
        // shielded hit, even when the hit is fully absorbed (H03 T15).
        if shield.shield > 0.0001 {
            shield.shield_alpha = 1.0;
        }
        let absorbed = shield.shield.max(0.0).min(amount);
        shield.shield -= absorbed;
        amount -= absorbed;
    }
    if amount <= 0.0 {
        return;
    }
    let Some(mut health) = world.get_mut::<Health>(entity) else {
        return;
    };
    health.health = (health.health - amount).max(0.0);
    if health.health <= 0.0 {
        health.dead = true;
    }
}

/// Adds `amount` to an entity's health, clamped to max (`HealthComp.heal`).
pub fn heal_health(world: &mut World, entity: Entity, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let Some(mut health) = world.get_mut::<Health>(entity) else {
        return;
    };
    health.health = (health.health + amount).min(health.max_health);
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

    /// I01 T6: `Damage.damage(.., air, ground, ..)` hits a unit only when
    /// `(isFlying() && air) || (isGrounded() && ground)`
    /// (`UnitComp.checkTarget`, `UnitComp.java:76-78`).
    #[test]
    fn ground_only_splash_spares_flyers() {
        use crate::content::BlockId;
        use crate::entities::comp::Health;
        use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};

        let mut sim = crate::sim::Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let ground = sim
            .spawn_live_unit("dagger", 2, 64.0, 64.0, 0.0)
            .expect("dagger");
        let flyer = sim
            .spawn_live_unit("flare", 2, 72.0, 64.0, 0.0)
            .expect("flare");
        let elevation = |sim: &crate::sim::Sim, e| {
            sim.ecs
                .0
                .get::<UnitCore>(e)
                .map_or(0.0, |core| core.elevation)
        };
        assert!(sim.ecs.0.get::<UnitTypeComp>(flyer).is_some());
        assert!(elevation(&sim, flyer) >= 0.09, "flare is flying");
        assert!(elevation(&sim, ground) < 0.001, "dagger is grounded");
        let content = crate::world::harness::BuildHarness::load_content();
        let health =
            |sim: &crate::sim::Sim, e| sim.ecs.0.get::<Health>(e).map_or(0.0, |h| h.health);
        let (ground_hp, flyer_hp) = (health(&sim, ground), health(&sim, flyer));

        // Ground-only splash (e.g. a `collidesAir = false` bullet).
        let applied = damage_area(
            &mut sim.ecs.0,
            &content,
            Some(1),
            64.0,
            64.0,
            48.0,
            100.0,
            DamageOptions {
                air: false,
                ground: true,
                ..DamageOptions::default()
            },
        );
        assert!(applied > 0.0);
        assert!(health(&sim, ground) < ground_hp, "the grounded unit is hit");
        assert_eq!(health(&sim, flyer), flyer_hp, "the flyer is spared");

        // Air-only splash: the flyer takes it, the ground unit does not.
        let applied = damage_area(
            &mut sim.ecs.0,
            &content,
            Some(1),
            72.0,
            64.0,
            48.0,
            100.0,
            DamageOptions {
                air: true,
                ground: false,
                ..DamageOptions::default()
            },
        );
        assert!(applied > 0.0);
        assert!(health(&sim, flyer) < flyer_hp, "the flyer is hit");
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

    /// H03 T15 (`ShieldComp.rawDamage`): a hit on a unit holding shield sets
    /// `shieldAlpha = 1`; a shield-less unit keeps 0.
    #[test]
    fn shield_hit_sets_shield_alpha() {
        use crate::entities::comp::Health;
        use crate::entities::comp::unit::comp::ShieldComp;
        let mut harness = CombatHarness::new(32, 32, 7);
        let shielded = harness
            .build
            .world
            .spawn((
                Health::new(10.0),
                ShieldComp {
                    shield: 5.0,
                    ..Default::default()
                },
            ))
            .id();
        apply_health(&mut harness.build.world, shielded, 3.0);
        let shield = harness
            .build
            .world
            .get::<ShieldComp>(shielded)
            .expect("shield comp");
        assert_eq!(shield.shield_alpha, 1.0);
        assert!(
            (shield.shield - 2.0).abs() < 1e-4,
            "shield {}",
            shield.shield
        );

        let bare = harness
            .build
            .world
            .spawn((Health::new(10.0), ShieldComp::default()))
            .id();
        apply_health(&mut harness.build.world, bare, 3.0);
        assert_eq!(
            harness
                .build
                .world
                .get::<ShieldComp>(bare)
                .expect("shield comp")
                .shield_alpha,
            0.0
        );
    }

    /// H03 T15 (`ShieldComp.update`): `shieldAlpha` decays `1/15` per tick and
    /// clamps at 0.
    #[test]
    fn shield_alpha_decays_in_fifteen_updates() {
        use crate::combat::damage::status::update_unit_status;
        use crate::entities::comp::Health;
        use crate::entities::comp::unit::comp::ShieldComp;
        let mut harness = CombatHarness::new(32, 32, 7);
        let unit = harness
            .build
            .world
            .spawn((
                Health::new(10.0),
                ShieldComp {
                    shield: 5.0,
                    shield_alpha: 1.0,
                },
            ))
            .id();
        for _ in 0..14 {
            update_unit_status(&mut harness.build.world, &harness.build.content, unit);
        }
        let alpha = harness
            .build
            .world
            .get::<ShieldComp>(unit)
            .expect("shield comp")
            .shield_alpha;
        assert!((alpha - 1.0 / 15.0).abs() < 1e-5, "alpha {alpha}");
        update_unit_status(&mut harness.build.world, &harness.build.content, unit);
        let alpha = harness
            .build
            .world
            .get::<ShieldComp>(unit)
            .expect("shield comp")
            .shield_alpha;
        // Java leaves float residue on the exact 15th step (`-= 1/15` fifteen
        // times); the clamp semantics are "fades to ~0".
        assert!(alpha < 1e-6, "alpha {alpha}");
    }
}
