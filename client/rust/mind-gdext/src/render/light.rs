// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LightRenderer` composite (`graphics/LightRenderer.java`, plan 16 §3.7 /
//! M5). The upstream 4×-downscaled additive target becomes a camera-sized
//! `Image` light map (deviation S16-4: a CPU map is deterministic and needs no
//! `RenderingDevice` blend-equation workaround under the locked `mobile`
//! renderer, `OD16-B`/`D16-3`).
//!
//! Every upstream primitive is ported: circle lights
//! (`LightRenderer.add(x,y,radius,color,opacity)`, pooled in
//! `mind_core::render::light`), line lights (`LightRenderer.line`, the
//! capsule quads + `circle-mid`/`circle-end` caps), region lights
//! (`LightRenderer.add(x,y,region,rotation,color,opacity)`) and custom lights
//! (the `lights.add(Runnable)` overload, which in the port is an arbitrary
//! world-space quad). Accumulation is `Gl.max` per channel, exactly like
//! `Gl.blendEquationSeparate(Gl.funcAdd, Gl.max)`.
//!
//! Light sources come from the G03 T12 `MindSimHost::light_sources` seam
//! (cores, illuminators, static `emitLight` blocks, units and bullets), the
//! `MindFx` `Drawf.light` requests (H03 T16) and, for the beam kinds whose
//! `BulletType.draw` emits a *line* light instead of the default circle, the
//! bullet draw list (`LaserBulletType.java:120-123`,
//! `ContinuousLaserBulletType.java:68-70`,
//! `ContinuousFlameBulletType.java:95-99`, `SapBulletType.java:60-62`).
//!
//! Deviation S16-5: the per-block `lightRadius`/`lightColor` overrides and the
//! illuminator `brightness`/efficiency factor are not yet in plan 02's
//! `BlockDef`/`LightState`; the pass uses the Java defaults (`Block.lightRadius`
//! 60, illuminator `radius * 2.5`, brightness 0.75). Per-block overrides found
//! in `J/content/Blocks.java` are listed in the T12 commit (PAR-0467 owns the
//! content side).
//!
//! Deviation S16-12: `LightRenderer.add(Runnable)` runs arbitrary draw code in
//! Java; the CPU map cannot replay Godot draws, so the port exposes the same
//! capability as [`CustomLight`] (an arbitrary quad) and [`RegionLight`]
//! (a textured, tinted, rotated rect) stamped into the map.


//! S16-5 residual (PAR-0467 landed the data half): per-block
//! `lightRadius`/`lightColor` now come from `BlockDef` (Java
//! `Block.java:344,350` + the class/`Blocks.java` overrides), so the render
//! pass stamps arbitrary radii/colours and the host emits the floor/overlay
//! environment lights. The illuminator `brightness` factor is still the Java
//! default 0.75 and the custom `drawLight` overrides
//! (`LandingPad`/`Accelerator`/`NuclearReactor` heat light) are not modelled.

use std::sync::Arc;

use godot::classes::{Image, ImageTexture, Material, Mesh, MeshInstance2D, Node2D, ShaderMaterial};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::content::registries::bullets::{BulletDef, BulletKind};
use mind_core::render::drawf::{self, pal};
use mind_core::render::light::{CircleLight, LightAccumulator, SCALING, enabled};
use mind_core::render::rules::RulesRenderView;
use mind_core::render::scan::CameraView;

use crate::sim_host::{BulletDraw, MindSimHost};

use super::atlas_bind::{Quad, build_mesh};
use super::shaders::ShaderRegistry;

/// Arc `Color.orange` (`Color.java:45`, `ffa500ff`) —
/// `ContinuousLaserBulletType.lightColor` (`:40`).
const ORANGE: [f32; 4] = [1.0, 0.647_058_8, 0.0, 1.0];
/// `ContinuousLaserBulletType.lightStroke` / `ContinuousFlameBulletType
/// .lightStroke` class default (`:12`/`:13`).
const CONTINUOUS_LIGHT_STROKE: f32 = 40.0;
/// `ContinuousLaserBulletType.lightOpacity` (`:41`).
const CONTINUOUS_LASER_LIGHT_OPACITY: f32 = 0.7;
/// `SapBulletType.draw`'s beam stroke (`SapBulletType.java:60`): `15f * fout`.
const SAP_LIGHT_STROKE: f32 = 15.0;
/// `LaserBulletType` line-light alpha: `Drawf.light(..., 0.6f)`
/// (`LaserBulletType.java:123`).
const LASER_LIGHT_ALPHA: f32 = 0.6;

/// Light pass counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct LightStats {
    /// Light-map rebuilds since boot.
    pub rebuilds: i64,
    /// Live circle lights in the last rebuild.
    pub lights: i64,
    /// Live line lights in the last rebuild.
    pub lines: i64,
    /// Live region lights in the last rebuild.
    pub regions: i64,
    /// Live custom lights in the last rebuild.
    pub customs: i64,
    /// Whether the composite is currently visible.
    pub visible: bool,
}

/// One `LightRenderer.line` primitive: the capsule the Java code builds from a
/// `circle-mid` band plus two `circle-end` caps (`LightRenderer.java:66-184`).
/// `stroke` is the Java *offset* (`Tmp.v1.trnsExact(rot + 90f, stroke)`), i.e.
/// the capsule radius, so the drawn band is `2 * stroke` wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineLight {
    /// Start x (world px).
    pub x: f32,
    /// Start y (world px).
    pub y: f32,
    /// End x (world px).
    pub x2: f32,
    /// End y (world px).
    pub y2: f32,
    /// Capsule radius (Java `stroke`).
    pub stroke: f32,
    /// Tint `[r, g, b, alpha]` (Java `Draw.color(tint, alpha)`).
    pub color: [f32; 4],
}

/// One `LightRenderer.add(x, y, region, rotation, color, opacity)` primitive
/// (`LightRenderer.java:48-64`): a `Draw.rect(region, x, y, rotation)` tinted by
/// `color` at `opacity`, sampled from [`RegionPixels`].
#[derive(Clone, Debug, PartialEq)]
pub struct RegionLight {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Region width in world px (`region.width`, the natural sprite size).
    pub w: f32,
    /// Region height in world px.
    pub h: f32,
    /// Rotation in degrees (`Draw.rect(region, x, y, rotation)`).
    pub rotation: f32,
    /// Tint `[r, g, b]`; the alpha is `opacity`.
    pub color: [f32; 4],
    /// The region's pixels (`Texture2D.get_image()`); `None` skips the light.
    pub pixels: Option<Arc<RegionPixels>>,
}

#[allow(dead_code)]
impl RegionLight {
    /// A region light with the region's own `rgba` pixels.
    pub fn new(
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rotation: f32,
        color: [f32; 4],
        pixels: Arc<RegionPixels>,
    ) -> Self {
        Self {
            x,
            y,
            w,
            h,
            rotation,
            color,
            pixels: Some(pixels),
        }
    }
}

/// An RGBA8 pixel source for [`RegionLight`] (the port's `TextureRegion`).
#[derive(Clone, Debug, PartialEq)]
pub struct RegionPixels {
    /// Pixel width.
    pub width: i32,
    /// Pixel height.
    pub height: i32,
    /// Row-major RGBA8 pixels (`width * height * 4` bytes).
    pub rgba: Vec<u8>,
}

// The region/custom overloads are the ported `LightRenderer` surface; Java has
// no core call sites for them either (`Drawf.java:314-317` is the only user of
// the region overload), so nothing in-tree queues one yet. The queue + stamp
// paths are unit-tested below.
#[allow(dead_code)]
impl RegionPixels {
    /// Wraps a row-major RGBA8 buffer.
    pub fn new(width: i32, height: i32, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            rgba,
        }
    }

    /// Reads one pixel as `[r, g, b, a]` (0 when out of range).
    fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return [0; 4];
        }
        let index = ((x + y * self.width) * 4) as usize;
        match self.rgba.get(index..index + 4) {
            Some(slice) => [slice[0], slice[1], slice[2], slice[3]],
            None => [0; 4],
        }
    }
}

/// The port's stand-in for `LightRenderer.add(Runnable)`: an arbitrary
/// world-space quad (four points in winding order) with a flat colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomLight {
    /// World-space corners, in winding order.
    pub quad: [[f32; 2]; 4],
    /// Tint `[r, g, b, alpha]`.
    pub color: [f32; 4],
}

/// The frame's queued light primitives (`LightRenderer`'s `Seq<Runnable>
/// lights` + pooled `Seq<CircleLight> circles`).
#[derive(Clone, Debug, Default)]
pub struct LightBuffer {
    /// Pooled circle lights (`mind_core::render::light::LightAccumulator`).
    circles: LightAccumulator,
    /// `LightRenderer.line` primitives.
    lines: Vec<LineLight>,
    /// `LightRenderer.add(region, …)` primitives.
    regions: Vec<RegionLight>,
    /// `LightRenderer.add(Runnable)` equivalents.
    customs: Vec<CustomLight>,
}

/// Per-primitive counts of one [`LightBuffer`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightCounts {
    /// Circle lights.
    pub circles: i64,
    /// Line lights.
    pub lines: i64,
    /// Region lights.
    pub regions: i64,
    /// Custom lights.
    pub customs: i64,
}

#[allow(dead_code)]
impl LightBuffer {
    /// An empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empties the queue for the next frame (keeps capacity, like the Java
    /// pools).
    pub fn clear(&mut self) {
        self.circles.clear();
        self.lines.clear();
        self.regions.clear();
        self.customs.clear();
    }

    /// `LightRenderer.add(x, y, radius, color, opacity)`.
    pub fn add_circle(&mut self, x: f32, y: f32, radius: f32, color: [f32; 4], opacity: f32) {
        self.circles.add(x, y, radius, color, opacity);
    }

    /// `LightRenderer.line(x, y, x2, y2, stroke, tint, alpha)`. Zero/negative
    /// strokes are ignored (they have no area, matching the Java quad).
    pub fn add_line(
        &mut self,
        x: f32,
        y: f32,
        x2: f32,
        y2: f32,
        stroke: f32,
        tint: [f32; 4],
        alpha: f32,
    ) {
        if stroke <= 0.0 {
            return;
        }
        self.lines.push(LineLight {
            x,
            y,
            x2,
            y2,
            stroke,
            color: [tint[0], tint[1], tint[2], alpha],
        });
    }

    /// `LightRenderer.add(x, y, region, rotation, color, opacity)`.
    pub fn add_region(&mut self, light: RegionLight) {
        if light.pixels.is_none() {
            return;
        }
        self.regions.push(light);
    }

    /// The `lights.add(Runnable)` overload: an arbitrary quad.
    pub fn add_custom(&mut self, light: CustomLight) {
        self.customs.push(light);
    }

    /// The live circle lights this frame.
    pub fn circles(&self) -> &[CircleLight] {
        self.circles.circles()
    }

    /// The live line lights this frame.
    pub fn lines(&self) -> &[LineLight] {
        &self.lines
    }

    /// The live region lights this frame.
    pub fn regions(&self) -> &[RegionLight] {
        &self.regions
    }

    /// The live custom lights this frame.
    pub fn customs(&self) -> &[CustomLight] {
        &self.customs
    }

    /// Total queued primitives.
    pub fn len(&self) -> usize {
        self.circles.len() + self.lines.len() + self.regions.len() + self.customs.len()
    }

    /// Whether nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Per-primitive counts.
    pub fn counts(&self) -> LightCounts {
        LightCounts {
            circles: self.circles.len() as i64,
            lines: self.lines.len() as i64,
            regions: self.regions.len() as i64,
            customs: self.customs.len() as i64,
        }
    }

    /// Stamps every queued primitive into the `width`×`height` RGBA8 light map
    /// (max-blended, the Java `Gl.max`).
    pub fn stamp_into(&self, bytes: &mut [u8], width: i32, height: i32, view: &CameraView) {
        for light in self.circles() {
            stamp_rgba(bytes, width, height, view, light);
        }
        for light in self.lines() {
            stamp_line(bytes, width, height, view, light);
        }
        for light in self.regions() {
            stamp_region(bytes, width, height, view, light);
        }
        for light in self.customs() {
            stamp_custom(bytes, width, height, view, light);
        }
    }
}

/// The light map's world↔texel mapping: one texel is `scale` world px
/// (`CameraView.w / map width`, the Java `LightRenderer.scaling`).
#[derive(Clone, Copy, Debug)]
struct MapFrame {
    scale: f32,
    origin_x: f32,
    origin_y: f32,
}

impl MapFrame {
    fn new(view: &CameraView, width: i32) -> Self {
        Self {
            scale: view.w.max(1.0) / width.max(1) as f32,
            origin_x: view.x - view.w / 2.0,
            origin_y: view.y - view.h / 2.0,
        }
    }

    /// World -> texel-space position.
    fn texel(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.origin_x) / self.scale,
            (y - self.origin_y) / self.scale,
        )
    }

    /// Texel-space position -> world.
    fn world(&self, tx: f32, ty: f32) -> (f32, f32) {
        (
            tx * self.scale + self.origin_x,
            ty * self.scale + self.origin_y,
        )
    }
}

/// Max-blends one texel (`Gl.blendEquationSeparate(Gl.funcAdd, Gl.max)`).
fn blend_max(bytes: &mut [u8], index: usize, r: u8, g: u8, b: u8, a: u8) {
    if index + 3 >= bytes.len() {
        return;
    }
    let channels = [r, g, b, a];
    for (offset, value) in channels.iter().enumerate() {
        bytes[index + offset] = bytes[index + offset].max(*value);
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
    let frame = MapFrame::new(view, width);
    let (cx, cy) = frame.texel(light.x, light.y);
    let r = light.radius / frame.scale;
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
            blend_max(
                bytes,
                index,
                (light.color[0] * falloff * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[1] * falloff * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[2] * falloff * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[3] * falloff * 255.0).clamp(0.0, 255.0) as u8,
            );
        }
    }
}

/// Shortest distance from `(px, py)` to the segment `(ax, ay)-(bx, by)`.
fn point_segment_distance(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let cx = ax + dx * t;
    let cy = ay + dy * t;
    let ex = px - cx;
    let ey = py - cy;
    (ex * ex + ey * ey).sqrt()
}

/// Stamps one `LightRenderer.line` capsule. Java draws the band as a quad
/// offset `±stroke` plus two `circle-end` caps of radius `stroke`
/// (`LightRenderer.java:81-182`), i.e. a capsule; the map applies the same
/// radial falloff as the circle lights.
fn stamp_line(bytes: &mut [u8], width: i32, height: i32, view: &CameraView, light: &LineLight) {
    if light.stroke <= 0.0 || width <= 0 || height <= 0 {
        return;
    }
    let frame = MapFrame::new(view, width);
    let (ax, ay) = frame.texel(light.x, light.y);
    let (bx, by) = frame.texel(light.x2, light.y2);
    let r = light.stroke / frame.scale;
    let x1 = ((ax.min(bx) - r).floor() as i32).max(0);
    let y1 = ((ay.min(by) - r).floor() as i32).max(0);
    let x2 = ((ax.max(bx) + r).ceil() as i32).min(width);
    let y2 = ((ay.max(by) + r).ceil() as i32).min(height);
    for ty in y1..y2 {
        for tx in x1..x2 {
            let px = tx as f32 + 0.5;
            let py = ty as f32 + 0.5;
            let d = point_segment_distance(px, py, ax, ay, bx, by);
            if d >= r {
                continue;
            }
            let falloff = 1.0 - d / r;
            let index = ((tx + ty * width) * 4) as usize;
            blend_max(
                bytes,
                index,
                (light.color[0] * falloff * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[1] * falloff * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[2] * falloff * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[3] * falloff * 255.0).clamp(0.0, 255.0) as u8,
            );
        }
    }
}

/// Stamps one region light: the region is sampled with its own RGBA pixels,
/// tinted by `color.rgb` and scaled by `opacity` in the alpha, rotated about
/// the light centre. Nearest-neighbour sampling keeps the pass deterministic
/// (the Java `Draw.rect` bilinear filter only smooths the atlas edge).
fn stamp_region(bytes: &mut [u8], width: i32, height: i32, view: &CameraView, light: &RegionLight) {
    let Some(pixels) = light.pixels.as_deref() else {
        return;
    };
    if light.w <= 0.0 || light.h <= 0.0 || width <= 0 || height <= 0 {
        return;
    }
    let frame = MapFrame::new(view, width);
    let (cx, cy) = frame.texel(light.x, light.y);
    let half_w = light.w * 0.5 / frame.scale;
    let half_h = light.h * 0.5 / frame.scale;
    let reach = (half_w * half_w + half_h * half_h).sqrt();
    let x1 = ((cx - reach).floor() as i32).max(0);
    let y1 = ((cy - reach).floor() as i32).max(0);
    let x2 = ((cx + reach).ceil() as i32).min(width);
    let y2 = ((cy + reach).ceil() as i32).min(height);
    let (sin, cos) = light.rotation.to_radians().sin_cos();
    for ty in y1..y2 {
        for tx in x1..x2 {
            let dx = tx as f32 + 0.5 - cx;
            let dy = ty as f32 + 0.5 - cy;
            // Inverse rotation (`Draw.rect` rotates clockwise on a Y-down
            // world, so the inverse is the +rotation matrix).
            let lx = dx * cos + dy * sin;
            let ly = -dx * sin + dy * cos;
            if lx.abs() > half_w || ly.abs() > half_h {
                continue;
            }
            let u = (lx + half_w) / (half_w * 2.0);
            let v = (ly + half_h) / (half_h * 2.0);
            let sx = (u * pixels.width as f32).floor() as i32;
            let sy = (v * pixels.height as f32).floor() as i32;
            let texel = pixels.pixel(sx, sy);
            if texel[3] == 0 {
                continue;
            }
            let index = ((tx + ty * width) * 4) as usize;
            blend_max(
                bytes,
                index,
                (texel[0] as f32 / 255.0 * light.color[0] * 255.0).clamp(0.0, 255.0) as u8,
                (texel[1] as f32 / 255.0 * light.color[1] * 255.0).clamp(0.0, 255.0) as u8,
                (texel[2] as f32 / 255.0 * light.color[2] * 255.0).clamp(0.0, 255.0) as u8,
                (texel[3] as f32 / 255.0 * light.color[3] * 255.0).clamp(0.0, 255.0) as u8,
            );
        }
    }
}

/// Coverage of a world point inside a convex quad (`[0, 1]`, feathered by one
/// texel so the edge is not aliased).
fn quad_coverage(px: f32, py: f32, quad: &[[f32; 2]; 4], feather: f32) -> f32 {
    let mut nearest = f32::INFINITY;
    let mut sign = 0.0f32;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let cross = (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0]);
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != 0.0 && cross.signum() != sign {
            return 0.0;
        }
        nearest = nearest.min(point_segment_distance(px, py, a[0], a[1], b[0], b[1]));
    }
    if feather <= 0.0 {
        return 1.0;
    }
    (nearest / feather).clamp(0.0, 1.0)
}

/// Stamps one custom light (an arbitrary quad, the `add(Runnable)` equivalent).
fn stamp_custom(bytes: &mut [u8], width: i32, height: i32, view: &CameraView, light: &CustomLight) {
    if width <= 0 || height <= 0 {
        return;
    }
    let frame = MapFrame::new(view, width);
    let min_x = light.quad.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    let max_x = light.quad.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
    let min_y = light.quad.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let max_y = light.quad.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    let (ax, ay) = frame.texel(min_x, min_y);
    let (bx, by) = frame.texel(max_x, max_y);
    let x1 = (ax.floor() as i32).max(0);
    let y1 = (ay.floor() as i32).max(0);
    let x2 = (bx.ceil() as i32).min(width);
    let y2 = (by.ceil() as i32).min(height);
    for ty in y1..y2 {
        for tx in x1..x2 {
            let (wx, wy) = frame.world(tx as f32 + 0.5, ty as f32 + 0.5);
            let coverage = quad_coverage(wx, wy, &light.quad, frame.scale);
            if coverage <= 0.0 {
                continue;
            }
            let index = ((tx + ty * width) * 4) as usize;
            blend_max(
                bytes,
                index,
                (light.color[0] * coverage * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[1] * coverage * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[2] * coverage * 255.0).clamp(0.0, 255.0) as u8,
                (light.color[3] * coverage * 255.0).clamp(0.0, 255.0) as u8,
            );
        }
    }
}

/// `Rgba` -> `[r, g, b, a]`.
fn rgba(color: mind_core::content::Rgba) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

/// The line light a bullet's `draw` emits, for the beam kinds that override
/// `BulletType.drawLight` to draw nothing (`LaserBulletType.java:125-128`,
/// `ContinuousLaserBulletType.java:73-76`,
/// `ContinuousFlameBulletType.java:101-104`, `SapBulletType.java:56-59`) and
/// instead put a `Drawf.light(x, y, x2, y2, stroke, tint, alpha)` along the
/// beam.
///
/// Colours/opacities are the Java type-initializer values: `BulletDef` still
/// stores the `BulletType` defaults (`Pal.powerLight`, 0.3) for every type
/// (the content table has no per-type light rows yet), so the class defaults
/// are exact where the initializer overrides them.
pub fn bullet_line_light(bullet: &BulletDraw, def: &BulletDef) -> Option<LineLight> {
    let fin = (bullet.time / bullet.lifetime.max(1e-4)).clamp(0.0, 1.0);
    let fout = 1.0 - fin;
    // `Tmp.v1.trns(rotation, length * 1.1f)` for the beam kinds.
    let beam = |length: f32, stroke: f32, color: [f32; 4]| {
        let (vx, vy) = drawf::trns(bullet.rotation, length * 1.1);
        LineLight {
            x: bullet.x,
            y: bullet.y,
            x2: bullet.x + vx,
            y2: bullet.y + vy,
            stroke,
            color,
        }
    };
    match def.kind {
        // `LaserBulletType.draw` (`:117-123`): `baseLen = fdata * curve(fin, 0,
        // 0.2)`, stroke `width * 1.4f * fout`, tint `colors[0]`
        // (`Pal.lancerLaser` at 0.4 alpha), alpha 0.6.
        BulletKind::Laser => {
            let base_len = bullet.fdata * drawf::curve(fin, 0.0, 0.2);
            let tint = def
                .colors
                .first()
                .map(|color| rgba(*color))
                .unwrap_or(pal::LANCER_LASER);
            let (vx, vy) = drawf::trns(bullet.rotation, base_len * 1.1);
            Some(LineLight {
                x: bullet.x,
                y: bullet.y,
                x2: bullet.x + vx,
                y2: bullet.y + vy,
                stroke: def.width.max(1.0) * 1.4 * fout,
                color: [tint[0], tint[1], tint[2], LASER_LIGHT_ALPHA],
            })
        }
        // `ContinuousLaserBulletType.draw` (`:68-70`): stroke `lightStroke`
        // (40), `Color.orange`, opacity 0.7. Content gap: the port's
        // `BulletDef::new(ContinuousLaser)` does not yet apply the Java
        // initializer's `lightColor`/`lightOpacity`
        // (`ContinuousLaserBulletType.java:40-41`), so the class defaults are
        // used here until `mc/content/registries/bullets.rs` sets them
        // (reported to the content owner).
        BulletKind::ContinuousLaser => Some(beam(
            bullet.fdata,
            CONTINUOUS_LIGHT_STROKE,
            [
                ORANGE[0],
                ORANGE[1],
                ORANGE[2],
                CONTINUOUS_LASER_LIGHT_OPACITY,
            ],
        )),
        // `ContinuousFlameBulletType.draw` (`:95-97`): same geometry/stroke,
        // `lightColor = hitColor` = `colors[1]` at full alpha, `lightOpacity`
        // 0.7 — both carried by the def (`Bullet` kind initializer).
        BulletKind::ContinuousFlame => {
            let tint = rgba(def.light_color);
            let tint = if tint[3] > 0.0 {
                tint
            } else {
                def.colors
                    .get(1)
                    .map(|color| rgba(*color))
                    .unwrap_or([0.882, 0.537, 0.961, 1.0])
            };
            Some(beam(
                bullet.fdata,
                CONTINUOUS_LIGHT_STROKE,
                [tint[0], tint[1], tint[2], def.light_opacity],
            ))
        }
        // `SapBulletType.draw` (`:60-62`): stroke `15f * fout`, `Pal.sap`,
        // opacity 0.6. The beam end is `fdata` (the sap length the behavior
        // stores, `kinds/sap.rs`), the closest port equivalent of the Java
        // `data.lerp(b, b.fin())` target.
        BulletKind::Sap => {
            let sap = rgba(def.light_color);
            let (vx, vy) = drawf::trns(bullet.rotation, bullet.fdata);
            Some(LineLight {
                x: bullet.x,
                y: bullet.y,
                x2: bullet.x + vx,
                y2: bullet.y + vy,
                stroke: SAP_LIGHT_STROKE * fout,
                color: [sap[0], sap[1], sap[2], def.light_opacity],
            })
        }
        _ => None,
    }
}

/// `LightRenderer` — camera-sized light map + `Shaders.light` composite.
pub struct LightRenderer {
    host: Gd<MindSimHost>,
    node: Gd<MeshInstance2D>,
    material: Option<Gd<ShaderMaterial>>,
    buffer: LightBuffer,
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
        // I02 T2: the node has no mesh until the first successful bake; a
        // visible mesh node without a mesh draws the `mesh_get_surface_count`
        // error.
        node.clone().set_visible(false);
        Self {
            host,
            node,
            material,
            buffer: LightBuffer::new(),
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

    /// The frame's light queue (the counters' source; H03 T6 probe seam).
    #[allow(dead_code)]
    pub fn buffer(&self) -> &LightBuffer {
        &self.buffer
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

        let revision = self.host.clone().bind().world_revision();
        let moved =
            (view.x - self.last_view.0).abs() > 1.0 || (view.y - self.last_view.1).abs() > 1.0;
        self.frames = self.frames.wrapping_add(1);
        if revision == self.last_revision && !moved && !self.frames.is_multiple_of(4) {
            return;
        }
        self.last_revision = revision;
        self.last_view = (view.x, view.y);

        let (width, height, bytes, counts) = {
            let host = self.host.clone();
            let host = host.bind();
            let rect = (view.x - view.w / 2.0, view.y - view.h / 2.0, view.w, view.h);
            // Grow the collection rect for the beam kinds: their line ends sit
            // outside the camera rect (Java's `lightClipSize`/`drawSize`
            // equivalents) and `bullet_draws` has no rect filter.
            let grown = (
                rect.0 - BEAM_LIGHT_GROW,
                rect.1 - BEAM_LIGHT_GROW,
                rect.2 + 2.0 * BEAM_LIGHT_GROW,
                rect.3 + 2.0 * BEAM_LIGHT_GROW,
            );
            self.buffer.clear();
            for source in host.light_sources(rect) {
                self.buffer.add_circle(
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
                self.buffer.add_circle(*x, *y, *radius, *color, *opacity);
            }
            // `LightRenderer.line` primitives from the beams whose
            // `BulletType.draw` overrides the default circle light with a
            // `Drawf.light(x, y, x2, y2, …)` (`bullet_line_light`).
            let lines: Vec<LineLight> = host
                .with_registry(|content| {
                    host.bullet_draws()
                        .iter()
                        .filter(|draw| {
                            rect_contains(grown, draw.x, draw.y) || {
                                let def = content.bullet(draw.def);
                                def.and_then(|def| bullet_line_light(draw, def))
                                    .map(|line| rect_contains(grown, line.x2, line.y2))
                                    .unwrap_or(false)
                            }
                        })
                        .filter_map(|draw| {
                            let def = content.bullet(draw.def)?;
                            bullet_line_light(draw, def)
                        })
                        .collect()
                })
                .unwrap_or_default();
            for line in lines {
                self.buffer.add_line(
                    line.x,
                    line.y,
                    line.x2,
                    line.y2,
                    line.stroke,
                    line.color,
                    line.color[3],
                );
            }
            let width = ((view.w / SCALING as f32).ceil() as i32).max(1);
            let height = ((view.h / SCALING as f32).ceil() as i32).max(1);
            let mut bytes = vec![0u8; (width * height * 4) as usize];
            self.buffer.stamp_into(&mut bytes, width, height, view);
            (width, height, bytes, self.buffer.counts())
        };

        let Some(mut material) = self.material.clone() else {
            self.node.set_visible(false);
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
            self.node.set_visible(false);
            return;
        };
        let Some(texture) = ImageTexture::create_from_image(&image) else {
            self.node.set_visible(false);
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
            // I02 T2: show the composite only while it holds a mesh.
            self.node.set_visible(true);
        } else {
            self.node.set_visible(false);
        }
        self.stats.rebuilds += 1;
        self.stats.lights = counts.circles;
        self.stats.lines = counts.lines;
        self.stats.regions = counts.regions;
        self.stats.customs = counts.customs;
    }
}

/// Beam line lights reach a laser's full length beyond the camera rect
/// (`LaserBulletType.length` 160, foreshadow 300+); the collection rect grows by
/// the longest vanilla beam so a beam ending outside the view still lights the
/// visible part.
const BEAM_LIGHT_GROW: f32 = 420.0;

/// Whether `(x, y)` lies inside `(x, y, w, h)`.
fn rect_contains(rect: (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= rect.0 && x <= rect.0 + rect.2 && y >= rect.1 && y <= rect.1 + rect.3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(w: f32, h: f32) -> CameraView {
        CameraView {
            x: w / 2.0,
            y: h / 2.0,
            w,
            h,
            zoom: 1.0,
            team: 0,
        }
    }

    fn alpha_at(bytes: &[u8], width: i32, x: i32, y: i32) -> u8 {
        bytes[((x + y * width) * 4 + 3) as usize]
    }

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

    /// `LightRenderer.line`: a capsule along the segment with round caps, not a
    /// circle at the start point.
    #[test]
    fn line_light_stamps_a_capsule() {
        let view = view(64.0, 64.0);
        let mut bytes = vec![0u8; 64 * 64 * 4];
        let line = LineLight {
            x: 8.0,
            y: 32.0,
            x2: 56.0,
            y2: 32.0,
            stroke: 4.0,
            color: [1.0, 1.0, 1.0, 1.0],
        };
        stamp_line(&mut bytes, 64, 64, &view, &line);
        // Middle of the segment is lit near the centre of the band.
        assert!(alpha_at(&bytes, 64, 32, 32) > 200);
        // The band's half-width is `stroke` (4): 2.5 px off is lit (falloff
        // 0.375), 5.5 px off is outside.
        assert!(alpha_at(&bytes, 64, 32, 34) > 0, "inside the band");
        assert!(
            alpha_at(&bytes, 64, 32, 34) < alpha_at(&bytes, 64, 32, 32),
            "falloff across the band"
        );
        assert_eq!(alpha_at(&bytes, 64, 32, 38), 0, "outside the band");
        // Round caps: past the end by < stroke is lit, by > stroke is not.
        assert!(alpha_at(&bytes, 64, 58, 32) > 0, "end cap");
        assert_eq!(alpha_at(&bytes, 64, 61, 32), 0, "past the end cap");
        // Far corners stay dark.
        assert_eq!(alpha_at(&bytes, 64, 1, 1), 0);
    }

    /// `LightRenderer.add(region, rotation, color, opacity)`: the region's
    /// pixels are tinted, scaled by the opacity, and rotated.
    #[test]
    fn region_light_stamps_tinted_pixels() {
        let view = view(16.0, 16.0);
        // 2x2 fully red, fully opaque region.
        let pixels = Arc::new(RegionPixels::new(2, 2, vec![255, 0, 0, 255].repeat(4)));
        let mut bytes = vec![0u8; 16 * 16 * 4];
        let light = RegionLight::new(8.0, 8.0, 8.0, 8.0, 0.0, [1.0, 1.0, 1.0, 0.5], pixels);
        stamp_region(&mut bytes, 16, 16, &view, &light);
        let index = ((8 + 8 * 16) * 4) as usize;
        assert_eq!(bytes[index], 255, "red channel preserved");
        assert_eq!(bytes[index + 1], 0, "green channel stays 0");
        assert!(
            (i32::from(bytes[index + 3]) - 127).abs() <= 1,
            "alpha = opacity * 255"
        );
        // Outside the 8x8 quad (world 14,14) nothing is stamped.
        assert_eq!(alpha_at(&bytes, 16, 14, 14), 0);
    }

    /// The Java `add(region, rotation, …)` rotates the quad; a 4x1 strip
    /// rotated 90° covers a vertical strip instead of a horizontal one.
    #[test]
    fn region_light_rotates() {
        let view = view(32.0, 32.0);
        let pixels = Arc::new(RegionPixels::new(4, 1, vec![255, 255, 255, 255].repeat(4)));
        let mut bytes = vec![0u8; 32 * 32 * 4];
        let light = RegionLight::new(16.0, 16.0, 16.0, 4.0, 90.0, [1.0, 1.0, 1.0, 1.0], pixels);
        stamp_region(&mut bytes, 32, 32, &view, &light);
        // Rotated: the long axis is vertical.
        assert!(alpha_at(&bytes, 32, 16, 8) > 0, "top of the strip");
        assert!(alpha_at(&bytes, 32, 16, 22) > 0, "bottom of the strip");
        assert_eq!(alpha_at(&bytes, 32, 16, 25), 0, "past the strip");
        // The unrotated long axis (3 px out from the centre) is outside.
        assert_eq!(alpha_at(&bytes, 32, 24, 16), 0, "no horizontal reach");
    }

    /// The `add(Runnable)` equivalent: an arbitrary quad.
    #[test]
    fn custom_light_stamps_a_quad() {
        let view = view(32.0, 32.0);
        let mut bytes = vec![0u8; 32 * 32 * 4];
        let light = CustomLight {
            quad: [[8.0, 8.0], [24.0, 8.0], [24.0, 24.0], [8.0, 24.0]],
            color: [0.0, 1.0, 0.0, 1.0],
        };
        stamp_custom(&mut bytes, 32, 32, &view, &light);
        assert!(alpha_at(&bytes, 32, 16, 16) > 200, "inside the quad");
        assert_eq!(alpha_at(&bytes, 32, 2, 2), 0, "outside the quad");
        let index = ((16 + 16 * 32) * 4) as usize;
        assert_eq!(bytes[index], 0, "red channel untouched");
        assert_eq!(bytes[index + 1], 255, "green channel lit");
    }

    /// `LightRenderer.line` ignores zero/negative strokes
    /// (`if(!enabled()) return` + the quad has no area).
    #[test]
    fn zero_stroke_lines_are_dropped() {
        let mut buffer = LightBuffer::new();
        buffer.add_line(0.0, 0.0, 8.0, 8.0, 0.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        assert!(buffer.lines().is_empty());
        buffer.add_line(0.0, 0.0, 8.0, 8.0, 2.0, [1.0, 1.0, 1.0, 1.0], 0.5);
        assert_eq!(buffer.lines().len(), 1);
        assert_eq!(buffer.lines()[0].color[3], 0.5, "alpha from the opacity");
    }

    /// The buffer pools every primitive and reports per-kind counts.
    #[test]
    fn buffer_counts_and_resets() {
        let mut buffer = LightBuffer::new();
        buffer.add_circle(4.0, 4.0, 8.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        buffer.add_line(0.0, 0.0, 8.0, 0.0, 2.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        buffer.add_custom(CustomLight {
            quad: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            color: [1.0, 1.0, 1.0, 1.0],
        });
        buffer.add_region(RegionLight::new(
            0.0,
            0.0,
            1.0,
            1.0,
            0.0,
            [1.0, 1.0, 1.0, 1.0],
            Arc::new(RegionPixels::new(1, 1, vec![255, 255, 255, 255])),
        ));
        let counts = buffer.counts();
        assert_eq!(
            counts,
            LightCounts {
                circles: 1,
                lines: 1,
                regions: 1,
                customs: 1
            }
        );
        assert_eq!(buffer.len(), 4);
        buffer.clear();
        assert!(buffer.is_empty());
        // A region without pixels is dropped (nothing to sample).
        buffer.add_region(RegionLight {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            rotation: 0.0,
            color: [1.0, 1.0, 1.0, 1.0],
            pixels: None,
        });
        assert!(buffer.regions().is_empty());
    }

    fn draw(kind: BulletKind, fdata: f32, rotation: f32, time: f32) -> (BulletDraw, BulletDef) {
        let def = BulletDef::new(kind);
        (
            BulletDraw {
                id: 1,
                x: 100.0,
                y: 100.0,
                rotation,
                time,
                lifetime: def.lifetime,
                fdata,
                def: def.id,
                team: 0,
            },
            def,
        )
    }

    /// `LaserBulletType.draw` (`:118-123`): at half life the beam is the full
    /// `fdata` length, `stroke = width * 1.4 * fout`, alpha 0.6.
    #[test]
    fn laser_bullet_emits_a_line_light() {
        let (bullet, def) = draw(BulletKind::Laser, 100.0, 0.0, 8.0);
        let line = bullet_line_light(&bullet, &def).expect("laser line light");
        assert!((line.x2 - (bullet.x + 110.0)).abs() < 1e-3, "1.1x length");
        assert!((line.y2 - bullet.y).abs() < 1e-3);
        assert!((line.stroke - 15.0 * 1.4 * 0.5).abs() < 1e-4, "stroke");
        assert!((line.color[3] - LASER_LIGHT_ALPHA).abs() < 1e-6);
        // At spawn the curve collapses the beam, matching `curve(fin, 0, 0.2)`.
        let (spawn, def) = draw(BulletKind::Laser, 100.0, 0.0, 0.0);
        let line = bullet_line_light(&spawn, &def).expect("laser line light");
        assert!((line.x2 - spawn.x).abs() < 1e-3, "zero length at fin 0");
    }

    /// `ContinuousLaserBulletType.draw` (`:68-70`): stroke 40, `Color.orange`,
    /// opacity 0.7, beam `fdata * 1.1` along the rotation.
    #[test]
    fn continuous_laser_emits_an_orange_line_light() {
        let (bullet, def) = draw(BulletKind::ContinuousLaser, 50.0, 90.0, 4.0);
        let line = bullet_line_light(&bullet, &def).expect("continuous line light");
        assert!((line.stroke - 40.0).abs() < 1e-6);
        assert!((line.color[0] - 1.0).abs() < 1e-6, "orange red");
        assert!((line.color[1] - 0.647_058_8).abs() < 1e-5, "orange green");
        assert!((line.color[3] - 0.7).abs() < 1e-6);
        // 90° points down the +Y axis in the Y-down world.
        assert!((line.x2 - bullet.x).abs() < 1e-3);
        assert!((line.y2 - (bullet.y + 55.0)).abs() < 1e-3);
    }

    /// `SapBulletType.draw` (`:60-62`): stroke `15 * fout`, `Pal.sap`,
    /// opacity 0.6, beam end at `fdata`.
    #[test]
    fn sap_emits_a_shrinking_line_light() {
        let (mut bullet, def) = draw(BulletKind::Sap, 40.0, 0.0, 8.0);
        // `SapBulletType` keeps the `BulletType` lifetime default; pin it so
        // `fout` is exactly 0.5.
        bullet.lifetime = 16.0;
        let line = bullet_line_light(&bullet, &def).expect("sap line light");
        assert!((line.x2 - (bullet.x + 40.0)).abs() < 1e-3);
        assert!((line.stroke - 7.5).abs() < 1e-4, "15 * fout");
        assert!((line.color[3] - 0.6).abs() < 1e-6, "def light_opacity");
    }

    /// Kinds whose `draw` has no line light stay circles (the default
    /// `BulletType.drawLight`), so the pass never double-emits.
    #[test]
    fn other_kinds_have_no_line_light() {
        for kind in [
            BulletKind::Basic,
            BulletKind::LaserBolt,
            BulletKind::Shrapnel,
            BulletKind::Rail,
        ] {
            let (bullet, def) = draw(kind, 40.0, 0.0, 8.0);
            assert!(bullet_line_light(&bullet, &def).is_none(), "{kind:?}");
        }
    }

    /// The collection rect grows by the longest vanilla beam so an off-screen
    /// beam end still lights the visible part.
    #[test]
    fn beam_grow_covers_the_longest_vanilla_beam() {
        // `LaserBulletType.length` 160 at 1.1x, foreshadow is the longest.
        assert!(BEAM_LIGHT_GROW >= 160.0 * 1.1);
        let rect = (0.0, 0.0, 100.0, 100.0);
        assert!(rect_contains(rect, 50.0, 50.0));
        assert!(!rect_contains(rect, 150.0, 50.0));
    }
}
