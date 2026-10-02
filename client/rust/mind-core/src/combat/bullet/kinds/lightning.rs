// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightningBulletType` behavior (`entities/bullet/LightningBulletType.java`).
//!
//! `init` creates an instant chain lightning and the bullet itself is a stub
//! (never travels). Plan 10 M3 owns `combat::lightning`; until it lands this is
//! inert.

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
            let length = def.lightning_length + def.lightning_length_rand;
            let damage = if def.lightning_damage >= 0.0 {
                def.lightning_damage
            } else {
                ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0)
            };
            (length, damage, def.lightning_color)
        };
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let team = ctx.team(b);
        crate::combat::lightning::create(ctx, team, color, damage, x, y, rotation, length.max(0));
    }

    fn range(&self, _def_range: f32) -> f32 {
        0.0
    }
}

/// Static lightning-behavior instance.
pub static LIGHTNING: LightningBehavior = LightningBehavior;
