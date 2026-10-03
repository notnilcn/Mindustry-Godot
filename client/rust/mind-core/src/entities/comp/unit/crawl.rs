// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CrawlComp` segment rotation + area scan (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/CrawlComp.java`: the crawl
//! body trails the body `rotation` within `segmentMaxRot` and scans the tiles
//! under it for solids/deeps, deriving `lastCrawlSlowdown` and `lastDeepFloor`.
//! `crawlDust` FX and the area damage to enemy buildings are plan 10/17 owners
//! (the plan-11 status lists melee/crush as plan-10); `floorSpeedMultiplier`
//! additionally needs the plan-02/06 floor speed metadata.
//!
//! **Owner note (plan 02/06):** the scan's deep branch reads
//! [`crate::world::behavior::environment::floor_is_deep`], which is `false` for
//! every block until plan 02 populates `FloorDef.is_deep` (registered
//! `deep-water` is currently a plain `Floor` with `is_deep == false`); the
//! slowdown branch is fully live.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::content::registries::units::UnitTypeDef;
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::{CrawlComp, HitboxComp, UnitCore};
use crate::world::behavior::environment::floor_is_deep;
use crate::world::{BlockTable, WorldGrid};

/// `CrawlComp.update`: rotate the crawl segment toward the movement direction
/// and update the solid/deep scan under the unit.
pub fn update_crawl(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
    unit_def: &UnitTypeDef,
) {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let moving = speed > 0.001;

    let (rotation, position, hit_size) = {
        let rotation = world
            .get::<UnitCore>(entity)
            .map(|core| core.rotation)
            .unwrap_or(0.0);
        let Some(position) = world.get::<Pos>(entity).copied() else {
            return;
        };
        let hit_size = world
            .get::<HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        (rotation, position, hit_size)
    };

    // Scan the tiles under the unit while it moves (upstream `if(moving())`).
    let (scan_slowdown, scan_deep) = if moving {
        scan_tiles(
            world, grid, content, position.x, position.y, hit_size, unit_def,
        )
    } else {
        (None, None)
    };

    if let Some(mut crawl) = world.get_mut::<CrawlComp>(entity) {
        if moving {
            crawl.segment_rot =
                rotate_toward(crawl.segment_rot, rotation, unit_def.segment_rot_speed);
            if let Some(slowdown) = scan_slowdown {
                crawl.last_crawl_slowdown = slowdown;
            }
            crawl.last_deep_floor = scan_deep;
        }
        crawl.segment_rot = clamp_range(crawl.segment_rot, rotation, unit_def.segment_max_rot);
        crawl.crawl_time += speed;
    }
}

/// Tile scan: `(0.75-deep lastDeepFloor, lastCrawlSlowdown)`.
fn scan_tiles(
    world: &World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    x: f32,
    y: f32,
    hit_size: f32,
    unit_def: &UnitTypeDef,
) -> (Option<f32>, Option<crate::content::BlockId>) {
    let tile_size = crate::config::TILESIZE as f32;
    let radius = (hit_size / tile_size * 2.0).max(0.0) as i32;
    let table = world.get_resource::<BlockTable>();
    let mut count = 0i32;
    let mut solids = 0i32;
    let mut deeps = 0i32;
    let mut last_deep = None;
    for cx in -radius..=radius {
        for cy in -radius..=radius {
            if cx * cx + cy * cy > radius {
                continue;
            }
            count += 1;
            let tx = WorldGrid::to_tile(x + cx as f32 * tile_size);
            let ty = WorldGrid::to_tile(y + cy as f32 * tile_size);
            if tx < 0 || ty < 0 || tx >= grid.width() || ty >= grid.height() {
                solids += 1;
                continue;
            }
            let tile = grid.tile(tx, ty);
            if tile.solid(content) {
                solids += 1;
            }
            if let Some(table) = table
                && floor_is_deep(table, tile.floor)
            {
                deeps += 1;
                last_deep = Some(tile.floor);
            }
        }
    }
    let deep_floor = if count > 0 && (deeps as f32) / (count as f32) >= 0.75 {
        last_deep
    } else {
        None
    };
    let slowdown = if count > 0 {
        let fraction = (solids as f32) / (count as f32) / unit_def.crawl_slowdown_frac.max(0.0001);
        Some(lerp(1.0, unit_def.crawl_slowdown, fraction.clamp(0.0, 1.0)))
    } else {
        None
    };
    (slowdown, deep_floor)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn rotate_toward(current: f32, target: f32, max_step: f32) -> f32 {
    let mut diff = (target - current) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    current + diff.clamp(-max_step, max_step)
}

/// `Angles.clampRange(angle, target, range)`: clamp `angle` to the arc around
/// `target` within `±range`.
fn clamp_range(angle: f32, target: f32, range: f32) -> f32 {
    let mut diff = (angle - target) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    target + diff.clamp(-range, range)
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::CrawlComp;
    use crate::entities::comp::unit::movement::update_kinematics;

    #[test]
    fn crawl_segment_rotates_toward_input_rotation() {
        use crate::entities::comp::unit::comp::UnitCore;

        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("latum", 0, 64.0, 64.0, 0.0).expect("latum");
        // Body faces +y; the segment trails it (`segmentRotSpeed` per tick).
        harness
            .build
            .world
            .get_mut::<UnitCore>(unit)
            .unwrap()
            .rotation = 90.0;
        for _ in 0..60 {
            update_kinematics(
                &mut harness.build.world,
                &harness.build.grid,
                &harness.build.content,
                unit,
                (0.0, 2.0),
            );
        }
        let crawl = harness.build.world.get::<CrawlComp>(unit).unwrap();
        // Segment rotated from 0 toward the 90-degree body facing.
        assert!(crawl.segment_rot > 0.0);
        assert!(crawl.crawl_time > 0.0);
    }

    #[test]
    fn crawl_scan_tracks_solids() {
        let mut harness = UnitHarness::new(16, 16, 1);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        // Two adjacent solids under the unit.
        harness.build.place(8, 8, wall, 0, true);
        harness.build.place(8, 9, wall, 0, true);
        let unit = harness
            .spawn("latum", 0, 8.5 * 8.0, 8.5 * 8.0, 0.0)
            .expect("latum");
        for _ in 0..10 {
            update_kinematics(
                &mut harness.build.world,
                &harness.build.grid,
                &harness.build.content,
                unit,
                (0.0, 1.0),
            );
        }
        let crawl = harness.build.world.get::<CrawlComp>(unit).unwrap();
        assert!(
            crawl.last_crawl_slowdown < 1.0,
            "solid tiles reduce the crawl slowdown"
        );
        // Deep metadata is plan-02/06 (see module note): inert until populated.
        assert_eq!(crawl.last_deep_floor, None);
    }
}
