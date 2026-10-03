// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map-lifecycle glue shared by the editor and the headless harness (plan 19 §3.8).
//!
//! 06 owns the `Maps` registry and its paths/cache; 04 owns `MapIo`. This module
//! supplies the editor-facing wrappers that 19 owns: an editor [`MapSource`] over
//! a live [`WorldGrid`], `tryImportMap` name sniffing/copying and the map-save
//! call. The Godot `EditorMapsDialog` subclass (plan 14's `MapListDialog`) is M3.

use std::path::Path;

use indexmap::IndexMap;

use crate::content::{BlockId, ContentRegistry};
use crate::io::StringMap;
use crate::io::fs::{FileSystem, Paths};
use crate::io::map::PreviewImage;
use crate::io::save::state::MapSource;
use crate::maps::{Map, MapError, Maps};
use crate::world::WorldGrid;

use super::preview::PreviewPipeline;

/// [`MapSource`] view of the live editor grid (writes the `map` region).
///
/// Buildings are reported only when a tile carries an ECS entity; the editor
/// harness that does not spawn buildings thus writes a plain tile map.
pub struct EditorMapSource<'a> {
    /// The grid being written.
    pub grid: &'a WorldGrid,
    /// Content registry (`shouldSaveData` classification).
    pub content: &'a ContentRegistry,
}

impl<'a> EditorMapSource<'a> {
    /// Wraps a grid and registry.
    pub fn new(grid: &'a WorldGrid, content: &'a ContentRegistry) -> Self {
        Self { grid, content }
    }
}

impl MapSource for EditorMapSource<'_> {
    fn width(&self) -> u16 {
        self.grid.tiles.width as u16
    }

    fn height(&self) -> u16 {
        self.grid.tiles.height as u16
    }

    fn floor_id(&self, index: usize) -> u16 {
        self.grid.tiles.geti(index).floor.raw()
    }

    fn overlay_id(&self, index: usize) -> u16 {
        self.grid.tiles.geti(index).overlay.raw()
    }

    fn block_id(&self, index: usize) -> u16 {
        self.grid.tiles.geti(index).block.raw()
    }

    fn has_building(&self, index: usize) -> bool {
        self.grid.tiles.geti(index).build.is_some()
    }

    fn is_center(&self, index: usize) -> bool {
        // Without ECS access the only observable center is a tile carrying no
        // entity; plan 07 supplies exact multiblock centers when it lands.
        self.grid.tiles.geti(index).build.is_none()
    }

    fn should_save_data(&self, index: usize) -> bool {
        self.grid.tiles.geti(index).should_save_data(self.content)
    }

    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32) {
        let tile = self.grid.tiles.geti(index);
        (
            tile.data as u8,
            tile.floor_data as u8,
            tile.overlay_data as u8,
            tile.extra_data,
        )
    }

    fn write_building(
        &self,
        _index: usize,
        _chunk: &mut crate::io::wire::WireWriter,
    ) -> crate::io::IoResult<()> {
        Ok(())
    }
}

/// `Maps.saveMap` (client half, §3.8): write `grid` with `map_tags` merged.
///
/// `embed_assets` embeds plan-20 data assets (export only). End-to-end callers
/// that also want preview pixels use [`save_map_e2e`].
pub fn save_editor_map(
    fs: &dyn FileSystem,
    file: &Path,
    grid: &WorldGrid,
    content: &ContentRegistry,
    base_tags: StringMap,
    map_tags: StringMap,
    embed_assets: bool,
) -> Result<(), MapError> {
    let source = EditorMapSource::new(grid, content);
    crate::maps::write_map_source(
        fs,
        file,
        &source,
        content,
        base_tags,
        map_tags,
        embed_assets,
    )
}

/// `EditorMapsDialog.tryImportMap`: sniff an import, resolve a free name and add
/// it to the registry.
///
/// Images are rejected (`MapIO.isImage`); a nameless header gets `unknownN`.
/// The file is copied into `dir` under a sanitized, collision-free name.
pub fn try_import_map(
    fs: &dyn FileSystem,
    maps: &mut Maps,
    dir: &Path,
    source: &Path,
) -> Result<Map, MapError> {
    maps.import_map(fs, dir, source)
}

/// End-to-end `Maps.saveMap` for the editor (plan 06 M4 / plan 19 M7): write the
/// native `MGRS` map, fill the preview spawn/team cache and generate the preview
/// pixels (PNG + cache) through the [`PreviewPipeline`].
///
/// Returns the registered map and its deterministic preview pixels so a headless
/// caller can assert the checksum.
#[allow(clippy::too_many_arguments)]
pub fn save_map_e2e(
    fs: &dyn FileSystem,
    paths: &Paths,
    pipeline: &mut PreviewPipeline,
    maps: &mut Maps,
    file: &Path,
    grid: &WorldGrid,
    registry: &mut ContentRegistry,
    base_tags: StringMap,
    map_tags: StringMap,
    embed_assets: bool,
) -> Result<(Map, PreviewImage), MapError> {
    let source = EditorMapSource::new(grid, registry);
    let map = maps.save_map(
        fs,
        paths,
        file,
        &source,
        registry,
        base_tags,
        map_tags,
        embed_assets,
    )?;
    pipeline.queue_new_preview(&map);
    pipeline.create_new_preview(fs, paths, registry, &map)?;
    let image = pipeline
        .texture_for(&map)
        .cloned()
        .ok_or_else(|| MapError::NotFound(file.display().to_string()))?;
    Ok((map, image))
}

/// End-to-end `Maps.importMap` (plan 06 §3.9 / plan 19 §3.8): copy + register an
/// imported map, then regenerate its preview pixels (PNG + cache).
pub fn import_map_e2e(
    fs: &dyn FileSystem,
    paths: &Paths,
    pipeline: &mut PreviewPipeline,
    maps: &mut Maps,
    dir: &Path,
    source: &Path,
    registry: &mut ContentRegistry,
) -> Result<(Map, PreviewImage), MapError> {
    let map = maps.import_map(fs, dir, source)?;
    pipeline.queue_new_preview(&map);
    pipeline.create_new_preview(fs, paths, registry, &map)?;
    let image = pipeline
        .texture_for(&map)
        .cloned()
        .ok_or_else(|| MapError::NotFound(map.file.display().to_string()))?;
    Ok((map, image))
}

/// Builds the standard base meta tags for an editor map export (`plan 04 §6.2`).
pub fn editor_base_tags(width: u16, height: u16, map_name: &str) -> StringMap {
    crate::io::save::versions::v1::base_meta_tags(width, height, 0, map_name)
}

/// Sorts editor tags for canonical, byte-stable saves (deterministic key order).
///
/// `saveMap` is idempotent only when tag ordering is normalized; this mirrors
/// the plan-19 §2.2 "after tag ordering is normalized" clause.
pub fn canonicalize_tags(tags: &mut IndexMap<String, String>) {
    tags.sort_keys();
}

/// Placeholder used by image import to map unmapped pixels to `air`.
pub const IMAGE_UNMAPPED: BlockId = BlockId::AIR;

/// Adapter exposing plan-06's [`crate::world::ColorMapper`] as the plan-04
/// image-import [`crate::io::map::ColorMapper`] trait.
struct ContentColorMapper<'a>(&'a crate::world::ColorMapper);

impl crate::io::map::ColorMapper for ContentColorMapper<'_> {
    fn block_for_color(&self, rgba: u32) -> Option<BlockId> {
        Some(self.0.get(rgba))
    }
}

/// [`crate::io::map::ImageTileSink`] over the live grid during an image import.
pub struct GridImageSink<'a> {
    /// The grid receiving floors/overlays.
    pub grid: &'a mut WorldGrid,
}

impl crate::io::map::ImageTileSink for GridImageSink<'_> {
    fn width(&self) -> u16 {
        self.grid.tiles.width as u16
    }

    fn height(&self) -> u16 {
        self.grid.tiles.height as u16
    }

    fn set_floor(&mut self, x: u16, y: u16, floor: BlockId) {
        if self.grid.tiles.in_bounds(x as i32, y as i32) {
            self.grid.tiles.get_mut(x as i32, y as i32).floor = floor;
        }
    }

    fn set_overlay(&mut self, x: u16, y: u16, overlay: BlockId) {
        if self.grid.tiles.in_bounds(x as i32, y as i32) {
            self.grid.tiles.get_mut(x as i32, y as i32).overlay = overlay;
        }
    }
}

/// `MapIO.readImage(img, tiles, colorMapper)`: import an image onto a grid.
///
/// The grid is assumed to already be floor-initialized (stone); unmapped pixels
/// keep that default.
pub fn import_image_into_grid(
    registry: &ContentRegistry,
    image: &crate::io::map::PreviewImage,
    grid: &mut WorldGrid,
) -> crate::io::IoResult<()> {
    let palette = crate::io::map::BlockPalette::of(registry);
    let mut mapper = crate::world::ColorMapper::new();
    mapper.load(registry);
    let adapter = ContentColorMapper(&mapper);
    let mut sink = GridImageSink { grid };
    crate::io::map::read_image(&palette, image, &mut sink, &adapter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::fs::MockFs;

    #[test]
    fn editor_map_source_reads_grid() {
        let content = test_registry();
        let mut grid = WorldGrid::new(4, 4);
        let ice = content.block_id("ice").unwrap();
        grid.tiles.get_mut(1, 2).floor = ice;
        let source = EditorMapSource::new(&grid, &content);
        assert_eq!(source.width(), 4);
        assert_eq!(source.height(), 4);
        assert_eq!(source.floor_id(grid.tiles.index(1, 2)), ice.raw());
    }

    #[test]
    fn canonicalize_tags_sorts_keys() {
        let mut tags = IndexMap::new();
        tags.insert("name".to_owned(), "n".to_owned());
        tags.insert("author".to_owned(), "a".to_owned());
        canonicalize_tags(&mut tags);
        let keys: Vec<&str> = tags.keys().map(String::as_str).collect();
        assert_eq!(keys, ["author", "name"]);
    }

    #[test]
    fn try_import_rejects_images() {
        let fs = MockFs::new();
        let mut maps = Maps::new();
        let file = std::path::PathBuf::from("/imports/pic.png");
        fs.write(&file, &crate::io::map::PNG_SIGNATURE).unwrap();
        let error = try_import_map(&fs, &mut maps, Path::new("/maps"), &file).unwrap_err();
        assert!(error.to_string().contains("image"));
    }

    /// End-to-end `save_map`/`import_map` with preview pixels (plan 06 M4 /
    /// plan 19 M7): the native map round-trips through an import copy and both
    /// preview generations are byte-identical.
    #[test]
    fn save_and_import_map_e2e_preview_pixels() {
        use crate::content::BlockId;
        use crate::io::fs::Paths;

        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut content = crate::content::test_support::test_registry();
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);

        let mut grid = WorldGrid::new(8, 8);
        for tile in grid.tiles.array_mut() {
            tile.floor = stone;
        }
        let mut spawns = 0u32;
        if let Some(spawn) = content.block_id("spawn") {
            grid.tiles.get_mut(1, 1).overlay = spawn;
            grid.tiles.get_mut(4, 4).overlay = spawn;
            spawns = 2;
        }

        let mut maps = Maps::new();
        let mut pipeline = PreviewPipeline::new();
        let file = std::path::PathBuf::from("/data/maps/e2e.msav");
        let mut tags = IndexMap::new();
        tags.insert("name".to_owned(), "E2E".to_owned());
        let base = editor_base_tags(8, 8, "E2E");
        let (map, image) = save_map_e2e(
            &fs,
            &paths,
            &mut pipeline,
            &mut maps,
            &file,
            &grid,
            &mut content,
            base,
            tags,
            false,
        )
        .unwrap();
        assert_eq!(maps.len(), 1);
        assert_eq!(map.name(), "E2E");
        assert_eq!(map.spawns, spawns);
        assert!(fs.exists(&crate::maps::preview_file(&paths, &map)));
        assert_eq!(image.rgba.len(), 8 * 8 * 4);

        // Import into a second registry/dir; the preview pixels must match.
        let dir = std::path::PathBuf::from("/data/imports");
        let mut imported_maps = Maps::new();
        let mut imported_pipeline = PreviewPipeline::new();
        let (imported, image2) = import_map_e2e(
            &fs,
            &paths,
            &mut imported_pipeline,
            &mut imported_maps,
            &dir,
            &file,
            &mut content,
        )
        .unwrap();
        assert_eq!(imported.name(), "E2E");
        assert_eq!(imported_maps.len(), 1);
        assert_eq!(image.rgba, image2.rgba, "preview pixels are deterministic");
    }
}
