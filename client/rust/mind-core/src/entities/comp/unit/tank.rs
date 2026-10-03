// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TankComp` tread state + overlap scan (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/TankComp.java`. The port keeps
//! the deterministic tread timer and the overlapping-tile scan that derives
//! `lastSlowdown`/`lastDeepFloor`. Tread dust rects are plan-17.
//!
//! **Plan-02/06 status (`lane/f27-floor-meta`):** `BlockDef` now carries
//! `crushFragile`/`crushDamageMultiplier`/`unitMoveBreakable` and `FloorDef`
//! carries `isDeep`/`speedMultiplier`, so [`update_tank`] applies the
//! `crushFragile` instant-kill and `crushDamage` building damage from
//! `TankComp.java`, and [`floor_speed_multiplier`] ports the tread-speed
//! replacement. `unitMoveBreakable` deconstruct still needs plan-07's
//! tile-removal API (a `&WorldGrid` cannot mutate the grid here); it is
//! owner-noted, not silently dropped.
//!
//! Deviation: `state.rules.unitDamage(team)` is not threaded into this movement
//! pass yet (plan 12/23), so crush damage uses a factor of `1`; `disarmed`
//! gating awaits plan-10's status feed.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::content::ContentRegistry;
use crate::content::registries::units::UnitTypeDef;
use crate::entities::comp::unit::comp::{HitboxComp, TankComp};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::world::behavior::environment::floor_is_deep;
use crate::world::{BlockTable, WorldGrid};

/// `TankComp.update`: advance the tread animation and recompute the overlap
/// slowdown/deep-floor state.
///
/// Returns the tiles overrun with a `unitMoveBreakable` block
/// (`ConstructBlock.deconstructFinish(t, t.block(), self())`). The caller owns
/// tile mutation through plan-06 `WorldCtx`, so the requests are collected here
/// rather than mutating the grid from this read-only movement pass.
pub fn update_tank(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
    unit_def: &UnitTypeDef,
) -> Vec<(i16, i16)> {
    let speed = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    let (position, hit_size) = {
        let Some(position) = world.get::<Pos>(entity).copied() else {
            return Vec::new();
        };
        let hit_size = world
            .get::<HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        (position, hit_size)
    };

    // `TankComp.update` crush pass runs before the overlap scan.
    let mut breaks = Vec::new();
    apply_crush(
        world,
        grid,
        content,
        entity,
        position.x,
        position.y,
        hit_size,
        unit_def,
        &mut breaks,
    );

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

    breaks
}

/// `TankComp.floorSpeedMultiplier` (`TankComp.java`):
/// `pow(speedMultiplier, floorMultiplier) * speedMultiplier * lastSlowdown`.
pub fn floor_speed_multiplier(
    meta: Option<crate::world::behavior::environment::FloorMeta>,
    floor_multiplier: f32,
    unit_speed_multiplier: f32,
    last_slowdown: f32,
) -> f32 {
    let speed = meta.map(|meta| meta.speed_multiplier).unwrap_or(1.0);
    speed.powf(floor_multiplier) * unit_speed_multiplier * last_slowdown
}

/// `TankComp.update` crush pass (`TankComp.java`): `crushFragile` instant-kill
/// on the 8 neighbors and `crushDamage` to enemy buildings under the treads.
#[allow(clippy::too_many_arguments)]
fn apply_crush(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    x: f32,
    y: f32,
    hit_size: f32,
    unit_def: &UnitTypeDef,
    breaks: &mut Vec<(i16, i16)>,
) {
    let Some(team) = world.get::<TeamComp>(entity).map(|comp| comp.team) else {
        return;
    };
    let tile_size = crate::config::TILESIZE as f32;

    let block_crush_fragile =
        |block: BlockId| content.block(block).is_some_and(|def| def.crush_fragile);

    // `crushFragile`: tanks destroy fragile enemy buildings in the 8-neighborhood.
    if unit_def.crush_fragile {
        for (ox, oy) in [
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
        ] {
            let tx = WorldGrid::to_tile(x + ox as f32 * tile_size);
            let ty = WorldGrid::to_tile(y + oy as f32 * tile_size);
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let tile = grid.tile(tx, ty);
            if let Some(build) = tile.build
                && world.get::<TeamComp>(build).map(|comp| comp.team) != Some(team)
                && block_crush_fragile(tile.block)
                && let Some(mut health) = world.get_mut::<Health>(build)
            {
                health.health -= 999_999_999.0;
            }
        }
    }

    // `crushDamage`: damage enemy buildings under the treads (radius − 1).
    if unit_def.crush_damage <= 0.0 {
        return;
    }
    let r = ((hit_size * 0.75 / tile_size) as i32).max(0);
    for dx in -r..=r {
        for dy in -r..=r {
            if dx.abs().max(dy.abs()) > r - 1 {
                continue;
            }
            let tx = WorldGrid::to_tile(x + dx as f32 * tile_size);
            let ty = WorldGrid::to_tile(y + dy as f32 * tile_size);
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let tile = grid.tile(tx, ty);
            if let Some(build) = tile.build {
                if world.get::<TeamComp>(build).map(|comp| comp.team) != Some(team)
                    && let Some(mut health) = world.get_mut::<Health>(build)
                {
                    let multiplier = content
                        .block(tile.block)
                        .map(|def| def.crush_damage_multiplier)
                        .unwrap_or(1.0);
                    health.health -= unit_def.crush_damage * multiplier;
                }
            } else if tile.block != BlockId::AIR
                && content
                    .block(tile.block)
                    .is_some_and(|def| def.unit_move_breakable)
            {
                // `ConstructBlock.deconstructFinish(t, t.block(), self())`: the
                // caller removes the tile (plan-06 `WorldCtx`), so queue it.
                breaks.push((tx as i16, ty as i16));
            }
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
    use super::*;
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::TankComp;
    use crate::entities::comp::unit::movement::update_kinematics;
    use crate::entities::comp::{Health, TeamComp};
    use crate::world::behavior::environment::FloorMeta;

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

    #[test]
    fn tank_floor_speed_multiplier_reads_floor_meta() {
        let slow = FloorMeta {
            is_deep: false,
            is_liquid: false,
            shallow: false,
            speed_multiplier: 0.5,
            drown_time: 0.0,
        };
        assert!((floor_speed_multiplier(Some(slow), 1.0, 1.0, 1.0) - 0.5).abs() < 1e-6);
        // Tanks use the raw `speedMultiplier` even on deep floors.
        let deep = FloorMeta {
            is_deep: true,
            is_liquid: true,
            shallow: false,
            speed_multiplier: 0.2,
            drown_time: 200.0,
        };
        assert!((floor_speed_multiplier(Some(deep), 1.0, 2.0, 0.5) - 0.2).abs() < 1e-6);
        assert_eq!(floor_speed_multiplier(None, 1.0, 2.0, 0.5), 1.0);
    }

    #[test]
    fn tank_crush_fragile_destroys_enemy_fragile_building() {
        let mut harness = UnitHarness::new(16, 16, 1);
        let bridge = harness
            .content()
            .block_id("bridge-conveyor")
            .expect("bridge");
        assert!(harness.build.place(9, 8, bridge, 0, true), "place bridge");
        let build = harness.build.grid.tile(9, 8).build.expect("build entity");
        harness.build.world.get_mut::<TeamComp>(build).unwrap().team = 1;
        // `locus` has `crushFragile = true`.
        let unit = harness
            .spawn("locus", 0, 8.5 * 8.0, 8.5 * 8.0, 0.0)
            .expect("locus");
        update_kinematics(
            &mut harness.build.world,
            &harness.build.grid,
            &harness.build.content,
            unit,
            (0.0, 0.0),
        );
        let health = harness.build.world.get::<Health>(build).expect("health");
        assert!(health.health <= 0.0, "crush-fragile building is destroyed");
    }

    #[test]
    fn tank_breaks_unit_move_breakable_prop() {
        let mut harness = UnitHarness::new(16, 16, 1);
        let boulder = harness.content().block_id("boulder").expect("boulder");
        assert!(
            harness
                .content()
                .block(boulder)
                .is_some_and(|def| def.unit_move_breakable),
            "boulder is unitMoveBreakable via the `Prop` constructor"
        );
        harness
            .build
            .with_ctx(|ctx| ctx.set_block(9, 8, boulder, 0, 0));
        assert_eq!(harness.build.grid.tile(9, 8).block, boulder);
        // `conquer` has `crushDamage = 5`; its tread radius covers the neighbor.
        let _unit = harness
            .spawn("conquer", 0, 8.5 * 8.0, 8.5 * 8.0, 0.0)
            .expect("conquer");
        harness.tick();
        assert_eq!(
            harness.build.grid.tile(9, 8).block,
            crate::content::BlockId::AIR,
            "the overrun prop is deconstructed through `WorldCtx`"
        );
    }

    #[test]
    fn tank_crush_damage_scales_by_block_multiplier() {
        let mut harness = UnitHarness::new(16, 16, 1);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.build.place(9, 8, wall, 0, true), "place wall");
        let build = harness.build.grid.tile(9, 8).build.expect("build entity");
        harness.build.world.get_mut::<TeamComp>(build).unwrap().team = 1;
        let before = harness.build.world.get::<Health>(build).unwrap().health;
        // `conquer` has `crushDamage = 5`; `copper-wall` has multiplier 5.
        let unit = harness
            .spawn("conquer", 0, 8.5 * 8.0, 8.5 * 8.0, 0.0)
            .expect("conquer");
        update_kinematics(
            &mut harness.build.world,
            &harness.build.grid,
            &harness.build.content,
            unit,
            (0.0, 0.0),
        );
        let after = harness.build.world.get::<Health>(build).unwrap().health;
        assert!((before - after - 25.0).abs() < 1e-3, "5 * 5 crush damage");
        assert!(after > 0.0);
    }
}
