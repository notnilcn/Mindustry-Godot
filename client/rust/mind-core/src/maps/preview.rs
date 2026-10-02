// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map preview paths and cache files (plan 06 §3.9, deviation R8).
//!
//! Ported from `core/src/mindustry/maps/Map.java` (`previewFile`/`cacheFile`)
//! and `Maps.java` (`writeCache`/`readCache`). Pixel/PNG generation stays in
//! plan 04/19; this module owns only the paths, the queue, and the cache file
//! read/write (plan 06 §2.3.7).

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::io::fs::{FileSystem, Paths};
use crate::io::wire::{WireReader, WireWriter};
use crate::io::{IoError, IoResult};

use super::Map;

/// Cache file format version (`version = 0`).
pub const CACHE_VERSION: u8 = 0;

/// The decoded preview cache (`Map.teams` + `Map.spawns`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewCache {
    /// Enemy spawn-overlay count.
    pub spawns: i32,
    /// Teams with a core building.
    pub teams: BTreeSet<u8>,
}

/// `<previews>/<name>_v2.png` (workshop maps use the parent folder name).
pub fn preview_file(paths: &Paths, map: &Map) -> PathBuf {
    paths
        .previews()
        .join(format!("{}_v2.png", preview_stem(map)))
}

/// `<previews>/<name>-cache_v2.dat` (workshop: `<parent>-workshop-cache.dat`).
pub fn cache_file(paths: &Paths, map: &Map) -> PathBuf {
    if map.workshop {
        let parent = map
            .file
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| map.name().to_owned());
        paths
            .previews()
            .join(format!("{parent}-workshop-cache.dat"))
    } else {
        paths
            .previews()
            .join(format!("{}-cache_v2.dat", preview_stem(map)))
    }
}

/// The name used for preview/cache files (`file.nameWithoutExtension`).
fn preview_stem(map: &Map) -> String {
    if map.workshop {
        map.file
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| map.name().to_owned())
    } else {
        map.file_stem().unwrap_or_else(|| map.name().to_owned())
    }
}

/// `Maps.writeCache`: serialize the cache payload.
pub fn encode_cache(cache: &PreviewCache) -> IoResult<Vec<u8>> {
    let mut bytes = Vec::with_capacity(6 + cache.teams.len());
    let mut writer = WireWriter::new(&mut bytes);
    writer.ub(CACHE_VERSION);
    writer.i(cache.spawns);
    writer.b(cache.teams.len() as i8);
    for team in &cache.teams {
        writer.ub(*team);
    }
    Ok(bytes)
}

/// `Maps.readCache`: parse the cache payload.
pub fn decode_cache(bytes: &[u8]) -> IoResult<PreviewCache> {
    let mut reader = WireReader::new(bytes);
    let _version = reader.ub()?;
    let spawns = reader.i()?;
    let count = reader.b()?;
    if count < 0 {
        return Err(IoError::corrupt("negative preview team count"));
    }
    let mut teams = BTreeSet::new();
    for _ in 0..count {
        teams.insert(reader.ub()?);
    }
    Ok(PreviewCache { spawns, teams })
}

/// Writes the preview cache for a map (`Maps.writeCache`).
pub fn write_cache(fs: &dyn FileSystem, paths: &Paths, map: &Map) -> IoResult<()> {
    let bytes = encode_cache(&PreviewCache {
        spawns: map.spawns as i32,
        teams: map.teams.clone(),
    })?;
    fs.write(&cache_file(paths, map), &bytes)
}

/// Reads and applies the preview cache for a map (`Maps.readCache`).
///
/// On any error the map keeps its defaults and the caller must re-queue a
/// preview (`Maps.loadPreviews`).
pub fn read_cache(fs: &dyn FileSystem, paths: &Paths, map: &mut Map) -> IoResult<()> {
    let bytes = fs.read(&cache_file(paths, map))?;
    let cache = decode_cache(&bytes)?;
    map.spawns = cache.spawns.max(0) as u32;
    map.teams = cache.teams;
    Ok(())
}

/// Queue element: a map whose preview must be (re)generated (plan 19 renders).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewQueue {
    /// Queued map file paths (deduplicated, insertion order).
    entries: Vec<PathBuf>,
}

impl PreviewQueue {
    /// Empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Maps.queueNewPreview`: add a map (deduplicated).
    pub fn queue(&mut self, map: &Map) {
        if !self.entries.contains(&map.file) {
            self.entries.push(map.file.clone());
        }
    }

    /// Queued file paths.
    pub fn entries(&self) -> &[PathBuf] {
        &self.entries
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `createAllPreviews`: drain the queue (plan 19 generates the pixels).
    pub fn take(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::fs::MockFs;

    #[test]
    fn preview_cache_roundtrip() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut map = Map::new(
            PathBuf::from("/data/maps/fork.msav"),
            16,
            16,
            indexmap::IndexMap::new(),
            true,
            1,
            -1,
        );
        map.spawns = 7;
        map.teams.insert(1);
        map.teams.insert(2);

        assert_eq!(
            preview_file(&paths, &map),
            PathBuf::from("/data/previews/fork_v2.png")
        );
        assert_eq!(
            cache_file(&paths, &map),
            PathBuf::from("/data/previews/fork-cache_v2.dat")
        );

        write_cache(&fs, &paths, &map).unwrap();
        let mut loaded = Map::new(
            PathBuf::from("/data/maps/fork.msav"),
            16,
            16,
            indexmap::IndexMap::new(),
            true,
            1,
            -1,
        );
        read_cache(&fs, &paths, &mut loaded).unwrap();
        assert_eq!(loaded.spawns, 7);
        assert_eq!(loaded.teams, [1u8, 2u8].into_iter().collect());

        // A corrupt cache is an error the caller re-queues on.
        fs.write(&cache_file(&paths, &map), b"\x00").unwrap();
        assert!(read_cache(&fs, &paths, &mut loaded).is_err());
    }
}
