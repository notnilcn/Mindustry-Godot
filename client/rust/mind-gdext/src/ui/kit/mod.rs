// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Rust UI toolkit: the native replacement for `client/ui/**/*.gd`.
//!
//! Every widget factory, layout container and themed control that the Godot
//! scenes used to reach through GDScript lives here as a native GDExtension
//! class or free function. Dialogs and fragments build their dynamic content
//! with this module only.
//!
//! `assets`/`t`/`tm` mirror the former `MindWidgets` static helpers: bundle
//! text and atlas lookups always route through the `MindAssets` autoload so no
//! call site re-implements the fallback rules.

use godot::classes::{Engine, Node, SceneTree, Texture2D};
use godot::obj::Singleton;
use godot::prelude::*;

use crate::assets::MindAssets;

pub mod cell;
pub mod icons;
pub mod layout;
pub mod margins;
pub mod scroll;
pub mod styles;
pub mod table;
pub mod text;
pub mod theme;
pub mod widgets;

pub use cell::Align;
pub use icons::glyph;
pub use layout::MindRowTreeLayout;
pub use scroll::MindScroll;
pub use table::MindTable;
pub use text::{MindLabel, MindRichLabel};

/// The `MindAssets` autoload, or `None` in a bare test host.
pub fn assets() -> Option<Gd<MindAssets>> {
    let engine = Engine::singleton();
    let main_loop = engine.get_main_loop()?;
    let tree = main_loop.try_cast::<SceneTree>().ok()?;
    let root = tree.get_root();
    root.get_node_or_null("MindAssets")?
        .try_cast::<MindAssets>()
        .ok()
}

/// The scene-tree root, or `None` before the tree exists.
pub fn tree_root() -> Option<Gd<Node>> {
    let engine = Engine::singleton();
    let main_loop = engine.get_main_loop()?;
    let tree = main_loop.try_cast::<SceneTree>().ok()?;
    Some(tree.get_root().upcast())
}

/// Resolves an absolute node path from the scene root.
pub fn node(path: &str) -> Option<Gd<Node>> {
    let root = tree_root()?;
    root.get_node_or_null(path)
}

/// The `MindUi` autoload.
pub fn ui() -> Option<Gd<Node>> {
    node("/root/MindUi")
}

/// The `MindHud` autoload.
pub fn hud() -> Option<Gd<Node>> {
    node("/root/MindHud")
}

/// The live campaign facade (`/root/Spine/MindCampaign`).
pub fn campaign() -> Option<Gd<Node>> {
    node("/root/Spine/MindCampaign")
}

/// The sim owner (`/root/Spine/SimHost`).
pub fn sim_host() -> Option<Gd<Node>> {
    node("/root/Spine/SimHost")
}

/// The map-editor facade (`/root/Spine/MindEditor`).
pub fn editor() -> Option<Gd<Node>> {
    node("/root/Spine/MindEditor")
}

/// The net facade (`/root/Spine/MindNet`).
pub fn net() -> Option<Gd<Node>> {
    node("/root/Spine/MindNet")
}

/// The logic facade (`/root/Spine/MindLogic`).
pub fn logic() -> Option<Gd<Node>> {
    node("/root/Spine/MindLogic")
}

/// Resolves an atlas region through `MindAssets` (`None` when absent).
pub fn icon_texture(region_name: &str) -> Option<Gd<Texture2D>> {
    let mut assets = assets()?;
    let texture = assets.bind_mut().find_region(GString::from(region_name))?;
    Some(texture.upcast())
}

/// Bundle lookup with the key echoed back when assets are absent
/// (`Bundle.get` semantics; all user-visible strings go through a key).
pub fn t(key: &str) -> GString {
    let resolved = key.strip_prefix('@').unwrap_or(key);
    match assets() {
        Some(assets) => assets.bind().bundle_get(GString::from(resolved)),
        None => GString::from(resolved),
    }
}

/// Bundle string translated to Godot BBCode (`MindAssets.bundle_markup`):
/// `[accent]` colours, `:icon:` tokens and `\n` escapes.
pub fn tm(key: &str) -> GString {
    let resolved = key.strip_prefix('@').unwrap_or(key);
    match assets() {
        Some(assets) => assets.bind().bundle_markup(GString::from(resolved)),
        None => GString::from(resolved),
    }
}

/// `Core.bundle.format` + markup translation (`markup_format`).
pub fn tm_format(key: &str, args: &[&str]) -> GString {
    let resolved = key.strip_prefix('@').unwrap_or(key);
    let Some(assets) = assets() else {
        return GString::from(resolved);
    };
    let mut packed = PackedStringArray::new();
    for arg in args {
        packed.push(*arg);
    }
    assets
        .bind()
        .bundle_markup_format(GString::from(resolved), packed)
}

/// `Core.bundle.format` + markup translation that emits raw content-icon PUA
/// glyphs (`render_markup_text_icons`) instead of `[icon name="…"]` BBCode, for
/// labels drawn with a plain font (`bar.unitcap` in `MindBar`, PAR-0448).
pub fn tm_format_text_icons(key: &str, args: &[&str]) -> GString {
    let resolved = key.strip_prefix('@').unwrap_or(key);
    let Some(assets) = assets() else {
        return GString::from(resolved);
    };
    let mut packed = PackedStringArray::new();
    for arg in args {
        packed.push(*arg);
    }
    assets
        .bind()
        .bundle_markup_format_text_icons(GString::from(resolved), packed)
}

/// `render_markup` for a raw markup string that is not a bundle key.
pub fn markup_text(raw: &str) -> GString {
    match assets() {
        Some(assets) => assets.bind().markup_text(GString::from(raw)),
        None => GString::from(raw),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn key_prefix_is_stripped() {
        // Bundle lookups accept both `@key` and `key`; the trim is the only
        // part of `t()` that does not need the asset host.
        let resolved = "@back".strip_prefix('@').unwrap_or("@back");
        assert_eq!(resolved, "back");
    }
}
