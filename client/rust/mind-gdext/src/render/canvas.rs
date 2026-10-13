// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared world-space canvas helpers for the custom-draw passes (G03 T1).
//!
//! The port's camera renders `+y` up (`gdext/camera.rs` sets a negative y
//! zoom) while textures are stored top-row-first, so every region drawn from a
//! world-space `Node2D` needs a vertical flip; [`draw_region`] is the only
//! place that is expressed (a negative-height rect, `tile = false`). These
//! functions mirror the Arc/Mindustry helpers of the same name cited below and
//! are called from inside a pass's `draw()` with `&mut self.base_mut()`.

// Consumed by the G03 T2+ render passes (`plans`, `links`, `overlay_top`, ...);
// landed one commit before its first user (T1).
#![allow(dead_code)]

use std::collections::HashMap;

use godot::builtin::{Color, PackedVector2Array, Rect2, Vector2};
use godot::classes::{Node2D, Texture2D};
use godot::prelude::*;

use mind_core::render::drawf;
use mind_core::render::drawf::pal;

/// Draws `texture` centred at `(x, y)`, size `(w, h)` world units, rotated
/// `rot_deg` degrees (Java/Arc convention, CCW, 0 = +x), modulated by `color`,
/// compensating the camera y-flip.
#[allow(clippy::too_many_arguments)] // signature fixed by the G03 T1 plan
pub(crate) fn draw_region(
    canvas: &mut Node2D,
    texture: &Gd<Texture2D>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rot_deg: f32,
    color: Color,
) {
    canvas
        .draw_set_transform_ex(Vector2::new(x, y))
        .rotation(rot_deg.to_radians())
        .done();
    // Negative height = vertical flip: the camera renders +y up, textures are
    // top-row-first. `draw_texture_rect` builds the quad as `[position,
    // position + |size|]`, so the position is the quad's minimum corner
    // (`-h / 2`), not the Java-style bottom-left.
    canvas
        .draw_texture_rect_ex(
            texture,
            Rect2::new(Vector2::new(-w / 2.0, -h / 2.0), Vector2::new(w, -h)),
            false,
        )
        .modulate(color)
        .done();
    canvas.draw_set_transform(Vector2::ZERO);
}

/// `Lines.line(region, x, y, x2, y2, false)` with `stroke` (`Lines.java:70`):
/// the region is stretched along the line; drawn through [`draw_region`] at the
/// midpoint.
#[allow(clippy::too_many_arguments)] // signature fixed by the G03 T1 plan
pub(crate) fn draw_region_line(
    canvas: &mut Node2D,
    texture: &Gd<Texture2D>,
    x: f32,
    y: f32,
    x2: f32,
    y2: f32,
    stroke: f32,
    color: Color,
) {
    let dx = x2 - x;
    let dy = y2 - y;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return;
    }
    let rot = dy.atan2(dx).to_degrees();
    draw_region(
        canvas,
        texture,
        (x + x2) * 0.5,
        (y + y2) * 0.5,
        len,
        stroke,
        rot,
        color,
    );
}

/// `Drawf.dashCircle(x, y, rad, color)` (`Drawf.java:398-404`): stroke 3
/// `Pal.gray` dash circle, then stroke 1 `color`.
pub(crate) fn dash_circle(canvas: &mut Node2D, x: f32, y: f32, r: f32, color: Color) {
    let segments = drawf::dash_circle_segments(x, y, r);
    let gray = color_f4(pal::GRAY);
    for s in &segments {
        canvas
            .draw_line_ex(Vector2::new(s[0], s[1]), Vector2::new(s[2], s[3]), gray)
            .width(3.0)
            .done();
    }
    for s in &segments {
        canvas
            .draw_line_ex(Vector2::new(s[0], s[1]), Vector2::new(s[2], s[3]), color)
            .width(1.0)
            .done();
    }
}

/// `Drawf.circles(x, y, rad, color)` (`Drawf.java:410-415`): stroke 3
/// `Pal.gray` circle, then stroke 1 `color`. Side count `Lines.circleVertices`
/// (`Lines.java:24-26`).
pub(crate) fn circles(canvas: &mut Node2D, x: f32, y: f32, r: f32, color: Color) {
    let sides = 11 + (r * 0.4) as u32;
    let ring = drawf::poly_ring(x, y, sides.max(3), r, 0.0);
    let gray = color_f4(pal::GRAY);
    let packed = pack_points(&ring);
    canvas.draw_polyline_ex(&packed, gray).width(3.0).done();
    canvas.draw_polyline_ex(&packed, color).width(1.0).done();
}

/// `Lines.poly` outline: a polyline through `pts` with the given `stroke`.
pub(crate) fn poly_line(canvas: &mut Node2D, pts: &[(f32, f32)], stroke: f32, color: Color) {
    if pts.len() < 2 {
        return;
    }
    let packed = pack_points(pts);
    canvas
        .draw_polyline_ex(&packed, color)
        .width(stroke.max(1.0))
        .done();
}

/// `Lines.rect(x, y, width, height)` (`Lines.java:511-522`): four filled edge
/// rects of thickness `stroke`.
pub(crate) fn rect_line(
    canvas: &mut Node2D,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    stroke: f32,
    color: Color,
) {
    fill_rect(canvas, x, y, x + w, y + stroke, color);
    fill_rect(canvas, x, y + h - stroke, x + w, y + h, color);
    fill_rect(canvas, x + w - stroke, y, x + w, y + h, color);
    fill_rect(canvas, x, y, x + stroke, y + h, color);
}

/// `Fill.tri(x1, y1, x2, y2, x3, y3)`.
pub(crate) fn fill_tri(canvas: &mut Node2D, pts: [(f32, f32); 3], color: Color) {
    let mut packed = PackedVector2Array::new();
    for (x, y) in pts {
        packed.push(Vector2::new(x, y));
    }
    canvas.draw_colored_polygon(&packed, color);
}

/// `Fill.crect(x, y, w, h)`, accepting either orientation.
pub(crate) fn fill_rect(canvas: &mut Node2D, x1: f32, y1: f32, x2: f32, y2: f32, color: Color) {
    let x = x1.min(x2);
    let y = y1.min(y2);
    let w = (x2 - x1).abs();
    let h = (y2 - y1).abs();
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    canvas
        .draw_rect_ex(Rect2::new(Vector2::new(x, y), Vector2::new(w, h)), color)
        .filled(true)
        .done();
}

/// RGBA8888 → `Color`.
pub(crate) fn color_u32(rgba: u32) -> Color {
    Color::from_rgba8(
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        (rgba & 0xff) as u8,
    )
}

/// `[f32; 4]` RGBA (0..1) → `Color`.
pub(crate) fn color_f4(c: [f32; 4]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], c[3])
}

/// Name → `AtlasTexture` cache over `/root/MindAssets` (`find_region`).
pub(crate) struct RegionCache {
    cache: HashMap<String, Option<Gd<Texture2D>>>,
}

impl Default for RegionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Resolves `name` exactly like `MindFx::region_texture` (fx.rs): the
    /// `MindAssets` autoload's `find_region`, cached per name (misses too).
    /// Call this **before** taking `base_mut()` in `draw()`.
    pub fn get(&mut self, node: &Node2D, name: &str) -> Option<Gd<Texture2D>> {
        if let Some(cached) = self.cache.get(name) {
            return cached.clone();
        }
        let texture = node
            .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets")
            .and_then(|mut assets| {
                assets
                    .bind_mut()
                    .find_region(GString::from(name))
                    .map(|texture| texture.upcast::<Texture2D>())
            });
        self.cache.insert(name.to_string(), texture.clone());
        texture
    }

    /// Natural world size of a region: `pixels * 0.25` (the atlas packs at
    /// 4 px per world unit, `scale_marker` = 4x4 px).
    pub fn natural_size(tex: &Gd<Texture2D>) -> (f32, f32) {
        let size = tex.get_size();
        (size.x * 0.25, size.y * 0.25)
    }
}

/// `[(f32, f32)]` → `PackedVector2Array`.
fn pack_points(pts: &[(f32, f32)]) -> PackedVector2Array {
    pts.iter()
        .map(|(x, y)| Vector2::new(*x, *y))
        .collect::<Vec<_>>()
        .into_iter()
        .collect()
}
