// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-kind bullet behaviors (`entities/bullet/*.java`).
//!
//! Each module mirrors one Java `BulletType` subclass. Kinds not yet given a
//! dedicated implementation fall back to the base behavior (plan 10 §3.4);
//! subsequent milestones add the remaining kind modules here.

pub mod basic;
pub mod continuous;
pub mod emp;
pub mod empty;
pub mod fire;
pub mod flak;
pub mod interceptor;
pub mod laser;
pub mod lightning;
pub mod liquid;
pub mod mass_driver;
pub mod multi;
pub mod point;
pub mod sap;
pub mod shrapnel;

use bevy_ecs::entity::Entity;

use crate::entities::comp::{Pos, TeamComp, Vel};

use super::CombatCtx;

/// Scans a tile square around `(x, y)` for the nearest enemy building within
/// `range` (used by point/sap/flak kinds).
pub(crate) fn nearest_enemy_building(
    ctx: &CombatCtx<'_>,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
) -> Option<Entity> {
    let ts = crate::config::TILESIZE as f32;
    let min_tx = ((x - range) / ts).floor() as i32;
    let max_tx = ((x + range) / ts).ceil() as i32;
    let min_ty = ((y - range) / ts).floor() as i32;
    let max_ty = ((y + range) / ts).ceil() as i32;
    let mut best: Option<(f32, Entity)> = None;
    for ty in min_ty..=max_ty {
        for tx in min_tx..=max_tx {
            if !ctx.grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let Some(build) = ctx.grid.tiles.get(tx, ty).build else {
                continue;
            };
            if ctx.world.get::<TeamComp>(build).map(|t| t.team) == Some(team) {
                continue;
            }
            let bx = (tx as f32 + 0.5) * ts;
            let by = (ty as f32 + 0.5) * ts;
            let d2 = (bx - x) * (bx - x) + (by - y) * (by - y);
            if d2 <= range * range && best.is_none_or(|(bd, _)| d2 < bd) {
                best = Some((d2, build));
            }
        }
    }
    best.map(|(_, build)| build)
}

/// Reads `(pos, vel, rotation)` for a bullet in one borrow.
#[allow(clippy::type_complexity)]
pub(crate) fn motion(ctx: &CombatCtx<'_>, b: Entity) -> Option<((f32, f32), (f32, f32), f32)> {
    let pos = ctx.world.get::<Pos>(b)?;
    let vel = ctx
        .world
        .get::<Vel>(b)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let rotation = ctx.bullet(b)?.rotation;
    Some(((pos.x, pos.y), (vel.x, vel.y), rotation))
}
