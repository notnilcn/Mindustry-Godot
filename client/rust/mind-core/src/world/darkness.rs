// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Edge darkness (`World.addDarkness`/`getDarkness`/`getWallDarkness`, plan 06 §3.6).
//!
//! `tile.data` is overloaded upstream as block save data **and** static-wall
//! darkness (risk R9); this ports the BFS exactly. `getDarkness` returns the
//! static-wall component; the `rules.borderDarkness` and sector-polygon
//! components are supplied by the caller (plan 12) until then.

use crate::constants::DARK_RADIUS;
use crate::content::ContentRegistry;

use super::WorldGrid;

/// The four-neighbour offsets (`Geometry.d4`).
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Runs the darkness BFS and writes `tile.data` for static tiles
/// (`World.addDarkness`).
pub fn add_darkness(grid: &mut WorldGrid, content: &ContentRegistry) {
    let width = grid.tiles.width;
    let height = grid.tiles.height;
    let len = (width as usize) * (height as usize);
    if len == 0 {
        return;
    }

    let is_darkened: Vec<bool> = (0..len)
        .map(|i| grid.tiles.geti(i).static_darkness(content))
        .collect();

    let mut dark = vec![0u8; len];
    let mut write = vec![0u8; len];
    let iterations = DARK_RADIUS;

    for (i, darkened) in is_darkened.iter().enumerate() {
        if *darkened {
            dark[i] = iterations;
        }
    }

    for _ in 0..iterations {
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                let mut lower_neighbour = false;
                for (dx, dy) in D4 {
                    let nx = x + dx;
                    let ny = y + dy;
                    if grid.tiles.in_bounds(nx, ny) {
                        let nidx = (ny * width + nx) as usize;
                        if dark[nidx] < dark[idx] {
                            lower_neighbour = true;
                            break;
                        }
                    }
                }
                write[idx] = dark[idx].saturating_sub(lower_neighbour as u8);
            }
        }
        dark.copy_from_slice(&write);
    }

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let mut data: Option<i8> = None;
            if is_darkened[idx] {
                data = Some(dark[idx] as i8);
            }
            if dark[idx] == iterations {
                let mut full = true;
                for (dx, dy) in D4 {
                    let px = x + dx;
                    let py = y + dy;
                    if grid.tiles.in_bounds(px, py) {
                        let nidx = (py * width + px) as usize;
                        if !(is_darkened[idx] && dark[nidx] == 4) {
                            full = false;
                            break;
                        }
                    }
                }
                if full {
                    data = Some((iterations + 1) as i8);
                }
            }
            if let Some(value) = data {
                let tile = grid.tiles.geti_mut(idx);
                tile.data = value;
            }
        }
    }
}

/// Wall darkness at a tile (`World.getWallDarkness`): the Manhattan distance to
/// the nearest non-darkened tile, minus one, clamped to `[0, darkRadius]`.
pub fn get_wall_darkness(grid: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) -> i8 {
    if !grid.tiles.in_bounds(x, y) {
        return 0;
    }
    if !grid.tiles.get(x, y).static_darkness(content) {
        return 0;
    }
    let mut min_dst = DARK_RADIUS as i32 + 1;
    for cx in (x - DARK_RADIUS as i32)..=(x + DARK_RADIUS as i32) {
        for cy in (y - DARK_RADIUS as i32)..=(y + DARK_RADIUS as i32) {
            if grid.tiles.in_bounds(cx, cy) && !grid.tiles.get(cx, cy).static_darkness(content) {
                min_dst = min_dst.min((cx - x).abs() + (cy - y).abs());
            }
        }
    }
    (min_dst - 1).max(0) as i8
}

/// Static-wall darkness component of `World.getDarkness` (the border/sector
/// falloff components are supplied by the caller, plan 12).
pub fn get_static_darkness(grid: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) -> f32 {
    if grid.tiles.in_bounds(x, y) {
        let tile = grid.tiles.get(x, y);
        if tile.static_darkness(content) {
            return tile.data as f32;
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    #[test]
    fn darkness_bfs_marks_fully_enclosed_walls() {
        let content = crate::content::test_support::test_registry();
        // Find any static-kind block (plan 02's environment registrations).
        let Some(block) = content
            .blocks()
            .iter()
            .find(|def| crate::world::tile::is_static_kind(def.kind))
            .map(|def| def.id)
        else {
            return;
        };
        let mut grid = WorldGrid::new(6, 6);
        for tile in grid.tiles.array_mut() {
            tile.block = block;
        }
        add_darkness(&mut grid, &content);
        // The interior is fully enclosed by darkened tiles: `darkRadius + 1`.
        let center = grid.tiles.geti(grid.tiles.index(3, 3));
        assert_eq!(center.data, DARK_RADIUS as i8 + 1);
        // A non-static grid has no darkness at all.
        let mut air = WorldGrid::new(4, 4);
        air.fill(BlockId::STONE_WALL, BlockId::AIR);
        add_darkness(&mut air, &content);
        assert!(air.tiles.iter().all(|tile| tile.data == 0));
    }

    #[test]
    fn no_static_blocks_leaves_data_zero() {
        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(4, 4);
        grid.fill(BlockId::STONE_WALL, BlockId::AIR);
        add_darkness(&mut grid, &content);
        assert!(grid.tiles.iter().all(|tile| tile.data == 0));
    }
}
