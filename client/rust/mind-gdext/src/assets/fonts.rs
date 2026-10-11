// SPDX-License-Identifier: GPL-3.0-only

//! Dynamic font loading (plan 03 §3.6/M7).
//!
//! Loads `FontFile`s for `def`/`defJp`/`monospace`/`icon`/`logic`/`tech`.
//! Upstream ships `font.woff`/`font_jp.woff`/`monospace.woff`; Godot 4 loads
//! WOFF natively, so the A7 pack-time WOFF→TTF conversion is deferred (documented
//! in the plan changelog) and `.woff` is loaded directly. Text layout/`RichText`
//! integration is plan 14; this module only produces the resources + glyph API.

use godot::builtin::{Array, Rect2, Vector2, Vector2i};
use godot::classes::text_server::FixedSizeScaleMode;
use godot::classes::image::Format;
use godot::classes::{FileAccess, Font, FontFile, Image, ImageTexture};
use godot::global::Error;
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::assets::atlas::{AtlasIndex, PageType};
use mind_core::assets::icons::Iconc;

/// Bitmap cell size of the content-icon font; labels scale it to their size.
const CONTENT_ICON_FONT_SIZE: i32 = 32;

/// Loaded font set (`Fonts.def`/`outline`/`icon`/… equivalents).
#[derive(Default)]
pub struct FontSet {
    /// Default UI font (`font.woff`).
    pub def: Option<Gd<FontFile>>,
    /// Japanese fallback (`font_jp.woff`).
    pub def_jp: Option<Gd<FontFile>>,
    /// Monospace font (`monospace.woff`).
    pub monospace: Option<Gd<FontFile>>,
    /// Content icon font (`icon.ttf`).
    pub icon: Option<Gd<FontFile>>,
    /// Logic icon font (`logic.ttf`).
    pub logic: Option<Gd<FontFile>>,
    /// Tech icon font (`tech.ttf`).
    pub tech: Option<Gd<FontFile>>,
    /// Content icons (`Fonts.loadContentIcons`): a bitmap font whose glyphs
    /// are the `icons.properties` UI-page regions (`item.emoji()` etc.).
    pub content_icons: Option<Gd<FontFile>>,
}

impl FontSet {
    /// Loads every font present under `<assets_dir>/fonts/` (missing → `None`).
    pub fn load(assets_dir: &str) -> Self {
        Self {
            def: load_font(assets_dir, "font.woff"),
            def_jp: load_font(assets_dir, "font_jp.woff"),
            monospace: load_font(assets_dir, "monospace.woff"),
            icon: load_font(assets_dir, "icon.ttf"),
            logic: load_font(assets_dir, "logic.ttf"),
            tech: load_font(assets_dir, "tech.ttf"),
            content_icons: None,
        }
    }

    /// `Fonts.loadContentIcons` + `registerIcon`: builds the content-icon
    /// bitmap font from the UI atlas page and chains it (then `icon.ttf`) as
    /// fallbacks of the default font, so every label using the theme font
    /// draws `item.emoji()` glyphs inline like upstream.
    pub fn register_content_icons(
        &mut self,
        iconc: &Iconc,
        index: &AtlasIndex,
        pages: &[Gd<ImageTexture>],
    ) {
        let Some(def) = self.def.clone() else {
            return;
        };
        let Some(ui_page) = index.pages().iter().position(|p| p.type_ == PageType::Ui) else {
            return;
        };
        let Some(mut image) = pages.get(ui_page).and_then(|tex| tex.get_image()) else {
            return;
        };
        if image.get_format() != Format::RGBA8 {
            image.convert(Format::RGBA8);
        }
        let font = build_content_icon_font(&def, iconc, index, ui_page, image);
        let mut fallbacks: Array<Gd<Font>> = Array::new();
        fallbacks.push(&font.clone().upcast::<Font>());
        if let Some(icon) = &self.icon {
            fallbacks.push(&icon.clone().upcast::<Font>());
        }
        let mut def = def;
        def.set_fallbacks(&fallbacks);
        self.content_icons = Some(font);
    }

    /// Number of fonts that actually loaded.
    pub fn loaded(&self) -> usize {
        [
            &self.def,
            &self.def_jp,
            &self.monospace,
            &self.icon,
            &self.logic,
            &self.tech,
        ]
        .into_iter()
        .filter(|font| font.is_some())
        .count()
    }

    /// Default UI font, if loaded.
    pub fn default_font(&self) -> Option<Gd<FontFile>> {
        self.def.clone()
    }
}

/// Loads one dynamic font; `None` when absent or Godot rejects the file.
fn load_font(assets_dir: &str, file: &str) -> Option<Gd<FontFile>> {
    let path = format!("{assets_dir}/fonts/{file}");
    if !FileAccess::file_exists(&path) {
        return None;
    }
    let mut font = FontFile::new_gd();
    if font.load_dynamic_font(&path) != Error::OK {
        log::warn!("[assets] font load failed: {path}");
        return None;
    }
    Some(font)
}

/// Builds the bitmap glyph table: each icon is fit into a square of the
/// default font's line height (`Scaling.fit` to `lineHeight`), centred, with
/// `xadvance = size` (`Fonts.registerIcon`).
fn build_content_icon_font(
    def: &Gd<FontFile>,
    iconc: &Iconc,
    index: &AtlasIndex,
    ui_page: usize,
    image: Gd<Image>,
) -> Gd<FontFile> {
    let s = CONTENT_ICON_FONT_SIZE;
    let ascent = def.get_ascent_ex().font_size(s).done() as f32;
    let descent = def.get_descent_ex().font_size(s).done() as f32;
    let line = (ascent + descent).max(1.0);
    let size = Vector2i::new(s, 0);
    let mut font = FontFile::new_gd();
    font.set_font_name("mindustry-content-icons");
    font.set_fixed_size(s);
    font.set_fixed_size_scale_mode(FixedSizeScaleMode::ENABLED);
    font.set_cache_ascent(0, s, ascent);
    font.set_cache_descent(0, s, descent);
    font.set_texture_image(0, size, 0, &image);
    for (code, texture) in iconc.entries() {
        let Some(region) = index.find(texture) else {
            continue;
        };
        if region.page != ui_page || region.w <= 0 || region.h <= 0 {
            continue;
        }
        let glyph = code as i32;
        // `Scaling.fit.apply(region.width, region.height, size, size)`.
        let scale = (line / region.w as f32).min(line / region.h as f32);
        let (w, h) = (
            (region.w as f32 * scale).floor(),
            (region.h as f32 * scale).floor(),
        );
        font.set_glyph_advance(0, s, glyph, Vector2::new(line, 0.0));
        font.set_glyph_offset(
            0,
            size,
            glyph,
            Vector2::new(((line - w) / 2.0).floor(), -ascent + ((line - h) / 2.0).floor()),
        );
        font.set_glyph_size(0, size, glyph, Vector2::new(w, h));
        font.set_glyph_uv_rect(
            0,
            size,
            glyph,
            Rect2::new(
                Vector2::new(region.x as f32, region.y as f32),
                Vector2::new(region.w as f32, region.h as f32),
            ),
        );
        font.set_glyph_texture_idx(0, size, glyph, 0);
    }
    font
}
