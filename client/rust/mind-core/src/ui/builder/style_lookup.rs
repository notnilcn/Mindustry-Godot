// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiStyleLookup.java.

//! Style-name lookup against `styles_manifest.json` (plan 14 §3.6).
//!
//! Java `UiStyleLookup.get(Class<Style>, name)` returns a scene2d style object.
//! Here the manifest is the source of truth: a name resolves to a
//! [`StyleKind`], and the GDScript theme maps it to a Godot type variation.

use crate::ui::manifest::StylesManifest;

/// Kind of scene2d style a name belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    /// Ninepatch/drawable style.
    Drawable,
    /// Text-button style.
    TextButton,
    /// Button style.
    Button,
    /// Image-button style.
    ImageButton,
    /// Pane style.
    Pane,
    /// Slider style.
    Slider,
    /// Label style.
    Label,
    /// Field style.
    Field,
    /// Check style.
    Check,
    /// Dialog style.
    Dialog,
    /// Tree style.
    Tree,
}

impl StyleKind {
    /// All kinds in manifest order.
    pub const ALL: [StyleKind; 11] = [
        StyleKind::Drawable,
        StyleKind::TextButton,
        StyleKind::Button,
        StyleKind::ImageButton,
        StyleKind::Pane,
        StyleKind::Slider,
        StyleKind::Label,
        StyleKind::Field,
        StyleKind::Check,
        StyleKind::Dialog,
        StyleKind::Tree,
    ];

    /// Manifest group key (`drawables`, `text_buttons`, …).
    pub fn key(self) -> &'static str {
        match self {
            StyleKind::Drawable => "drawables",
            StyleKind::TextButton => "text_buttons",
            StyleKind::Button => "buttons",
            StyleKind::ImageButton => "image_buttons",
            StyleKind::Pane => "panes",
            StyleKind::Slider => "sliders",
            StyleKind::Label => "labels",
            StyleKind::Field => "fields",
            StyleKind::Check => "checks",
            StyleKind::Dialog => "dialogs",
            StyleKind::Tree => "trees",
        }
    }

    /// Names registered for this kind.
    pub fn names(self, manifest: &StylesManifest) -> &[String] {
        match self {
            StyleKind::Drawable => &manifest.drawables,
            StyleKind::TextButton => &manifest.text_buttons,
            StyleKind::Button => &manifest.buttons,
            StyleKind::ImageButton => &manifest.image_buttons,
            StyleKind::Pane => &manifest.panes,
            StyleKind::Slider => &manifest.sliders,
            StyleKind::Label => &manifest.labels,
            StyleKind::Field => &manifest.fields,
            StyleKind::Check => &manifest.checks,
            StyleKind::Dialog => &manifest.dialogs,
            StyleKind::Tree => &manifest.trees,
        }
    }

    /// Finds the kind that owns `name`, if any.
    pub fn of(manifest: &StylesManifest, name: &str) -> Option<StyleKind> {
        StyleKind::ALL
            .into_iter()
            .find(|kind| kind.names(manifest).iter().any(|n| n == name))
    }
}

/// Whether `name` resolves to `kind`.
pub fn lookup(manifest: &StylesManifest, kind: StyleKind, name: &str) -> bool {
    kind.names(manifest).iter().any(|n| n == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::manifest::StylesManifest;

    fn manifest() -> StylesManifest {
        StylesManifest::from_json(
            r#"{"format":1,"text_buttons":["defaultt","grayt"],"labels":["defaultLabel"],"drawables":["black","grayPanel"]}"#,
        )
        .unwrap()
    }

    #[test]
    fn style_lookup_resolves_kinds() {
        let manifest = manifest();
        assert!(lookup(&manifest, StyleKind::TextButton, "grayt"));
        assert!(!lookup(&manifest, StyleKind::TextButton, "black"));
        assert!(lookup(&manifest, StyleKind::Drawable, "black"));
        assert_eq!(
            StyleKind::of(&manifest, "grayt"),
            Some(StyleKind::TextButton)
        );
        assert_eq!(StyleKind::of(&manifest, "missing"), None);
        assert_eq!(StyleKind::TextButton.key(), "text_buttons");
    }
}
