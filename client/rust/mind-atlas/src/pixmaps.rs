// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Behavioral reimplementation of Arc (Apache-2.0, revision 7445105cd2):
// `arc-core/src/arc/graphics/Pixmap.java`, `arc-core/src/arc/graphics/Pixmaps.java`,
// `arc-core/src/arc/math/geom/Geometry.java` (circle iteration used by `median`).

//! RGBA8888 pixmap and image ops (plan 03 §3.5/A3).
//!
//! Pixel values are `u32` in `r << 24 | g << 16 | b << 8 | a` order, matching
//! Arc's `Pixmap.getRaw`. Coordinates are top-left origin, y down.

use crate::error::{AtlasError, Result};

/// Solid white (`Color.whiteRgba`).
pub const WHITE: u32 = 0xffff_ffff;
/// Fully transparent (`Color.clearRgba`).
pub const CLEAR: u32 = 0;

/// Red channel of an RGBA8888 pixel.
pub fn ri(rgba: u32) -> u32 {
    (rgba & 0xff00_0000) >> 24
}

/// Green channel of an RGBA8888 pixel.
pub fn gi(rgba: u32) -> u32 {
    (rgba & 0x00ff_0000) >> 16
}

/// Blue channel of an RGBA8888 pixel.
pub fn bi(rgba: u32) -> u32 {
    (rgba & 0x0000_ff00) >> 8
}

/// Alpha channel of an RGBA8888 pixel.
pub fn ai(rgba: u32) -> u32 {
    rgba & 0x0000_00ff
}

/// Packs channels into an RGBA8888 pixel (`Color.rgba8888`).
pub fn rgba8888(r: u32, g: u32, b: u32, a: u32) -> u32 {
    (r << 24) | (g << 16) | (b << 8) | a
}

/// Float-channel pack (`Color.rgba8888(float...)`).
pub fn rgba8888f(r: f32, g: f32, b: f32, a: f32) -> u32 {
    rgba8888(
        (r.clamp(0.0, 1.0) * 255.0) as u32,
        (g.clamp(0.0, 1.0) * 255.0) as u32,
        (b.clamp(0.0, 1.0) * 255.0) as u32,
        (a.clamp(0.0, 1.0) * 255.0) as u32,
    )
}

/// Multiplies every channel of `rgba` by the channels of `other` / 255
/// (`Color.muli`).
pub fn muli(rgba: u32, other: u32) -> u32 {
    rgba8888(
        ri(rgba) * ri(other) / 255,
        gi(rgba) * gi(other) / 255,
        bi(rgba) * bi(other) / 255,
        ai(rgba) * ai(other) / 255,
    )
}

/// Source-over blend (`Pixmap.blend`).
pub fn blend(src: u32, dst: u32) -> u32 {
    let src_a = src & 0xff;
    if src_a == 0 {
        return dst;
    }
    let dst_a = dst & 0xff;
    if dst_a == 0 {
        return src;
    }
    let dst_b = (dst >> 8) & 0xff;
    let dst_g = (dst >> 16) & 0xff;
    let dst_r = (dst >> 24) & 0xff;

    let dst_a = dst_a - (dst_a * src_a) / 255;
    let a = dst_a + src_a;
    let out_r = (dst_r * dst_a + ((src >> 24) & 0xff) * src_a) / a;
    let out_g = (dst_g * dst_a + ((src >> 16) & 0xff) * src_a) / a;
    let out_b = (dst_b * dst_a + ((src >> 8) & 0xff) * src_a) / a;
    (out_r << 24) | (out_g << 16) | (out_b << 8) | a
}

/// Whether the alpha of a packed pixel is 0 (`Pixmap.empty`).
pub fn empty(rgba: u32) -> bool {
    rgba & 0xff == 0
}

/// RGBA8888 image, top-left origin (`Pixmap`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixmap {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// RGBA bytes, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

impl Pixmap {
    /// Creates a new transparent pixmap (`Pixmap(width, height)`).
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height * 4],
        }
    }

    /// Builds from raw RGBA bytes.
    pub fn from_raw(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self> {
        if pixels.len() != width * height * 4 {
            return Err(AtlasError::Invalid(format!(
                "raw pixel buffer {} bytes != {}x{}x4",
                pixels.len(),
                width,
                height
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        (x + y * self.width) * 4
    }

    /// Pixel at `(x, y)` or 0 out of bounds (`Pixmap.get`).
    pub fn get(&self, x: i32, y: i32) -> u32 {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            self.get_raw(x as usize, y as usize)
        } else {
            0
        }
    }

    /// Pixel at `(x, y)` clamped to bounds (`Pixmap.getClamp`).
    pub fn get_clamp(&self, x: i32, y: i32) -> u32 {
        let cx = x.clamp(0, self.width.saturating_sub(1) as i32) as usize;
        let cy = y.clamp(0, self.height.saturating_sub(1) as i32) as usize;
        self.get_raw(cx, cy)
    }

    /// Pixel at `(x, y)`; no bounds checks (`Pixmap.getRaw`).
    pub fn get_raw(&self, x: usize, y: usize) -> u32 {
        let i = self.index(x, y);
        u32::from_be_bytes([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ])
    }

    /// Alpha byte at `(x, y)`; no bounds checks (`Pixmap.getA`).
    pub fn get_a(&self, x: usize, y: usize) -> u8 {
        self.pixels[self.index(x, y) + 3]
    }

    /// Whether the pixel at `(x, y)` has alpha 0; no bounds checks (`Pixmap.empty`).
    pub fn empty_at(&self, x: usize, y: usize) -> bool {
        self.get_a(x, y) == 0
    }

    /// Whether `(x, y)` is inside the pixmap (`Pixmap.in`).
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// Sets the pixel if inside bounds (`Pixmap.set`).
    pub fn set(&mut self, x: i32, y: i32, color: u32) {
        if self.contains(x, y) {
            self.set_raw(x as usize, y as usize, color);
        }
    }

    /// Sets the pixel; no bounds checks (`Pixmap.setRaw`).
    pub fn set_raw(&mut self, x: usize, y: usize, color: u32) {
        let i = self.index(x, y);
        self.pixels[i..i + 4].copy_from_slice(&color.to_be_bytes());
    }

    /// Fills the pixmap with `color` (`Pixmap.fill`).
    pub fn fill(&mut self, color: u32) {
        let bytes = color.to_be_bytes();
        let (chunks, _) = self.pixels.as_chunks_mut::<4>();
        for chunk in chunks {
            *chunk = bytes;
        }
    }

    /// Deep copy (`Pixmap.copy`).
    pub fn copy(&self) -> Self {
        self.clone()
    }

    /// Maps every pixel through `f` (`Pixmap.replace`).
    pub fn replace(&mut self, f: impl Fn(u32) -> u32) {
        for y in 0..self.height {
            for x in 0..self.width {
                let value = f(self.get_raw(x, y));
                self.set_raw(x, y, value);
            }
        }
    }

    /// Iterates every position (`Pixmap.each`).
    pub fn each(&self, mut f: impl FnMut(usize, usize)) {
        for y in 0..self.height {
            for x in 0..self.width {
                f(x, y);
            }
        }
    }

    /// Cropped copy; out-of-bounds source pixels read as 0 (`Pixmap.crop`).
    pub fn crop(&self, x: i32, y: i32, width: usize, height: usize) -> Self {
        let mut out = Pixmap::new(width, height);
        out.draw_region(self, x, y, width, height, 0, 0);
        out
    }

    /// Horizontally flipped copy (`Pixmap.flipX`).
    pub fn flip_x(&self) -> Self {
        let mut out = Pixmap::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let value = self.get_raw(x, y);
                out.set_raw(self.width - 1 - x, y, value);
            }
        }
        out
    }

    /// Vertically flipped copy (`Pixmap.flipY`).
    pub fn flip_y(&self) -> Self {
        let mut out = Pixmap::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let value = self.get_raw(x, y);
                out.set_raw(x, self.height - 1 - y, value);
            }
        }
        out
    }

    /// Same-size blit at `(dx, dy)` (`Pixmap.draw(pixmap, x, y)`); source pixels
    /// outside the source are skipped, target pixels outside are skipped.
    pub fn draw(&mut self, src: &Pixmap, dx: i32, dy: i32) {
        self.draw_region(src, 0, 0, src.width, src.height, dx, dy);
    }

    /// Same-size blit with source-over blending (`Pixmap.draw(pixmap, x, y, true)`).
    pub fn draw_blended(&mut self, src: &Pixmap, dx: i32, dy: i32) {
        self.draw_full(
            src, 0, 0, src.width, src.height, dx, dy, src.width, src.height, true,
        );
    }

    /// Same-size sub-region blit (`Pixmap.draw(pixmap, x, y, srcx, srcy, w, h)`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_region(
        &mut self,
        src: &Pixmap,
        srcx: i32,
        srcy: i32,
        width: usize,
        height: usize,
        dx: i32,
        dy: i32,
    ) {
        self.draw_full(src, srcx, srcy, width, height, dx, dy, width, height, false);
    }

    /// Full draw: scale/stretch with nearest filtering, optional blend
    /// (`Pixmap.draw(src, srcx, srcy, sw, sh, dx, dy, dw, dh, filtering, blending)`).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::needless_range_loop)]
    pub fn draw_full(
        &mut self,
        src: &Pixmap,
        srcx: i32,
        srcy: i32,
        src_width: usize,
        src_height: usize,
        dstx: i32,
        dsty: i32,
        dst_width: usize,
        dst_height: usize,
        blending: bool,
    ) {
        self.draw_filtered(
            src, srcx, srcy, src_width, src_height, dstx, dsty, dst_width, dst_height, false,
            blending,
        );
    }

    /// Full draw with filter selection (`Pixmap.draw(..., filtering, blending)`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_filtered(
        &mut self,
        src: &Pixmap,
        srcx: i32,
        srcy: i32,
        src_width: usize,
        src_height: usize,
        dstx: i32,
        dsty: i32,
        dst_width: usize,
        dst_height: usize,
        filtering: bool,
        blending: bool,
    ) {
        if src_width == 0 || src_height == 0 || dst_width == 0 || dst_height == 0 {
            return;
        }
        if src_width == dst_width && src_height == dst_height {
            // Same-size blit.
            for i in 0..src_height {
                let sy = srcy + i as i32;
                let dy = dsty + i as i32;
                if sy < 0 || dy < 0 {
                    continue;
                }
                if sy as usize >= src.height || dy as usize >= self.height {
                    break;
                }
                for j in 0..src_width {
                    let sx = srcx + j as i32;
                    let dx = dstx + j as i32;
                    if sx < 0 || dx < 0 {
                        continue;
                    }
                    if sx as usize >= src.width || dx as usize >= self.width {
                        break;
                    }
                    let (sxu, dxu, syu, dyu) = (sx as usize, dx as usize, sy as usize, dy as usize);
                    let value = src.get_raw(sxu, syu);
                    let out = if blending {
                        blend(value, self.get_raw(dxu, dyu))
                    } else {
                        value
                    };
                    self.set_raw(dxu, dyu, out);
                }
            }
        } else if filtering {
            // Bilinear stretch.
            let x_ratio = (src_width as f32 - 1.0) / dst_width as f32;
            let y_ratio = (src_height as f32 - 1.0) / dst_height as f32;
            let rx = x_ratio.round().max(1.0) as i32;
            let ry = y_ratio.round().max(1.0) as i32;
            for i in 0..dst_height {
                let fy = y_ratio * i as f32 + srcy as f32;
                let sy = fy as i32;
                let dy = i as i32 + dsty;
                let ydiff = fy - sy as f32;
                if sy < 0 || dy < 0 {
                    continue;
                }
                if sy as usize >= src.height || dy as usize >= self.height {
                    break;
                }
                for j in 0..dst_width {
                    let fx = x_ratio * j as f32 + srcx as f32;
                    let sx = fx as i32;
                    let dx = j as i32 + dstx;
                    let xdiff = fx - sx as f32;
                    if sx < 0 || dx < 0 {
                        continue;
                    }
                    if sx as usize >= src.width || dx as usize >= self.width {
                        break;
                    }
                    let c1 = src.get_raw(sx as usize, sy as usize);
                    let c2 = if sx + rx < src_width as i32 {
                        src.get_raw((sx + rx) as usize, sy as usize)
                    } else {
                        c1
                    };
                    let c3 = if sy + ry < src_height as i32 {
                        src.get_raw(sx as usize, (sy + ry) as usize)
                    } else {
                        c1
                    };
                    let c4 = if sx + rx < src_width as i32 && sy + ry < src_height as i32 {
                        src.get_raw((sx + rx) as usize, (sy + ry) as usize)
                    } else {
                        c1
                    };
                    let ta = (1.0 - xdiff) * (1.0 - ydiff);
                    let tb = xdiff * (1.0 - ydiff);
                    let tc = (1.0 - xdiff) * ydiff;
                    let td = xdiff * ydiff;
                    let mix = |shift: u32| -> u32 {
                        ((c1 >> shift & 0xff) as f32 * ta
                            + (c2 >> shift & 0xff) as f32 * tb
                            + (c3 >> shift & 0xff) as f32 * tc
                            + (c4 >> shift & 0xff) as f32 * td) as u32
                            & 0xff
                    };
                    let value = (mix(24) << 24) | (mix(16) << 16) | (mix(8) << 8) | mix(0);
                    let out = if blending {
                        blend(value, self.get_raw(dx as usize, dy as usize))
                    } else {
                        value
                    };
                    self.set_raw(dx as usize, dy as usize, out);
                }
            }
        } else {
            // Nearest-neighbor stretch.
            let xratio = ((src_width as u64) << 16) / dst_width as u64 + 1;
            let yratio = ((src_height as u64) << 16) / dst_height as u64 + 1;
            for i in 0..dst_height {
                let sy = ((i as u64 * yratio) >> 16) as i32 + srcy;
                let dy = i as i32 + dsty;
                if sy < 0 || dy < 0 {
                    continue;
                }
                if sy as usize >= src.height || dy as usize >= self.height {
                    break;
                }
                for j in 0..dst_width {
                    let sx = ((j as u64 * xratio) >> 16) as i32 + srcx;
                    let dx = j as i32 + dstx;
                    if sx < 0 || dx < 0 {
                        continue;
                    }
                    if sx as usize >= src.width || dx as usize >= self.width {
                        break;
                    }
                    let value = src.get_raw(sx as usize, sy as usize);
                    let out = if blending {
                        blend(value, self.get_raw(dx as usize, dy as usize))
                    } else {
                        value
                    };
                    self.set_raw(dx as usize, dy as usize, out);
                }
            }
        }
    }

    /// Copy with an outline drawn around non-empty pixels (`Pixmap.outline`).
    pub fn outline(&self, color: u32, radius: i32) -> Pixmap {
        let mut out = self.copy();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get_a(x, y) == 0 {
                    let mut found = false;
                    'outer: for dx in -radius..=radius {
                        for dy in -radius..=radius {
                            if dx * dx + dy * dy <= radius * radius
                                && !empty(self.get(x as i32 + dx, y as i32 + dy))
                            {
                                found = true;
                                break 'outer;
                            }
                        }
                    }
                    if found {
                        out.set_raw(x, y, color);
                    }
                }
            }
        }
        out
    }

    /// Centers `other` on a copy of `self` (`ImagePacker.drawCenter` helper).
    pub fn draw_center(&mut self, other: &Pixmap) {
        self.draw_blended(
            other,
            self.width as i32 / 2 - other.width as i32 / 2,
            self.height as i32 / 2 - other.height as i32 / 2,
        );
    }
}

/// Outlines a pixmap with `padding` transparent pixels around it
/// (`Pixmaps.outline(region, color, radius, padding)`).
pub fn outline_padded(src: &Pixmap, color: u32, radius: i32, padding: usize) -> Pixmap {
    let mut out = Pixmap::new(src.width + padding * 2, src.height + padding * 2);
    out.draw(src, padding as i32, padding as i32);
    for y in 0..src.height {
        for x in 0..src.width {
            if src.get_a(x, y) < 255 {
                let mut found = false;
                'outer: for rx in -radius..=radius {
                    for ry in -radius..=radius {
                        let px = x as i32 + rx;
                        let py = y as i32 + ry;
                        if px >= 0
                            && py >= 0
                            && (px as usize) < src.width
                            && (py as usize) < src.height
                            && rx * rx + ry * ry <= radius * radius
                            && src.get_a(px as usize, py as usize) != 0
                        {
                            found = true;
                            break 'outer;
                        }
                    }
                }
                if found {
                    out.set_raw(x + padding, y + padding, color);
                }
            }
        }
    }
    out
}

/// Median filter over a circle of `radius`, picking the `percentile`-th sorted
/// value (`Pixmaps.median`).
pub fn median(input: &Pixmap, radius: i32, percentile: f64) -> Pixmap {
    let mut out = Pixmap::new(input.width, input.height);
    let mut tmp: Vec<u32> = Vec::new();
    for y in 0..input.height {
        for x in 0..input.width {
            tmp.clear();
            // Geometry.circle(x, y, width, height, radius, cons)
            let x0 = (x as i32 - radius).max(0);
            let x1 = (x as i32 + radius).min(input.width as i32 - 1);
            let y0 = (y as i32 - radius).max(0);
            let y1 = (y as i32 + radius).min(input.height as i32 - 1);
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    // Mathf.within(dx, dy, x, y, radius)
                    let ddx = cx - x as i32;
                    let ddy = cy - y as i32;
                    if ddx * ddx + ddy * ddy <= radius * radius {
                        tmp.push(input.get_raw(cx as usize, cy as usize));
                    }
                }
            }
            tmp.sort_unstable();
            let pick =
                ((tmp.len() as f64 * percentile) as usize).clamp(0, tmp.len().saturating_sub(1));
            out.set_raw(x, y, tmp[pick]);
        }
    }
    out
}

/// `Pixmaps.antialias` (the `tools:pack` AA pass).
pub fn antialias(pixmap: &mut Pixmap) {
    let prev = pixmap.copy();
    for y in 0..prev.height {
        for x in 0..prev.width {
            let (x, y) = (x as i32, y as i32);
            let a = prev.get_clamp(x - 1, y + 1);
            let b = prev.get_clamp(x, y + 1);
            let c = prev.get_clamp(x + 1, y + 1);
            let d = prev.get_clamp(x - 1, y);
            let e = prev.get_clamp(x, y);
            let f = prev.get_clamp(x + 1, y);
            let g = prev.get_clamp(x - 1, y - 1);
            let h = prev.get_clamp(x, y - 1);
            let i = prev.get_clamp(x + 1, y - 1);

            let mut p = [e; 9];
            if d == b && d != h && b != f {
                p[0] = d;
            }
            if (d == b && d != h && b != f && e != c) || (b == f && b != d && f != h && e != a) {
                p[1] = b;
            }
            if b == f && b != d && f != h {
                p[2] = f;
            }
            if (h == d && h != f && d != b && e != a) || (d == b && d != h && b != f && e != g) {
                p[3] = d;
            }
            if (b == f && b != d && f != h && e != i) || (f == h && f != b && h != d && e != c) {
                p[5] = f;
            }
            if h == d && h != f && d != b {
                p[6] = d;
            }
            if (f == h && f != b && h != d && e != g) || (h == d && h != f && d != b && e != i) {
                p[7] = h;
            }
            if f == h && f != b && h != d {
                p[8] = f;
            }

            let mut sumr = 0f32;
            let mut sumg = 0f32;
            let mut sumb = 0f32;
            let mut suma = 0f32;
            for val in p {
                let r = ri(val) as f32 / 255.0;
                let g = gi(val) as f32 / 255.0;
                let b = bi(val) as f32 / 255.0;
                let a = ai(val) as f32 / 255.0;
                sumr += r * a;
                sumg += g * a;
                sumb += b * a;
                suma += a;
            }
            let fm = if suma <= 0.001 { 0.0 } else { 1.0 / suma };
            sumr *= fm;
            sumg *= fm;
            sumb *= fm;

            let mut total = 0f32;
            let mut tr = 0f32;
            let mut tg = 0f32;
            let mut tb = 0f32;
            let mut ta = 0f32;
            for val in p {
                let mut r = ri(val) as f32 / 255.0;
                let mut g = gi(val) as f32 / 255.0;
                let mut b = bi(val) as f32 / 255.0;
                let a = ai(val) as f32 / 255.0;
                let t = 1.0 - a;
                r += t * (sumr - r);
                g += t * (sumg - g);
                b += t * (sumb - b);
                tr += r;
                tg += g;
                tb += b;
                ta += a;
                total += 1.0;
            }
            let fm = 1.0 / total;
            pixmap.set_raw(
                x as usize,
                y as usize,
                rgba8888f(tr * fm, tg * fm, tb * fm, ta * fm),
            );
        }
    }
}

const BLEED_OFFSETS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Single-pass alpha bleed (`Pixmaps.bleed(image)`).
pub fn bleed_once(image: &mut Pixmap) {
    let (w, h) = (image.width, image.height);
    let src = image.clone();
    for y in 0..h {
        for x in 0..w {
            if src.empty_at(x, y) {
                let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
                for (ox, oy) in BLEED_OFFSETS {
                    let nx = x as i32 + ox;
                    let ny = y as i32 + oy;
                    if nx >= 0
                        && ny >= 0
                        && (nx as usize) < w
                        && (ny as usize) < h
                        && !src.empty_at(nx as usize, ny as usize)
                    {
                        let value = src.get_raw(nx as usize, ny as usize);
                        r += ri(value);
                        g += gi(value);
                        b += bi(value);
                        count += 1;
                    }
                }
                if let Some(count) = std::num::NonZeroU32::new(count) {
                    let i = src.index(x, y);
                    image.pixels[i] = (r / count) as u8;
                    image.pixels[i + 1] = (g / count) as u8;
                    image.pixels[i + 2] = (b / count) as u8;
                }
            }
        }
    }
}

/// Iterated alpha bleed (`Pixmaps.bleed(image, maxIterations)`).
pub fn bleed(image: &mut Pixmap, max_iterations: usize) {
    let (w, h) = (image.width, image.height);
    let total = w * h;
    let mut data = vec![false; total];
    let mut pending: Vec<usize> = Vec::with_capacity(total);
    for (i, chunk) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
        if chunk[3] == 0 {
            pending.push(i);
        } else {
            data[i] = true;
        }
    }

    let mut iterations = 0;
    let mut last_pending: Option<usize> = None;
    while !pending.is_empty() && last_pending != Some(pending.len()) && iterations < max_iterations
    {
        last_pending = Some(pending.len());
        let mut index = 0;
        let mut changed: Vec<usize> = Vec::new();
        while index < pending.len() {
            let pixel_index = pending[index];
            let x = pixel_index % w;
            let y = pixel_index / w;
            let (mut r, mut g, mut b, mut count) = (0u32, 0u32, 0u32, 0u32);
            for (ox, oy) in BLEED_OFFSETS {
                let nx = x as i32 + ox;
                let ny = y as i32 + oy;
                if nx < 0 || nx as usize >= w || ny < 0 || ny as usize >= h {
                    continue;
                }
                let current = ny as usize * w + nx as usize;
                if data[current] {
                    let si = current * 4;
                    r += image.pixels[si] as u32;
                    g += image.pixels[si + 1] as u32;
                    b += image.pixels[si + 2] as u32;
                    count += 1;
                }
            }
            if let Some(count) = std::num::NonZeroU32::new(count) {
                let idx = pixel_index * 4;
                image.pixels[idx] = (r / count) as u8;
                image.pixels[idx + 1] = (g / count) as u8;
                image.pixels[idx + 2] = (b / count) as u8;
                changed.push(pending.swap_remove(index));
            } else {
                index += 1;
            }
        }
        for pixel in changed {
            data[pixel] = true;
        }
        iterations += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: usize, h: usize) -> Pixmap {
        let mut pix = Pixmap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                if (x + y) % 2 == 0 {
                    pix.set_raw(x, y, rgba8888(255, 0, 0, 255));
                }
            }
        }
        pix
    }

    #[test]
    fn pixel_packing_matches_arc() {
        assert_eq!(rgba8888(0xff, 0, 0, 0xff), 0xff00_00ff);
        assert_eq!(ri(0x1234_5678), 0x12);
        assert_eq!(gi(0x1234_5678), 0x34);
        assert_eq!(bi(0x1234_5678), 0x56);
        assert_eq!(ai(0x1234_5678), 0x78);
    }

    #[test]
    fn blend_matches_arc_rules() {
        // src alpha 0 keeps dst; dst alpha 0 keeps src.
        assert_eq!(blend(0, 0xff0000ff), 0xff0000ff);
        assert_eq!(blend(0xff0000ff, 0), 0xff0000ff);
        // Full-over-full keeps src.
        assert_eq!(blend(0x00ff00ff, 0xff0000ff), 0x00ff00ff);
    }

    #[test]
    fn outline_draws_around_content() {
        let mut pix = Pixmap::new(5, 5);
        pix.set_raw(2, 2, WHITE);
        let out = pix.outline(0x0000_00ff, 1);
        assert_eq!(out.get_raw(2, 2), WHITE);
        // Radius-1 disc covers the 4 orthogonal neighbors only.
        assert_eq!(out.get_raw(2, 1), 0x0000_00ff);
        assert_eq!(out.get_raw(1, 2), 0x0000_00ff);
        // Diagonals are outside the radius-1 disc.
        assert_eq!(out.get_raw(1, 1), 0);
        assert_eq!(out.get_raw(0, 0), 0);
    }

    #[test]
    fn crop_and_flip() {
        let pix = checker(4, 4);
        let cropped = pix.crop(1, 1, 2, 2);
        assert_eq!(cropped.width, 2);
        assert_eq!(cropped.get_raw(0, 0), pix.get_raw(1, 1));
        let flipped = pix.flip_x();
        assert_eq!(flipped.get_raw(0, 0), pix.get_raw(3, 0));
    }

    #[test]
    fn draw_blended_overlaps() {
        let mut base = Pixmap::new(4, 4);
        base.fill(0xff0000ff);
        let mut over = Pixmap::new(2, 2);
        over.fill(0x00ff0080);
        base.draw_blended(&over, 1, 1);
        // Center blended, corner untouched.
        assert_eq!(base.get_raw(0, 0), 0xff0000ff);
        assert_ne!(base.get_raw(1, 1), 0xff0000ff);
    }

    #[test]
    fn antialias_is_deterministic() {
        let mut a = checker(8, 8);
        let mut b = a.copy();
        antialias(&mut a);
        antialias(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn bleed_fills_transparent_rgb() {
        let mut pix = Pixmap::new(3, 3);
        pix.set_raw(0, 0, rgba8888(200, 100, 50, 255));
        bleed(&mut pix, 8);
        // Corner color must have bled into neighbors; alpha stays 0.
        assert_eq!(ai(pix.get_raw(1, 1)), 0);
        assert_eq!(ri(pix.get_raw(1, 1)), 200);
        assert_eq!(gi(pix.get_raw(1, 1)), 100);
        assert_eq!(bi(pix.get_raw(1, 1)), 50);
    }

    #[test]
    fn median_picks_middle_value() {
        let mut pix = Pixmap::new(3, 3);
        pix.fill(rgba8888(10, 10, 10, 255));
        pix.set_raw(1, 1, rgba8888(250, 250, 250, 255));
        let out = median(&pix, 1, 0.5);
        assert_eq!(out.get_raw(0, 0), rgba8888(10, 10, 10, 255));
    }
}
