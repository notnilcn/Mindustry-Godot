// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightRenderer` composite (`graphics/LightRenderer.java`, plan 16 §3.7 /
//! M5). The upstream 4×-downscaled additive target becomes a camera-sized
//! `Image` light map (deviation S16-4: a CPU map is deterministic and needs no
//! `RenderingDevice` blend-equation workaround under the locked `mobile`
//! renderer, `OD16-B`/`D16-3`). Circle accumulation lives in
//! `mind_core::render::light`; sources come from the G03 T12
//! `MindSimHost::light_sources` seam (cores, illuminators, static `emitLight`
//! blocks, units and bullets); this module owns the Godot texture + `light`
//! shader composite.
//!
//! Deviation S16-5: the per-block `lightRadius`/`lightColor` overrides and the
//! illuminator `brightness`/efficiency factor are not yet in plan 02's
//! `BlockDef`/`LightState`; the pass uses the Java defaults (`Block.lightRadius`
//! 60, illuminator `radius * 2.5`, brightness 0.75). Per-block overrides found
//! in `J/content/Blocks.java` are listed in the T12 commit.

use godot::classes::{Image, ImageTexture, Material, Mesh, MeshInstance2D, Node2D, ShaderMaterial};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::render::light::{CircleLight, LightAccumulator, SCALING, enabled};
use mind_core::render::rules::RulesRenderView;
use mind_core::render::scan::CameraView;

use crate::sim_host::MindSimHost;

use super::atlas_bind::{Quad, build_mesh};
use super::shaders::ShaderRegistry;

/// Light pass counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct LightStats {
    /// Light-map rebuilds since boot.
    pub rebuilds: i64,
    /// Live circle lights in the last rebuild.
    pub lights: i64,
    /// Whether the composite is currently visible.
    pub visible: bool,
}

/// `LightRenderer` — camera-sized light map + `Shaders.light` composite.
pub struct LightRenderer {
    host: Gd<MindSimHost>,
    node: Gd<MeshInstance2D>,
    material: Option<Gd<ShaderMaterial>>,
    accumulator: LightAccumulator,
    last_revision: i64,
    /// Camera centre of the last rebuild (motion forces one).
    last_view: (f32, f32),
    /// Frames since boot (the map rebuilds every 4 while lighting runs).
    frames: u64,
    stats: LightStats,
}

impl LightRenderer {
    /// Builds the pass over the `Layer::Light` band.
    pub fn new(host: Gd<MindSimHost>, light_band: Gd<Node2D>, shaders: &ShaderRegistry) -> Self {
        let material = shaders.get("light").map(|shader| {
            let mut material = ShaderMaterial::new_gd();
            material.set_shader(&shader);
            material
        });
        let node = MeshInstance2D::new_alloc();
        // code-instantiated: one composite quad for the whole light pass; the
        // node count is fixed by the pipeline, not by content.
        light_band.clone().add_child(&node);
        Self {
            host,
            node,
            material,
            accumulator: LightAccumulator::new(),
            last_revision: i64::MIN,
            last_view: (f32::INFINITY, f32::INFINITY),
            frames: 0,
            stats: LightStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> LightStats {
        self.stats
    }

    /// The composite mesh node (H03 T6 debug probe).
    pub fn node(&self) -> Gd<MeshInstance2D> {
        self.node.clone()
    }

    /// The composite material, if the `light` shader loaded.
    pub fn material(&self) -> Option<Gd<ShaderMaterial>> {
        self.material.clone()
    }

    /// Rebuilds the camera-sized light map when the world, the camera or the
    /// 4-frame cadence asks for it, then composites with `rules.ambient_light`.
    ///
    /// `fx_lights` are the frame's `Drawf.light` prims from `MindFx` (H03 T16):
    /// Java feeds the same requests into `renderer.lights` (`Drawf.java:301-
    /// 304`), gated by `LightRenderer.enabled()` — exactly `active` here.
    pub fn update(
        &mut self,
        view: &CameraView,
        rules: &RulesRenderView,
        draw_light: bool,
        fx_lights: &[(f32, f32, f32, [f32; 4], f32)],
    ) {
        let active =
            enabled(rules.lighting, rules.ambient_light[3], draw_light) && self.material.is_some();
        self.stats.visible = active;
        if !active {
            self.node.set_visible(false);
            return;
        }
        self.node.set_visible(true);

        let revision = self.host.clone().bind().world_revision();
        let moved =
            (view.x - self.last_view.0).abs() > 1.0 || (view.y - self.last_view.1).abs() > 1.0;
        self.frames = self.frames.wrapping_add(1);
        if revision == self.last_revision && !moved && !self.frames.is_multiple_of(4) {
            return;
        }
        self.last_revision = revision;
        self.last_view = (view.x, view.y);

        let (width, height, bytes, lights) = {
            let host = self.host.clone();
            let host = host.bind();
            let rect = (view.x - view.w / 2.0, view.y - view.h / 2.0, view.w, view.h);
            self.accumulator.clear();
            for source in host.light_sources(rect) {
                self.accumulator.add(
                    source.x,
                    source.y,
                    source.radius,
                    source.color,
                    source.opacity,
                );
            }
            // H03 T16: the FX system's `Drawf.light` requests (`Fx.light`
            // prims from hit/shoot effects), same accumulator.
            for (x, y, radius, color, opacity) in fx_lights {
                self.accumulator.add(*x, *y, *radius, *color, *opacity);
            }
            let width = ((view.w / SCALING as f32).ceil() as i32).max(1);
            let height = ((view.h / SCALING as f32).ceil() as i32).max(1);
            let mut bytes = vec![0u8; (width * height * 4) as usize];
            for light in self.accumulator.circles() {
                stamp_rgba(&mut bytes, width, height, view, light);
            }
            (width, height, bytes, self.accumulator.len() as i64)
        };

        let Some(mut material) = self.material.clone() else {
            return;
        };
        let data = PackedByteArray::from(bytes);
        let image = Image::create_from_data(
            width,
            height,
            false,
            godot::classes::image::Format::RGBA8,
            &data,
        );
        let Some(image) = image else {
            return;
        };
        let Some(texture) = ImageTexture::create_from_image(&image) else {
            return;
        };
        material.set_shader_parameter("u_texture", &texture.to_variant());
        material.set_shader_parameter(
            "u_ambient",
            &Color::from_rgba(
                rules.ambient_light[0],
                rules.ambient_light[1],
                rules.ambient_light[2],
                rules.ambient_light[3],
            )
            .to_variant(),
        );
        let quad = Quad {
            cx: view.x,
            cy: view.y,
            w: view.w,
            h: view.h,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: 0xffff_ffff,
        };
        if let Some(mesh) = build_mesh(&[quad], 0.0, false) {
            self.node.set_mesh(&mesh.upcast::<Mesh>());
            self.node.set_material(&material.upcast::<Material>());
            // The canvas item needs a texture even though the shader samples
            // `u_texture`; without it the mesh item is skipped by the canvas
            // renderer (found in-engine during G03 T12).
            self.node.set_texture(&texture);
        }
        self.stats.rebuilds += 1;
        self.stats.lights = lights;
    }
}

/// Stamps one light into the camera-sized RGBA map (1/4-scale texels,
/// row-major top-first like the camera view). Java blends the circle buffer
/// with `Gl.max`, so rgb and alpha both take the max over circles; the texel
/// falloff approximates the `circle-shadow` radial profile.
fn stamp_rgba(bytes: &mut [u8], width: i32, height: i32, view: &CameraView, light: &CircleLight) {
    if light.radius <= 0.0 || width <= 0 || height <= 0 {
        return;
    }
    let scale = view.w.max(1.0) / width as f32;
    let origin_x = view.x - view.w / 2.0;
    let origin_y = view.y - view.h / 2.0;
    let cx = (light.x - origin_x) / scale;
    let cy = (light.y - origin_y) / scale;
    let r = light.radius / scale;
    let x1 = ((cx - r).floor() as i32).max(0);
    let y1 = ((cy - r).floor() as i32).max(0);
    let x2 = ((cx + r).ceil() as i32).min(width);
    let y2 = ((cy + r).ceil() as i32).min(height);
    for ty in y1..y2 {
        for tx in x1..x2 {
            let dx = tx as f32 + 0.5 - cx;
            let dy = ty as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d >= r {
                continue;
            }
            let falloff = 1.0 - d / r;
            let index = ((tx + ty * width) * 4) as usize;
            let r8 = (light.color[0] * falloff * 255.0).clamp(0.0, 255.0) as u8;
            let g8 = (light.color[1] * falloff * 255.0).clamp(0.0, 255.0) as u8;
            let b8 = (light.color[2] * falloff * 255.0).clamp(0.0, 255.0) as u8;
            let a8 = (light.color[3] * falloff * 255.0).clamp(0.0, 255.0) as u8;
            bytes[index] = bytes[index].max(r8);
            bytes[index + 1] = bytes[index + 1].max(g8);
            bytes[index + 2] = bytes[index + 2].max(b8);
            bytes[index + 3] = bytes[index + 3].max(a8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_is_brightest_at_the_center() {
        let view = CameraView {
            x: 16.0,
            y: 16.0,
            w: 32.0,
            h: 32.0,
            zoom: 1.0,
            team: 0,
        };
        let mut bytes = vec![0u8; 8 * 8 * 4];
        let light = CircleLight {
            x: 16.0,
            y: 16.0,
            color: [1.0, 0.5, 0.0, 1.0],
            radius: 16.0,
        };
        stamp_rgba(&mut bytes, 8, 8, &view, &light);
        // Texel (4,4) maps to world (18,18), ~1 px from the light centre.
        let index = ((4 + 4 * 8) * 4) as usize;
        assert!(
            usize::from(bytes[index]) >= usize::from(bytes[0]),
            "falloff"
        );
        assert!(bytes[index + 3] > 0, "alpha stamped");
        // Outside the circle (world 32,32 is 22 px away) stays untouched.
        assert_eq!(bytes[(7 + 7 * 8) * 4 + 3], 0);
    }
}
