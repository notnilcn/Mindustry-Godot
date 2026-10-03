// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `WaterMoveComp` naval movement state (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/WaterMoveComp.java`. The port
//! keeps the wave-trail timer and advances it with movement, and ports the
//! water solidity predicate (`EntityCollisions.waterSolid`), the `onLiquid`
//! predicate and the naval `floorSpeedMultiplier` replacement. These read the
//! plan-02 floor metadata (`Floor.isLiquid`/`shallow`/`speedMultiplier`) that
//! `BlockKindData::FloorDef` now carries (`lane/f27-floor-meta`).
//!
//! The two wave-trail `Trail` objects remain view data owned by plan 17.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::world::WorldGrid;
use crate::world::block::BlockTable;

use super::comp::WaterMoveComp;

/// `WaterMoveComp.update`: advance the twin wave-trail timers with movement.
pub fn update_water_move(world: &mut World, entity: Entity, delta: (f32, f32)) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    if let Some(mut water) = world.get_mut::<WaterMoveComp>(entity) {
        water.trail_time += speed;
    }
}

/// `EntityCollisions.waterSolid(x, y)`: solid unless the tile is a liquid floor.
pub fn water_solid(
    grid: &WorldGrid,
    content: &crate::content::ContentRegistry,
    table: &BlockTable,
    x: i32,
    y: i32,
) -> bool {
    if !grid.tiles.in_bounds(x, y) {
        return true;
    }
    let tile = grid.tile(x, y);
    tile.solid(content) || !crate::world::behavior::environment::floor_is_liquid(table, tile.floor)
}

/// `WaterMoveComp.onLiquid()`: the tile under the unit is a liquid floor.
pub fn on_liquid(grid: &WorldGrid, table: &BlockTable, x: i32, y: i32) -> bool {
    grid.tiles.in_bounds(x, y)
        && crate::world::behavior::environment::floor_is_liquid(table, grid.tile(x, y).floor)
}

/// `WaterMoveComp.floorSpeedMultiplier` (`(shallow ? 1 : 1.3) * speedMultiplier`).
///
/// `unit_speed_multiplier` is the unit's `speedMultiplier` import; `meta` is the
/// walked floor (`None`/air counts as a non-shallow dry floor).
pub fn floor_speed_multiplier(
    meta: Option<crate::world::behavior::environment::FloorMeta>,
    unit_speed_multiplier: f32,
) -> f32 {
    let shallow = meta.is_some_and(|meta| meta.shallow);
    (if shallow { 1.0 } else { 1.3 }) * unit_speed_multiplier
}

/// Convenience: floor metadata for the tile under `(x, y)`.
pub fn tile_floor_meta(
    table: &BlockTable,
    grid: &WorldGrid,
    x: i32,
    y: i32,
) -> Option<crate::world::behavior::environment::FloorMeta> {
    if !grid.tiles.in_bounds(x, y) {
        return None;
    }
    crate::world::behavior::environment::floor_meta(table, grid.tile(x, y).floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::WaterMoveComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn water_trail_time_advances() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("risso", 0, 64.0, 64.0, 0.0).expect("risso");
        update_kinematics(
            &mut harness.build.world,
            &harness.build.grid,
            &harness.build.content,
            unit,
            (3.0, 0.0),
        );
        assert!(
            harness
                .build
                .world
                .get::<WaterMoveComp>(unit)
                .unwrap()
                .trail_time
                > 0.0
        );
    }

    #[test]
    fn water_predicates_and_floor_speed_read_metadata() {
        use crate::content::BlockId;
        use crate::content::test_support::test_registry;
        use crate::world::WorldGrid;
        use crate::world::behavior::environment::floor_meta;
        use crate::world::block::BlockTable;

        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let stone = content.block_id("stone").expect("stone");
        let deep = content.block_id("deep-water").expect("deep-water");
        let water = content.block_id("shallow-water").expect("shallow-water");
        let sand = content.block_id("sand-water").expect("sand-water");

        let mut grid = WorldGrid::new(4, 4);
        grid.fill(stone, BlockId::AIR);
        grid.tiles.get_mut(0, 1).floor = stone;
        grid.tiles.get_mut(1, 1).floor = deep;
        grid.tiles.get_mut(2, 1).floor = water;
        grid.tiles.get_mut(3, 1).floor = sand;

        assert!(water_solid(&grid, &content, &table, 0, 1));
        assert!(!water_solid(&grid, &content, &table, 1, 1));
        assert!(!water_solid(&grid, &content, &table, 2, 1));
        assert!(!water_solid(&grid, &content, &table, 3, 1));
        assert!(water_solid(&grid, &content, &table, -1, 0));

        assert!(!on_liquid(&grid, &table, 0, 1));
        assert!(on_liquid(&grid, &table, 1, 1));
        assert!(on_liquid(&grid, &table, 2, 1));
        assert!(on_liquid(&grid, &table, 3, 1));

        // `(shallow ? 1 : 1.3) * speedMultiplier`.
        assert!((floor_speed_multiplier(floor_meta(&table, sand), 1.0) - 1.0).abs() < 1e-6);
        assert!((floor_speed_multiplier(floor_meta(&table, deep), 1.0) - 1.3).abs() < 1e-6);
    }
}
