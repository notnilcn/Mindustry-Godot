// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PointDefenseBulletWeapon` behavior
//! (`core/src/mindustry/type/weapons/PointDefenseBulletWeapon.java`).
//!
//! Fires an `InterceptorBulletType` bullet whose `data` is the intercepted
//! target bullet. The weighted selection (`dst2 - damage * damageTargetWeight`)
//! matches upstream.

use bevy_ecs::entity::Entity;

use crate::combat::bullet::{BulletData, CombatCtx};

use super::WeaponMount;

/// `Groups.bullet.intersect(...).min(...)` weighted by remaining damage.
pub fn select_target(
    ctx: &CombatCtx<'_>,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
    damage_target_weight: f32,
    candidates: &[Entity],
) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for &candidate in candidates {
        if ctx.team(candidate) == team {
            continue;
        }
        let Some(def_id) = ctx.bullet(candidate).map(|b| b.def) else {
            continue;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            continue;
        };
        // `hittable && !(collidesAir && !collidesTiles)`.
        let eligible = def.hittable && !(def.collides_air && !def.collides_tiles);
        if !eligible {
            continue;
        }
        let Some((bx, by)) = ctx.pos(candidate) else {
            continue;
        };
        let dst2 = (bx - x).powi(2) + (by - y).powi(2);
        if dst2 > range * range {
            continue;
        }
        let damage = ctx.bullet(candidate).map(|b| b.damage).unwrap_or(0.0);
        let weight = dst2 - damage * damage_target_weight;
        if best.is_none_or(|(current, _)| weight < current) {
            best = Some((weight, candidate));
        }
    }
    best.map(|(_, entity)| entity)
}

/// Ports `handleBullet`: store the intercept target in the fired bullet's `data`.
pub fn handle_bullet(ctx: &mut CombatCtx<'_>, mount: &WeaponMount, bullet: Entity) {
    if let Some(target) = mount.target
        && let Ok(mut entity) = ctx.world.get_entity_mut(bullet)
        && let Some(mut state) = entity.get_mut::<crate::combat::bullet::Bullet>()
    {
        state.data = BulletData::Bullet(target);
    }
}
