// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SapBulletType` behavior (`entities/bullet/SapBulletType.java`).
//!
//! `init` line-casts (`Damage.linecast`) to the first enemy target within a
//! random length, heals the shooter by `sapStrength` of the target's current
//! health (capped at the bullet damage), then collides the bullet with the
//! target: units through `BulletComp.collision`, buildings through
//! `tile.collision` + `hit`. The beam itself is view data (G03).

use bevy_ecs::entity::Entity;

use crate::combat::damage::area::heal_health;

use super::super::behavior::BulletBehavior;
use super::super::{
    CombatCtx, HIT, behavior_hit, hit_building, line_hit::line_hit_unit, linecast_enemy,
};
use super::motion;

/// `SapBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct SapBehavior;

impl BulletBehavior for SapBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let length = def.length;
        let length_rand = def.length_rand;
        let sap_strength = def.sap_strength;
        let damage = ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0);
        let owner = ctx.bullet(b).and_then(|bullet| bullet.owner);
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let len = ctx.rng.range(
            crate::determinism::RngStream::Sim,
            length,
            length + length_rand,
        );
        // `SapBulletType.init` (`SapBulletType.java:72-98`).
        let target = linecast_enemy(ctx, b, x, y, rotation, len);
        if let Some(target) = target {
            // `float result = Math.max(Math.min(target.health(), damage), 0);
            //  if(b.owner instanceof Healthc h) h.heal(result * sapStrength);`
            let health = ctx.world.get::<crate::entities::comp::Health>(target);
            let result = health.map_or(0.0, |h| h.health).min(damage).max(0.0);
            if let Some(owner) = owner {
                heal_health(ctx.world, owner, result * sap_strength);
            }
            if ctx
                .world
                .get::<crate::entities::comp::Building>(target)
                .is_some()
            {
                // `if(tile.collide(b)){ tile.collision(b); hit(b, tile.x, tile.y); }`
                hit_building(ctx, b, target);
                if let Some((tx, ty)) = ctx.pos(target) {
                    behavior_hit(ctx, b, tx, ty, true);
                }
            } else {
                // `hit.collision(b, ...); b.collision(hit, ...)` — the unit-side
                // `controller.hit` callback has no port seam (documented).
                if let Some((tx, ty)) = ctx.pos(target) {
                    line_hit_unit(ctx, b, target, tx, ty);
                }
            }
        }
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.fdata = len;
            bullet.set(HIT);
        }
    }
}

/// Static sap-behavior instance.
pub static SAP: SapBehavior = SapBehavior;
