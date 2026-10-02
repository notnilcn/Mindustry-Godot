// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ShadowRenderer` — shadow + darkness data maps and `Shaders.darkness`
//! composites (plan 16 §3.6 / M3).
//!
//! The upstream `FrameBuffer`s are ported as 1 px/tile `Image`-backed
//! `ImageTexture`s (D16-9 fallback: a `SubViewport` is not required for a
//! deterministic data texture, and the CPU path is headless-testable). Both
//! composites run through the ported `darkness` shader (`Renderer.java:516`,
//! `BlockRenderer.drawDarkness`).
//!
//! All tile math lives in `mind_core::render::shadow`; this module only owns
//! the Godot textures and the mesh nodes.

use godot::classes::{Image, ImageTexture, Material, Mesh, MeshInstance2D, Node2D, ShaderMaterial};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::BlockDrawMeta;
use mind_core::render::shadow::{build_darkness_map, build_shadow_map, shadow_tile_color};
use mind_core::world::darkness::get_static_darkness;

use crate::sim_host::MindSimHost;

use super::atlas_bind::{Quad, build_mesh};
use super::shaders::ShaderRegistry;

/// Shadow/darkness counters mirrored into `MindWorldRenderer`'s stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShadowStats {
    /// Shadow-map rebuilds since boot.
    pub shadow_rebuilds: i64,
    /// Darkness-map rebuilds since boot.
    pub darkness_rebuilds: i64,
    /// Shadow events recorded in the last rebuild.
    pub shadow_events: i64,
}

/// Shadow + darkness composite pass.
pub struct ShadowRenderer {
    host: Gd<MindSimHost>,
    shadow_node: Gd<MeshInstance2D>,
    dark_node: Gd<MeshInstance2D>,
    shadow_material: Option<Gd<ShaderMaterial>>,
    dark_material: Option<Gd<ShaderMaterial>>,
    last_revision: i64,
    stats: ShadowStats,
}

impl ShadowRenderer {
    /// Builds the pass over the shadow (`blockUnder`) and darkness bands.
    pub fn new(
        host: Gd<MindSimHost>,
        shadow_band: Gd<Node2D>,
        dark_band: Gd<Node2D>,
        shaders: &ShaderRegistry,
    ) -> Self {
        let make_material = || {
            shaders.get("darkness").map(|shader| {
                let mut material = ShaderMaterial::new_gd();
                material.set_shader(&shader);
                material
            })
        };
        let shadow_material = make_material();
        let dark_material = make_material();
        let shadow_node = make_quad_node(&shadow_band);
        let dark_node = make_quad_node(&dark_band);
        Self {
            host,
            shadow_node,
            dark_node,
            shadow_material,
            dark_material,
            last_revision: i64::MIN,
            stats: ShadowStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> ShadowStats {
        self.stats
    }

    /// Rebuilds the maps when the world changed, then composites.
    pub fn update(&mut self, force: bool) {
        let revision = self.host.clone().bind().world_revision();
        if !force && revision == self.last_revision {
            return;
        }
        self.last_revision = revision;

        let Some(mut shadow_material) = self.shadow_material.clone() else {
            return;
        };
        let Some(mut dark_material) = self.dark_material.clone() else {
            return;
        };

        let (width, height, shadow_pixels, dark_pixels, event_count) = {
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

            // Shadow events: static shadow casters (plan 02 `hasShadow` gap is
            // approximated by `is_static_kind` until `BlockDef` carries it).
            let mut events: Vec<(i32, i32, [f32; 4])> = Vec::new();
            for (pos, _) in world.iter_row_major() {
                let x = pos.x() as i32;
                let y = pos.y() as i32;
                let tile = world.tile(x, y);
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                if !BlockDrawMeta::from_def(def).display_shadow {
                    continue;
                }
                let color =
                    shadow_tile_color(true, false, tile.build.is_some(), true, false, true, false);
                events.push((x, y, color));
            }
            let shadow_pixels = build_shadow_map(width, height, &events);
            let event_count = events.len() as i64;

            let dark_pixels = build_darkness_map(width, height, None, |x, y| {
                get_static_darkness(world, content, x, y)
            });
            (width, height, shadow_pixels, dark_pixels, event_count)
        };

        let shadow_texture = make_texture(width, height, &shadow_pixels);
        let dark_texture = make_texture(width, height, &dark_pixels);
        if let (Some(shadow_texture), Some(dark_texture)) = (shadow_texture, dark_texture) {
            shadow_material.set_shader_parameter("u_texture", &shadow_texture.to_variant());
            dark_material.set_shader_parameter("u_texture", &dark_texture.to_variant());
            if let Some(mesh) = build_mesh(&[world_quad(width, height)], 0.0) {
                let mesh = mesh.upcast::<Mesh>();
                self.shadow_node.set_mesh(&mesh);
                self.dark_node.set_mesh(&mesh);
            }
            self.shadow_node
                .set_material(&shadow_material.upcast::<Material>());
            self.dark_node
                .set_material(&dark_material.upcast::<Material>());
        }
        self.stats.shadow_rebuilds += 1;
        self.stats.darkness_rebuilds += 1;
        self.stats.shadow_events = event_count;
    }
}

/// Builds the world-covering composite quad in world pixels.
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

/// Encodes an f32 RGBA map to an RGBA8 `ImageTexture`.
fn make_texture(width: i32, height: i32, pixels: &[[f32; 4]]) -> Option<Gd<ImageTexture>> {
    let mut bytes = Vec::with_capacity(pixels.len() * 4);
    for pixel in pixels {
        for channel in pixel {
            bytes.push((channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    let data = PackedByteArray::from(bytes);
    let image = Image::create_from_data(
        width,
        height,
        false,
        godot::classes::image::Format::RGBA8,
        &data,
    )?;
    ImageTexture::create_from_image(&image)
}

/// Creates a mesh node under a band.
fn make_quad_node(band: &Gd<Node2D>) -> Gd<MeshInstance2D> {
    let node = MeshInstance2D::new_alloc();
    // code-instantiated: one composite quad per shadow/darkness band; the node
    // count is fixed by the pipeline, not by content.
    band.clone().add_child(&node);
    node
}
