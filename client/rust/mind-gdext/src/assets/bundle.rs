// SPDX-License-Identifier: GPL-3.0-only

//! Bundle/icon file loading for [`crate::assets::MindAssets`] (plan 03 §3.6/M7).
//!
//! Reads the locale chain + `global.properties` + `locales` + `icons.properties`
//! through `FileAccess` and hands the text to the Godot-free parsers in
//! `mind_core::assets::{bundle,icons}`.

use godot::classes::FileAccess;

use mind_core::assets::bundle::{Bundle, locale_chain, parse_properties};
use mind_core::assets::icons::Iconc;

/// Loads `assets/bundles/*` for `locale` with `global.properties` merged.
pub fn load_bundle(assets_dir: &str, locale: &str) -> Bundle {
    let mut layers = Vec::new();
    for suffix in locale_chain(locale) {
        let path = format!("{assets_dir}/bundles/{suffix}.properties");
        if FileAccess::file_exists(&path) {
            let text = FileAccess::get_file_as_string(&path).to_string();
            layers.push(parse_properties(&text));
        }
    }
    let mut bundle = Bundle::from_layers(layers);
    let global = format!("{assets_dir}/bundles/global.properties");
    if FileAccess::file_exists(&global) {
        let text = FileAccess::get_file_as_string(&global).to_string();
        bundle.merge_global(parse_properties(&text));
    }
    bundle
}

/// Reads the generated `assets/locales` list.
pub fn load_locales(assets_dir: &str) -> Vec<String> {
    let path = format!("{assets_dir}/locales");
    if !FileAccess::file_exists(&path) {
        return Vec::new();
    }
    FileAccess::get_file_as_string(&path)
        .to_string()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Loads and parses `assets/icons/icons.properties`.
pub fn load_iconc(assets_dir: &str) -> Iconc {
    let path = format!("{assets_dir}/icons/icons.properties");
    if !FileAccess::file_exists(&path) {
        return Iconc::default();
    }
    Iconc::from_properties(&FileAccess::get_file_as_string(&path).to_string())
}
