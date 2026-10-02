// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Screen-shake accumulation/decay math (plan 17 §3.11).
//!
//! Ported from `entities/Effect.shake` (distance falloff) and
//! `core/Renderer.java:85-88,205-220` (max-combine, decay, offset). The camera
//! applies the offset (plans 15/16); this module only computes it.

use crate::math::ArcRand;

/// `Effect.shakeFalloff`.
pub const SHAKE_FALLOFF: f32 = 10000.0;

/// `Mathf.clamp(1f / (distance * distance / shakeFalloff))` for `distance >= 1`.
pub fn shake_falloff(distance: f32) -> f32 {
    let d = distance.max(1.0);
    (SHAKE_FALLOFF / (d * d)).clamp(0.0, 1.0)
}

/// `Effect.shake(intensity, duration, x, y)` → the clamped intensity.
pub fn shake_at(intensity: f32, camera_x: f32, camera_y: f32, x: f32, y: f32) -> f32 {
    let dx = camera_x - x;
    let dy = camera_y - y;
    let distance = (dx * dx + dy * dy).sqrt();
    shake_falloff(distance) * intensity
}

/// Whether `Renderer.shake` receives a non-trivial value.
pub fn shake_visible(intensity: f32, duration: f32) -> bool {
    intensity > 0.0 && duration > 0.0
}

/// Accumulated renderer shake state.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShakeState {
    /// `Renderer.shakeIntensity`.
    pub intensity: f32,
    /// `Renderer.shakeTime`.
    pub time: f32,
    /// `Renderer.shakeReduction`.
    pub reduction: f32,
    /// Current camera offset.
    pub offset: (f32, f32),
}

impl ShakeState {
    /// `Renderer.shake(intensity, duration)`.
    pub fn add(&mut self, intensity: f32, duration: f32) {
        self.intensity = self.intensity.max(intensity.clamp(0.0, 100.0));
        self.time = self.time.max(duration);
        self.reduction = self.intensity / self.time;
    }

    /// `Renderer` per-frame update + offset. `screenshake` is the `0..=4`
    /// setting (default 4); `view_tick` seeds the deterministic direction.
    pub fn offset(&mut self, screenshake: i32, view_tick: u64) -> (f32, f32) {
        if self.time > 0.0 {
            let intensity = self.intensity * (screenshake as f32 / 4.0) * 0.75;
            let mut rand = ArcRand::new(view_tick ^ 0x9e37_79b9_7f4a_7c15);
            let dir = rand.random_float(360.0);
            let mag = rand.random_float(intensity.max(0.0));
            self.offset = (dir.to_radians().cos() * mag, dir.to_radians().sin() * mag);
            self.intensity -= self.reduction;
            self.time -= 1.0;
            self.intensity = self.intensity.clamp(0.0, 100.0);
        } else {
            self.offset = (0.0, 0.0);
            self.intensity = 0.0;
        }
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_and_decay() {
        assert!((shake_falloff(1.0) - 1.0).abs() < 1e-6);
        assert!((shake_falloff(100.0) - 1.0).abs() < 1e-6);
        // 10000 / 200^2 = 0.25
        assert!((shake_falloff(200.0) - 0.25).abs() < 1e-5);
        // distance < 1 clamps to 1.
        assert!((shake_falloff(0.1) - 1.0).abs() < 1e-6);

        let mut state = ShakeState::default();
        state.add(10.0, 5.0);
        assert_eq!(state.intensity, 10.0);
        assert_eq!(state.time, 5.0);
        assert_eq!(state.reduction, 2.0);
        let _ = state.offset(4, 1);
        assert!((state.intensity - 8.0).abs() < 1e-5);
        assert!((state.time - 4.0).abs() < 1e-5);
    }

    #[test]
    fn zero_screenshake_makes_zero_offset() {
        let mut state = ShakeState::default();
        state.add(10.0, 5.0);
        let offset = state.offset(0, 1);
        assert_eq!(offset, (0.0, 0.0));
    }

    #[test]
    fn shake_at_falls_off_with_distance() {
        let near = shake_at(10.0, 0.0, 0.0, 0.0, 0.0);
        let far = shake_at(10.0, 0.0, 0.0, 1000.0, 0.0);
        assert_eq!(near, 10.0);
        assert!(far < near);
    }
}
