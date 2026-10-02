// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Godot-free `g3d` data (plan 16 §3.1/M7): the `PlanetGrid` icosphere sector
//! grid and the `MeshBuilder` vertex/index counts. The Godot-facing mesh
//! construction lives in `mind-gdext::render::g3d`.

pub mod grid;
pub mod mesh_data;
pub mod params;

pub use grid::{Corner, Edge, PlanetGrid, Ptile};
pub use mesh_data::{MeshData, build_planet_grid, hex_is_indexed};
pub use params::PlanetParams;
