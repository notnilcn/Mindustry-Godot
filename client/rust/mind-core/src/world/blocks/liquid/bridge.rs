// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LiquidBridge` / `DirectionLiquidBridge` (`world/blocks/liquid/LiquidBridge.java`,
//! `world/blocks/distribution/DirectionLiquidBridge.java`) — plan 09 M5.
//!
//! Upstream these extend plan 08's `ItemBridge` / `DirectionBridge`; plan 08 M3
//! has not landed yet (lane/08-logistics), so this file defines the **minimal
//! seam** the orchestrator reconciles at join:
//!
//! * [`LiquidBridgeLink`] mirrors the `ItemBridgeBuild` fields this plan reads
//!   (`link: i32` packed pos, `range`, `warmup`, `transport_time`, `leaks`).
//!   When 08's `ItemBridgeBuild` lands, delete this component and read the base
//!   fields directly (`// reconcile 08: ItemBridgeBuild.link/warmup`).
//! * [`DirectionLiquidBridgeLink`] mirrors `DirectionBridgeBuild.occupied[4]`.
//! * [`resolve_link`] / [`update_liquid_bridge`] / [`update_direction_bridge`]
//!   port the `updateTransport`/`doDump`/`updateTile` liquid paths verbatim over
//!   the link seam.
//!
//! The transfer math itself ([`super::movement`]) is already base-independent.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::LiquidId;
use crate::world::WorldGrid;
use crate::world::modules::LiquidModule;

use super::current_liquid;
use super::movement::{dump_liquid, move_liquid};

/// `ItemBridgeBuild` subset read by the liquid bridge
/// (`reconcile 08: ItemBridgeBuild.link/warmup`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct LiquidBridgeLink {
    /// Linked building packed tile pos, or `-1` (`ItemBridgeBuild.link`).
    pub link: i32,
    /// `ItemBridge.range` in tiles.
    pub range: i32,
    /// `ItemBridgeBuild.warmup`.
    pub warmup: f32,
    /// `ItemBridge.transportTime`.
    pub transport_time: f32,
    /// `LiquidBridge.leaks`.
    pub leaks: bool,
}

impl Default for LiquidBridgeLink {
    fn default() -> Self {
        Self {
            link: -1,
            range: 4,
            warmup: 0.0,
            transport_time: 50.0,
            leaks: false,
        }
    }
}

/// `DirectionBridgeBuild.occupied[4]` subset
/// (`reconcile 08: DirectionBridgeBuild.occupied`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct DirectionLiquidBridgeLink {
    /// Occupied output point per cardinal direction (packed pos, `-1` = free).
    pub occupied: [i32; 4],
    /// `DirectionLiquidBridge.speed`.
    pub speed: f32,
}

impl Default for DirectionLiquidBridgeLink {
    fn default() -> Self {
        Self {
            occupied: [-1; 4],
            speed: 5.0,
        }
    }
}

/// `ItemBridge.positionsValid(a, b, range)`: same row/column and within range.
pub fn positions_valid(ax: i32, ay: i32, bx: i32, by: i32, range: i32) -> bool {
    (ax == bx && (ay - by).abs() <= range) || (ay == by && (ax - bx).abs() <= range)
}

/// Resolves a packed link to an entity (`world.build(link)`), or `None`.
pub fn resolve_link(_world: &World, grid: &WorldGrid, link: i32) -> Option<Entity> {
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
    let Some(link) = world.get::<LiquidBridgeLink>(entity).copied() else {
        return 0.0;
    };
    if link.warmup < 0.25 {
        return 0.0;
    }
    let Some(other) = resolve_link(world, grid, link.link) else {
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
/// forward when no link is present. `last_link` is returned for the caller's
/// occupancy bookkeeping (plan 16).
pub fn update_direction_liquid_bridge(
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
) -> Option<Entity> {
    world.get::<DirectionLiquidBridgeLink>(entity)?;
    let liquid = world.get::<LiquidModule>(entity).and_then(current_liquid)?;
    let link = first_occupied(world, grid, entity, liquid);
    match link {
        Some(other) => {
            move_liquid(world, grid, entity, other, liquid);
            Some(other)
        }
        None => {
            dump_liquid_bridge(world, grid, entity, liquid);
            None
        }
    }
}

/// First occupied slot whose bridge links back to `entity`
/// (`DirectionBridgeBuild.findLink` over the liquid seam).
pub fn first_occupied(
    world: &World,
    grid: &WorldGrid,
    entity: Entity,
    _liquid: LiquidId,
) -> Option<Entity> {
    let slots = world.get::<DirectionLiquidBridgeLink>(entity)?.occupied;
    for slot in slots {
        let Some(other) = resolve_link(world, grid, slot) else {
            continue;
        };
        let links_back = world
            .get::<DirectionLiquidBridgeLink>(other)
            .is_some_and(|link| {
                link.occupied
                    .iter()
                    .any(|value| *value == packed_of(world, entity))
            });
        if links_back {
            return Some(other);
        }
    }
    None
}

fn packed_of(world: &World, entity: Entity) -> i32 {
    world
        .get::<crate::entities::comp::Building>(entity)
        .map(|building| building.tile.pack())
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::TilePos;
    use crate::world::blocks::liquid::LiquidNode;

    fn spawn(world: &mut World, grid: &mut WorldGrid, x: i16, y: i16, node: LiquidNode) -> Entity {
        let entity = world
            .spawn((
                Building::new(TilePos::new(x, y), BlockId::AIR, 0),
                TeamComp { team: 0 },
                LiquidModule::with_liquids(2),
                node,
            ))
            .id();
        grid.tiles.get_mut(x as i32, y as i32).build = Some(entity);
        entity
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
        world.entity_mut(source).insert(LiquidBridgeLink {
            link: TilePos::new(4, 0).pack(),
            warmup: 1.0,
            ..LiquidBridgeLink::default()
        });

        let moved = update_liquid_bridge(&mut world, &grid, source, water);
        assert!(moved > 0.0, "moved={moved}");
        assert!(world.get::<LiquidModule>(dest).expect("dest").get(water) > 0.0);

        // Cold: no move, dump fallback instead.
        world
            .get_mut::<LiquidBridgeLink>(source)
            .expect("link")
            .warmup = 0.0;
        let moved = update_liquid_bridge(&mut world, &grid, source, water);
        assert_eq!(moved, 0.0);
    }
}
