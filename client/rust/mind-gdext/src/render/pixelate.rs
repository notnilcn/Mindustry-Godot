// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Pixelator` host (`graphics/Pixelator.java`, plan 16 §3.8/M6).
//!
//! Owns the low-res target size and the half-pixel camera snap. Deviation
//! S16-9: the actual re-render into a low-res `SubViewport` + `screenspace`
//! blit at `Layer.end` is deferred (the `mobile` renderer's `SubViewport`
//! semantics are exercised by the compositor-testing skill); this host wires the
//! deterministic size/snap math and the enable gate so the setting is observable
//! through `get_render_stats()`.

use mind_core::render::pixelate::pixel_size;

/// Pixelator counters + derived target.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pixelator {
    enabled: bool,
    size: (i32, i32),
}

impl Pixelator {
    /// A disabled pixelator.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Pixelator.enabled`.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Sets the `pixelate` setting.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// The low-res target size (`Pixelator.drawPixelate`).
    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// Recomputes the target when enabled.
    pub fn update(
        &mut self,
        scale: f32,
        camera_w: f32,
        camera_h: f32,
        screen_w: i32,
        screen_h: i32,
        cutscene: bool,
        land_scale: f32,
    ) {
        if !self.enabled {
            return;
        }
        self.size = pixel_size(
            scale, camera_w, camera_h, screen_w, screen_h, cutscene, land_scale,
        );
    }
}
