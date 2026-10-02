// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlakBulletType` behavior (`entities/bullet/FlakBulletType.java`).
//!
//! Proximity airburst: once `time >= flakDelay`, a nearby enemy primes the
//! bullet (`fdata` becomes an `explodeDelay` countdown); when the countdown
//! reaches zero the bullet's lifetime is exhausted so it explodes.

use bevy_ecs::entity::Entity;

use super::super::CombatCtx;
use super::super::behavior::BulletBehavior;
use super::nearest_enemy_building;

/// `FlakBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlakBehavior;

impl BulletBehavior for FlakBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let explode_range = def.explode_range;
        let explode_delay = def.explode_delay;
        let flak_delay = def.flak_delay;
        let time = ctx.bullet(b).map(|bullet| bullet.time).unwrap_or(0.0);
        let lifetime = ctx.bullet(b).map(|bullet| bullet.lifetime).unwrap_or(0.0);
        let fdata = ctx.bullet(b).map(|bullet| bullet.fdata).unwrap_or(0.0);
        let Some(pos) = ctx.pos(b) else {
            return;
        };
        let team = ctx.team(b);

        let mut next = fdata;
        if fdata >= 0.0
            && time >= flak_delay
            && nearest_enemy_building(ctx, team, pos.0, pos.1, explode_range).is_some()
        {
            next = explode_delay.max(1.0);
        }
        if next > 0.0 {
            next -= 1.0;
            if next <= 0.0 {
                next = -1.0;
                if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
                    bullet.time = lifetime;
                }
            }
        }
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.fdata = next;
        }
    }
}

/// Static flak-behavior instance.
pub static FLAK: FlakBehavior = FlakBehavior;
