// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PointBulletType` behavior (`entities/bullet/PointBulletType.java`).
//!
//! `init` teleports the bullet to its endpoint, hits the closest entity/build
//! there and removes the bullet.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Pos;

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, HIT, hit_building};
use super::{motion, nearest_enemy_building};

/// `PointBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct PointBehavior;

impl BulletBehavior for PointBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let lifetime = ctx.bullet(b).map(|bullet| bullet.lifetime).unwrap_or(0.0);
        let Some(((x, y), (vx, vy), _rotation)) = motion(ctx, b) else {
            return;
        };
        let px = x + lifetime * vx;
        let py = y + lifetime * vy;
        let team = ctx.team(b);
        // Closest enemy unit/building within 1 unit of the endpoint.
        if let Some(target) = nearest_enemy_building(ctx, team, px, py, 1.0) {
            let _ = hit_building(ctx, b, target);
            if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
                bullet.set(HIT);
            }
        }
        if let Some(mut pos) = ctx.world.get_mut::<Pos>(b) {
            pos.x = px;
            pos.y = py;
        }
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.time = lifetime;
            bullet.hit_size = def.hit_size;
            bullet.set(HIT);
        }
    }
}

/// Static point-behavior instance.
pub static POINT: PointBehavior = PointBehavior;
