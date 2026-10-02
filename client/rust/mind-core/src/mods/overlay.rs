// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mod overlay provider (plan 20 §3.8, plan 03 `AssetOverlayProvider`).
//!
//! Drives plan 03's stable handshake from loaded mods: `sprites/` prefixes,
//! `sprites-override/` replaces + missing-target warnings, `getPage` routing,
//! `texturescale` recording, `pregenerated`, and locale bundle overlays.
//! `mind-core` stays Godot-free: PNG bytes are carried, never decoded here.

use indexmap::IndexMap;

use crate::assets::atlas::PageType;
use crate::assets::bundle::parse_properties;
use crate::assets::overlay::{
    AssetOverlayProvider, OverlayImage, OverlayShader, OverlaySound, OverlaySprite, sprite_page,
    sprite_region_name,
};
use crate::io::FileSystem;

use super::Mods;

/// One mod `texturescale` entry (`name → scale`), applied to `Region.scale`
/// after overlay packaging (upstream `textureResize`).
#[derive(Debug, Clone, PartialEq)]
pub struct TextureScale {
    /// Resolved region name.
    pub name: String,
    /// Scale factor.
    pub scale: f32,
}

/// Built overlay data from all enabled mods, implementing plan 03's provider
/// trait. `MindAssets` consumes it to append overlay atlas pages/bundles.
#[derive(Debug, Default, Clone)]
pub struct ModsOverlay {
    sprites: Vec<OverlaySprite>,
    bundles: Vec<(String, IndexMap<String, String>)>,
    sounds: Vec<OverlaySound>,
    images: Vec<OverlayImage>,
    shaders: Vec<OverlayShader>,
    /// Region name → texture scale (only for `!= 1.0`).
    pub texture_scales: Vec<TextureScale>,
    /// Non-fatal warnings (missing override targets, malformed bundles).
    pub warnings: Vec<String>,
    /// Whether any contributing mod set `pregenerated` (disables bleeding and
    /// generated icons, plan 03).
    pub pregenerated: bool,
}

impl AssetOverlayProvider for ModsOverlay {
    fn sprites(&self) -> &[OverlaySprite] {
        &self.sprites
    }

    fn bundles(&self) -> &[(String, IndexMap<String, String>)] {
        &self.bundles
    }

    fn sounds(&self) -> &[OverlaySound] {
        &self.sounds
    }

    fn images(&self) -> &[OverlayImage] {
        &self.images
    }

    fn shaders(&self) -> &[OverlayShader] {
        &self.shaders
    }
}

impl ModsOverlay {
    /// Region names present in the overlay (harness `--probe`).
    pub fn region_names(&self) -> Vec<&str> {
        self.sprites
            .iter()
            .map(|sprite| sprite.name.as_str())
            .collect()
    }

    /// Region name → page (`probe` output).
    pub fn probe(&self, name: &str) -> Option<(&str, PageType)> {
        self.sprites
            .iter()
            .find(|sprite| sprite.name == name)
            .map(|sprite| (sprite.path.as_str(), sprite.page))
    }

    /// Scale for a region, if any.
    pub fn texture_scale(&self, name: &str) -> Option<f32> {
        self.texture_scales
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.scale)
    }
}

impl Mods {
    /// Builds overlay data from enabled mods in dependency order. `atlas_has`
    /// reports whether a `sprites-override/` target exists in the vanilla atlas.
    pub fn build_overlay(
        &mut self,
        fs: &dyn FileSystem,
        atlas_has: &dyn Fn(&str) -> bool,
    ) -> ModsOverlay {
        let mut overlay = ModsOverlay::default();
        let ordered = self.ordered_mods();
        for index in ordered {
            let Some(mod_) = self.mods.get(index) else {
                continue;
            };
            if !mod_.enabled() {
                continue;
            }
            if mod_.meta.pregenerated {
                overlay.pregenerated = true;
            }
            pack_sprite_dir(fs, mod_, "sprites", true, atlas_has, &mut overlay);
            pack_sprite_dir(fs, mod_, "sprites-override", false, atlas_has, &mut overlay);

            // Locale bundle overlays (`bundles/bundle[_<locale>].properties`).
            if let Ok(files) = mod_.root.walk(fs, "bundles") {
                for path in files {
                    let Some(file_name) = path.rsplit('/').next() else {
                        continue;
                    };
                    if !file_name.starts_with("bundle") || !file_name.ends_with(".properties") {
                        continue;
                    }
                    match mod_.root.read_to_string(fs, &path) {
                        Ok(text) => {
                            overlay
                                .bundles
                                .push((path.clone(), parse_properties(&text)));
                        }
                        Err(error) => overlay.warnings.push(format!("{path}: {error}")),
                    }
                }
            }
        }
        overlay
    }
}

/// `Mods.packSprites` file half for one directory.
fn pack_sprite_dir(
    fs: &dyn FileSystem,
    mod_: &super::LoadedMod,
    dir: &str,
    prefix: bool,
    atlas_has: &dyn Fn(&str) -> bool,
    overlay: &mut ModsOverlay,
) {
    let Ok(files) = mod_.root.walk(fs, dir) else {
        return;
    };
    for path in files {
        if !path.ends_with(".png") {
            continue;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            continue;
        };
        if file_name.is_empty() {
            continue;
        }
        let name = sprite_region_name(&mod_.name, file_name, prefix);
        if name.is_empty() {
            continue;
        }
        if !prefix && !atlas_has(&name) && !overlay.sprites.iter().any(|sprite| sprite.name == name)
        {
            let warning = crate::assets::overlay::override_warning(true, &name, atlas_has)
                .unwrap_or_else(|| {
                    format!("sprite `{name}` attempts to override a non-existent sprite")
                });
            overlay.warnings.push(warning);
        }
        let Ok(png) = mod_.root.read(fs, &path) else {
            continue;
        };
        let page = sprite_page(&path);
        overlay.sprites.push(OverlaySprite {
            path: path.clone(),
            name: name.clone(),
            png,
            replace: !prefix,
            page,
        });
        if (mod_.meta.texturescale - 1.0).abs() > f32::EPSILON {
            overlay.texture_scales.push(TextureScale {
                name,
                scale: mod_.meta.texturescale,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;
    use crate::io::SettingsStore;
    use std::path::Path;

    fn write_mod(fs: &MockFs, meta: &str) {
        fs.write(Path::new("/mods/m/mod.json"), meta.as_bytes())
            .expect("mod.json");
    }

    fn load(fs: &MockFs) -> Mods {
        let mut mods = Mods::new(true, "/mods");
        mods.load_single(fs, Path::new("/mods/m"), &SettingsStore::new())
            .expect("load");
        mods
    }

    fn no_atlas(_: &str) -> bool {
        false
    }

    /// Plan 20 M5: `overlay::prefix_rule_already_prefixed`.
    #[test]
    fn prefix_rule_already_prefixed() {
        let fs = MockFs::new();
        write_mod(&fs, r#"{"name":"MyMod","minGameVersion":"146"}"#);
        fs.write(Path::new("/mods/m/sprites/blocks/walls/copper.png"), b"a")
            .expect("sprite");
        fs.write(Path::new("/mods/m/sprites/block-mymod-full.png"), b"b")
            .expect("sprite");
        let mut mods = load(&fs);
        let overlay = mods.build_overlay(&fs, &no_atlas);
        let names = overlay.region_names();
        assert!(names.contains(&"mymod-copper"));
        assert!(
            names.contains(&"block-mymod-full"),
            "already category-prefixed"
        );
    }

    /// Plan 20 M5: `overlay::override_warns_missing`.
    #[test]
    fn override_warns_missing() {
        let fs = MockFs::new();
        write_mod(&fs, r#"{"name":"MyMod","minGameVersion":"146"}"#);
        fs.write(Path::new("/mods/m/sprites-override/ghost.png"), b"a")
            .expect("sprite");
        let mut mods = load(&fs);
        let overlay = mods.build_overlay(&fs, &no_atlas);
        assert_eq!(overlay.region_names(), vec!["ghost"]);
        assert!(!overlay.warnings.is_empty(), "missing override warns");
    }

    /// Plan 20 M5: `overlay::page_routing`.
    #[test]
    fn page_routing() {
        let fs = MockFs::new();
        write_mod(&fs, r#"{"name":"MyMod","minGameVersion":"146"}"#);
        fs.write(
            Path::new("/mods/m/sprites/blocks/environment/stone.png"),
            b"a",
        )
        .expect("sprite");
        fs.write(Path::new("/mods/m/sprites/ui/bar.png"), b"b")
            .expect("sprite");
        let mut mods = load(&fs);
        let overlay = mods.build_overlay(&fs, &no_atlas);
        assert_eq!(
            overlay.probe("mymod-stone").map(|(_, page)| page),
            Some(PageType::Environment)
        );
        assert_eq!(
            overlay.probe("mymod-bar").map(|(_, page)| page),
            Some(PageType::Ui)
        );
    }

    /// Plan 20 M5: `overlay::texture_scale_map`.
    #[test]
    fn texture_scale_map() {
        let fs = MockFs::new();
        write_mod(
            &fs,
            r#"{"name":"MyMod","minGameVersion":"146","texturescale":2.0}"#,
        );
        fs.write(Path::new("/mods/m/sprites/foo.png"), b"a")
            .expect("sprite");
        let mut mods = load(&fs);
        let overlay = mods.build_overlay(&fs, &no_atlas);
        assert_eq!(overlay.texture_scale("mymod-foo"), Some(2.0));
    }

    /// Plan 20 M5: `overlay::pregenerated_no_bleed`.
    #[test]
    fn pregenerated_no_bleed() {
        let fs = MockFs::new();
        write_mod(
            &fs,
            r#"{"name":"MyMod","minGameVersion":"146","pregenerated":true}"#,
        );
        fs.write(Path::new("/mods/m/sprites/foo.png"), b"a")
            .expect("sprite");
        let mut mods = load(&fs);
        let overlay = mods.build_overlay(&fs, &no_atlas);
        assert!(overlay.pregenerated);
    }

    #[test]
    fn bundle_overlay_merges() {
        let fs = MockFs::new();
        write_mod(&fs, r#"{"name":"MyMod","minGameVersion":"146"}"#);
        fs.write(
            Path::new("/mods/m/bundles/bundle.properties"),
            b"mod.mymod.key=Hello\n",
        )
        .expect("bundle");
        let mut mods = load(&fs);
        let overlay = mods.build_overlay(&fs, &no_atlas);
        assert_eq!(overlay.bundles().len(), 1);
        assert_eq!(
            overlay.bundles()[0]
                .1
                .get("mod.mymod.key")
                .map(String::as_str),
            Some("Hello")
        );
    }
}
