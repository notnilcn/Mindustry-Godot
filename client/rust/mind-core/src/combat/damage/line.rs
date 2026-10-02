// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Line/laser damage (`core/src/mindustry/entities/Damage.java`
//! `collideLine`/`linecast`/`findLength`).
//!
//! Ports the observable effect for building targets: walk the tile line from the
//! source, damage every enemy building in order, and report the first blocking
//! entity. Piercing lasers with a `pierceCap` stop after the cap.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::TeamComp;
use crate::world::WorldGrid;

use super::area::damage_entity;

/// Converts a world pixel to a tile coordinate (floor; tile centers are `x.5`).
fn to_tile(value: f32) -> i32 {
    (value / crate::config::TILESIZE as f32).floor() as i32
}

/// `Damage.linecast`: first building entity on the ray, or `None`.
///
/// Team filtering is the caller's responsibility (it holds the world).
pub fn linecast(
    grid: &WorldGrid,
    _source_team: u8,
    x: f32,
    y: f32,
    angle_deg: f32,
    length: f32,
) -> Option<Entity> {
    let rad = angle_deg.to_radians();
    let (x1, y1) = (to_tile(x), to_tile(y));
    let (x2, y2) = (
        to_tile(x + rad.cos() * length),
        to_tile(y + rad.sin() * length),
    );
    let mut found = None;
    crate::world::raycast::raycast_each(x1, y1, x2, y2, |tx, ty| {
        if !grid.tiles.in_bounds(tx, ty) {
            return true;
        }
        if let Some(build) = grid.tiles.get(tx, ty).build {
            found = Some(build);
            return true;
        }
        false
    });
    found
}

/// `Damage.collideLine`: damages enemy buildings along the ray.
///
/// Returns `(first_hit, total_applied)`.
#[allow(clippy::too_many_arguments)]
pub fn collide_line(
    world: &mut World,
    content: &ContentRegistry,
    grid: &WorldGrid,
    source_team: u8,
    x: f32,
    y: f32,
    angle_deg: f32,
    length: f32,
    pierce_cap: i32,
    damage: f32,
    pierce_armor: bool,
    armor_multiplier: f32,
) -> (Option<Entity>, f32) {
    let rad = angle_deg.to_radians();
    let (x1, y1) = (to_tile(x), to_tile(y));
    let (x2, y2) = (
        to_tile(x + rad.cos() * length),
        to_tile(y + rad.sin() * length),
    );
    let mut first = None;
    let mut total = 0.0f32;
    let mut hits = 0i32;
    let mut stop = false;
    crate::world::raycast::raycast_each(x1, y1, x2, y2, |tx, ty| {
        if !grid.tiles.in_bounds(tx, ty) {
            return false;
        }
        let Some(build) = grid.tiles.get(tx, ty).build else {
            return false;
        };
        let same_team = world.get::<TeamComp>(build).map(|t| t.team) == Some(source_team);
        if same_team {
            return false;
        }
        if first.is_none() {
            first = Some(build);
        }
        total += damage_entity(
            world,
            content,
            build,
            damage,
            pierce_armor,
            armor_multiplier,
        );
        hits += 1;
        if pierce_cap >= 0 && hits >= pierce_cap {
            stop = true;
        }
        stop
    });
    (first, total)
}

/// `Damage.findLength`: distance along the ray to the first building.
pub fn find_length(
    grid: &WorldGrid,
    _source_team: u8,
    x: f32,
    y: f32,
    angle_deg: f32,
    max_length: f32,
) -> f32 {
    let rad = angle_deg.to_radians();
    let (x1, y1) = (to_tile(x), to_tile(y));
    let (x2, y2) = (
        to_tile(x + rad.cos() * max_length),
        to_tile(y + rad.sin() * max_length),
    );
    let mut dist = max_length;
    crate::world::raycast::raycast_each(x1, y1, x2, y2, |tx, ty| {
        if !grid.tiles.in_bounds(tx, ty) {
            return true;
        }
        if grid.tiles.get(tx, ty).build.is_some() {
            let (cx, cy) = (
                (tx as f32 + 0.5) * crate::config::TILESIZE as f32,
                (ty as f32 + 0.5) * crate::config::TILESIZE as f32,
            );
            dist = ((cx - x).powi(2) + (cy - y).powi(2)).sqrt();
            true
        } else {
            false
        }
    });
    dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn collide_line_damages_enemy_in_order() {
        let mut harness = CombatHarness::new(48, 16, 3);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        for x in 12..=14 {
            assert!(harness.place(x, 8, wall, 0, true));
        }
        let (x, y) = CombatHarness::tile_center(4, 8);
        let (first, total) = collide_line(
            &mut harness.build.world,
            &harness.build.content,
            &harness.build.grid,
            1,
            x,
            y,
            0.0,
            200.0,
            -1,
            40.0,
            false,
            1.0,
        );
        assert!(first.is_some());
        assert_eq!(total, 120.0);
        assert!(harness.building_health_at(12, 8) < 320.0);
    }

    #[test]
    fn linecast_finds_first_building() {
        let mut harness = CombatHarness::new(48, 16, 3);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(12, 8, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(4, 8);
        assert!(linecast(&harness.build.grid, 1, x, y, 0.0, 200.0).is_some());
    }
}
