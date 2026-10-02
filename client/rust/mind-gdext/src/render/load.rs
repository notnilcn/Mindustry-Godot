// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LoadRenderer` host (`graphics/LoadRenderer.java`, plan 16 §3.10/M7).
//!
//! Owns the loading-screen data: the rotating `PlanetGrid.create(2)` mesh
//! topology, the `colorRed` tint (`Pal.breakInvalid.lerp(black, 0.3)`), and the
//! backend version string. Deviation S16-11: the `Bar` layout is plan 14's
//! (plan 16 §2.3); this host exposes the mesh/version data plan 14 binds and the
//! `loading.tscn` host can render.

use mind_core::render::drawf::pal;
use mind_core::render::g3d::PlanetGrid;
use mind_core::render::g3d::mesh_data::build_planet_grid;

/// `LoadRenderer` host state.
#[derive(Clone, Debug)]
pub struct LoadRenderer {
    grid_size: usize,
    grid: PlanetGrid,
    color_red: u32,
}

impl Default for LoadRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadRenderer {
    /// Builds the `PlanetGrid.create(2)` mesh and tint.
    pub fn new() -> Self {
        // `Pal.breakInvalid.lerp(black, 0.3)`.
        let red = mind_core::render::drawf::lerp(pal::BREAK_INVALID, [0.0, 0.0, 0.0, 1.0], 0.3);
        Self {
            grid_size: 2,
            grid: PlanetGrid::create(2),
            color_red: mind_core::render::drawf::to_bits(red),
        }
    }

    /// The subdivision level (`LoadRenderer` uses `PlanetGrid.create(2)`).
    pub fn grid_size(&self) -> usize {
        self.grid_size
    }

    /// The `colorRed` tint used by `MeshBuilder.buildPlanetGrid`.
    pub fn color_red(&self) -> u32 {
        self.color_red
    }

    /// The loading-grid vertex count (for the mesh upload / stats).
    pub fn mesh_vertex_count(&self) -> usize {
        build_planet_grid(&self.grid, self.color_red, 1.0)
            .vertices
            .len()
    }
}
