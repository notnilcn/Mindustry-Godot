// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! View-side image provider for the CPU display rasterizer (I03 T13).
//!
//! `LogicDisplay.commandImage` draws `UnlockableContent.fullIcon`
//! (`LogicDisplay.java:222-225`); the port crops the icon out of the packed
//! atlas page and hands it to `mind_core::logic::display_raster`.
//! `commandPrint`/`printchar` glyphs come from the `Fonts.logic` font
//! (`logic.ttf`) rasterised through Godot's `FontFile` at 16 px (L07 T4).

use std::cell::RefCell;
use std::collections::HashMap;

use godot::builtin::{Rect2i, Vector2i};
use godot::classes::Image;
use godot::classes::image::Format;
use godot::prelude::*;

use mind_core::content::{ContentRegistry, ContentType};
use mind_core::logic::display_raster::{DisplayDrawContext, Glyph, RgbaImage};

use crate::assets::MindAssets;

/// Per-content-type names (`[type ordinal][id]`), snapshotted from the live
/// registry; content ids are append-only, so the snapshot stays valid.
#[derive(Debug, Default)]
pub(crate) struct IconNames {
    names: Vec<Vec<String>>,
}

impl IconNames {
    /// Snapshots every content name from the registry.
    pub(crate) fn build(content: &ContentRegistry) -> Self {
        let mut names = vec![Vec::new(); ContentType::ALL.len()];
        for type_ in ContentType::ALL.iter().copied() {
            let list: Vec<String> = content
                .entries(type_)
                .into_iter()
                .map(|entry| entry.name.unwrap_or_default().to_owned())
                .collect();
            names[type_ as usize] = list;
        }
        Self { names }
    }

    /// Content name for `(type ordinal, id)`.
    pub(crate) fn get(&self, type_: u8, id: u16) -> Option<&str> {
        self.names
            .get(type_ as usize)
            .and_then(|list| list.get(id as usize))
            .map(String::as_str)
    }
}

/// Crops content icons out of the packed atlas (`UnlockableContent.fullIcon`).
pub(crate) struct AtlasIcons<'a> {
    assets: Option<Gd<MindAssets>>,
    names: &'a IconNames,
    cache: RefCell<HashMap<(u8, u16), Option<RgbaImage>>>,
    glyphs: RefCell<HashMap<char, Option<Glyph>>>,
}

impl<'a> AtlasIcons<'a> {
    /// Creates the provider over the `MindAssets` autoload.
    pub(crate) fn new(assets: Option<Gd<MindAssets>>, names: &'a IconNames) -> Self {
        Self {
            assets,
            names,
            cache: RefCell::new(HashMap::new()),
            glyphs: RefCell::new(HashMap::new()),
        }
    }

    /// Rasterises one `Fonts.logic` glyph at 16 px (`LogicDisplay.java:227-235`).
    ///
    /// `'\0'` is the font's `.notdef` box (`fonts.logic`'s `missingGlyph`,
    /// `FreeTypeFontGenerator` installs it for every unmapped character):
    /// Godot's `FontFile` never exposes glyph index `0`, so it is synthesised
    /// by [`notdef_glyph`] instead. Blank glyphs (the synthesised space) have a
    /// positive glyph index but no atlas texture (`get_glyph_texture_idx` `-1`);
    /// querying `get_texture_image(-1)` logged a Godot error, so they are
    /// skipped exactly like Java's `getGlyph(c) == null` branch.
    fn logic_glyph(&self, c: char) -> Option<Glyph> {
        if c == '\0' {
            return Some(notdef_glyph());
        }
        let assets = self.assets.clone()?;
        let mut font = assets.bind().logic_font()?;
        let size = Vector2i::new(16, 0);
        let index = font.get_glyph_index(16, c as u32, 0);
        if index <= 0 {
            return None;
        }
        font.render_glyph(0, size, index);
        let texture_index = font.get_glyph_texture_idx(0, size, index);
        if texture_index < 0 {
            // No atlas page for this glyph (space and other blank glyphs).
            return None;
        }
        let image = font.get_texture_image(0, size, texture_index)?;
        let uv = font.get_glyph_uv_rect(0, size, index);
        let (image_width, image_height) = (image.get_width() as f32, image.get_height() as f32);
        if image_width <= 0.0 || image_height <= 0.0 {
            return None;
        }
        let rect = if uv.size.x <= 1.0 && uv.size.y <= 1.0 && image_width > 1.0 {
            Rect2i::new(
                Vector2i::new(
                    (uv.position.x * image_width).floor() as i32,
                    (uv.position.y * image_height).floor() as i32,
                ),
                Vector2i::new(
                    (uv.size.x * image_width).round().max(1.0) as i32,
                    (uv.size.y * image_height).round().max(1.0) as i32,
                ),
            )
        } else {
            Rect2i::new(
                Vector2i::new(uv.position.x as i32, uv.position.y as i32),
                Vector2i::new(uv.size.x as i32, uv.size.y as i32),
            )
        };
        let glyph_image = image.get_region(rect)?;
        let image = rgba_from_image(&glyph_image)?;
        let offset = font.get_glyph_offset(0, size, index);
        let ascent = font.get_ascent();
        // `BitmapFontData.capHeight`: the capital glyph's pixel height.
        let cap_index = font.get_glyph_index(16, 'H' as u32, 0);
        let cap_height = if cap_index > 0 {
            font.get_glyph_size(0, size, cap_index).y
        } else {
            ascent * 0.75
        };
        Some(Glyph {
            image,
            x_offset: offset.x,
            y_offset: offset.y,
            cap_height,
            ascent,
        })
    }

    /// `[x, y, w, h, page]` crop of one atlas region as RGBA8888.
    fn region_pixels(&self, name: &str) -> Option<RgbaImage> {
        let assets = self.assets.clone()?;
        let geometry = assets.bind().region_geometry(GString::from(name));
        if geometry.len() < 5 {
            return None;
        }
        let (x, y, w, h, page) = (
            geometry[0],
            geometry[1],
            geometry[2],
            geometry[3],
            geometry[4],
        );
        if w <= 0 || h <= 0 {
            return None;
        }
        let texture = assets.bind().page_texture(i64::from(page))?;
        let image = texture.get_image()?;
        let region = image.get_region(Rect2i::new(Vector2i::new(x, y), Vector2i::new(w, h)))?;
        rgba_from_image(&region)
    }
}

impl DisplayDrawContext for AtlasIcons<'_> {
    fn icon(&self, content_type: u8, id: u16) -> Option<RgbaImage> {
        let key = (content_type, id);
        if let Some(cached) = self.cache.borrow().get(&key) {
            return cached.clone();
        }
        let name = self.names.get(content_type, id).unwrap_or_default();
        let type_name = ContentType::ALL
            .iter()
            .find(|type_| **type_ as u8 == content_type)
            .map(|type_| type_.name())
            .unwrap_or("");
        let mut found = None;
        if !name.is_empty() {
            // `UnlockableContent.loadIcon()` fallback chain.
            for candidate in [
                format!("{type_name}-{name}-full"),
                format!("{name}-full"),
                name.to_owned(),
                format!("{type_name}-{name}"),
                format!("{name}1"),
            ] {
                if let Some(image) = self.region_pixels(&candidate) {
                    found = Some(image);
                    break;
                }
            }
        }
        self.cache.borrow_mut().insert(key, found.clone());
        found
    }

    fn glyph(&self, c: char) -> Option<Glyph> {
        if let Some(cached) = self.glyphs.borrow().get(&c) {
            return cached.clone();
        }
        let glyph = self.logic_glyph(c);
        self.glyphs.borrow_mut().insert(c, glyph.clone());
        glyph
    }
}

/// The `.notdef` box for characters the logic font cannot draw
/// (`FontData.missingGlyph`; `'\0'` in `mind_core::logic::font::GLYPH_CHARS`).
///
/// A 5x7 hollow rectangle at cap height, sitting on the baseline (y-down
/// `y_offset` `-height`). Godot's `FontFile` does not expose glyph index `0`
/// (`get_glyph_index` returns `0` for "not found"), so drawing the real
/// `.notdef` would need `render_glyph(0, ..)`; synthesising the box is exact
/// enough for the 16 px display font and cannot log a Godot error.
fn notdef_glyph() -> Glyph {
    const WIDTH: u32 = 5;
    const HEIGHT: u32 = 7;
    let mut pixels = vec![0u32; (WIDTH * HEIGHT) as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1 {
                pixels[(y * WIDTH + x) as usize] = 0xffff_ffff;
            }
        }
    }
    Glyph {
        image: RgbaImage {
            width: WIDTH,
            height: HEIGHT,
            pixels,
        },
        x_offset: 1.0,
        y_offset: -(HEIGHT as f32),
        cap_height: HEIGHT as f32,
        ascent: 3.0,
    }
}

/// An `Image` as RGBA8888 pixels in `0xRRGGBBAA` order (row 0 = top).
fn rgba_from_image(image: &Gd<Image>) -> Option<RgbaImage> {
    let mut image = image.clone();
    if image.get_format() != Format::RGBA8 {
        image.convert(Format::RGBA8);
    }
    let width = image.get_width().max(0) as u32;
    let height = image.get_height().max(0) as u32;
    let bytes = image.get_data().to_vec();
    if bytes.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let pixels = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .take((width as usize) * (height as usize))
        .map(|chunk| u32::from_be_bytes(*chunk))
        .collect();
    Some(RgbaImage {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::notdef_glyph;

    /// The synthesised `.notdef` box keeps the logic-font cap-height
    /// proportions and places like a baseline glyph (`Glyph::print_center`).
    #[test]
    fn notdef_box_is_a_hollow_baseline_box() {
        let glyph = notdef_glyph();
        assert_eq!(glyph.image.width, 5);
        assert_eq!(glyph.image.height, 7);
        // Border pixels are opaque, the interior is clear.
        assert_eq!(glyph.image.pixels[0], 0xffff_ffff);
        assert_eq!(glyph.image.pixels[5 + 2], 0);
        assert_eq!(glyph.image.pixels[4 * 5 + 2], 0, "bottom border is row 6");
        assert_eq!(glyph.image.pixels[6 * 5 + 2], 0xffff_ffff);
        // `print_center`: (x + w/2 + xoffset, y - yoffset - h/2).
        let (cx, cy) = glyph.print_center(10.0, 20.0);
        assert!((cx - 13.5).abs() < 1e-6, "cx {cx}");
        assert!((cy - 23.5).abs() < 1e-6, "cy {cy}");
    }
}
