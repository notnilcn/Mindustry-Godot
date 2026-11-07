// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Styles.java (field inventory),
//         mindustry.graphics.Pal (accent/stat/negstat),
//         core/src/mindustry/ui/builder/UiStyleLookup.java.

//! `MindStyles` colour constants and the MSUI style lookup.
//!
//! Style *names* are the parity ABI and must never change (plan 14 §3.10 rule
//! 5); only these presentation values are local.

use godot::prelude::*;
use mind_core::ui::manifest::StylesManifest;

pub const ACCENT: Color = Color::from_rgba(1.0, 0.827, 0.498, 1.0);
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub const UNLAUNCHED: Color = Color::from_rgba(0.537, 0.510, 0.929, 1.0);
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub const HIGHLIGHT: Color = Color::from_rgba(1.0, 0.878, 0.651, 1.0);
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub const STAT: Color = Color::from_rgba(1.0, 0.827, 0.498, 1.0);
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub const NEGSTAT: Color = Color::from_rgba(0.898, 0.329, 0.329, 1.0);
pub const REMOVE: Color = Color::from_rgba(0.898, 0.329, 0.329, 1.0);

pub const BLACK: Color = Color::from_rgba(0.0, 0.0, 0.0, 1.0);
pub const BLACK_3: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.3);
pub const BLACK_5: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.5);
pub const BLACK_6: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.6);
pub const BLACK_8: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.8);
pub const BLACK_9: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.9);
pub const GRAY_PANEL: Color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
pub const GRAY_PANEL_DARK: Color = Color::from_rgba(0.05, 0.05, 0.05, 1.0);
/// Arc `Color.gray` (0x7f7f7fff).
pub const ARC_GRAY: Color = Color::from_rgba8(0x7f, 0x7f, 0x7f, 0xff);
/// Arc `Color.lightGray` (0xbfbfbfff).
pub const ARC_LIGHT_GRAY: Color = Color::from_rgba8(0xbf, 0xbf, 0xbf, 0xff);
/// Arc `Color.darkGray` (0x3f3f3fff).
pub const ARC_DARK_GRAY: Color = Color::from_rgba8(0x3f, 0x3f, 0x3f, 0xff);
/// `Tex.pane` border in the reference frames (#454545).
pub const PANE_OUTLINE_GRAY: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `flatOver` (#454545) — the Arc hover/pressed background tint.
pub const FLAT_OVER: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);

/// Manifest group name -> Godot base class for type variations.
///
/// Declaration order matches `StylesManifest::groups()`.
pub const GROUP_BASE_TYPES: [(&str, &str); 11] = [
    ("drawables", "PanelContainer"),
    ("text_buttons", "Button"),
    ("buttons", "Button"),
    ("image_buttons", "Button"),
    ("panes", "PanelContainer"),
    ("sliders", "HSlider"),
    ("labels", "RichTextLabel"),
    ("fields", "LineEdit"),
    ("checks", "CheckBox"),
    ("dialogs", "PanelContainer"),
    ("trees", "Tree"),
];

/// The manifest group key for a Godot base type (inverse of [`GROUP_BASE_TYPES`]).
pub fn kind_for_base_type(base_type: &str) -> &'static str {
    match base_type {
        "Button" => "text_buttons",
        "PanelContainer" | "Panel" => "panes",
        "HSlider" => "sliders",
        "RichTextLabel" | "Label" => "labels",
        "LineEdit" => "fields",
        "CheckBox" => "checks",
        "Tree" => "trees",
        _ => "drawables",
    }
}

/// Returns `name` when it belongs to `kind`, else `""` (unknown styles are a
/// no-op), matching `MindStyleLookup.get_style`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn get_style(manifest: &StylesManifest, kind: &str, name: &str) -> String {
    let groups = manifest.groups();
    let names = match kind {
        "drawables" => groups[0],
        "text_buttons" => groups[1],
        "buttons" => groups[2],
        "image_buttons" => groups[3],
        "panes" => groups[4],
        "sliders" => groups[5],
        "labels" => groups[6],
        "fields" => groups[7],
        "checks" => groups[8],
        "dialogs" => groups[9],
        "trees" => groups[10],
        _ => return String::new(),
    };
    if names.iter().any(|candidate| candidate == name) {
        name.to_string()
    } else {
        String::new()
    }
}

/// Drawable colour for a `Styles` name: the named translucent blacks keep
/// their Java alpha; every other drawable stays the pane colour.
pub fn drawable_color(name: &str) -> Color {
    match name {
        "black" => BLACK,
        "black3" => BLACK_3,
        "black5" => BLACK_5,
        "black6" => BLACK_6,
        "black8" => BLACK_8,
        "black9" => BLACK_9,
        _ => GRAY_PANEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> StylesManifest {
        let parsed = StylesManifest::from_json(
            r#"{"format":1,"drawables":["black","none"],"text_buttons":["defaultt"],
                "buttons":[],"image_buttons":[],"panes":[],"sliders":[],"labels":[],
                "fields":[],"checks":[],"dialogs":[],"trees":[]}"#,
        );
        let Ok(manifest) = parsed else {
            panic!("the inline styles manifest must parse");
        };
        manifest
    }

    #[test]
    fn style_lookup_is_group_scoped() {
        let manifest = manifest();
        assert_eq!(get_style(&manifest, "drawables", "black"), "black");
        // A button style is not a drawable.
        assert_eq!(get_style(&manifest, "drawables", "defaultt"), "");
        assert_eq!(get_style(&manifest, "text_buttons", "defaultt"), "defaultt");
        assert_eq!(get_style(&manifest, "nope", "defaultt"), "");
    }

    #[test]
    fn base_type_inverse_mapping() {
        assert_eq!(kind_for_base_type("Button"), "text_buttons");
        assert_eq!(kind_for_base_type("Panel"), "panes");
        assert_eq!(kind_for_base_type("RichTextLabel"), "labels");
        assert_eq!(kind_for_base_type("LineEdit"), "fields");
        assert_eq!(kind_for_base_type("Tree"), "trees");
    }

    #[test]
    fn group_order_matches_manifest() {
        assert_eq!(GROUP_BASE_TYPES.len(), 11);
        let manifest = manifest();
        let groups = manifest.groups();
        for (index, (group, _base)) in GROUP_BASE_TYPES.iter().enumerate() {
            // `kind_for_base_type` is the many-to-one inverse (four groups
            // share `Button`/`PanelContainer`), so deriving the group from its
            // base class cannot round-trip. Pin the declaration order against
            // the manifest's own group order instead; the inline fixture fills
            // only the first two groups.
            let expected = match index {
                0 => "drawables",
                1 => "text_buttons",
                2 => "buttons",
                3 => "image_buttons",
                4 => "panes",
                5 => "sliders",
                6 => "labels",
                7 => "fields",
                8 => "checks",
                9 => "dialogs",
                _ => "trees",
            };
            assert_eq!(*group, expected);
            assert_eq!(groups[index].is_empty(), index > 1);
        }
    }
}
