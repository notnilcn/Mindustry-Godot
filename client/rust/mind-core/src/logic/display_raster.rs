// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! CPU display rasterizer for `LogicDisplay` (`I03 T12`, PAR-0105 part 1).
//!
//! Java renders each display into a GPU `FrameBuffer`
//! (`LogicDisplay.java:158-251`). The port renders into a deterministic CPU
//! RGBA buffer in `mind-core` and `mind-gdext` uploads that buffer into an
//! `ImageTexture` (T13). The buffer lives in a view-side resource, never in
//! the checksummed sim state.
//!
//! Command order and geometry follow `LogicDisplay.processCommands` and the
//! Arc drawing primitives it calls (`Fill.crect/quad/poly/tri`, `Lines.line/
//! rect/poly`); every coordinate is a display pixel with `(0, 0)` at the
//! bottom-left. `pixels` is stored top-row-first (the Java buffer is drawn
//! with a negative height), so row `0` is the TOP of the image.

use std::collections::VecDeque;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use indexmap::IndexMap;

use crate::content::BlockKind;
use crate::logic::blocks::LogicDisplayState;
use crate::logic::blocks::block_kind_of;
use crate::logic::executor::draw::{
    COMMAND_CLEAR, COMMAND_COLOR, COMMAND_IMAGE, COMMAND_LINE, COMMAND_LINE_POLY,
    COMMAND_LINE_RECT, COMMAND_POLY, COMMAND_PRINT, COMMAND_RECT, COMMAND_RESET_TRANSFORM,
    COMMAND_ROTATE, COMMAND_SCALE, COMMAND_STROKE, COMMAND_TRANSLATE, COMMAND_TRIANGLE,
    DISPLAY_DRAW_TYPE, SCALE_STEP, cmd_p1, cmd_p2, cmd_p3, cmd_p4, cmd_type, cmd_x, cmd_y,
    unpack_sign,
};

/// RGBA8888 pixel (`0xRRGGBBAA`).
pub type Rgba = u32;

/// Opaque white (`Color.whiteFloatBits` equivalent for the raster path).
pub const WHITE: Rgba = 0xffff_ffff;

/// `LogicDisplay.maxSides` (`LogicDisplay.java:51`; no vanilla display
/// overrides it): `poly`/`linePoly` draw `Math.min(p1, maxSides)` sides
/// (`LogicDisplay.java:208-209`).
pub const MAX_SIDES: i32 = 25;

/// Decoded RGBA image handed to the rasterizer (icons, glyphs).
#[derive(Debug, Clone, PartialEq)]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8888, row 0 = top.
    pub pixels: Vec<Rgba>,
}

/// A borrowed image view (another display's raster).
#[derive(Debug, Clone, Copy)]
pub struct ImageRef<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA8888, row 0 = top.
    pub pixels: &'a [Rgba],
}

/// One logic-font glyph (`Fonts.logic.getData().getGlyph`).
///
/// The placement fields follow Godot's `Font` metrics, because that is what the
/// view-side provider reads (`mind-gdext::display_context`): `x_offset` is the
/// rect's left edge from the pen and `y_offset` its **top** edge from the
/// baseline in the y-down font convention (negative above the baseline,
/// `Font.get_glyph_offset`). The glyph bitmap includes the font atlas' 1 px
/// pad on every side; [`Glyph::print_center`] folds both into Java's
/// `LogicDisplay` placement.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// Glyph bitmap (row 0 = top).
    pub image: RgbaImage,
    /// Rect left edge from the pen (libGDX `glyph.xoffset`,
    /// `Font.get_glyph_offset().x`).
    pub x_offset: f32,
    /// Rect top edge from the baseline, y-down (`Font.get_glyph_offset().y`).
    /// libGDX's `glyph.yoffset` is the opposite corner; see
    /// [`Glyph::print_center`].
    pub y_offset: f32,
    /// Font cap height for callers that lay text out. Not read by the
    /// placement: Arc folds `capHeight` into `FontData.ascent`
    /// (`FreeTypeFontGenerator.java:377-383`), and the provider reports
    /// Godot's padded glyph metric here, not Arc's value.
    pub cap_height: f32,
    /// Font ascender for callers that lay text out. Not read by the
    /// placement; [`Glyph::print_center`] documents the Arc/Godot conversion.
    pub ascent: f32,
}

impl Glyph {
    /// The glyph rect's centre in `LogicDisplay` command space for a `print` at
    /// `(x, y)` (`LogicDisplay.java:233`).
    ///
    /// Java: `x + w/2 + glyph.xoffset`,
    /// `y + h/2 + glyph.yoffset + capHeight + ascent`.
    ///
    /// With libGDX metrics `ascent = rawAscender - capHeight` and
    /// `glyph.yoffset = -(h - top) - rawAscender`
    /// (`FreeTypeFontGenerator.java:618-621`), the vertical sum collapses to
    /// `y + top - h/2`. The provider's Godot metrics already measure `h`
    /// including the 1 px atlas pad and `y_offset` from the baseline to the
    /// padded rect's top, so the same expression is `y - y_offset - h/2`.
    pub fn print_center(&self, x: f32, y: f32) -> (f32, f32) {
        let width = self.image.width as f32;
        let height = self.image.height as f32;
        (
            x + width / 2.0 + self.x_offset,
            y - self.y_offset - height / 2.0,
        )
    }
}

/// View-side image provider (icons from the atlas, glyphs from the logic
/// font). T13 implements this over `MindAssets`; `NoImages` is the headless
/// fallback.
pub trait DisplayDrawContext {
    /// Content icon pixels (`UnlockableContent.fullIcon`).
    fn icon(&self, content_type: u8, id: u16) -> Option<RgbaImage>;
    /// Logic-font glyph (`Fonts.logic`).
    fn glyph(&self, c: char) -> Option<Glyph>;
}

/// No images (`headless`); every lookup misses.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoImages;

impl DisplayDrawContext for NoImages {
    fn icon(&self, _content_type: u8, _id: u16) -> Option<RgbaImage> {
        None
    }
    fn glyph(&self, _c: char) -> Option<Glyph> {
        None
    }
}

/// Internal draw context: the caller's icons/glyphs plus other displays'
/// rasters (resolved from the sim arena).
pub trait DisplayImages {
    /// Content icon pixels.
    fn icon(&self, content_type: u8, id: u16) -> Option<RgbaImage>;
    /// Logic-font glyph.
    fn glyph(&self, c: char) -> Option<Glyph>;
    /// Another display's raster by arena index (`displays.get(id).rootDisplay`).
    fn display(&self, index: i32) -> Option<ImageRef<'_>>;
}

impl DisplayImages for NoImages {
    fn icon(&self, _content_type: u8, _id: u16) -> Option<RgbaImage> {
        None
    }
    fn glyph(&self, _c: char) -> Option<Glyph> {
        None
    }
    fn display(&self, _index: i32) -> Option<ImageRef<'_>> {
        None
    }
}

/// A display's CPU frame buffer (`LogicDisplayBuild.buffer` minus the GPU).
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayRaster {
    /// Buffer width in pixels.
    pub width: u32,
    /// Buffer height in pixels.
    pub height: u32,
    /// RGBA8888, row 0 = top.
    pub pixels: Vec<Rgba>,
    /// Current draw color (packed RGBA8888; `LogicDisplayBuild.color`).
    pub color: Rgba,
    /// Current stroke (`LogicDisplayBuild.stroke`).
    pub stroke: f32,
    /// Current transform as an affine `[a, b, c, d, e, f]`
    /// (`x' = a*x + c*y + e`, `y' = b*x + d*y + f`); `Mat` equivalent.
    pub transform: [f32; 6],
}

/// Identity affine.
const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

impl DisplayRaster {
    /// Creates a buffer filled with `background` (opaque).
    pub fn new(width: u32, height: u32, background: Rgba) -> Self {
        Self {
            width,
            height,
            pixels: vec![background | 0xff; (width as usize) * (height as usize)],
            color: WHITE,
            stroke: 1.0,
            transform: IDENTITY,
        }
    }

    /// Command-space pixel (bottom-left origin) as RGBA8888.
    pub fn pixel(&self, x: i32, y: i32) -> Option<Rgba> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        let row = self.height as i32 - 1 - y;
        self.pixels
            .get((row as usize) * self.width as usize + x as usize)
            .copied()
    }

    /// Writes one command-space pixel.
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Rgba) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let row = self.height as i32 - 1 - y;
        let index = (row as usize) * self.width as usize + x as usize;
        if let Some(slot) = self.pixels.get_mut(index) {
            *slot = color;
        }
    }

    /// Blends `src` over one pixel (`SpriteBatch`'s
    /// `GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA`), so a transparent texel leaves
    /// the buffer untouched.
    fn blend_pixel(&mut self, x: i32, y: i32, src: Rgba) {
        let alpha = src & 0xff;
        if alpha == 0 {
            return;
        }
        if alpha == 0xff {
            self.set_pixel(x, y, src);
            return;
        }
        let Some(dst) = self.pixel(x, y) else {
            return;
        };
        let mix = |from: u32, to: u32| (from * alpha + to * (255 - alpha)) / 255;
        let channel = |value: Rgba, shift: u32| (value >> shift) & 0xff;
        let r = mix(channel(src, 24), channel(dst, 24));
        let g = mix(channel(src, 16), channel(dst, 16));
        let b = mix(channel(src, 8), channel(dst, 8));
        let a = mix(alpha, channel(dst, 0));
        self.set_pixel(x, y, (r << 24) | (g << 16) | (b << 8) | a);
    }

    /// Fills the whole buffer with the RGB of `color` at alpha 255
    /// (`Core.graphics.clear`).
    pub fn clear(&mut self, color: Rgba) {
        let rgb = (color >> 8) << 8;
        self.pixels.fill(rgb | 0xff);
    }

    /// Applies the affine transform to a point.
    fn xf(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.transform;
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// Scanline fill of a transformed polygon (pixel-centre sampling, no AA).
    fn fill_polygon(&mut self, points: &[(f32, f32)], color: Rgba) {
        let transformed: Vec<(f32, f32)> = points.iter().map(|(x, y)| self.xf(*x, *y)).collect();
        if transformed.len() < 3 {
            return;
        }
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (_, y) in &transformed {
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }
        let y0 = (min_y - 0.5).ceil().max(0.0) as i32;
        let y1 = (max_y - 0.5).floor().min(self.height as f32 - 1.0) as i32;
        let mut crossings: Vec<f32> = Vec::with_capacity(8);
        for py in y0..=y1 {
            let sample_y = py as f32 + 0.5;
            crossings.clear();
            for i in 0..transformed.len() {
                let (ax, ay) = transformed[i];
                let (bx, by) = transformed[(i + 1) % transformed.len()];
                // Half-open edge rule (`ay <= y < by` or the reverse).
                let (lo, hi, lox, hix) = if ay < by {
                    (ay, by, ax, bx)
                } else {
                    (by, ay, bx, ax)
                };
                if sample_y < lo || sample_y >= hi || hi - lo == 0.0 {
                    continue;
                }
                let t = (sample_y - lo) / (hi - lo);
                crossings.push(lox + (hix - lox) * t);
            }
            if crossings.len() < 2 {
                continue;
            }
            crossings.sort_by(f32::total_cmp);
            let mut i = 0;
            while i + 1 < crossings.len() {
                let (start, end) = (crossings[i], crossings[i + 1]);
                let px0 = (start - 0.5).ceil().max(0.0) as i32;
                let px1 = (end - 0.5).floor().min(self.width as f32 - 1.0) as i32;
                for px in px0..=px1 {
                    self.set_pixel(px, py, color);
                }
                i += 2;
            }
        }
    }

    /// `Fill.quad(x1,y1,x2,y2,x3,y3,x4,y4)`.
    fn quad(&mut self, p: [(f32, f32); 4], color: Rgba) {
        self.fill_polygon(&p, color);
    }

    /// `Fill.crect(x, y, w, h)`: a rectangle whose lower-left corner is
    /// `(x, y)` (`Draw.rect(white, x + w/2, y + h/2, w, h)`).
    fn crect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.quad([(x, y), (x + w, y), (x + w, y + h), (x, y + h)], color);
    }

    /// `Lines.line(x1, y1, x2, y2)` (`cap = true`): a `stroke`-wide quad with
    /// half-stroke caps on both ends (`arc Lines.line` non-legacy branch).
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, color: Rgba) {
        let hstroke = self.stroke / 2.0;
        let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
        if len <= f32::EPSILON {
            return;
        }
        let diffx = (x2 - x1) / len * hstroke;
        let diffy = (y2 - y1) / len * hstroke;
        self.quad(
            [
                (x1 - diffx - diffy, y1 - diffy + diffx),
                (x1 - diffx + diffy, y1 - diffy - diffx),
                (x2 + diffx + diffy, y2 + diffy - diffx),
                (x2 + diffx - diffy, y2 + diffy + diffx),
            ],
            color,
        );
    }

    /// `Lines.rect(x, y, w, h)` (space 0): four `stroke`-wide crects.
    fn line_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        let stroke = self.stroke;
        self.crect(x, y, w, stroke, color);
        self.crect(x, y + h, w, -stroke, color);
        self.crect(x + w, y, -stroke, h, color);
        self.crect(x, y, stroke, h, color);
    }

    /// `Fill.poly(x, y, sides, radius, rotation)`: the full regular polygon
    /// (the Arc triangle-fan union is identical for a convex polygon).
    fn fill_poly(&mut self, x: f32, y: f32, sides: i32, radius: f32, rotation: f32, color: Rgba) {
        if sides < 3 {
            // Arc's fan (`Fill.java:272-297`) draws no quads for `sides < 3`
            // and, for an odd count, one trailing `tri(center, i, i + 1)` with
            // `i = sides - 1` (Java `%` keeps the sign). `0..=2` sides are
            // degenerate; a negative odd count is a real sector triangle.
            if sides < 0 && sides % 2 != 0 {
                let space = 360.0 / sides as f32;
                let i = (sides - 1) as f32;
                let point = |angle: f32| {
                    let a = angle.to_radians();
                    (x + a.cos() * radius, y + a.sin() * radius)
                };
                let p1 = point(space * i + rotation);
                let p2 = point(space * (i + 1.0) + rotation);
                self.tri([(x, y), p1, p2], color);
            }
            return;
        }
        let space = 360.0 / sides as f32;
        let mut points = Vec::with_capacity(sides as usize);
        for i in 0..sides {
            let angle = (space * i as f32 + rotation).to_radians();
            points.push((x + angle.cos() * radius, y + angle.sin() * radius));
        }
        self.fill_polygon(&points, color);
    }

    /// `Lines.poly(x, y, sides, radius, startAngle, endAngle)`.
    fn line_poly(&mut self, x: f32, y: f32, sides: i32, radius: f32, rotation: f32, color: Rgba) {
        // `for(int i = 0; i < sides; i++)` (`Lines.java:398-411`): no quads
        // for `sides <= 0`.
        if sides <= 0 {
            return;
        }
        let space = 360.0 / sides as f32;
        let hstep = self.stroke / 2.0 / (space / 2.0).to_radians().cos();
        let r1 = radius - hstep;
        let r2 = radius + hstep;
        for i in 0..sides {
            let a = (space * i as f32 + rotation).to_radians();
            let a2 = (space * (i + 1) as f32 + rotation).to_radians();
            let (s1, c1) = a.sin_cos();
            let (s2, c2) = a2.sin_cos();
            self.quad(
                [
                    (x + r1 * c1, y + r1 * s1),
                    (x + r1 * c2, y + r1 * s2),
                    (x + r2 * c2, y + r2 * s2),
                    (x + r2 * c1, y + r2 * s1),
                ],
                color,
            );
        }
    }

    /// `Fill.tri(x1, y1, x2, y2, x3, y3)`.
    fn tri(&mut self, points: [(f32, f32); 3], color: Rgba) {
        self.fill_polygon(&points, color);
    }

    /// `Draw.rect(region, x, y, width, height, rotation)`: the source image is
    /// centred at `(x, y)`, sized `w x h`, rotated `rotation` degrees, then the
    /// current transform applies.
    #[allow(clippy::too_many_arguments)]
    fn blit(
        &mut self,
        image: ImageRef<'_>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rotation: f32,
        tint: Rgba,
    ) {
        if image.width == 0 || image.height == 0 || w == 0.0 || h == 0.0 {
            return;
        }
        let (sin, cos) = rotation.to_radians().sin_cos();
        // Local (image-centred) -> display space: scale, rotate, translate,
        // then the current affine transform.
        let rot = [cos, sin, -sin, cos, 0.0, 0.0];
        let translate = [1.0, 0.0, 0.0, 1.0, x, y];
        let m = mul(self.transform, mul(translate, rot));
        let corners = [
            (-w / 2.0, -h / 2.0),
            (w / 2.0, -h / 2.0),
            (w / 2.0, h / 2.0),
            (-w / 2.0, h / 2.0),
        ];
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut dst = [(0.0f32, 0.0f32); 4];
        for (i, (cx, cy)) in corners.iter().enumerate() {
            let px = m[0] * cx + m[2] * cy + m[4];
            let py = m[1] * cx + m[3] * cy + m[5];
            dst[i] = (px, py);
            min_x = min_x.min(px);
            max_x = max_x.max(px);
            min_y = min_y.min(py);
            max_y = max_y.max(py);
        }
        let Some(inv) = invert(m) else {
            return;
        };
        let x0 = (min_x - 0.5).ceil().max(0.0) as i32;
        let x1 = (max_x - 0.5).floor().min(self.width as f32 - 1.0) as i32;
        let y0 = (min_y - 0.5).ceil().max(0.0) as i32;
        let y1 = (max_y - 0.5).floor().min(self.height as f32 - 1.0) as i32;
        for py in y0..=y1 {
            for px in x0..=x1 {
                let (dx, dy) = (px as f32 + 0.5, py as f32 + 0.5);
                if !point_in_quad((dx, dy), &dst) {
                    continue;
                }
                let lx = inv[0] * dx + inv[2] * dy + inv[4];
                let ly = inv[1] * dx + inv[3] * dy + inv[5];
                let u = lx + w / 2.0;
                let v = h / 2.0 - ly;
                if u < 0.0 || v < 0.0 || u >= image.width as f32 || v >= image.height as f32 {
                    continue;
                }
                let sx = u as u32;
                let sy = v as u32;
                let Some(pixel) = image
                    .pixels
                    .get((sy as usize) * image.width as usize + sx as usize)
                    .copied()
                else {
                    continue;
                };
                // `Draw.rect` is alpha-blended, so a transparent texel (a
                // glyph-ish mask's pad row, a cut-out icon) leaves the buffer
                // alone instead of punching the background out.
                self.blend_pixel(px, py, mul_rgba(pixel, tint));
            }
        }
    }
}

/// `m1 * m2` in the column-vector convention used by [`DisplayRaster`].
fn mul(m1: [f32; 6], m2: [f32; 6]) -> [f32; 6] {
    [
        m1[0] * m2[0] + m1[2] * m2[1],
        m1[1] * m2[0] + m1[3] * m2[1],
        m1[0] * m2[2] + m1[2] * m2[3],
        m1[1] * m2[2] + m1[3] * m2[3],
        m1[0] * m2[4] + m1[2] * m2[5] + m1[4],
        m1[1] * m2[4] + m1[3] * m2[5] + m1[5],
    ]
}

/// Inverse affine (`None` when singular).
fn invert(m: [f32; 6]) -> Option<[f32; 6]> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let a = m[3] * inv;
    let b = -m[1] * inv;
    let c = -m[2] * inv;
    let d = m[0] * inv;
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

/// Point-in-convex-quad test (winding-insensitive).
fn point_in_quad(p: (f32, f32), quad: &[(f32, f32); 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (ax, ay) = quad[i];
        let (bx, by) = quad[(i + 1) % 4];
        let cross = (bx - ax) * (p.1 - ay) - (by - ay) * (p.0 - ax);
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// Multiplies an RGBA8888 pixel by an RGBA8888 tint.
fn mul_rgba(pixel: Rgba, tint: Rgba) -> Rgba {
    let channel = |value: u32, shift: u32| (value >> shift) & 0xff;
    let mul = |a: u32, b: u32| (a * b / 255) & 0xff;
    let r = mul(channel(pixel, 24), channel(tint, 24));
    let g = mul(channel(pixel, 16), channel(tint, 16));
    let b = mul(channel(pixel, 8), channel(tint, 8));
    let a = mul(channel(pixel, 0), channel(tint, 0));
    (r << 24) | (g << 16) | (b << 8) | a
}

/// `LogicDisplay.processCommands()`: drains the whole queue in order.
///
/// Returns `true` when at least one command was processed.
pub fn process_commands(
    raster: &mut DisplayRaster,
    commands: &mut VecDeque<u64>,
    images: &dyn DisplayImages,
) -> bool {
    let mut processed = false;
    while let Some(command) = commands.pop_front() {
        processed = true;
        let type_ = cmd_type(command);
        let x = unpack_sign(cmd_x(command)) as f32;
        let y = unpack_sign(cmd_y(command)) as f32;
        let p1 = unpack_sign(cmd_p1(command)) as f32;
        let p2 = unpack_sign(cmd_p2(command)) as f32;
        let p3 = unpack_sign(cmd_p3(command)) as f32;
        let p4 = unpack_sign(cmd_p4(command)) as f32;
        let color = raster.color;
        match type_ {
            COMMAND_CLEAR => raster.clear(rgba(x as i32, y as i32, p1 as i32, 255)),
            COMMAND_COLOR => raster.color = rgba(x as i32, y as i32, p1 as i32, p2 as i32),
            COMMAND_STROKE => raster.stroke = x,
            COMMAND_LINE => raster.line(x, y, p1, p2, color),
            COMMAND_RECT => raster.crect(x, y, p1, p2, color),
            COMMAND_LINE_RECT => raster.line_rect(x, y, p1, p2, color),
            COMMAND_POLY => raster.fill_poly(x, y, (p1 as i32).min(MAX_SIDES), p2, p3, color),
            COMMAND_LINE_POLY => raster.line_poly(x, y, (p1 as i32).min(MAX_SIDES), p2, p3, color),
            COMMAND_TRIANGLE => raster.tri([(x, y), (p1, p2), (p3, p4)], color),
            COMMAND_IMAGE => {
                let packed = ((p4 as i32) << 10) | p1 as i32;
                let ctype = packed & 0x1f;
                let id = packed >> 5;
                if ctype == DISPLAY_DRAW_TYPE {
                    if let Some(source) = images.display(id) {
                        let width = p2;
                        let height = if source.width == 0 {
                            p2
                        } else {
                            p2 * source.height as f32 / source.width as f32
                        };
                        raster.blit(source, x, y, width, height, p3, color);
                    }
                } else if ctype < 32
                    && let Some(icon) = images.icon(ctype as u8, id as u16)
                {
                    let width = p2;
                    let height = if icon.width == 0 {
                        p2
                    } else {
                        p2 * icon.height as f32 / icon.width as f32
                    };
                    raster.blit(
                        ImageRef {
                            width: icon.width,
                            height: icon.height,
                            pixels: &icon.pixels,
                        },
                        x,
                        y,
                        width,
                        height,
                        p3,
                        color,
                    );
                }
            }
            COMMAND_PRINT => {
                let glyph_char = char::from_u32(p1 as u32).unwrap_or('\0');
                // `LogicDisplay.java:228-229`: `getGlyph` is null for a
                // character outside `Fonts.logic`'s generated set, so nothing is
                // drawn (the layout still advanced the cursor).
                if crate::logic::font::has_glyph(glyph_char)
                    && let Some(glyph) = images.glyph(glyph_char)
                {
                    // `LogicDisplay.java:230-234`: the glyph region's centre,
                    // see [`Glyph::print_center`].
                    let (cx, cy) = glyph.print_center(x, y);
                    let gw = glyph.image.width as f32;
                    let gh = glyph.image.height as f32;
                    raster.blit(
                        ImageRef {
                            width: glyph.image.width,
                            height: glyph.image.height,
                            pixels: &glyph.image.pixels,
                        },
                        cx,
                        cy,
                        gw,
                        gh,
                        0.0,
                        color,
                    );
                }
            }
            COMMAND_TRANSLATE => raster.transform = mul(raster.transform, translation(x, y)),
            COMMAND_SCALE => {
                raster.transform = mul(raster.transform, scale(x * SCALE_STEP, y * SCALE_STEP));
            }
            COMMAND_ROTATE => raster.transform = mul(raster.transform, rotation(p1)),
            COMMAND_RESET_TRANSFORM => raster.transform = IDENTITY,
            _ => {}
        }
    }
    processed
}

/// `Mat.translate(x, y)`.
fn translation(x: f32, y: f32) -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

/// `Mat.scale(x, y)`.
fn scale(x: f32, y: f32) -> [f32; 6] {
    [x, 0.0, 0.0, y, 0.0, 0.0]
}

/// `Mat.rotate(degrees)` (counter-clockwise; the display space is y-up).
fn rotation(degrees: f32) -> [f32; 6] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [cos, sin, -sin, cos, 0.0, 0.0]
}

/// `Color.rgba8888` from four byte channels.
fn rgba(r: i32, g: i32, b: i32, a: i32) -> Rgba {
    let channel = |value: i32| (value.clamp(0, 255) as u32) & 0xff;
    (channel(r) << 24) | (channel(g) << 16) | (channel(b) << 8) | channel(a)
}

/// Resolved geometry of one root's buffer (`TileableLogicDisplay` origins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RasterGeometry {
    /// Buffer width in pixels.
    pub width: u32,
    /// Buffer height in pixels.
    pub height: u32,
    /// Bottom-left origin tile x.
    pub origin_x: i32,
    /// Bottom-left origin tile y.
    pub origin_y: i32,
}

/// View resource: root display entity → CPU raster (`LogicDisplayBuild.buffer`).
#[derive(Debug, Default, Clone, Resource)]
pub struct LogicDisplayRasters {
    /// One raster per root display.
    pub rasters: IndexMap<Entity, DisplayRaster>,
    /// Per-root buffer geometry (for the tileable merge/resize).
    pub geometry: IndexMap<Entity, RasterGeometry>,
}

/// Display-source lookup over the live arena (`TileableLogicDisplayBuild`
/// forwarding resolved to the root raster).
struct ArenaImages<'a> {
    /// Arena slot index -> root display entity
    /// (`displays.get(id).rootDisplay`).
    roots: &'a [Option<Entity>],
    rasters: &'a LogicDisplayRasters,
    ctx: &'a dyn DisplayDrawContext,
}

impl DisplayImages for ArenaImages<'_> {
    fn icon(&self, content_type: u8, id: u16) -> Option<RgbaImage> {
        self.ctx.icon(content_type, id)
    }
    fn glyph(&self, c: char) -> Option<Glyph> {
        self.ctx.glyph(c)
    }
    fn display(&self, index: i32) -> Option<ImageRef<'_>> {
        let root = (*self.roots.get(usize::try_from(index).ok()?)?)?;
        let raster = self.rasters.rasters.get(&root)?;
        Some(ImageRef {
            width: raster.width,
            height: raster.height,
            pixels: &raster.pixels,
        })
    }
}

/// The root's buffer dimensions (`LogicDisplay.ensureBuffer` /
/// `TileableLogicDisplayBuild.ensureBuffer`).
fn root_geometry(world: &World, root: Entity, state: &LogicDisplayState) -> (u32, u32) {
    if block_kind_of(world, root) == Some(BlockKind::TileableLogicDisplay) {
        let frame = 6;
        let width = (state.tiles_width * 32 - frame * 2).max(0) as u32;
        let height = (state.tiles_height * 32 - frame * 2).max(0) as u32;
        if width > 0 && height > 0 {
            return (width, height);
        }
    }
    let size = state.display_size.max(0) as u32;
    (size, size)
}

/// `LogicDisplayBuild.processCommands` + `ensureBuffer` for one root.
///
/// Drains the root's command queue into its CPU raster and returns whether any
/// command was processed. The VIEW calls this (T13); the sim never does,
/// matching Java (Java only processes when the display is drawn).
pub fn process_display(world: &mut World, root: Entity, ctx: &dyn DisplayDrawContext) -> bool {
    process_display_inner(world, root, ctx, &mut std::collections::HashSet::new())
}

/// `LogicDisplayBuild.processCommands`'s off-screen image-source pre-pass
/// (`LogicDisplay.java:163-178`): a `draw image` referencing another display
/// forces that display's queued commands through its own buffer first.
fn process_display_inner(
    world: &mut World,
    root: Entity,
    ctx: &dyn DisplayDrawContext,
    visiting: &mut std::collections::HashSet<Entity>,
) -> bool {
    if !visiting.insert(root) {
        // `!source.processing` guard: a reference cycle stops here.
        return false;
    }
    let sources = world
        .get::<LogicDisplayState>(root)
        .map(|state| referenced_display_roots(world, root, &state.commands))
        .unwrap_or_default();
    for source in sources {
        process_display_inner(world, source, ctx, visiting);
    }
    let processed = process_one_display(world, root, ctx);
    visiting.remove(&root);
    processed
}

/// `displays.get(id).rootDisplay` targets referenced by `draw image`.
fn referenced_display_roots(world: &World, root: Entity, commands: &VecDeque<u64>) -> Vec<Entity> {
    let mut out = Vec::new();
    for &command in commands {
        if cmd_type(command) != COMMAND_IMAGE {
            continue;
        }
        let p1 = unpack_sign(cmd_p1(command));
        let p4 = unpack_sign(cmd_p4(command));
        let packed = (p4 << 10) | p1;
        if packed & 0x1f != DISPLAY_DRAW_TYPE {
            continue;
        }
        let id = packed >> 5;
        if id < 0 {
            continue;
        }
        let Some(displays) = world.get_resource::<crate::logic::blocks::LogicDisplays>() else {
            continue;
        };
        let Some(Some(entity)) = displays.slots.get(id as usize) else {
            continue;
        };
        let source = if *entity == root {
            root
        } else {
            world
                .get::<LogicDisplayState>(*entity)
                .and_then(|state| state.root_display)
                .unwrap_or(*entity)
        };
        if source != root && !out.contains(&source) {
            out.push(source);
        }
    }
    out
}

fn process_one_display(world: &mut World, root: Entity, ctx: &dyn DisplayDrawContext) -> bool {
    let Some(mut state) = world.entity_mut(root).take::<LogicDisplayState>() else {
        return false;
    };
    if state.commands.is_empty() {
        world.entity_mut(root).insert(state);
        return false;
    }
    let (width, height) = root_geometry(world, root, &state);
    if width == 0 || height == 0 {
        world.entity_mut(root).insert(state);
        return false;
    }
    let mut rasters = world
        .remove_resource::<LogicDisplayRasters>()
        .unwrap_or_default();
    let geometry = RasterGeometry {
        width,
        height,
        origin_x: state.origin_x,
        origin_y: state.origin_y,
    };
    let previous = rasters.geometry.insert(root, geometry);
    let existed = rasters.rasters.contains_key(&root);
    let mut raster = rasters
        .rasters
        .shift_remove(&root)
        .unwrap_or_else(|| DisplayRaster::new(width, height, display_background()));
    if !existed && let Some(matrix) = &state.transform {
        // A loaded save restores the display transform (`LogicDisplay.java:272-286`);
        // the 2D affine parts of the 4x4 column-major `Mat` are
        // `[m00, m01, m10, m11, m30, m31]`.
        raster.transform = [
            matrix[0], matrix[1], matrix[4], matrix[5], matrix[12], matrix[13],
        ];
    }
    if raster.width != width || raster.height != height {
        // `MergeBuffer` (`TileableLogicDisplay.java:186-205`): copy the old
        // pixels at their absolute display position into the new buffer.
        let mut resized = DisplayRaster::new(width, height, display_background());
        if let Some(old) = previous {
            for old_row in 0..raster.height as i32 {
                for old_col in 0..raster.width as i32 {
                    let Some(pixel) = raster.pixel(old_col, raster.height as i32 - 1 - old_row)
                    else {
                        continue;
                    };
                    let absolute_x = old.origin_x * 32 + old_col;
                    let absolute_y = old.origin_y * 32 + (raster.height as i32 - 1 - old_row);
                    resized.set_pixel(
                        absolute_x - geometry.origin_x * 32,
                        absolute_y - geometry.origin_y * 32,
                        pixel,
                    );
                }
            }
        }
        raster = resized;
    }
    // `displays.get(id).rootDisplay` lookup for `commandImage` sources.
    let roots: Vec<Option<Entity>> = world
        .get_resource::<crate::logic::blocks::LogicDisplays>()
        .map(|displays| {
            displays
                .slots
                .iter()
                .map(|slot| {
                    slot.map(|entity| {
                        if entity == root {
                            return root;
                        }
                        world
                            .get::<LogicDisplayState>(entity)
                            .and_then(|state| state.root_display)
                            .unwrap_or(entity)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let processed = {
        let images = ArenaImages {
            roots: &roots,
            rasters: &rasters,
            ctx,
        };
        process_commands(&mut raster, &mut state.commands, &images)
    };
    rasters.rasters.insert(root, raster);
    world.insert_resource(rasters);
    world.entity_mut(root).insert(state);
    processed
}

/// `LogicDisplay.backgroundColor = Pal.darkerMetal`.
fn display_background() -> Rgba {
    crate::content::registries::pal::DARKER_METAL.to_rgba8888()
}

/// One visible display row for the render pass (T13).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayDrawRow {
    /// Root display tile (the raster/texture key).
    pub root_x: i16,
    /// Root display tile y.
    pub root_y: i16,
    /// Whether the buffer quad is drawn at `(x, y)`.
    pub draw_buffer: bool,
    /// Draw centre x (world pixels).
    pub x: f32,
    /// Draw centre y (world pixels).
    pub y: f32,
    /// Draw width (world pixels).
    pub w: f32,
    /// Draw height (world pixels).
    pub h: f32,
    /// Whether this tile draws the tileable frame sprite.
    pub frame: bool,
    /// Frame centre x (world pixels).
    pub frame_x: f32,
    /// Frame centre y (world pixels).
    pub frame_y: f32,
    /// 8-neighbour mask (`TileableLogicDisplayBuild.bits`).
    pub bits: u8,
    /// `TileBitmask.values[bits]` (frame sprite suffix).
    pub sprite: u8,
}

/// Whether a display's building centre lies inside the camera rect.
fn display_visible(
    world: &World,
    e: Entity,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> bool {
    let Some(building) = world.get::<crate::entities::comp::Building>(e) else {
        return false;
    };
    let offset =
        ((crate::logic::blocks::block_size_of(world, e).unwrap_or(1) + 1) % 2) as f32 * 4.0;
    let x = f32::from(building.tile.x()) * 8.0 + offset;
    let y = f32::from(building.tile.y()) * 8.0 + offset;
    x >= min_x && x <= max_x && y >= min_y && y <= max_y
}

/// `LogicDisplayBuild.draw` selection for one camera rect (T13).
///
/// Processes every visible root (`process_display`) and returns the draw rows
/// the render pass needs: one row per visible display tile with the buffer quad
/// (`drawFull` for a rectangular tileable group) and the tileable frame sprite.
pub fn display_rows(
    world: &mut World,
    ctx: &dyn DisplayDrawContext,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Vec<DisplayDrawRow> {
    let mut displays: Vec<(u64, Entity)> = {
        let mut query = world.query::<(Entity, &LogicDisplayState)>();
        query
            .iter(world)
            .map(|(e, _)| {
                let seq = world
                    .get::<crate::ecs::EntitySeq>(e)
                    .map(|seq| seq.0)
                    .unwrap_or(u64::MAX);
                (seq, e)
            })
            .collect()
    };
    displays.sort_by_key(|(seq, e)| (*seq, e.index().index()));
    if displays.is_empty() {
        return Vec::new();
    }
    // Process every root with at least one visible member.
    let mut roots: Vec<(Entity, bool)> = Vec::new();
    for (_, e) in &displays {
        let root = world
            .get::<LogicDisplayState>(*e)
            .and_then(|state| state.root_display)
            .unwrap_or(*e);
        let visible = display_visible(world, *e, min_x, min_y, max_x, max_y);
        match roots.iter_mut().find(|(candidate, _)| *candidate == root) {
            Some(entry) => entry.1 |= visible,
            None => roots.push((root, visible)),
        }
    }
    for (root, visible) in roots {
        if visible {
            let _ = process_display(world, root, ctx);
        }
    }
    let rasters = world
        .get_resource::<LogicDisplayRasters>()
        .cloned()
        .unwrap_or_default();
    let mut rows = Vec::new();
    for (_, e) in displays {
        if !display_visible(world, e, min_x, min_y, max_x, max_y) {
            continue;
        }
        let Some(building) = world.get::<crate::entities::comp::Building>(e) else {
            continue;
        };
        let offset =
            ((crate::logic::blocks::block_size_of(world, e).unwrap_or(1) + 1) % 2) as f32 * 4.0;
        let (cx, cy) = (
            f32::from(building.tile.x()) * 8.0 + offset,
            f32::from(building.tile.y()) * 8.0 + offset,
        );
        let state = world.get::<LogicDisplayState>(e);
        let root = state.and_then(|state| state.root_display).unwrap_or(e);
        let root_state = world.get::<LogicDisplayState>(root);
        let Some(geometry) = rasters.geometry.get(&root).copied() else {
            continue;
        };
        let scale = crate::logic::blocks::display_def_of(world, e)
            .map(|def| def.scale_factor)
            .filter(|scale| *scale > 0.0)
            .unwrap_or(1.0);
        let (root_x, root_y) = world
            .get::<crate::entities::comp::Building>(root)
            .map(|building| (building.tile.x(), building.tile.y()))
            .unwrap_or((building.tile.x(), building.tile.y()));
        let tileable = crate::logic::blocks::block_kind_of(world, e)
            == Some(crate::content::BlockKind::TileableLogicDisplay);
        let bits = state.map(|state| state.bits).unwrap_or(0);
        let mut row = DisplayDrawRow {
            root_x,
            root_y,
            draw_buffer: false,
            x: cx,
            y: cy,
            w: geometry.width as f32 * scale * 0.25,
            h: geometry.height as f32 * scale * 0.25,
            frame: tileable,
            frame_x: cx,
            frame_y: cy,
            bits,
            sprite: crate::render::tile_bitmask::value(bits),
        };
        if tileable {
            // `drawFull` (`TileableLogicDisplay.java:230-235`): the root draws
            // the whole buffer once per frame; non-rectangular groups fall back
            // to per-tile slices in the view pass.
            let rectangular = root_state.is_some_and(|state| state.rectangular);
            if rectangular
                && root == e
                && let Some(state) = root_state
            {
                let (tiles_w, tiles_h) = (state.tiles_width as f32, state.tiles_height as f32);
                let rel_x = f32::from(building.tile.x()) - f32::from(root_x);
                let rel_y = f32::from(building.tile.y()) - f32::from(root_y);
                row.x = cx + 8.0 * (tiles_w - 1.0 - 2.0 * rel_x) / 2.0;
                row.y = cy + 8.0 * (tiles_h - 1.0 - 2.0 * rel_y) / 2.0;
                row.draw_buffer = true;
            }
        } else {
            row.draw_buffer = true;
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(w: u32, h: u32) -> DisplayRaster {
        DisplayRaster::new(w, h, 0x0000_00ff)
    }

    fn cmd(type_: u8, x: i32, y: i32, p1: i32, p2: i32, p3: i32, p4: i32) -> u64 {
        // `DrawI` sign-packs every field before `DisplayCmd.get`; the raw
        // `pack_cmd` helper expects the packed values.
        let field = crate::logic::executor::draw::pack_sign;
        crate::logic::executor::draw::pack_cmd(
            type_,
            field(x),
            field(y),
            field(p1),
            field(p2),
            field(p3),
            field(p4),
        )
    }

    /// Draw context with one fixed glyph for every character.
    struct TestImages {
        glyph: Option<Glyph>,
    }

    impl TestImages {
        fn glyph(glyph: Glyph) -> Self {
            Self { glyph: Some(glyph) }
        }
    }

    impl DisplayDrawContext for TestImages {
        fn icon(&self, _content_type: u8, _id: u16) -> Option<RgbaImage> {
            None
        }
        fn glyph(&self, _c: char) -> Option<Glyph> {
            self.glyph.clone()
        }
    }

    impl DisplayImages for TestImages {
        fn icon(&self, content_type: u8, id: u16) -> Option<RgbaImage> {
            DisplayDrawContext::icon(self, content_type, id)
        }
        fn glyph(&self, c: char) -> Option<Glyph> {
            DisplayDrawContext::glyph(self, c)
        }
        fn display(&self, _index: i32) -> Option<ImageRef<'_>> {
            None
        }
    }

    /// The real `logic.ttf` 'A' mask from Godot's `FontFile` at 16 px: an 8x10
    /// padded rect with a 6x8 glyph inset by 1 px on every side, the rect top
    /// 9 px above the baseline.
    fn capped_glyph() -> Glyph {
        let mut pixels = vec![0u32; 8 * 10];
        for row in 1..9 {
            for col in 1..7 {
                pixels[row * 8 + col] = WHITE;
            }
        }
        Glyph {
            image: RgbaImage {
                width: 8,
                height: 10,
                pixels,
            },
            x_offset: 0.0,
            y_offset: -9.0,
            cap_height: 7.0,
            ascent: 3.0,
        }
    }

    /// `print` places the glyph rect with Java's `LogicDisplay` geometry
    /// (`LogicDisplay.java:230-234`): for the 8x10 padded mask above, the
    /// content lands in `[x+1, x+7) x [y, y+8)`; the transparent pad ring must
    /// not touch the buffer.
    #[test]
    fn print_places_glyphs_with_java_geometry() {
        let mut r = raster(80, 80);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_PRINT, 20, 10, 'A' as i32, 0, 0, 0));
        let images = TestImages::glyph(capped_glyph());
        assert!(process_commands(&mut r, &mut queue, &images));
        for y in 0..80 {
            for x in 0..80 {
                let expected = (21..27).contains(&x) && (10..18).contains(&y);
                let pixel = r.pixel(x, y).unwrap_or(0);
                assert_eq!(
                    pixel,
                    if expected { WHITE } else { 0x0000_00ff },
                    "pixel ({x},{y})"
                );
            }
        }
    }

    /// A small glyph sits on the baseline too: 'a' is 7x8 with a 5x6 inset.
    #[test]
    fn print_places_a_lowercase_glyph_on_the_baseline() {
        let mut pixels = vec![0u32; 7 * 8];
        for row in 1..7 {
            for col in 1..6 {
                pixels[row * 7 + col] = WHITE;
            }
        }
        let glyph = Glyph {
            image: RgbaImage {
                width: 7,
                height: 8,
                pixels,
            },
            x_offset: 0.0,
            y_offset: -7.0,
            cap_height: 7.0,
            ascent: 3.0,
        };
        let mut r = raster(32, 32);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_PRINT, 8, 8, 'a' as i32, 0, 0, 0));
        assert!(process_commands(
            &mut r,
            &mut queue,
            &TestImages::glyph(glyph)
        ));
        for y in 0..32 {
            for x in 0..32 {
                let expected = (9..14).contains(&x) && (8..14).contains(&y);
                assert_eq!(
                    r.pixel(x, y),
                    Some(if expected { WHITE } else { 0x0000_00ff }),
                    "pixel ({x},{y})"
                );
            }
        }
    }

    /// `LogicDisplay.java:228-229`: a character outside `Fonts.logic`'s
    /// generated set has no glyph, so nothing is drawn even if a provider
    /// returns a bitmap for it. Space has a glyph (`getGlyph(' ')`) but a
    /// zero-height region (`FreeTypeFontGenerator.java:508-517`), so it draws
    /// nothing either.
    #[test]
    fn print_skips_characters_outside_the_generated_set() {
        let mut r = raster(16, 16);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_PRINT, 4, 4, 'é' as i32, 0, 0, 0));
        // U+20AC is 172 after the 10-bit `DisplayCmd.p1` truncation, which the
        // generated set does not contain either; 402 (`ƒ`) is outside it too.
        queue.push_back(cmd(COMMAND_PRINT, 4, 4, 172, 0, 0, 0));
        queue.push_back(cmd(COMMAND_PRINT, 4, 4, 402, 0, 0, 0));
        let images = TestImages::glyph(capped_glyph());
        assert!(process_commands(&mut r, &mut queue, &images));
        assert!(
            r.pixels.iter().all(|pixel| *pixel == 0x0000_00ff),
            "no pixel is drawn"
        );
    }

    /// `Draw.rect` blends (`SpriteBatch` `GL_SRC_ALPHA,
    /// GL_ONE_MINUS_SRC_ALPHA`): transparent texels leave the buffer alone and
    /// partial alpha mixes with the destination.
    #[test]
    fn blit_blends_transparent_and_partial_texels() {
        let mut r = raster(8, 8);
        // Row 0 (top) is [transparent, white]; row 1 is [50% white, white].
        let image = RgbaImage {
            width: 2,
            height: 2,
            pixels: vec![0x0000_0000, 0xffff_ffff, 0xffff_ff80, 0xffff_ffff],
        };
        r.blit(
            ImageRef {
                width: image.width,
                height: image.height,
                pixels: &image.pixels,
            },
            4.0,
            4.0,
            2.0,
            2.0,
            0.0,
            WHITE,
        );
        assert_eq!(r.pixel(3, 4), Some(0x0000_00ff), "transparent texel");
        assert_eq!(r.pixel(4, 4), Some(WHITE), "opaque texel");
        assert_eq!(r.pixel(4, 3), Some(WHITE), "opaque texel");
        // 50% white over the black background: `(255*128 + 0*127) / 255 = 128`
        // for r/g/b; the destination alpha 255 mixes to 191.
        assert_eq!(r.pixel(3, 3), Some(0x8080_80bf), "half-alpha texel");
    }

    /// `clear` fills the whole buffer with the RGB and alpha 255.
    #[test]
    fn clear_fills_buffer() {
        let mut r = raster(8, 8);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_CLEAR, 255, 0, 0, 0, 0, 0));
        assert!(process_commands(&mut r, &mut queue, &NoImages));
        assert!(queue.is_empty());
        assert!(r.pixels.iter().all(|pixel| *pixel == 0xff00_00ff));
    }

    /// `rect 10 10 20 20` on an 80x80 buffer covers pixels x/y 10..29
    /// (bottom-left origin, pixel-centre sampling).
    #[test]
    fn rect_covers_expected_pixels() {
        let mut r = raster(80, 80);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_RECT, 10, 10, 20, 20, 0, 0));
        assert!(process_commands(&mut r, &mut queue, &NoImages));
        for y in 0..80 {
            for x in 0..80 {
                let expected = (10..30).contains(&x) && (10..30).contains(&y);
                let pixel = r.pixel(x, y).unwrap_or(0);
                if expected {
                    assert_eq!(pixel, WHITE, "pixel ({x},{y}) is filled");
                } else {
                    assert_eq!(pixel, 0x0000_00ff, "pixel ({x},{y}) is background");
                }
            }
        }
        // `Flush`-style readback of a pixel uses the same command space.
        assert_eq!(r.pixels[0], 0x0000_00ff);
    }

    /// `Lines.line` uses the current stroke width.
    #[test]
    fn line_with_stroke() {
        let mut r = raster(64, 64);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_STROKE, 3, 0, 0, 0, 0, 0));
        queue.push_back(cmd(COMMAND_LINE, 10, 10, 30, 10, 0, 0));
        assert!(process_commands(&mut r, &mut queue, &NoImages));
        assert_eq!(r.pixel(20, 10), Some(WHITE), "on the line");
        assert_eq!(r.pixel(20, 9), Some(WHITE), "half stroke below");
        assert_eq!(r.pixel(20, 12), Some(0x0000_00ff), "outside the stroke");
        assert_eq!(r.pixel(5, 10), Some(0x0000_00ff), "before the line");
    }

    /// A hexagon of radius 20 has area `2.598 * r^2`; pixel-centre sampling
    /// lands within 2%. At radius 10 the raster fills exactly the 268 pixel
    /// centres inside the ideal hexagon (counted analytically): the 3.15% gap
    /// to the 259.8 area there is the sampling grid, not the rasterizer, so
    /// the 2% area check uses a radius where that grid error is ~1%.
    #[test]
    fn poly_hexagon_area_within_2pct() {
        let filled_for = |radius: i32| {
            let mut r = raster(80, 80);
            let mut queue = VecDeque::new();
            queue.push_back(cmd(COMMAND_POLY, 40, 40, 6, radius, 0, 0));
            assert!(process_commands(&mut r, &mut queue, &NoImages));
            r.pixels.iter().filter(|pixel| **pixel == WHITE).count()
        };
        assert_eq!(filled_for(10), 268, "exact pixel-centre count at r=10");
        let filled = filled_for(20) as f32;
        let expected = 2.598_076 * 400.0;
        let error = (filled - expected).abs() / expected;
        assert!(
            error < 0.02,
            "hexagon area {filled} vs {expected} ({:.2}% off)",
            error * 100.0
        );
    }

    /// `Math.min(p1, maxSides)` (`LogicDisplay.java:208-209`): `poly` and
    /// `linePoly` with more than 25 sides draw exactly the 25-gon.
    #[test]
    fn poly_sides_capped_at_max_sides() {
        let draw = |type_: u8, sides: i32| {
            let mut r = raster(80, 80);
            let mut queue = VecDeque::new();
            queue.push_back(cmd(type_, 40, 40, sides, 30, 0, 0));
            assert!(process_commands(&mut r, &mut queue, &NoImages));
            r.pixels
        };
        for type_ in [COMMAND_POLY, COMMAND_LINE_POLY] {
            let capped = draw(type_, MAX_SIDES);
            assert_eq!(draw(type_, 200), capped, "type {type_}: 200 sides -> 25");
            assert_eq!(draw(type_, 26), capped, "type {type_}: 26 sides -> 25");
            assert_ne!(draw(type_, 24), capped, "type {type_}: 24 sides differ");
        }
    }

    /// Arc `Fill.poly` below three sides: `0..=2` and negative even counts
    /// draw nothing; a negative odd count draws Arc's trailing fan triangle
    /// (`Fill.java:272-297`). `Lines.poly` draws nothing for `sides <= 0`.
    #[test]
    fn poly_degenerate_side_counts_follow_arc() {
        let filled = |type_: u8, sides: i32| {
            let mut r = raster(80, 80);
            let mut queue = VecDeque::new();
            queue.push_back(cmd(type_, 40, 40, sides, 20, 0, 0));
            assert!(process_commands(&mut r, &mut queue, &NoImages));
            r.pixels.iter().filter(|pixel| **pixel == WHITE).count()
        };
        for sides in [0, 1, 2, -2, -4] {
            assert_eq!(filled(COMMAND_POLY, sides), 0, "poly {sides} sides");
        }
        // `sides = -3`: `space = -120`, `i = -4` -> tri(center, 480deg, 360deg),
        // a 120-degree sector triangle of area `r^2 * sin(120) / 2` ~ 173.
        let sector = filled(COMMAND_POLY, -3);
        assert!((150..=200).contains(&sector), "poly -3 sides: {sector}");
        for sides in [0, -1, -5] {
            assert_eq!(filled(COMMAND_LINE_POLY, sides), 0, "linePoly {sides}");
        }
    }

    /// `translate 10 10` then `rect 0 0 5 5` lands at 10..14; `resetTransform`
    /// returns to the identity.
    #[test]
    fn transform_translate_then_rect() {
        let mut r = raster(32, 32);
        let mut queue = VecDeque::new();
        queue.push_back(cmd(COMMAND_TRANSLATE, 10, 10, 0, 0, 0, 0));
        queue.push_back(cmd(COMMAND_RECT, 0, 0, 5, 5, 0, 0));
        assert!(process_commands(&mut r, &mut queue, &NoImages));
        assert_eq!(r.pixel(10, 10), Some(WHITE));
        assert_eq!(r.pixel(14, 14), Some(WHITE));
        assert_eq!(r.pixel(15, 15), Some(0x0000_00ff));
        assert_eq!(r.pixel(9, 9), Some(0x0000_00ff));
        queue.push_back(cmd(COMMAND_RESET_TRANSFORM, 0, 0, 0, 0, 0, 0));
        queue.push_back(cmd(COMMAND_RECT, 0, 0, 5, 5, 0, 0));
        assert!(process_commands(&mut r, &mut queue, &NoImages));
        assert_eq!(r.pixel(2, 2), Some(WHITE));
    }

    /// `rotate 90` then `rect` rotates counter-clockwise about the origin
    /// (`x' = -y`, `y' = x`).
    #[test]
    fn transform_rotate_then_rect() {
        let mut r = raster(32, 32);
        let mut queue = VecDeque::new();
        // The rotation angle travels in `p1` (`LExecutor.DrawI`/`LogicDisplay`).
        queue.push_back(cmd(COMMAND_ROTATE, 0, 0, 90, 0, 0, 0));
        // x in [5,9), y in [-9,-5) -> x' in (5,9], y' in [5,9).
        queue.push_back(cmd(COMMAND_RECT, 5, -9, 4, 4, 0, 0));
        assert!(process_commands(&mut r, &mut queue, &NoImages));
        assert_eq!(
            r.pixel(6, 6),
            Some(WHITE),
            "rect lands in the first quadrant"
        );
        assert_eq!(r.pixel(6, 2), Some(0x0000_00ff), "nothing at the old place");
    }
}
