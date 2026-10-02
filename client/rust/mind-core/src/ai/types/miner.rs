// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MinerAI` (plan 11 §4.2). Ported from
//! `core/src/mindustry/ai/types/MinerAI.java`: choose the nearest ore tile in
//! `mineRange`, walk onto it and mine. Delivery to the core (upstream
//! `offload`/`moveTo` core branch) is plan-08/12 logistics and is marked below.

use bevy_ecs::entity::Entity;

use crate::content::ContentRegistry;
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::MinerComp;
use crate::world::{TilePos, WorldGrid};

use super::super::ai_controller::AiCtx;
use super::super::types::ground::tile_center;

/// Finds the nearest tile that drops an item (`MinerAI.findMinePosition`).
///
/// Checks both the wall block and the floor overlay (`OreBlock`), matching
/// `Tile.drop()` + overlay ore placement. Deterministic row-major scan with an
/// entity-free distance tie-break (nearest, then top-left).
pub fn find_ore_tile(
    grid: &WorldGrid,
    content: &ContentRegistry,
    x: f32,
    y: f32,
    range: f32,
) -> Option<TilePos> {
    let width = grid.width();
    let height = grid.height();
    if width <= 0 || height <= 0 {
        return None;
    }
    let range2 = range * range;
    let cx = WorldGrid::to_tile(x);
    let cy = WorldGrid::to_tile(y);
    let radius = range.ceil() as i32;
    let mut best: Option<(f32, i32, i32)> = None;
    for ty in (cy - radius).max(0)..=(cy + radius).min(height - 1) {
        for tx in (cx - radius).max(0)..=(cx + radius).min(width - 1) {
            let tile = grid.tiles.get(tx, ty);
            let drops = tile.drop(content).is_some()
                || content
                    .block(tile.overlay)
                    .is_some_and(|def| def.item_drop.is_some());
            if !drops {
                continue;
            }
            let (ox, oy) = tile_center(tx, ty);
            let dx = ox - x;
            let dy = oy - y;
            let dist2 = dx * dx + dy * dy;
            if dist2 > range2 {
                continue;
            }
            match best {
                Some((best_dist, best_x, best_y))
                    if best_dist < dist2
                        || (best_dist == dist2 && (best_y, best_x) <= (ty, tx)) => {}
                _ => best = Some((dist2, tx, ty)),
            }
        }
    }
    best.map(|(_, tx, ty)| TilePos::new(tx as i16, ty as i16))
}

/// `MinerAI.updateUnit`: set/pursue the mining tile.
///
/// Returns `true` once the unit is standing on the ore and mining.
pub fn update_miner(ctx: &mut AiCtx, unit: Entity) -> bool {
    let Some((mine_range, mine_tier, hit_size)) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| (def.mine_range.max(40.0), def.mine_tier, def.hit_size))
    else {
        return false;
    };
    if mine_tier <= 0 {
        return false;
    }
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return false;
    };
    let tile = find_ore_tile(ctx.grid, ctx.content, pos.x, pos.y, mine_range);
    let Some(tile) = tile else {
        if let Some(mut miner) = ctx.world.get_mut::<MinerComp>(unit) {
            miner.mine_tile = None;
            miner.mining = false;
        }
        return false;
    };
    if let Some(mut miner) = ctx.world.get_mut::<MinerComp>(unit) {
        miner.mine_tile = Some(tile);
    }
    let (cx, cy) = tile_center(tile.x() as i32, tile.y() as i32);
    let dx = cx - pos.x;
    let dy = cy - pos.y;
    if (dx * dx + dy * dy).sqrt() <= hit_size.max(4.0) {
        if let Some(mut miner) = ctx.world.get_mut::<MinerComp>(unit) {
            miner.mining = true;
        }
        return true;
    }
    ctx.pathfind(unit, tile, 4.0);
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn miner_selects_and_reaches_ore() {
        let mut harness = UnitHarness::new(32, 32, 1);
        // Paint an ore wall directly (ore overlays are plan-06 generation data).
        let ore = harness.content().block_by_name("ore-copper").expect("ore");
        harness.build.grid.tiles.get_mut(10, 10).block = ore.id;
        let unit = harness.spawn("mono", 0, 48.0, 48.0, 0.0).expect("mono");
        let mut reached = false;
        for _ in 0..1500 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            if update_miner(&mut ctx, unit) {
                reached = true;
                break;
            }
        }
        assert!(reached, "miner reached the ore tile");
        let miner = harness.build.world.get::<MinerComp>(unit).unwrap();
        assert!(miner.mine_tile.is_some());
        assert!(miner.mining);
    }
}
