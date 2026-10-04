// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MenuRenderer` host (`graphics/MenuRenderer.java`, plan 16 §3.10/M7).
//!
//! Owns the procedurally generated menu world and the flyer animation clock.
//! Deviation S16-12: the menu world's floor/wall caches are plan-16 chunk
//! pipeline bakes and the flyer `UnitType` draws need plans 02/11/17; this host
//! generates the deterministic terrain (`--menu-seed`) and exposes the tile/
//! flyer counts plus the flyer positions so the background host can draw them.

use mind_core::render::menu::{MenuWorld, generate};

/// `MenuRenderer` host state.
#[derive(Clone, Debug, Default)]
pub struct MenuRenderer {
    world: Option<MenuWorld>,
    time: f32,
    flyer_rot: f32,
}

impl MenuRenderer {
    /// An ungenerated menu renderer.
    pub fn new() -> Self {
        Self {
            world: None,
            time: 0.0,
            flyer_rot: 45.0,
        }
    }

    /// `MenuRenderer.generate` pinned by `seed`.
    pub fn generate(&mut self, seed: i32, mobile: bool) {
        self.world = Some(generate(seed, mobile));
        self.time = 0.0;
        self.flyer_rot = 45.0;
    }

    /// Whether the world has been generated.
    pub fn is_ready(&self) -> bool {
        self.world.is_some()
    }

    /// The generated menu world (`None` before `generate`).
    pub fn world(&self) -> Option<&MenuWorld> {
        self.world.as_ref()
    }

    /// Tile count.
    pub fn tile_count(&self) -> usize {
        self.world.as_ref().map(|w| w.tiles.len()).unwrap_or(0)
    }

    /// Flyer count.
    pub fn flyers(&self) -> usize {
        self.world.as_ref().map(|w| w.flyers).unwrap_or(0)
    }

    /// Advances the animation clock (`MenuRenderer.render` `time += delta`).
    pub fn update(&mut self, delta: f32) {
        self.time += delta;
    }
}
