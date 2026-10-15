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
/// Resolves a region's atlas geometry, falling back to the numbered sprite
/// variants (`name1..name3`) that upstream `Floor.variantRegions` /
/// `StaticWall.variantRegions` load (`Floor.load`: `atlas.find(name + (i + 1))`).
/// The bare region wins when the atlas ships one (buildings, effects).
pub fn region_geometry_with_variants(
    assets: &MindAssets,
    name: &str,
) -> godot::builtin::PackedInt32Array {
    for candidate in variant_region_candidates(name) {
        let geometry = assets.region_geometry(GString::from(candidate.as_str()));
        if !geometry.is_empty() {
            return geometry;
        }
    }
    godot::builtin::PackedInt32Array::new()
}

/// Candidate atlas region names for a content region: the bare name first, then
/// the upstream numbered variants (`Floor.variantRegions`/`StaticWall` ship
/// `name1..name3`; the content registry stores only the bare name).
pub fn variant_region_candidates(name: &str) -> [String; 4] {
    [
        name.to_owned(),
        format!("{name}1"),
        format!("{name}2"),
        format!("{name}3"),
    ]
}

/// A resolved atlas region.
#[derive(Clone)]
pub struct ResolvedRegion {
    /// Manifest page index.
    pub page: i32,
    /// Page texture (absent when no atlas is loaded).
    pub texture: Option<Gd<Texture2D>>,
    /// Normalized `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
    /// Region width in pixels.
    pub w: f32,
    /// Region height in pixels.
    pub h: f32,
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

    /// Resolves a region without the `error` fallback (cached; `None` when
    /// absent), for callers that walk a candidate chain (`Core.atlas.find`
    /// with `isFound` checks).
    pub fn find(&mut self, name: &str) -> Option<ResolvedRegion> {
        let key = format!("\u{0}{name}");
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let resolved = self.lookup_with(name, false);
        self.cache.insert(key, resolved.clone());
        resolved
    }

    fn lookup(&mut self, name: &str) -> Option<ResolvedRegion> {
        self.lookup_with(name, true)
    }

    fn lookup_with(&mut self, name: &str, fallback: bool) -> Option<ResolvedRegion> {
        let assets = self.assets.clone()?;
        let assets = assets.bind();
        let mut geometry = region_geometry_with_variants(&assets, name);
        if geometry.is_empty() && !fallback {
            // Optional region (team overlay, outline, ...): callers walk a
            // chain, so a miss is normal and only interesting when debugging.
            log::debug!("atlas find miss: `{name}`");
            return None;
        }
        if geometry.is_empty() && name != ERROR_REGION {
            log::warn!("atlas error fallback: `{name}`");
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
            w: slice[2] as f32,
            h: slice[3] as f32,
        })
    }
}

/// Whether a quad slice holds anything to draw (I02 T2). `SurfaceTool.commit()`
/// with no vertices still returns a valid `ArrayMesh` with **0 surfaces**, so
/// every mesh builder returns `None` for an empty slice and the passes keep
/// their `MeshInstance2D` hidden instead of assigning an empty mesh (a visible
/// mesh node with no mesh is what the `mesh_get_surface_count` errors come
/// from; H03 T14).
pub(crate) fn has_mesh_quads<T>(quads: &[T]) -> bool {
    !quads.is_empty()
}

/// Builds an `ArrayMesh` from quads (two triangles each).
pub fn build_mesh(quads: &[Quad], grow: f32, flip_v: bool) -> Option<Gd<ArrayMesh>> {
    if !has_mesh_quads(quads) {
        return None;
    }
    let mut tool = SurfaceTool::new_gd();
    tool.begin(PrimitiveType::TRIANGLES);
    for quad in quads {
        let x = quad.cx - quad.w / 2.0 - grow;
        let y = quad.cy - quad.h / 2.0 - grow;
        let w = quad.w + grow * 2.0;
        let h = quad.h + grow * 2.0;
        let color = color_from(quad.color);
        let [u0, v0, u1, v1] = quad.uv;
        // World tile sprites are drawn through the y-flip camera (round 3), so
        // their v axis is swapped to keep the sprite upright; screen-space
        // overlays (fog, underwater) pass `flip_v = false`.
        let (v_top, v_bottom) = if flip_v { (v1, v0) } else { (v0, v1) };
        let corners = [
            (x, y, u0, v_top),
            (x + w, y, u1, v_top),
            (x + w, y + h, u1, v_bottom),
            (x, y + h, u0, v_bottom),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_candidates_prefer_bare_then_numbered() {
        assert_eq!(
            variant_region_candidates("snow"),
            ["snow", "snow1", "snow2", "snow3"]
        );
        assert_eq!(
            variant_region_candidates("ice-wall"),
            ["ice-wall", "ice-wall1", "ice-wall2", "ice-wall3"]
        );
    }

    /// I02 T2: the builders return `None` for an empty slice, and the passes
    /// read this helper to hide their node instead of assigning an empty mesh.
    #[test]
    fn empty_quad_slice_has_no_mesh_surface() {
        assert!(!has_mesh_quads::<Quad>(&[]));
        let quad = Quad {
            cx: 0.0,
            cy: 0.0,
            w: 8.0,
            h: 8.0,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: 0xffff_ffff,
        };
        assert!(has_mesh_quads(std::slice::from_ref(&quad)));
    }
}
