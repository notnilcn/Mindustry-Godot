// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Grid raycasts (`World.raycastEach*`/`raycast`, plan 06 §2.1.3).
//!
//! Exact ports of the Bresenham variants; consumed by plans 10/11/15.

use crate::config::TILESIZE;

use super::TilePos;

/// World pixel coordinate to tile coordinate (`World.toTile`; rounded).
pub fn to_tile(pixel: f32) -> i32 {
    (pixel / TILESIZE as f32).round() as i32
}

/// Visits every tile on the line, stopping when the callback returns `true`
/// (`World.raycastEach`).
pub fn raycast_each(x1: i32, y1: i32, x2: i32, y2: i32, mut visit: impl FnMut(i32, i32) -> bool) {
    let mut x = x1;
    let dx = (x2 - x).abs();
    let sx = if x < x2 { 1 } else { -1 };
    let mut y = y1;
    let dy = (y2 - y).abs();
    let sy = if y < y2 { 1 } else { -1 };
    let mut err = dx - dy;

    loop {
        if visit(x, y) {
            break;
        }
        if x == x2 && y == y2 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// No-diagonal traversal variant (`World.raycastEachNoDiagonal`).
pub fn raycast_each_no_diagonal(
    start_x: i32,
    start_y: i32,
    end_x: i32,
    end_y: i32,
    mut visit: impl FnMut(i32, i32) -> bool,
) {
    let mut x = start_x;
    let mut y = start_y;
    let x_dist = (end_x - start_x).abs();
    let y_dist = -(end_y - start_y).abs();
    let x_step = if start_x < end_x { 1 } else { -1 };
    let y_step = if start_y < end_y { 1 } else { -1 };
    let mut error = x_dist + y_dist;

    loop {
        if visit(x, y) || (x == end_x && y == end_y) {
            break;
        }
        if 2 * error - y_dist > x_dist - 2 * error {
            error += y_dist;
            x += x_step;
        } else {
            error += x_dist;
            y += y_step;
        }
    }
}

/// Like [`raycast_each`] but returns whether the callback stopped it
/// (`World.raycast`).
pub fn raycast(
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    mut visit: impl FnMut(i32, i32) -> bool,
) -> bool {
    let mut x = x1;
    let dx = (x2 - x).abs();
    let sx = if x < x2 { 1 } else { -1 };
    let mut y = y1;
    let dy = (y2 - y).abs();
    let sy = if y < y2 { 1 } else { -1 };
    let mut err = dx - dy;

    loop {
        if visit(x, y) {
            return true;
        }
        if x == x2 && y == y2 {
            return false;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

/// World-space (pixel) [`raycast_each`] (`World.raycastEachWorld`).
pub fn raycast_each_world(x0: f32, y0: f32, x1: f32, y1: f32, visit: impl FnMut(i32, i32) -> bool) {
    raycast_each(to_tile(x0), to_tile(y0), to_tile(x1), to_tile(y1), visit)
}

/// Result of the first blocking tile on a ray (`TilePos`).
pub fn raycast_first(
    grid: &super::WorldGrid,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) -> Option<TilePos> {
    let mut hit = None;
    raycast(x1, y1, x2, y2, |x, y| {
        if !grid.tiles.in_bounds(x, y) {
            return false;
        }
        if grid.tiles.get(x, y).block != crate::content::BlockId::AIR {
            hit = Some(TilePos::new(x as i16, y as i16));
            true
        } else {
            false
        }
    });
    hit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_ray_visits_all_tiles() {
        let mut visited = Vec::new();
        raycast_each(0, 0, 3, 0, |x, y| {
            visited.push((x, y));
            false
        });
        assert_eq!(visited, vec![(0, 0), (1, 0), (2, 0), (3, 0)]);
    }

    #[test]
    fn raycast_stops_at_first_hit() {
        let mut visited = Vec::new();
        let stopped = raycast(0, 0, 5, 0, |x, _y| {
            visited.push(x);
            x == 2
        });
        assert!(stopped);
        assert_eq!(visited, vec![0, 1, 2]);
    }

    #[test]
    fn no_diagonal_ray_uses_axis_steps() {
        let mut visited = Vec::new();
        raycast_each_no_diagonal(0, 0, 2, 2, |x, y| {
            visited.push((x, y));
            false
        });
        // No diagonal step: each move changes one axis at a time.
        for pair in visited.windows(2) {
            let dx = (pair[1].0 - pair[0].0).abs();
            let dy = (pair[1].1 - pair[0].1).abs();
            assert!(dx + dy == 1, "diagonal step {pair:?}");
        }
    }
}
