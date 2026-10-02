// SPDX-License-Identifier: GPL-3.0-only

//! Dynamic font loading (plan 03 §3.6/M7).
//!
//! Loads `FontFile`s for `def`/`defJp`/`monospace`/`icon`/`logic`/`tech`.
//! Upstream ships `font.woff`/`font_jp.woff`/`monospace.woff`; Godot 4 loads
//! WOFF natively, so the A7 pack-time WOFF→TTF conversion is deferred (documented
//! in the plan changelog) and `.woff` is loaded directly. Text layout/`RichText`
//! integration is plan 14; this module only produces the resources + glyph API.

use godot::classes::{FileAccess, FontFile};
use godot::global::Error;
use godot::obj::NewGd;
use godot::prelude::*;

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
        }
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
