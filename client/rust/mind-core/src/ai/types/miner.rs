// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MinerAI` (plan G01 T10a), ported from
//! `core/src/mindustry/ai/types/MinerAI.java:17-91`. The controller picks a
//! target item (lowest held count among `type.mineItems`, filtered by what the
//! map actually contains), walks to the closest ore within `core` reach and
//! sets `MinerComp.mineTile`; delivery happens in `MinerComp.update`
//! (`entities/comp/unit/miner.rs`).

use bevy_ecs::entity::Entity;

use crate::content::{ContentRegistry, ItemId};
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::{ItemsComp, MinerComp};
use crate::world::{TilePos, WorldGrid};

use super::super::ai_controller::AiCtx;
use super::super::types::ground::tile_center;

/// `MinerAI` per-unit state (`mining`, `targetItem`, `ore` + the
/// `timerTarget2`/`timerTarget3` intervals).
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub struct MinerAiState {
    /// Whether the unit is in the mining phase (`MinerAI.mining`, init `true`).
    pub mining: bool,
    /// Item the AI wants to mine (`targetItem`).
    pub target_item: Option<ItemId>,
    /// Ore tile being walked to (`ore`).
    pub ore: Option<TilePos>,
    /// `timer.get(timerTarget2, 60 * 4)` countdown.
    pub timer2: f32,
    /// `timer.get(timerTarget3, 60)` countdown.
    pub timer3: f32,
}

impl Default for MinerAiState {
    fn default() -> Self {
        Self {
            mining: true,
            target_item: None,
            ore: None,
            timer2: 0.0,
            timer3: 0.0,
        }
    }
}

/// Finds the nearest tile that drops any item (`MinerAI.findMinePosition`).
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

/// Finds the nearest tile that drops `item` (`MinerAI.findClosestOre` via
/// `BlockIndexer`): row-major scan, floor block air + the floor/overlay drop,
/// strictly-smaller `dst2` wins (the Java indexer takes the first ore per
/// quadrant; documented deviation).
pub fn find_closest_ore_for_item(
    grid: &WorldGrid,
    content: &ContentRegistry,
    item: ItemId,
    x: f32,
    y: f32,
) -> Option<TilePos> {
    let mut best: Option<(f32, TilePos)> = None;
    for ty in 0..grid.height() {
        for tx in 0..grid.width() {
            let cell = grid.tiles.get(tx, ty);
            if cell.block != crate::content::BlockId::AIR {
                continue;
            }
            let drop = cell
                .drop(content)
                .or_else(|| content.block(cell.overlay).and_then(|def| def.item_drop));
            if drop != Some(item) {
                continue;
            }
            let (ox, oy) = tile_center(tx, ty);
            let dx = ox - x;
            let dy = oy - y;
            let dist2 = dx * dx + dy * dy;
            if best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
                best = Some((dist2, TilePos::new(tx as i16, ty as i16)));
            }
        }
    }
    best.map(|(_, tile)| tile)
}

/// `BlockIndexer.findClosestWallOre`: the nearest tile whose **wall** drops
/// `item` (the erekir `mineWalls` branch).
pub fn find_closest_wall_ore_for_item(
    grid: &WorldGrid,
    content: &ContentRegistry,
    item: ItemId,
    x: f32,
    y: f32,
) -> Option<TilePos> {
    let mut best: Option<(f32, TilePos)> = None;
    for ty in 0..grid.height() {
        for tx in 0..grid.width() {
            let cell = grid.tiles.get(tx, ty);
            if cell.wall_drop(content) != Some(item) {
                continue;
            }
            let (ox, oy) = tile_center(tx, ty);
            let dx = ox - x;
            let dy = oy - y;
            let dist2 = dx * dx + dy * dy;
            if best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
                best = Some((dist2, TilePos::new(tx as i16, ty as i16)));
            }
        }
    }
    best.map(|(_, tile)| tile)
}

/// `BlockIndexer.hasOre(item)`: any floor/overlay ore of `item` on the map.
pub fn has_ore(grid: &WorldGrid, content: &ContentRegistry, item: ItemId) -> bool {
    for ty in 0..grid.height() {
        for tx in 0..grid.width() {
            let cell = grid.tiles.get(tx, ty);
            if cell.block != crate::content::BlockId::AIR {
                continue;
            }
            let drop = cell
                .drop(content)
                .or_else(|| content.block(cell.overlay).and_then(|def| def.item_drop));
            if drop == Some(item) {
                return true;
            }
        }
    }
    false
}

/// `BlockIndexer.hasWallOre(item)`.
pub fn has_wall_ore(grid: &WorldGrid, content: &ContentRegistry, item: ItemId) -> bool {
    for ty in 0..grid.height() {
        for tx in 0..grid.width() {
            if grid.tiles.get(tx, ty).wall_drop(content) == Some(item) {
                return true;
            }
        }
    }
    false
}

/// `content.items().min(pred, key)` (`MinerAI.java:41-49`): the lowest
/// `core.items.get(i)` among the candidate items, ties in id order.
fn select_target_item(
    ctx: &AiCtx,
    candidates: &[ItemId],
    core: Entity,
    mine_floor: bool,
    mine_walls: bool,
    mine_tier: i32,
) -> Option<ItemId> {
    let capacity = |item: ItemId| {
        crate::world::blocks::distribution::transfer::item_count(ctx.world, core, item)
    };
    let mut best: Option<(i32, ItemId)> = None;
    for &item in candidates {
        let can_mine = ctx
            .content
            .item(item)
            .is_some_and(|def| mine_tier >= def.hardness);
        if !can_mine {
            continue;
        }
        let present = (mine_floor && has_ore(ctx.grid, ctx.content, item))
            || (mine_walls && has_wall_ore(ctx.grid, ctx.content, item));
        if !present {
            continue;
        }
        let count = capacity(item);
        if best.is_none_or(|(best_count, _)| count < best_count) {
            best = Some((count, item));
        }
    }
    best.map(|(_, item)| item)
}

/// `MinerAI.updateMovement` (`MinerAI.java:24-91`).
pub fn update_miner_ai(ctx: &mut AiCtx, unit: Entity, state: &mut MinerAiState) {
    let Some(def) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .cloned()
    else {
        return;
    };
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return;
    };
    let team = ctx.team;
    // `Building core = unit.closestCore(); if(!unit.canMine() || core == null) return;`
    let Some(core) =
        crate::entities::comp::unit::miner::closest_core(ctx.world, pos.x, pos.y, team)
    else {
        return;
    };
    if !crate::entities::comp::unit::miner::can_mine(ctx.world, ctx.content, unit) {
        return;
    }
    // `if(!unit.validMine(unit.mineTile)) unit.mineTile(null);`
    let current_tile = ctx
        .world
        .get::<MinerComp>(unit)
        .and_then(|comp| comp.mine_tile);
    if !crate::entities::comp::unit::miner::valid_mine(
        ctx.world,
        ctx.grid,
        ctx.content,
        unit,
        current_tile,
        true,
    ) && let Some(mut comp) = ctx.world.get_mut::<MinerComp>(unit)
    {
        comp.mine_tile = None;
    }

    let ts = crate::config::TILESIZE as f32;
    if state.mining {
        // `timer.get(timerTarget2, 60 * 4) || targetItem == null`.
        state.timer2 -= 1.0;
        if state.timer2 <= 0.0 || state.target_item.is_none() {
            state.timer2 = 240.0;
            state.target_item = select_target_item(
                ctx,
                &def.mine_items,
                core.0,
                def.mine_floor,
                def.mine_walls,
                def.mine_tier,
            );
        }

        // `if(targetItem != null && core.acceptStack(targetItem, 1, unit) == 0)`:
        // the core is full of the target item — stop mining it.
        if let Some(target) = state.target_item
            && crate::entities::comp::unit::miner::accepted_stack(ctx.world, core.0, target, 1) == 0
        {
            if let Some(mut items) = ctx.world.get_mut::<ItemsComp>(unit) {
                items.clear_item();
            }
            if let Some(mut comp) = ctx.world.get_mut::<MinerComp>(unit) {
                comp.mine_tile = None;
            }
            return;
        }

        // `if(unit.stack.amount >= type.itemCapacity || (targetItem != null &&
        // !unit.acceptsItem(targetItem))) mining = false;`
        let stack = ctx
            .world
            .get::<ItemsComp>(unit)
            .copied()
            .unwrap_or_default()
            .item;
        let capacity = def.item_capacity.max(1);
        let full = stack.is_some_and(|(_, amount)| amount >= capacity)
            || state.target_item.is_some_and(|target| match stack {
                Some((held, amount)) => held != target || amount >= capacity,
                None => false,
            });
        if full {
            state.mining = false;
        } else {
            // `if(timer.get(timerTarget3, 60) && targetItem != null)`.
            state.timer3 -= 1.0;
            if state.timer3 <= 0.0
                && let Some(target) = state.target_item
            {
                state.timer3 = 60.0;
                state.ore = None;
                if def.mine_floor {
                    state.ore =
                        find_closest_ore_for_item(ctx.grid, ctx.content, target, core.1, core.2);
                }
                if state.ore.is_none() && def.mine_walls {
                    state.ore = find_closest_wall_ore_for_item(
                        ctx.grid,
                        ctx.content,
                        target,
                        core.1,
                        core.2,
                    );
                }
            }
            if let Some(ore) = state.ore {
                // `moveTo(ore, type.mineRange / 2f, 20f)` (no lookAt).
                crate::ai::types::flying::move_to_point(
                    ctx.world,
                    ctx.grid,
                    ctx.content,
                    unit,
                    ore.x() as f32 * ts,
                    ore.y() as f32 * ts,
                    def.mine_range / 2.0,
                    20.0,
                    false,
                );
                // `if(unit.within(ore, type.mineRange) && unit.validMine(ore))`.
                let (ox, oy) = (ore.x() as f32 * ts, ore.y() as f32 * ts);
                let dx = ox - pos.x;
                let dy = oy - pos.y;
                let range = def.mine_range.max(40.0);
                if dx * dx + dy * dy <= range * range
                    && crate::entities::comp::unit::miner::valid_mine(
                        ctx.world,
                        ctx.grid,
                        ctx.content,
                        unit,
                        Some(ore),
                        false,
                    )
                    && let Some(mut comp) = ctx.world.get_mut::<MinerComp>(unit)
                {
                    comp.mine_tile = Some(ore);
                }
            }
        }
    } else {
        // Delivery phase (`MinerAI.java:64-84`).
        if let Some(mut comp) = ctx.world.get_mut::<MinerComp>(unit) {
            comp.mine_tile = None;
        }
        let stack = ctx
            .world
            .get::<ItemsComp>(unit)
            .copied()
            .unwrap_or_default()
            .item;
        let Some((held, amount)) = stack else {
            state.mining = true;
            return;
        };
        // `if(unit.within(core, unit.type.range))`.
        let dx = core.1 - pos.x;
        let dy = core.2 - pos.y;
        if dx * dx + dy * dy <= def.range * def.range {
            if crate::entities::comp::unit::miner::accepted_stack(ctx.world, core.0, held, amount)
                > 0
            {
                crate::world::blocks::distribution::transfer::dispatch_handle_stack(
                    ctx.world, core.0, held, amount,
                );
            }
            if let Some(mut items) = ctx.world.get_mut::<ItemsComp>(unit) {
                items.clear_item();
            }
            state.mining = true;
        }
        // `circle(core, unit.type.range / 1.8f)`.
        crate::ai::types::flying::circle_pos(
            ctx.world,
            ctx.grid,
            ctx.content,
            unit,
            core.1,
            core.2,
            def.range / 1.8,
        );
    }

    // `if(!unit.type.flying) unit.updateBoosting(boostWhenMining || duct ||
    // damageTaken > 0 || deep)` — the port has no per-kind floor duct/damage
    // data yet, so deep floors stand in (`FloorMeta.is_deep`).
    if !def.flying {
        let deep = ctx
            .world
            .get::<Pos>(unit)
            .and_then(|pos| {
                ctx.grid
                    .tiles
                    .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y))
            })
            .and_then(|tile| {
                ctx.world
                    .get_resource::<crate::world::BlockTable>()
                    .and_then(|table| {
                        crate::world::behavior::environment::floor_meta(table, tile.floor)
                    })
            })
            .is_some_and(|meta| meta.is_deep);
        let boosting = def.boost_when_mining || deep;
        if let Some(mut core) = ctx
            .world
            .get_mut::<crate::entities::comp::unit::comp::UnitCore>(unit)
        {
            core.boosting = boosting;
        }
    }
}
