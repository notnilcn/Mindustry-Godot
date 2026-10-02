// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DataImagePacker` driver (plan 20 §3.7): `dp-` region prefix, page routing,
//! generated-sprite hashing and `texturescale` recording.
//!
//! Godot-free and record-only: actual atlas pages are appended by plan 03's
//! `AtlasOverlayBuilder` in `mind-gdext` from these entries. Headless (server)
//! keeps the same records and writes generated PNGs under
//! `data/assets/sprites/generated/` when icons exist (accepted deviation 2.4.7).

use crate::assets::atlas::PageType;
use crate::assets::overlay::sprite_page;

use super::{DATA_PREFIX, encode_hash, image_asset_name, sha256};

/// One image record produced from a mod/map image asset.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageEntry {
    /// Source path (or `generated/<hash>/<name>.png` for generated icons).
    pub path: String,
    /// Region name (`dp-` prefixed unless already generated).
    pub name: String,
    /// Target atlas page (`getPage`).
    pub page: PageType,
    /// Whether this is a content-generated icon.
    pub generated: bool,
    /// `texturescale` applied to this region after overlay.
    pub scale: f32,
}

/// `DataImagePacker`: records mod/map images and generated content sprites.
#[derive(Debug, Default)]
pub struct ImageApplier {
    entries: Vec<ImageEntry>,
    generated_hashes: Vec<String>,
}

impl ImageApplier {
    /// Empty applier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads image assets (`DataImagePacker.add`), applying `texturescale` to
    /// each region. `assets` are `(path, generated)` in caller order.
    pub fn load_images(&mut self, assets: &[(String, bool)], texturescale: f32) -> &[ImageEntry] {
        let start = self.entries.len();
        for (path, generated) in assets {
            let stem = image_stem(path);
            let name = image_asset_name(&stem, *generated);
            self.entries.push(ImageEntry {
                path: path.clone(),
                name,
                page: sprite_page(path),
                generated: *generated,
                scale: texturescale,
            });
        }
        &self.entries[start..]
    }

    /// Records a generated content sprite (`regenerateContentSprites`).
    ///
    /// Returns `false` when an icon with the same content hash was already
    /// generated and `force` is `false` (upstream skips unchanged content).
    pub fn add_generated(&mut self, type_name: &str, json: &str, base_name: &str) -> bool {
        let hash = generated_hash(type_name, json);
        if self.generated_hashes.contains(&hash) {
            return false;
        }
        let name = image_asset_name(base_name, true);
        self.entries.push(ImageEntry {
            path: generated_path(&hash, &name),
            name,
            page: PageType::Main,
            generated: true,
            scale: 1.0,
        });
        self.generated_hashes.push(hash);
        true
    }

    /// Generated content hashes recorded so far.
    pub fn generated_hashes(&self) -> &[String] {
        &self.generated_hashes
    }

    /// All image entries.
    pub fn entries(&self) -> &[ImageEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is loaded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Region names present.
    pub fn region_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    /// Removes generated icons but keeps regular mod images
    /// (`clearGeneratedImages` + force regeneration).
    pub fn clear_generated(&mut self) {
        self.entries.retain(|entry| !entry.generated);
        self.generated_hashes.clear();
    }

    /// Unloads every recorded image, returning the removed entries.
    pub fn unload(&mut self) -> Vec<ImageEntry> {
        self.generated_hashes.clear();
        std::mem::take(&mut self.entries)
    }
}

/// `ContentAsset.hashData`: `"<type>_<base32(sha256(data))>"`.
pub fn generated_hash(type_name: &str, json: &str) -> String {
    format!("{type_name}_{}", encode_hash(&sha256(json.as_bytes())))
}

/// `DataManager` generated-sprite cache path (`generated/<hash>/<name>.png`).
pub fn generated_path(hash: &str, name: &str) -> String {
    format!("generated/{hash}/{name}.png")
}

/// File stem of an image path without extension.
pub fn image_stem(path: &str) -> String {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(file_name)
        .to_owned()
}

/// Whether a region name is already reserved (`dp-`).
pub fn is_reserved(name: &str) -> bool {
    name.starts_with(DATA_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan 20 M4: `assets::dp_prefixes` (image driver half) + page routing.
    #[test]
    fn image_prefix_and_page_routing() {
        let mut applier = ImageApplier::new();
        applier.load_images(
            &[
                ("sprites/blocks/environment/stone.png".to_owned(), false),
                ("sprites/ui/bar.png".to_owned(), false),
                ("generated/abc/dp-icon.png".to_owned(), true),
            ],
            2.0,
        );
        assert_eq!(applier.entries()[0].name, "dp-stone");
        assert_eq!(applier.entries()[0].page, PageType::Environment);
        assert_eq!(applier.entries()[0].scale, 2.0);
        assert_eq!(applier.entries()[1].name, "dp-bar");
        assert_eq!(applier.entries()[1].page, PageType::Ui);
        // Generated names keep their existing `dp-`.
        assert_eq!(applier.entries()[2].name, "dp-icon");
        assert!(applier.entries()[2].generated);
    }

    #[test]
    fn generated_paths_and_dedup() {
        let mut applier = ImageApplier::new();
        assert!(applier.add_generated("block", r#"{"name":"X"}"#, "test-wall"));
        assert!(
            !applier.add_generated("block", r#"{"name":"X"}"#, "test-wall"),
            "same hash is skipped"
        );
        assert!(applier.add_generated("block", r#"{"name":"Y"}"#, "test-wall"));
        let entry = &applier.entries()[0];
        assert!(entry.name.starts_with("dp-test-wall"));
        assert!(entry.path.starts_with("generated/"));
        assert!(entry.path.ends_with("dp-test-wall.png"));
        let hash = generated_hash("block", r#"{"name":"X"}"#);
        assert_eq!(entry.path, generated_path(&hash, &entry.name));
        assert_eq!(applier.generated_hashes().len(), 2);
    }

    #[test]
    fn unload_clears_generated_hashes() {
        let mut applier = ImageApplier::new();
        applier.add_generated("item", "{}", "x");
        assert_eq!(applier.len(), 1);
        let removed = applier.unload();
        assert_eq!(removed.len(), 1);
        assert!(applier.is_empty());
        assert!(applier.generated_hashes().is_empty());
        assert!(applier.add_generated("item", "{}", "x"));
    }
}
