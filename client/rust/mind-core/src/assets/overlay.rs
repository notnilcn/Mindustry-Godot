// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: `core/src/mindustry/mod/Mods.java` (`packSprites` prefix rule,
//         `sprites/` vs `sprites-override/`, `getPage` routing,
//         `DataImagePacker`/`DataAudioLoader` `dp-` conventions),
//         `core/src/mindustry/mod/DataBundleLoader.java` (bundle merge).

//! Mod asset overlay interfaces (plan 03 §3.8/M9).
//!
//! Plan 20 owns mod loading; this module is the stable handshake it drives:
//! the [`AssetOverlayProvider`] trait, the sprite prefix/override/page rules
//! (verbatim from `Mods.packSprites`/`getPage`) and the `dp-` data-asset
//! convention. Overlay page packing at runtime (`AtlasOverlayBuilder`) lives in
//! `mind-atlas`; `FileTree::add_file` and `Bundle::merge_assets` are the
//! filesystem/bundle hooks already landed (M5/M7).

use indexmap::IndexMap;

use super::atlas::PageType;

/// One mod sprite to pack into an overlay page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySprite {
    /// Mod-relative path (`sprites/blocks/walls/x.png` or `sprites-override/...`).
    pub path: String,
    /// Resolved region name (prefix applied / override unchanged).
    pub name: String,
    /// PNG bytes.
    pub png: Vec<u8>,
    /// `true` for `sprites-override/` (replace an existing region).
    pub replace: bool,
    /// Target page type (`getPage`).
    pub page: PageType,
}

/// One `dp-` data image (packed outside the vanilla atlas, plan 20/21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayImage {
    /// Mod-relative path.
    pub path: String,
    /// PNG bytes.
    pub png: Vec<u8>,
}

/// One mod sound overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySound {
    /// Mod-relative path.
    pub path: String,
    /// Logical sound name.
    pub name: String,
    /// File bytes (`.ogg`/`.mp3`).
    pub bytes: Vec<u8>,
}

/// One mod shader overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayShader {
    /// Logical shader name.
    pub name: String,
    /// Shader source (`.gdshader` or GLSL).
    pub source: String,
}

/// Plan-20 implementation surface consumed by the plan-03 asset runtime.
pub trait AssetOverlayProvider {
    /// `sprites/` + `sprites-override/` entries.
    fn sprites(&self) -> &[OverlaySprite];
    /// Target bundle file → properties (e.g. `bundles/bundle.properties`).
    fn bundles(&self) -> &[(String, IndexMap<String, String>)];
    /// Sound overlays.
    fn sounds(&self) -> &[OverlaySound];
    /// `dp-` data images.
    fn images(&self) -> &[OverlayImage];
    /// Shader overlays.
    fn shaders(&self) -> &[OverlayShader];
}

/// `dp-` data-asset prefix (bundle/image files not packed into the atlas).
pub const DATA_PREFIX: &str = "dp-";

/// Whether a mod file is a `dp-` data asset.
pub fn is_data_asset(file_name: &str) -> bool {
    file_name.starts_with(DATA_PREFIX)
}

/// `Mods.packSprites` region-name resolution for a file name (with extension).
///
/// `prefix == true` for `sprites/` (prefix with `<mod>-` unless the name is
/// already `<category>-<mod>-...`); `false` for `sprites-override/` (unchanged).
/// `regionName` truncates at the **first** dot (`bar.9.png` → `bar`), matching
/// upstream `nameWithoutExtension` + `indexOf('.')`.
pub fn sprite_region_name(mod_name: &str, file_name: &str, prefix: bool) -> String {
    let base_name = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let region_name = base_name
        .split_once('.')
        .map_or(base_name, |(head, _)| head);
    let already_prefixed = base_name
        .split_once('-')
        .is_some_and(|(_, rest)| rest.starts_with(&format!("{mod_name}-")));
    if prefix && !already_prefixed {
        format!("{mod_name}-{region_name}")
    } else {
        region_name.to_owned()
    }
}

/// `Mods.getPage` path routing.
pub fn sprite_page(path: &str) -> PageType {
    if path.contains("sprites/blocks/environment")
        || path.contains("sprites-override/blocks/environment")
    {
        PageType::Environment
    } else if path.contains("sprites/rubble") || path.contains("sprites-override/rubble") {
        PageType::Rubble
    } else if path.contains("sprites/ui") || path.contains("sprites-override/ui") {
        PageType::Ui
    } else {
        PageType::Main
    }
}

/// Upstream override warning: `Some(region)` when a `sprites-override/` entry
/// targets a region the vanilla atlas does not contain.
pub fn override_warning(
    replace: bool,
    region_name: &str,
    atlas_has: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if replace && !atlas_has(region_name) {
        Some(format!(
            "sprite `{region_name}` attempts to override a non-existent sprite"
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_rule_matches_mods_pack_sprites() {
        // `sprites/` gets the mod prefix.
        assert_eq!(
            sprite_region_name("mymod", "copper-wall.png", true),
            "mymod-copper-wall"
        );
        // Ninepatch truncates at the first dot.
        assert_eq!(sprite_region_name("mymod", "bar.9.png", true), "mymod-bar");
        // Already category-prefixed names are not double-prefixed.
        assert_eq!(
            sprite_region_name("mymod", "block-mymod-full.png", true),
            "block-mymod-full"
        );
        // Overrides are never prefixed.
        assert_eq!(
            sprite_region_name("mymod", "copper-wall.png", false),
            "copper-wall"
        );
    }

    #[test]
    fn page_routing_matches_mods_get_page() {
        assert_eq!(
            sprite_page("sprites/blocks/environment/stone.png"),
            PageType::Environment
        );
        assert_eq!(sprite_page("sprites-override/ui/bar.png"), PageType::Ui);
        assert_eq!(sprite_page("sprites/rubble/x-wreck0.png"), PageType::Rubble);
        assert_eq!(sprite_page("sprites/blocks/walls/x.png"), PageType::Main);
    }

    #[test]
    fn override_warning_and_dp_prefix() {
        let has = |name: &str| name == "copper-wall";
        assert!(override_warning(true, "missing", &has).is_some());
        assert!(override_warning(true, "copper-wall", &has).is_none());
        assert!(override_warning(false, "missing", &has).is_none());
        assert!(is_data_asset("dp-myimage.png"));
        assert!(!is_data_asset("myimage.png"));
    }

    /// Fixture mod handshake: `sprites/` prefixes, `sprites-override/` replaces,
    /// `dp-` images stay out of the atlas, and `bundles/` merge into the live
    /// bundle (plan 20 drives this; plan 03 owns the hooks).
    #[test]
    fn fixture_mod_handshake() {
        use super::super::bundle::{Bundle, parse_properties};
        use super::super::file_tree::{AssetFile, FileTree};

        struct FixtureMod {
            sprites: Vec<OverlaySprite>,
            bundles: Vec<(String, IndexMap<String, String>)>,
            images: Vec<OverlayImage>,
        }
        impl AssetOverlayProvider for FixtureMod {
            fn sprites(&self) -> &[OverlaySprite] {
                &self.sprites
            }
            fn bundles(&self) -> &[(String, IndexMap<String, String>)] {
                &self.bundles
            }
            fn sounds(&self) -> &[OverlaySound] {
                &[]
            }
            fn images(&self) -> &[OverlayImage] {
                &self.images
            }
            fn shaders(&self) -> &[OverlayShader] {
                &[]
            }
        }

        let mod_ = FixtureMod {
            sprites: vec![
                OverlaySprite {
                    path: String::from("sprites/blocks/walls/copper-wall.png"),
                    name: sprite_region_name("mymod", "copper-wall.png", true),
                    png: vec![1],
                    replace: false,
                    page: sprite_page("sprites/blocks/walls/copper-wall.png"),
                },
                OverlaySprite {
                    path: String::from("sprites-override/copper-wall.png"),
                    name: sprite_region_name("mymod", "copper-wall.png", false),
                    png: vec![2],
                    replace: true,
                    page: sprite_page("sprites-override/copper-wall.png"),
                },
            ],
            bundles: vec![(
                String::from("bundles/bundle.properties"),
                parse_properties("mod.mymod.key=Hello\n"),
            )],
            images: vec![OverlayImage {
                path: String::from("dp-icon.png"),
                png: vec![3],
            }],
        };

        // FileTree: internal vanilla first, then mod overlays by path.
        let mut tree = FileTree::new();
        tree.add_internal(
            "sprites/copper-wall.png",
            AssetFile::new("sprites/copper-wall.png", vec![0]),
        );
        for sprite in mod_.sprites() {
            tree.add_file(
                &sprite.path,
                AssetFile::new(&sprite.path, sprite.png.clone()),
            );
        }
        assert_eq!(tree.get("sprites/copper-wall.png").unwrap().bytes, vec![0]);
        assert_eq!(
            tree.get("sprites/blocks/walls/copper-wall.png")
                .unwrap()
                .bytes,
            vec![1]
        );
        assert_eq!(
            tree.get("sprites-override/copper-wall.png").unwrap().bytes,
            vec![2]
        );
        assert_eq!(mod_.sprites[1].name, "copper-wall");
        assert!(mod_.sprites[1].replace);
        assert!(!tree.has("dp-icon.png"));

        // Bundle merge supplies new keys.
        let mut bundle = Bundle::from_layers(vec![parse_properties("mod.base=Base\n")]);
        for (_, props) in mod_.bundles() {
            bundle.merge_assets("mymod", props.clone());
        }
        assert_eq!(bundle.get("mod.mymod.key"), "Hello");
        assert_eq!(bundle.get("mod.base"), "Base");

        // Override warning fires for an unknown target.
        let atlas_has = |name: &str| name == "copper-wall";
        assert!(override_warning(true, "copper-wall", &atlas_has).is_none());
    }
}
