// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bloom capture/render band math (`core/Renderer.java:391`, plan 16 §3.9/M5).
//!
//! `Renderer.draw` queues `bloom.capture` at `Layer.bullet - 0.02` and
//! `bloom.render` at `Layer.effect + 0.02`, with intensity
//! `settings.bloomintensity / 4 + 1` and `BlurPasses = settings.bloomblur`.
//! Godot hosts the capture `SubViewport`; this module owns the deterministic
//! band keys and settings math.

use crate::render::layer::Layer;

/// The capture offset below `Layer.bullet` (`0.02`).
pub const CAPTURE_OFFSET: f32 = 0.02;
/// The render offset above `Layer.effect` (`0.02`).
pub const RENDER_OFFSET: f32 = 0.02;
/// Bloom threshold (upstream `Bloom` default, plan 16 §3.9).
pub const THRESHOLD: f32 = 0.8;

/// `Layer.bullet - 0.02` capture z.
pub fn capture_z() -> f32 {
    Layer::Bullet.z() - CAPTURE_OFFSET
}

/// `Layer.effect + 0.02` render z.
pub fn render_z() -> f32 {
    Layer::Effect.z() + RENDER_OFFSET
}

/// `bloom.setBloomIntensity(settings.bloomintensity / 4f + 1f)`.
pub fn intensity(settings: i32) -> f32 {
    settings as f32 / 4.0 + 1.0
}

/// `bloom.blurPasses = settings.bloomblur` (default `1`).
pub fn blur_passes(settings: i32) -> i32 {
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_offsets_match_renderer() {
        assert!((capture_z() - 99.98).abs() < 1e-4);
        assert!((render_z() - 110.02).abs() < 1e-4);
    }

    #[test]
    fn intensity_and_blur_settings() {
        assert_eq!(intensity(6), 2.5);
        assert_eq!(blur_passes(1), 1);
    }
}
