// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MultiBulletType` behavior (`entities/bullet/MultiBulletType.java`).
//!
//! A fake bullet: `init` spawns every child bullet `repeat` times at the same
//! position and removes itself. `spawn_bullets` holds the child defs.

use bevy_ecs::entity::Entity;

use super::super::behavior::BulletBehavior;
use super::super::{BulletSpawn, CombatCtx, HIT};
use super::motion;

/// `MultiBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct MultiBehavior;

impl BulletBehavior for MultiBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let children = def.spawn_bullets.clone();
        let repeat = def.multi_repeat.max(1);
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let team = ctx.team(b);
        let damage = ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0);
        for _ in 0..repeat {
            for &child in &children {
                let spawn = BulletSpawn {
                    def: child,
                    x,
                    y,
                    angle: rotation,
                    team,
                    damage,
                    ..BulletSpawn::default()
                };
                let _ = ctx.spawn(&spawn);
            }
        }
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.set(HIT);
        }
    }

    fn range(&self, def_range: f32) -> f32 {
        def_range
    }
}

/// Static multi-behavior instance.
pub static MULTI: MultiBehavior = MultiBehavior;
