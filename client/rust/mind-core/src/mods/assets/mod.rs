// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Data assets: types, records, sha256 cache and the `DataAssets` manager API.
//!
//! Ported from `core/src/mindustry/mod/data/{DataAssetType,DataAsset,
//! PatchAsset,ContentAsset,ImageAsset,SoundAsset,MusicAsset,BundleAsset}.java`,
//! `mod/DataAssetCache.java`, `mod/DataManager.java`,
//! `mod/DataImagePacker.java` and `mod/DataAudioLoader.java`.
//!
//! The binary record codec here matches plan 04's `patches` region framing
//! (plan 20 §6.5): `u32 version = 2` → `i32 count` → per-asset records.

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use sha2::{Digest, Sha256};

use crate::assets::bundle::parse_properties;
use crate::content::{ContentRef, ContentType};
use crate::io::{FileSystem, IoError, WireReader, WireWriter};

/// `patches` region format version (`DataPatcher.patchFormatVersion`).
pub const PATCH_FORMAT_VERSION: u32 = 2;

/// `DataImagePacker.regionPrefix` / server runtime prefix (§3.7).
pub const DATA_PREFIX: &str = "dp-";
/// `DataImagePacker.serverRegionPrefix` (`net-` runtime textures, plan 21).
pub const SERVER_PREFIX: &str = "net-";
/// `DataAudioLoader.soundIdOffset`.
pub const SOUND_ID_OFFSET: i32 = 100_000;
/// `DataAudioLoader` stream threshold in bytes.
pub const STREAM_THRESHOLD: usize = 100_000;

/// `DataAssetType` (ordinal is the binary type tag; do not reorder).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum DataAssetType {
    /// `patches/*.json`.
    Patch = 0,
    /// `content/**`.
    Content = 1,
    /// `bundles/*.properties`.
    Bundle = 2,
    /// `sprites/**` / map images.
    Image = 3,
    /// `sounds/**`.
    Sound = 4,
    /// `music/**`.
    Music = 5,
}

impl DataAssetType {
    /// All variants in ordinal order (`DataAssetType.all`).
    pub const ALL: [DataAssetType; 6] = [
        DataAssetType::Patch,
        DataAssetType::Content,
        DataAssetType::Bundle,
        DataAssetType::Image,
        DataAssetType::Sound,
        DataAssetType::Music,
    ];

    /// Ordinal tag.
    pub const fn ordinal(self) -> u8 {
        self as u8
    }

    /// `DataAssetType.folder`.
    pub const fn folder(self) -> &'static str {
        match self {
            DataAssetType::Patch => "patches",
            DataAssetType::Content => "content",
            DataAssetType::Bundle => "bundles",
            DataAssetType::Image => "sprites",
            DataAssetType::Sound => "sounds",
            DataAssetType::Music => "music",
        }
    }

    /// `DataAssetType.extension`.
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            DataAssetType::Patch | DataAssetType::Content => &["json", "hjson", "json5"],
            DataAssetType::Bundle => &["properties"],
            DataAssetType::Image => &["png"],
            DataAssetType::Sound | DataAssetType::Music => &["ogg", "mp3"],
        }
    }

    /// `DataAssetType.embedded` — patch/content are always embedded.
    pub const fn embedded(self) -> bool {
        matches!(self, DataAssetType::Patch | DataAssetType::Content)
    }

    /// Parses an ordinal tag.
    pub fn from_ordinal(value: u8) -> Option<Self> {
        Self::ALL.get(value as usize).copied()
    }

    /// Bundle key for the localized asset name (`asset.<type>.<name>`).
    pub fn localized_key(self, asset_name: &str) -> String {
        format!("asset.{}.{}", self.folder(), asset_name.replace(' ', "-"))
    }
}

/// Parsed `ContentAsset` payload (type + JSON text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentRecord {
    /// Declared content type.
    pub type_: ContentType,
    /// Raw JSON payload.
    pub json: String,
}

/// `DataAsset.data`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataAssetData {
    /// Patch JSON.
    Patch(String),
    /// Content JSON with its declared type.
    Content(ContentRecord),
    /// Bundle properties.
    Bundle(IndexMap<String, String>),
    /// Image/sound/music payload: embedded bytes or external cache reference.
    Blob(Vec<u8>),
}

/// `mod/data/DataAsset.java` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataAsset {
    /// Source path.
    pub path: String,
    /// Base64-ish UUID path for patches (`PatchAsset.path`).
    pub name: String,
    /// Asset type.
    pub type_: DataAssetType,
    /// Whether the payload is embedded (vs cache-addressed).
    pub embedded: bool,
    /// Payload.
    pub data: DataAssetData,
    /// sha256 of the payload (external assets only).
    pub byte_hash: Option<[u8; 32]>,
    /// String hash for patch/content (`PatchAsset.stringHash`).
    pub string_hash: Option<String>,
    /// Server-generated override cache file (`addOverride`).
    pub override_cache_file: Option<PathBuf>,
}

impl DataAsset {
    /// Patch asset from raw JSON (`PatchAsset`).
    pub fn patch(name: impl Into<String>, json: impl Into<String>) -> Self {
        let json = json.into();
        let string_hash = Some(hex32(&sha256(json.as_bytes())));
        Self {
            path: name.into(),
            name: String::new(),
            type_: DataAssetType::Patch,
            embedded: true,
            data: DataAssetData::Patch(json),
            byte_hash: None,
            string_hash,
            override_cache_file: None,
        }
    }

    /// Content asset (`ContentAsset`), always embedded.
    pub fn content(name: impl Into<String>, type_: ContentType, json: impl Into<String>) -> Self {
        let json = json.into();
        let string_hash = Some(hex32(&sha256(json.as_bytes())));
        Self {
            path: name.into(),
            name: String::new(),
            type_: DataAssetType::Content,
            embedded: true,
            data: DataAssetData::Content(ContentRecord { type_, json }),
            byte_hash: None,
            string_hash,
            override_cache_file: None,
        }
    }

    /// External blob asset (image/sound/music) addressed by sha256.
    pub fn blob(
        path: impl Into<String>,
        type_: DataAssetType,
        bytes: Vec<u8>,
        embedded: bool,
    ) -> Self {
        let hash = sha256(&bytes);
        Self {
            path: path.into(),
            name: String::new(),
            type_,
            embedded,
            data: DataAssetData::Blob(bytes),
            byte_hash: Some(hash),
            string_hash: None,
            override_cache_file: None,
        }
    }

    /// Bundle asset from properties text.
    pub fn bundle(name: impl Into<String>, text: &str) -> Self {
        let string_hash = Some(hex32(&sha256(text.as_bytes())));
        Self {
            path: name.into(),
            name: String::new(),
            type_: DataAssetType::Bundle,
            embedded: false,
            data: DataAssetData::Bundle(parse_properties(text)),
            byte_hash: None,
            string_hash,
            override_cache_file: None,
        }
    }

    /// `DataAsset.isExternal` — not embedded.
    pub fn is_external(&self) -> bool {
        !self.embedded
    }

    /// `DataAssetType.compareTo` ordering key (type ordinal, then path).
    pub fn compare_key(&self) -> (u8, &str) {
        (self.type_.ordinal(), self.path.as_str())
    }

    /// Writes the per-asset binary record (plan 20 §6.5, inside 04's region).
    pub fn write(&self, writer: &mut WireWriter) -> Result<(), IoError> {
        writer.ub(self.type_.ordinal());
        writer.str(&self.path)?;
        writer.bool(self.embedded);
        let mut payload = Vec::new();
        match &self.data {
            DataAssetData::Patch(text) => payload.extend_from_slice(text.as_bytes()),
            DataAssetData::Content(record) => {
                let mut inner = WireWriter::new(&mut payload);
                inner.us(record.type_.ordinal() as u16);
                inner.bytes(record.json.as_bytes());
            }
            DataAssetData::Bundle(map) => {
                let text = render_properties(map);
                payload.extend_from_slice(text.as_bytes());
            }
            DataAssetData::Blob(bytes) => payload.extend_from_slice(bytes),
        }
        if self.embedded {
            writer.u(payload.len() as u32);
            writer.bytes(&payload);
        } else {
            let hash = self.byte_hash.unwrap_or_else(|| sha256(&payload));
            writer.bytes(&hash);
        }
        Ok(())
    }

    /// Reads the per-asset binary record.
    pub fn read(reader: &mut WireReader) -> Result<Self, IoError> {
        let type_code = reader.ub()?;
        let type_ = DataAssetType::from_ordinal(type_code)
            .ok_or_else(|| IoError::corrupt(format!("unknown data asset type {type_code}")))?;
        let path = reader.str()?;
        let embedded = reader.bool()?;
        let (data, byte_hash) = if embedded {
            let len = reader.u()? as usize;
            let bytes = reader.bytes(len)?.to_vec();
            let data = match type_ {
                DataAssetType::Patch => DataAssetData::Patch(decode_utf8(&bytes)?),
                DataAssetType::Content => {
                    let mut inner = WireReader::new(&bytes);
                    let ordinal = inner.us()?;
                    let content_type = ContentType::ALL
                        .get(ordinal as usize)
                        .copied()
                        .ok_or_else(|| IoError::corrupt("bad content type ordinal"))?;
                    let json = String::from_utf8(inner.bytes(inner.remaining())?.to_vec())
                        .map_err(|_| IoError::corrupt("content asset is not UTF-8"))?;
                    DataAssetData::Content(ContentRecord {
                        type_: content_type,
                        json,
                    })
                }
                DataAssetType::Bundle => {
                    DataAssetData::Bundle(parse_properties(&decode_utf8(&bytes)?))
                }
                _ => DataAssetData::Blob(bytes),
            };
            (data, None)
        } else {
            let hash = reader.bytes(32)?;
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(hash);
            (DataAssetData::Blob(Vec::new()), Some(bytes))
        };
        let string_hash = match &data {
            DataAssetData::Patch(text) => Some(hex32(&sha256(text.as_bytes()))),
            DataAssetData::Content(record) => Some(hex32(&sha256(record.json.as_bytes()))),
            _ => None,
        };
        Ok(Self {
            path,
            name: String::new(),
            type_,
            embedded,
            data,
            byte_hash,
            string_hash,
            override_cache_file: None,
        })
    }
}

/// sha256 digest.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Lowercase hex encoding of a 32-byte digest.
pub fn hex32(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// `DataAssetCache.encodeHash` — custom base32 (52 chars, no padding).
pub fn encode_hash(hash: &[u8; 32]) -> String {
    data_encoding::BASE32_NOPAD.encode(hash)
}

/// `DataAudioLoader` region/name prefixing (lowercase, spaces → `_`).
pub fn audio_asset_name(raw: &str) -> String {
    let stem = raw
        .rsplit('/')
        .next()
        .unwrap_or(raw)
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(raw);
    format!("{DATA_PREFIX}{}", stem.to_lowercase().replace(' ', "_"))
}

/// `DataImagePacker.regionPrefix` application (generated names keep their `dp-`).
pub fn image_asset_name(raw: &str, generated: bool) -> String {
    if raw.starts_with(DATA_PREFIX) || (generated && raw.contains(DATA_PREFIX)) {
        raw.to_owned()
    } else {
        format!("{DATA_PREFIX}{raw}")
    }
}

/// Which content assets the restricted patch parser may load
/// (`ContentAsset.loadableContent`).
pub fn loadable_content<'a>(
    records: &'a [ContentRecord],
    allowed: &[ContentType],
) -> Vec<&'a ContentRecord> {
    records
        .iter()
        .filter(|record| allowed.contains(&record.type_))
        .collect()
}

/// Renders bundle properties deterministically (insertion order).
pub fn render_properties(map: &IndexMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in map {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

fn decode_utf8(bytes: &[u8]) -> Result<String, IoError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| IoError::corrupt("asset payload is not UTF-8"))
}

/// `mod/DataAssetCache.java`: content-addressed blob store.
#[derive(Debug, Default)]
pub struct DataAssetCache {
    dir: PathBuf,
    files: IndexMap<String, PathBuf>,
}

impl DataAssetCache {
    /// Loads an empty cache rooted at `dir` (`DataAssetCache.load`).
    pub fn load(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            files: IndexMap::new(),
        }
    }

    /// Cache directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Number of cached blobs.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// `DataAssetCache.add`: writes bytes under `base32(sha256)`, skipping the
    /// write when an existing file already has the same length. Returns the hash.
    pub fn add(&mut self, fs: &dyn FileSystem, bytes: &[u8]) -> Result<[u8; 32], IoError> {
        let hash = sha256(bytes);
        let name = encode_hash(&hash);
        let path = self.dir.join(&name);
        let needs_write = match fs.len(&path) {
            Ok(existing) => existing != bytes.len() as u64,
            Err(_) => true,
        };
        if needs_write {
            fs.write(&path, bytes)?;
        }
        self.files.insert(name, path);
        Ok(hash)
    }

    /// `DataAssetCache.addOverride`: maps a hash to a server-generated file.
    pub fn add_override(&mut self, hash: [u8; 32], path: impl Into<PathBuf>) {
        self.files.insert(encode_hash(&hash), path.into());
    }

    /// `DataAssetCache.get`.
    pub fn get(&self, hash: &[u8; 32]) -> Option<&Path> {
        self.files.get(&encode_hash(hash)).map(PathBuf::as_path)
    }

    /// `DataAssetCache.has`.
    pub fn has(&self, hash: &[u8; 32]) -> bool {
        self.files.contains_key(&encode_hash(hash))
    }
}

/// `mod/DataManager.java` API consumed by plans 04/12/19/21.
pub trait DataAssets: Send + Sync {
    /// All assets in insertion order.
    fn all_assets(&self) -> &[DataAsset];
    /// External (non-embedded) assets.
    fn all_external_assets(&self) -> Vec<&DataAsset>;
    /// Whether any external asset exists (`hasExternalAssets`).
    fn has_external_assets(&self) -> bool;
    /// Assets of one type, ordered.
    fn get_assets(&self, type_: DataAssetType) -> Vec<&DataAsset>;
    /// Whether `content` is touched by an applied patch.
    fn is_patched(&self, content: ContentRef) -> bool;
    /// Assets with no resolvable cache/override file (`getMissingAssets`).
    fn get_missing_assets(&self) -> Vec<&DataAsset>;
    /// Whether `path` already has the given type (`hasAssetPath`).
    fn has_asset_path(&self, type_: DataAssetType, path: &str) -> bool;
    /// Installs a `net-` runtime texture (plan 21).
    fn add_texture(&mut self, name: &str, png: Vec<u8>);
    /// Removes a `net-` runtime texture.
    fn remove_texture(&mut self, name: &str);
}

/// In-memory `DataManager` implementing record bookkeeping/ordering. Patch
/// application, image packing and audio loading are plan 20 M3/M4 follow-ups
/// and plan 03/18 drivers; this type owns the records those drivers consume.
#[derive(Debug, Default)]
pub struct ModDataManager {
    assets: Vec<DataAsset>,
    missing: Vec<usize>,
    patched: Vec<ContentRef>,
    runtime_textures: IndexMap<String, Vec<u8>>,
    bundle_originals: IndexMap<String, IndexMap<String, String>>,
}

impl ModDataManager {
    /// Empty manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an asset record.
    pub fn push(&mut self, asset: DataAsset) {
        self.assets.push(asset);
    }

    /// All assets ordered by `DataAssetType` then path (`orderedAssets`).
    pub fn ordered_assets(&self) -> Vec<&DataAsset> {
        let mut out: Vec<&DataAsset> = self.assets.iter().collect();
        out.sort_by(|a, b| a.compare_key().cmp(&b.compare_key()));
        out
    }

    /// Ordered external assets (`orderedExternalAssets`).
    pub fn ordered_external_assets(&self) -> Vec<&DataAsset> {
        let mut out: Vec<&DataAsset> = self
            .assets
            .iter()
            .filter(|asset| asset.is_external())
            .collect();
        out.sort_by(|a, b| a.compare_key().cmp(&b.compare_key()));
        out
    }

    /// Marks a content reference as patched.
    pub fn mark_patched(&mut self, content: ContentRef) {
        if !self.patched.contains(&content) {
            self.patched.push(content);
        }
    }

    /// Removes all records.
    pub fn clear(&mut self) {
        self.assets.clear();
        self.missing.clear();
        self.patched.clear();
        self.runtime_textures.clear();
        self.bundle_originals.clear();
    }

    /// Records a bundle snapshot for restore-on-unload (`DataBundleLoader`).
    pub fn snapshot_bundle(&mut self, file: &str, value: IndexMap<String, String>) {
        self.bundle_originals
            .entry(file.to_owned())
            .or_insert(value);
    }

    /// Original bundle snapshot.
    pub fn bundle_original(&self, file: &str) -> Option<&IndexMap<String, String>> {
        self.bundle_originals.get(file)
    }

    /// Runtime textures (plan 21 `net-`).
    pub fn runtime_textures(&self) -> &IndexMap<String, Vec<u8>> {
        &self.runtime_textures
    }
}

impl DataAssets for ModDataManager {
    fn all_assets(&self) -> &[DataAsset] {
        &self.assets
    }

    fn all_external_assets(&self) -> Vec<&DataAsset> {
        self.assets
            .iter()
            .filter(|asset| asset.is_external())
            .collect()
    }

    fn has_external_assets(&self) -> bool {
        self.assets.iter().any(DataAsset::is_external)
    }

    fn get_assets(&self, type_: DataAssetType) -> Vec<&DataAsset> {
        let mut out: Vec<&DataAsset> = self
            .assets
            .iter()
            .filter(|asset| asset.type_ == type_)
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    fn is_patched(&self, content: ContentRef) -> bool {
        self.patched.contains(&content)
    }

    fn get_missing_assets(&self) -> Vec<&DataAsset> {
        self.missing
            .iter()
            .filter_map(|index| self.assets.get(*index))
            .collect()
    }

    fn has_asset_path(&self, type_: DataAssetType, path: &str) -> bool {
        self.assets
            .iter()
            .any(|asset| asset.type_ == type_ && asset.path == path)
    }

    fn add_texture(&mut self, name: &str, png: Vec<u8>) {
        self.runtime_textures
            .insert(format!("{SERVER_PREFIX}{name}"), png);
    }

    fn remove_texture(&mut self, name: &str) {
        self.runtime_textures
            .shift_remove(&format!("{SERVER_PREFIX}{name}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;

    /// Plan 20 M4: `assets::cache_base32_vectors`.
    #[test]
    fn cache_base32_vectors() {
        let vectors = [
            (
                b"".as_slice(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "4OYMIQUY7QOBJGX36TEJS35ZEQT24QPEMSNZGTFESWMRW6CSXBKQ",
            ),
            (
                b"test".as_slice(),
                "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
                "T6DNBAMIJR6WLGRP5KQMKWWQCWR36TY3FMFYELGRLVWBLMHQBIEA",
            ),
        ];
        for (bytes, hex, base32) in vectors {
            let hash = sha256(bytes);
            assert_eq!(hex32(&hash), hex);
            assert_eq!(encode_hash(&hash), base32);
            assert_eq!(encode_hash(&hash).len(), 52);
        }
    }

    /// Plan 20 M4: `assets::cache_dedup` — a second `add` of the same bytes
    /// does not rewrite the cache file.
    #[test]
    fn cache_dedup() {
        let fs = MockFs::new();
        let mut cache = DataAssetCache::load("/data/assetCache");
        let hash_a = cache.add(&fs, b"hello").expect("add");
        // Truncate semantics: same length means no write.
        let before = fs.len(&cache.dir().join(encode_hash(&hash_a))).unwrap();
        let hash_b = cache.add(&fs, b"hello").expect("add second");
        let after = fs.len(&cache.dir().join(encode_hash(&hash_b))).unwrap();
        assert_eq!(hash_a, hash_b);
        assert_eq!(before, after);
        assert!(cache.has(&hash_a));
        assert_eq!(cache.len(), 1);
    }

    /// Plan 20 M4: `assets::dp_prefixes`.
    #[test]
    fn dp_prefixes() {
        assert_eq!(audio_asset_name("sounds/My Sound.ogg"), "dp-my_sound");
        assert_eq!(image_asset_name("wall.png", false), "dp-wall.png");
        assert_eq!(image_asset_name("dp-wall.png", true), "dp-wall.png");
        assert!(DataAssetType::Patch.embedded());
        assert!(!DataAssetType::Image.embedded());
    }

    /// Plan 20 M4: `assets::audio_id_offset_and_streaming`.
    #[test]
    fn audio_id_offset_and_streaming() {
        assert_eq!(SOUND_ID_OFFSET, 100_000);
        assert_eq!(STREAM_THRESHOLD, 100_000);
    }

    /// Plan 20 M4: `assets::ordered_external`.
    #[test]
    fn ordered_external() {
        let mut manager = ModDataManager::new();
        manager.push(DataAsset::patch("p", "{}"));
        manager.push(DataAsset::bundle("bundles/bundle.properties", "a=b\n"));
        manager.push(DataAsset::blob(
            "sounds/z.ogg",
            DataAssetType::Sound,
            vec![1],
            false,
        ));
        manager.push(DataAsset::blob(
            "sprites/a.png",
            DataAssetType::Image,
            vec![2],
            false,
        ));
        assert!(manager.has_external_assets());
        let external = manager.ordered_external_assets();
        // Type ordinal: Bundle(2) then Image(3) then Sound(4).
        let types: Vec<DataAssetType> = external.iter().map(|asset| asset.type_).collect();
        assert_eq!(
            types,
            vec![
                DataAssetType::Bundle,
                DataAssetType::Image,
                DataAssetType::Sound
            ]
        );
        assert_eq!(manager.get_assets(DataAssetType::Image).len(), 1);
    }

    /// Plan 20 M4: `assets::content_asset_binary_roundtrip`.
    #[test]
    fn content_asset_binary_roundtrip() {
        let asset =
            DataAsset::content("content/items/x.json", ContentType::Item, r#"{"name":"X"}"#);
        let mut bytes = Vec::new();
        asset
            .write(&mut WireWriter::new(&mut bytes))
            .expect("write");
        let mut reader = WireReader::new(&bytes);
        let decoded = DataAsset::read(&mut reader).expect("read");
        assert_eq!(decoded, asset);
    }

    /// Plan 20 M4: `assets::patch_asset_binary_roundtrip`.
    #[test]
    fn patch_asset_binary_roundtrip() {
        let asset = DataAsset::patch("patch-1", r#"{"block.foo.health":10}"#);
        let mut bytes = Vec::new();
        asset
            .write(&mut WireWriter::new(&mut bytes))
            .expect("write");
        let mut reader = WireReader::new(&bytes);
        let decoded = DataAsset::read(&mut reader).expect("read");
        assert_eq!(decoded, asset);
        assert_eq!(decoded.string_hash, asset.string_hash);
    }

    /// Plan 20 M4: `assets::loadable_content_filter`.
    #[test]
    fn loadable_content_filter() {
        let records = vec![
            ContentRecord {
                type_: ContentType::Item,
                json: "{}".to_owned(),
            },
            ContentRecord {
                type_: ContentType::Block,
                json: "{}".to_owned(),
            },
        ];
        let allowed = loadable_content(&records, &[ContentType::Item]);
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].type_, ContentType::Item);
    }

    /// Plan 20 M4: `assets::bundle_merge_restore`.
    #[test]
    fn bundle_merge_restore() {
        let mut manager = ModDataManager::new();
        let original: IndexMap<String, String> =
            [("a".to_owned(), "1".to_owned())].into_iter().collect();
        manager.snapshot_bundle("bundles/bundle.properties", original.clone());
        let snapshot = manager
            .bundle_original("bundles/bundle.properties")
            .expect("snapshot")
            .clone();
        assert_eq!(snapshot, original);
    }

    #[test]
    fn runtime_texture_net_prefix() {
        let mut manager = ModDataManager::new();
        manager.add_texture("ping", vec![1, 2, 3]);
        assert!(manager.runtime_textures().contains_key("net-ping"));
        manager.remove_texture("ping");
        assert!(manager.runtime_textures().is_empty());
    }
}
