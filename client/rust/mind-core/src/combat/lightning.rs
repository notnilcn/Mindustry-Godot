// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Lightning` chain helper (`core/src/mindustry/entities/Lightning.java`).
//!
//! Ports the observable simulation: at each step a `damageLightning` bullet is
//! created, the chain snaps to the farthest enemy building within `hitRange*2`
//! (up to `maxChain` links) or advances along a randomized angle. Java's static
//! `lastSeed++` becomes a monotonic [`LightningCounter`] resource so replays are
//! stable (plan 10 §3.2). Rendering (`Fx.lightning`) is plan 17.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use smallvec::SmallVec;

use crate::content::Rgba;
use crate::content::registries::bullets::DAMAGE_LIGHTNING;
use crate::entities::comp::TeamComp;
use crate::random::Rand;

use super::bullet::{BulletSpawn, CombatCtx};

/// Monotonic lightning seed (`Lightning.lastSeed`), reset on match reset.
#[derive(Debug, Clone, Copy, Default, Resource)]
pub struct LightningCounter(pub i32);

/// Chain limits (`Lightning.maxChain`/`hitRange`).
const MAX_CHAIN: usize = 8;
/// `Lightning.hitRange`.
const HIT_RANGE: f32 = 30.0;

/// Result of one lightning branch (test/inspection output).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LightningResult {
    /// Endpoint of the branch.
    pub end: (f32, f32),
    /// Number of chained targets.
    pub links: usize,
    /// Chained target entities in order.
    pub hit: Vec<Entity>,
}

/// Creates a lightning branch (`Lightning.create`).
///
/// `team` should be the caster's team; pass a team that matches nothing to hit
/// everyone (upstream `Team.derelict`).
#[allow(clippy::too_many_arguments)]
pub fn create(
    ctx: &mut CombatCtx<'_>,
    team: u8,
    color: Rgba,
    damage: f32,
    x0: f32,
    y0: f32,
    rotation0: f32,
    length: i32,
) -> LightningResult {
    let seed = next_seed(ctx);
    let mut rand = Rand::new(seed as i64 as u64);
    let mut x = x0;
    let mut y = y0;
    let mut rotation = rotation0;
    let mut hit: Vec<Entity> = Vec::new();
    let mut lines: SmallVec<[(f32, f32); 32]> = SmallVec::new();

    let steps = (length / 2).max(0);
    for _ in 0..steps {
        let spawn = BulletSpawn {
            def: DAMAGE_LIGHTNING,
            x,
            y,
            angle: rotation,
            team,
            damage,
            ..BulletSpawn::default()
        };
        let _ = ctx.spawn(&spawn);
        lines.push((x + rand.range_symmetric(3.0), y + rand.range_symmetric(3.0)));

        if hit.len() < MAX_CHAIN {
            if let Some(target) = farthest_enemy_building(ctx, team, x, y, HIT_RANGE * 2.0, &hit) {
                hit.push(target);
                let (tx, ty) = ctx.pos(target).unwrap_or((x, y));
                x = tx;
                y = ty;
            } else {
                rotation += rand.range_symmetric(20.0);
                let rad = rotation.to_radians();
                x += rad.cos() * HIT_RANGE / 2.0;
                y += rad.sin() * HIT_RANGE / 2.0;
            }
        }
    }

    let _ = color;
    LightningResult {
        end: (x, y),
        links: hit.len(),
        hit,
    }
}

/// Increments and returns the monotonic lightning seed.
fn next_seed(ctx: &mut CombatCtx<'_>) -> i32 {
    if ctx.world.get_resource::<LightningCounter>().is_none() {
        ctx.world.insert_resource(LightningCounter(0));
    }
    let Some(mut counter) = ctx.world.get_resource_mut::<LightningCounter>() else {
        return 0;
    };
    let seed = counter.0;
    counter.0 = counter.0.wrapping_add(1);
    seed
}

/// `Geometry.findFurthest`: farthest enemy building not already chained.
fn farthest_enemy_building(
    ctx: &CombatCtx<'_>,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
    exclude: &[Entity],
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
            if exclude.contains(&build) {
                continue;
            }
            if ctx.world.get::<TeamComp>(build).map(|t| t.team) == Some(team) {
                continue;
            }
            let bx = (tx as f32 + 0.5) * ts;
            let by = (ty as f32 + 0.5) * ts;
            let d2 = (bx - x) * (bx - x) + (by - y) * (by - y);
            if d2 <= range * range && best.is_none_or(|(bd, _)| d2 > bd) {
                best = Some((d2, build));
            }
        }
    }
    best.map(|(_, build)| build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn chain_is_deterministic_for_same_seed() {
        let mut a = CombatHarness::new(48, 48, 5);
        let mut b = CombatHarness::new(48, 48, 5);
        let wall = a.content().block_id("copper-wall").expect("wall");
        assert!(a.place(8, 8, wall, 1, true));
        assert!(b.place(8, 8, wall, 1, true));
        let (ax, ay) = CombatHarness::tile_center(4, 4);
        let ra = a.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 10);
        let rb = b.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 10);
        assert_eq!(ra, rb);
        assert!(ra.links >= 1, "chained to the wall");
    }

    #[test]
    fn chain_stops_after_max_links() {
        let mut a = CombatHarness::new(64, 64, 5);
        let wall = a.content().block_id("copper-wall").expect("wall");
        for x in [8, 10, 12, 14, 16, 18, 20, 22, 24, 26] {
            assert!(a.place(x, 8, wall, 1, true));
        }
        let (ax, ay) = CombatHarness::tile_center(4, 8);
        let result = a.lightning(1, Rgba::WHITE, 10.0, ax, ay, 0.0, 40);
        assert!(result.links <= MAX_CHAIN);
    }
}
