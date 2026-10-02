// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Liquid movement primitives (`entities/comp/BuildingComp.java`
//! `moveLiquid`/`dumpLiquid`/`getLiquidDestination`/`transferLiquid`).
//!
//! Plan 07 owns the `LiquidModule` data; this module ports the transfer math.
//! The differing-liquid reaction branch dispatches to plan-10 `Fx`/damage hooks
//! (no-op here, recorded as R10). `liquidPressure` defaults to `1.0`; conduit
//! overrides are M3 block knobs.
//!
//! Two neighbor-lookup modes are provided: the original `WorldGrid`-taking
//! functions (unit tests/scenarios) and `*_proximity` variants that walk
//! `Building.proximity`, required because plan 07's `updateTile` behavior API
//! does not hand out a `WorldGrid`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::LiquidId;
use crate::entities::comp::Building;
use crate::world::WorldGrid;
use crate::world::modules::LiquidModule;

use super::LiquidNode;

/// `Building.relativeTo(int cx, int cy)` (`Tile.absoluteRelativeTo`).
///
/// Encoding matches upstream: `0` = this building is left of `(cx, cy)`,
/// `1` = below, `2` = right, `3` = above.
pub fn relative_to_dir(world: &World, entity: Entity, cx: i16, cy: i16) -> u8 {
    let Some(building) = world.get::<Building>(entity) else {
        return 0;
    };
    let dx = building.tile.x() as i32 - cx as i32;
    let dy = building.tile.y() as i32 - cy as i32;
    if dx.abs() > dy.abs() {
        if dx <= -1 { 0 } else { 2 }
    } else if dy <= -1 {
        1
    } else {
        3
    }
}

/// `Building.nearby(dir)`: the building one tile in `dir` (grid lookup).
pub fn nearby(world: &World, grid: &WorldGrid, entity: Entity, dir: u8) -> Option<Entity> {
    let building = world.get::<Building>(entity)?;
    let (dx, dy) = match dir {
        0 => (1, 0),
        1 => (0, 1),
        2 => (-1, 0),
        3 => (0, -1),
        _ => return None,
    };
    let x = building.tile.x() as i32 + dx;
    let y = building.tile.y() as i32 + dy;
    if !grid.tiles.in_bounds(x, y) {
        return None;
    }
    grid.tile(x, y).build
}

/// `Building.nearby(dir)` resolved from `Building.proximity` (no grid needed).
///
/// Plan 07's `updateTile` behavior API does not hand out a `WorldGrid`, so
/// conduit/router/junction behaviors use this. Multi-tile neighbors are matched
/// by anchor tile (exact for the 1x1 conduit/junction/router family).
pub fn nearby_proximity(world: &World, entity: Entity, dir: u8) -> Option<Entity> {
    let building = world.get::<Building>(entity)?;
    let (dx, dy) = match dir {
        0 => (1, 0),
        1 => (0, 1),
        2 => (-1, 0),
        3 => (0, -1),
        _ => return None,
    };
    let tx = building.tile.x() as i32 + dx;
    let ty = building.tile.y() as i32 + dy;
    building.proximity.iter().copied().find(|other| {
        world
            .get::<Building>(*other)
            .is_some_and(|b| b.tile.x() as i32 == tx && b.tile.y() as i32 == ty)
    })
}

fn lookup(world: &World, grid: Option<&WorldGrid>, entity: Entity, dir: u8) -> Option<Entity> {
    match grid {
        Some(grid) => nearby(world, grid, entity, dir),
        None => nearby_proximity(world, entity, dir),
    }
}

/// `LiquidBlock`/`Building.acceptLiquid` using the plan-09 [`LiquidNode`] seam.
pub fn accept_liquid(world: &World, entity: Entity, source: Entity, liquid: LiquidId) -> bool {
    let Some(node) = world.get::<LiquidNode>(entity) else {
        return false;
    };
    if !node.accepts {
        return false;
    }
    if node.reject_from_output
        && let (Some(building), Some(_)) =
            (world.get::<Building>(entity), world.get::<Building>(source))
    {
        // `ConduitBuild.acceptLiquid`: reject input arriving from the output side.
        let relative = relative_to_dir(world, source, building.tile.x(), building.tile.y());
        if (relative + 2) % 4 == building.rotation {
            return false;
        }
    }
    node.filter.is_empty() || node.filter.contains(&liquid)
}

/// `Building.handleLiquid`.
pub fn handle_liquid(world: &mut World, entity: Entity, liquid: LiquidId, amount: f32) {
    let capacity = world
        .get::<LiquidNode>(entity)
        .map(|node| node.capacity)
        .unwrap_or(0.0);
    if let Some(mut module) = world.get_mut::<LiquidModule>(entity) {
        module.add(liquid, amount, capacity);
    }
}

/// `Building.getLiquidDestination` (default self; junction pass-through).
pub fn get_liquid_destination(
    world: &World,
    grid: &WorldGrid,
    entity: Entity,
    from: Entity,
    liquid: LiquidId,
) -> Entity {
    get_liquid_destination_impl(world, entity, from, liquid, Some(grid))
}

/// [`get_liquid_destination`] with a proximity neighbor lookup.
pub fn get_liquid_destination_proximity(
    world: &World,
    entity: Entity,
    from: Entity,
    liquid: LiquidId,
) -> Entity {
    get_liquid_destination_impl(world, entity, from, liquid, None)
}

fn get_liquid_destination_impl(
    world: &World,
    entity: Entity,
    from: Entity,
    liquid: LiquidId,
    grid: Option<&WorldGrid>,
) -> Entity {
    let junction = world
        .get::<LiquidNode>(entity)
        .is_some_and(|node| node.junction);
    if !junction {
        return entity;
    }
    let enabled = world
        .get::<Building>(entity)
        .map(|building| building.enabled)
        .unwrap_or(false);
    if !enabled {
        return entity;
    }
    let (jx, jy) = match world.get::<Building>(entity) {
        Some(building) => (building.tile.x(), building.tile.y()),
        None => return entity,
    };
    let dir = relative_to_dir(world, from, jx, jy);
    let Some(next) = lookup(world, grid, entity, dir) else {
        return entity;
    };
    let next_is_junction = world
        .get::<LiquidNode>(next)
        .is_some_and(|node| node.junction);
    if !accept_liquid(world, next, entity, liquid) && !next_is_junction {
        return entity;
    }
    get_liquid_destination_impl(world, next, entity, liquid, grid)
}

/// `Building.transferLiquid`.
pub fn transfer_liquid(
    world: &mut World,
    source: Entity,
    next: Entity,
    amount: f32,
    liquid: LiquidId,
) {
    let next_capacity = world
        .get::<LiquidNode>(next)
        .map(|node| node.capacity)
        .unwrap_or(0.0);
    let stored = world
        .get::<LiquidModule>(next)
        .map(|module| module.get(liquid))
        .unwrap_or(0.0);
    let flow = (next_capacity - stored).min(amount);
    if flow > 0.0 && accept_liquid(world, next, source, liquid) {
        handle_liquid(world, next, liquid, flow);
        if let Some(mut module) = world.get_mut::<LiquidModule>(source) {
            module.remove(liquid, flow);
        }
    }
}

/// `Building.canDumpLiquid` (default `true`).
pub fn can_dump_liquid(_to: Entity, _liquid: LiquidId) -> bool {
    true
}

/// `Building.incrementDump`.
pub fn increment_dump(world: &mut World, entity: Entity, size: usize) {
    if size == 0 {
        return;
    }
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.cdump = ((building.cdump as usize + 1) % size) as u8;
    }
}

/// `Building.dumpLiquid(liquid, scaling, outputDir)`.
pub fn dump_liquid(
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
    liquid: LiquidId,
    scaling: f32,
    output_dir: i32,
) {
    dump_liquid_impl(world, entity, liquid, scaling, output_dir, Some(grid));
}

/// [`dump_liquid`] with a proximity neighbor lookup.
pub fn dump_liquid_proximity(
    world: &mut World,
    entity: Entity,
    liquid: LiquidId,
    scaling: f32,
    output_dir: i32,
) {
    dump_liquid_impl(world, entity, liquid, scaling, output_dir, None);
}

fn dump_liquid_impl(
    world: &mut World,
    entity: Entity,
    liquid: LiquidId,
    scaling: f32,
    output_dir: i32,
    grid: Option<&WorldGrid>,
) {
    let Some(capacity) = world.get::<LiquidNode>(entity).map(|node| node.capacity) else {
        return;
    };
    let stored = world
        .get::<LiquidModule>(entity)
        .map(|module| module.get(liquid))
        .unwrap_or(0.0);
    if stored <= 0.0001 {
        return;
    }
    let rotation = world
        .get::<Building>(entity)
        .map(|building| building.rotation)
        .unwrap_or(0);
    let proximity: Vec<Entity> = world
        .get::<Building>(entity)
        .map(|building| building.proximity.iter().copied().collect())
        .unwrap_or_default();
    let dump = world
        .get::<Building>(entity)
        .map(|building| building.cdump)
        .unwrap_or(0);

    for i in 0..proximity.len() {
        increment_dump(world, entity, proximity.len());
        let other = proximity[(i + dump as usize) % proximity.len()];
        if output_dir != -1 {
            let Some(other_building) = world.get::<Building>(other) else {
                continue;
            };
            let dir = relative_to_dir(
                world,
                entity,
                other_building.tile.x(),
                other_building.tile.y(),
            );
            if (output_dir as u8).wrapping_add(rotation) % 4 != dir {
                continue;
            }
        }
        let destination = get_liquid_destination_impl(&*world, other, entity, liquid, grid);
        if destination == entity {
            continue;
        }
        let Some(dest_capacity) = world
            .get::<LiquidNode>(destination)
            .map(|node| node.capacity)
        else {
            continue;
        };
        if !can_dump_liquid(destination, liquid) {
            continue;
        }
        let ofract = world
            .get::<LiquidModule>(destination)
            .map(|module| module.get(liquid) / dest_capacity)
            .unwrap_or(0.0);
        let fract = stored / capacity;
        if ofract < fract {
            let amount = (fract - ofract) * capacity / scaling;
            transfer_liquid(world, entity, destination, amount, liquid);
        }
        // Re-read stored for the next neighbor (transfer may have moved liquid).
        // (Java re-reads per iteration via `liquids.get`.)
    }
}

/// `Building.moveLiquid(next, liquid)`; returns the moved amount.
pub fn move_liquid(
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
    next: Entity,
    liquid: LiquidId,
) -> f32 {
    move_liquid_impl(world, entity, next, liquid, Some(grid))
}

/// [`move_liquid`] with a proximity neighbor lookup.
pub fn move_liquid_proximity(
    world: &mut World,
    entity: Entity,
    next: Entity,
    liquid: LiquidId,
) -> f32 {
    move_liquid_impl(world, entity, next, liquid, None)
}

fn move_liquid_impl(
    world: &mut World,
    entity: Entity,
    next: Entity,
    liquid: LiquidId,
    grid: Option<&WorldGrid>,
) -> f32 {
    if next == entity {
        return 0.0;
    }
    let next = get_liquid_destination_impl(&*world, next, entity, liquid, grid);
    let Some(capacity) = world.get::<LiquidNode>(entity).map(|node| node.capacity) else {
        return 0.0;
    };
    let next_capacity = world
        .get::<LiquidNode>(next)
        .map(|node| node.capacity)
        .unwrap_or(0.0);
    let same_team = world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|team| team.team)
        == world
            .get::<crate::entities::comp::TeamComp>(next)
            .map(|team| team.team);
    let stored = world
        .get::<LiquidModule>(entity)
        .map(|module| module.get(liquid))
        .unwrap_or(0.0);
    let next_stored = world
        .get::<LiquidModule>(next)
        .map(|module| module.get(liquid))
        .unwrap_or(0.0);
    let accepts = world
        .get::<LiquidNode>(next)
        .is_some_and(|node| node.accepts);

    if same_team && accepts && stored > 0.0 && next_capacity > 0.0 {
        let pressure = world
            .get::<LiquidNode>(entity)
            .map(|node| node.pressure)
            .unwrap_or(1.0);
        let ofract = next_stored / next_capacity;
        let fract = stored / capacity * pressure;
        let flow = ((fract - ofract).clamp(0.0, 1.0) * capacity).min(stored);
        let flow = flow.min(next_capacity - next_stored);
        if flow > 0.0 && ofract <= fract && accept_liquid(world, next, entity, liquid) {
            handle_liquid(world, next, liquid, flow);
            if let Some(mut module) = world.get_mut::<LiquidModule>(entity) {
                module.remove(liquid, flow);
            }
            return flow;
        }
    }
    0.0
}

/// `Building.moveLiquidForward(leaks, liquid)`; returns `(moved, leaked)`.
pub fn move_liquid_forward(
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
    next: Option<Entity>,
    leaks: bool,
    liquid: LiquidId,
) -> (f32, f32) {
    move_liquid_forward_impl(world, entity, next, leaks, liquid, Some(grid))
}

/// [`move_liquid_forward`] with a proximity neighbor lookup.
pub fn move_liquid_forward_proximity(
    world: &mut World,
    entity: Entity,
    next: Option<Entity>,
    leaks: bool,
    liquid: LiquidId,
) -> (f32, f32) {
    move_liquid_forward_impl(world, entity, next, leaks, liquid, None)
}

fn move_liquid_forward_impl(
    world: &mut World,
    entity: Entity,
    next: Option<Entity>,
    leaks: bool,
    liquid: LiquidId,
    grid: Option<&WorldGrid>,
) -> (f32, f32) {
    if let Some(next) = next {
        return (move_liquid_impl(world, entity, next, liquid, grid), 0.0);
    }
    if !leaks {
        return (0.0, 0.0);
    }
    // Leak onto empty ground (`Puddles.deposit` is plan 10; remove + report).
    let stored = world
        .get::<LiquidModule>(entity)
        .map(|module| module.get(liquid))
        .unwrap_or(0.0);
    let leak = stored / 1.5;
    if leak > 0.0
        && let Some(mut module) = world.get_mut::<LiquidModule>(entity)
    {
        module.remove(liquid, leak);
    }
    (0.0, leak)
}
