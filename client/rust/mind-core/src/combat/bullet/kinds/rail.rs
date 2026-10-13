// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RailBulletType` behavior (`entities/bullet/RailBulletType.java`).
//!
//! An instant beam: `init` runs `Damage.collideLine` along `length`, records
//! the result in `fdata` and removes the (non-moving) bullet. `hitTile` only
//! applies the pierce falloff — no splash/heal. The `pierceEffect`/`pointEffect`
//! drawing is view data (G03).

use bevy_ecs::entity::Entity;

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, HIT, collide_line_bullet, line_hit::rail_handle_pierce};

/// `RailBulletType` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct RailBehavior;

impl BulletBehavior for RailBehavior {
    fn init(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let length = def.length;
        let pierce_cap = def.pierce_cap;
        let Some(((x, y), _vel, rotation)) = super::motion(ctx, b) else {
            return;
        };
        // `RailBulletType.init` (`RailBulletType.java:59-64`): the beam runs
        // immediately with `large = false`, `laser = false`.
        let _ = collide_line_bullet(ctx, b, x, y, rotation, length, false, pierce_cap);
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.set(HIT);
        }
    }

    fn hit_tile(
        &self,
        ctx: &mut CombatCtx<'_>,
        b: Entity,
        _build: Entity,
        x: f32,
        y: f32,
        initial_health: f32,
        _direct: bool,
    ) {
        // `RailBulletType.hitTile` (`RailBulletType.java:106-109`): only the
        // pierce falloff, no `hit` pipeline.
        rail_handle_pierce(ctx, b, initial_health, x, y);
    }

    fn test_collision(
        &self,
        _content: &crate::content::ContentRegistry,
        _def: crate::content::BulletId,
        team: u8,
        target_team: u8,
    ) -> bool {
        // `RailBulletType.testCollision` (`RailBulletType.java:102-104`).
        team != target_team
    }
}

/// Static rail-behavior instance.
pub static RAIL: RailBehavior = RailBehavior;
