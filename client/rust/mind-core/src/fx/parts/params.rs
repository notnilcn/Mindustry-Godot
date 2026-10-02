// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawPart.PartParams` / `PartMove` (plan 17 §3.7).
//!
//! Ported verbatim from `entities/part/DrawPart.java:24-71`.

/// Draw parameters shared by all part types.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PartParams {
    /// Weapon warmup `0..1`.
    pub warmup: f32,
    /// Weapon reload (`1` right after shooting, `0` ready).
    pub reload: f32,
    /// Smoothed reload.
    pub smooth_reload: f32,
    /// Weapon heat `0..1`.
    pub heat: f32,
    /// Weapon recoil, no curve applied.
    pub recoil: f32,
    /// Lifetime fraction `0..1` (missiles).
    pub life: f32,
    /// Weapon charge `0..1`.
    pub charge: f32,
    /// Current part x.
    pub x: f32,
    /// Current part y.
    pub y: f32,
    /// Current part rotation (degrees).
    pub rotation: f32,
    /// Overrides the part side (`-1` = auto).
    pub side_override: i32,
    /// Side sign multiplier (`1` default).
    pub side_multiplier: i32,
}

impl PartParams {
    /// `PartParams.set(...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn set(
        &mut self,
        warmup: f32,
        reload: f32,
        smooth_reload: f32,
        heat: f32,
        recoil: f32,
        charge: f32,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> &mut Self {
        self.warmup = warmup;
        self.reload = reload;
        self.heat = heat;
        self.recoil = recoil;
        self.smooth_reload = smooth_reload;
        self.charge = charge;
        self.x = x;
        self.y = y;
        self.rotation = rotation;
        self.side_override = -1;
        self.life = 0.0;
        self.side_multiplier = 1;
        self
    }

    /// `PartParams.setRecoil(float)`.
    pub fn set_recoil(&mut self, recoils: f32) -> &mut Self {
        self.recoil = recoils;
        self
    }
}

/// A move transform driven by a progress spec (`DrawPart.PartMove`).
#[derive(Clone, Debug, PartialEq)]
pub struct PartMove {
    /// Progress source.
    pub progress: super::progress::PartProgressSpec,
    /// Move x.
    pub x: f32,
    /// Move y.
    pub y: f32,
    /// Grow x.
    pub grow_x: f32,
    /// Grow y.
    pub grow_y: f32,
    /// Rotate.
    pub rot: f32,
}

impl Default for PartMove {
    fn default() -> Self {
        Self {
            progress: super::progress::PartProgressSpec::Warmup,
            x: 0.0,
            y: 0.0,
            grow_x: 0.0,
            grow_y: 0.0,
            rot: 0.0,
        }
    }
}

impl PartMove {
    /// `PartMove(progress, x, y, rot)`.
    pub fn new(progress: super::progress::PartProgressSpec, x: f32, y: f32, rot: f32) -> Self {
        Self {
            progress,
            x,
            y,
            rot,
            ..Default::default()
        }
    }

    /// `PartMove(progress, x, y, gx, gy, rot)`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_grow(
        progress: super::progress::PartProgressSpec,
        x: f32,
        y: f32,
        grow_x: f32,
        grow_y: f32,
        rot: f32,
    ) -> Self {
        Self {
            progress,
            x,
            y,
            grow_x,
            grow_y,
            rot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_resets_side_and_life() {
        let mut p = PartParams {
            side_override: 3,
            side_multiplier: -1,
            life: 0.5,
            ..Default::default()
        };
        p.set(1.0, 0.2, 0.3, 0.4, 0.5, 0.6, 7.0, 8.0, 90.0);
        assert_eq!(p.side_override, -1);
        assert_eq!(p.side_multiplier, 1);
        assert_eq!(p.life, 0.0);
        assert_eq!((p.x, p.y, p.rotation), (7.0, 8.0, 90.0));
    }
}
