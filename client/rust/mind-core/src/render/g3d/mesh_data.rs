// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-free `MeshBuilder` data (`graphics/g3d/MeshBuilder.java`, plan 16
//! §3.1/M7). Produces vertex/index arrays that `mg::g3d::mesh` uploads into an
//! `ArrayMesh`; the exact `tiles*6 < 65535` indexed threshold is preserved.

use super::grid::PlanetGrid;

/// One built mesh (positions + per-vertex RGBA8888 colors + optional indices).
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    /// Vertex positions.
    pub vertices: Vec<[f32; 3]>,
    /// Per-vertex RGBA8888 colors.
    pub colors: Vec<u32>,
    /// Triangle indices (empty when non-indexed).
    pub indices: Vec<u32>,
    /// Whether indices are used.
    pub indexed: bool,
}

/// `MeshBuilder.buildHex` indexed threshold: `tiles * 6 < 65535`.
pub fn hex_is_indexed(tiles: usize) -> bool {
    tiles * 6 < 65535
}

/// `MeshBuilder.buildPlanetGrid`: two vertices per tile edge (no indices). The
/// upstream draw primitive is a line list; the caller supplies the primitive.
pub fn build_planet_grid(grid: &PlanetGrid, color: u32, scale: f32) -> MeshData {
    let mut vertices = Vec::with_capacity(grid.tiles.len() * 12);
    let mut colors = Vec::with_capacity(grid.tiles.len() * 12);
    for tile in &grid.tiles {
        let count = tile.corners.len();
        for i in 0..count {
            let v1 = grid.corners[tile.corners[i]].v;
            let v2 = grid.corners[tile.corners[(i + 1) % count]].v;
            vertices.push([v1[0] * scale, v1[1] * scale, v1[2] * scale]);
            colors.push(color);
            vertices.push([v2[0] * scale, v2[1] * scale, v2[2] * scale]);
            colors.push(color);
        }
    }
    MeshData {
        vertices,
        colors,
        indices: Vec::new(),
        indexed: false,
    }
}

/// Capacity plan for `MeshBuilder.buildHex` (used by the Godot mesh upload):
/// `(indexed, vertex_capacity, index_capacity)`.
pub fn hex_build_plan(tiles: usize) -> (bool, usize, usize) {
    let indexed = hex_is_indexed(tiles);
    if indexed {
        (true, tiles * 6, tiles * 4 * 3)
    } else {
        (false, tiles * 12, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::g3d::grid::PlanetGrid;

    #[test]
    fn indexed_threshold() {
        // 65535 / 6 = 10922.5.
        assert!(hex_is_indexed(10922));
        assert!(!hex_is_indexed(10923));
        let (indexed, verts, indices) = hex_build_plan(12);
        assert!(indexed);
        assert_eq!(verts, 72);
        assert_eq!(indices, 144);
    }

    #[test]
    fn planet_grid_mesh_vertex_count() {
        let grid = PlanetGrid::create(1);
        let mesh = build_planet_grid(&grid, 0xffff_ffff, 1.0);
        // Two vertices per tile corner; the 12 originals have 5 corners each,
        // the 20 former corners have 6 => 180 corners total, 360 vertices.
        let corners: usize = grid.tiles.iter().map(|t| t.corners.len()).sum();
        assert_eq!(corners, 180);
        assert_eq!(mesh.vertices.len(), corners * 2);
        assert_eq!(mesh.colors.len(), mesh.vertices.len());
        assert!(!mesh.indexed);
    }
}
