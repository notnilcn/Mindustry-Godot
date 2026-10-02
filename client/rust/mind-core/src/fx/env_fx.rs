// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EnvRenderers` effect bodies (plan 17 M6 / §3.12).
//!
//! Ports the underwater (water tint, caustics blit, 50 rays, suspended
//! particles) and scorching (`distortAlpha` noise layers) visual bodies from
//! `graphics/EnvRenderers.java`. Registration/pass selection stays plan 16's
//! `render::env::EnvRegistry`; this module only builds the prims.

use crate::content::{Rgba, WeatherId};
use crate::math::ArcRand;
use crate::render::draw::{DrawPrim, PrimKind, RegionKey, ShaderKey, TextureKey};
use crate::render::env::{scorching, underwater};
use crate::render::layer::Layer;

use super::weather_fx::{WeatherFx, WeatherStateView, WeatherView};

/// `sprites/rays.png` loose-texture key.
pub const RAYS_TEXTURE: &str = "rays";
/// The suspended-particle region (`Core.atlas.find("particle")`).
pub const PARTICLE_REGION: RegionKey = RegionKey("particle");
/// The caustics shader (`Shaders.caustics`).
pub const CAUSTICS_SHADER: ShaderKey = ShaderKey("caustics");

/// The underwater pass prims (`EnvRenderers.init` underwater body).
///
/// `view_tick` replaces `Time.time`; `cam_*` is the camera rect used for the
/// ray vertical fade; `ray_w`/`ray_h` are the native `rays.png` dimensions.
#[allow(clippy::too_many_arguments)]
pub fn underwater_prims(
    view_tick: f32,
    world_w: f32,
    world_h: f32,
    cam_x: f32,
    cam_y: f32,
    cam_w: f32,
    cam_h: f32,
    ray_w: f32,
    ray_h: f32,
    out: &mut Vec<DrawPrim>,
) {
    let tint_z = Layer::Light.z() + 1.0;
    out.push(DrawPrim::at(
        tint_z,
        PrimKind::Rect {
            x: cam_x,
            y: cam_y,
            w: cam_w,
            h: cam_h,
            color: Rgba::new(
                underwater::COLOR[0],
                underwater::COLOR[1],
                underwater::COLOR[2],
                underwater::COLOR[3],
            ),
        },
    ));
    // Caustics blit is additive (executed by plan 16).
    out.push(
        DrawPrim::at(
            tint_z,
            PrimKind::ShaderBlit {
                shader: CAUSTICS_SHADER,
            },
        )
        .additive(),
    );

    let ray_z = Layer::Light.z() + 2.0;
    let mut rand = ArcRand::new(0);
    let t = view_tick / underwater::TIME_SCALE;
    let world_w = world_w.max(1.0);
    let world_h = world_h.max(1.0);
    for _ in 0..underwater::RAYS {
        let offset = rand.random_float(1.0);
        let time = t + offset;
        let pos = time as i32;
        let life = time.rem_euclid(1.0);
        let mut opacity = rand.random_range_float(0.2, 0.7) * slope(life) * 0.7;
        let x = (rand.random_float(world_w) + (pos % 100) as f32 * 753.0) % world_w;
        let y = (rand.random_float(world_h) + (pos % 120) as f32 * 453.0) % world_h;
        let rot = rand.random_float(7.0);
        let size_scale = 1.0 + rand.random_float(0.3);

        let top_dst = (cam_y + cam_h / 2.0) - (y + ray_h / 2.0 + ray_h * 1.9 * size_scale / 2.0);
        let inv_dst = top_dst / 1000.0;
        opacity = opacity.min(-inv_dst);
        if opacity > 0.01 {
            out.push(DrawPrim::at(
                ray_z,
                PrimKind::Region {
                    region: RegionKey(RAYS_TEXTURE),
                    x,
                    y: y + ray_h / 2.0,
                    w: ray_w * 2.0 * size_scale,
                    h: ray_h * 2.0 * size_scale,
                    rotation_deg: rot,
                    origin: (0.5, 0.5),
                    color: Rgba::WHITE.with_alpha(opacity),
                    mix: None,
                    wrap: true,
                },
            ));
        }
    }

    // Suspended particles (`Weather.drawParticles`).
    let particles = WeatherFx {
        region: PARTICLE_REGION,
        color: Rgba::new(
            underwater::PARTICLE_COLOR[0],
            underwater::PARTICLE_COLOR[1],
            underwater::PARTICLE_COLOR[2],
            1.0,
        ),
        size_min: 1.4,
        size_max: 4.0,
        density: 10000.0,
        min_alpha: 0.5,
        max_alpha: 1.0,
        sin_scl_min: 30.0,
        sin_scl_max: 80.0,
        sin_mag_min: 1.0,
        sin_mag_max: 7.0,
        random_particle_rotation: false,
        ..WeatherFx::default()
    };
    let windx = wind_axis(underwater::WIND_ANGLE, underwater::WIND_SPEED).0;
    let windy = wind_axis(underwater::WIND_ANGLE, underwater::WIND_SPEED).1;
    let state = WeatherStateView {
        weather: WeatherId::new(0),
        intensity: 1.0,
        opacity: 1.0,
        wind_vector: (windx, windy),
        life: 1.0,
    };
    let view = WeatherView {
        x: cam_x - cam_w / 2.0,
        y: cam_y - cam_h / 2.0,
        w: cam_w,
        h: cam_h,
    };
    let weather_z = Layer::Weather.z();
    let start = out.len();
    particles.build_particles(&state, view, view, view_tick, out);
    for prim in &mut out[start..] {
        prim.z = weather_z;
    }
}

/// The scorching pass prims (`distortAlpha` noise layers).
pub fn scorching_prims(fog: bool, out: &mut Vec<DrawPrim>) {
    let fx = WeatherFx::default();
    let mut layers = Vec::new();
    fx.build_noise_layers(
        TextureKey(scorching::TEXTURE),
        Rgba::new(
            scorching::COLOR[0],
            scorching::COLOR[1],
            scorching::COLOR[2],
            scorching::COLOR[3],
        ),
        0.24,
        0.4,
        1.0,
        (1.0, 0.0),
        scorching::LAYERS,
        scorching::LAYER_BIAS,
        0.7,
        0.8,
        0.9,
        &mut layers,
    );
    let z = if fog {
        Layer::FogOfWar.z() + 1.0
    } else {
        Layer::Weather.z() - 1.0
    };
    for mut prim in layers {
        prim.z = z;
        out.push(prim);
    }
}

/// `Mathf.slope` (triangular ramp `0 -> 1 -> 0`).
fn slope(x: f32) -> f32 {
    if x <= 0.5 { x * 2.0 } else { (1.0 - x) * 2.0 }
}

/// `Mathf.cosDeg/sinDeg` scaled by `speed`.
fn wind_axis(angle_deg: f32, speed: f32) -> (f32, f32) {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    (cos * speed, sin * speed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn underwater_emits_tint_caustics_rays_and_particles() {
        let mut out = Vec::new();
        underwater_prims(
            0.0, 1024.0, 1024.0, 512.0, 512.0, 400.0, 400.0, 32.0, 64.0, &mut out,
        );
        assert!(
            out.iter()
                .any(|p| matches!(p.kind, PrimKind::ShaderBlit { .. }))
        );
        assert!(out.iter().any(|p| matches!(p.kind, PrimKind::Rect { .. })));
        assert!(
            out.iter()
                .any(|p| matches!(p.kind, PrimKind::Region { .. }))
        );
        // Rays are bounded by 50.
        let rays = out
            .iter()
            .filter(
                |p| matches!(&p.kind, PrimKind::Region { region, .. } if region.0 == RAYS_TEXTURE),
            )
            .count();
        assert!(rays <= underwater::RAYS);
    }

    #[test]
    fn scorching_emits_four_noise_layers_at_fog_or_weather() {
        let mut out = Vec::new();
        scorching_prims(false, &mut out);
        assert_eq!(out.len(), scorching::LAYERS as usize);
        assert!(out.iter().all(|p| p.z == Layer::Weather.z() - 1.0));
        let mut fog = Vec::new();
        scorching_prims(true, &mut fog);
        assert!(fog.iter().all(|p| p.z == Layer::FogOfWar.z() + 1.0));
    }
}
