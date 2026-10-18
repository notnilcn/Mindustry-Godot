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
use crate::io::json::JsonIo;
use crate::io::json::objectives::MapObjectives;
use crate::io::json::rules::Rules;
use crate::io::map::PreviewImage;
use crate::io::save::state::MapSource;
use crate::maps::{Map, MapError, Maps};
use crate::world::WorldGrid;

use super::preview::PreviewPipeline;

/// [`MapSource`] view of the live editor grid (writes the `map` region).
///
/// Buildings are reported only when a tile carries an ECS entity; with an ECS
/// attached ([`Self::with_ecs`]) the building chunks come from the tile's
/// `BuildingComp` entity (the editor's [`super::EditorWorldHooks`]), so a drawn
/// core keeps its team/rotation in the saved map.
pub struct EditorMapSource<'a> {
    /// The grid being written.
    pub grid: &'a WorldGrid,
    /// Content registry (`shouldSaveData` classification).
    pub content: &'a ContentRegistry,
    /// The editor ECS world (building entities), when available.
    pub ecs: Option<&'a bevy_ecs::world::World>,
}

impl<'a> EditorMapSource<'a> {
    /// Wraps a grid and registry (no ECS: the building chunk is empty).
    pub fn new(grid: &'a WorldGrid, content: &'a ContentRegistry) -> Self {
        Self {
            grid,
            content,
            ecs: None,
        }
    }

    /// Wraps a grid, registry and the editor ECS.
    pub fn with_ecs(
        grid: &'a WorldGrid,
        content: &'a ContentRegistry,
        ecs: &'a bevy_ecs::world::World,
    ) -> Self {
        Self {
            grid,
            content,
            ecs: Some(ecs),
        }
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
        // entity; with the editor ECS the `Building.tile` decides (the same
        // check as `world::map_source::EcsMapSource`).
        let Some(entity) = self.grid.tiles.geti(index).build else {
            return true;
        };
        let Some(ecs) = self.ecs else {
            return true;
        };
        let width = self.width() as usize;
        ecs.get::<crate::entities::comp::building::Building>(entity)
            .is_some_and(|building| {
                building.tile.x() as usize == index % width
                    && building.tile.y() as usize == index / width
            })
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
        index: usize,
        chunk: &mut crate::io::wire::WireWriter,
    ) -> crate::io::IoResult<()> {
        let Some(entity) = self.grid.tiles.geti(index).build else {
            return Err(crate::io::IoError::corrupt(
                "missing building at center tile",
            ));
        };
        let Some(ecs) = self.ecs else {
            return Ok(());
        };
        if ecs
            .get::<crate::entities::comp::building::Building>(entity)
            .is_none()
        {
            return Ok(());
        }
        // Java tile-entity chunk (`SaveVersion.writeMap` -> `Building.writeAll`):
        // the per-kind revision + `writeBase` with its embedded base version —
        // the same writer the live `EcsMapSource` path uses.
        crate::world::building_io::write_all_java(ecs, entity, chunk, false)
    }
}

/// `Maps.saveMap` (client half, §3.8): write `grid` with `map_tags` merged.
///
/// `embed_assets` embeds plan-20 data assets (export only). End-to-end callers
/// that also want preview pixels use [`save_map_e2e`]. Dialog-owned
/// `objectives`/`spawns` tags are folded into `rules` first (§3.11 step 2).
pub fn save_editor_map(
    fs: &dyn FileSystem,
    file: &Path,
    grid: &WorldGrid,
    content: &ContentRegistry,
    base_tags: StringMap,
    mut map_tags: StringMap,
    embed_assets: bool,
) -> Result<(), MapError> {
    fold_dialog_tags(&mut map_tags)?;
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

/// [`save_editor_map`] with the editor's ECS world.
///
/// `Maps.saveMap` writes the live buildings (team/rotation/health/modules and
/// exact multiblock centers); [`EditorMapSource`] has no ECS access, so its
/// `write_building` is empty. The live editor passes its ECS here so a saved
/// map keeps its cores ([`crate::world::map_source::EcsMapSource`]).
#[allow(clippy::too_many_arguments)]
pub fn save_editor_map_with_ecs(
    fs: &dyn FileSystem,
    file: &Path,
    grid: &WorldGrid,
    content: &ContentRegistry,
    ecs: &bevy_ecs::world::World,
    base_tags: StringMap,
    map_tags: StringMap,
    embed_assets: bool,
) -> Result<(), MapError> {
    save_editor_map_with_ecs_and_patches(
        fs,
        file,
        grid,
        content,
        ecs,
        base_tags,
        map_tags,
        embed_assets,
        None,
    )
}

/// [`save_editor_map_with_ecs`] with the map's data assets (`state.data`).
///
/// `MapIO.writeMap` -> `SaveIO.write` always writes the owning manager's
/// assets; the Godot editor passes the `MindMods` owner here (PAR-0394).
#[allow(clippy::too_many_arguments)]
pub fn save_editor_map_with_ecs_and_patches(
    fs: &dyn FileSystem,
    file: &Path,
    grid: &WorldGrid,
    content: &ContentRegistry,
    ecs: &bevy_ecs::world::World,
    base_tags: StringMap,
    mut map_tags: StringMap,
    embed_assets: bool,
    patches: Option<&dyn crate::io::save::state::PatchSetIo>,
) -> Result<(), MapError> {
    fold_dialog_tags(&mut map_tags)?;
    let source = EditorMapSource::with_ecs(grid, content, ecs);
    crate::maps::write_map_source_with_patches(
        fs,
        file,
        &source,
        content,
        base_tags,
        map_tags,
        embed_assets,
        patches,
    )
}

/// Folds the objectives/waves dialog tags into `tags["rules"]` (plan 19 M7
/// sim-host follow-up; `MapEditor.save` §3.11 step 2).
///
/// Upstream's `MapObjectivesDialog`/`WaveInfoDialog` mutate `state.rules`
/// directly so `JsonIO.write(state.rules)` already carries them. The port keeps
/// the dialog models in dedicated `objectives`/`spawns` tags, so `save`
/// re-serializes `rules` (canonical `Rules` JSON ABI) with the current dialog
/// content. Idempotent and a no-op when neither dialog tag is present, so
/// existing maps keep byte-identical `rules`.
pub fn fold_dialog_tags(tags: &mut StringMap) -> Result<(), crate::io::IoError> {
    let objectives = tags
        .get("objectives")
        .filter(|json| !json.trim().is_empty())
        .cloned();
    let spawns = tags
        .get("spawns")
        .filter(|json| !json.trim().is_empty())
        .cloned();
    if objectives.is_none() && spawns.is_none() {
        return Ok(());
    }
    let mut rules: Rules = match tags.get("rules") {
        Some(json) if !json.trim().is_empty() => JsonIo::read(json)?,
        _ => Rules::default(),
    };
    if let Some(json) = objectives {
        rules.objectives = MapObjectives::from_json(&json)?;
    }
    if let Some(json) = spawns {
        rules.spawns = serde_json::from_str(&json)?;
    }
    tags.insert("rules".to_owned(), JsonIo::write(&rules)?);
    Ok(())
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
    mut map_tags: StringMap,
    embed_assets: bool,
) -> Result<(Map, PreviewImage), MapError> {
    fold_dialog_tags(&mut map_tags)?;
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

    fn set_block(&mut self, x: u16, y: u16, block: BlockId, _team: u8, _rot: u8) {
        if self.grid.tiles.in_bounds(x as i32, y as i32) {
            let tile = self.grid.tiles.get_mut(x as i32, y as i32);
            tile.block = block;
            // Image import only targets environment tiles; no building entity is
            // spawned (upstream `Tile.setBlock` with `Team.derelict`).
            tile.build = None;
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

    #[test]
    fn fold_dialog_tags_writes_objectives_and_spawns_into_rules() {
        let mut tags = IndexMap::new();
        tags.insert(
            "rules".to_owned(),
            JsonIo::write(&Rules::default()).expect("write rules"),
        );
        tags.insert(
            "objectives".to_owned(),
            r#"[{"class":"Item","item":"copper","amount":5}]"#.to_owned(),
        );
        tags.insert(
            "spawns".to_owned(),
            r#"[{"type":"dagger","amount":2}]"#.to_owned(),
        );
        fold_dialog_tags(&mut tags).expect("fold");
        let rules: Rules = JsonIo::read(tags.get("rules").unwrap()).expect("read rules");
        assert_eq!(rules.objectives.len(), 1);
        assert_eq!(rules.spawns.len(), 1);
        assert_eq!(rules.spawns[0].type_, "dagger");
        assert_eq!(rules.spawns[0].unit_amount, 2);

        // Idempotent: a second fold produces byte-identical `rules`.
        let once = tags.get("rules").unwrap().clone();
        fold_dialog_tags(&mut tags).expect("fold again");
        assert_eq!(tags.get("rules").unwrap(), &once);

        // No dialog tags -> no-op (byte-identical rules).
        let mut bare = IndexMap::new();
        bare.insert("rules".to_owned(), "{}".to_owned());
        let before = bare.get("rules").unwrap().clone();
        fold_dialog_tags(&mut bare).expect("no-op fold");
        assert_eq!(bare.get("rules").unwrap(), &before);
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

    /// `save_editor_map_with_ecs` keeps the editor's building entities
    /// (`Maps.saveMap` writes the live teams; `EditorMapSource` alone writes an
    /// empty building chunk, so a saved core loses its team).
    #[test]
    fn editor_save_with_ecs_keeps_core_team() {
        use crate::content::test_support::test_registry;
        use crate::ecs::MindWorld;
        use crate::editor::EditorGrid;
        use crate::editor::context::EditorContext;
        use crate::editor::grid::WorldEditorGrid;
        use crate::io::fs::MockFs;
        use crate::io::save::SaveIo;
        use crate::io::save::state::SaveReadState;
        use crate::world::{NoopRenderHooks, WorldGrid};

        let mut content = test_registry();
        let core = content.block_id("core-shard").unwrap();
        let mut grid = WorldGrid::new(16, 16);
        grid.begin_map_load();
        let mut ecs = MindWorld::new();
        let hooks = crate::editor::EditorWorldHooks::new();
        {
            let mut editor =
                WorldEditorGrid::new(&mut grid, &content, &mut ecs.0, &hooks, &NoopRenderHooks);
            editor.set_block(8, 8, core, crate::game::team::SHARDED.0, 0);
        }
        grid.end_map_load(&content);

        let fs = MockFs::new();
        let file = std::path::PathBuf::from("/data/maps/editor-core.msav");
        let base = editor_base_tags(16, 16, "editor-core");
        let mut tags = StringMap::new();
        tags.insert("name".to_owned(), "editor-core".to_owned());
        save_editor_map_with_ecs(&fs, &file, &grid, &content, &ecs.0, base, tags, false)
            .expect("editor save");

        let bytes = fs.read(&file).unwrap();
        let mut loaded = WorldGrid::new(0, 0);
        loaded.begin_map_load();
        let pending = {
            let mut context = EditorContext::new(&mut loaded, &content);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut content),
                ..SaveReadState::default()
            };
            SaveIo::load_bytes(&bytes, &mut state).expect("saved map loads");
            context.take_pending_buildings()
        };
        loaded.end_map_load(&content);
        assert!(
            pending.iter().any(|(_, base)| base.team == 1),
            "the saved map keeps the core's team"
        );
    }
}
