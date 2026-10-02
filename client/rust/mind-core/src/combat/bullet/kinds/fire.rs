// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FireBulletType` behavior (`entities/bullet/FireBulletType.java`).
//!
//! Random flame trail; on hit it creates fire (plan 10 M3 owns `Fires`). Until
//! then the bullet is a damage-free piercer (upstream: `collides = false`,
//! `pierce = true`).

use bevy_ecs::entity::Entity;

use super::super::CombatCtx;
use super::super::behavior::BulletBehavior;

/// `FireBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct FireBehavior;

impl BulletBehavior for FireBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        // `FireBulletType.init`: randomize velocity magnitude within
        // `[velMin, velMax]` (defaults `0.6..2.6`).
        let Some(lifetime) = ctx.bullet(b).map(|bullet| bullet.lifetime) else {
            return;
        };
        let _ = lifetime;
        let speed = ctx.rng.range(crate::determinism::RngStream::Sim, 0.6, 2.6);
        let Some(vel) = ctx
            .world
            .get::<crate::entities::comp::Vel>(b)
            .map(|v| (v.x, v.y))
        else {
            return;
        };
        let len = (vel.0 * vel.0 + vel.1 * vel.1).sqrt();
        if len > 0.0
            && let Some(mut vel) = ctx.world.get_mut::<crate::entities::comp::Vel>(b)
        {
            vel.x = vel.x / len * speed;
            vel.y = vel.y / len * speed;
        }
    }
}

/// Static fire-behavior instance.
pub static FIRE: FireBehavior = FireBehavior;
