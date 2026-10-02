// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PointDefenseWeapon` behavior (`core/src/mindustry/type/weapons/PointDefenseWeapon.java`).
//!
//! Targets enemy bullets and destroys/weakens them on fire. The spatial index is
//! plan 05's `Groups.bullet.intersect`; this module exposes the selection and
//! interception logic over an explicitly supplied candidate list.

use bevy_ecs::entity::Entity;

use crate::combat::bullet::{Bullet, CombatCtx};
use crate::content::registries::bullets::BulletDef;

use crate::content::Rgba;
use crate::content::registries::fx_meta::{EffectId, EffectRef};

/// Whether a bullet can be intercepted (`BulletType.hittable`).
pub fn hittable(def: &BulletDef) -> bool {
    def.hittable
}

/// `Groups.bullet.intersect(...).min(b -> b.team != unit.team && hittable, b -> dst2)`.
pub fn select_target(
    ctx: &CombatCtx<'_>,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
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
        if !ctx.content.bullet(def_id).is_some_and(hittable) {
            continue;
        }
        let Some((bx, by)) = ctx.pos(candidate) else {
            continue;
        };
        let dst2 = (bx - x).powi(2) + (by - y).powi(2);
        if dst2 > range * range {
            continue;
        }
        if best.is_none_or(|(current, _)| dst2 < current) {
            best = Some((dst2, candidate));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `PointDefenseWeapon.shoot`: subtract damage from the target bullet, removing
/// it when the incoming damage is lethal. Returns whether the target was removed.
pub fn shoot(
    ctx: &mut CombatCtx<'_>,
    unit: Entity,
    target: Entity,
    damage_multiplier: f32,
    color: Rgba,
) -> bool {
    let Some((x, y)) = ctx.pos(unit) else {
        return false;
    };
    let bullet_damage = ctx.bullet(target).map(|b| b.damage).unwrap_or(0.0) * damage_multiplier;
    let lethal = ctx
        .bullet(target)
        .map(|b| b.damage <= bullet_damage)
        .unwrap_or(false);
    ctx.fx
        .effect(&EffectRef::Named(EffectId::POINT_BEAM), x, y, 0.0, color);
    if lethal {
        let _ = ctx.world.despawn(target);
    } else if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(target) {
        bullet.damage -= bullet_damage;
    }
    lethal
}
