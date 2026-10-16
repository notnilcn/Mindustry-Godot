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

/// `Drawf.target` (`Drawf.java:279-289`): a `Lines.poly` square at radius
/// `rad` rotated `rot` plus 4 `Lines.spikes` from `3/7 rad` to `9/7 rad`, each
/// drawn twice — stroke 3 grey (`Pal.gray`, alpha `alpha`) then stroke 1
/// `color` (alpha `alpha`).
pub(crate) fn target(
    canvas: &mut Node2D,
    x: f32,
    y: f32,
    rad: f32,
    rot: f32,
    alpha: f32,
    color: Color,
) {
    let mut gray = color_f4(pal::GRAY);
    gray.a = alpha;
    let mut color = color;
    color.a = alpha;
    let mut ring = drawf::poly_ring(x, y, 4, rad, rot);
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    let packed = pack_points(&ring);
    let spikes = target_spikes(x, y, rad, rot);
    canvas.draw_polyline_ex(&packed, gray).width(3.0).done();
    for (x1, y1, x2, y2) in spikes {
        canvas
            .draw_line_ex(Vector2::new(x1, y1), Vector2::new(x2, y2), gray)
            .width(3.0)
            .done();
    }
    canvas.draw_polyline_ex(&packed, color).width(1.0).done();
    for (x1, y1, x2, y2) in spikes {
        canvas
            .draw_line_ex(Vector2::new(x1, y1), Vector2::new(x2, y2), color)
            .width(1.0)
            .done();
    }
}

/// `Lines.spikes(x, y, radius, length, 4, rot)` (`Lines.java:372-383`): the 4
/// spike segments of [`target`] as `(x1, y1, x2, y2)`; spike `i` starts at
/// `trns(i*90 + rot, 3/7*rad)` and ends at `trns(i*90 + rot, 9/7*rad)`.
pub(crate) fn target_spikes(x: f32, y: f32, rad: f32, rot: f32) -> [(f32, f32, f32, f32); 4] {
    let radius = 3.0 / 7.0 * rad;
    let length = 6.0 / 7.0 * rad;
    std::array::from_fn(|i| {
        let angle = (i as f32 * 90.0 + rot).to_radians();
        let (sin, cos) = angle.sin_cos();
        (
            x + cos * radius,
            y + sin * radius,
            x + cos * (radius + length),
            y + sin * (radius + length),
        )
    })
}

/// `Lines.circle(x, y, rad)` at `stroke` (single colour; the overlay-top core
/// radius and power-node rings use `Lines.stroke` per call). Side count as
/// [`circles`].
pub(crate) fn circle_stroke(
    canvas: &mut Node2D,
    x: f32,
    y: f32,
    r: f32,
    stroke: f32,
    color: Color,
) {
    let sides = 11 + (r * 0.4) as u32;
    let mut ring = drawf::poly_ring(x, y, sides.max(3), r, 0.0);
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    let packed = pack_points(&ring);
    canvas
        .draw_polyline_ex(&packed, color)
        .width(stroke.max(0.5))
        .done();
}

/// `Lines.dashCircle(x, y, radius)` at `stroke` (single colour; the drop-zone
/// circle draws `Lines.stroke(2)` then this).
pub(crate) fn dash_circle_stroke(
    canvas: &mut Node2D,
    x: f32,
    y: f32,
    r: f32,
    stroke: f32,
    color: Color,
) {
    for s in drawf::dash_circle_segments(x, y, r) {
        canvas
            .draw_line_ex(Vector2::new(s[0], s[1]), Vector2::new(s[2], s[3]), color)
            .width(stroke.max(0.5))
            .done();
    }
}

/// `Drawf.square(x, y, radius, rotation, color)` (`Drawf.java:426-438`):
/// stroke 3 `Pal.gray` (alpha = color alpha) then stroke 1 `color`, both on
/// `Lines.square(x, y, radius + 1, rotation)` = `poly(.., rot - 45)`.
pub(crate) fn square(canvas: &mut Node2D, x: f32, y: f32, r: f32, rot: f32, color: Color) {
    let mut gray = color_f4(pal::GRAY);
    gray.a = color.a;
    let mut ring = drawf::poly_ring(x, y, 4, r + 1.0, rot - 45.0);
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    let packed = pack_points(&ring);
    canvas.draw_polyline_ex(&packed, gray).width(3.0).done();
    canvas.draw_polyline_ex(&packed, color).width(1.0).done();
}

/// `Drawf.select(x, y, radius, color)` (`Drawf.java:418-424`): stroke 3 gray
/// axis-aligned square half-width `radius + 1`, then stroke 1 `color` at
/// `radius`.
pub(crate) fn select(canvas: &mut Node2D, x: f32, y: f32, r: f32, color: Color) {
    let gray = color_f4(pal::GRAY);
    let mut outer = drawf::poly_ring(x, y, 4, r + 1.0, -45.0);
    if let Some(first) = outer.first().copied() {
        outer.push(first);
    }
    let packed_outer = pack_points(&outer);
    canvas
        .draw_polyline_ex(&packed_outer, gray)
        .width(3.0)
        .done();
    let mut inner = drawf::poly_ring(x, y, 4, r, -45.0);
    if let Some(first) = inner.first().copied() {
        inner.push(first);
    }
    let packed_inner = pack_points(&inner);
    canvas
        .draw_polyline_ex(&packed_inner, color)
        .width(1.0)
        .done();
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

/// `Fill.circle(x, y, radius)` (`Fill.java:120-124`): a filled polygon with
/// `Lines.circleVertices` sides (`Lines.java:24-26`).
pub(crate) fn fill_circle(canvas: &mut Node2D, x: f32, y: f32, r: f32, color: Color) {
    if r <= 0.0 {
        return;
    }
    let sides = (11 + (r * 0.4) as u32).max(3);
    let ring = drawf::poly_ring(x, y, sides, r, 0.0);
    let packed = pack_points(&ring);
    canvas.draw_colored_polygon(&packed, color);
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

/// World-space visible rect `(min x, min y, w, h)` from the `/root/Spine/World/
/// Camera2D` transform (`BlockExtras`/turret pass cull helper).
pub(crate) fn visible_world_rect(canvas: &Node2D) -> Option<(f32, f32, f32, f32)> {
    let camera =
        canvas.try_get_node_as::<godot::classes::Camera2D>("/root/Spine/World/Camera2D")?;
    let viewport = canvas.get_viewport()?;
    let rect = viewport.get_visible_rect();
    let inverse = camera.get_canvas_transform().affine_inverse();
    let corners = [
        inverse * rect.position,
        inverse * (rect.position + Vector2::new(rect.size.x, 0.0)),
        inverse * rect.position + rect.size,
        inverse * (rect.position + Vector2::new(0.0, rect.size.y)),
    ];
    let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_x = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    Some((min_x, min_y, max_x - min_x, max_y - min_y))
}

/// `Fill.light` (`Fill.java:171-191`): a triangle fan with `center` at the
/// hub and `edge` on the rim (`sides` rounded up to even by Arc).
pub(crate) fn fill_light(
    canvas: &mut Node2D,
    x: f32,
    y: f32,
    sides: u32,
    radius: f32,
    center: Color,
    edge: Color,
) {
    if radius <= 0.0 {
        return;
    }
    let ring = drawf::poly_ring(x, y, sides.max(3), radius, 0.0);
    let mut points = PackedVector2Array::new();
    let mut colors = PackedColorArray::new();
    points.push(Vector2::new(x, y));
    colors.push(center);
    for (px, py) in &ring {
        points.push(Vector2::new(*px, *py));
        colors.push(edge);
    }
    canvas.draw_polygon(&points, &colors);
}

/// `[(f32, f32)]` → `PackedVector2Array`.
fn pack_points(pts: &[(f32, f32)]) -> PackedVector2Array {
    pts.iter()
        .map(|(x, y)| Vector2::new(*x, *y))
        .collect::<Vec<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::target_spikes;

    #[test]
    fn target_spike_geometry_matches_lines() {
        // rad 7, rot 0: radius 3/7*7 = 3, length 6/7*7 = 6; the first spike
        // runs from (x+3, y) to (x+9, y) (`Lines.spikes`).
        let spikes = target_spikes(0.0, 0.0, 7.0, 0.0);
        assert!((spikes[0].0 - 3.0).abs() < 1e-5);
        assert!((spikes[0].1 - 0.0).abs() < 1e-5);
        assert!((spikes[0].2 - 9.0).abs() < 1e-5);
        assert!((spikes[0].3 - 0.0).abs() < 1e-5);
        // Second spike at 90 degrees: (x, y+3)..(x, y+9) in Java (CCW).
        assert!((spikes[1].0 - 0.0).abs() < 1e-5);
        assert!((spikes[1].1 - 3.0).abs() < 1e-5);
        assert!((spikes[1].2 - 0.0).abs() < 1e-5);
        assert!((spikes[1].3 - 9.0).abs() < 1e-5);
    }
}
