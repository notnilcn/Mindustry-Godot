// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Explosions (`core/src/mindustry/entities/Damage.java`
//! `dynamicExplosion`/`tileDamage`).
//!
//! `dynamicExplosion` combines radial damage with fire/lightning/wave scheduling
//! upstream; the sim core here applies the radial damage and the explosion-shield
//! absorption hook (`ExplosionShield`, plan 10 M7). Fire/lightning/particles are
//! plan 17 + M3.

use bevy_ecs::world::World;

use crate::content::ContentRegistry;

use super::area::{DamageOptions, complete_damage, damage_area};

/// `Damage.dynamicExplosion`: radial damage to enemies of `source_team`.
///
/// `fire`/`flammability`/`explosiveness` drive M3's `Fires`/lightning/waves and
/// are accepted for signature parity; they are inert until those land.
#[allow(clippy::too_many_arguments)]
pub fn dynamic_explosion(
    world: &mut World,
    content: &ContentRegistry,
    source_team: Option<u8>,
    x: f32,
    y: f32,
    radius: f32,
    damage: f32,
    power: f32,
    fire: bool,
) -> f32 {
    let _ = (power, fire);
    damage_area(
        world,
        content,
        source_team,
        x,
        y,
        radius,
        damage,
        DamageOptions::default(),
    )
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
        let dealt = dynamic_explosion(
            &mut harness.build.world,
            &harness.build.content,
            Some(1),
            x,
            y,
            32.0,
            50.0,
            0.0,
            false,
        );
        assert!(dealt > 0.0);
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
