// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Continuous (line) bullet behaviors
//! (`entities/bullet/{ContinuousBulletType,ContinuousLaserBulletType,
//! ContinuousFlameBulletType,PointLaserBulletType}.java`).
//!
//! A continuous bullet does not collide as a projectile; instead it damages
//! along a line every `damageInterval` ticks (`Damage.collideLine`). The
//! concrete subclasses differ in draw data and length interpolation, which
//! plans 16/17 read from the def.

use bevy_ecs::entity::Entity;

use super::super::behavior::BulletBehavior;
use super::super::{Bullet, CombatCtx, collide_line_bullet};
use super::motion;

/// `ContinuousBulletType` behavior (shared by all continuous subclasses).
#[derive(Debug, Default, Clone, Copy)]
pub struct ContinuousBehavior;

impl BulletBehavior for ContinuousBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let interval = def.damage_interval;
        let time = ctx.bullet(b).map(|bullet| bullet.time).unwrap_or(0.0);
        if interval <= 0.0 || (time % interval).abs() >= 0.5 {
            return;
        }
        let pierce_cap = def.pierce_cap;
        let large = def.large_hit;
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let length = self.current_length(ctx, b);
        // `ContinuousBulletType.applyDamage` (`ContinuousBulletType.java:82-90`):
        // the pierce falloff mutates `b.damage`, so save and restore it around
        // the line collision.
        let damage = ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0);
        // `Damage.collideLine(..., largeHit, laserAbsorb, pierceCap)`
        // (`ContinuousBulletType.java:88`): `findLength` cuts the beam (L06 T3).
        let laser = def.laser_absorb;
        let _ = collide_line_bullet(ctx, b, x, y, rotation, length, large, laser, pierce_cap);
        if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(b) {
            bullet.damage = damage;
        }
    }

    fn continuous_damage(&self, damage: f32, damage_interval: f32) -> f32 {
        if damage_interval <= 0.0 {
            return -1.0;
        }
        damage / damage_interval * 60.0
    }

    fn current_length(&self, ctx: &CombatCtx<'_>, b: Entity) -> f32 {
        ctx.bullet(b)
            .and_then(|bullet: &Bullet| ctx.content.bullet(bullet.def))
            .map(|def| def.length.max(def.range))
            .unwrap_or(0.0)
    }

    fn range(&self, def_range: f32) -> f32 {
        def_range
    }
}

/// Static continuous-behavior instance.
pub static CONTINUOUS: ContinuousBehavior = ContinuousBehavior;
