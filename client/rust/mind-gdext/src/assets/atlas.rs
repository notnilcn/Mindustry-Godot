// SPDX-License-Identifier: GPL-3.0-only

//! `MindAssets` — the Godot runtime atlas/loose-texture binding and MCP probe
//! (plan 03 §3.6/§7.1c).
//!
//! Loads `sprites.atlas.json` + page PNGs through `FileAccess` + runtime
//! `Image` decode (uniform for vanilla and future mod pages, A1/R6), builds
//! `AtlasTexture`s lazily per region, loads loose textures and exposes:
//!
//! * `probe(name) -> Dictionary` — MCP assertion payload.
//! * `find_region(name) -> Texture2D` — lazy `AtlasTexture` (null when absent).
//! * `region_count()`, `atlas_duplicates()`.
//!
//! The in-engine MCP scenario is deferred to the orchestrator's single-editor
//! mutex; the exact eval strings live in `assets/parity/mcp_assets_scenario.md`.

use std::collections::HashMap;

use godot::builtin::{Array, GString, Rect2, VarDictionary, Vector2};
use godot::classes::{AtlasTexture, FileAccess, INode, Image, ImageTexture, Node};
use godot::global::Error;
use godot::obj::{Base, NewGd};
use godot::prelude::*;

use mind_core::assets::atlas::{AtlasIndex, Region};
use mind_core::assets::file_tree::FileTree;

use crate::assets::loader::resolve_assets_dir;

/// Runtime atlas binding + probe singleton.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindAssets {
    base: Base<Node>,
    /// Region metadata index (empty until [`MindAssets::load_assets`]).
    index: AtlasIndex,
    /// Mods-first virtual FS (populated by plan 20; empty for vanilla).
    tree: FileTree,
    /// Resolved asset root.
    assets_dir: String,
    /// Decoded page textures, indexed by page index.
    pages: Vec<Gd<ImageTexture>>,
    /// Lazily built region textures.
    region_textures: HashMap<String, Gd<AtlasTexture>>,
    /// Loose textures (`sprites/<name>.png` not in the atlas).
    loose_textures: HashMap<String, Gd<ImageTexture>>,
    /// Whether [`MindAssets::load_assets`] completed.
    loaded: bool,
}

#[godot_api]
impl INode for MindAssets {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            index: AtlasIndex::default(),
            tree: FileTree::new(),
            assets_dir: String::new(),
            pages: Vec::new(),
            region_textures: HashMap::new(),
            loose_textures: HashMap::new(),
            loaded: false,
        }
    }

    fn ready(&mut self) {
        let ok = self.load_assets();
        log::info!(
            "[assets] ready ok={ok} dir={} pages={} regions={}",
            self.assets_dir,
            self.pages.len(),
            self.index.len()
        );
    }
}

#[godot_api]
impl MindAssets {
    /// Loads the manifest + pages once (re-entrant no-op after success).
    #[func]
    pub fn load_assets(&mut self) -> bool {
        if self.loaded {
            return true;
        }
        self.assets_dir = resolve_assets_dir();
        let manifest_path = format!("{}/sprites/sprites.atlas.json", self.assets_dir);
        if !FileAccess::file_exists(&manifest_path) {
            log::error!("[assets] missing manifest {manifest_path}");
            return false;
        }
        let manifest_json = FileAccess::get_file_as_string(&manifest_path).to_string();
        let index = match AtlasIndex::from_manifest_json(&manifest_json) {
            Ok(index) => index,
            Err(error) => {
                log::error!("[assets] manifest parse failed: {error}");
                return false;
            }
        };

        let mut pages = Vec::with_capacity(index.pages().len());
        for page in index.pages() {
            let path = format!("{}/sprites/{}", self.assets_dir, page.file);
            let bytes = FileAccess::get_file_as_bytes(&path);
            let mut image = Image::new_gd();
            if image.load_png_from_buffer(&bytes) != Error::OK {
                log::error!("[assets] page decode failed: {path}");
                return false;
            }
            let Some(texture) = ImageTexture::create_from_image(&image) else {
                log::error!("[assets] texture create failed: {path}");
                return false;
            };
            pages.push(texture);
        }

        self.index = index;
        self.pages = pages;
        self.loaded = true;
        true
    }

    /// `Core.atlas.find(name) != null`.
    #[func]
    pub fn has_region(&self, name: GString) -> bool {
        self.index.has(&name.to_string())
    }

    /// Number of regions in the loaded manifest.
    #[func]
    pub fn region_count(&self) -> i64 {
        self.index.len() as i64
    }

    /// Region names appearing more than once (always empty for a valid pack).
    #[func]
    pub fn atlas_duplicates(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in self.index.duplicates() {
            out.push(&GString::from(name));
        }
        out
    }

    /// `MindAssets.probe(name)` — MCP assertion payload (`§3.8`).
    ///
    /// Returns `{found, page, pageType, x, y, w, h, splits, pads}`; `found` is
    /// `false` and the geometry keys are absent for an unknown region (no
    /// silent `error` fallback).
    #[func]
    pub fn probe(&self, name: GString) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = name.to_string();
        let Some(region) = self.index.find(&key) else {
            dict.set(&GString::from("found"), &false.to_variant());
            return dict;
        };
        dict.set(&GString::from("found"), &true.to_variant());
        write_region(&mut dict, region);
        dict
    }

    /// Lazily builds (and caches) the `AtlasTexture` for a region.
    #[func]
    pub fn find_region(&mut self, name: GString) -> Option<Gd<AtlasTexture>> {
        let key = name.to_string();
        if let Some(texture) = self.region_textures.get(&key) {
            return Some(texture.clone());
        }
        let region = self.index.find(&key)?.clone();
        let page = self.pages.get(region.page)?.clone();
        let mut texture = AtlasTexture::new_gd();
        texture.set_atlas(&page.upcast::<godot::classes::Texture2D>());
        texture.set_region(Rect2::new(
            Vector2::new(region.x as f32, region.y as f32),
            Vector2::new(region.w as f32, region.h as f32),
        ));
        texture.set_filter_clip(true);
        self.region_textures.insert(key, texture.clone());
        Some(texture)
    }

    /// Loads and caches a loose `sprites/<name>.png` texture (no atlas entry).
    #[func]
    pub fn find_loose(&mut self, name: GString) -> Option<Gd<ImageTexture>> {
        let key = name.to_string();
        if let Some(texture) = self.loose_textures.get(&key) {
            return Some(texture.clone());
        }
        let path = format!("{}/sprites/{key}.png", self.assets_dir);
        if !FileAccess::file_exists(&path) {
            return None;
        }
        let bytes = FileAccess::get_file_as_bytes(&path);
        let mut image = Image::new_gd();
        if image.load_png_from_buffer(&bytes) != Error::OK {
            return None;
        }
        let texture = ImageTexture::create_from_image(&image)?;
        self.loose_textures.insert(key, texture.clone());
        Some(texture)
    }

    /// Resolved asset root (for diagnostics).
    #[func]
    pub fn assets_dir(&self) -> GString {
        GString::from(self.assets_dir.as_str())
    }

    /// Mods-first virtual FS (plan 20 hook); vanilla reads stay on disk.
    #[func]
    pub fn file_tree(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        for path in self.tree.overlay_paths() {
            dict.set(&GString::from(path), &true.to_variant());
        }
        dict
    }
}

/// Writes the region's geometry fields into a probe dictionary.
fn write_region(dict: &mut VarDictionary, region: &Region) {
    dict.set(&GString::from("page"), &(region.page as i64).to_variant());
    dict.set(
        &GString::from("pageType"),
        &GString::from(region.page_type.name()).to_variant(),
    );
    dict.set(&GString::from("x"), &(region.x as i64).to_variant());
    dict.set(&GString::from("y"), &(region.y as i64).to_variant());
    dict.set(&GString::from("w"), &(region.w as i64).to_variant());
    dict.set(&GString::from("h"), &(region.h as i64).to_variant());
    if let Some(splits) = region.splits {
        let mut array = Array::<i64>::new();
        for value in splits {
            array.push(value as i64);
        }
        dict.set(&GString::from("splits"), &array.to_variant());
    }
    if let Some(pads) = region.pads {
        let mut array = Array::<i64>::new();
        for value in pads {
            array.push(value as i64);
        }
        dict.set(&GString::from("pads"), &array.to_variant());
    }
}
