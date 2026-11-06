// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightningBulletType` behavior (`entities/bullet/LightningBulletType.java`).
//!
//! `init` creates an instant chain lightning and the bullet itself is a stub
//! (never travels). The branch is created through
//! [`crate::combat::lightning::create_from_bullet`], so the lightning bullet's
//! own `collidesAir`/`collidesGround`/`pierceCap` gate the chain exactly like
//! Java's `Lightning.create(b, ...)` (`LightningBulletType.java:40`).

use bevy_ecs::entity::Entity;

use super::super::CombatCtx;
use super::super::behavior::BulletBehavior;
use super::motion;

/// `LightningBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct LightningBehavior;

impl BulletBehavior for LightningBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let (length, damage, color) = {
            let Some(def) = ctx.content.bullet(def_id) else {
                return;
            };
            // `Mathf.random(lightningLengthRand)` uses the unseeded view RNG;
            // the port's deterministic substitution is the maximum length.
            let length = def.lightning_length + def.lightning_length_rand;
            // `LightningBulletType.java:40` passes the BulletType `damage`
            // field (the instance damage is re-derived by
            // `Lightning.create(b, ...)`'s `hitter.damageMultiplier()`).
            (length, def.damage, def.lightning_color)
        };
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        // `Lightning.create(b, ...)`: the branch uses the lightning bullet as
        // the `hitter` (air/ground gate, pierceCap, damage multiplier,
        // `lightningType`).
        crate::combat::lightning::create_from_bullet(ctx, b, color, damage, x, y, rotation, length);
    }

    fn range(&self, _def_range: f32) -> f32 {
        0.0
    }
}

/// Static lightning-behavior instance.
pub static LIGHTNING: LightningBehavior = LightningBehavior;
