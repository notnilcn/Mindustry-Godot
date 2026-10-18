// SPDX-License-Identifier: GPL-3.0-only

//! Data-asset decorators (PAR-0394): `state.data` records → live Godot assets.
//!
//! Java `DataManager` applies its records directly (`DataImagePacker` adds
//! atlas regions, `DataAudioLoader` registers `Sound`/`Music`, `DataBundleLoader`
//! mutates the runtime `Bundle`). The port keeps the records in
//! `mind_core::mods::assets::ModDataManager` (Godot-free) and applies them here:
//!
//! * [`MindAssets::apply_data_images`] — `dp-<name>` atlas regions (the mod
//!   overlay's page append, reused for data images) and `net-` runtime textures
//!   (`Logic.java` display textures);
//! * [`MindAssets::apply_data_bundles`] — merge `bundles/*.properties` into the
//!   live locale bundle;
//! * [`MindAssets::apply_data_audio`] — register `dp-<name>` streams in the
//!   asset probe registry (the playback half lives in `crate::audio`).
//!
//! The set is replace-only: every call drops the previous `dp-`/`net-` regions
//! first (`DataManager` reloads unapply before applying).

use godot::classes::{Image, ImageTexture};
use godot::global::Error;
use godot::prelude::*;

use mind_core::assets::atlas::{PageType, Region};
use mind_core::mods::assets::{DataAssetData, DataAssetType, ModDataManager};

use super::atlas::MindAssets;

impl MindAssets {
    /// Applies the manager's image records + `net-` runtime textures.
    ///
    /// External (non-embedded) records without a resolved payload are skipped
    /// with a warning: the asset cache (`DataAssetCache`) is not mounted on the
    /// client yet (recorded gap on PAR-0394).
    pub fn apply_data_images(&mut self, manager: &ModDataManager) {
        for name in std::mem::take(&mut self.data_regions) {
            self.index.remove_region(&name);
            self.region_textures.remove(&name);
        }
        let entries = manager.image_applier().entries().to_vec();
        for entry in entries {
            let Some(bytes) = data_blob(manager, DataAssetType::Image, &entry.path) else {
                log::warn!(
                    "[assets] data image `{}` has no embedded payload (cache not mounted)",
                    entry.path
                );
                continue;
            };
            let mut image = Image::new_gd();
            if image.load_png_from_buffer(&PackedByteArray::from(bytes.as_slice())) != Error::OK {
                log::warn!("[assets] data image decode failed: {}", entry.path);
                continue;
            }
            if (entry.scale - 1.0).abs() > f32::EPSILON {
                let width = ((image.get_width() as f32) * entry.scale).round().max(1.0) as i32;
                let height = ((image.get_height() as f32) * entry.scale).round().max(1.0) as i32;
                image.resize(width, height);
            }
            let width = image.get_width();
            let height = image.get_height();
            let Some(texture) = ImageTexture::create_from_image(&image) else {
                continue;
            };
            let page = self.index.add_overlay_page(entry.page, width, height);
            self.pages.push(texture);
            self.index.add_overlay_region(Region {
                name: entry.name.clone(),
                page,
                x: 0,
                y: 0,
                w: width,
                h: height,
                splits: None,
                pads: None,
                offsets: [0, 0],
                page_type: entry.page,
            });
            self.region_textures.remove(&entry.name);
            self.data_regions.push(entry.name);
        }

        // `net-` runtime textures (`DataManager.addTexture`, plan 21). Names
        // are already prefixed by the manager.
        for (name, png) in manager.runtime_textures() {
            let mut image = Image::new_gd();
            if image.load_png_from_buffer(&PackedByteArray::from(png.as_slice())) != Error::OK {
                log::warn!("[assets] runtime texture decode failed: {name}");
                continue;
            }
            let width = image.get_width();
            let height = image.get_height();
            let Some(texture) = ImageTexture::create_from_image(&image) else {
                continue;
            };
            let page = self.index.add_overlay_page(PageType::Main, width, height);
            self.pages.push(texture);
            self.index.add_overlay_region(Region {
                name: name.clone(),
                page,
                x: 0,
                y: 0,
                w: width,
                h: height,
                splits: None,
                pads: None,
                offsets: [0, 0],
                page_type: PageType::Main,
            });
            self.region_textures.remove(name);
            self.data_regions.push(name.clone());
        }
    }

    /// Applies the manager's bundle records (`DataBundleLoader.load`).
    pub fn apply_data_bundles(&mut self, manager: &ModDataManager) {
        let locale = self.locale.clone();
        for asset in manager.assets_of(DataAssetType::Bundle) {
            if let DataAssetData::Bundle(properties) = &asset.data {
                self.bundle.merge_assets(&locale, properties.clone());
            }
        }
    }

    /// Registers the manager's audio records in the asset probe registry
    /// (`MindAssets.sound_stream`), keyed by the `dp-` entry name.
    pub fn apply_data_audio(&mut self, manager: &ModDataManager) {
        for entry in manager.audio_applier().entries() {
            let type_ = if entry.music {
                DataAssetType::Music
            } else {
                DataAssetType::Sound
            };
            let Some(bytes) = data_blob(manager, type_, &entry.path) else {
                log::warn!(
                    "[assets] data audio `{}` has no embedded payload (cache not mounted)",
                    entry.path
                );
                continue;
            };
            self.audio.add_named_sound(&entry.name, bytes);
        }
    }
}

/// Embedded bytes of a data asset (`DataAssetData::Blob`), if any.
pub(crate) fn data_blob(
    manager: &ModDataManager,
    type_: DataAssetType,
    path: &str,
) -> Option<Vec<u8>> {
    manager.all().iter().find_map(|asset| {
        if asset.type_ != type_ || asset.path != path {
            return None;
        }
        match &asset.data {
            DataAssetData::Blob(bytes) if !bytes.is_empty() => Some(bytes.clone()),
            _ => None,
        }
    })
}
