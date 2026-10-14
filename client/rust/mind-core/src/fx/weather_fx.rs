// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weather particle/rain/splash visual builders (plan 17 §3.12).
//!
//! Ported from `type/Weather.java` `drawParticles`/`drawRain` and the
//! `ParticleWeather`/`RainWeather` draw halves. Weather **state** (life,
//! opacity, wind, status application) stays in plan 12; this module owns only
//! the draw math. `Time.time` is the fixed view tick (deviation #7).

use crate::content::registries::weathers::WeatherDef;
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

/// Arc `Rectangle.overlaps` for a centered rect (`Tmp.r3.setCentered`).
///
/// `cam` is the bottom-left camera rect (`Core.camera.bounds`).
fn overlaps_centered(x: f32, y: f32, size: f32, cam: WeatherView) -> bool {
    let h = size / 2.0;
    x - h < cam.x + cam.w && x + h > cam.x && y - h < cam.y + cam.h && y + h > cam.y
}

/// `Mathf.mod(f, n) = ((f % n) + n) % n`.
fn arc_mod(f: f32, n: f32) -> f32 {
    ((f % n) + n) % n
}

/// Maps a `ParticleWeather` region name to the port's static region key.
fn particle_region(name: &str, weather: &str) -> RegionKey {
    match name {
        "particle" | "circle-small" | "circle-shadow" => RegionKey(match name {
            "circle-small" => "circle-small",
            "circle-shadow" => "circle-shadow",
            _ => "particle",
        }),
        other => {
            log::warn!("weather `{weather}`: unknown particle region `{other}`; using `particle`");
            RegionKey("particle")
        }
    }
}

/// `WeatherFx` for a weather def (`ParticleWeather`/`RainWeather` draw halves,
/// T6): `None` for `fog` (no particles) and non-drawing weathers.
pub fn weather_fx_for(def: &WeatherDef) -> Option<WeatherFx> {
    if let Some(rain) = &def.rain {
        return Some(WeatherFx {
            kind: WeatherKind::Rain,
            xspeed: rain.xspeed,
            yspeed: rain.yspeed,
            stroke: rain.stroke,
            size_min: rain.size_min,
            size_max: rain.size_max,
            density: rain.density,
            color: rain.color,
            ..WeatherFx::default()
        });
    }
    let particle = def.particle.as_ref()?;
    if !particle.draw_particles {
        return None;
    }
    Some(WeatherFx {
        kind: WeatherKind::Particle,
        region: particle_region(&particle.particle_region, &def.name),
        color: particle.color,
        size_min: particle.size_min,
        size_max: particle.size_max,
        density: particle.density,
        min_alpha: particle.min_alpha,
        max_alpha: particle.max_alpha,
        sin_scl_min: particle.sin_scl_min,
        sin_scl_max: particle.sin_scl_max,
        sin_mag_min: particle.sin_mag_min,
        sin_mag_max: particle.sin_mag_max,
        random_particle_rotation: particle.random_particle_rotation,
        ..WeatherFx::default()
    })
}

impl WeatherFx {
    /// `(int)(area / density * intensity)` for `drawParticles`/`drawRain`.
    pub fn particle_count(&self, view: WeatherView, intensity: f32) -> i32 {
        (view.w * view.h / self.density * intensity) as i32
    }

    /// Builds the wrapped particle field (ported from `Weather.drawParticles`).
    ///
    /// `view_tick` replaces `Time.time`; `view` is the grown camera rect and
    /// `cam` the bottom-left camera bounds used for the overlap test. `wind` is
    /// the caller-resolved `(windx, windy)` (`useWindVector` rule lives in
    /// `ParticleWeather.drawOver`). The RNG draw order matches Java exactly:
    /// `scl, scl2, size, x, y, alpha, [rotation], sinScl, sinMag` per particle.
    pub fn build_particles(
        &self,
        state: &WeatherStateView,
        view: WeatherView,
        cam: WeatherView,
        view_tick: f32,
        wind: (f32, f32),
        out: &mut Vec<DrawPrim>,
    ) {
        if state.opacity <= 0.0 || state.intensity <= 0.0 {
            return;
        }
        let mut rand = ArcRand::new(WEATHER_SEED);
        let total = self.particle_count(view, state.intensity);
        for _ in 0..total {
            let scl = rand.random_range_float(0.5, 1.0);
            let scl2 = rand.random_range_float(0.5, 1.0);
            let size = rand.random_range_float(self.size_min, self.size_max);
            let mut x = rand.random_float(BOUND_MAX) + view_tick * wind.0 * scl2;
            let mut y = rand.random_float(BOUND_MAX) + view_tick * wind.1 * scl;
            let alpha = rand.random_range_float(self.min_alpha, self.max_alpha);
            let rotation = if self.random_particle_rotation {
                rand.random_float(360.0)
            } else {
                0.0
            };
            let sin_scl = rand.random_range_float(self.sin_scl_min, self.sin_scl_max);
            let sin_mag = rand.random_range_float(self.sin_mag_min, self.sin_mag_max);
            x += (y / sin_scl).sin() * sin_mag;

            x = arc_mod(x - view.x, view.w.max(1.0)) + view.x;
            y = arc_mod(y - view.y, view.h.max(1.0)) + view.y;

            if overlaps_centered(x, y, size, cam) {
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

    /// `Weather.drawRain`: per-drop tint drawn from the string alpha passed by
    /// the caller (`Draw.getColorAlpha()` = `weatherAlpha * opacity *
    /// opacityMultiplier`). RNG order: `scl, scl2, size, x, y, tint`.
    pub fn build_rain(
        &self,
        state: &WeatherStateView,
        view: WeatherView,
        cam: WeatherView,
        view_tick: f32,
        alpha: f32,
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
            let tint = rand.random_float(1.0) * alpha;

            x = arc_mod(x - view.x, view.w.max(1.0)) + view.x;
            y = arc_mod(y - view.y, view.h.max(1.0)) + view.y;

            if overlaps_centered(x, y, size, cam) {
                let ang = crate::fx::angles::angle(self.xspeed * scl2, -self.yspeed * scl);
                out.push(DrawPrim::at(
                    crate::render::layer::Layer::Weather.z(),
                    PrimKind::Line {
                        x1: x,
                        y1: y,
                        x2: x + trnsx(ang, size / 2.0),
                        y2: y + crate::fx::angles::trnsy(ang, size / 2.0),
                        stroke: self.stroke,
                        color: self.color.with_alpha(tint),
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

/// One noise layer to draw (T7): the `NoiseEffect.drawNoise` UV rect after
/// `TextureRegion.scroll`, the layer tint and the texture name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseLayerDraw {
    /// `(u, v, u2, v2)`; may run outside `[0, 1]` (the texture must repeat).
    pub uv: [f32; 4],
    /// `noiseColor` with alpha `state.opacity * salpha * opacityMultiplier`.
    pub color: Rgba,
    /// `"fog"` or `"noiseAlpha"` (`sprites/<noisePath>.png`).
    pub texture: &'static str,
}

/// `NoiseEffect.drawNoise` UV rect after `TextureRegion.scroll`
/// (`NoiseEffect.java:57-71`; Arc `TextureRegion.java:267-278`).
pub fn noise_uv(
    cam: WeatherView,
    time: f32,
    noisescl: f32,
    base_speed: f32,
    intensity: f32,
    vwind: (f32, f32),
    offset: f32,
) -> [f32; 4] {
    let speed = base_speed * intensity;
    let (wx, wy) = (vwind.0 * speed, vwind.1 * speed);
    let scale = 1.0 / noisescl;
    let scroll = time * scale + offset;
    let (mut u, mut v) = (cam.x * scale, cam.y * scale);
    let (mut u2, mut v2) = ((cam.x + cam.w) * scale, (cam.y + cam.h) * scale);
    let (xa, ya) = (-wx * scroll, -wy * scroll);
    if xa != 0.0 {
        let w = u2 - u;
        u = (u + xa) % 1.0;
        u2 = u + w;
    }
    if ya != 0.0 {
        let h = v2 - v;
        v = (v + ya) % 1.0;
        v2 = v + h;
    }
    [u, v, u2, v2]
}

/// `ParticleWeather.drawOver` noise loop (`ParticleWeather.java:66-83`): one
/// layer row per `noiseLayers`, with the per-layer speed/alpha/scale/offset
/// progression. `time` replaces `Time.time`.
pub fn particle_noise_layers(
    def: &WeatherDef,
    state: &WeatherStateView,
    cam: WeatherView,
    time: f32,
    out: &mut Vec<NoiseLayerDraw>,
) {
    let Some(p) = def.particle.as_ref() else {
        return;
    };
    if !p.draw_noise {
        return;
    }
    // Same wind resolution as `drawOver` (`useWindVector` rule).
    let wind = if p.use_wind_vector {
        let speed = p.base_speed * state.intensity;
        (state.wind_vector.0 * speed, state.wind_vector.1 * speed)
    } else {
        (p.xspeed, p.yspeed)
    };
    let texture = if p.noise_path == "fog" {
        "fog"
    } else {
        "noiseAlpha"
    };
    let mut sspeed = 1.0f32;
    let mut sscl = 1.0f32;
    let mut salpha = 1.0f32;
    let mut offset = 0.0f32;
    for _ in 0..p.noise_layers {
        let uv = noise_uv(
            cam,
            time,
            p.noise_scale * sscl,
            sspeed * if p.use_wind_vector { 1.0 } else { p.base_speed },
            state.intensity,
            wind,
            offset,
        );
        out.push(NoiseLayerDraw {
            uv,
            color: p
                .noise_color
                .with_alpha(state.opacity * salpha * def.opacity_multiplier),
            texture,
        });
        sspeed *= p.noise_layer_speed_m;
        salpha *= p.noise_layer_alpha_m;
        sscl *= p.noise_layer_scl_m;
        offset += 0.29;
    }
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
        // `WeatherStateComp.draw`: `drawUnder` renders at `Layer.debris`.
        let layer = crate::render::layer::Layer::Debris.z();
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
            if !overlaps_centered(x, y, life * 4.0, cam) {
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
        fx.build_particles(&state, view, cam, 3.0, state.wind_vector, &mut out);
        for prim in &out {
            if let PrimKind::Region { x, y, .. } = &prim.kind {
                assert!((400.0..=600.0).contains(x));
                assert!((400.0..=600.0).contains(y));
            }
        }
    }

    /// H07 T6: the particle RNG sequence matches `Weather.drawParticles`
    /// (`scl, scl2, size, x, y, alpha, sinScl, sinMag`; +rotation after alpha
    /// when `randomParticleRotation`).
    #[test]
    fn particles_rng_sequence_matches_java() {
        let fx = WeatherFx {
            kind: WeatherKind::Particle,
            density: 80_000.0 * 80_000.0,
            size_min: 5.0,
            size_max: 5.0,
            min_alpha: 0.3,
            max_alpha: 0.3,
            sin_scl_min: 40.0,
            sin_scl_max: 40.0,
            sin_mag_min: 2.0,
            sin_mag_max: 2.0,
            ..WeatherFx::default()
        };
        let view = WeatherView {
            x: 0.0,
            y: 0.0,
            w: 80_000.0,
            h: 80_000.0,
        };
        let state = WeatherStateView {
            intensity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        let mut out = Vec::new();
        fx.build_particles(&state, view, view, 0.0, (0.0, 0.0), &mut out);
        assert_eq!(out.len(), 1, "total = area/density = 1");

        // Independent recomputation in the Java draw order.
        let mut rand = ArcRand::new(WEATHER_SEED);
        let _scl = rand.random_range_float(0.5, 1.0);
        let _scl2 = rand.random_range_float(0.5, 1.0);
        let size = rand.random_range_float(fx.size_min, fx.size_max);
        let mut x = rand.random_float(BOUND_MAX);
        let y = rand.random_float(BOUND_MAX);
        let alpha = rand.random_range_float(fx.min_alpha, fx.max_alpha);
        let sin_scl = rand.random_range_float(fx.sin_scl_min, fx.sin_scl_max);
        let sin_mag = rand.random_range_float(fx.sin_mag_min, fx.sin_mag_max);
        x += (y / sin_scl).sin() * sin_mag;
        let x = ((x % view.w) + view.w) % view.w;
        let PrimKind::Region {
            x: px,
            y: py,
            w,
            color,
            rotation_deg,
            ..
        } = &out[0].kind
        else {
            unreachable!()
        };
        assert!((px - x).abs() < 0.1, "{px} vs {x}");
        assert!((py - y).abs() < 0.1, "{py} vs {y}");
        assert!((w - size).abs() < 1.0e-6);
        assert!((color.a - alpha).abs() < 1.0e-6);
        assert_eq!(*rotation_deg, 0.0);

        // `randomParticleRotation = true` inserts the 360 draw after alpha.
        let rotated = WeatherFx {
            random_particle_rotation: true,
            ..fx
        };
        let mut out = Vec::new();
        rotated.build_particles(&state, view, view, 0.0, (0.0, 0.0), &mut out);
        let mut rand = ArcRand::new(WEATHER_SEED);
        let _ = rand.random_range_float(0.5, 1.0);
        let _ = rand.random_range_float(0.5, 1.0);
        let _ = rand.random_range_float(rotated.size_min, rotated.size_max);
        let _ = rand.random_float(BOUND_MAX);
        let _ = rand.random_float(BOUND_MAX);
        let _ = rand.random_range_float(rotated.min_alpha, rotated.max_alpha);
        let rotation = rand.random_float(360.0);
        assert!(matches!(
            &out[0].kind,
            PrimKind::Region { rotation_deg, .. } if (*rotation_deg - rotation).abs() < 0.1
        ));
    }

    /// H07 T6: rain draws its per-drop `tint` (sixth draw) and the line colour
    /// alpha is `tint * alpha`.
    #[test]
    fn rain_tint_is_drawn() {
        let fx = WeatherFx {
            kind: WeatherKind::Rain,
            density: 80_000.0 * 80_000.0 / 2.0,
            size_min: 8.0,
            size_max: 8.0,
            xspeed: 1.5,
            yspeed: 5.0,
            stroke: 0.75,
            color: Rgba::from_hex("7a95eaff").unwrap_or(Rgba::WHITE),
            ..WeatherFx::default()
        };
        let view = WeatherView {
            x: 0.0,
            y: 0.0,
            w: 80_000.0,
            h: 80_000.0,
        };
        let state = WeatherStateView {
            intensity: 1.0,
            opacity: 1.0,
            ..Default::default()
        };
        let mut out = Vec::new();
        fx.build_rain(&state, view, view, 0.0, 0.5, &mut out);
        assert_eq!(out.len(), 2, "total = 2");

        let mut rand = ArcRand::new(WEATHER_SEED);
        let _ = rand.random_range_float(0.5, 1.0);
        let _ = rand.random_range_float(0.5, 1.0);
        let _ = rand.random_range_float(fx.size_min, fx.size_max);
        let _ = rand.random_float(BOUND_MAX);
        let _ = rand.random_float(BOUND_MAX);
        let tint = rand.random_float(1.0) * 0.5;
        let PrimKind::Line { color, .. } = &out[0].kind else {
            unreachable!()
        };
        assert!((color.a - tint).abs() < 1.0e-6, "first tint {tint}");

        // Second drop's `x` is its fourth draw.
        let _ = rand.random_range_float(0.5, 1.0);
        let _ = rand.random_range_float(0.5, 1.0);
        let _ = rand.random_range_float(fx.size_min, fx.size_max);
        let x2 = rand.random_float(BOUND_MAX);
        let PrimKind::Line { x1, .. } = &out[1].kind else {
            unreachable!()
        };
        assert!((x1 - x2).abs() < 0.1, "second drop x {x1} vs {x2}");
        assert_eq!(out[1].z, crate::render::layer::Layer::Weather.z());
    }

    /// H07 T6: splashes render at the debris band (`WeatherStateComp.draw`).
    #[test]
    fn splashes_on_debris_layer() {
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
        let cam = view;
        let splashes = [RegionKey("splash-0"), RegionKey("splash-1")];
        let mut out = Vec::new();
        fx.build_splashes(
            &state,
            view,
            cam,
            0.0,
            1000.0,
            1.0,
            0.75,
            &splashes,
            8.0,
            |_, _| SplashGround::Clear,
            &mut out,
        );
        assert!(!out.is_empty());
        assert!(
            out.iter()
                .all(|prim| prim.z == crate::render::layer::Layer::Debris.z())
        );
    }

    /// H07 T6: the overlap test is Arc `Rectangle.overlaps` against the
    /// bottom-left camera rect.
    #[test]
    fn overlap_uses_bottom_left_rect() {
        let cam = WeatherView {
            x: 100.0,
            y: 100.0,
            w: 50.0,
            h: 50.0,
        };
        assert!(overlaps_centered(101.0, 101.0, 1.0, cam));
        assert!(!overlaps_centered(95.0, 101.0, 1.0, cam));
        assert!(!overlaps_centered(101.0, 95.0, 1.0, cam));
        assert!(overlaps_centered(149.5, 149.5, 1.0, cam));
    }

    /// H07 T6: `weather_fx_for` matches `Weathers.java` values.
    #[test]
    fn weather_fx_for_matches_weathers_java() {
        let content = crate::content::test_support::test_registry();
        let snow = weather_fx_for(content.weather_by_name("snowing").expect("snowing")).unwrap();
        assert_eq!(snow.kind, WeatherKind::Particle);
        assert_eq!(snow.region, RegionKey("particle"));
        assert_eq!(snow.size_min, 2.6);
        assert_eq!(snow.size_max, 13.0);
        assert_eq!(snow.density, 1200.0);
        assert_eq!((snow.min_alpha, snow.max_alpha), (1.0, 1.0));
        assert_eq!((snow.sin_scl_min, snow.sin_scl_max), (30.0, 80.0));
        assert_eq!((snow.sin_mag_min, snow.sin_mag_max), (1.0, 7.0));

        let rain = weather_fx_for(content.weather_by_name("rain").expect("rain")).unwrap();
        assert_eq!(rain.kind, WeatherKind::Rain);
        assert_eq!(rain.stroke, 0.75);
        assert_eq!((rain.size_min, rain.size_max), (8.0, 40.0));
        assert_eq!((rain.xspeed, rain.yspeed), (1.5, 5.0));
        assert_eq!(rain.density, 1200.0);
        assert_eq!(
            rain.color,
            Rgba::from_hex("7a95eaff").unwrap_or(Rgba::WHITE)
        );

        let sand =
            weather_fx_for(content.weather_by_name("sandstorm").expect("sandstorm")).unwrap();
        assert_eq!((sand.size_min, sand.size_max), (70.0, 140.0));
        assert_eq!((sand.min_alpha, sand.max_alpha), (0.0, 0.2));
        assert_eq!(sand.density, 1500.0);
        assert_eq!(sand.color, Rgba::from_hex("f7cba4").unwrap_or(Rgba::WHITE));

        let spores =
            weather_fx_for(content.weather_by_name("sporestorm").expect("sporestorm")).unwrap();
        assert_eq!(spores.region, RegionKey("circle-small"));
        assert_eq!((spores.size_min, spores.size_max), (2.5, 5.0));
        assert_eq!((spores.min_alpha, spores.max_alpha), (0.1, 0.8));
        assert_eq!(spores.density, 2000.0);
        assert_eq!(
            spores.color,
            Rgba::from_hex("7457ce").unwrap_or(Rgba::WHITE)
        );

        // `fog` has `drawParticles = false`.
        assert!(weather_fx_for(content.weather_by_name("fog").expect("fog")).is_none());
    }

    /// H07 T7: `noise_uv` matches `TextureRegion.scroll` after
    /// `NoiseEffect.drawNoise`.
    #[test]
    fn uv_scroll_math() {
        let cam = WeatherView {
            x: 50.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        // `xAmount = -0.0` is `== 0.0`: no scroll at `time = 0`.
        let uv = noise_uv(cam, 0.0, 1000.0, 1.0, 1.0, (1.0, 0.0), 0.0);
        assert_eq!(uv, [0.05, 0.0, 0.15, 0.1]);
        // `time = 1000`: `scroll = 1`, `u = (0.05 - 1) % 1 = -0.95`.
        let uv = noise_uv(cam, 1000.0, 1000.0, 1.0, 1.0, (1.0, 0.0), 0.0);
        assert!((uv[0] - -0.95).abs() < 1.0e-6, "{uv:?}");
        assert!((uv[1]).abs() < 1.0e-6, "{uv:?}");
        assert!((uv[2] - -0.85).abs() < 1.0e-6, "{uv:?}");
        assert!((uv[3] - 0.1).abs() < 1.0e-6, "{uv:?}");
    }

    /// H07 T7: fog's three layers match `ParticleWeather.drawOver` multipliers
    /// (uses `Weathers.java` fog values).
    #[test]
    fn fog_layers_match_java() {
        let content = crate::content::test_support::test_registry();
        let fog = content.weather_by_name("fog").expect("fog");
        let state = WeatherStateView {
            intensity: 1.0,
            opacity: 0.8,
            ..Default::default()
        };
        let cam = WeatherView {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let mut out = Vec::new();
        particle_noise_layers(fog, &state, cam, 0.0, &mut out);
        assert_eq!(out.len(), 3, "fog has 3 noise layers");
        assert_eq!(out[0].texture, "fog");
        for (index, layer) in out.iter().enumerate() {
            let alpha = 0.8 * 0.47 * 0.7f32.powi(index as i32);
            assert!(
                (layer.color.a - alpha).abs() < 1.0e-6,
                "layer {index} alpha {} vs {alpha}",
                layer.color.a
            );
            assert!((layer.color.r - 0.4).abs() < 1.0e-6);
        }
        // `noiseScale` decays by 0.6: `{1100, 660, 396}` -> uv width cam.w/scl.
        for (index, scale) in [1100.0f32, 660.0, 396.0].into_iter().enumerate() {
            let width = out[index].uv[2] - out[index].uv[0];
            assert!(
                (width - 100.0 / scale).abs() < 1.0e-6,
                "layer {index} width {width}"
            );
        }
        // Per-layer base speed `{0.05, 0.1}` checked against the exact scroll:
        // `u = (0 - wind.x * speed * (time / noisescl + offset)) % 1`.
        let mut scrolled = Vec::new();
        particle_noise_layers(fog, &state, cam, 1.0, &mut scrolled);
        let expected0 = -(1.0 * 0.05 * (1.0 / 1100.0));
        assert!(
            (scrolled[0].uv[0] - expected0).abs() < 1.0e-9,
            "layer 0 u {} vs {expected0}",
            scrolled[0].uv[0]
        );
        let expected1 = -(1.0 * 0.1 * (1.0 / 660.0 + 0.29));
        assert!(
            (scrolled[1].uv[0] - expected1).abs() < 1.0e-6,
            "layer 1 u {} vs {expected1}",
            scrolled[1].uv[0]
        );
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
