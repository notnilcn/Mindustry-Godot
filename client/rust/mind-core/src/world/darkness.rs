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

/// `World.getDarkness` border component (`rules.borderDarkness`; callers take
/// the max with [`get_static_darkness`]): with `border`, the distance to the
/// play-area edge (or the map edge when no limit is set) fades in over
/// `edgeBlend = 2` tiles to full black at `edgeDst <= 0`
/// (`World.java:484-503`).
pub fn border_darkness(
    grid: &WorldGrid,
    border: bool,
    limit: Option<(i32, i32, i32, i32)>,
    x: i32,
    y: i32,
) -> f32 {
    if !border {
        return 0.0;
    }
    const EDGE_BLEND: i32 = 2;
    let edge_dst = match limit {
        Some((lx, ly, lw, lh)) => (x - lx)
            .min(y - ly)
            .min(-(x - (lx + lw - 1)))
            .min(-(y - (ly + lh - 1))),
        None => x
            .min(y)
            .min(-(x - (grid.tiles.width - 1)))
            .min(-(y - (grid.tiles.height - 1))),
    };
    if edge_dst <= EDGE_BLEND {
        (EDGE_BLEND - edge_dst) as f32 * (4.0 / EDGE_BLEND as f32)
    } else {
        0.0
    }
}

/// `World.getDarkness` generated-sector polygon term (`World.java:505-529`):
/// dark when the tile lies outside the projected sector polygon (hexagon or
/// pentagon), with the Arc noise ragged edge. `grid.sector_polygon == None`
/// (presets/saves) returns 0.
pub fn sector_polygon_darkness(grid: &WorldGrid, x: i32, y: i32) -> f32 {
    let Some(polygon) = grid.sector_polygon else {
        return 0.0;
    };
    if polygon.sides <= 0 {
        return 0.0;
    }
    const CIRCLE_BLEND: f32 = 5.0;
    let width = grid.tiles.width;
    let height = grid.tiles.height;
    // `state.getSector().rect.rotation + 90`.
    let offset = polygon.rotation + 90.0;
    // `Angles.angle(x, y, tiles.width/2, tiles.height/2) + offset`.
    // Deviation: Arc `Mathf.atan2` is a polynomial approximation; the port
    // uses libm (`port convention`, plan 06 goldens are Rust-recorded).
    let ang = (height / 2 - y) as f32;
    let ang2 = (width / 2 - x) as f32;
    let mut angle = ang.atan2(ang2) * (180.0f32 / std::f32::consts::PI);
    if angle < 0.0 {
        angle += 360.0;
    }
    let angle = angle + offset;
    // Polygon sides and quantized angles.
    let sides = polygon.sides;
    let step = 360.0 / sides as f32;
    // `Mathf.round(value, step) = (int)(value / step) * step`.
    let prev = ((angle / step) as i32) as f32 * step;
    let next = prev + step;
    // Raw line length to be translated.
    let length = polygon.size as f32 / 2.0;
    // `Tmp.v1.trns(prev, length)` / `Tmp.v2.trns(next, length)`.
    let (sx, sy) = (length * deg_cos(prev), length * deg_sin(prev));
    let (ex, ey) = (length * deg_cos(next), length * deg_sin(next));
    // `Tmp.v3.set(x - tiles.width/2, y - tiles.height/2).rotate(offset)`.
    let rx = (x - width / 2) as f32;
    let ry = (y - height / 2) as f32;
    let cos = deg_cos(offset);
    let sin = deg_sin(offset);
    let px = rx * cos - ry * sin;
    let py = rx * sin + ry * cos;

    // `Intersector.distanceLinePoint(sx, sy, ex, ey, px, py)`.
    let normal_length = ((ex - sx) * (ex - sx) + (ey - sy) * (ey - sy)).sqrt();
    let dist = if normal_length == 0.0 {
        0.0
    } else {
        ((px - sx) * (ey - sy) - (py - sy) * (ex - sx)).abs() / normal_length
    };
    let mut raw_dst = dist / 1.732_050_8 - 1.0;
    // `rawDst += Noise.noise(x, y, 11f, 7f) + Noise.noise(x, y, 22f, 15f)`.
    raw_dst += crate::math::arc_noise::noise(x as f32, y as f32, 11.0, 7.0);
    raw_dst += crate::math::arc_noise::noise(x as f32, y as f32, 22.0, 15.0);
    let circle_dst = (raw_dst - (length - CIRCLE_BLEND)) as i32;
    if circle_dst > 0 {
        circle_dst as f32
    } else {
        0.0
    }
}

/// `Vec2.trns`/`Vec2.rotate` degree trig (`Vec2.java:456-458`); port convention
/// is libm.
fn deg_cos(degrees: f32) -> f32 {
    degrees.to_radians().cos()
}

fn deg_sin(degrees: f32) -> f32 {
    degrees.to_radians().sin()
}

/// `World.getDarkness` (`World.java:484-536`): the max of the border term, the
/// generated-sector polygon term and the static-wall term, in Java order.
pub fn get_darkness(grid: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) -> f32 {
    let mut dark = border_darkness(grid, grid.border_darkness, grid.limit_area, x, y);
    dark = dark.max(sector_polygon_darkness(grid, x, y));
    dark = dark.max(get_static_darkness(grid, content, x, y));
    dark
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    /// Border darkness ramps 4 at the edge to 0 two tiles inside, for both the
    /// map-bounds and the `limit_area` variants (`World.java:489-501`).
    #[test]
    fn border_darkness_ramps_at_the_edge() {
        let grid = WorldGrid::new(10, 10);
        // Map bounds: x=0 -> 4, x=1 -> 2, x=2 -> 0.
        assert_eq!(border_darkness(&grid, true, None, 0, 5), 4.0);
        assert_eq!(border_darkness(&grid, true, None, 1, 5), 2.0);
        assert_eq!(border_darkness(&grid, true, None, 2, 5), 0.0);
        // Off the map clamps further out (still >= 4).
        assert_eq!(border_darkness(&grid, true, None, -1, 5), 6.0);
        // Border disabled.
        assert_eq!(border_darkness(&grid, false, None, 0, 5), 0.0);
    }

    #[test]
    fn border_darkness_uses_the_limit_rect() {
        let grid = WorldGrid::new(16, 16);
        let limit = Some((4, 4, 8, 8));
        // Inside the limit rect, away from its edge.
        assert_eq!(border_darkness(&grid, true, limit, 8, 8), 0.0);
        // On the limit's left edge (x = 4) and two tiles in.
        assert_eq!(border_darkness(&grid, true, limit, 4, 8), 4.0);
        assert_eq!(border_darkness(&grid, true, limit, 6, 8), 0.0);
        // Outside the limit (x = 3) is fully black (>= 4).
        assert_eq!(border_darkness(&grid, true, limit, 3, 8), 6.0);
        // Right/top edges of the rect (x = 11 = 4+8-1).
        assert_eq!(border_darkness(&grid, true, limit, 11, 8), 4.0);
        assert_eq!(border_darkness(&grid, true, limit, 8, 11), 4.0);
    }

    #[test]
    fn darkness_bfs_marks_fully_enclosed_walls() {
        let content = crate::content::test_support::test_registry();
        // Find a static, solid, floor-filling block (`Tile.staticDarkness`
        // after the plan-02 `fillsTile` reconciliation).
        let Some(block) = content
            .blocks()
            .iter()
            .find(|def| def.solid && def.fills_tile && crate::world::tile::is_static_kind(def.kind))
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

    /// Generated-sector polygon darkness (`World.java:505-529`): the centre of
    /// a 300-tile hexagon is clear, the map corner is dark (>= 3).
    #[test]
    fn sector_polygon_dark_outside() {
        let mut grid = WorldGrid::new(300, 300);
        grid.sector_polygon = Some(crate::world::SectorPolygon {
            rotation: 0.0,
            sides: 6,
            size: 300,
        });
        assert_eq!(sector_polygon_darkness(&grid, 150, 150), 0.0);
        assert!(sector_polygon_darkness(&grid, 0, 0) >= 3.0);
    }
}
