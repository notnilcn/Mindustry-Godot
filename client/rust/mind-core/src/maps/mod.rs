// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map registry (`Maps`, plan 06 §3.9).
//!
//! Ported from `core/src/mindustry/maps/Maps.java`: the built-in/default map
//! list, custom/mod/workshop discovery through plan 04's `MapIo`, save/import/
//! remove, preview queue/cache paths and `ShuffleMode`. Pixel generation and
//! the editor live in plans 19.

pub mod error;
pub mod filters;
pub mod fix;
pub mod generators;
pub mod locales;
pub mod map;
pub mod planet;
pub mod preview;
pub mod sector_damage;
pub mod shuffle;

pub use error::{MapError, MapException};
pub use map::{ENV_SCORCHING, Map, UNKNOWN};
pub use preview::{PreviewCache, PreviewQueue, cache_file, preview_file};
pub use sector_damage::{DamageBuilding, DamageFx, NoopDamageFx, SectorDamageState};
pub use shuffle::{GameMode, MapProvider, ShuffleMode};

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bevy_ecs::prelude::Resource;
use indexmap::IndexMap;

use crate::content::ContentRegistry;
use crate::io::StringMap;
use crate::io::fs::{FileSystem, Paths, SAVE_EXTENSION};
use crate::io::map::{BlockPalette, MapIo};
use crate::io::save::WriteContext;
use crate::io::save::state::MapSource;
use crate::random::JavaRandom;
use crate::util::strings::sanitize_filename;

/// All built-in map names, filenames only (`Maps.defaultMapNames`).
pub const DEFAULT_MAP_NAMES: [&str; 18] = [
    "maze",
    "fortress",
    "labyrinth",
    "islands",
    "tendrils",
    "caldera",
    "wasteland",
    "shattered",
    "fork",
    "triad",
    "mudFlats",
    "moltenLake",
    "archipelago",
    "debrisField",
    "domain",
    "veins",
    "glacier",
    "passage",
];

/// Maps tagged as PvP (`Maps.pvpMaps`).
pub const PVP_MAPS: [&str; 3] = ["veins", "glacier", "passage"];

/// Where the registry finds maps (`Maps.load` inputs).
#[derive(Debug, Clone, Default)]
pub struct MapSources {
    /// Directory containing built-ins (`<dir>/default/<name>.msav` or `<dir>/<name>.msav`).
    pub builtin_dir: Option<PathBuf>,
    /// Custom-map directory (`Paths.maps()`); walked recursively.
    pub custom_dir: Option<PathBuf>,
    /// Whether built-ins live under `default/` (`Maps.useDefaultFolder`).
    pub use_default_folder: bool,
}

impl MapSources {
    /// Converts a [`Paths`] layout into sources (`maps/` + built-in sibling).
    pub fn from_paths(paths: &Paths) -> Self {
        Self {
            builtin_dir: None,
            custom_dir: Some(paths.maps()),
            use_default_folder: true,
        }
    }
}

/// The ordered map registry (`Maps`).
#[derive(Resource, Default)]
pub struct Maps {
    /// All maps in sorted order (`Maps.maps`).
    maps: Vec<Map>,
    /// Automatic selection strategy (`Maps.shuffleMode`).
    pub shuffle_mode: ShuffleMode,
    /// Custom provider override (`Maps.setMapProvider`).
    pub provider: Option<Box<dyn MapProvider>>,
    /// Explicit next map (`Maps.setNextMapOverride`), matched by file path.
    pub next_override: Option<PathBuf>,
    /// Maps whose previews must be regenerated (plan 19 renders).
    pub preview_queue: PreviewQueue,
}

impl Maps {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// All maps in sorted order (`Maps.all`).
    pub fn all(&self) -> &[Map] {
        &self.maps
    }

    /// Number of maps.
    pub fn len(&self) -> usize {
        self.maps.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// Custom maps (`Maps.customMaps`).
    pub fn custom_maps(&self) -> Vec<&Map> {
        self.maps.iter().filter(|map| map.custom).collect()
    }

    /// Built-in (non-mod) maps (`Maps.defaultMaps`).
    pub fn default_maps(&self) -> Vec<&Map> {
        self.maps
            .iter()
            .filter(|map| !map.custom && map.mod_id.is_none())
            .collect()
    }

    /// Mod maps (`Maps.moddedMaps`).
    pub fn modded_maps(&self) -> Vec<&Map> {
        self.maps
            .iter()
            .filter(|map| map.mod_id.is_some())
            .collect()
    }

    /// First map with a display name (`Maps.byName`).
    pub fn by_name(&self, name: &str) -> Option<&Map> {
        self.maps.iter().find(|map| map.name() == name)
    }

    /// Index of a map by file path.
    pub fn index_of_file(&self, file: &Path) -> Option<usize> {
        self.maps.iter().position(|map| map.file == file)
    }

    /// Adds a map and re-sorts (`Maps.saveMap`/`loadMap`).
    pub fn add(&mut self, map: Map) {
        self.maps.push(map);
        self.sort();
    }

    /// Sorts maps by [`Map::compare_to`] (`maps.sort`).
    pub fn sort(&mut self) {
        self.maps.sort_by(Map::compare_to);
    }

    /// Loads built-ins and custom maps (`Maps.load`).
    ///
    /// Missing built-ins are warned about and skipped (plan 03 assets are not
    /// yet present, risk R11); corrupt custom maps are skipped with a warning.
    /// Returns the number of maps loaded.
    pub fn load(&mut self, fs: &dyn FileSystem, sources: &MapSources) -> usize {
        let mut loaded = 0;

        if let Some(dir) = &sources.builtin_dir {
            for name in DEFAULT_MAP_NAMES {
                let default_path = dir.join("default").join(format!("{name}.{SAVE_EXTENSION}"));
                let flat_path = dir.join(format!("{name}.{SAVE_EXTENSION}"));
                let file = if sources.use_default_folder && fs.exists(&default_path) {
                    default_path
                } else if fs.exists(&flat_path) {
                    flat_path
                } else {
                    log::warn!("built-in map `{name}` is missing at {}", dir.display());
                    continue;
                };
                match self.load_map_file(fs, &file, false) {
                    Ok(()) => loaded += 1,
                    Err(error) => {
                        log::error!("Failed to load built-in map `{}`: {error}", file.display())
                    }
                }
            }
        }

        if let Some(dir) = &sources.custom_dir {
            loaded += self.load_from_dir(fs, dir, true);
        }

        loaded
    }

    /// Loads every `.msav` file below `dir` (`customMapDirectory.walk`).
    ///
    /// Corrupt entries are skipped with a warning (plan 06 §7b `maps list`).
    pub fn load_from_dir(&mut self, fs: &dyn FileSystem, dir: &Path, custom: bool) -> usize {
        let mut loaded = 0;
        let files = match fs.walk(dir) {
            Ok(files) => files,
            Err(error) => {
                log::warn!("failed to walk map directory {}: {error}", dir.display());
                return 0;
            }
        };
        for file in files {
            let is_map = file
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(SAVE_EXTENSION));
            if !is_map {
                continue;
            }
            match self.load_map_file(fs, &file, custom) {
                Ok(()) => loaded += 1,
                Err(error) => log::warn!(
                    "Failed to load custom map file '{}': {error}",
                    file.display()
                ),
            }
        }
        loaded
    }

    /// Loads one map file (`Maps.loadMap`); validates the name.
    pub fn load_map_file(
        &mut self,
        fs: &dyn FileSystem,
        file: &Path,
        custom: bool,
    ) -> Result<(), MapError> {
        let header = MapIo::create_map(fs, file, custom)?;
        let map = Map::from_header(&header, custom);
        map.validate_name()?;
        self.maps.push(map);
        self.sort();
        Ok(())
    }

    /// Loads an internal map without adding it (`Maps.loadInternalMap`).
    pub fn load_internal_map(&self, fs: &dyn FileSystem, name: &str) -> Result<Map, MapError> {
        let file = PathBuf::from(format!("maps/{name}.{SAVE_EXTENSION}"));
        let header = MapIo::create_map(fs, &file, false)?;
        let map = Map::from_header(&header, false);
        map.validate_name()?;
        Ok(map)
    }

    /// Reloads the registry from sources (`Maps.reload`).
    pub fn reload(&mut self, fs: &dyn FileSystem, sources: &MapSources) -> usize {
        self.maps.clear();
        self.preview_queue = PreviewQueue::new();
        self.load(fs, sources)
    }

    /// Removes a map, its file and its preview/cache files (`Maps.removeMap`).
    pub fn remove_map(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        name: &str,
    ) -> Result<(), MapError> {
        let index = self
            .maps
            .iter()
            .position(|map| map.name() == name)
            .ok_or_else(|| MapError::NotFound(name.to_owned()))?;
        let map = self.maps.remove(index);
        let _ = fs.delete(&map.file);
        let _ = fs.delete(&preview::preview_file(paths, &map));
        let _ = fs.delete(&preview::cache_file(paths, &map));
        Ok(())
    }

    /// Finds a free filename for a new map (`Maps.findFile`).
    pub fn find_file(&self, fs: &dyn FileSystem, dir: &Path, unsanitized_name: &str) -> PathBuf {
        let mut name = sanitize_filename(unsanitized_name);
        if name.is_empty() {
            name = "blank".to_owned();
        }
        let mut index = 0;
        loop {
            let suffix = if index == 0 {
                String::new()
            } else {
                format!("_{index}")
            };
            let candidate = dir.join(format!("{name}{suffix}.{SAVE_EXTENSION}"));
            if !fs.exists(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }

    /// `Maps.saveMap` registry half (plan 06 §3.9 / plan 19 M7): write a live
    /// tile source as a native `MGRS` map, fill the spawn/team preview metadata,
    /// write the cache file and re-sort.
    ///
    /// Pixel/PNG generation stays in plan 19 (`PreviewPipeline`); this method
    /// only owns the file, the registry entry and the cache file. `map_tags`
    /// win over `base_tags` (`SaveVersion.write` lets `ctx.tags` override).
    #[allow(clippy::too_many_arguments)]
    pub fn save_map(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        file: &Path,
        source: &dyn MapSource,
        content: &ContentRegistry,
        base_tags: StringMap,
        map_tags: StringMap,
        embed_assets: bool,
    ) -> Result<Map, MapError> {
        write_map_source(fs, file, source, content, base_tags, map_tags, embed_assets)?;
        let header = MapIo::create_map(fs, file, true)?;
        let mut map = Map::from_header(&header, true);
        let scan = scan_map_source(source, content);
        map.spawns = scan.spawns;
        map.teams = scan.teams;
        map.validate_name()?;
        // Cache write failures are non-fatal upstream (the preview still loads).
        if let Err(error) = preview::write_cache(fs, paths, &map) {
            log::warn!(
                "failed to write preview cache for `{}`: {error}",
                map.name()
            );
        }
        self.add(map.clone());
        Ok(map)
    }

    /// `Maps.importMap` (plan 06 §3.9 / plan 19 §3.8): reject images, sniff a
    /// free name, copy the file into `dir` and register it.
    ///
    /// Preview regeneration (pixels/PNG) is the plan-19 client half
    /// (`editor::maps_glue::import_map_e2e`).
    pub fn import_map(
        &mut self,
        fs: &dyn FileSystem,
        dir: &Path,
        source: &Path,
    ) -> Result<Map, MapError> {
        if MapIo::is_image(fs, source) {
            return Err(MapError::exception(
                source.to_path_buf(),
                "image files cannot be imported as maps",
            ));
        }
        let header = MapIo::create_map(fs, source, true)?;
        let name = if header.name.trim().is_empty() {
            "unknown".to_owned()
        } else {
            header.name.clone()
        };
        let file = self.find_file(fs, dir, &name);
        let bytes = fs.read(source)?;
        fs.write(&file, &bytes)?;
        self.load_map_file(fs, &file, true)?;
        let index = self.index_of_file(&file).ok_or(MapError::NotFound(name))?;
        self.all()
            .get(index)
            .cloned()
            .ok_or_else(|| MapError::NotFound(file.display().to_string()))
    }

    /// Selects the next map (`Maps.getNextMap`).
    ///
    /// Honours `next_override` first, then a custom provider, then the shuffle
    /// mode. `previous` is an index into [`Maps::all`].
    pub fn get_next_map(
        &mut self,
        mode: GameMode,
        previous: Option<usize>,
        rng: &mut JavaRandom,
    ) -> Option<usize> {
        if let Some(file) = self.next_override.take() {
            return self.index_of_file(&file);
        }
        if let Some(provider) = &self.provider {
            return provider.next_index(mode, previous, &self.maps, rng);
        }
        self.shuffle_mode.next(mode, previous, &self.maps, rng)
    }

    /// The built-in map names (`Maps.defaultMapNames`).
    pub fn default_map_names() -> &'static [&'static str; 18] {
        &DEFAULT_MAP_NAMES
    }

    /// Builds a map record for a file name (used by generators/tests).
    pub fn map_for_file(file: PathBuf, width: i32, height: i32, name: &str, custom: bool) -> Map {
        let mut tags = IndexMap::new();
        tags.insert("name".to_owned(), name.to_owned());
        Map::new(file, width, height, tags, custom, 1, -1)
    }
}

/// Preview metadata scanned from a live tile source (`Maps.saveMap` client half).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapScan {
    /// Enemy spawn-overlay count.
    pub spawns: u32,
    /// Teams with core buildings; empty for tile-only sources (teams live on
    /// plan-07 building entities, not on [`crate::world::Tile`]).
    pub teams: BTreeSet<u8>,
}

/// Scans core teams and spawn overlays from a live source (`Maps.saveMap`).
///
/// The tile-source shape carries no building team, so `teams` is populated only
/// when a caller supplies it; the spawn count is exact (`BlockPalette::is_spawn`).
pub fn scan_map_source(source: &dyn MapSource, content: &ContentRegistry) -> MapScan {
    let palette = BlockPalette::of(content);
    let len = source.width() as usize * source.height() as usize;
    let mut spawns = 0u32;
    for index in 0..len {
        if palette.is_spawn(source.overlay_id(index)) {
            spawns = spawns.saturating_add(1);
        }
    }
    MapScan {
        spawns,
        teams: BTreeSet::new(),
    }
}

/// Shared native-map writer: merges `map_tags` over `base_tags` and writes the
/// `map` region (plan 04 `MapIo::write_map`). Used by `Maps::save_map` and the
/// editor glue (`map_tags` win because they enter `ctx.tags`).
#[allow(clippy::too_many_arguments)]
pub fn write_map_source(
    fs: &dyn FileSystem,
    file: &Path,
    source: &dyn MapSource,
    content: &ContentRegistry,
    base_tags: StringMap,
    map_tags: StringMap,
    embed_assets: bool,
) -> Result<(), MapError> {
    let mut tags = base_tags;
    for (key, value) in &map_tags {
        tags.insert(key.clone(), value.clone());
    }
    let ctx = WriteContext {
        tags,
        content: Some(content),
        map: Some(source),
        entities: None,
        markers: None,
        patches: None,
        custom_chunks: None,
    };
    MapIo::write_map(fs, file, &ctx, map_tags, embed_assets).map_err(MapError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::fs::MockFs;
    use crate::io::map::header::MapHeader;
    use crate::io::save::fixture::FixtureWorld;
    use crate::io::save::versions::v1::base_meta_tags;
    use crate::io::save::{SaveIo, SaveOptions, WriteContext};

    fn write_fixture(
        fs: &dyn FileSystem,
        file: &Path,
        registry: &crate::content::ContentRegistry,
        name: &str,
    ) {
        let world = FixtureWorld::synthetic(registry, 16, 16);
        let mut tags = base_meta_tags(16, 16, 0, "fixture");
        tags.insert("name".to_owned(), name.to_owned());
        let mut ctx = WriteContext::meta_only(tags);
        ctx.content = Some(registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        SaveIo::save(fs, file, &ctx, &SaveOptions::new()).unwrap();
    }

    #[test]
    fn maps_list_skips_corrupt_and_sorts() {
        let fs = MockFs::new();
        let registry = test_registry();
        let dir = PathBuf::from("/data/maps");
        write_fixture(&fs, &dir.join("zulu.msav"), &registry, "Zulu");
        write_fixture(&fs, &dir.join("alpha.msav"), &registry, "Alpha");
        fs.write(&dir.join("broken.msav"), b"not a map").unwrap();
        let sub = dir.join("nested");
        write_fixture(&fs, &sub.join("beta.msav"), &registry, "Beta");

        let mut maps = Maps::new();
        let loaded = maps.load_from_dir(&fs, &dir, true);
        assert_eq!(loaded, 3, "corrupt map skipped, nested map found");
        let names: Vec<&str> = maps.all().iter().map(Map::name).collect();
        assert_eq!(names, ["Alpha", "Beta", "Zulu"]);
        assert!(maps.by_name("Alpha").is_some());
        assert!(maps.by_name("Missing").is_none());
    }

    /// `maps::tests::corrupt_map_skipped`.
    #[test]
    fn corrupt_map_skipped() {
        let fs = MockFs::new();
        let dir = PathBuf::from("/data/maps");
        fs.write(&dir.join("bad.msav"), b"MGRSgarbage").unwrap();
        let mut maps = Maps::new();
        assert_eq!(maps.load_from_dir(&fs, &dir, true), 0);
        assert!(maps.is_empty());
    }

    #[test]
    fn find_file_suffixes_and_sanitizes() {
        let fs = MockFs::new();
        let dir = PathBuf::from("/data/maps");
        let maps = Maps::new();
        let first = maps.find_file(&fs, &dir, "My Map?");
        assert_eq!(first, dir.join("My Map_.msav"));
        fs.write(&first, b"x").unwrap();
        let second = maps.find_file(&fs, &dir, "My Map?");
        assert_eq!(second, dir.join("My Map__1.msav"));
    }

    /// Writes the committed map fixtures used by `mind-headless maps list`
    /// (plan 06 §6.6/§7b). Run with `--ignored` after format changes.
    #[test]
    #[ignore = "writes committed map fixtures (`cargo test -- --ignored`)"]
    fn generate_map_fixtures() {
        let fs = crate::io::fs::NativeFs;
        let registry = test_registry();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("maps");
        std::fs::create_dir_all(&dir).unwrap();
        write_fixture(&fs, &dir.join("fork.msav"), &registry, "Fork");
        write_fixture(&fs, &dir.join("caldera.msav"), &registry, "Caldera");
        fs.write(&dir.join("broken.msav"), b"not a native map")
            .unwrap();
        fs.write(&dir.join("notes.txt"), b"ignored").unwrap();
    }

    #[test]
    fn header_roundtrip_from_registry() {
        let fs = MockFs::new();
        let registry = test_registry();
        let file = PathBuf::from("/data/maps/fork.msav");
        write_fixture(&fs, &file, &registry, "Fork");
        let header: MapHeader = MapIo::create_map(&fs, &file, true).unwrap();
        let map = Map::from_header(&header, true);
        assert!(map.validate_name().is_ok());
        assert_eq!(map.width, 16);
    }
}
