// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapAssetsDialog` data + zip import/export (plan 19 M6/§3.12;
//! `editor/data/MapAssetsDialog.java`).
//!
//! Plan 20 owns the asset records ([`crate::mods::assets`]); this module is the
//! editor-facing view. `MapAssetsDialog` imports/exports a `.zip` whose layout
//! follows the `DataAssetType` folders (`patches/content/bundles/sprites/
//! sounds/music`, §6.7). The [`DataManagerApi`] trait names match upstream
//! (`get_all_assets`/`get_full_path`/`get_data`) so `mind-gdext` can adapt the
//! plan-20 manager without drift.

use std::io::{Cursor, Read, Write};

use crate::content::ContentType;
use crate::io::{IoError, IoResult};
use crate::mods::assets::{DataAsset, DataAssetData, DataAssetType};

/// Minimal plan-20 manager surface the dialog needs.
pub trait DataManagerApi {
    /// All asset records (`DataManager.get_all_assets`).
    fn get_all_assets(&self) -> Vec<&DataAsset>;
    /// The zip path for an asset (`DataAsset.get_full_path`).
    fn get_full_path(&self, asset: &DataAsset) -> String;
    /// The payload bytes (`DataAsset.get_data` / cache-file bytes).
    fn get_data(&self, asset: &DataAsset) -> Vec<u8>;
    /// Whether `path` already exists for a type (`DataManager.hasAssetPath`).
    fn has_asset_path(&self, type_: DataAssetType, path: &str) -> bool;
}

impl DataManagerApi for Vec<DataAsset> {
    fn get_all_assets(&self) -> Vec<&DataAsset> {
        self.iter().collect()
    }

    fn get_full_path(&self, asset: &DataAsset) -> String {
        full_path(asset)
    }

    fn get_data(&self, asset: &DataAsset) -> Vec<u8> {
        asset_bytes(asset)
    }

    fn has_asset_path(&self, type_: DataAssetType, path: &str) -> bool {
        self.iter()
            .any(|asset| asset.type_ == type_ && asset.path == path)
    }
}

/// The zip path of an asset (`type.folder + "/" + path`).
pub fn full_path(asset: &DataAsset) -> String {
    let folder = asset.type_.folder();
    let relative = match &asset.data {
        DataAssetData::Content(record) => {
            format!("{}/{}.json", record.type_.folder(), asset.name)
        }
        DataAssetData::Bundle(_) => format!("{}.properties", asset.name),
        DataAssetData::Blob(_) => asset.path.clone(),
        DataAssetData::Patch(_) => asset.path.clone(),
    };
    format!("{folder}/{relative}")
}

/// The payload bytes of an asset (`get_data`).
pub fn asset_bytes(asset: &DataAsset) -> Vec<u8> {
    match &asset.data {
        DataAssetData::Patch(json) => json.clone().into_bytes(),
        DataAssetData::Content(record) => record.json.clone().into_bytes(),
        DataAssetData::Bundle(map) => crate::mods::assets::render_properties(map).into_bytes(),
        DataAssetData::Blob(bytes) => bytes.clone(),
    }
}

/// Exports every asset as a `.zip` (`.zip` bytes).
pub fn export_zip(api: &dyn DataManagerApi) -> IoResult<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for asset in api.get_all_assets() {
        let name = api.get_full_path(asset);
        writer
            .start_file(name, options)
            .map_err(|error| IoError::corrupt(format!("zip start_file: {error}")))?;
        writer
            .write_all(&api.get_data(asset))
            .map_err(|error| IoError::corrupt(format!("zip write: {error}")))?;
    }
    let cursor = writer
        .finish()
        .map_err(|error| IoError::corrupt(format!("zip finish: {error}")))?;
    Ok(cursor.into_inner())
}

/// Imports assets from a `.zip` (upstream import dialog).
pub fn import_zip(bytes: &[u8]) -> IoResult<Vec<DataAsset>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| IoError::corrupt(format!("invalid asset zip: {error}")))?;
    let mut assets = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| IoError::corrupt(format!("invalid zip entry: {error}")))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|error| IoError::corrupt(format!("reading zip entry: {error}")))?;
        if let Some(asset) = asset_from_zip(&name, data) {
            assets.push(asset);
        }
    }
    Ok(assets)
}

/// Builds a [`DataAsset`] from one zip path + payload, or `None` if the folder
/// is not a known `DataAssetType` folder.
fn asset_from_zip(path: &str, data: Vec<u8>) -> Option<DataAsset> {
    let (folder, relative) = path.split_once('/')?;
    let type_ = DataAssetType::ALL
        .into_iter()
        .find(|type_| type_.folder() == folder)?;
    match type_ {
        DataAssetType::Patch => {
            let json = String::from_utf8(data).ok()?;
            Some(DataAsset::patch(relative.to_owned(), json))
        }
        DataAssetType::Content => {
            let (content_folder, file_name) = relative.split_once('/')?;
            let content_type = content_type_from_folder(content_folder)?;
            let json = String::from_utf8(data).ok()?;
            Some(DataAsset::content(file_name.to_owned(), content_type, json))
        }
        DataAssetType::Bundle => {
            let text = String::from_utf8(data).ok()?;
            Some(DataAsset::bundle(relative.to_owned(), &text))
        }
        DataAssetType::Image | DataAssetType::Sound | DataAssetType::Music => {
            Some(DataAsset::blob(relative.to_owned(), type_, data, false))
        }
    }
}

/// `ContentType` from a `content/<folder>` name.
pub fn content_type_from_folder(folder: &str) -> Option<ContentType> {
    ContentType::ALL
        .into_iter()
        .find(|type_| type_.folder() == folder)
}

/// The asset list shape returned by `MindEditor.assets()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    /// Display name.
    pub name: String,
    /// Relative path.
    pub path: String,
    /// Type folder.
    pub type_folder: String,
    /// sha256 hex of the payload.
    pub hash: String,
    /// Whether the asset is always embedded.
    pub always_embedded: bool,
}

/// Converts one asset to the editor JSON record.
pub fn asset_record(asset: &DataAsset) -> AssetRecord {
    AssetRecord {
        name: asset.name.clone(),
        path: asset.path.clone(),
        type_folder: asset.type_.folder().to_owned(),
        hash: asset
            .string_hash
            .clone()
            .or_else(|| asset.byte_hash.as_ref().map(crate::mods::assets::hex32))
            .unwrap_or_default(),
        always_embedded: asset.type_.embedded(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use indexmap::IndexMap;

    fn sample_assets() -> Vec<DataAsset> {
        let mut bundle = IndexMap::new();
        bundle.insert("foo.name".to_owned(), "Foo".to_owned());
        vec![
            DataAsset::patch("patch-one.json", r#"{"patch":true}"#),
            DataAsset::bundle("bundle.properties", "foo.name=Foo\n"),
            DataAsset::blob("icon.png", DataAssetType::Image, vec![1, 2, 3, 4], false),
        ]
    }

    #[test]
    fn zip_export_import_round_trip() {
        let assets = sample_assets();
        let zip = export_zip(&assets).expect("export");
        assert!(zip.starts_with(b"PK"));

        let imported = import_zip(&zip).expect("import");
        assert_eq!(imported.len(), assets.len());
        // Compare type + payload (path reconstruction is normalized).
        for original in &assets {
            let found = imported
                .iter()
                .find(|candidate| candidate.type_ == original.type_)
                .expect("type present");
            assert_eq!(asset_bytes(found), asset_bytes(original));
        }
    }

    #[test]
    fn import_maps_content_type_folder() {
        let zip_bytes = {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file(
                    "content/blocks/test-block.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(br#"{"name":"test-block"}"#).unwrap();
            writer.finish().unwrap().into_inner()
        };
        let imported = import_zip(&zip_bytes).unwrap();
        assert_eq!(imported.len(), 1);
        let DataAssetData::Content(record) = &imported[0].data else {
            panic!("expected content");
        };
        assert_eq!(record.type_, ContentType::Block);
        assert_eq!(content_type_from_folder("units"), Some(ContentType::Unit));
    }

    #[test]
    fn records_expose_hash_and_embedding() {
        let _ = test_registry();
        let assets = sample_assets();
        let record = asset_record(&assets[0]);
        assert_eq!(record.type_folder, "patches");
        assert!(record.always_embedded);
        assert!(!record.hash.is_empty());
    }
}
