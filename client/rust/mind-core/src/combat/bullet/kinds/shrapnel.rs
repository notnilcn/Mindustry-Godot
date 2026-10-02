// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ShrapnelBulletType` behavior (`entities/bullet/ShrapnelBulletType.java`).
//!
//! `init(Bullet)` runs an instant `Damage.collideLine` along the beam and the
//! bullet disappears.

use bevy_ecs::entity::Entity;

use crate::combat::damage::line::collide_line;

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, HIT};
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
        let pierce_armor = def.pierce_armor;
        let armor_multiplier = def.armor_multiplier;
        let damage = ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0);
        let team = ctx.team(b);
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let _ = collide_line(
            ctx.world,
            ctx.content,
            ctx.grid,
            team,
            x,
            y,
            rotation,
            length,
            pierce_cap,
            damage,
            pierce_armor,
            armor_multiplier,
        );
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.fdata = length;
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
