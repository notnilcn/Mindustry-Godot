// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared plan-03 atlas binding for the chunk/block mesh bakers (plan 16 §3.5/
//! §3.6). Resolves a region name to its page texture + normalized UVs, with the
//! upstream `error` fallback, and builds a `SurfaceTool` quad mesh. The
//! resolver is append-only per frame so steady-state frames do not intern
//! strings (plan 16 §7.4).

use std::collections::HashMap;

use godot::classes::mesh::PrimitiveType;
use godot::classes::{ArrayMesh, SurfaceTool, Texture2D};
use godot::obj::NewGd;
use godot::prelude::*;

use crate::assets::MindAssets;

/// `FloorRenderer` error fallback region.
pub const ERROR_REGION: &str = "env-error";

/// A resolved atlas region.
#[derive(Clone)]
pub struct ResolvedRegion {
    /// Manifest page index.
    pub page: i32,
    /// Page texture (absent when no atlas is loaded).
    pub texture: Option<Gd<Texture2D>>,
    /// Normalized `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
}

/// One baked quad.
#[derive(Clone, Copy)]
pub struct Quad {
    /// Center x in world pixels.
    pub cx: f32,
    /// Center y in world pixels.
    pub cy: f32,
    /// Quad width.
    pub w: f32,
    /// Quad height.
    pub h: f32,
    /// UV rect.
    pub uv: [f32; 4],
    /// RGBA8888 tint.
    pub color: u32,
}

/// Region-name → resolved geometry cache over `MindAssets`.
pub struct RegionResolver {
    assets: Option<Gd<MindAssets>>,
    cache: HashMap<String, Option<ResolvedRegion>>,
    missing: u64,
}

impl RegionResolver {
    /// Builds a resolver over the optional atlas autoload.
    pub fn new(assets: Option<Gd<MindAssets>>) -> Self {
        Self {
            assets,
            cache: HashMap::new(),
            missing: 0,
        }
    }

    /// Number of regions that fell back to `error`/placeholder.
    pub fn missing(&self) -> u64 {
        self.missing
    }

    /// Resolves a region name (cached; `error` fallback; `None` = placeholder).
    pub fn resolve(&mut self, name: &str) -> Option<ResolvedRegion> {
        if let Some(cached) = self.cache.get(name) {
            return cached.clone();
        }
        let resolved = self.lookup(name);
        self.cache.insert(name.to_owned(), resolved.clone());
        resolved
    }

    fn lookup(&mut self, name: &str) -> Option<ResolvedRegion> {
        let assets = self.assets.clone()?;
        let assets = assets.bind();
        let mut geometry = assets.region_geometry(GString::from(name));
        if geometry.is_empty() && name != ERROR_REGION {
            self.missing += 1;
            geometry = assets.region_geometry(GString::from(ERROR_REGION));
            if geometry.is_empty() {
                geometry = assets.region_geometry(GString::from("error"));
            }
        }
        if geometry.len() < 5 {
            return None;
        }
        let slice = geometry.as_slice();
        let page = slice[4];
        let texture = assets
            .page_texture(page as i64)
            .map(|texture| texture.upcast::<Texture2D>());
        let (width, height) = texture
            .as_ref()
            .map(|texture| {
                (
                    texture.get_width().max(1) as f32,
                    texture.get_height().max(1) as f32,
                )
            })
            .unwrap_or((1.0, 1.0));
        Some(ResolvedRegion {
            page,
            texture,
            uv: [
                slice[0] as f32 / width,
                slice[1] as f32 / height,
                (slice[0] + slice[2]) as f32 / width,
                (slice[1] + slice[3]) as f32 / height,
            ],
        })
    }
}

/// Builds an `ArrayMesh` from quads (two triangles each).
pub fn build_mesh(quads: &[Quad], grow: f32) -> Option<Gd<ArrayMesh>> {
    let mut tool = SurfaceTool::new_gd();
    tool.begin(PrimitiveType::TRIANGLES);
    for quad in quads {
        let x = quad.cx - quad.w / 2.0 - grow;
        let y = quad.cy - quad.h / 2.0 - grow;
        let w = quad.w + grow * 2.0;
        let h = quad.h + grow * 2.0;
        let color = color_from(quad.color);
        let [u0, v0, u1, v1] = quad.uv;
        let corners = [
            (x, y, u0, v0),
            (x + w, y, u1, v0),
            (x + w, y + h, u1, v1),
            (x, y + h, u0, v1),
        ];
        for index in [0usize, 1, 2, 0, 2, 3] {
            let (vx, vy, u, v) = corners[index];
            tool.set_color(color);
            tool.set_uv(Vector2::new(u, v));
            tool.add_vertex(Vector3::new(vx, vy, 0.0));
        }
    }
    tool.commit()
}

/// RGBA8888 → `Color`.
pub fn color_from(rgba: u32) -> Color {
    Color::from_rgba8(
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        (rgba & 0xff) as u8,
    )
}
