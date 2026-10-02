// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weather particle/rain/splash visual builders (plan 17 §3.12).
//!
//! Ported from `type/Weather.java` `drawParticles`/`drawRain` and the
//! `ParticleWeather`/`RainWeather` draw halves. Weather **state** (life,
//! opacity, wind, status application) stays in plan 12; this module owns only
//! the draw math. `Time.time` is the fixed view tick (deviation #7).

use crate::content::{Rgba, WeatherId};
use crate::math::{ArcRand, curve};
use crate::render::draw::{DrawPrim, PrimKind, RegionKey};

use super::angles::trnsx;

/// `Weather.boundMax`.
pub const BOUND_MAX: f32 = 80000.0;
/// `Weather.rand.setSeed(0)` for a stable per-call field.
pub const WEATHER_SEED: u64 = 0;

/// The two vanilla weather render classes (plan 02 `WeatherDef`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    /// `ParticleWeather`.
    Particle,
    /// `RainWeather`.
    Rain,
}

/// Frozen view contract from plan 12 (plan 17 §3.12).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherStateView {
    /// Weather id.
    pub weather: WeatherId,
    /// `WeatherState.intensity`.
    pub intensity: f32,
    /// `WeatherState.opacity`.
    pub opacity: f32,
    /// Wind vector `(x, y)`.
    pub wind_vector: (f32, f32),
    /// `WeatherState.life`.
    pub life: f32,
}

impl Default for WeatherStateView {
    fn default() -> Self {
        Self {
            weather: WeatherId::new(0),
            intensity: 0.0,
            opacity: 0.0,
            wind_vector: (0.0, 0.0),
            life: 0.0,
        }
    }
}

/// The current camera rect used for cell wrapping (`setCentered` + grow).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherView {
    /// Rect x.
    pub x: f32,
    /// Rect y.
    pub y: f32,
    /// Rect width.
    pub w: f32,
    /// Rect height.
    pub h: f32,
}

impl WeatherView {
    /// `Tmp.r1.setCentered(cam.x, cam.y, w, h); grow(sizeMax * 1.5f)`.
    pub fn centered(cam_x: f32, cam_y: f32, w: f32, h: f32, grow: f32) -> Self {
        Self {
            x: cam_x - w / 2.0 - grow,
            y: cam_y - h / 2.0 - grow,
            w: w + grow * 2.0,
            h: h + grow * 2.0,
        }
    }
}

/// Weather draw parameters (`Weather.drawParticles`/`drawRain`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherFx {
    /// Draw class.
    pub kind: WeatherKind,
    /// Region.
    pub region: RegionKey,
    /// Base tint.
    pub color: Rgba,
    /// `sizeMin`.
    pub size_min: f32,
    /// `sizeMax`.
    pub size_max: f32,
    /// `density`.
    pub density: f32,
    /// `minAlpha`.
    pub min_alpha: f32,
    /// `maxAlpha`.
    pub max_alpha: f32,
    /// `sinSclMin`.
    pub sin_scl_min: f32,
    /// `sinSclMax`.
    pub sin_scl_max: f32,
    /// `sinMagMin`.
    pub sin_mag_min: f32,
    /// `sinMagMax`.
    pub sin_mag_max: f32,
    /// `randomParticleRotation`.
    pub random_particle_rotation: bool,
    /// Rain x speed.
    pub xspeed: f32,
    /// Rain y speed.
    pub yspeed: f32,
    /// Rain stroke.
    pub stroke: f32,
}

impl Default for WeatherFx {
    fn default() -> Self {
        Self {
            kind: WeatherKind::Particle,
            region: RegionKey("particle"),
            color: Rgba::WHITE,
            size_min: 3.0,
            size_max: 5.0,
            density: 16.0,
            min_alpha: 0.2,
            max_alpha: 0.6,
            sin_scl_min: 40.0,
            sin_scl_max: 100.0,
            sin_mag_min: 0.0,
            sin_mag_max: 0.5,
            random_particle_rotation: false,
            xspeed: 0.0,
            yspeed: 1.0,
            stroke: 1.0,
        }
    }
}

impl WeatherFx {
    /// `(int)(area / density * intensity)` for `drawParticles`/`drawRain`.
    pub fn particle_count(&self, view: WeatherView, intensity: f32) -> i32 {
        (view.w * view.h / self.density * intensity) as i32
    }

    /// Builds the wrapped particle field (ported from `Weather.drawParticles`).
    ///
    /// `view_tick` replaces `Time.time`; `view` is the grown camera rect and
    /// `cam` the camera bounds used for the overlap test.
    pub fn build_particles(
        &self,
        state: &WeatherStateView,
        view: WeatherView,
        cam: WeatherView,
        view_tick: f32,
        out: &mut Vec<DrawPrim>,
    ) {
        if state.opacity <= 0.0 || state.intensity <= 0.0 {
            return;
        }
        let (windx, windy) = state.wind_vector;
        let mut rand = ArcRand::new(WEATHER_SEED);
        let total = self.particle_count(view, state.intensity);
        for _ in 0..total {
            let _scl = rand.random_range_float(0.5, 1.0);
            let scl2 = rand.random_range_float(0.5, 1.0);
            let _scl3 = rand.random_range_float(0.5, 1.0);
            let size = rand.random_range_float(self.size_min, self.size_max);
            let mut x = rand.random_float(BOUND_MAX) + view_tick * windx * scl2;
            let mut y = rand.random_float(BOUND_MAX) + view_tick * windy * scl2;
            let alpha = rand.random_range_float(self.min_alpha, self.max_alpha);
            let sc = rand.random_range_float(self.sin_scl_min, self.sin_scl_max);
            let mag = rand.random_range_float(self.sin_mag_min, self.sin_mag_max);
            x += (y / sc).sin() * mag;

            x -= view.x;
            y -= view.y;
            x = x.rem_euclid(view.w.max(1.0));
            y = y.rem_euclid(view.h.max(1.0));
            x += view.x;
            y += view.y;

            // Overlap test with camera bounds.
            if (x - cam.x - cam.w / 2.0).abs() <= cam.w / 2.0 + size / 2.0
                && (y - cam.y - cam.h / 2.0).abs() <= cam.h / 2.0 + size / 2.0
            {
                let rotation = if self.random_particle_rotation {
                    rand.random_float(360.0)
                } else {
                    0.0
                };
                out.push(DrawPrim::at(
                    crate::render::layer::Layer::Weather.z(),
                    PrimKind::Region {
                        region: self.region,
                        x,
                        y,
                        w: size,
                        h: size,
                        rotation_deg: rotation,
                        origin: (0.5, 0.5),
                        color: self.color.with_alpha(alpha * state.opacity),
                        mix: None,
                        wrap: false,
                    },
                ));
            }
        }
    }

    /// `Weather.drawRain`.
    pub fn build_rain(
        &self,
        state: &WeatherStateView,
        view: WeatherView,
        cam: WeatherView,
        view_tick: f32,
        out: &mut Vec<DrawPrim>,
    ) {
        let mut rand = ArcRand::new(WEATHER_SEED);
        let total = self.particle_count(view, state.intensity);
        for _ in 0..total {
            let scl = rand.random_range_float(0.5, 1.0);
            let scl2 = rand.random_range_float(0.5, 1.0);
            let size = rand.random_range_float(self.size_min, self.size_max);
            let mut x = rand.random_float(BOUND_MAX) + view_tick * self.xspeed * scl2;
            let mut y = rand.random_float(BOUND_MAX) - view_tick * self.yspeed * scl;
            x -= view.x;
            y -= view.y;
            x = x.rem_euclid(view.w.max(1.0));
            y = y.rem_euclid(view.h.max(1.0));
            x += view.x;
            y += view.y;
            if (x - cam.x).abs() <= cam.w / 2.0 && (y - cam.y).abs() <= cam.h / 2.0 {
                let ang = crate::fx::angles::angle(self.xspeed * scl2, -self.yspeed * scl);
                out.push(DrawPrim::at(
                    crate::render::layer::Layer::Weather.z(),
                    PrimKind::Line {
                        x1: x,
                        y1: y,
                        x2: x + trnsx(ang, size / 2.0),
                        y2: y + crate::fx::angles::trnsy(ang, size / 2.0),
                        stroke: self.stroke,
                        color: self.color.with_alpha(state.opacity),
                        cap: false,
                    },
                ));
            }
        }
    }
}

/// `Weather.drawSplashes` gate: only when intensity/opacity are meaningful.
pub fn splash_visible(state: &WeatherStateView) -> bool {
    curve(state.intensity, 0.0, 1.0) > 0.0 && state.opacity > 0.0001
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn particle_counts_scale_with_area() {
        let fx = WeatherFx::default();
        let small = WeatherView::centered(0.0, 0.0, 100.0, 100.0, 0.0);
        let big = WeatherView::centered(0.0, 0.0, 200.0, 100.0, 0.0);
        assert!(fx.particle_count(big, 1.0) > fx.particle_count(small, 1.0));
        assert_eq!(fx.particle_count(small, 0.0), 0);
    }

    #[test]
    fn mod_wrap_keeps_particles_in_view() {
        let fx = WeatherFx::default();
        let state = WeatherStateView {
            intensity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        let view = WeatherView::centered(500.0, 500.0, 100.0, 100.0, 10.0);
        let cam = WeatherView::centered(500.0, 500.0, 100.0, 100.0, 0.0);
        let mut out = Vec::new();
        fx.build_particles(&state, view, cam, 3.0, &mut out);
        for prim in &out {
            if let PrimKind::Region { x, y, .. } = &prim.kind {
                assert!((400.0..=600.0).contains(x));
                assert!((400.0..=600.0).contains(y));
            }
        }
    }

    #[test]
    fn splash_gate() {
        let mut state = WeatherStateView {
            intensity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        assert!(splash_visible(&state));
        state.opacity = 0.0;
        assert!(!splash_visible(&state));
    }
}
