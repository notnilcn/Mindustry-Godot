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

/// The world surface under a splash sample (`Weather.drawSplashes` branch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SplashGround {
    /// No floor (`tile == null`): skip.
    None,
    /// A non-liquid, non-solid floor: draw the spark lines.
    Clear,
    /// A liquid floor: draw the splash frame tinted by `color`.
    Liquid(Rgba),
    /// A solid floor: skip.
    Solid,
}

impl WeatherFx {
    /// `Weather.drawSplashes` (`Weather.java:191-235`): the splash frame field
    /// plus the ground spark lines. `view` is the padded camera rect, `cam` the
    /// camera bounds; `ground` resolves the tile floor at world `(x, y)`.
    ///
    /// `view_tick` replaces `Time.time` (deviation #7). `splash_regions` are the
    /// ordered splash frames; `splash_size` is their native pixel size.
    #[allow(clippy::too_many_arguments)]
    pub fn build_splashes(
        &self,
        state: &WeatherStateView,
        view: WeatherView,
        cam: WeatherView,
        view_tick: f32,
        density: f32,
        time_scale: f32,
        stroke: f32,
        splash_regions: &[RegionKey],
        splash_size: f32,
        ground: impl Fn(f32, f32) -> SplashGround,
        out: &mut Vec<DrawPrim>,
    ) {
        if !splash_visible(state) || splash_regions.is_empty() || density <= 0.0 {
            return;
        }
        let total = (view.w * view.h / density * state.intensity) as i32 / 2;
        let t = view_tick / time_scale.max(1e-6);
        let mut rand = ArcRand::new(WEATHER_SEED);
        let layer = crate::render::layer::Layer::Weather.z();
        for _ in 0..total {
            let offset = rand.random_float(1.0);
            let time = t + offset;
            let pos = time as i32;
            let life = time.rem_euclid(1.0);
            let mut x = rand.random_float(BOUND_MAX) + pos as f32 * 953.0;
            let mut y = rand.random_float(BOUND_MAX) - pos as f32 * 453.0;
            x -= view.x;
            y -= view.y;
            x = x.rem_euclid(view.w.max(1.0));
            y = y.rem_euclid(view.h.max(1.0));
            x += view.x;
            y += view.y;

            // `Tmp.r3.setCentered(x, y, life * 4).overlaps(cam)`.
            let r3 = life * 4.0;
            if (x - cam.x).abs() > (r3 + cam.w) / 2.0 || (y - cam.y).abs() > (r3 + cam.h) / 2.0 {
                continue;
            }
            match ground(x, y) {
                SplashGround::Liquid(floor_color) => {
                    let idx = (life * (splash_regions.len() - 1) as f32) as usize;
                    let region = splash_regions[idx.min(splash_regions.len() - 1)];
                    out.push(DrawPrim::at(
                        layer,
                        PrimKind::Region {
                            region,
                            x,
                            y,
                            w: splash_size,
                            h: splash_size,
                            rotation_deg: 0.0,
                            origin: (0.5, 0.5),
                            color: floor_color.with_alpha(state.opacity),
                            mix: None,
                            wrap: false,
                        },
                    ));
                }
                SplashGround::Clear => {
                    // `Mathf.slope(life)`: triangular ramp `0 -> 1 -> 0`.
                    let alpha = (1.0 - (2.0 * life - 1.0).abs()) * state.opacity;
                    let color = self.color.with_alpha(alpha);
                    for sign in [-1.0f32, 1.0] {
                        let a = 90.0 + sign * 45.0;
                        let len = 1.0 + 5.0 * life;
                        let sx = x + trnsx(a, len);
                        let sy = y + crate::fx::angles::trnsy(a, len);
                        out.push(DrawPrim::at(
                            layer,
                            PrimKind::Line {
                                x1: sx,
                                y1: sy,
                                x2: sx + trnsx(a, 3.0 * (1.0 - life)),
                                y2: sy + crate::fx::angles::trnsy(a, 3.0 * (1.0 - life)),
                                stroke,
                                color,
                                cap: true,
                            },
                        ));
                    }
                }
                SplashGround::None | SplashGround::Solid => {}
            }
        }
    }

    /// `Weather.drawNoiseLayers` (`Weather.java:238-241` ->
    /// `NoiseEffect.drawNoiseLayers`): the layer chain with per-layer speed,
    /// alpha, scale and color multipliers. `view_tick` replaces `Time.time`.
    #[allow(clippy::too_many_arguments)]
    pub fn build_noise_layers(
        &self,
        texture: crate::render::draw::TextureKey,
        color: Rgba,
        opacity: f32,
        base_speed: f32,
        intensity: f32,
        wind: (f32, f32),
        layers: i32,
        layer_speed_mul: f32,
        layer_alpha_mul: f32,
        layer_scl_mul: f32,
        layer_color_mul: f32,
        out: &mut Vec<DrawPrim>,
    ) {
        let mut sspeed = 1.0f32;
        let mut salpha = 1.0f32;
        let mut _sscl = 1.0f32;
        let mut offset = 0.0f32;
        let mut col = color;
        for _ in 0..layers {
            let speed = sspeed * base_speed * intensity;
            let scroll = [-(wind.0 * speed), -(wind.1 * speed)];
            out.push(DrawPrim::at(
                crate::render::layer::Layer::Weather.z(),
                PrimKind::NoiseLayer {
                    texture,
                    rect: [0.0, 0.0, 0.0, 0.0],
                    tint: col,
                    opacity: salpha * opacity,
                    scroll,
                    offset,
                },
            ));
            sspeed *= layer_speed_mul;
            salpha *= layer_alpha_mul;
            _sscl *= layer_scl_mul;
            offset += 0.29;
            col = Rgba::new(
                col.r * layer_color_mul,
                col.g * layer_color_mul,
                col.b * layer_color_mul,
                col.a,
            );
        }
    }
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

    #[test]
    fn noise_layers_emit_one_prim_per_layer() {
        let fx = WeatherFx::default();
        let mut out = Vec::new();
        fx.build_noise_layers(
            crate::render::draw::TextureKey("distortAlpha"),
            Rgba::new(1.0, 0.14, 0.14, 1.0),
            0.24,
            0.4,
            1.0,
            (1.0, 0.0),
            4,
            -1.3,
            0.7,
            0.8,
            0.9,
            &mut out,
        );
        assert_eq!(out.len(), 4);
        assert!(
            out.iter()
                .all(|p| matches!(p.kind, PrimKind::NoiseLayer { .. }))
        );
        // Alpha decays by `layerAlphaMul` each layer.
        let first = match &out[0].kind {
            PrimKind::NoiseLayer { opacity, .. } => *opacity,
            _ => unreachable!(),
        };
        let second = match &out[1].kind {
            PrimKind::NoiseLayer { opacity, .. } => *opacity,
            _ => unreachable!(),
        };
        assert!(second < first);
    }

    #[test]
    fn splashes_branch_on_ground() {
        let fx = WeatherFx::default();
        let state = WeatherStateView {
            intensity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        let view = WeatherView {
            x: 0.0,
            y: 0.0,
            w: 1000.0,
            h: 1000.0,
        };
        let cam = WeatherView {
            x: 500.0,
            y: 500.0,
            w: 1000.0,
            h: 1000.0,
        };
        let splashes = [RegionKey("splash-0"), RegionKey("splash-1")];

        let mut liquid = Vec::new();
        fx.build_splashes(
            &state,
            view,
            cam,
            0.0,
            1000.0,
            1.0,
            1.0,
            &splashes,
            16.0,
            |_, _| SplashGround::Liquid(Rgba::WHITE),
            &mut liquid,
        );
        assert!(
            liquid
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Region { .. }))
        );

        let mut clear = Vec::new();
        fx.build_splashes(
            &state,
            view,
            cam,
            0.0,
            1000.0,
            1.0,
            1.0,
            &splashes,
            16.0,
            |_, _| SplashGround::Clear,
            &mut clear,
        );
        assert!(
            clear
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Line { .. }))
        );

        let mut solid = Vec::new();
        fx.build_splashes(
            &state,
            view,
            cam,
            0.0,
            1000.0,
            1.0,
            1.0,
            &splashes,
            16.0,
            |_, _| SplashGround::Solid,
            &mut solid,
        );
        assert!(solid.is_empty());
    }
}
