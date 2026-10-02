// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SapBulletType` behavior (`entities/bullet/SapBulletType.java`).
//!
//! `init` line-casts to the first target within `length`, damages it (capped at
//! the target's current health) and heals the shooter by `sapStrength` of the
//! dealt amount. The beam itself is view data (plan 16).

use bevy_ecs::entity::Entity;

use crate::combat::damage::line::linecast;
use crate::entities::comp::TeamComp;

use super::super::behavior::BulletBehavior;
use super::super::{CombatCtx, HIT, hit_building};
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
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let team = ctx.team(b);
        let rand = ctx.rng.range(
            crate::determinism::RngStream::Sim,
            length,
            length + length_rand,
        );
        if let Some(target) = linecast(ctx.grid, team, x, y, rotation, rand) {
            let same_team = ctx.world.get::<TeamComp>(target).map(|t| t.team) == Some(team);
            if !same_team {
                let _ = hit_building(ctx, b, target);
            }
        }
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.fdata = rand;
            bullet.set(HIT);
        }
    }
}

/// Static sap-behavior instance.
pub static SAP: SapBehavior = SapBehavior;
