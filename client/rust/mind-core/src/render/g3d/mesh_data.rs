// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-free `MeshBuilder` data (`graphics/g3d/MeshBuilder.java`, plan 16
//! §3.1/M7). Produces vertex/index arrays that `mg::g3d::mesh` uploads into an
//! `ArrayMesh`; the exact `tiles*6 < 65535` indexed threshold is preserved.

use super::grid::PlanetGrid;
use crate::maps::generators::planet::HexMesher;

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
    /// Per-vertex normals (`build_hex` tiles; empty for line grids).
    pub normals: Vec<[f32; 3]>,
    /// Per-vertex emissive RGBA8888 (`CUSTOM0`; empty when not emissive).
    pub emissive: Vec<u32>,
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
        normals: Vec::new(),
        emissive: Vec::new(),
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

/// `MeshBuilder.buildHex` (`MeshBuilder.java:84-190`): a subdivided
/// icosphere with per-corner heights `(1 + getHeight(corner.v) * intensity) *
/// radius`, a flat normal per tile from corners 0/2/4, and the generator's
/// colour (plus emissive when `isEmissive()`) per tile. Tiles pass the
/// `tiles * 6 < 65535` indexed threshold; `skip(v)` tiles are dropped.
pub fn build_hex(
    mesher: &mut dyn HexMesher,
    divisions: usize,
    radius: f32,
    intensity: f32,
) -> MeshData {
    // `generator.seed = generator.baseSeed` (the mesher resets its own).
    mesher.reseed();
    let grid = PlanetGrid::create(divisions);
    let emit = mesher.is_emissive();
    let indexed = hex_is_indexed(grid.tiles.len());
    let mut heights = vec![0.0f32; grid.corners.len()];
    // Cache heights: one `getHeight` per corner (the expensive noise calls).
    for (index, corner) in grid.corners.iter().enumerate() {
        heights[index] = (1.0 + mesher.get_height(corner.v) * intensity) * radius;
    }
    let mut vertices = Vec::new();
    let mut colors = Vec::new();
    let mut normals = Vec::new();
    let mut emissive = Vec::new();
    let mut indices = Vec::new();
    let mut position: u32 = 0;
    for tile in &grid.tiles {
        if mesher.skip(tile.v) {
            continue;
        }
        let h1 = heights[tile.corners[0]];
        let h2 = heights[tile.corners[2]];
        let h3 = heights[tile.corners[4]];
        let v1 = grid.corners[tile.corners[0]].v;
        let v2 = grid.corners[tile.corners[2]].v;
        let v3 = grid.corners[tile.corners[4]].v;
        let normal = face_normal(
            [v1[0] * h1, v1[1] * h1, v1[2] * h1],
            [v2[0] * h2, v2[1] * h2, v2[2] * h2],
            [v3[0] * h3, v3[1] * h3, v3[2] * h3],
        );
        let color = crate::render::drawf::to_bits(mesher.get_color(tile.v));
        let emissive_color =
            emit.then(|| crate::render::drawf::to_bits(mesher.get_emissive_color(tile.v)));
        let mut push_vertex = |corner: usize| {
            let height = heights[corner];
            let v = grid.corners[corner].v;
            vertices.push([v[0] * height, v[1] * height, v[2] * height]);
            colors.push(color);
            normals.push(normal);
            if let Some(value) = emissive_color {
                emissive.push(value);
            }
        };
        if indexed {
            for corner in &tile.corners {
                push_vertex(*corner);
            }
            indices.extend_from_slice(&[
                position,
                position + 1,
                position + 2,
                position,
                position + 2,
                position + 3,
                position,
                position + 3,
                position + 4,
            ]);
            if tile.corners.len() > 5 {
                indices.extend_from_slice(&[position, position + 4, position + 5]);
            }
            position += tile.corners.len() as u32;
        } else {
            // Fan triangles (0,1,2), (0,2,3), (0,3,4) [+ (0,4,5)].
            let c = &tile.corners;
            let triangles: &[[usize; 3]] =
                &[[c[0], c[1], c[2]], [c[0], c[2], c[3]], [c[0], c[3], c[4]]];
            for triangle in triangles {
                for corner in triangle {
                    push_vertex(*corner);
                }
            }
            if c.len() > 5 {
                for corner in [c[0], c[4], c[5]] {
                    push_vertex(corner);
                }
            }
        }
    }
    MeshData {
        vertices,
        colors,
        indices,
        indexed,
        normals,
        emissive,
    }
}

/// `MeshBuilder.normal(v1, v2, v3)`: `(v2 - v1) x (v3 - v1)`, normalized.
fn face_normal(v1: [f32; 3], v2: [f32; 3], v3: [f32; 3]) -> [f32; 3] {
    let x = v2[0] - v1[0];
    let y = v2[1] - v1[1];
    let z = v2[2] - v1[2];
    let vx = v3[0] - v1[0];
    let vy = v3[1] - v1[1];
    let vz = v3[2] - v1[2];
    let cx = y * vz - z * vy;
    let cy = z * vx - x * vz;
    let cz = x * vy - y * vx;
    let len = (cx * cx + cy * cy + cz * cz).sqrt();
    if len <= 0.0 {
        [0.0, 0.0, 1.0]
    } else {
        [cx / len, cy / len, cz / len]
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

    /// Flat test mesher (`MeshBuilder.buildHex` vertex/index/emissive paths).
    struct FlatMesher {
        emissive: bool,
    }

    impl HexMesher for FlatMesher {
        fn get_height(&self, _position: [f32; 3]) -> f32 {
            0.0
        }

        fn get_color(&self, _position: [f32; 3]) -> [f32; 4] {
            [1.0, 0.0, 0.0, 1.0]
        }

        fn get_emissive_color(&self, _position: [f32; 3]) -> [f32; 4] {
            [0.0, 1.0, 0.0, 0.5]
        }

        fn is_emissive(&self) -> bool {
            self.emissive
        }
    }

    #[test]
    fn build_hex_counts() {
        // `divisions = 1`: 12 pentagons + 20 hexagons, 180 corners; indexed
        // (`32 * 6 < 65535`). Per tile: 5/6 vertices, 3/4 fan triangles.
        let mut mesher = FlatMesher { emissive: false };
        let mesh = build_hex(&mut mesher, 1, 1.0, 0.2);
        assert!(mesh.indexed);
        assert_eq!(mesh.vertices.len(), 180);
        assert_eq!(mesh.colors.len(), 180);
        assert_eq!(mesh.normals.len(), 180);
        assert!(
            mesh.emissive.is_empty(),
            "non-emissive mesher has no channel"
        );
        assert_eq!(mesh.indices.len(), 12 * 9 + 20 * 12);
        assert_eq!(
            mesh.colors[0],
            crate::render::drawf::to_bits([1.0, 0.0, 0.0, 1.0])
        );
        // Sphere positions stay on the `radius * (1 + height * intensity)`
        // shell for a flat height.
        for vertex in &mesh.vertices {
            let len =
                (vertex[0] * vertex[0] + vertex[1] * vertex[1] + vertex[2] * vertex[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn build_hex_emissive_channel() {
        let mut mesher = FlatMesher { emissive: true };
        let mesh = build_hex(&mut mesher, 1, 2.0, 0.2);
        assert_eq!(mesh.emissive.len(), mesh.vertices.len());
        assert_eq!(
            mesh.emissive[0],
            crate::render::drawf::to_bits([0.0, 1.0, 0.0, 0.5])
        );
        // Heights scale by `radius`.
        let len = (mesh.vertices[0][0] * mesh.vertices[0][0]
            + mesh.vertices[0][1] * mesh.vertices[0][1]
            + mesh.vertices[0][2] * mesh.vertices[0][2])
            .sqrt();
        assert!((len - 2.0).abs() < 1e-3);
    }
}
