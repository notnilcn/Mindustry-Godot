// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FogRenderer` host (`graphics/FogRenderer.java`, plan 16 §3.7/M6; H03 T12).
//!
//! Two world-sized CPU maps composite through `Shaders.fog`:
//! * **dynamic**: rebuilt every 2 frames at 1 texel per tile — filled 20-gon
//!   discs (`FogRenderer.poly`) for every player-team fog source
//!   (`fog_sources`: buildings with `hasFogRadius` + units, radius in world px),
//!   black outside, with the 1-texel border clip (`ScissorStack`).
//! * **static**: `fog_static_bytes(team)` (discovered tiles, 0/255) copied when
//!   its hash changes; drawn only under `rules.staticFog` (`copyFromCpu`).
//!
//! The composite mirrors `Draw.fbo(texture, w, h, tilesize[, tilesize/2])`:
//! a world-covering quad whose texel grid is `tilesize` px, offset 0 for the
//! dynamic map and half a tile for the static one. No fog data (H09
//! `fog_static_bytes` returns `None`) means nothing is drawn — the old opaque
//! placeholder quad is gone.

use godot::classes::{Image, ImageTexture, Material, Mesh, MeshInstance2D, Node2D, ShaderMaterial};
use godot::obj::NewGd;
use godot::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mind_core::config::TILESIZE;
use mind_core::render::fog::{fog_event, fog_poly};
use mind_core::render::rules::RulesRenderView;
use mind_core::render::scan::CameraView;

use crate::sim_host::MindSimHost;

use super::atlas_bind::{Quad, build_mesh};
use super::shaders::ShaderRegistry;

/// Fog pass counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct FogStats {
    /// Queued static fog events.
    pub events: i64,
    /// Whether the fog composite drew this frame.
    pub active: bool,
}

/// `FogRenderer` host.
pub struct FogRenderer {
    host: Gd<MindSimHost>,
    dynamic_node: Gd<MeshInstance2D>,
    static_node: Option<Gd<MeshInstance2D>>,
    dynamic_material: Option<Gd<ShaderMaterial>>,
    static_material: Option<Gd<ShaderMaterial>>,
    width: i32,
    height: i32,
    /// Hash of the last static bytes (`copyFromCpu` trigger).
    static_hash: u64,
    /// 2-frame dynamic rebuild cadence (`FogRenderer.drawFog`).
    frames: u64,
    events: Vec<i64>,
    stats: FogStats,
}

impl FogRenderer {
    /// Builds the pass over the `Layer::FogOfWar` band.
    pub fn new(host: Gd<MindSimHost>, band: Gd<Node2D>, shaders: &ShaderRegistry) -> Self {
        let dynamic_material = shaders.get("fog").map(|shader| {
            let mut material = ShaderMaterial::new_gd();
            material.set_shader(&shader);
            material
        });
        let static_material = shaders.get("fog").map(|shader| {
            let mut material = ShaderMaterial::new_gd();
            material.set_shader(&shader);
            material
        });
        let dynamic_node = MeshInstance2D::new_alloc();
        // code-instantiated: one dynamic fog quad; the node count is fixed by
        // the pipeline, not by content.
        band.clone().add_child(&dynamic_node);
        dynamic_node.clone().set_visible(false);
        let static_node = static_material.as_ref().map(|_| {
            let node = MeshInstance2D::new_alloc();
            // code-instantiated: one static fog quad (see above).
            band.clone().add_child(&node);
            node.clone().set_visible(false);
            node
        });
        Self {
            host,
            dynamic_node,
            static_node,
            dynamic_material,
            static_material,
            width: 0,
            height: 0,
            static_hash: 0,
            frames: 0,
            events: Vec::new(),
            stats: FogStats::default(),
        }
    }

    /// `FogRenderer.handleEvent(packed)` (`ClientHooks::fog_handle_event`).
    pub fn handle_event(&mut self, packed: i64) {
        self.events.push(packed);
    }

    /// Queues a fog event from tile coordinates.
    pub fn push_event(&mut self, x: i32, y: i32, radius: i32, team: u8) {
        self.handle_event(fog_event(x, y, radius, team));
    }

    /// Current counters.
    pub fn stats(&self) -> FogStats {
        self.stats
    }

    /// `FogRenderer.drawFog` (H03 T12).
    pub fn update(&mut self, _view: &CameraView, rules: &RulesRenderView) {
        self.stats.events = self.events.len() as i64;
        let team = self.host.clone().bind().player_team();
        let static_bytes = self.host.clone().bind().fog_static_bytes(team);
        // `if(fogControl.getDiscovered(team) == null) return;` — no fog data
        // means nothing is drawn (the pre-H09 opaque placeholder is gone).
        let Some((width, height, bytes)) = static_bytes else {
            self.stats.active = false;
            self.dynamic_node.set_visible(false);
            if let Some(mut node) = self.static_node.clone() {
                node.set_visible(false);
            }
            self.events.clear();
            return;
        };
        self.stats.active = rules.fog && width > 0 && height > 0;
        if !self.stats.active {
            self.dynamic_node.set_visible(false);
            if let Some(mut node) = self.static_node.clone() {
                node.set_visible(false);
            }
            self.events.clear();
            return;
        }
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.static_hash = 0;
        }
        self.frames = self.frames.wrapping_add(1);

        // Dynamic map: every 2 frames (`dynamicFog` block). Sources are world
        // px; the 1-tile border clip stays black.
        if self.dynamic_material.is_some() && self.frames.is_multiple_of(2) {
            let mut map = vec![0u8; (width * height * 4) as usize];
            let sources = self.host.clone().bind().fog_sources(team);
            for (x, y, radius) in sources {
                stamp_disc(
                    &mut map,
                    width,
                    height,
                    x / TILESIZE as f32,
                    y / TILESIZE as f32,
                    radius / TILESIZE as f32,
                );
            }
            if let Some(texture) = make_texture(width, height, &map) {
                self.attach_quad(
                    0,
                    width,
                    height,
                    0.0,
                    rules.dynamic_fog_color(),
                    &texture,
                    false,
                );
            }
        }

        // Static map: `copyFromCpu` when the discovered bits change (or the
        // world resized). Only under `rules.staticFog`.
        if rules.static_fog {
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            let hash = hasher.finish();
            if hash != self.static_hash {
                self.static_hash = hash;
                let mut map = vec![0u8; (width * height * 4) as usize];
                for y in 0..height {
                    for x in 0..width {
                        let value = bytes.get((y * width + x) as usize).copied().unwrap_or(0);
                        let index = ((y * width + x) * 4) as usize;
                        map[index] = value;
                        map[index + 1] = value;
                        map[index + 2] = value;
                        map[index + 3] = 0xff;
                    }
                }
                if let Some(texture) = make_texture(width, height, &map) {
                    self.attach_quad(
                        1,
                        width,
                        height,
                        -TILESIZE as f32 / 2.0,
                        rules.static_fog_color(),
                        &texture,
                        true,
                    );
                }
            }
            let visible = self.static_node.is_some();
            if let Some(mut node) = self.static_node.clone() {
                node.set_visible(visible);
            }
        } else if let Some(mut node) = self.static_node.clone() {
            node.set_visible(false);
        }
        self.dynamic_node.set_visible(true);
        // Events are consumed by `copyFromCpu` (the discovered bits already
        // contain them); drop the queue like Java's static pass does.
        self.events.clear();
    }

    /// Uploads one composite quad (`which`: 0 dynamic, 1 static).
    #[allow(clippy::too_many_arguments)]
    fn attach_quad(
        &mut self,
        which: u8,
        width: i32,
        height: i32,
        offset: f32,
        color: [f32; 4],
        texture: &Gd<ImageTexture>,
        _static: bool,
    ) {
        let (node, material) = if which == 0 {
            (self.dynamic_node.clone(), self.dynamic_material.clone())
        } else {
            let Some(node) = self.static_node.clone() else {
                return;
            };
            (node, self.static_material.clone())
        };
        let Some(mut material) = material else {
            return;
        };
        let mut node = node;
        material.set_shader_parameter("u_texture", &texture.to_variant());
        // `Draw.fbo(..., tilesize[, tilesize/2])`: the static map is sampled
        // half a tile over, i.e. its quad sits half a tile down-left.
        let quad = world_quad(width, height, offset);
        if let Some(mesh) = build_mesh(&[quad], 0.0, false) {
            node.set_mesh(&mesh.upcast::<Mesh>());
        }
        node.set_material(&material.upcast::<Material>());
        // The canvas renderer needs a texture on the item even though the
        // shader samples `u_texture` (light pass note, G03 T12).
        node.set_texture(&texture.clone().upcast::<godot::classes::Texture2D>());
        node.set_modulate(Color::from_rgba(color[0], color[1], color[2], color[3]));
        node.set_texture_filter(godot::classes::canvas_item::TextureFilter::LINEAR);
        node.set_visible(true);
    }
}

/// Fills the 20-gon `FogRenderer.poly(x, y, radius)` into the tile map
/// (value 255, alpha 255), clipping the outer 1-texel border like the Java
/// `ScissorStack` rect.
fn stamp_disc(map: &mut [u8], width: i32, height: i32, cx: f32, cy: f32, radius: f32) {
    let polygon = fog_poly(cx, cy, radius);
    let min_x = ((cx - radius).floor() as i32).max(1);
    let max_x = ((cx + radius).ceil() as i32).min(width - 2);
    let min_y = ((cy - radius).floor() as i32).max(1);
    let max_y = ((cy + radius).ceil() as i32).min(height - 2);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if !point_in_poly(x as f32 + 0.5, y as f32 + 0.5, &polygon) {
                continue;
            }
            let index = ((y * width + x) * 4) as usize;
            map[index] = 0xff;
            map[index + 1] = 0xff;
            map[index + 2] = 0xff;
            map[index + 3] = 0xff;
        }
    }
}

/// Even-odd point-in-polygon test.
fn point_in_poly(x: f32, y: f32, polygon: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let mut j = polygon.len().wrapping_sub(1);
    for i in 0..polygon.len() {
        let (xi, yi) = (polygon[i][0], polygon[i][1]);
        let (xj, yj) = (polygon[j][0], polygon[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// World-covering composite quad in world pixels (`Draw.fbo` coverage).
///
/// `offset` shifts the quad centre (`0` dynamic, `-tilesize/2` static — the
/// static buffer's tiles sit half a tile over, matching Java's extra
/// `tilesize/2` sampling offset).
fn world_quad(width: i32, height: i32, offset: f32) -> Quad {
    let size = TILESIZE as f32;
    Quad {
        cx: width as f32 * size / 2.0 + offset,
        cy: height as f32 * size / 2.0 + offset,
        w: width as f32 * size,
        h: height as f32 * size,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: 0xffff_ffff,
    }
}

/// RGBA8 bytes → `ImageTexture`.
fn make_texture(width: i32, height: i32, bytes: &[u8]) -> Option<Gd<ImageTexture>> {
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

#[cfg(test)]
mod tests {
    use super::point_in_poly;

    #[test]
    fn point_in_poly_square() {
        let square = [[0.0f32, 0.0f32], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];
        assert!(point_in_poly(1.0, 1.0, &square));
        assert!(!point_in_poly(3.0, 1.0, &square));
        assert!(!point_in_poly(-1.0, 1.0, &square));
    }
}
