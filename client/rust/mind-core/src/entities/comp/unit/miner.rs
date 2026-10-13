// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MinerComp.update` for every unit (plan G01 T10a).
//!
//! Ported from `entities/comp/MinerComp.java:68-122`: tile validation, the
//! `mineTimer` rate (`mineSpeed * rules.unitMineSpeed`), production accounting,
//! the core offload branches (`offloadImmediately()` = the unit is a player)
//! and the single-item `acceptsItem` inventory gate. The AI side
//! (`ai/types/miner.rs`) sets `MinerComp.mine_tile`; the player order
//! (`PlayerInput.mine_tile`) is mirrored in by `ai/types/player.rs`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::units::UnitTypeDef;
use crate::content::{BlockId, ContentRegistry, ItemId};
use crate::entities::comp::Player;
use crate::entities::comp::unit::comp::{ItemsComp, MinerComp, UnitTypeComp};
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::world::blocks::distribution::transfer::{
    dispatch_get_maximum_accepted, dispatch_handle_stack, item_count,
};
use crate::world::blocks::storage::core_block::CoreBuild;
use crate::world::{TilePos, WorldGrid};

/// `Vars.mineTransferRange`: the core-transfer radius shared by all units.
pub const MINE_TRANSFER_RANGE: f32 = 220.0;

/// `Teams.closestCore(x, y, team)`: nearest team core with its world position;
/// ties break by the stable entity index. The core's position is its anchor
/// tile's corner (`Building.x`/`y`), the centre for the odd multiblocks the
/// campaign fields.
pub fn closest_core(world: &World, x: f32, y: f32, team: u8) -> Option<(Entity, f32, f32)> {
    let mut best: Option<(f32, u32, Entity)> = None;
    for entity_ref in world.iter_entities() {
        let id = entity_ref.id();
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
            continue;
        }
        if entity_ref.get::<CoreBuild>().is_none() {
            continue;
        }
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        let ts = crate::config::TILESIZE as f32;
        let (cx, cy) = (building.tile.x() as f32 * ts, building.tile.y() as f32 * ts);
        let dx = cx - x;
        let dy = cy - y;
        let dist2 = dx * dx + dy * dy;
        let index = id.index().index();
        let better = match best {
            None => true,
            Some((best_dist, best_index, _)) => {
                dist2 < best_dist || (dist2 == best_dist && index < best_index)
            }
        };
        if better {
            best = Some((dist2, index, id));
        }
    }
    let (_, _, core) = best?;
    let building = world.get::<Building>(core)?;
    let ts = crate::config::TILESIZE as f32;
    Some((
        core,
        building.tile.x() as f32 * ts,
        building.tile.y() as f32 * ts,
    ))
}

/// `Tile.drop()`/`Tile.wallDrop()` for the mining branches, with the port's
/// overlay fallback for ore tiles.
fn tile_drop(
    grid: &WorldGrid,
    content: &ContentRegistry,
    tile: TilePos,
    walls: bool,
) -> Option<ItemId> {
    let (tx, ty) = (tile.x() as i32, tile.y() as i32);
    if !grid.tiles.in_bounds(tx, ty) {
        return None;
    }
    let cell = grid.tiles.get(tx, ty);
    if walls {
        cell.wall_drop(content)
    } else {
        cell.drop(content)
            .or_else(|| content.block(cell.overlay).and_then(|def| def.item_drop))
    }
}

/// `MinerComp.getMineResult(tile)` (`MinerComp.java:34-48`): the floor/overlay
/// drop when the tile block is air and the type mines floors, the wall drop
/// when it mines walls, gated by `canMine` (item hardness <= `mineTier`).
pub fn get_mine_result(
    content: &ContentRegistry,
    def: &UnitTypeDef,
    grid: &WorldGrid,
    tile: TilePos,
) -> Option<ItemId> {
    let (tx, ty) = (tile.x() as i32, tile.y() as i32);
    if !grid.tiles.in_bounds(tx, ty) {
        return None;
    }
    let cell = grid.tiles.get(tx, ty);
    let result = if def.mine_floor && cell.block == BlockId::AIR {
        tile_drop(grid, content, tile, false)
    } else if def.mine_walls {
        tile_drop(grid, content, tile, true)
    } else {
        None
    }?;
    let hardness = content.item(result).map(|item| item.hardness).unwrap_or(0);
    (def.mine_tier >= hardness).then_some(result)
}

/// `MinerComp.validMine(tile, checkDst)` (`MinerComp.java:50-60`): within
/// `mineRange` (when `check_dst`) and a mineable drop.
pub fn valid_mine(
    world: &World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    tile: Option<TilePos>,
    check_dst: bool,
) -> bool {
    let Some(tile) = tile else {
        return false;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return false;
    };
    if check_dst {
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            return false;
        };
        let ts = crate::config::TILESIZE as f32;
        // `within(tile.worldx(), tile.worldy(), mineRange)` uses the tile corner.
        let (cx, cy) = (tile.x() as f32 * ts, tile.y() as f32 * ts);
        let dx = cx - pos.x;
        let dy = cy - pos.y;
        let range = def.mine_range.max(40.0);
        if dx * dx + dy * dy > range * range {
            return false;
        }
    }
    get_mine_result(content, def, grid, tile).is_some()
}

/// `MinerComp.canMine()`: `mineSpeed * rules.unitMineSpeed(team) > 0` and
/// `mineTier >= 0`.
pub fn can_mine(world: &World, content: &ContentRegistry, entity: Entity) -> bool {
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return false;
    };
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    let speed =
        crate::game::runtime::rules_of(world).map_or(1.0, |rules| rules.unit_mine_speed(team));
    def.mine_speed * speed > 0.0 && def.mine_tier >= 0
}

/// `Unit.acceptsItem(item)` (`UnitComp.java:601-608`): an empty inventory or
/// the same item below `itemCapacity` (and not dead).
fn accepts_item(stack: Option<(ItemId, i32)>, item: ItemId, capacity: i32) -> bool {
    match stack {
        None => true,
        Some((held, amount)) => held == item && amount < capacity,
    }
}

/// How many of `item` a core accepts right now
/// (`CoreBuild.acceptStack(item, amount, unit)`).
pub(crate) fn accepted_stack(world: &World, core: Entity, item: ItemId, amount: i32) -> i32 {
    if amount <= 0 {
        return 0;
    }
    let cap = dispatch_get_maximum_accepted(world, core, item);
    let cur = item_count(world, core, item);
    (cap - cur).clamp(0, amount)
}

/// Adds one `item` to the unit's stack (`addItem`/`transferItemToUnit`).
fn add_one_item(world: &mut World, entity: Entity, item: ItemId, capacity: i32) {
    if let Some(mut items) = world.get_mut::<ItemsComp>(entity) {
        let current = items.item;
        match current {
            None => items.item = Some((item, 1)),
            Some((held, amount)) if held == item && amount < capacity => {
                items.item = Some((item, amount + 1));
            }
            _ => {}
        }
    }
}

/// `MinerComp.update` (`MinerComp.java:68-122`) for any unit with a non-empty
/// `MinerComp.mine_tile`.
pub fn update_miner_comp(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
) {
    let Some(mut comp) = world.get::<MinerComp>(entity).copied() else {
        return;
    };
    let Some(tile) = comp.mine_tile else {
        return;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .cloned()
    else {
        return;
    };
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    let core = closest_core(world, pos.x, pos.y, team);
    let item = get_mine_result(content, &def, grid, tile);
    let offload_immediately = world.get::<Player>(entity).is_some();
    let capacity = def.item_capacity.max(1);
    let stack = world
        .get::<ItemsComp>(entity)
        .copied()
        .unwrap_or_default()
        .item;

    // `MinerComp.java:74-84`: a non-player unit standing next to its core hands
    // over a stack of a *different* item than the one it is mining.
    if !offload_immediately
        && let Some(core) = core
        && let Some(mined) = item
        && !accepts_item(stack, mined, capacity)
        && let Some((held, amount)) = stack
    {
        let dx = core.1 - pos.x;
        let dy = core.2 - pos.y;
        if dx * dx + dy * dy <= MINE_TRANSFER_RANGE * MINE_TRANSFER_RANGE {
            let accepted = accepted_stack(world, core.0, held, amount);
            if accepted > 0 {
                dispatch_handle_stack(world, core.0, held, accepted);
                if let Some(mut items) = world.get_mut::<ItemsComp>(entity) {
                    items.clear_item();
                }
            }
        }
    }

    // `if((!net.client() || isLocal()) && !validMine(mineTile))`: drop invalid
    // orders (server-local in the port).
    if !valid_mine(world, grid, content, entity, Some(tile), true) {
        comp.mine_tile = None;
        comp.mine_timer = 0.0;
        comp.mining = false;
        write_comp(world, entity, comp);
        return;
    }
    // `mining()` = `mineTile != null && !activelyBuilding()`.
    let actively_building =
        crate::ai::types::player::player_actively_building(world, content, entity, (pos.x, pos.y))
            .is_some();
    let Some(item) = item else {
        comp.mining = false;
        write_comp(world, entity, comp);
        return;
    };
    if actively_building {
        comp.mining = false;
        write_comp(world, entity, comp);
        return;
    }

    // `rate = Time.delta * mineSpeed * rules.unitMineSpeed(team)`.
    let mine_speed =
        crate::game::runtime::rules_of(world).map_or(1.0, |rules| rules.unit_mine_speed(team));
    comp.mine_timer += def.mine_speed * mine_speed;
    let hardness = content.item(item).map(|item| item.hardness).unwrap_or(0);
    let threshold = 50.0
        + if def.mine_hardness_scaling {
            hardness as f32 * 15.0
        } else {
            15.0
        };
    if comp.mine_timer >= threshold {
        comp.mine_timer = 0.0;
        comp.mined = comp.mined.wrapping_add(1);
        // `state.rules.sector.info.handleProduction(item, 1)` for the default
        // team (`MinerComp.java:98`; campaign tick folds the delta).
        if let Some(queues) = world.get_resource::<crate::game::runtime::CampaignCoreQueues>()
            && let Ok(mut state) = queues.0.lock()
            && state.is_campaign
            && team == state.default_team
        {
            state.production_deltas.push((item, 1));
        }
        let core_in_range = core.is_some_and(|(_, cx, cy)| {
            let dx = cx - pos.x;
            let dy = cy - pos.y;
            dx * dx + dy * dy <= MINE_TRANSFER_RANGE * MINE_TRANSFER_RANGE
        });
        let offload_to_core = core
            .filter(|(core_entity, _, _)| {
                core_in_range
                    && offload_immediately
                    && accepted_stack(world, *core_entity, item, 1) == 1
            })
            .map(|(core_entity, _, _)| core_entity);
        if let Some(core_entity) = offload_to_core {
            // `if(item() == item) addItem(item)` then the transfer takes it.
            if stack.map(|(held, _)| held) == Some(item) {
                add_one_item(world, entity, item, capacity);
            }
            dispatch_handle_stack(world, core_entity, item, 1);
        } else if accepts_item(stack, item, capacity) {
            // `InputHandler.transferItemToUnit(item, .., this)`.
            add_one_item(world, entity, item, capacity);
        } else {
            comp.mine_tile = None;
            comp.mine_timer = 0.0;
            comp.mining = false;
            write_comp(world, entity, comp);
            return;
        }
    }
    comp.mining = true;
    write_comp(world, entity, comp);
}

/// `CommandAI.command`'s `unit.mineTile = null` on a command switch.
pub fn clear_mine_tile(world: &mut World, entity: Entity) {
    if let Some(mut comp) = world.get_mut::<MinerComp>(entity) {
        comp.mine_tile = None;
        comp.mine_timer = 0.0;
        comp.mining = false;
    }
}

fn write_comp(world: &mut World, entity: Entity, comp: MinerComp) {
    if let Some(mut stored) = world.get_mut::<MinerComp>(entity) {
        *stored = comp;
    }
}
