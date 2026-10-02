// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DirectionBridge` build state (`world/blocks/distribution/DirectionBridge.java`)
//! — plan 08 M3; shared with plan 09 (`DirectionLiquidBridge`, plan 08 L10).
//!
//! ## Frozen API for plan 09
//!
//! * `DirectionBridgeBuild { occupied: [Option<Entity>; 4], last_link: Option<Entity> }`.
//! * `find_link(world, entity, range) -> Option<Entity>` scans exactly `range`
//!   tiles ahead along `rotation` for a same-block, same-team `DirectionBridgeBuild`.
//! * `positions_valid(x1, y1, x2, y2, range) -> bool` (`x1 == x2` or `y1 == y2`
//!   within `range`).
//! * The occupancy protocol: each tick a linker writes
//!   `link.occupied[rotation] = self` and clears stale entries whose
//!   `rotation != i`, `!isValid`, or `last_link != self`.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::entities::comp::Building;
use crate::world::TileBuilds;
use crate::world::edges::facing_edge;

/// `DirectionBridge.DirectionBridgeBuild` state (transient; not serialized).
#[derive(Debug, Default, Clone, Component)]
pub struct DirectionBridgeBuild {
    /// Buildings pointing into this bridge, indexed by absolute direction.
    pub occupied: [Option<Entity>; 4],
    /// Last resolved link (`lastLink`).
    pub last_link: Option<Entity>,
}

/// `DirectionBridge.positionsValid(x1, y1, x2, y2, range)`.
pub fn positions_valid(x1: i32, y1: i32, x2: i32, y2: i32, range: i32) -> bool {
    if x1 == x2 {
        (y1 - y2).abs() <= range
    } else if y1 == y2 {
        (x1 - x2).abs() <= range
    } else {
        false
    }
}

/// `DirectionBridge.DirectionBridgeBuild.findLink()`.
pub fn find_link(world: &World, e: Entity, range: i32) -> Option<Entity> {
    let building = world.get::<Building>(e)?;
    let index = world.get_resource::<TileBuilds>()?;
    let block = building.block;
    let team = world
        .get::<crate::entities::comp::TeamComp>(e)
        .map(|t| t.team);
    let (dx, dy) = super::super::autotiler::d4(building.rotation);
    for i in 1..=range {
        let x = building.tile.x() as i32 + dx * i;
        let y = building.tile.y() as i32 + dy * i;
        let Some(other) = index.get(x, y) else {
            continue;
        };
        if other == e {
            continue;
        }
        if world.get::<DirectionBridgeBuild>(other).is_none() {
            continue;
        }
        let same_block = world.get::<Building>(other).map(|b| b.block) == Some(block);
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(other)
            .map(|t| t.team)
            == team;
        if same_block && same_team {
            return Some(other);
        }
    }
    None
}

/// `Building.relativeToEdge(source)` for a size-1 bridge (direction self→source).
pub fn relative_to_edge(world: &World, e: Entity, source: Entity) -> i8 {
    let (Some(a), Some(b)) = (world.get::<Building>(e), world.get::<Building>(source)) else {
        return -1;
    };
    let size = world
        .get_resource::<crate::world::BlockTable>()
        .and_then(|table| table.get(b.block))
        .map(|inst| inst.def.size)
        .unwrap_or(1)
        .max(1);
    let edge = facing_edge(size, b.tile.x() as i32, b.tile.y() as i32, a.tile);
    let x = a.tile.x() as i32;
    let y = a.tile.y() as i32;
    let cx = edge.x() as i32;
    let cy = edge.y() as i32;
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

/// Whether a linked target is still valid (`Building.isValid()` approximation).
pub fn is_valid(world: &World, e: Entity) -> bool {
    world
        .get::<crate::entities::comp::Health>(e)
        .is_none_or(|health| health.health > 0.0)
        && world.get::<Building>(e).is_some()
}

/// Sends the tile-reference `BlockId` of `e` (helper for tests).
pub fn block_of(world: &World, e: Entity) -> Option<BlockId> {
    world.get::<Building>(e).map(|b| b.block)
}
