// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LaserBulletType` behavior (`entities/bullet/LaserBulletType.java`).
//!
//! `init(Bullet)` runs `Damage.collideLaser` immediately (an instant line hit),
//! stores the hit length in `fdata` and makes the bullet disappear.

use bevy_ecs::entity::Entity;

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, HIT, collide_laser};
use super::motion;

/// `LaserBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct LaserBehavior;

impl BulletBehavior for LaserBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let length = if def.length > 0.0 {
            def.length
        } else {
            let lifetime = ctx.bullet(b).map(|bullet| bullet.lifetime).unwrap_or(0.0);
            def.speed * lifetime
        };
        let pierce_cap = def.pierce_cap;
        let large = def.large_hit;
        let laser = def.laser_absorb;
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        // `LaserBulletType.init` -> `Damage.collideLaser(b, length, largeHit,
        // laserAbsorb, pierceCap)` (`LaserBulletType.java:68`): the
        // `findPierceLength`/`findLaserLength` pre-pass cuts the beam at the
        // pierce cap / an enemy absorber (plastanium walls); `fdata` = the
        // result length (L06 T3).
        let _ = collide_laser(ctx, b, x, y, rotation, length, large, laser, pierce_cap);
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.set(HIT);
        }
    }

    fn current_length(&self, ctx: &CombatCtx<'_>, b: Entity) -> f32 {
        ctx.bullet(b)
            .and_then(|bullet| ctx.content.bullet(bullet.def))
            .map(|def| def.length)
            .unwrap_or(0.0)
    }
}

/// Static laser-behavior instance.
pub static LASER: LaserBehavior = LaserBehavior;
