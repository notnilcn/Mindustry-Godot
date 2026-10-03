// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LiquidBulletType`/`SpaceLiquidBulletType` behavior
//! (`entities/bullet/{LiquidBulletType,SpaceLiquidBulletType}.java`).
//!
//! On hit, deposits a puddle and extinguishes fires; gaseous liquids vaporize
//! (handled in `Puddles::deposit`). Space-liquid bullets do not pool.

use bevy_ecs::entity::Entity;

use crate::combat::puddles;
use crate::content::BulletKind;

use super::super::CombatCtx;
use super::super::behavior::BulletBehavior;

/// `Geometry.d4` neighbor offsets.
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// `LiquidBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiquidBehavior;

impl BulletBehavior for LiquidBehavior {
    fn hit(&self, ctx: &mut CombatCtx<'_>, b: Entity, x: f32, y: f32, _create_frags: bool) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let (liquid, space, puddle_size) = {
            let Some(def) = ctx.content.bullet(def_id) else {
                return;
            };
            (
                def.puddle_liquid,
                def.kind == BulletKind::SpaceLiquid,
                def.puddle_amount.max(6.0),
            )
        };
        if space {
            return;
        }
        let ts = crate::config::TILESIZE as f32;
        let (tx, ty) = ((x / ts).floor() as i16, (y / ts).floor() as i16);
        let _ = puddles::deposit(
            ctx.world,
            &ctx.grid.tiles,
            ctx.content,
            tx,
            ty,
            liquid,
            puddle_size,
            ctx.rng,
        );
        // `LiquidBulletType.hit` also extinguishes fires for non-flammable cold
        // liquids (`liquid.temperature <= 0.5 && flammability < 0.3`).
        let cold = ctx
            .content
            .liquid(liquid)
            .is_some_and(|def| def.temperature <= 0.5 && def.flammability < 0.3);
        if cold {
            let intensity = 400.0;
            let _ = crate::combat::fires::extinguish(ctx.world, tx, ty, intensity);
            for (dx, dy) in D4 {
                let _ = crate::combat::fires::extinguish(
                    ctx.world,
                    tx + dx as i16,
                    ty + dy as i16,
                    intensity,
                );
            }
        }
    }
}

/// Static liquid-behavior instance.
pub static LIQUID: LiquidBehavior = LiquidBehavior;
