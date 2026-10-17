// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ShrapnelBulletType` behavior (`entities/bullet/ShrapnelBulletType.java`).
//!
//! `init(Bullet)` runs an instant `Damage.collideLine` along the beam and the
//! bullet disappears.

use bevy_ecs::entity::Entity;

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, HIT, collide_laser};
use super::motion;

/// `ShrapnelBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShrapnelBehavior;

impl BulletBehavior for ShrapnelBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let length = def.length.max(def.range);
        let pierce_cap = def.pierce_cap;
        let large = def.large_hit;
        let laser = def.laser_absorb;
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        // `ShrapnelBulletType.init` -> `Damage.collideLaser(b, length,
        // hitLarge, laserAbsorb, pierceCap)` (`ShrapnelBulletType.java:39`):
        // `fdata` is the pierce/absorber-cut length (L06 T3).
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

/// Static shrapnel-behavior instance.
pub static SHRAPNEL: ShrapnelBehavior = ShrapnelBehavior;
