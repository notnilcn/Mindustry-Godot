// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! View-side image provider for the CPU display rasterizer (I03 T13).
//!
//! `LogicDisplay.commandImage` draws `UnlockableContent.fullIcon`
//! (`LogicDisplay.java:222-225`); the port crops the icon out of the packed
//! atlas page and hands it to `mind_core::logic::display_raster`. Logic-font
//! glyphs (`commandPrint`) are deferred: `glyph` always misses until a glyph
//! rasterizer lands.

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
}

impl<'a> AtlasIcons<'a> {
    /// Creates the provider over the `MindAssets` autoload.
    pub(crate) fn new(assets: Option<Gd<MindAssets>>, names: &'a IconNames) -> Self {
        Self {
            assets,
            names,
            cache: RefCell::new(HashMap::new()),
        }
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

    fn glyph(&self, _c: char) -> Option<Glyph> {
        // Deferred: the logic font rasterizer is not ported yet.
        None
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
