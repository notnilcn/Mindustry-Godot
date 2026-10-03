// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TankComp` tread state + overlap scan (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/TankComp.java`. The port keeps
//! the deterministic tread timer and the overlapping-tile scan that derives
//! `lastSlowdown`/`lastDeepFloor`. Tread dust rects are plan-17.
//!
//! **Owner notes:** `crushFragile`/`crushDamage` damage is plan 10 and needs the
//! plan-02/06 block metadata `crushFragile`/`crushDamageMultiplier`/
//! `unitMoveBreakable` (absent from [`crate::content::BlockDef`] today). The
//! deep-floor branch reads [`crate::world::behavior::environment::floor_is_deep`],
//! which is `false` until plan 02 populates `FloorDef.is_deep`; the slowdown
//! branch is fully live.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::content::registries::units::UnitTypeDef;
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::{HitboxComp, TankComp};
use crate::world::behavior::environment::floor_is_deep;
use crate::world::{BlockTable, WorldGrid};

/// `TankComp.update`: advance the tread animation and recompute the overlap
/// slowdown/deep-floor state.
pub fn update_tank(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
    unit_def: &UnitTypeDef,
) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let (position, hit_size) = {
        let Some(position) = world.get::<Pos>(entity).copied() else {
            return;
        };
        let hit_size = world
            .get::<HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        (position, hit_size)
    };

    let (slowdown, deep_floor) = scan_tiles(
        world, grid, content, position.x, position.y, hit_size, unit_def,
    );

    if let Some(mut tank) = world.get_mut::<TankComp>(entity) {
        tank.last_slowdown = slowdown;
        tank.last_deep_floor = deep_floor;
        // Trigger the animation only when walking manually (`walked`).
        if speed > 0.001 {
            tank.tread_time += speed;
        }
    }
}

/// Overlapping-tile scan: `(lastSlowdown, lastDeepFloor)`.
fn scan_tiles(
    world: &World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    x: f32,
    y: f32,
    hit_size: f32,
    unit_def: &UnitTypeDef,
) -> (f32, Option<crate::content::BlockId>) {
    let tile_size = crate::config::TILESIZE as f32;
    let radius = (hit_size * 0.75 / tile_size).max(0.0) as i32;
    let total = ((radius * 2 + 1) * (radius * 2 + 1)) as f32;
    let table = world.get_resource::<BlockTable>();
    let mut solids = 0i32;
    let mut last_deep = None;
    let mut any_non_deep = false;
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            let tx = WorldGrid::to_tile(x + dx as f32 * tile_size);
            let ty = WorldGrid::to_tile(y + dy as f32 * tile_size);
            let out_of_bounds = tx < 0 || ty < 0 || tx >= grid.width() || ty >= grid.height();
            if out_of_bounds || grid.tile(tx, ty).solid(content) {
                solids += 1;
            }
            let tile = if out_of_bounds {
                None
            } else {
                Some(grid.tile(tx, ty))
            };
            let deep = match (tile, table) {
                (Some(tile), Some(table)) => floor_is_deep(table, tile.floor),
                _ => false,
            };
            if deep {
                last_deep = tile.map(|tile| tile.floor);
            } else {
                any_non_deep = true;
            }
        }
    }
    if any_non_deep {
        last_deep = None;
    }
    let fraction = (solids as f32) / total.max(1.0) / unit_def.crawl_slowdown_frac.max(0.0001);
    let slowdown = lerp(1.0, unit_def.crawl_slowdown, fraction.clamp(0.0, 1.0));
    (slowdown, last_deep)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::TankComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn tank_tread_time_advances() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("stell", 0, 64.0, 64.0, 0.0).expect("stell");
        for _ in 0..8 {
            update_kinematics(
                &mut harness.build.world,
                &harness.build.grid,
                &harness.build.content,
                unit,
                (1.5, 0.0),
            );
        }
        assert!(
            harness
                .build
                .world
                .get::<TankComp>(unit)
                .unwrap()
                .tread_time
                > 0.0
        );
    }

    #[test]
    fn tank_scan_slows_on_solids() {
        let mut harness = UnitHarness::new(16, 16, 1);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        harness.build.place(8, 8, wall, 0, true);
        let unit = harness.spawn("stell", 0, 64.0, 64.0, 0.0).expect("stell");
        update_kinematics(
            &mut harness.build.world,
            &harness.build.grid,
            &harness.build.content,
            unit,
            (0.0, 0.0),
        );
        let tank = harness.build.world.get::<TankComp>(unit).unwrap();
        assert!(
            tank.last_slowdown < 1.0,
            "solid under the treads reduces speed"
        );
        // Deep metadata is plan-02/06 (see module note): inert until populated.
        assert_eq!(tank.last_deep_floor, None);
    }
}
