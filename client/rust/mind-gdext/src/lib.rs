// SPDX-License-Identifier: GPL-3.0-only

//! `mind-gdext` — the Godot 4.7 GDExtension cdylib.
//!
//! Owns view drivers, input, the fixed-step frame pump and autoloads. Contains no
//! game rules (D1/HLP §2.2). The M4 spine classes are `MindSimHost`
//! (sim owner + fixed 60 Hz pump + input/API), `MindCamera2D` (pan/zoom,
//! screen↔tile) and `MindTileGrid` (`_draw` grid/quads) per plan §3.5.

mod camera;
mod hello;
mod log_bridge;
mod settings;
mod sim_host;
mod tile_grid;

pub use camera::MindCamera2D;
pub use sim_host::MindSimHost;
pub use tile_grid::MindTileGrid;

use godot::prelude::*;

/// Version of the `mind-gdext` crate, taken from `Cargo.toml`.
pub const MIND_VERSION: &str = env!("CARGO_PKG_VERSION");

struct MindExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MindExtension {
    fn on_stage_init(stage: InitStage) {
        // Scene is this library's default `min_level()` and runs before the main
        // scene is instantiated, so the bridge is live for the first `_ready`.
        if stage == InitStage::Scene {
            log_bridge::install();
        }
    }
}
