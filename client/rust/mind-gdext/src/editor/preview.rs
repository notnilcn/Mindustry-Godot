// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindPreview` — plan-19 map-preview provider (gdext facade half).
//!
//! The Godot-free half is `mind_core::editor::preview::PreviewPipeline`
//! (plan 19 §3.8): it owns the queue, the generated [`PreviewImage`]s and the
//! decoded cache. This node is the `Texture2D` binding seam named by plan 19
//! §3.1/§3.8 and the Rust-side data provider for `EditorMapsDialog`
//! (`maps_list`).
//!
//! The in-engine MCP §7c run is deferred to the single-editor mutex (like the
//! rest of the editor shell); this class compiles and is exercisable from
//! GDScript without any live-world dependency.

use std::collections::VecDeque;
use std::path::PathBuf;

use godot::builtin::{Array, GString, PackedByteArray, VarDictionary};
use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Image, ImageTexture, Node, Texture2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore};
use mind_core::editor::preview::PreviewPipeline;
use mind_core::game::gamemode::{Gamemode, MapView};
use mind_core::game::waves::WaveMapHooks;
use mind_core::io::fs::{FileSystem, NativeFs, Paths};
use mind_core::io::map::{BlockPalette, MapIo, PreviewFacts, PreviewImage, decode_png};
use mind_core::maps::preview::read_cache;
use mind_core::maps::{Map, Maps, preview_file};

use crate::assets::loader::{globalize_path, load_map_registry, user_data_root};

/// `Maps.createAllPreviews` in small per-frame batches (the menu must not stall).
const PREVIEWS_PER_FRAME: usize = 2;

/// `MindPreview` — map preview textures + `EditorMapsDialog` provider rows.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindPreview {
    base: Base<Node>,
    pipeline: PreviewPipeline,
    maps: Maps,
    /// The data root that holds `maps/` and `previews/`.
    root: PathBuf,
    /// Lazily built content registry (preview generation + map scans).
    content: Option<ContentRegistry>,
    /// Registry palette with the real `sprites/block_colors.png` minimap
    /// colours applied (`ContentLoader.loadColors`); built with `content`.
    palette: Option<BlockPalette>,
    /// Per-map scan facts (`Map.spawns`/`Map.teams`), memoized.
    facts: std::collections::HashMap<PathBuf, PreviewFacts>,
    /// Pending batched generation queue (`Maps.previewList`).
    pending: VecDeque<PathBuf>,
}

#[godot_api]
impl INode for MindPreview {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            pipeline: PreviewPipeline::new(),
            maps: Maps::new(),
            root: user_data_root(),
            content: None,
            palette: None,
            facts: std::collections::HashMap::new(),
            pending: VecDeque::new(),
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn process(&mut self, _delta: f64) {
        self.pump_previews();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

impl MindPreview {
    /// Rebuilds the node's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        let loaded = self.reload();
        log::info!("MindPreview ready ({loaded} maps)");
    }

    /// Reloads the live registry (`Maps.load`): the built-in map set from the
    /// resolved assets folder (`<assets>/maps/default`) plus custom maps from
    /// the data root's `maps/`.
    fn reload(&mut self) -> usize {
        self.maps = load_map_registry(&self.root);
        self.facts.clear();
        self.maps.len()
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.root)
    }

    fn map_for(&self, file: &str) -> Option<Map> {
        self.maps
            .all()
            .iter()
            .find(|map| map.file.display().to_string() == file)
            .cloned()
    }

    /// A map by display name or by file path (the dialogs carry either).
    fn map_for_name(&self, name: &str) -> Option<Map> {
        self.maps
            .by_name(name)
            .cloned()
            .or_else(|| self.map_for(name))
    }

    /// Lazily builds the content registry used for preview generation and map
    /// scans (same lifecycle as `MindEditor`/`MindCampaign`), plus its palette
    /// with the atlas minimap colours (`ContentLoader.loadColors`).
    fn ensure_content(&mut self) -> bool {
        if self.content.is_some() {
            return true;
        }
        // The shared content boot (`content::boot::build_registry_with_mods`):
        // mod content renders in previews and map lists (J09 T3).
        match crate::content_boot::build_registry(&MemoryBundle::new(), &MemoryUnlockStore::new()) {
            Ok(mut registry) => {
                if let Err(error) = registry.init().and_then(|()| registry.post_init()) {
                    log::warn!("MindPreview content lifecycle failed: {error}");
                    return false;
                }
                let mut palette = BlockPalette::of(&registry);
                palette.apply_block_colors(&crate::render::minimap::load_block_colors());
                self.palette = Some(palette);
                self.content = Some(registry);
                true
            }
            Err(error) => {
                log::warn!("MindPreview content boot failed: {error}");
                false
            }
        }
    }

    /// The map's scan facts (`Map.spawns`/`Map.teams`), from a previous scan,
    /// the preview cache, or a one-off `MapIO.generatePreview` read. Zero facts
    /// when the map or the registry is unavailable.
    fn map_facts(&mut self, map: &Map) -> PreviewFacts {
        if let Some(facts) = self.facts.get(&map.file) {
            return facts.clone();
        }
        // `Maps.readCache`: a preview cache written by an earlier generation.
        let paths = self.paths();
        let mut probe = map.clone();
        if read_cache(&NativeFs, &paths, &mut probe).is_ok() {
            let facts = PreviewFacts {
                spawns: probe.spawns,
                teams: probe.teams,
            };
            self.facts.insert(map.file.clone(), facts.clone());
            return facts;
        }
        // Scan the file (`MapIO.generatePreview`'s metadata half).
        if !self.ensure_content() {
            return PreviewFacts::default();
        }
        let Some(registry) = self.content.as_mut() else {
            return PreviewFacts::default();
        };
        match NativeFs
            .read(&map.file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                MapIo::generate_preview_with_facts(registry, &bytes)
                    .map_err(|error| error.to_string())
            }) {
            Ok((_, facts)) => {
                self.facts.insert(map.file.clone(), facts.clone());
                facts
            }
            Err(error) => {
                log::warn!(
                    "MindPreview scan of `{}` failed: {error}",
                    map.file.display()
                );
                PreviewFacts::default()
            }
        }
    }

    /// One `_process` tick of the batched `Maps.createAllPreviews`.
    ///
    /// Generates up to [`PREVIEWS_PER_FRAME`] queued previews, emits
    /// `previews_changed` when a batch landed and stops processing when the
    /// queue drains.
    fn pump_previews(&mut self) {
        let mut created = 0usize;
        for _ in 0..PREVIEWS_PER_FRAME {
            let Some(file) = self.pending.pop_front() else {
                break;
            };
            if self.generate_preview_for(&file) {
                created += 1;
            }
        }
        if created > 0 {
            let _ = self.base_mut().emit_signal("previews_changed", &[]);
        }
        if self.pending.is_empty() {
            self.base_mut().set_process(false);
        }
    }

    /// One `Maps.createNewPreview`: generate + write the PNG and its cache.
    fn generate_preview_for(&mut self, file: &PathBuf) -> bool {
        let Some(map) = self
            .maps
            .all()
            .iter()
            .find(|map| map.file == *file)
            .cloned()
        else {
            return false;
        };
        if !self.ensure_content() {
            return false;
        }
        let paths = self.paths();
        let Some(palette) = self.palette.clone() else {
            return false;
        };
        let Some(registry) = self.content.as_mut() else {
            return false;
        };
        if let Err(error) = self
            .pipeline
            .create_new_preview_with_palette(&NativeFs, &paths, registry, &map, palette)
        {
            log::warn!(
                "MindPreview generation of `{}` failed: {error}",
                map.file.display()
            );
            return false;
        }
        // Memoize the scan facts the fresh cache holds (`Map.spawns`/`teams`).
        let mut probe = map.clone();
        if read_cache(&NativeFs, &paths, &mut probe).is_ok() {
            self.facts.insert(
                map.file.clone(),
                PreviewFacts {
                    spawns: probe.spawns,
                    teams: probe.teams,
                },
            );
        }
        true
    }
}

#[godot_api]
impl MindPreview {
    /// Emitted when one batched `Maps.createAllPreviews` generation lands.
    #[signal]
    fn previews_changed();

    /// Data root used for `maps/` + `previews/` (defaults to `user://`).
    #[func]
    pub fn data_root(&self) -> GString {
        GString::from(self.root.display().to_string().as_str())
    }

    /// Overrides the data root (`res://`/`user://` resolved via Godot).
    #[func]
    pub fn set_data_root(&mut self, root: GString) {
        self.root = globalize_path(&root.to_string());
        self.facts.clear();
    }

    /// Reloads the live map registry (`Maps.load`): built-ins from the assets
    /// folder plus `<root>/maps` custom maps. Returns the map count.
    #[func]
    pub fn refresh(&mut self) -> i64 {
        self.reload() as i64
    }

    /// `Maps.loadPreviews` + the batched `createAllPreviews`: queues every map
    /// whose preview PNG or cache is missing/broken, then `_process` generates
    /// [`PREVIEWS_PER_FRAME`] per frame and emits `previews_changed` per batch.
    /// Returns the queued count.
    #[func]
    pub fn generate_previews(&mut self) -> i64 {
        if !self.ensure_content() {
            return 0;
        }
        let paths = self.paths();
        self.pipeline.load_previews(&NativeFs, &paths, &self.maps);
        let queued = self.pipeline.take_queued(usize::MAX);
        let mut added = 0i64;
        for file in queued {
            // A second `generate_previews` before the queue drains must not
            // enqueue the same file twice.
            if !self.pending.contains(&file) {
                self.pending.push_back(file);
                added += 1;
            }
        }
        if !self.pending.is_empty() {
            self.base_mut().set_process(true);
        }
        added
    }

    /// `CustomGameDialog`/`EditorMapsDialog` provider rows:
    /// `{name, path, custom, author, description, width, height,
    /// lastModified, preview}`.
    #[func]
    pub fn maps_list(&self) -> Array<VarDictionary> {
        let paths = self.paths();
        let mut out = Array::<VarDictionary>::new();
        for map in self.maps.all() {
            let (name, path, custom, author, description, width, height) = map_row_fields(map);
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &GString::from(name.as_str()).to_variant(),
            );
            row.set(
                &GString::from("path"),
                &GString::from(path.as_str()).to_variant(),
            );
            row.set(&GString::from("custom"), &custom.to_variant());
            row.set(&GString::from("mod"), &map.mod_id.is_some().to_variant());
            row.set(
                &GString::from("author"),
                &GString::from(author.as_str()).to_variant(),
            );
            row.set(
                &GString::from("description"),
                &GString::from(description.as_str()).to_variant(),
            );
            row.set(&GString::from("width"), &width.to_variant());
            row.set(&GString::from("height"), &height.to_variant());
            row.set(
                &GString::from("lastModified"),
                &map_row_last_modified(map).to_variant(),
            );
            row.set(
                &GString::from("preview"),
                &NativeFs.exists(&preview_file(&paths, map)).to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Number of registered maps.
    #[func]
    pub fn map_count(&self) -> i64 {
        self.maps.len() as i64
    }

    /// The on-disk preview PNG path for a map file ('' when unknown).
    #[func]
    pub fn preview_path(&self, map_file: GString) -> GString {
        match self.map_for(&map_file.to_string()) {
            Some(map) => GString::from(
                preview_file(&self.paths(), &map)
                    .display()
                    .to_string()
                    .as_str(),
            ),
            None => GString::new(),
        }
    }

    /// The map file path for a display name or file path ('' when unknown).
    /// Bare names resolve through the registry rows, so a caller (e.g.
    /// `MindSimHost.load_map_world`) can address a map by name.
    #[func]
    pub fn map_path(&self, map_name: GString) -> GString {
        match self.map_for_name(&map_name.to_string()) {
            Some(map) => GString::from(map.file.display().to_string().as_str()),
            None => GString::new(),
        }
    }

    /// `Gamemode.valid(map)` per mode (`Gamemode.java:17-37`), from the map's
    /// preview scan facts (`MapIO.generatePreview`):
    /// `{survival, sandbox, attack, pvp, editor}`.
    #[func]
    pub fn map_modes(&mut self, map_name: GString) -> VarDictionary {
        let mut out = VarDictionary::new();
        let facts = match self.map_for_name(&map_name.to_string()) {
            Some(map) => self.map_facts(&map),
            None => PreviewFacts::default(),
        };
        let view = MapView::new(facts.spawns as usize, facts.teams.len());
        for mode in Gamemode::ALL {
            out.set(&GString::from(mode.name()), &mode.valid(view).to_variant());
        }
        out
    }

    /// `Map.applyRules(mode)` serialized (`Control.playMap`'s `rules`, custom
    /// rules JSON when the dialog edited one). `{}` for an unknown
    /// map/mode.
    #[func]
    pub fn map_rules_json(&mut self, map_name: GString, mode: GString) -> GString {
        let Some(map) = self.map_for_name(&map_name.to_string()) else {
            return GString::from("{}");
        };
        let Some(mode) = Gamemode::from_name(&mode.to_string()) else {
            return GString::from("{}");
        };
        let hooks = WaveMapHooks::new();
        let rules = map.apply_rules(mode, &hooks);
        GString::from(
            serde_json::to_string(&rules)
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// `Map.safeTexture()`: binds a map's preview pixels to a `Texture2D`.
    ///
    /// The core pipeline's generated image is preferred; otherwise the cached
    /// preview PNG is decoded (plan 19 §3.8 `error.png` fallback is the caller's
    /// concern until the plan-16 frame hook lands).
    #[func]
    pub fn texture_for(&mut self, map_file: GString) -> Option<Gd<Texture2D>> {
        let map = self.map_for(&map_file.to_string())?;
        if let Some(image) = self.pipeline.texture_for(&map) {
            return bind_texture(image);
        }
        let preview = preview_file(&self.paths(), &map);
        let bytes = NativeFs.read(&preview).ok()?;
        let image = decode_png(&bytes).ok()?;
        bind_texture(&image)
    }

    /// [`Self::texture_for`] by display name (the map lists carry names).
    #[func]
    pub fn texture_for_name(&mut self, map_name: GString) -> Option<Gd<Texture2D>> {
        let path = self.map_path(map_name);
        if path.is_empty() {
            return None;
        }
        self.texture_for(path)
    }

    /// `Map.getHighScore`'s settings key: `"hiscore" + file
    /// nameWithoutExtension + steamid` (`Map.java:66-68`). `""` when the map is
    /// unknown.
    #[func]
    pub fn map_high_score_key(&self, map_name: GString) -> GString {
        match self.map_for_name(&map_name.to_string()) {
            Some(map) => GString::from(
                format!(
                    "hiscore{}{}",
                    map.file_stem().unwrap_or_else(|| map.name().to_owned()),
                    map.steam_id().unwrap_or("")
                )
                .as_str(),
            ),
            None => GString::new(),
        }
    }
}

/// Plain metadata of one registered map (`maps_list` row fields).
fn map_row_fields(map: &Map) -> (String, String, bool, String, String, i32, i32) {
    (
        map.name().to_owned(),
        map.file.display().to_string(),
        map.custom,
        map.author().to_owned(),
        map.description().to_owned(),
        map.width,
        map.height,
    )
}

/// `map.file.lastModified()` in milliseconds — `MapPriority.recent`'s primary
/// key (`MapListDialog.java:48`). Java's built-in maps are classpath entries
/// whose `Fi.lastModified()` is 0, so non-file-backed maps report 0 here and
/// the real millisecond mtime is used for custom/mod/workshop files (Java's
/// comparator drops the sub-millisecond digits, so clients on the same files
/// order identically).
fn map_row_last_modified(map: &Map) -> i64 {
    if !map.custom && !map.workshop && map.mod_id.is_none() {
        return 0;
    }
    std::fs::metadata(&map.file)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Binds a [`PreviewImage`] to a Godot `Texture2D` (RGBA8).
pub(crate) fn bind_texture(image: &PreviewImage) -> Option<Gd<Texture2D>> {
    let data = PackedByteArray::from(image.rgba.as_slice());
    let godot_image = Image::create_from_data(
        image.width as i32,
        image.height as i32,
        false,
        godot::classes::image::Format::RGBA8,
        &data,
    )?;
    ImageTexture::create_from_image(&godot_image).map(|texture| texture.upcast::<Texture2D>())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use mind_core::io::StringMap;

    fn fixture_map() -> Map {
        let mut tags = StringMap::new();
        tags.insert("name".to_owned(), "alpha".to_owned());
        tags.insert("author".to_owned(), "tester".to_owned());
        tags.insert("description".to_owned(), "a test map".to_owned());
        Map::new(
            PathBuf::from("/tmp/maps/alpha.msav"),
            64,
            32,
            tags,
            true,
            1,
            -1,
        )
    }

    #[test]
    fn map_row_fields_carry_registry_metadata() {
        let (name, path, custom, author, description, width, height) =
            map_row_fields(&fixture_map());
        assert_eq!(name, "alpha");
        assert_eq!(path, "/tmp/maps/alpha.msav");
        assert!(custom);
        assert_eq!(author, "tester");
        assert_eq!(description, "a test map");
        assert_eq!((width, height), (64, 32));
    }

    /// T3: the row's `lastModified` is the file mtime in **milliseconds** for
    /// custom maps (Java `Fi.lastModified`, `MapListDialog.java:48`) and 0 for
    /// classpath-style built-ins and missing files.
    #[test]
    fn map_row_last_modified_is_ms_for_files_and_zero_for_builtins() {
        let dir = std::env::temp_dir().join(format!("mind-map-mtime-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("alpha.msav");
        std::fs::write(&file, b"x").expect("write");
        let expected = std::fs::metadata(&file)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);
        assert!(expected > 0, "temp file has a real mtime");

        let mut custom = fixture_map();
        custom.file = file.clone();
        custom.custom = true;
        assert_eq!(map_row_last_modified(&custom), expected);

        let mut builtin = fixture_map();
        builtin.file = file.clone();
        builtin.custom = false;
        assert_eq!(
            map_row_last_modified(&builtin),
            0,
            "classpath built-ins report 0"
        );

        let mut missing = fixture_map();
        missing.file = dir.join("missing.msav");
        assert_eq!(map_row_last_modified(&missing), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
