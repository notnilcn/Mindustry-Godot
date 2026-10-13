// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightRenderer` view data (`graphics/LightRenderer.java`, plan 16 §3.7 /
//! M5). Godot owns the downscaled target and the additive composite; this
//! module owns the deterministic circle-light accumulation, the `enabled()`
//! gate and the 4× downscale constant. View-only.

/// `LightRenderer.scaling`.
pub const SCALING: i32 = 4;

/// One pooled circle light (`LightRenderer.CircleLight`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CircleLight {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// RGBA (premultiplied not required; composited with `light`).
    pub color: [f32; 4],
    /// Radius in world pixels.
    pub radius: f32,
}

/// `LightRenderer.enabled()`: lighting on, ambient light visible and the
/// renderer's `drawLight` toggle.
pub fn enabled(lighting: bool, ambient_alpha: f32, draw_light: bool) -> bool {
    lighting && ambient_alpha > 0.0001 && draw_light
}

/// `light.frag` composite (`core/assets/shaders/light.frag`): the accumulated
/// circle buffer is mixed into the ambient by its alpha and the result's alpha
/// is `ambient.a - light.a` (the light *adds* brightness).
pub fn light_composite(ambient: [f32; 4], light: [f32; 4]) -> [f32; 4] {
    let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
    [
        mix(ambient[0], light[0], light[3]).clamp(0.0, 1.0),
        mix(ambient[1], light[1], light[3]).clamp(0.0, 1.0),
        mix(ambient[2], light[2], light[3]).clamp(0.0, 1.0),
        (ambient[3] - light[3]).clamp(0.0, 1.0),
    ]
}

/// `LightRenderer` circle pool: reuses slots instead of allocating per add.
#[derive(Clone, Debug, Default)]
pub struct LightAccumulator {
    circles: Vec<CircleLight>,
    circle_index: usize,
}

impl LightAccumulator {
    /// A fresh accumulator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties the pool for the next frame (keeps capacity).
    pub fn clear(&mut self) {
        self.circle_index = 0;
    }

    /// `LightRenderer.add(x, y, radius, color, opacity)`. Zero/negative radii
    /// are ignored, matching upstream.
    pub fn add(&mut self, x: f32, y: f32, radius: f32, color: [f32; 4], opacity: f32) {
        if radius <= 0.0 {
            return;
        }
        let light = CircleLight {
            x,
            y,
            color: [color[0], color[1], color[2], opacity],
            radius,
        };
        if self.circle_index < self.circles.len() {
            self.circles[self.circle_index] = light;
        } else {
            self.circles.push(light);
        }
        self.circle_index += 1;
    }

    /// The live circle lights this frame.
    pub fn circles(&self) -> &[CircleLight] {
        &self.circles[..self.circle_index.min(self.circles.len())]
    }

    /// Number of live lights.
    pub fn len(&self) -> usize {
        self.circle_index
    }

    /// Whether no lights are queued.
    pub fn is_empty(&self) -> bool {
        self.circle_index == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_gate_matches_upstream() {
        assert!(enabled(true, 0.01, true));
        assert!(!enabled(false, 0.01, true));
        assert!(!enabled(true, 0.0, true));
        assert!(!enabled(true, 0.01, false));
    }

    #[test]
    fn ambient_mix_matches_shader() {
        let out = light_composite([0.01, 0.01, 0.04, 0.99], [1.0, 0.83, 0.5, 0.6]);
        assert!((out[0] - 0.604).abs() < 1e-5);
        assert!((out[1] - 0.502).abs() < 1e-5);
        assert!((out[2] - 0.316).abs() < 1e-5);
        assert!((out[3] - 0.39).abs() < 1e-5);
    }

    #[test]
    fn accumulator_pools_and_resets() {
        let mut acc = LightAccumulator::new();
        acc.add(1.0, 2.0, 3.0, [1.0, 1.0, 1.0, 1.0], 0.5);
        acc.add(4.0, 5.0, 0.0, [1.0, 1.0, 1.0, 1.0], 0.5);
        assert_eq!(acc.len(), 1, "zero radius ignored");
        acc.add(6.0, 7.0, 8.0, [0.0, 0.0, 1.0, 1.0], 0.25);
        assert_eq!(acc.len(), 2);
        acc.clear();
        assert!(acc.is_empty());
        // Reuse keeps the same capacity (no allocation observed by the pool).
        acc.add(9.0, 9.0, 1.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        assert_eq!(acc.circles().len(), 1);
        assert_eq!(acc.circles()[0].x, 9.0);
    }
}
