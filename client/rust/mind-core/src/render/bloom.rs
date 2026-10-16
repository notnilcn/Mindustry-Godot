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

/// The single-pass gaussian tap radius for `blurPasses` (`J10 T2` deviation).
///
/// Java repeats `alpha_gaussian` H+V `blurPasses` times over quarter-res
/// ping-pong buffers (`Bloom.java:145-159`); the port's `bloom.gdshader` runs
/// one wider separable kernel instead. Repeated convolution grows the standard
/// deviation as `sqrt(n)`, so the tap offsets scale with `sqrt(passes)`,
/// clamped at 4x so extreme slider values (16) stay cheap.
pub fn blur_radius(passes: i32) -> f32 {
    (passes.max(1) as f32).sqrt().min(4.0)
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

    #[test]
    fn blur_radius_grows_with_sqrt_and_clamps() {
        assert_eq!(blur_radius(0), 1.0);
        assert_eq!(blur_radius(1), 1.0);
        assert_eq!(blur_radius(4), 2.0);
        assert_eq!(blur_radius(9), 3.0);
        assert_eq!(blur_radius(16), 4.0);
        assert_eq!(blur_radius(64), 4.0);
    }
}
