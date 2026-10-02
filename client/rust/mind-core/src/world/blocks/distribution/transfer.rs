// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generic item transfer helpers (`BuildingComp` transfer region).
//!
//! These free functions port `acceptItem`/`handleItem`/`offload`/`dump`/
//! `dumpAccumulate`/`moveForward`/`canDump`/`getMaximumAccepted` semantics from
//! `entities/comp/BuildingComp.java` and are shared by the logistics behaviors.
//! Neighbor lookup uses the building `proximity` list (`Building.front/back`);
//! links that are explicitly configured (bridges/drivers) never use this path.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::Building;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;

use super::super::autotiler::relative_to;

/// `Block.itemCapacity` for a building (0 when unknown).
pub fn item_capacity(world: &World, e: Entity) -> i32 {
    let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
        return 0;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block).map(|inst| inst.def.item_capacity))
        .unwrap_or(0)
}

/// `BuildingComp.getMaximumAccepted(item)` default (`block.itemCapacity`).
pub fn get_maximum_accepted(world: &World, e: Entity, _item: ItemId) -> i32 {
    item_capacity(world, e)
}

/// Java default `acceptItem`: item module has room under `getMaximumAccepted`.
pub fn default_accept_item(world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
    let cap = get_maximum_accepted(world, e, item);
    world
        .get::<ItemModule>(e)
        .is_some_and(|items| items.get(item) < cap)
}

/// Java default `handleItem`: add one item to the module.
pub fn default_handle_item(world: &mut World, e: Entity, _src: Entity, item: ItemId) {
    if let Some(mut items) = world.get_mut::<ItemModule>(e) {
        items.add(item, 1, i32::MAX / 2);
    }
}

/// `BuildingComp.acceptStack(item, amount, source)` default.
pub fn default_accept_stack(
    world: &World,
    e: Entity,
    item: ItemId,
    amount: i32,
    source: Option<Entity>,
) -> i32 {
    let source_ok = match source {
        Some(src) => world
            .get::<crate::entities::comp::TeamComp>(src)
            .zip(world.get::<crate::entities::comp::TeamComp>(e))
            .is_some_and(|(a, b)| a.team == b.team),
        None => true,
    };
    if default_accept_item(world, e, source.unwrap_or(e), item) && source_ok {
        (get_maximum_accepted(world, e, item) - item_count(world, e, item)).min(amount)
    } else {
        0
    }
}

/// `BuildingComp.handleStack` default.
pub fn default_handle_stack(world: &mut World, e: Entity, item: ItemId, amount: i32) {
    let cap = item_capacity(world, e);
    if let Some(mut items) = world.get_mut::<ItemModule>(e) {
        items.add(item, amount, cap.max(0));
    }
}

/// `BuildingComp.removeStack` default.
pub fn default_remove_stack(world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
    if let Some(mut items) = world.get_mut::<ItemModule>(e) {
        items.remove(item, amount)
    } else {
        0
    }
}

/// Amount of `item` stored by a building.
pub fn item_count(world: &World, e: Entity, item: ItemId) -> i32 {
    world.get::<ItemModule>(e).map(|m| m.get(item)).unwrap_or(0)
}

/// Total items stored by a building.
pub fn item_total(world: &World, e: Entity) -> i32 {
    world.get::<ItemModule>(e).map(|m| m.total).unwrap_or(0)
}

fn remove_one(world: &mut World, e: Entity, item: ItemId) {
    if let Some(mut items) = world.get_mut::<ItemModule>(e) {
        items.remove(item, 1);
    }
}

/// Dispatches `acceptItem` to `target`'s registered behavior.
pub fn dispatch_accept_item(world: &World, target: Entity, src: Entity, item: ItemId) -> bool {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return false;
    };
    match world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    {
        Some(inst) => inst.behavior.accept_item(world, target, src, item),
        None => false,
    }
}

/// Dispatches `handleItem` to `target`'s registered behavior.
pub fn dispatch_handle_item(world: &mut World, target: Entity, src: Entity, item: ItemId) {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return;
    };
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return;
    };
    inst.behavior.handle_item(world, target, src, item);
}

/// Dispatches `canDump` to `target`'s registered behavior.
pub fn dispatch_can_dump(world: &World, target: Entity, src: Entity, item: ItemId) -> bool {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return false;
    };
    match world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    {
        Some(inst) => inst.behavior.can_dump(world, target, src, item),
        None => false,
    }
}

/// Dispatches `getMaximumAccepted` to `target`'s registered behavior.
pub fn dispatch_get_maximum_accepted(world: &World, target: Entity, item: ItemId) -> i32 {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return 0;
    };
    match world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    {
        Some(inst) => inst.behavior.get_maximum_accepted(world, target, item),
        None => 0,
    }
}

/// Dispatches `canUnload` to `target`'s registered behavior.
pub fn dispatch_can_unload(world: &World, target: Entity) -> bool {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return false;
    };
    match world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    {
        Some(inst) => inst.behavior.can_unload(world, target),
        None => false,
    }
}

/// Dispatches `removeStack` to `target`'s registered behavior.
pub fn dispatch_remove_stack(world: &mut World, target: Entity, item: ItemId, amount: i32) -> i32 {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return 0;
    };
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return 0;
    };
    inst.behavior.remove_stack(world, target, item, amount)
}

/// Dispatches `itemTaken` to `target`'s registered behavior.
pub fn dispatch_item_taken(world: &mut World, target: Entity, item: ItemId) {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return;
    };
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return;
    };
    inst.behavior.item_taken(world, target, item)
}

/// Cloned proximity list of a building.
pub fn proximity(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Building>(e)
        .map(|b| b.proximity.iter().copied().collect())
        .unwrap_or_default()
}

/// The same-team building adjacent in direction `rotation` via the tile index.
///
/// `Building.front()` is the building occupying the tile in the facing
/// direction; using [`nearby`] here handles multiblock neighbors whose center
/// tile is not itself adjacent (their footprint edge is).
pub fn front(world: &World, e: Entity) -> Option<Entity> {
    let rotation = world.get::<Building>(e)?.rotation;
    nearby(world, e, rotation)
}

/// Direction from `e` toward `other` (`Building.relativeTo(Building)`), or `-1`.
pub fn relative_dir(world: &World, e: Entity, other: Entity) -> i8 {
    let (Some(a), Some(b)) = (world.get::<Building>(e), world.get::<Building>(other)) else {
        return -1;
    };
    relative_to(
        a.tile.x() as i32,
        a.tile.y() as i32,
        b.tile.x() as i32,
        b.tile.y() as i32,
    )
}

/// `Building.nearby(dir)`: the adjacent building in absolute direction `dir`.
///
/// Adjacency comes from the same-team `proximity` list (team-only linking),
/// which matches upstream for all same-team transfer paths; cross-team neighbors
/// are rejected by the callers' team checks regardless.
pub fn nearby(world: &World, e: Entity, dir: u8) -> Option<Entity> {
    let building = world.get::<Building>(e)?;
    let (dx, dy) = super::super::autotiler::d4(dir);
    if let Some(index) = world.get_resource::<crate::world::TileBuilds>()
        && !index.cells.is_empty()
    {
        return index.get(building.tile.x() as i32 + dx, building.tile.y() as i32 + dy);
    }
    // Fallback (no grid mirror): same-team proximity center scan.
    for other in &building.proximity {
        if relative_dir(world, e, *other) == dir as i8 {
            return Some(*other);
        }
    }
    None
}

/// `Building.front()` (the `rotation` neighbor).
pub fn back(world: &World, e: Entity) -> Option<Entity> {
    let rotation = world.get::<Building>(e)?.rotation;
    nearby(world, e, (rotation + 2) % 4)
}

/// `Building.relativeToEdge(Tile other)`: direction from the facing edge of
/// `other`'s footprint toward `e`, or `-1`.
pub fn relative_to_edge(world: &World, e: Entity, other: Entity) -> i8 {
    let (Some(a), Some(b)) = (world.get::<Building>(e), world.get::<Building>(other)) else {
        return -1;
    };
    let other_size = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(b.block))
        .map(|inst| inst.def.size)
        .unwrap_or(1)
        .max(1);
    let edge =
        crate::world::edges::facing_edge(other_size, b.tile.x() as i32, b.tile.y() as i32, a.tile);
    relative_to(
        edge.x() as i32,
        edge.y() as i32,
        a.tile.x() as i32,
        a.tile.y() as i32,
    )
}

/// `Edges.getFacingEdge(source.tile, e.tile)` as a direction relative to `e`.
pub fn facing_edge_dir(world: &World, e: Entity, source: Entity) -> i8 {
    relative_to_edge(world, e, source)
}

/// `Building.relativeToEdge(source)`: direction from `e` toward the facing edge
/// of `source` (inverse of [`relative_to_edge`] for size-1 blocks).
pub fn self_to_source(world: &World, e: Entity, source: Entity) -> i8 {
    match relative_to_edge(world, e, source) {
        dir if dir >= 0 => (dir + 2) % 4,
        _ => -1,
    }
}

/// First stored item (`ItemModule.first()`); id order in the Rust module port.
pub fn first_item(world: &World, e: Entity) -> Option<ItemId> {
    world
        .get::<ItemModule>(e)
        .and_then(|module| module.stacks().next().map(|(item, _)| item))
}

/// `Building.incrementDump(prox)`.
pub fn increment_dump(world: &mut World, e: Entity, prox: usize) {
    if prox == 0 {
        return;
    }
    if let Some(mut building) = world.get_mut::<Building>(e) {
        building.cdump = ((building.cdump as usize + 1) % prox) as u8;
    }
}

/// `Building.offload(item)`.
pub fn offload(world: &mut World, e: Entity, item: ItemId) {
    let prox = proximity(world, e);
    let dump = world.get::<Building>(e).map(|b| b.cdump).unwrap_or(0) as usize;
    for i in 0..prox.len() {
        increment_dump(world, e, prox.len());
        let other = prox[(i + dump) % prox.len()];
        if dispatch_accept_item(world, other, e, item) && dispatch_can_dump(world, other, e, item) {
            dispatch_handle_item(world, other, e, item);
            return;
        }
    }
    default_handle_item(world, e, e, item);
}

/// `Building.dump(todump)`.
pub fn dump(world: &mut World, e: Entity, todump: Option<ItemId>) -> bool {
    let Some(building) = world.get::<Building>(e) else {
        return false;
    };
    let prox: Vec<Entity> = building.proximity.iter().copied().collect();
    let cdump = building.cdump as usize;
    if prox.is_empty() {
        return false;
    }
    let (total, candidates) = {
        let Some(module) = world.get::<ItemModule>(e) else {
            return false;
        };
        let candidates: Vec<ItemId> = match todump {
            Some(item) => vec![item],
            None => module.stacks().map(|(item, _)| item).collect(),
        };
        (module.total, candidates)
    };
    if total == 0 {
        return false;
    }
    if let Some(item) = todump
        && item_count(world, e, item) == 0
    {
        return false;
    }

    for i in 0..prox.len() {
        let other = prox[(i + cdump) % prox.len()];
        for item in &candidates {
            if dispatch_accept_item(world, other, e, *item)
                && dispatch_can_dump(world, other, e, *item)
            {
                dispatch_handle_item(world, other, e, *item);
                remove_one(world, e, *item);
                increment_dump(world, e, prox.len());
                return true;
            }
        }
        increment_dump(world, e, prox.len());
    }
    false
}

/// `Building.dumpAccumulate(item)`.
pub fn dump_accumulate(world: &mut World, e: Entity, item: Option<ItemId>) -> bool {
    let delta = world
        .get::<Building>(e)
        .map(|b| b.time_scale)
        .unwrap_or(0.0);
    let mut result = false;
    let accum = {
        let Some(mut building) = world.get_mut::<Building>(e) else {
            return false;
        };
        building.dump_accum += delta;
        building.dump_accum
    };
    let mut accum = accum;
    while accum >= 1.0 {
        result |= dump(world, e, item);
        accum -= 1.0;
    }
    if let Some(mut building) = world.get_mut::<Building>(e) {
        building.dump_accum = accum;
    }
    result
}

/// `Building.moveForward(item)`.
pub fn move_forward(world: &mut World, e: Entity, item: ItemId) -> bool {
    let Some(other) = front(world, e) else {
        return false;
    };
    if dispatch_accept_item(world, other, e, item) {
        dispatch_handle_item(world, other, e, item);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::entities::comp::Building;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn setup() -> (EcsWorld, Entity, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let conveyor = table.get_named("conveyor").expect("conveyor").clone();
        let vault = table.get_named("vault").expect("vault").clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let source = conveyor.spawn(
            &mut world,
            0,
            TilePos::new(2, 2),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        let sink = vault.spawn(
            &mut world,
            1,
            TilePos::new(3, 2),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        // Link proximity manually (grid-free).
        if let Some(mut b) = world.get_mut::<Building>(source) {
            b.proximity.push(sink);
        }
        if let Some(mut b) = world.get_mut::<Building>(sink) {
            b.proximity.push(source);
        }
        (world, source, sink)
    }

    #[test]
    fn dump_moves_item_to_accepting_neighbor() {
        let (mut world, source, sink) = setup();
        let copper = ItemId::new(0);
        if let Some(mut items) = world.get_mut::<ItemModule>(source) {
            items.add(copper, 1, 10);
        }
        assert!(dump(&mut world, source, None));
        assert_eq!(item_count(&world, source, copper), 0);
        // Vault is storage (plan 08 M4); until it registers, generic
        // `handle_item` still deposits through the ItemModule.
        assert_eq!(item_count(&world, sink, copper), 1);
    }

    #[test]
    fn move_forward_uses_facing_neighbor() {
        let (mut world, source, sink) = setup();
        let copper = ItemId::new(0);
        if let Some(mut items) = world.get_mut::<ItemModule>(source) {
            items.add(copper, 1, 10);
        }
        assert!(move_forward(&mut world, source, copper));
        assert_eq!(item_count(&world, sink, copper), 1);
    }
}
