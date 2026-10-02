// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Precomputed multiblock edge offsets and pixel polygons
//! (`world/Edges.java`, plan 06 §3.7).
//!
//! Ported exactly: the `bot`/`top` construction, the `Mathf.angle` sort and the
//! inside-edge clamp. `ApplicationTests.edges` is the oracle.

use std::sync::OnceLock;

use crate::constants::MAX_BLOCK_SIZE;
use crate::world::TilePos;

/// Edge radius cap (`Edges.maxRadius`).
pub const MAX_RADIUS: usize = 12;

/// A precomputed edge table (`edges`, `edge_inside`, `polygons`).
pub struct Edges;

struct Tables {
    edges: Vec<Vec<TilePos>>,
    edge_inside: Vec<Vec<TilePos>>,
    polygons: Vec<Vec<[f32; 2]>>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(build_tables)
}

/// Arc `Mathf.angle(x, y)`: `atan2(y, x)` in degrees, normalized to `[0, 360)`.
pub fn angle(x: f32, y: f32) -> f32 {
    let mut degrees = y.atan2(x).to_degrees();
    if degrees < 0.0 {
        degrees += 360.0;
    }
    degrees
}

fn build_tables() -> Tables {
    let max_block = MAX_BLOCK_SIZE as usize;
    let mut edges = Vec::with_capacity(max_block);
    let mut edge_inside = Vec::with_capacity(max_block);

    for i in 0..max_block {
        let bot = -(i as f32 / 2.0) as i32 - 1;
        let top = (i as f32 / 2.0 + 0.5) as i32 + 1;
        let mut row = Vec::with_capacity((i + 1) * 4);
        for j in 0..i + 1 {
            let j = j as i32;
            // bottom, top, left, right (upstream order).
            row.push(TilePos::new((bot + 1 + j) as i16, bot as i16));
            row.push(TilePos::new((bot + 1 + j) as i16, top as i16));
            row.push(TilePos::new(bot as i16, (bot + j + 1) as i16));
            row.push(TilePos::new(top as i16, (bot + j + 1) as i16));
        }
        row.sort_by(|a, b| {
            angle(a.x() as f32, a.y() as f32).total_cmp(&angle(b.x() as f32, b.y() as f32))
        });

        let lower = -((i as f32 / 2.0) as i32);
        let upper = (i as f32 / 2.0 + 0.5) as i32;
        let inside: Vec<TilePos> = row
            .iter()
            .map(|point| {
                TilePos::new(
                    point.x().clamp(lower as i16, upper as i16),
                    point.y().clamp(lower as i16, upper as i16),
                )
            })
            .collect();
        edges.push(row);
        edge_inside.push(inside);
    }

    let polygons = (0..MAX_RADIUS * 2)
        .map(|i| pixel_circle((i + 1) as f32 / 2.0))
        .collect();

    Tables {
        edges,
        edge_inside,
        polygons,
    }
}

/// `Edges.getEdges(size)`; `size` is 1-based (`size == 0` is invalid).
pub fn edges(size: i32) -> &'static [TilePos] {
    assert!(
        (1..=MAX_BLOCK_SIZE).contains(&size),
        "Block size must be between 0 and {MAX_BLOCK_SIZE}"
    );
    &tables().edges[(size - 1) as usize]
}

/// `Edges.getInsideEdges(size)`.
pub fn inside_edges(size: i32) -> &'static [TilePos] {
    assert!(
        (1..=MAX_BLOCK_SIZE).contains(&size),
        "Block size must be between 0 and {MAX_BLOCK_SIZE}"
    );
    &tables().edge_inside[(size - 1) as usize]
}

/// `Edges.getPixelPolygon(radius)` (`radius` in `1..=12`).
pub fn pixel_polygon(radius: f32) -> &'static [[f32; 2]] {
    assert!(
        (1.0..=MAX_RADIUS as f32).contains(&radius),
        "Polygon size must be between 1 and {MAX_RADIUS}"
    );
    &tables().polygons[(radius * 2.0) as usize - 1]
}

/// The tile of a multiblock facing another tile (`Edges.getFacingEdge`).
pub fn facing_edge(block_size: i32, tilex: i32, tiley: i32, other: TilePos) -> TilePos {
    if block_size <= 1 {
        return TilePos::new(tilex as i16, tiley as i16);
    }
    let size = block_size;
    let cx = (other.x() as i32 - tilex).clamp(-(size - 1) / 2, size / 2);
    let cy = (other.y() as i32 - tiley).clamp(-(size - 1) / 2, size / 2);
    TilePos::new((tilex + cx) as i16, (tiley + cy) as i16)
}

/// `Geometry.pixelCircle` (Arc): the discrete pixel outline of a circle.
fn pixel_circle(radius: f32) -> Vec<[f32; 2]> {
    // Upstream samples every pixel square on the boundary via angle buckets.
    let d = radius * 2.0;
    let mut points = Vec::new();
    let sides = (d * 2.0).ceil() as i32;
    for i in 0..sides.max(1) {
        let theta = i as f32 / sides.max(1) as f32 * std::f32::consts::TAU;
        let px = radius * theta.cos();
        let py = radius * theta.sin();
        points.push([px.floor() + 0.5, py.floor() + 0.5]);
    }
    points.dedup();
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `ApplicationTests.edges`.
    #[test]
    fn edge_order() {
        let e1 = edges(1);
        assert_eq!(e1[0], TilePos::new(1, 0));
        assert_eq!(e1[1], TilePos::new(0, 1));
        assert_eq!(e1[2], TilePos::new(-1, 0));
        assert_eq!(e1[3], TilePos::new(0, -1));

        let e2 = edges(2);
        assert_eq!(e2.len(), 8);
    }

    #[test]
    fn inside_edges_are_clamped() {
        for size in 1..=MAX_BLOCK_SIZE {
            let inside = inside_edges(size);
            let lower = -((size - 1) / 2);
            let upper = size / 2;
            for point in inside {
                assert!(point.x() as i32 >= lower && point.x() as i32 <= upper);
                assert!(point.y() as i32 >= lower && point.y() as i32 <= upper);
            }
        }
    }
}
