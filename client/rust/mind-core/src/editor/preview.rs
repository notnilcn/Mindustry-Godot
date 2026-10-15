// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map preview pipeline (plan 19 §3.8; `maps/MapPreviewLoader.java` +
//! `Maps.loadPreviews`/`queueNewPreview`/`createNewPreview`).
//!
//! This is the **Godot-free core half**: it owns the preview queue, generated
//! [`PreviewImage`]s and the decoded cache, and writes the preview PNG/cache
//! through plan-04/06. The `Gd<ImageTexture>` binding named in §3.8 is the M3
//! view adapter (`mind-gdext::editor::preview`), which maps [`Self::texture_for`]
//! onto a `Texture2D`; M2 verifies the deterministic pixels + PNG round-trip
//! headlessly. The upstream keep-alive reflection hack (`Rules.fog`) is not
//! ported — plan 12's `FogControl` marks previews retained directly.

use std::path::PathBuf;

use indexmap::IndexMap;

use crate::content::ContentRegistry;
use crate::io::fs::{FileSystem, Paths};
use crate::io::map::{BlockPalette, MapIo, PreviewImage, encode_png};
use crate::maps::preview::{PreviewCache, PreviewQueue, preview_file, read_cache, write_cache};
use crate::maps::{Map, MapError, Maps};

/// The client preview pipeline (`Maps.previewList` + texture cache).
#[derive(Default)]
pub struct PreviewPipeline {
    queue: PreviewQueue,
    images: IndexMap<PathBuf, PreviewImage>,
    caches: IndexMap<PathBuf, PreviewCache>,
}

impl PreviewPipeline {
    /// An empty pipeline.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Maps.queueNewPreview`: enqueue a map whose preview must be regenerated.
    pub fn queue_new_preview(&mut self, map: &Map) {
        self.queue.queue(map);
    }

    /// `Maps.loadPreviews`: read each existing cache; queue missing/broken ones.
    ///
    /// A missing preview file or an unreadable cache re-queues the map (the
    /// upstream `MapPreviewLoader` delete-and-requeue behavior).
    pub fn load_previews(&mut self, fs: &dyn FileSystem, paths: &Paths, maps: &Maps) {
        for map in maps.all() {
            if !fs.exists(&preview_file(paths, map)) {
                self.queue.queue(map);
                continue;
            }
            let mut probe = map.clone();
            match read_cache(fs, paths, &mut probe) {
                Ok(()) => {
                    self.caches.insert(
                        map.file.clone(),
                        PreviewCache {
                            spawns: probe.spawns as i32,
                            teams: probe.teams,
                        },
                    );
                }
                Err(_) => self.queue.queue(map),
            }
        }
    }

    /// `Maps.createAllPreviews`: drain the queue and generate each preview.
    ///
    /// Returns the number of previews successfully generated.
    pub fn create_all_previews(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        registry: &mut ContentRegistry,
        maps: &Maps,
    ) -> usize {
        let queued = self.queue.take();
        let mut created = 0;
        for file in queued {
            let Some(map) = maps.all().iter().find(|map| map.file == file) else {
                continue;
            };
            if self.create_new_preview(fs, paths, registry, map).is_ok() {
                created += 1;
            }
        }
        created
    }

    /// `Maps.createNewPreview`: generate pixels, then write the PNG (and cache).
    ///
    /// The cache carries the scan facts (`Map.spawns`/`Map.teams`) the generator
    /// collected: Java's `MapIO.generatePreview(map)` fills the map header before
    /// `Maps.writeCache` runs (`Maps.java:418-456`), so later `map_modes` reads
    /// never rescan the file.
    pub fn create_new_preview(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        registry: &mut ContentRegistry,
        map: &Map,
    ) -> Result<(), MapError> {
        let palette = BlockPalette::of(registry);
        self.create_new_preview_with_palette(fs, paths, registry, map, palette)
    }

    /// [`Self::create_new_preview`] with the caller's [`BlockPalette`] (the
    /// gdext side supplies the `sprites/block_colors.png` minimap colours).
    pub fn create_new_preview_with_palette(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        registry: &mut ContentRegistry,
        map: &Map,
        palette: BlockPalette,
    ) -> Result<(), MapError> {
        let bytes = fs.read(&map.file)?;
        let (image, facts) = MapIo::generate_preview_with_palette(palette, registry, &bytes)?;
        fs.write(&preview_file(paths, map), &encode_png(&image)?)?;
        // Cache failures are non-fatal upstream (the preview still loads).
        let mut scanned = map.clone();
        scanned.spawns = facts.spawns;
        scanned.teams = facts.teams;
        if let Err(error) = write_cache(fs, paths, &scanned) {
            log::warn!(
                "failed to write preview cache for `{}`: {error}",
                map.name()
            );
        }
        self.images.insert(map.file.clone(), image);
        Ok(())
    }

    /// `Map.safeTexture()` equivalent: the generated pixels for a map, if any.
    pub fn texture_for(&self, map: &Map) -> Option<&PreviewImage> {
        self.images.get(&map.file)
    }

    /// The decoded cache for a map, if it was loaded.
    pub fn cache_for(&self, map: &Map) -> Option<&PreviewCache> {
        self.caches.get(&map.file)
    }

    /// 16's frame call site (`checkPreviews`): keep every map's preview present.
    ///
    /// Returns the number of newly queued maps.
    pub fn check_previews(&mut self, fs: &dyn FileSystem, paths: &Paths, maps: &Maps) -> usize {
        let mut queued = 0;
        for map in maps.all() {
            if !self.images.contains_key(&map.file) && !fs.exists(&preview_file(paths, map)) {
                self.queue.queue(map);
                queued += 1;
            }
        }
        queued
    }

    /// The pending preview queue.
    pub fn queue(&self) -> &PreviewQueue {
        &self.queue
    }

    /// `Maps.createAllPreviews` batched: pop up to `max` queued map files.
    pub fn take_queued(&mut self, max: usize) -> Vec<PathBuf> {
        self.queue.take_up_to(max)
    }

    /// Number of generated previews.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether no preview has been generated.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::fs::MockFs;

    fn map(name: &str, file: &str) -> Map {
        let mut tags = IndexMap::new();
        tags.insert("name".to_owned(), name.to_owned());
        Map::new(PathBuf::from(file), 4, 4, tags, true, 1, -1)
    }

    #[test]
    fn load_previews_queues_missing_and_reads_cache() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let maps = Maps::new();
        let missing = map("Missing", "/data/maps/missing.msav");

        // Missing preview queues the map.
        let mut pipeline = PreviewPipeline::new();
        pipeline.load_previews(&fs, &paths, &maps);
        assert!(pipeline.queue().is_empty()); // no maps registered
        pipeline.queue_new_preview(&missing);
        assert_eq!(pipeline.queue().entries().len(), 1);

        // A present preview + valid cache is read, not queued.
        let present = map("Present", "/data/maps/present.msav");
        fs.write(&preview_file(&paths, &present), b"\x89PNG")
            .unwrap();
        write_cache(&fs, &paths, &present).unwrap();
        let mut pipeline = PreviewPipeline::new();
        pipeline.load_previews(&fs, &paths, &maps);
        assert!(pipeline.queue().is_empty());
    }

    #[test]
    fn create_new_preview_writes_png() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut registry = crate::content::test_support::test_registry();
        let file = PathBuf::from("/data/maps/fork.msav");
        let world = crate::io::save::fixture::FixtureWorld::synthetic(&registry, 16, 16);
        let mut tags = crate::io::save::versions::v1::base_meta_tags(16, 16, 0, "Fork");
        tags.insert("name".to_owned(), "Fork".to_owned());
        let mut ctx = crate::io::save::WriteContext::meta_only(tags);
        ctx.content = Some(&registry);
        ctx.map = Some(&world);
        ctx.entities = Some(&world);
        crate::io::save::SaveIo::save(&fs, &file, &ctx, &crate::io::save::SaveOptions::new())
            .unwrap();

        let map = map("Fork", "/data/maps/fork.msav");
        let mut pipeline = PreviewPipeline::new();
        pipeline
            .create_new_preview(&fs, &paths, &mut registry, &map)
            .unwrap();
        assert_eq!(pipeline.len(), 1);
        assert!(fs.exists(&preview_file(&paths, &map)));
        assert!(pipeline.texture_for(&map).is_some());
    }

    /// The cache written by `createNewPreview` must carry the scan facts
    /// `MapIO.generatePreview` collected (`Maps.java:418-456`), so a later
    /// `map_modes` never has to rescan the file.
    #[test]
    #[cfg(feature = "msav-import")]
    fn create_new_preview_writes_scan_facts_into_cache() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

        let file = PathBuf::from(format!(
            "{}/../../../assets/maps/default/caldera.msav",
            env!("CARGO_MANIFEST_DIR")
        ));
        assert!(file.is_file(), "{} is missing", file.display());
        let fs = crate::io::fs::NativeFs;
        let paths = Paths::new(std::env::temp_dir().join("i06-t5-preview-facts"));
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content registry");
        let map = Map::new(file, 0, 0, indexmap::IndexMap::new(), true, 0, -1);
        let mut pipeline = PreviewPipeline::new();
        pipeline
            .create_new_preview(&fs, &paths, &mut registry, &map)
            .expect("preview generation");
        assert!(fs.exists(&preview_file(&paths, &map)));

        let mut probe = map.clone();
        read_cache(&fs, &paths, &mut probe).expect("cache written");
        assert!(probe.spawns > 0, "caldera has spawn overlays");
        assert!(probe.teams.contains(&1), "caldera has a player core team");
    }

    #[test]
    fn take_queued_drains_in_batches() {
        let mut pipeline = PreviewPipeline::new();
        pipeline.queue_new_preview(&map("A", "/data/maps/a.msav"));
        pipeline.queue_new_preview(&map("B", "/data/maps/b.msav"));
        pipeline.queue_new_preview(&map("C", "/data/maps/c.msav"));
        assert_eq!(pipeline.take_queued(2).len(), 2);
        assert_eq!(pipeline.queue().entries().len(), 1);
        assert_eq!(pipeline.take_queued(2).len(), 1);
        assert!(pipeline.queue().is_empty());
    }
}
