// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building proximity linking (`core/src/mindustry/entities/comp/BuildingComp.java`
//! `updateProximity`/`removeFromProximity`).
//!
//! Ported exactly: `Edges.edges(size)` neighbor order, same-team linking,
//! `onProximityAdded` → `onProximityUpdate` → neighbor `onProximityUpdate`.
//! Java has no public event for this; [`ProximityUpdateEvent`] is a Rust-only
//! observable fired once after the hooks so plan 11's indexer and plan 16's
//! renderer can invalidate without changing hook order (plan 07 §3.8).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};

use super::WorldGrid;
use super::block::BlockTable;
use super::edges::edges;

/// Rust-only observable fired after `onProximityUpdate` completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProximityUpdateEvent {
    /// Building whose proximity changed.
    pub entity: Entity,
}

/// Recomputes a building's proximity (`Building.updateProximity`).
pub fn update_proximity(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
) -> Option<ProximityUpdateEvent> {
    let (tile, block, team) = read_identity(world, entity)?;
    let inst = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))?;
    let size = inst.def.size.max(1);
    let behavior = inst.behavior.clone();

    // Clear, then gather neighbors in `Edges` order.
    let mut neighbors: Vec<Entity> = Vec::new();
    {
        let mut building = world.get_mut::<crate::entities::comp::Building>(entity)?;
        building.proximity.clear();
    }

    let center = tile;
    for offset in edges(size) {
        let tx = center.x() as i32 + offset.x() as i32;
        let ty = center.y() as i32 + offset.y() as i32;
        if !grid.tiles.in_bounds(tx, ty) {
            continue;
        }
        let Some(other) = grid.tile(tx, ty).build else {
            continue;
        };
        if other == entity {
            continue;
        }
        // Teammate-only linking.
        let other_team = world
            .get::<crate::entities::comp::TeamComp>(other)
            .map(|t| t.team);
        if other_team != Some(team) {
            continue;
        }
        if !neighbors.contains(&other) {
            neighbors.push(other);
        }
        // Add self to the neighbor's proximity if absent.
        if let Some(mut neighbor) = world.get_mut::<crate::entities::comp::Building>(other)
            && !neighbor.proximity.contains(&entity)
        {
            neighbor.proximity.push(entity);
        }
    }

    if let Some(mut building) = world.get_mut::<crate::entities::comp::Building>(entity) {
        building.proximity = neighbors.iter().copied().collect();
    }

    // Hook order: own added -> own update -> neighbor updates.
    behavior.on_proximity_added(world, entity);
    behavior.on_proximity_update(world, entity);
    for neighbor in &neighbors {
        let other = *neighbor;
        if let Some(block) = world
            .get::<crate::entities::comp::Building>(other)
            .map(|building| building.block)
            && let Some(inst) = world
                .get_resource::<BlockTable>()
                .and_then(|table| table.instance(block))
        {
            inst.behavior.on_proximity_update(world, other);
        }
    }

    // `Block.drawCached -> recache()` is plan 16's (no-op in core).
    let _ = content;
    Some(ProximityUpdateEvent { entity })
}

/// Removes an entity from every neighbor's proximity list
/// (`Building.removeFromProximity`).
pub fn remove_from_proximity(
    world: &mut World,
    entity: Entity,
    content: &ContentRegistry,
) -> Option<ProximityUpdateEvent> {
    let (_tile, block, _team) = read_identity(world, entity)?;
    let inst = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))?;
    let behavior = inst.behavior.clone();

    behavior.on_proximity_removed(world, entity);

    let neighbors: Vec<Entity> = world
        .get::<crate::entities::comp::Building>(entity)
        .map(|building| building.proximity.iter().copied().collect())
        .unwrap_or_default();

    for other in &neighbors {
        let other = *other;
        if let Some(mut building) = world.get_mut::<crate::entities::comp::Building>(other) {
            building.proximity.retain(|candidate| *candidate != entity);
        }
        if let Some(block) = world
            .get::<crate::entities::comp::Building>(other)
            .map(|building| building.block)
            && let Some(inst) = world
                .get_resource::<BlockTable>()
                .and_then(|table| table.instance(block))
        {
            inst.behavior.on_proximity_update(world, other);
        }
    }

    if let Some(mut building) = world.get_mut::<crate::entities::comp::Building>(entity) {
        building.proximity.clear();
    }
    let _ = content;
    Some(ProximityUpdateEvent { entity })
}

fn read_identity(world: &World, entity: Entity) -> Option<(super::TilePos, BlockId, u8)> {
    let building = world.get::<crate::entities::comp::Building>(entity)?;
    let team = world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|t| t.team)?;
    Some((building.tile, building.block, team))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn two_walls() -> (EcsWorld, WorldGrid, ContentRegistry, Entity, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let wall_id = content.block_id("copper-wall").expect("wall");
        let inst = table.instance(wall_id).expect("instance");
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let item_count = content.items().len();
        let liquid_count = content.liquids().len();
        let first = inst.spawn(
            &mut world,
            0,
            TilePos::new(4, 4),
            0,
            0,
            item_count,
            liquid_count,
        );
        let second = inst.spawn(
            &mut world,
            1,
            TilePos::new(5, 4),
            0,
            0,
            item_count,
            liquid_count,
        );
        world.insert_resource(table);
        let mut grid = WorldGrid::new(8, 8);
        grid.tiles.get_mut(4, 4).build = Some(first);
        grid.tiles.get_mut(5, 4).build = Some(second);
        (world, grid, content, first, second)
    }

    #[test]
    fn two_adjacent_walls_link() {
        let (mut world, grid, content, first, second) = two_walls();
        let event = update_proximity(&mut world, &grid, &content, first);
        assert!(event.is_some());
        let first_proximity = world.get::<Building>(first).expect("first");
        assert!(first_proximity.proximity.contains(&second));
        let second_proximity = world.get::<Building>(second).expect("second");
        assert!(second_proximity.proximity.contains(&first));
        assert!(world.get::<TeamComp>(first).is_some());
    }

    #[test]
    fn remove_from_proximity_unlinks_both() {
        let (mut world, grid, content, first, second) = two_walls();
        update_proximity(&mut world, &grid, &content, first);
        let event = remove_from_proximity(&mut world, first, &content);
        assert!(event.is_some());
        assert!(
            world
                .get::<Building>(first)
                .expect("first")
                .proximity
                .is_empty()
        );
        assert!(
            !world
                .get::<Building>(second)
                .expect("second")
                .proximity
                .contains(&first)
        );
    }
}
