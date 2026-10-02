// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The append-only save-version chain (plan 04 §3.2/§3.3).
//!
//! Ported from `core/src/mindustry/io/SaveVersion.java` and the
//! `SaveIO.versionArray` registry. The chain is append-only forever: adding a
//! native version means adding `versions/vN.rs`, appending to
//! [`version_array`] and overriding only the region hooks that changed; old
//! entries are never renumbered or removed (`io/AGENTS.md`).

use std::sync::{Arc, OnceLock};

use super::super::{IoError, StringMap};
use super::chunk::{SaveReader, SaveWriter};
use super::meta::SaveMeta;
use super::options::SaveOptions;
use super::state::{CustomChunk, EntitySource, MapSource, MarkersIo, PatchSetIo, SaveReadState};
use super::versions::v1::SaveV1;
use crate::content::ContentRegistry;

/// One native save format version (`SaveVersion`).
///
/// Only the region hooks that differ between versions are overridden; the
/// newest entry in the chain is the writer.
pub trait SaveVersion: Send + Sync {
    /// The format version number written after the magic.
    fn version(&self) -> u32;

    /// Writes all regions in canonical order.
    fn write(&self, w: &mut SaveWriter, options: &SaveOptions) -> Result<(), IoError>;

    /// Reads regions in stream order, tolerating unknown names and missing
    /// trailing regions (plan 04 §3.3).
    fn read(&self, r: &mut SaveReader<'_>, state: &mut SaveReadState) -> Result<(), IoError>;

    /// Reads only the header + `meta` region (`SaveVersion.getMeta`); used for
    /// save/map listing and `is_save_valid`.
    fn get_meta(&self, r: &mut SaveReader<'_>) -> Result<SaveMeta, IoError>;
}

/// Data source for one save write (the upstream `Vars` globals made explicit).
///
/// Grows with the milestones: M0 writes `meta` from `tags` with an empty
/// world; M4 fills `content`/`map`/`entities`; plans 12/20 attach markers,
/// patches and custom chunks.
pub struct WriteContext<'a> {
    /// Fully computed meta tags (the standard key set of plan 04 §6.2);
    /// merged over `SaveOptions.extra_tags`.
    pub tags: StringMap,
    /// Content registry for the `content` header; `None` writes an empty
    /// header (M0 empty world).
    pub content: Option<&'a ContentRegistry>,
    /// Tile data for the `map` region; `None` writes a `0×0` map.
    pub map: Option<&'a dyn MapSource>,
    /// Entity data for the `entities` region; `None` writes empty sections.
    pub entities: Option<&'a dyn EntitySource>,
    /// Map markers (plan 12); `None` writes an empty `markers` region.
    pub markers: Option<&'a dyn MarkersIo>,
    /// Data patches (plan 20); `None` writes an empty `patches` region.
    pub patches: Option<&'a dyn PatchSetIo>,
    /// Mod custom chunks (plan 20); `None` writes an empty `custom` region.
    pub custom_chunks: Option<&'a indexmap::IndexMap<String, std::sync::Arc<dyn CustomChunk>>>,
}

impl WriteContext<'_> {
    /// A meta-only context (M0 empty world).
    pub fn meta_only(tags: StringMap) -> Self {
        Self {
            tags,
            content: None,
            map: None,
            entities: None,
            markers: None,
            patches: None,
            custom_chunks: None,
        }
    }
}

/// The append-only native chain (`SaveIO.versionArray`). The last entry writes.
static VERSIONS: OnceLock<Vec<Arc<dyn SaveVersion>>> = OnceLock::new();

/// All native versions in order.
pub fn version_array() -> &'static [Arc<dyn SaveVersion>] {
    VERSIONS.get_or_init(|| vec![Arc::new(SaveV1)]).as_slice()
}

/// The current writer (`SaveIO.getSaveWriter()`): the last chain entry.
pub fn current_writer() -> Arc<dyn SaveVersion> {
    // The chain is statically initialized with at least v1 and only ever
    // appended to, so `last` is never `None` (append-only invariant).
    version_array()[version_array().len() - 1].clone()
}

/// The current native format version (`SaveIO.getVersion()`).
pub fn current_version() -> u32 {
    current_writer().version()
}

/// Resolves a reader/writer by version (`SaveIO.getSaveWriter(int)`).
pub fn get_writer(version: u32) -> Option<Arc<dyn SaveVersion>> {
    version_array()
        .iter()
        .find(|entry| entry.version() == version)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_chain_is_append_only_native_v1() {
        let array = version_array();
        assert_eq!(array.len(), 1);
        assert_eq!(array[0].version(), 1);
        assert_eq!(current_version(), 1);
        assert!(get_writer(1).is_some());
        assert!(get_writer(13).is_none());
        // Same registry instance every call.
        assert!(std::ptr::eq(version_array(), version_array()));
    }
}
