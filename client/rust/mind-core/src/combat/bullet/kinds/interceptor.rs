// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `InterceptorBulletType` behavior (`entities/bullet/InterceptorBulletType.java`).
//!
//! Only used by `PointDefenseBulletWeapon`/`PointDefenseTurret`: flies at a
//! target bullet, and on overlap either removes it (if lethal) or subtracts the
//! interceptor's damage.

use bevy_ecs::entity::Entity;

use super::super::behavior::BulletBehavior;
use super::super::{BulletData, CombatCtx, HIT, hit_bullet};
use super::motion;

/// `InterceptorBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct InterceptorBehavior;

impl BulletBehavior for InterceptorBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(data) = ctx.bullet(b).map(|bullet| bullet.data) else {
            return;
        };
        let BulletData::Bullet(other) = data else {
            return;
        };
        if ctx.bullet(other).is_none() {
            if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
                bullet.data = BulletData::None;
            }
            return;
        }
        let Some(((bx, by), _bv, _br)) = motion(ctx, b) else {
            return;
        };
        let Some(((ox, oy), _ov, _or)) = motion(ctx, other) else {
            return;
        };
        let hit_size = ctx.bullet(b).map(|bullet| bullet.hit_size).unwrap_or(0.0);
        let other_size = ctx
            .bullet(other)
            .map(|bullet| bullet.hit_size)
            .unwrap_or(0.0);
        let dx = bx - ox;
        let dy = by - oy;
        let reach = hit_size + other_size;
        if dx * dx + dy * dy > reach * reach {
            return;
        }
        // Snap to the target, run the interceptor hit, and remove self.
        if let Some(mut pos) = ctx.world.get_mut::<crate::entities::comp::Pos>(b) {
            pos.x = ox;
            pos.y = oy;
        }
        hit_bullet(ctx, b, ox, oy, false);
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.set(HIT);
        }
        // Damage the intercepted bullet, removing it when the hit is lethal.
        let interceptor_damage = ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0);
        let other_damage = ctx.bullet(other).map(|bullet| bullet.damage).unwrap_or(0.0);
        if other_damage > interceptor_damage {
            if let Some(mut target) = ctx.world.get_mut::<super::super::Bullet>(other) {
                target.damage = other_damage - interceptor_damage;
            }
        } else if let Some(mut target) = ctx.world.get_mut::<super::super::Bullet>(other) {
            target.set(HIT);
        }
    }
}

/// Static interceptor-behavior instance.
pub static INTERCEPTOR: InterceptorBehavior = InterceptorBehavior;
