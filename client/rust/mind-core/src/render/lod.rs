// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Level-of-detail gating (`graphics/Lod.java`, plan 16 §3.8).
//!
//! View-only: `l1`/`l2` gate expensive per-block text/bars; `alpha1`/`alpha2`
//! fade them. `disable` forces full detail (map screenshots).

/// `Lod` state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lod {
    /// `Lod.disable`.
    pub disable: bool,
    /// `Lod.l1`.
    pub l1: bool,
    /// `Lod.alpha1`.
    pub alpha1: f32,
    /// `Lod.l2`.
    pub l2: bool,
    /// `Lod.alpha2`.
    pub alpha2: f32,
}

impl Default for Lod {
    fn default() -> Self {
        Self {
            disable: false,
            l1: true,
            alpha1: 1.0,
            l2: true,
            alpha2: 1.0,
        }
    }
}

/// `Lod.threshold1`.
pub const THRESHOLD1: f32 = 1.4;
/// `Lod.threshold2`.
pub const THRESHOLD2: f32 = 0.8;
/// `Lod.fade`.
pub const FADE: f32 = 0.2;

impl Lod {
    /// `Lod.update()`: `scale = screen_width / camera_width`.
    pub fn update(&mut self, screen_width: f32, camera_width: f32) {
        if self.disable {
            self.l1 = true;
            self.l2 = true;
            self.alpha1 = 1.0;
            self.alpha2 = 1.0;
            return;
        }
        let scale = if camera_width != 0.0 {
            screen_width / camera_width
        } else {
            0.0
        };
        self.alpha1 = clamp01((scale - THRESHOLD1) / FADE);
        self.alpha2 = clamp01((scale - THRESHOLD2) / FADE);
        self.l1 = self.alpha1 >= 1.0 / 255.0;
        self.l2 = self.alpha2 >= 1.0 / 255.0;
    }
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_thresholds() {
        let mut lod = Lod::default();
        // camera width 100, screen 100 => scale 1.0: alpha1 = 0, alpha2 = 1.
        lod.update(100.0, 100.0);
        assert!(lod.alpha1 < 1.0 / 255.0);
        assert!((lod.alpha2 - 1.0).abs() < f32::EPSILON);
        assert!(!lod.l1);
        assert!(lod.l2);

        // scale 1.6 => alpha1 = clamp(0.2/0.2) = 1, alpha2 = 1.
        lod.update(160.0, 100.0);
        assert!((lod.alpha1 - 1.0).abs() < f32::EPSILON);
        assert!((lod.alpha2 - 1.0).abs() < f32::EPSILON);
        assert!(lod.l1 && lod.l2);

        // scale 0.5 => both alphas clamp to 0.
        lod.update(50.0, 100.0);
        assert!(lod.alpha1 < 1.0 / 255.0);
        assert!(lod.alpha2 < 1.0 / 255.0);
        assert!(!lod.l1 && !lod.l2);
    }

    #[test]
    fn disable_forces_full_detail() {
        let mut lod = Lod {
            disable: true,
            alpha1: 0.0,
            alpha2: 0.0,
            l1: false,
            l2: false,
        };
        lod.update(1.0, 1000.0);
        assert!(lod.l1 && lod.l2);
        assert_eq!(lod.alpha1, 1.0);
        assert_eq!(lod.alpha2, 1.0);
    }
}
