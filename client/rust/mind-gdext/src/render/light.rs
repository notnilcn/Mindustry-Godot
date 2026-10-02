// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightRenderer` composite (`graphics/LightRenderer.java`, plan 16 §3.7 /
//! M5). The upstream 4×-downscaled additive target becomes a world-sized
//! `Image` light map (deviation S16-4: a CPU map is deterministic and needs no
//! `RenderingDevice` blend-equation workaround under the locked `mobile`
//! renderer, `OD16-B`/`D16-3`). Circle accumulation lives in
//! `mind_core::render::light`; this module owns the Godot texture + `light`
//! shader composite.
//!
//! Deviation S16-5: the per-block `lightRadius`/`lightColor` fields are not yet
//! in plan 02's `BlockDef`, so an `emit_light` block contributes a circle of
//! radius `size * tilesize * 2` with a warm accent tint. Replace with the
//! reconciled fields when plan 02 exposes them.

use godot::classes::{Image, ImageTexture, Material, Mesh, MeshInstance2D, Node2D, ShaderMaterial};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::draw_meta::BlockDrawMeta;
use mind_core::render::light::{LightAccumulator, enabled};
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

/// `LightRenderer` — world light map + `Shaders.light` composite.
pub struct LightRenderer {
    host: Gd<MindSimHost>,
    node: Gd<MeshInstance2D>,
    material: Option<Gd<ShaderMaterial>>,
    accumulator: LightAccumulator,
    last_revision: i64,
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
            stats: LightStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> LightStats {
        self.stats
    }

    /// Rebuilds the light map when the world changed, then composites.
    pub fn update(
        &mut self,
        view: &CameraView,
        lighting: bool,
        ambient_alpha: f32,
        draw_light: bool,
    ) {
        let active = enabled(lighting, ambient_alpha, draw_light) && self.material.is_some();
        self.stats.visible = active;
        if !active {
            self.node.set_visible(false);
            return;
        }
        self.node.set_visible(true);

        let revision = self.host.clone().bind().world_revision();
        if revision == self.last_revision {
            return;
        }
        self.last_revision = revision;

        let (width, height, bytes, lights) = {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return;
            };
            let width = world.width();
            let height = world.height();
            if width <= 0 || height <= 0 {
                return;
            }
            self.accumulator.clear();
            let size = TILESIZE as f32;
            let x1 = ((view.x - view.w / 2.0) / size).floor() as i32;
            let y1 = ((view.y - view.h / 2.0) / size).floor() as i32;
            let x2 = ((view.x + view.w / 2.0) / size).ceil() as i32;
            let y2 = ((view.y + view.h / 2.0) / size).ceil() as i32;
            for ty in y1.max(0)..y2.min(height) {
                for tx in x1.max(0)..x2.min(width) {
                    let tile = world.tile(tx, ty);
                    let Some(def) = content.block(tile.block) else {
                        continue;
                    };
                    if !BlockDrawMeta::from_def(def).emit_light {
                        continue;
                    }
                    self.accumulator.add(
                        (tx as f32 + 0.5) * size,
                        (ty as f32 + 0.5) * size,
                        def.size.max(1) as f32 * size * 2.0,
                        [1.0, 0.92, 0.75, 1.0],
                        1.0,
                    );
                }
            }
            let mut alpha = vec![0.0f32; (width * height) as usize];
            for light in self.accumulator.circles() {
                stamp_circle(&mut alpha, width, height, light.x, light.y, light.radius);
            }
            let mut bytes = Vec::with_capacity(alpha.len() * 4);
            for a in &alpha {
                let v = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
                bytes.extend_from_slice(&[255, 255, 255, v]);
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
            &Color::from_rgba(0.01, 0.01, 0.04, 0.99).to_variant(),
        );
        if let Some(mesh) = build_mesh(&[world_quad(width, height)], 0.0) {
            self.node.set_mesh(&mesh.upcast::<Mesh>());
            self.node.set_material(&material.upcast::<Material>());
        }
        self.stats.rebuilds += 1;
        self.stats.lights = lights;
    }
}

/// Adds a radial falloff into a normalized `width`×`height` float map.
fn stamp_circle(map: &mut [f32], width: i32, height: i32, cx: f32, cy: f32, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    let unit = TILESIZE as f32;
    let x1 = ((cx - radius) / unit).floor() as i32;
    let y1 = ((cy - radius) / unit).floor() as i32;
    let x2 = ((cx + radius) / unit).ceil() as i32;
    let y2 = ((cy + radius) / unit).ceil() as i32;
    for y in y1.max(0)..y2.min(height) {
        for x in x1.max(0)..x2.min(width) {
            let dx = (x as f32 + 0.5) * unit - cx;
            let dy = (y as f32 + 0.5) * unit - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d >= radius {
                continue;
            }
            let falloff = 1.0 - d / radius;
            let index = (x + y * width) as usize;
            map[index] = (map[index] + falloff).clamp(0.0, 1.0);
        }
    }
}

/// World-covering quad in world pixels.
fn world_quad(width: i32, height: i32) -> Quad {
    let size = TILESIZE as f32;
    Quad {
        cx: width as f32 * size / 2.0,
        cy: height as f32 * size / 2.0,
        w: width as f32 * size,
        h: height as f32 * size,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: 0xffff_ffff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_circle_is_brightest_at_tile_center() {
        let mut map = vec![0.0f32; 16];
        // Tile (1,1) centre is world (12,12); stamp exactly there.
        stamp_circle(&mut map, 4, 4, 12.0, 12.0, 16.0);
        assert!((map[1 + 4] - 1.0).abs() < 1e-3);
        assert!(map.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
