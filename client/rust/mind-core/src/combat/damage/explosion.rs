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

use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::determinism::SimRng;
use crate::world::WorldGrid;

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
