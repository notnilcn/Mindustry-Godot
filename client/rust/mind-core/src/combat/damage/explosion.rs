// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Explosions (`core/src/mindustry/entities/Damage.java`
//! `dynamicExplosion`/`tileDamage`).
//!
//! [`dynamic_explosion`] ports the wave-damage half of `Damage.dynamicExplosion`
//! (the `Time.run(i * 2f)` stagger lands immediately in the same order). The
//! lightning half (`power / 700` bolts) is not ported, and the
//! `Bullets.fireball` fire spread is replaced by a single `fires::create` at
//! the explosion tile.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::determinism::SimRng;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::WorldGrid;
use crate::world::blocks::defense::projectors::{BaseShieldState, base_shield_radius};
use crate::world::blocks::defense::shields::ForceProjectorState;
use crate::world::blocks::defense::shields::is_in_regular_polygon;

use super::area::{DamageOptions, complete_damage, damage_area};

/// `Damage.dynamicExplosion` (Damage.java:93-130): radial wave damage from a
/// destroyed block/unit.
///
/// `damage == false` skips all of it (`rules.damageExplosions`).
#[allow(clippy::too_many_arguments)]
pub fn dynamic_explosion(
    world: &mut World,
    content: &ContentRegistry,
    rng: &mut SimRng,
    x: f32,
    y: f32,
    flammability: f32,
    explosiveness: f32,
    power: f32,
    radius: f32,
    damage: bool,
    fire: bool,
    ignore_team: Option<u8>,
) {
    if !damage {
        return;
    }
    // Lightning: `clamp(power / 700, 0, 8)` bolts — not ported (plan 10 M7
    // owns `Lightning.create`).
    let _ = power;
    if fire && flammability > 0.0 {
        // `Call.createBullet(Bullets.fireball, ...)` is not ported; leave the
        // blaze the fireballs would start.
        let _ = crate::combat::fires::create(
            world,
            WorldGrid::to_tile(x) as i16,
            WorldGrid::to_tile(y) as i16,
            rng,
        );
    }
    // `int waves = explosiveness <= 2 ? 0 : clamp((int)(explosiveness / 11), 1, 25)`.
    let waves = if explosiveness <= 2.0 {
        0
    } else {
        ((explosiveness / 11.0) as i32).clamp(1, 25)
    };
    let damage_per_wave = explosiveness / 2.0;
    for i in 0..waves {
        // `if(shields empty || !shields.contains(s -> s instanceof
        // ExplosionShield e && e.absorbExplosion(x, y, damagePerWave)))`
        // (Damage.java:120-127): any absorbing shield skips the whole wave.
        if explosion_absorbed(world, x, y, damage_per_wave, ignore_team) {
            continue;
        }
        // `clamp(radius + explosiveness, 0, 50f) * ((i + 1f) / waves)`.
        let wave_radius =
            (radius + explosiveness).clamp(0.0, 50.0) * ((i as f32 + 1.0) / waves as f32);
        let _ = damage_area(
            world,
            content,
            ignore_team,
            x,
            y,
            wave_radius,
            damage_per_wave,
            DamageOptions::default(),
        );
    }
}

/// `ForceBuild.absorbExplosion(ex, ey, damage)` (ForceProjector.java:270-278)
/// over the live shields: `ForceProjectorState` uses its polygon and buildup
/// (`absorb_explosion`), `BaseShieldState` the simplified latch. Any absorbing
/// enemy shield skips the whole wave.
pub fn explosion_absorbed(
    world: &mut World,
    x: f32,
    y: f32,
    damage: f32,
    ignore_team: Option<u8>,
) -> bool {
    let mut projectors: Vec<Entity> = Vec::new();
    let mut base_shields: Vec<Entity> = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<TeamComp>().map(|team| team.team) == ignore_team {
            continue;
        }
        if entity_ref.get::<ForceProjectorState>().is_some() {
            projectors.push(entity_ref.id());
        } else if entity_ref.get::<BaseShieldState>().is_some() {
            base_shields.push(entity_ref.id());
        }
    }
    let mut absorbed = false;
    for entity in projectors {
        let pos = world.get::<Pos>(entity).copied();
        if let (Some(pos), Some(mut state)) = (pos, world.get_mut::<ForceProjectorState>(entity))
            && state.absorb_explosion(pos.x, pos.y, x, y, damage)
        {
            absorbed = true;
        }
    }
    let mut hit: Vec<Entity> = Vec::new();
    for entity in base_shields {
        let Some(state) = world.get::<BaseShieldState>(entity) else {
            continue;
        };
        if state.broken {
            continue;
        }
        let Some(pos) = world.get::<Pos>(entity) else {
            continue;
        };
        let radius = base_shield_radius(state);
        if radius > 0.0 && is_in_regular_polygon(state.sides, pos.x, pos.y, radius, 0.0, x, y) {
            hit.push(entity);
        }
    }
    for entity in hit {
        if let Some(mut state) = world.get_mut::<BaseShieldState>(entity) {
            state.hit = 1.0;
        }
        absorbed = true;
    }
    absorbed
}

/// `Damage.tileDamage`: square-radius building damage from an explosion.
pub fn tile_damage(
    world: &mut World,
    content: &ContentRegistry,
    x: f32,
    y: f32,
    radius: f32,
    damage: f32,
) -> f32 {
    complete_damage(world, content, x, y, radius, damage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn dynamic_explosion_damages_radius() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(10, 10);
        let mut rng = crate::determinism::SimRng::new(5);
        dynamic_explosion(
            &mut harness.build.world,
            &harness.build.content,
            &mut rng,
            x,
            y,
            0.0,
            40.0,
            0.0,
            32.0,
            true,
            false,
            Some(1),
        );
        assert!(harness.building_health_at(10, 10) < 320.0);
    }

    #[test]
    fn tile_damage_is_square() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(10, 10);
        assert!(
            tile_damage(
                &mut harness.build.world,
                &harness.build.content,
                x,
                y,
                12.0,
                40.0
            ) > 0.0
        );
    }
}
