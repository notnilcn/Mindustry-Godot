// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PlanetRenderer` host (`graphics/g3d/PlanetRenderer.java`, plan 16 §3.10/M7).
//!
//! Owns the `PlanetParams` view state and the lazily built `PlanetGrid` sector
//! topology + mesh vertex plan. Deviation S16-10: the actual `Camera3D`,
//! cubemap skybox, planet/cloud/atmosphere/planetgrid spatial shaders, orbit
//! rings and sector overlays are Godot-3D work verified through the deferred
//! MCP planet screenshot; this host exposes the deterministic data
//! (`sector_counts`, `mesh_vertex_count`) and the camera params so plan 14's
//! `PlanetDialog` can bind them.

use std::collections::HashMap;

use mind_core::render::g3d::mesh_data::build_planet_grid;
use mind_core::render::g3d::{PlanetGrid, PlanetParams};

/// `PlanetRenderer` host state.
#[derive(Clone, Debug)]
pub struct PlanetRenderer {
    params: PlanetParams,
    grids: HashMap<usize, PlanetGrid>,
}

impl Default for PlanetRenderer {
    fn default() -> Self {
        Self {
            params: PlanetParams::default(),
            grids: HashMap::new(),
        }
    }
}

impl PlanetRenderer {
    /// A host with default params.
    pub fn new() -> Self {
        Self::default()
    }

    /// The active params (`PlanetParams`), mutated by plan 14.
    pub fn params(&self) -> &PlanetParams {
        &self.params
    }

    /// `PlanetGrid.create(size)`, cached per size (upstream `PlanetGrid.cache`).
    pub fn grid(&mut self, size: usize) -> &PlanetGrid {
        self.grids
            .entry(size)
            .or_insert_with(|| PlanetGrid::create(size))
    }

    /// `(tiles, corners, edges)` for a subdivision level.
    pub fn sector_counts(&mut self, size: usize) -> (usize, usize, usize) {
        let grid = self.grid(size);
        (grid.tiles.len(), grid.corners.len(), grid.edges.len())
    }

    /// Vertex count of `MeshBuilder.buildPlanetGrid` for a subdivision level.
    pub fn mesh_vertex_count(&mut self, size: usize) -> usize {
        let grid = self.grid(size);
        build_planet_grid(grid, 0xffff_ffff, 1.0).vertices.len()
    }
}
