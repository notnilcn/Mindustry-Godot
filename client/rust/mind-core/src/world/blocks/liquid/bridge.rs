// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LiquidBridge` / `DirectionLiquidBridge` (`world/blocks/liquid/LiquidBridge.java`,
//! `world/blocks/distribution/DirectionLiquidBridge.java`) — plan 09 M5.
//!
//! Upstream these extend plan 08's `ItemBridge` / `DirectionBridge`. The M6 seam
//! that carried duplicate transition components was reconciled at M5: these
//! functions now read plan 08's state directly —
//!
//! * [`ItemBridgeBuild::link`]/[`ItemBridgeBuild::warmup`] for `LiquidBridge`
//!   (`item_bridge.rs`).
//! * [`DirectionBridgeBuild::occupied`]/[`DirectionBridgeBuild::last_link`] and
//!   [`find_link`] for `DirectionLiquidBridge` (`direction_bridge.rs`).
//!
//! The transfer math itself ([`super::movement`]) is base-independent. Plan 08's
//! [`positions_valid`] is re-exported as the single source of truth.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::LiquidId;
use crate::world::WorldGrid;
use crate::world::blocks::distribution::direction_bridge::{DirectionBridgeBuild, find_link};
use crate::world::blocks::distribution::item_bridge::ItemBridgeBuild;
use crate::world::modules::LiquidModule;

use super::current_liquid;
use super::movement::{dump_liquid, move_liquid};

/// `ItemBridge.positionsValid` / `DirectionBridge.positionsValid` (plan 08).
pub use crate::world::blocks::distribution::item_bridge::positions_valid;

/// Resolves a packed link to an entity (`world.build(link)`), or `None`.
pub fn resolve_link(grid: &WorldGrid, link: i32) -> Option<Entity> {
    if link < 0 {
        return None;
    }
    grid.entity_at(crate::world::TilePos::from_pack(link))
}

/// `LiquidBridgeBuild.updateTransport(other)`: moves `current` when warm.
///
/// Returns the moved amount; `0.0` when cold/disabled. `moved |= moved > 0.05`
/// is the caller's animation concern (plan 16).
pub fn update_liquid_bridge(
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
    liquid: LiquidId,
) -> f32 {
    let Some((warmup, link)) = world
        .get::<ItemBridgeBuild>(entity)
        .map(|bridge| (bridge.warmup, bridge.link))
    else {
        return 0.0;
    };
    if warmup < 0.25 {
        return 0.0;
    }
    let Some(other) = resolve_link(grid, link) else {
        return 0.0;
    };
    move_liquid(world, grid, entity, other, liquid)
}

/// `LiquidBridgeBuild.doDump()`: dump `current` in the fallback direction.
pub fn dump_liquid_bridge(world: &mut World, grid: &WorldGrid, entity: Entity, liquid: LiquidId) {
    dump_liquid(world, grid, entity, liquid, 1.0, -1);
}

/// `LiquidBridgeBuild.updateTile` liquid path: transport when warm, else dump.
pub fn update_liquid_bridge_tile(world: &mut World, grid: &WorldGrid, entity: Entity) -> f32 {
    let Some(liquid) = world.get::<LiquidModule>(entity).and_then(current_liquid) else {
        return 0.0;
    };
    let moved = update_liquid_bridge(world, grid, entity, liquid);
    if moved <= 0.0 {
        dump_liquid_bridge(world, grid, entity, liquid);
    }
    moved
}

/// `DirectionLiquidBridgeBuild.updateTile`: move to the linked output, or dump
/// forward when no link is present. Returns the resolved link (also stored in
/// [`DirectionBridgeBuild::last_link`]) for the caller's occupancy bookkeeping.
pub fn update_direction_liquid_bridge(
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
    range: i32,
) -> Option<Entity> {
    let liquid = world.get::<LiquidModule>(entity).and_then(current_liquid);
    let link = find_link(world, entity, range);
    if let Some(mut bridge) = world.get_mut::<DirectionBridgeBuild>(entity) {
        bridge.last_link = link;
    }

    if let Some(link) = link {
        if let Some(liquid) = liquid {
            move_liquid(world, grid, entity, link, liquid);
            let rotation = world
                .get::<crate::entities::comp::Building>(entity)
                .map(|building| building.rotation)
                .unwrap_or(0);
            // `link.occupied[rotation % 4] = this`.
            if let Some(mut target) = world.get_mut::<DirectionBridgeBuild>(link) {
                target.occupied[(rotation % 4) as usize] = Some(entity);
            }
        }
    } else if let Some(liquid) = liquid {
        // `moveLiquidForward(false, current)` along `rotation`.
        let next = forward_neighbor(world, grid, entity);
        super::movement::move_liquid_forward(world, grid, entity, next, false, liquid);
    }

    clear_stale_occupancy(world, entity);
    link
}

/// `DirectionLiquidBridgeBuild.acceptLiquid` occupancy gate (uses plan 08's
/// `occupied`/`find_link`).
pub fn direction_bridge_accepts(world: &World, entity: Entity, source: Entity, range: i32) -> bool {
    let Some(building) = world.get::<crate::entities::comp::Building>(entity) else {
        return false;
    };
    let Some(source_building) = world.get::<crate::entities::comp::Building>(source) else {
        return false;
    };
    let link_ok = find_link(world, entity, range).is_some()
        || find_link(world, source, range) == Some(entity);
    if !link_ok {
        return false;
    }
    let rel = relative_edge(building.tile, source_building.tile);
    if rel < 0 {
        return false;
    }
    let occupied = world
        .get::<DirectionBridgeBuild>(entity)
        .map(|bridge| bridge.occupied[((rel as u8 + 2) % 4) as usize]);
    rel as u8 != building.rotation && occupied.is_none_or(|entry| entry == Some(source))
}

fn clear_stale_occupancy(world: &mut World, entity: Entity) {
    let Some(bridge) = world.get::<DirectionBridgeBuild>(entity).cloned() else {
        return;
    };
    for i in 0..4 {
        let stale = match bridge.occupied[i] {
            None => true,
            Some(other) => {
                let other_rotation = world
                    .get::<crate::entities::comp::Building>(other)
                    .map(|building| building.rotation)
                    .unwrap_or(i as u8);
                other_rotation != i as u8
                    || !crate::world::blocks::distribution::direction_bridge::is_valid(world, other)
                    || world
                        .get::<DirectionBridgeBuild>(other)
                        .map(|bridge| bridge.last_link)
                        .unwrap_or(None)
                        != Some(entity)
            }
        };
        if stale && let Some(mut bridge) = world.get_mut::<DirectionBridgeBuild>(entity) {
            bridge.occupied[i] = None;
        }
    }
}

fn forward_neighbor(world: &World, grid: &WorldGrid, entity: Entity) -> Option<Entity> {
    let building = world.get::<crate::entities::comp::Building>(entity)?;
    let (dx, dy) = match building.rotation % 4 {
        0 => (1, 0),
        1 => (0, 1),
        2 => (-1, 0),
        _ => (0, -1),
    };
    let x = building.tile.x() as i32 + dx;
    let y = building.tile.y() as i32 + dy;
    grid.tiles
        .in_bounds(x, y)
        .then(|| grid.tile(x, y).build)
        .flatten()
}

/// `Building.relativeToEdge(source)` for size-1 tiles (0 left, 1 below, 2 right,
/// 3 above), or `-1` when not orthogonal.
fn relative_edge(origin: crate::world::TilePos, other: crate::world::TilePos) -> i8 {
    let x = origin.x() as i32;
    let y = origin.y() as i32;
    let cx = other.x() as i32;
    let cy = other.y() as i32;
    if x == cx && y == cy - 1 {
        1
    } else if x == cx && y == cy + 1 {
        3
    } else if x == cx - 1 && y == cy {
        0
    } else if x == cx + 1 && y == cy {
        2
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::TileBuilds;
    use crate::world::TilePos;
    use crate::world::blocks::liquid::LiquidNode;

    fn spawn(
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
        rotation: u8,
        node: LiquidNode,
    ) -> Entity {
        let entity = world
            .spawn((
                Building::new(TilePos::new(x, y), BlockId::AIR, rotation),
                TeamComp { team: 0 },
                LiquidModule::with_liquids(2),
                node,
            ))
            .id();
        grid.tiles.get_mut(x as i32, y as i32).build = Some(entity);
        entity
    }

    fn sync_tile_builds(world: &mut World, grid: &WorldGrid) {
        let mut index = world
            .get_resource::<TileBuilds>()
            .cloned()
            .unwrap_or_default();
        index.rebuild(grid);
        world.insert_resource(index);
    }

    #[test]
    fn positions_valid_cartesian_only() {
        assert!(positions_valid(0, 0, 4, 0, 4));
        assert!(positions_valid(0, 0, 0, 4, 4));
        assert!(!positions_valid(0, 0, 4, 4, 6));
        assert!(!positions_valid(0, 0, 5, 0, 4));
    }

    #[test]
    fn bridge_moves_when_warm() {
        let water = LiquidId::WATER;
        let mut world = World::new();
        let mut grid = WorldGrid::new(16, 16);
        let source = spawn(
            &mut world,
            &mut grid,
            0,
            0,
            0,
            LiquidNode {
                capacity: 100.0,
                ..LiquidNode::default()
            },
        );
        let dest = spawn(
            &mut world,
            &mut grid,
            4,
            0,
            0,
            LiquidNode {
                capacity: 100.0,
                accepts: true,
                ..LiquidNode::default()
            },
        );
        world
            .get_mut::<LiquidModule>(source)
            .expect("source")
            .liquids[water.index()] = 50.0;
        world
            .get_mut::<LiquidModule>(source)
            .expect("source")
            .current_amount = 50.0;
        world.entity_mut(source).insert(ItemBridgeBuild {
            link: TilePos::new(4, 0).pack(),
            warmup: 1.0,
            ..ItemBridgeBuild::default()
        });

        let moved = update_liquid_bridge(&mut world, &grid, source, water);
        assert!(moved > 0.0, "moved={moved}");
        assert!(world.get::<LiquidModule>(dest).expect("dest").get(water) > 0.0);

        // Cold: no move.
        world
            .get_mut::<ItemBridgeBuild>(source)
            .expect("link")
            .warmup = 0.0;
        let moved = update_liquid_bridge(&mut world, &grid, source, water);
        assert_eq!(moved, 0.0);
    }

    #[test]
    fn direction_bridge_finds_link_and_occupies() {
        let water = LiquidId::WATER;
        let mut world = World::new();
        let mut grid = WorldGrid::new(16, 16);
        // Two `reinforced-bridge-conduit` instances facing each other.
        let source = spawn(
            &mut world,
            &mut grid,
            0,
            0,
            0,
            LiquidNode {
                capacity: 100.0,
                accepts: true,
                ..LiquidNode::default()
            },
        );
        let dest = spawn(
            &mut world,
            &mut grid,
            4,
            0,
            2,
            LiquidNode {
                capacity: 100.0,
                accepts: true,
                ..LiquidNode::default()
            },
        );
        // `find_link` matches on `DirectionBridgeBuild` presence + block/team;
        // both share `BlockId::AIR` in this fixture.
        world
            .entity_mut(source)
            .insert(DirectionBridgeBuild::default());
        world
            .entity_mut(dest)
            .insert(DirectionBridgeBuild::default());
        world
            .get_mut::<LiquidModule>(source)
            .expect("source")
            .liquids[water.index()] = 40.0;
        world
            .get_mut::<LiquidModule>(source)
            .expect("source")
            .current_amount = 40.0;
        sync_tile_builds(&mut world, &grid);

        let link = update_direction_liquid_bridge(&mut world, &grid, source, 4);
        assert_eq!(link, Some(dest));
        assert!(world.get::<LiquidModule>(dest).expect("dest").get(water) > 0.0);
        // The source registered itself in the target's occupied[rotation=0].
        assert_eq!(
            world
                .get::<DirectionBridgeBuild>(dest)
                .expect("dest bridge")
                .occupied[0],
            Some(source)
        );
    }
}
