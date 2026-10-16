// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindEditor` — the plan-19 editor facade (M3 Godot shell).
//!
//! A Godot-free `mind_core::editor::MapEditor` plus an owned editor world
//! ([`WorldGrid`] + [`ContentRegistry`] + `bevy_ecs::World`) wrapped in a thin
//! `#[func]` surface for GDScript and the MCP oracle. All editor rules live in
//! `mind-core` (OD19-B); this node owns only view handling (pan/zoom/projection)
//! and the type conversions across the Godot boundary.
//!
//! Declared in `res://scenes/game.tscn` at `/root/Spine/MindEditor` (HLP §6.6)
//! rather than as a project autoload singleton. In-engine MCP §7c-1 verification
//! is deferred to the orchestrator's single-editor mutex; this class compiles
//! and exposes the API surface today.
//!
//! Milestone scope: M3 covers the state/command API, palette data, map view and
//! the op-log oracle. Meta dialogs (M4), objectives/waves (M5), processors/
//! locales/assets (M6) and save/playtest/export/exit (M7) append to this facade
//! in their own milestones.

use std::path::PathBuf;

use godot::builtin::{
    Array, GString, PackedInt64Array, PackedVector2Array, VarDictionary, Variant, Vector2,
};
use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Node, ProjectSettings, Texture2D};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::content::{
    BlockDef, BlockId, BlockKind, BuildVisibility, Category, ContentRegistry, GeneratorKind,
    MemoryBundle, MemoryUnlockStore,
};
use mind_core::determinism::Checksummer;
use mind_core::ecs::MindWorld;
use mind_core::editor::EditorGrid as _;
use mind_core::editor::banned;
use mind_core::editor::draw_op::DrawOperation;
use mind_core::editor::grid::WorldEditorGrid;
use mind_core::editor::lifecycle::try_catch_map_error;
use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map_with_ecs};
use mind_core::editor::objectives::{FieldFlags, FieldKind};
use mind_core::editor::playtest::EditorPlayState;
use mind_core::editor::{BRUSH_SIZES, EditorTool, EditorWorldHooks, MapEditor};
use mind_core::game::play::PlaySession;
use mind_core::game::team::Team;
use mind_core::io::fs::{FileSystem, NativeFs};
use mind_core::io::map::{BlockPalette, MapIo};
use mind_core::logic::blocks::LogicBlockState;
use mind_core::maps::Map;
use mind_core::maps::filters::BlockPredicate;
use mind_core::maps::filters::block_info;
use mind_core::world::{BlockKindData, BlockTable, NoopRenderHooks, WorldGrid};

mod editor_renderer;
mod map_view;
pub mod preview;

pub use editor_renderer::MindEditorRenderer;
use map_view::{MapViewDriver, brush_polygon};
pub use preview::MindPreview;

/// Builds a `{GString: Variant}` dictionary (mirrors the other facades).
fn dict(pairs: Vec<(&str, Variant)>) -> VarDictionary {
    let mut out = VarDictionary::new();
    for (key, value) in pairs {
        out.set(&GString::from(key), &value);
    }
    out
}

/// Parses a `spawns` JSON tag into spawn groups (invalid entries skipped).
fn parse_spawn_groups(json: Option<&str>) -> Vec<mind_core::game::spawn_group::SpawnGroup> {
    let Some(json) = json else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| mind_core::game::spawn_group::SpawnGroup::from_json(item).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `MindEditor` — editor state/command facade for GDScript + MCP.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindEditor {
    base: Base<Node>,
    editor: MapEditor,
    grid: WorldGrid,
    content: Option<ContentRegistry>,
    ecs: MindWorld,
    hooks: EditorWorldHooks,
    render: NoopRenderHooks,
    file: Option<PathBuf>,
    last_error_key: Option<String>,
    last_error_message: Option<String>,
    view: MapViewDriver,
    play: EditorPlayState,
    play_session: PlaySession,
    /// Minimap palette with `sprites/block_colors.png` applied, lazily built
    /// for the generate-dialog preview (`ContentLoader.loadColors`).
    palette: Option<BlockPalette>,
    /// Monotonic tile-edit counter (`MindEditorRenderer` invalidation).
    tiles_rev: u64,
}

#[godot_api]
impl INode for MindEditor {
    fn init(base: Base<Node>) -> Self {
        // The content registry builds in `bootstrap()`: the main scene is
        // instantiated before the autoloads' `_ready`, and the boot path reads
        // `.mods` from the `MindMods` autoload (empty until then).
        Self {
            base,
            editor: MapEditor::new(),
            grid: WorldGrid::new(0, 0),
            content: None,
            ecs: MindWorld::new(),
            hooks: EditorWorldHooks::new(),
            render: NoopRenderHooks,
            file: None,
            last_error_key: None,
            last_error_message: None,
            view: MapViewDriver::new(),
            play: EditorPlayState::new(),
            play_session: PlaySession::default(),
            palette: None,
            tiles_rev: 0,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

impl MindEditor {
    /// Rebuilds the node's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        // I07 T4: the one content boot path (base content + enabled mods);
        // Java `ClientLauncher.java:166-189`. Runs here, not in `init`, because
        // the main scene instantiates before the autoloads' `_ready`.
        if self.content.is_none() {
            match crate::content_boot::build_registry(
                &MemoryBundle::new(),
                &MemoryUnlockStore::new(),
            ) {
                Ok(registry) => {
                    self.editor.reset(&registry);
                    self.content = Some(registry);
                }
                Err(error) => {
                    log::warn!("MindEditor content boot failed: {error}");
                }
            }
        }
        // Logic-block placements carry `LogicBlockState`
        // (`LogicBlockBehavior.create_state`; the editor ECS holds no behavior
        // table, so the seeds come from the runtime `BlockTable`). The table
        // also becomes an ECS resource so `logic_def_of`/`privileged_of`
        // resolve for editor processors (`logic_get_state`,
        // `logic_configure_code`).
        if let Some(content) = self.content.as_ref()
            && let Ok(table) = BlockTable::build_default(content)
        {
            let mut seeds = Vec::new();
            for def in content.blocks() {
                if def.kind != BlockKind::LogicBlock {
                    continue;
                }
                if let Some(instance) = table.get(def.id)
                    && let BlockKindData::Logic(logic) = &instance.kind_data
                {
                    seeds.push((def.id, logic.ipt, logic.privileged));
                }
            }
            self.hooks.set_logic_seeds(seeds);
            self.ecs.0.insert_resource(table);
        }
        let blocks = self.palette_blocks(GString::new()).len();
        log::info!("MindEditor ready ({blocks} editor blocks)");
    }

    /// Resolves a Godot `res://`/`user://` path to a native path.
    fn native_path(raw: &str) -> PathBuf {
        if raw.starts_with("res://") || raw.starts_with("user://") {
            PathBuf::from(ProjectSettings::singleton().globalize_path(raw).to_string())
        } else {
            PathBuf::from(raw)
        }
    }

    /// Records a `MapError` for the inspector / UI (`tryCatchMapError`).
    fn set_map_error(&mut self, error: &mind_core::maps::MapError) {
        let (key, message) = try_catch_map_error(error);
        self.last_error_key = Some(key.to_owned());
        self.last_error_message = Some(message);
    }

    /// Clears the last error after a successful command.
    fn clear_error(&mut self) {
        self.last_error_key = None;
        self.last_error_message = None;
    }

    /// Marks the tile grid as changed (editor-renderer invalidation;
    /// `EditorTile.setBlock`'s `ui.editor.resetSaved()`).
    fn mark_tiles_changed(&mut self) {
        self.tiles_rev = self.tiles_rev.wrapping_add(1);
        self.editor.saved = false;
    }

    /// Read-only grid for the editor renderer (crate-internal view seam).
    pub(crate) fn grid_ref(&self) -> &WorldGrid {
        &self.grid
    }

    /// Read-only content registry for the editor renderer.
    pub(crate) fn content_ref(&self) -> Option<&ContentRegistry> {
        self.content.as_ref()
    }

    /// Read-only ECS (building teams/rotations) for the editor renderer.
    pub(crate) fn ecs_ref(&self) -> &MindWorld {
        &self.ecs
    }
}

#[godot_api]
impl MindEditor {
    // --- state ---

    /// Active tool name (`v/i/l/b/e/g/r` tools).
    #[func]
    pub fn tool(&self) -> GString {
        GString::from(self.editor.tool.name())
    }

    /// Selects a tool by name; returns whether the name resolved.
    #[func]
    pub fn set_tool(&mut self, name: GString) -> bool {
        match EditorTool::from_name(&name.to_string()) {
            Some(tool) => {
                self.editor.tool = tool;
                true
            }
            None => false,
        }
    }

    /// Active tool's alternate mode (`-1` = standard).
    #[func]
    pub fn tool_mode(&self) -> i64 {
        self.editor.tool_modes[self.editor.tool.index()] as i64
    }

    /// Sets the active tool's alternate mode.
    #[func]
    pub fn set_tool_mode(&mut self, mode: i64) {
        let index = self.editor.tool.index();
        self.editor.tool_modes[index] = mode.clamp(-1, 4) as i32;
    }

    /// Brush radius (`BRUSH_SIZES` entry).
    #[func]
    pub fn brush_size(&self) -> f64 {
        self.editor.brush_size as f64
    }

    /// Sets the brush radius (clamped to the upstream `1..=20` range).
    #[func]
    pub fn set_brush_size(&mut self, radius: f64) {
        self.editor.brush_size =
            (radius as f32).clamp(BRUSH_SIZES[0], BRUSH_SIZES[BRUSH_SIZES.len() - 1]);
    }

    /// Current draw-block content name.
    #[func]
    pub fn draw_block(&self) -> GString {
        match self.content.as_ref() {
            Some(content) => content
                .block(self.editor.draw_block)
                .map(|def| GString::from(def.name.as_str()))
                .unwrap_or_else(|| GString::from("air")),
            None => GString::from("air"),
        }
    }

    /// Selects the draw block by content name.
    #[func]
    pub fn set_draw_block(&mut self, name: GString) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        match content.block_id(&name.to_string()) {
            Some(id) => {
                self.editor.draw_block = id;
                true
            }
            None => false,
        }
    }

    /// Current draw team id.
    #[func]
    pub fn draw_team(&self) -> i64 {
        self.editor.draw_team as i64
    }

    /// Selects the draw team by name (base teams).
    #[func]
    pub fn set_draw_team(&mut self, name: GString) -> bool {
        let requested = name.to_string();
        if let Some(team) = Team::base_teams()
            .iter()
            .find(|team| team.name.eq_ignore_ascii_case(&requested))
        {
            self.editor.draw_team = team.id;
            true
        } else {
            false
        }
    }

    /// Placement rotation `0..=3`.
    #[func]
    pub fn rotation(&self) -> i64 {
        self.editor.rotation as i64
    }

    /// Sets the placement rotation (`Mathf.mod(rotation, 4)`).
    #[func]
    pub fn set_rotation(&mut self, rotation: i64) {
        self.editor.rotation = rotation.rem_euclid(4) as i32;
    }

    /// Whether terrain (walls) is shown.
    #[func]
    pub fn show_terrain(&self) -> bool {
        self.editor.show_terrain
    }

    /// Toggles terrain visibility (`showterrain`).
    #[func]
    pub fn set_show_terrain(&mut self, show: bool) {
        self.editor.show_terrain = show;
    }

    /// Whether floors are shown.
    #[func]
    pub fn show_floor(&self) -> bool {
        self.editor.show_floor
    }

    /// Toggles floor visibility (`showfloor`).
    #[func]
    pub fn set_show_floor(&mut self, show: bool) {
        self.editor.show_floor = show;
    }

    /// Whether buildings are shown.
    #[func]
    pub fn show_buildings(&self) -> bool {
        self.editor.show_buildings
    }

    /// Toggles building visibility (`showblocks`).
    #[func]
    pub fn set_show_buildings(&mut self, show: bool) {
        self.editor.show_buildings = show;
    }

    /// Grid overlay state.
    #[func]
    pub fn grid(&self) -> bool {
        self.view.grid
    }

    /// Toggles the grid overlay (`ctrl+g`).
    #[func]
    pub fn set_grid(&mut self, enabled: bool) {
        self.view.grid = enabled;
        self.editor.grid = enabled;
    }

    /// Whether an undo is available.
    #[func]
    pub fn can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    /// Whether a redo is available.
    #[func]
    pub fn can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    /// Undoes one operation.
    #[func]
    pub fn undo(&mut self) {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return;
        };
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        self.editor.undo(&mut world, content);
    }

    /// Redoes one operation.
    #[func]
    pub fn redo(&mut self) {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return;
        };
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        self.editor.redo(&mut world, content);
    }

    /// A copy of `editor.tags` (name/description/author/rules/genfilters/...).
    #[func]
    pub fn tags(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        for (key, value) in &self.editor.tags {
            out.set(
                &GString::from(key.as_str()),
                &GString::from(value.as_str()).to_variant(),
            );
        }
        out
    }

    /// Sets one `editor.tags` entry.
    #[func]
    pub fn set_tag(&mut self, key: GString, value: GString) {
        self.editor.tags.insert(key.to_string(), value.to_string());
    }

    // --- banned content (`BannedContentDialog`) ---

    /// Parses `"block"`/`"unit"` (everything else means blocks).
    fn ban_kind(kind: &str) -> banned::BanKind {
        if kind == "unit" {
            banned::BanKind::Unit
        } else {
            banned::BanKind::Block
        }
    }

    /// `BannedContentDialog` picker payload `{categories, rows}`.
    ///
    /// Rows are every content type entry passing Java's `pred` (`Block::canBeBuilt`,
    /// `BannedContentDialog.java:36`; `!UnitType.isHidden()`, `:37`) in content-id
    /// order, `{name, id, category, ui_icon}`. `categories` is the block filter
    /// row (`Category.values()`, `:73`) and empty for units.
    #[func]
    pub fn bannable_content_json(&self, kind: GString) -> GString {
        let empty = "{\"categories\":[],\"rows\":[]}";
        let Some(content) = self.content.as_ref() else {
            return GString::from(empty);
        };
        let kind = Self::ban_kind(&kind.to_string());
        let rows: Vec<serde_json::Value> = match kind {
            banned::BanKind::Block => content
                .blocks()
                .iter()
                .filter(|def| {
                    !matches!(
                        def.build_visibility,
                        BuildVisibility::Hidden | BuildVisibility::DebugOnly
                    )
                })
                .map(|def| {
                    serde_json::json!({
                        "name": def.name,
                        "id": def.id.raw(),
                        "category": def.category.name(),
                        "ui_icon": format!("block-{}-ui", def.name),
                    })
                })
                .collect(),
            banned::BanKind::Unit => content
                .units()
                .iter()
                .filter(|unit| !unit.is_hidden())
                .map(|unit| {
                    serde_json::json!({
                        "name": unit.name,
                        "id": unit.id.raw(),
                        "category": "",
                        "ui_icon": format!("unit-{}-ui", unit.name),
                    })
                })
                .collect(),
        };
        let categories: Vec<&str> = match kind {
            banned::BanKind::Block => Category::ALL
                .iter()
                .map(|category| category.name())
                .collect(),
            banned::BanKind::Unit => Vec::new(),
        };
        let payload = serde_json::json!({ "categories": categories, "rows": rows });
        GString::from(
            serde_json::to_string(&payload)
                .unwrap_or_else(|_| empty.to_owned())
                .as_str(),
        )
    }

    /// The map's banned set for `kind` from the `rules` tag
    /// (`state.rules.bannedBlocks`/`bannedUnits`), as a JSON array.
    #[func]
    pub fn banned_names_json(&self, kind: GString) -> GString {
        let kind = Self::ban_kind(&kind.to_string());
        let tag = self.editor.tags.get("rules").cloned().unwrap_or_default();
        let names = banned::names_from_tag_json(&tag, kind);
        GString::from(
            serde_json::to_string(&names)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// Replaces the banned set in the `rules` tag, preserving every other key
    /// (`BannedContentDialog` mutates the live `ObjectSet`; `CustomRulesDialog.java:189,210`).
    #[func]
    pub fn set_banned_names_json(&mut self, kind: GString, names_json: GString) -> bool {
        let kind = Self::ban_kind(&kind.to_string());
        let names: Vec<String> = match serde_json::from_str(&names_json.to_string()) {
            Ok(names) => names,
            Err(_) => return false,
        };
        let tag = self.editor.tags.get("rules").cloned().unwrap_or_default();
        match banned::apply_to_tag_json(&tag, kind, &names) {
            Ok(updated) => {
                self.editor.tags.insert("rules".to_owned(), updated);
                self.clear_error();
                true
            }
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                false
            }
        }
    }

    /// Monotonic tile-edit counter (`MindEditorRenderer` invalidation).
    #[func]
    pub fn tiles_revision(&self) -> i64 {
        self.tiles_rev as i64
    }

    /// Concise editor status for the inspector / MCP oracle.
    #[func]
    pub fn status(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        out.set(&key("tool"), &self.tool().to_variant());
        out.set(&key("tool_mode"), &self.tool_mode().to_variant());
        out.set(&key("brush_size"), &self.brush_size().to_variant());
        out.set(&key("draw_block"), &self.draw_block().to_variant());
        out.set(&key("draw_team"), &self.draw_team().to_variant());
        out.set(&key("rotation"), &self.rotation().to_variant());
        out.set(&key("show_terrain"), &self.show_terrain().to_variant());
        out.set(&key("show_floor"), &self.show_floor().to_variant());
        out.set(&key("show_buildings"), &self.show_buildings().to_variant());
        out.set(&key("grid"), &self.grid().to_variant());
        out.set(&key("width"), &(self.grid.width() as i64).to_variant());
        out.set(&key("height"), &(self.grid.height() as i64).to_variant());
        out.set(&key("can_undo"), &self.can_undo().to_variant());
        out.set(&key("can_redo"), &self.can_redo().to_variant());
        out.set(
            &key("retained_ops"),
            &(self.editor.retained_ops() as i64).to_variant(),
        );
        out.set(
            &key("current_ops"),
            &(self.editor.ops() as i64).to_variant(),
        );
        out.set(&key("saved"), &self.editor.saved.to_variant());
        out.set(&key("loading"), &self.editor.is_loading().to_variant());
        out.set(
            &key("file"),
            &GString::from(
                self.file
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
                    .as_str(),
            )
            .to_variant(),
        );
        out.set(
            &key("last_error"),
            &GString::from(self.last_error_key.as_deref().unwrap_or("")).to_variant(),
        );
        out
    }

    /// Last map error as `{Key, message}` (`{}` when none).
    #[func]
    pub fn last_error(&self) -> VarDictionary {
        match (&self.last_error_key, &self.last_error_message) {
            (Some(key), Some(message)) => dict(vec![
                ("Key", GString::from(key.as_str()).to_variant()),
                ("message", GString::from(message.as_str()).to_variant()),
            ]),
            _ => VarDictionary::new(),
        }
    }

    // --- palette (behaviour: filter + upstream sort, plan 19 §3.10) ---

    /// Editor block palette rows, filtered by `search` and sorted upstream:
    /// `(is_core desc, synthetic asc, is_overlay asc, id asc)`.
    #[func]
    pub fn palette_blocks(&self, search: GString) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(content) = self.content.as_ref() else {
            return out;
        };
        let needle = search.to_string().to_ascii_lowercase();
        let mut rows: Vec<&BlockDef> = content
            .blocks()
            .iter()
            .filter(|def| def.in_editor && def.build_visibility != BuildVisibility::DebugOnly)
            .filter(|def| {
                needle.is_empty() || def.name.to_ascii_lowercase().contains(needle.as_str())
            })
            .collect();
        rows.sort_by(|a, b| {
            let a_core = a.kind == BlockKind::CoreBlock;
            let b_core = b.kind == BlockKind::CoreBlock;
            b_core
                .cmp(&a_core)
                .then_with(|| block_info::synthetic(a).cmp(&block_info::synthetic(b)))
                .then_with(|| block_info::is_overlay(a).cmp(&block_info::is_overlay(b)))
                .then_with(|| a.id.raw().cmp(&b.id.raw()))
        });
        for def in rows {
            out.push(&palette_row(def));
        }
        out
    }

    /// One block's editor metadata (`ui_icon`, config/size flags) by name.
    #[func]
    pub fn block_info(&self, name: GString) -> VarDictionary {
        let Some(content) = self.content.as_ref() else {
            return VarDictionary::new();
        };
        match content.block_by_name(&name.to_string()) {
            Some(def) => palette_row(def),
            None => VarDictionary::new(),
        }
    }

    /// Base teams for the toolbar (`[{id, name, color}]`).
    #[func]
    pub fn palette_teams(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for team in Team::base_teams() {
            let mut row = VarDictionary::new();
            row.set(&GString::from("id"), &(team.id as i64).to_variant());
            row.set(
                &GString::from("name"),
                &GString::from(team.name.as_str()).to_variant(),
            );
            row.set(
                &GString::from("color"),
                &team.color.to_rgba8888().to_variant(),
            );
            out.push(&row);
        }
        out
    }

    /// Available brush radii.
    #[func]
    pub fn brush_sizes(&self) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        for size in BRUSH_SIZES {
            out.push((size * 10.0) as i64);
        }
        out
    }

    // --- lifecycle (M1 model; M3 facade) ---

    /// `begin_new` / `beginEditSize`: a fresh stone-floored `width x height` map.
    #[func]
    pub fn begin_new(&mut self, width: i32, height: i32) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let width = width.clamp(1, 16_383);
        let height = height.clamp(1, 16_383);
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        self.editor
            .begin_edit_size(&mut world, content, width, height);
        self.editor.adopt_world(&mut world);
        self.file = None;
        self.clear_error();
        true
    }

    /// `begin_edit_map`: loads a `.msav` into the editor.
    #[func]
    pub fn begin_edit_map(&mut self, file: GString) -> bool {
        self.mark_tiles_changed();
        let path = Self::native_path(&file.to_string());
        let fs = NativeFs;
        let header = match MapIo::create_map(&fs, &path, true) {
            Ok(header) => header,
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        let map = Map::from_header(&header, true);
        let Some(content) = self.content.as_mut() else {
            return false;
        };
        match self
            .editor
            .begin_edit_map(&mut self.grid, content, &fs, &map)
        {
            Ok(()) => {
                self.file = Some(path);
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_map_error(&error);
                false
            }
        }
    }

    /// `begin_edit_image`: imports a color-mapped PNG map (≤800 per axis).
    #[func]
    pub fn begin_edit_image(&mut self, file: GString) -> bool {
        self.mark_tiles_changed();
        let path = Self::native_path(&file.to_string());
        let fs = NativeFs;
        let bytes = match fs.read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        let image = match mind_core::io::map::decode_png(&bytes) {
            Ok(image) => image,
            Err(error) => {
                self.last_error_key = Some("@editor.errorimage".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        if image.width > 800 || image.height > 800 {
            self.last_error_key = Some("@editor.errorimage".to_owned());
            self.last_error_message = Some("image map exceeds 800x800".to_owned());
            return false;
        }
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        match self
            .editor
            .begin_edit_image(&mut self.grid, content, &image)
        {
            Ok(()) => {
                self.file = Some(path);
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_map_error(&error);
                false
            }
        }
    }

    /// `MapResizeDialog` OK: resize + shift (clears the op stack).
    #[func]
    pub fn resize_map(&mut self, width: i32, height: i32, shift_x: i32, shift_y: i32) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let width = width.clamp(1, 16_383);
        let height = height.clamp(1, 16_383);
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        self.editor
            .resize(&mut world, content, width, height, shift_x, shift_y);
        true
    }

    // --- M4/M5/M6 dialog data (plan 19 M4-M6) ---

    /// `MapGenerateDialog.filters`: the `genfilters` tag JSON, or the default
    /// stack when unset.
    #[func]
    pub fn filters_json(&self) -> GString {
        if let Some(tag) = self.editor.tags.get("genfilters")
            && !tag.trim().is_empty()
        {
            return GString::from(tag.as_str());
        }
        match self.content.as_ref() {
            Some(content) => GString::from(
                mind_core::maps::filters::write_filters(
                    content,
                    &mind_core::maps::filters::default_filter_stack(content),
                )
                .as_str(),
            ),
            None => GString::from("[]"),
        }
    }

    /// Stores the `genfilters` tag from the generate dialog.
    #[func]
    pub fn set_filters_json(&mut self, json: GString) -> bool {
        // GDScript JSON round-trips turn integral numbers into floats; restore
        // the `int` fields before the tag keeps an unparsable `"seed": 0.0`.
        let normalized = mind_core::maps::filters::normalize_filter_numbers(&json.to_string());
        self.editor.tags.insert("genfilters".to_owned(), normalized);
        self.clear_error();
        true
    }

    /// `MapGenerateDialog.applyToEditor`: run the tag filters over the world and
    /// clear the op stack.
    #[func]
    pub fn apply_filters(&mut self) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let tag = self.editor.tags.get("genfilters").cloned();
        let mut filters = mind_core::editor::generate::filters_from_tag(content, tag.as_deref());
        mind_core::editor::generate::apply_filters_to_editor(
            &mut self.editor,
            &mut self.grid,
            content,
            &mut filters,
            0,
        );
        self.clear_error();
        true
    }

    /// `MapGenerateDialog.rebuildFilters`: one row per tag filter with its
    /// `FilterOption` descriptors and the resolved values (`filter.options()`
    /// plus each option's getter, class defaults merged).
    #[func]
    pub fn filter_options_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let tag = self
            .editor
            .tags
            .get("genfilters")
            .cloned()
            .unwrap_or_default();
        GString::from(mind_core::maps::filters::filter_options_json(content, &tag).as_str())
    }

    /// `MapGenerateDialog.showAdd`: `Maps.allFilterTypes` rows (class tag,
    /// `simpleName`, icon block, `isPost`).
    #[func]
    pub fn filter_types_json(&self) -> GString {
        GString::from(mind_core::maps::filters::filter_types_json().as_str())
    }

    /// `Maps.addDefaultOres` as a `genfilters` JSON fragment (the `@add`
    /// picker's "Default Ores" row).
    #[func]
    pub fn default_ores_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let filters = mind_core::maps::filters::default_ore_filters(content);
        GString::from(mind_core::maps::filters::write_filters(content, &filters).as_str())
    }

    /// `FilterOption.BlockOption` picker rows: every block the named predicate
    /// accepts (`content.blocks()` + `Boolf<Block>`), in content-id order.
    #[func]
    pub fn filter_blocks_json(&self, predicate: GString) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let Some(predicate) = BlockPredicate::from_name(&predicate.to_string()) else {
            return GString::from("[]");
        };
        let rows: Vec<serde_json::Value> = content
            .blocks()
            .iter()
            .filter(|def| predicate.test(content, def.id))
            .map(|def| {
                serde_json::json!({
                    "name": def.name,
                    "id": def.id.raw(),
                    "ui_icon": format!("block-{}-ui", def.name),
                })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// `MapGenerateDialog.update()`: the live filter preview over the editor
    /// grid, colored with the map-preview palette (`MapIO.colorFor`).
    #[func]
    pub fn generate_preview_texture(&mut self) -> Option<Gd<Texture2D>> {
        if self.palette.is_none() {
            let content = self.content.as_ref()?;
            let mut palette = BlockPalette::of(content);
            palette.apply_block_colors(&crate::render::minimap::load_block_colors());
            self.palette = Some(palette);
        }
        let snapshot = mind_core::editor::generate::EditorSnapshot::capture(&self.grid);
        let tag = self.editor.tags.get("genfilters").cloned();
        let content = self.content.as_ref()?;
        let palette = self.palette.as_ref()?;
        let mut filters = mind_core::editor::generate::filters_from_tag(content, tag.as_deref());
        let image = mind_core::editor::generate::generate_preview_with_palette(
            &snapshot,
            &mut filters,
            content,
            palette,
            0,
        );
        preview::bind_texture(&image)
    }

    /// `SectorGenerateDialog.apply`: generation is owned by the planet/sector
    /// host (plan 06 `WorldGrid::load_sector`); the dialogue records the request
    /// and reports deferred until that host is wired in-engine.
    #[func]
    pub fn sector_generate(&mut self, planet: GString, sector: i64, seed: i64) -> bool {
        let planet_name = planet.to_string();
        let Ok(sector_id) = u16::try_from(sector) else {
            self.last_error_key = Some("@editor.sectorgenerate".to_owned());
            self.last_error_message = Some(format!("invalid sector {sector}"));
            return false;
        };
        let seed_offset = seed.max(0) as u64;
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        // `World.loadSector` -> `PlanetGenerator.generate` (the structure
        // passes stay sim-side; the editor gets the generated terrain).
        let extras = mind_core::game::planet::static_gen_extras(content, &planet_name, sector_id);
        let Some(generated) = mind_core::maps::planet::generate_sector_with_seed(
            &planet_name,
            sector_id,
            content,
            extras,
            seed_offset,
        ) else {
            self.last_error_key = Some("@editor.sectorgenerate".to_owned());
            self.last_error_message = Some(format!("no generator for `{planet_name}`"));
            return false;
        };
        let generated_rules = generated.rules.clone();
        // `editor.load(() -> world.loadSector(...))` + `logic.reset()`: the
        // generated grid replaces the editor world and its entities.
        self.grid = generated.grid;
        self.ecs = MindWorld::new();
        // Java writes the generated `state.rules`; the port's editor keeps the
        // map rules in the `rules` tag, and clears `sector` + the extra filters
        // (`state.rules.sector = null`, `editor.tags.put("genfilters", "{}")`).
        if let Ok(json) = mind_core::io::json::JsonIo::write(&generated_rules) {
            self.editor.tags.insert("rules".to_owned(), json);
        }
        self.editor
            .tags
            .insert("genfilters".to_owned(), "{}".to_owned());
        self.editor.tags.shift_remove("sector");
        self.file = None;
        self.editor.saved = false;
        self.mark_tiles_changed();
        self.clear_error();
        true
    }

    /// `SectorGenerateDialog`'s planet picker (`:39-41`): every planet with a
    /// generator, non-empty sectors and `accessible`.
    #[func]
    pub fn sector_planets_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let rows: Vec<serde_json::Value> = content
            .planets()
            .iter()
            .filter(|def| {
                def.generator != GeneratorKind::None && !def.sectors.is_empty() && def.accessible
            })
            .map(|def| {
                serde_json::json!({
                    "name": def.name,
                    "sectors": def.sectors.len(),
                })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// `Sector.getSize()` for the picker's `[ NxN ]` label
    /// (`Sector.java:203-205`); `0` when the planet/cell is unknown.
    #[func]
    pub fn sector_size(&self, planet: GString, sector: i64) -> i64 {
        let Some(content) = self.content.as_ref() else {
            return 0;
        };
        let Ok(sector_id) = u16::try_from(sector) else {
            return 0;
        };
        mind_core::maps::planet::sector_size(&planet.to_string(), sector_id, content)
            .map(|size| size as i64)
            .unwrap_or(0)
    }

    /// `MapObjectivesDialog`'s type table
    /// (`MapObjectives.allObjectiveTypes` + `#[derive(ObjectiveFields)]`):
    /// every objective class with its editor field descriptors. The
    /// `kind`/`flags` tags are the GDScript ABI (`content:item`, `vec2i`,
    /// `seq:int`, flags `second`/`tilepos`/`multiline`/`logiccode`/
    /// `researchable`/`synthetic`/`hidden`).
    #[func]
    pub fn objective_classes_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = mind_core::editor::objectives::all_class_tags()
            .iter()
            .map(|tag| {
                let fields: Vec<serde_json::Value> =
                    mind_core::editor::objectives::class_fields(tag)
                        .iter()
                        .map(|field| {
                            serde_json::json!({
                                "name": field.name,
                                "kind": objective_kind_tag(&field.kind),
                                "flags": objective_flag_tags(field.flags),
                            })
                        })
                        .collect();
                serde_json::json!({ "tag": tag, "fields": fields })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// A fresh objective of class `tag` with Java's provider defaults
    /// (`MapObjectivesDialog`'s `getProvider(MapObjective.class)`, `:213-240`)
    /// plus the canvas default `editorPos`.
    #[func]
    pub fn objective_default_json(&self, tag: GString) -> GString {
        let tag = tag.to_string();
        let mut map = serde_json::Map::new();
        map.insert("class".to_owned(), serde_json::Value::String(tag.clone()));
        let fields = mind_core::editor::objectives::class_fields(&tag);
        if fields.is_empty() {
            return GString::from("{}");
        }
        for field in fields {
            map.insert(
                field.name.to_owned(),
                mind_core::editor::objectives::provider_default(&field.kind, self.content.as_ref()),
            );
        }
        map.insert(
            "editorPos".to_owned(),
            serde_json::Value::from(mind_core::io::json::objectives::default_editor_pos()),
        );
        GString::from(
            serde_json::to_string(&serde_json::Value::Object(map))
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// `MapObjectivesDialog`'s marker type table (`MapObjectives.allMarkerTypes`)
    /// with the same field-descriptor shape as `objective_classes_json`
    /// (`ObjectiveMarker` subclass reflection in `defaultInterpreter`).
    #[func]
    pub fn marker_classes_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = mind_core::editor::objectives::all_marker_tags()
            .iter()
            .map(|tag| {
                let fields: Vec<serde_json::Value> =
                    mind_core::editor::objectives::marker_fields(tag)
                        .iter()
                        .map(|field| {
                            serde_json::json!({
                                "name": field.name,
                                "kind": objective_kind_tag(&field.kind),
                                "flags": objective_flag_tags(field.flags),
                            })
                        })
                        .collect();
                serde_json::json!({ "tag": tag, "fields": fields })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// A fresh marker of class `tag` with the Java field defaults
    /// (`getProvider(ObjectiveMarker.class)`, `MapObjectivesDialog.java:236-256`).
    #[func]
    pub fn marker_default_json(&self, tag: GString) -> GString {
        let value = mind_core::editor::objectives::marker_default_json(&tag.to_string());
        GString::from(
            serde_json::to_string(&value)
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// `MapObjectivesDialog`: the objectives JSON (`MapInfoDialog` route).
    #[func]
    pub fn objectives_json(&self) -> GString {
        match self.editor.tags.get("objectives") {
            Some(json) if !json.trim().is_empty() => GString::from(json.as_str()),
            _ => GString::from("[]"),
        }
    }

    /// Stores the objectives JSON.
    #[func]
    pub fn set_objectives_json(&mut self, json: GString) -> bool {
        let parsed = mind_core::editor::objectives::parse_objectives(&json.to_string());
        match parsed {
            Ok(objectives) => match mind_core::editor::objectives::write_objectives(&objectives) {
                Ok(canonical) => {
                    self.editor.tags.insert("objectives".to_owned(), canonical);
                    self.clear_error();
                    true
                }
                Err(error) => {
                    self.last_error_key = Some("@editor.errorload".to_owned());
                    self.last_error_message = Some(error.to_string());
                    false
                }
            },
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                false
            }
        }
    }

    /// `WaveInfoDialog`: the `spawns` JSON.
    #[func]
    pub fn waves_json(&self) -> GString {
        match self.editor.tags.get("spawns") {
            Some(json) if !json.trim().is_empty() => GString::from(json.as_str()),
            _ => GString::from("[]"),
        }
    }

    /// Stores the `spawns` JSON after validating each group.
    #[func]
    pub fn set_waves_json(&mut self, json: GString) -> bool {
        let value: serde_json::Value = match serde_json::from_str(&json.to_string()) {
            Ok(value) => value,
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                return false;
            }
        };
        let Some(items) = value.as_array() else {
            self.last_error_key = Some("@editor.errorload".to_owned());
            self.last_error_message = Some("spawns must be an array".to_owned());
            return false;
        };
        for item in items {
            if let Err(error) = mind_core::game::spawn_group::SpawnGroup::from_json(item) {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error);
                return false;
            }
        }
        self.editor
            .tags
            .insert("spawns".to_owned(), json.to_string());
        self.clear_error();
        true
    }

    /// `MapLocalesDialog`: the `locales` JSON.
    #[func]
    pub fn locales_json(&self) -> GString {
        match self.editor.tags.get("locales") {
            Some(json) if !json.trim().is_empty() => GString::from(json.as_str()),
            _ => GString::from("{}"),
        }
    }

    /// Stores the `locales` JSON after validating it parses.
    #[func]
    pub fn set_locales_json(&mut self, json: GString) -> bool {
        match mind_core::maps::locales::parse_json(&json.to_string()) {
            Ok(locales) => match mind_core::maps::locales::write_json(&locales) {
                Ok(canonical) => {
                    self.editor.tags.insert("locales".to_owned(), canonical);
                    self.clear_error();
                    true
                }
                Err(error) => {
                    self.last_error_key = Some("@editor.errorload".to_owned());
                    self.last_error_message = Some(error.to_string());
                    false
                }
            },
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                false
            }
        }
    }

    /// `WaveInfoDialog.showUnits`: unit rows (`{name, health}`) for the pickers
    /// and the health sort; icons are omitted.
    #[func]
    pub fn wave_units_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let rows: Vec<serde_json::Value> = content
            .units()
            .iter()
            .map(|unit| {
                serde_json::json!({
                    "name": unit.name,
                    "health": unit.health,
                })
            })
            .collect();
        GString::from(serde_json::to_string(&rows).unwrap_or_default().as_str())
    }

    /// `WaveInfoDialog.showEffects`: the status-effect names (`content.statusEffects()`).
    #[func]
    pub fn wave_effects_json(&self) -> GString {
        let Some(content) = self.content.as_ref() else {
            return GString::from("[]");
        };
        let names: Vec<&str> = content
            .statuses()
            .iter()
            .map(|status| status.name.as_str())
            .collect();
        GString::from(serde_json::to_string(&names).unwrap_or_default().as_str())
    }

    /// `WaveInfoDialog`'s `@waves.random`: `groups = Waves.generate(1f / 10f)`.
    #[func]
    pub fn randomize_waves(&mut self) -> bool {
        let groups = mind_core::game::waves::Waves::generate(0.1);
        let values: Vec<serde_json::Value> = groups.iter().map(|group| group.to_json()).collect();
        let json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned());
        self.set_waves_json(GString::from(json.as_str()))
    }

    /// `WaveInfoDialog`'s spawn picker (`spawner.getSpawns()`): the map's
    /// spawn-overlay tiles packed `Point2.pack(x, y)`.
    #[func]
    pub fn spawn_points_json(&self) -> GString {
        let Some(spawn) = self
            .content
            .as_ref()
            .and_then(|content| content.block_id("spawn"))
        else {
            return GString::from("[]");
        };
        let mut packed: Vec<i32> = Vec::new();
        for y in 0..self.grid.height() {
            for x in 0..self.grid.width() {
                if self.grid.tile(x, y).overlay == spawn {
                    packed.push(mind_core::math::rand_arc::pack_point2(x, y) as i32);
                }
            }
        }
        GString::from(serde_json::to_string(&packed).unwrap_or_default().as_str())
    }

    /// `WaveInfoDialog`'s `@settings.reset`: the vanilla wave table
    /// (`JsonIO.copy(waves.get())`).
    #[func]
    pub fn default_waves_json(&self) -> GString {
        let mut waves = mind_core::game::waves::Waves::default();
        let values: Vec<serde_json::Value> =
            waves.get().iter().map(|group| group.to_json()).collect();
        GString::from(serde_json::to_string(&values).unwrap_or_default().as_str())
    }

    /// `WaveGraph.rebuild`: the counts/health series for the `spawns` tag.
    #[func]
    pub fn wave_graph(&self, from: i64, to: i64) -> VarDictionary {
        use mind_core::editor::wave_graph::WaveGraphData;
        let Some(content) = self.content.as_ref() else {
            return dict(vec![("from", from.to_variant()), ("to", to.to_variant())]);
        };
        let groups = parse_spawn_groups(self.editor.tags.get("spawns").map(String::as_str));
        let resolve = |name: &str| {
            content
                .unit_by_name(name)
                .map(|def| (def.id.raw(), def.health))
        };
        let data = WaveGraphData::compute(&groups, from as i32, to as i32, resolve);

        let mut units: Array<VarDictionary> = Array::new();
        for unit in &data.units {
            units.push(&dict(vec![
                ("id", (unit.id as i64).to_variant()),
                ("name", GString::from(unit.name.as_str()).to_variant()),
                ("health", (unit.health as f64).to_variant()),
            ]));
        }
        let mut series: Array<Variant> = Array::new();
        for row in &data.series {
            let mut values = PackedInt64Array::new();
            for value in row {
                values.push(*value as i64);
            }
            series.push(&values.to_variant());
        }
        let mut totals = PackedInt64Array::new();
        for value in &data.totals {
            totals.push(*value as i64);
        }
        let mut health: Array<Variant> = Array::new();
        for value in &data.health {
            health.push(&(*value as f64).to_variant());
        }

        dict(vec![
            ("from", (data.from as i64).to_variant()),
            ("to", (data.to as i64).to_variant()),
            ("units", units.to_variant()),
            ("series", series.to_variant()),
            ("totals", totals.to_variant()),
            ("health", health.to_variant()),
            ("max", (data.max as i64).to_variant()),
            ("max_total", (data.max_total as i64).to_variant()),
            ("max_health", (data.max_health as f64).to_variant()),
        ])
    }

    /// `MapProcessorsDialog`: `world-processor` centers with tag/icon.
    #[func]
    pub fn processors(&self) -> Array<VarDictionary> {
        let mut out = Array::new();
        let Some(content) = self.content.as_ref() else {
            return out;
        };
        let Some(processor) = content.block_id("world-processor") else {
            return out;
        };
        for index in 0..self.grid.tiles.len() {
            let tile = self.grid.tiles.geti(index);
            if tile.block != processor {
                continue;
            }
            let Some(entity) = tile.build else {
                continue;
            };
            let (tag, icon_tag) = self
                .ecs
                .0
                .get::<mind_core::logic::blocks::logic_block::LogicBlockState>(entity)
                .map(|state| (state.tag.clone(), state.icon_tag))
                .unwrap_or((None, '\0'));
            out.push(&dict(vec![
                ("x", (tile.x as i32).to_variant()),
                ("y", (tile.y as i32).to_variant()),
                (
                    "tag",
                    GString::from(tag.unwrap_or_default().as_str()).to_variant(),
                ),
                ("icon_tag", (icon_tag as i64).to_variant()),
            ]));
        }
        out
    }

    /// Editor building entity at `(x, y)` carrying `LogicBlockState`.
    fn logic_entity_at(&self, x: i64, y: i64) -> Option<bevy_ecs::entity::Entity> {
        let (Ok(xi), Ok(yi)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let entity = self
            .grid
            .entity_at(mind_core::world::TilePos::new(xi, yi))?;
        self.ecs
            .0
            .get::<LogicBlockState>(entity)
            .is_some()
            .then_some(entity)
    }

    /// `MapProcessorsDialog` `@add`: the first non-synthetic tile becomes a
    /// `world-processor` (`MapProcessorsDialog.java:33-56`); false when the
    /// whole map is synthetic (`@editor.worldprocessors.nospace`).
    #[func]
    pub fn add_processor(&mut self) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let Some(processor) = content.block_id("world-processor") else {
            return false;
        };
        let Some((x, y)) = mind_core::editor::processors::free_processor_tile(&self.grid, content)
        else {
            return false;
        };
        let team = Team::get(1).id;
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        world.set_block(x, y, processor, team, 0);
        self.mark_tiles_changed();
        true
    }

    /// `MapProcessorsDialog` rename: `log.tag = tag` directly (the Java
    /// comment bypasses privileged configure checks, `:100-105`).
    #[func]
    pub fn rename_processor(&mut self, x: i64, y: i64, tag: GString) -> bool {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return false;
        };
        let clamped = mind_core::editor::processors::clamp_tag(&tag.to_string());
        let Some(mut state) = self.ecs.0.get_mut::<LogicBlockState>(entity) else {
            return false;
        };
        state.tag = (!clamped.is_empty()).then_some(clamped);
        true
    }

    /// `MapProcessorsDialog` icon select: `log.iconTag = (char)ic` (`:86-90`);
    /// `0` clears the icon (`Icon.none`).
    #[func]
    pub fn set_processor_icon(&mut self, x: i64, y: i64, code: i64) -> bool {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return false;
        };
        let Some(mut state) = self.ecs.0.get_mut::<LogicBlockState>(entity) else {
            return false;
        };
        state.icon_tag = u32::try_from(code)
            .ok()
            .and_then(char::from_u32)
            .unwrap_or('\0');
        true
    }

    /// `MapProcessorsDialog` delete (`:116-134`): a processor surrounded by
    /// walls/static blocks becomes `stone-wall` (the port does not track
    /// `floor().wall`, `editor/processors.rs`), else air. Confirmation is the
    /// caller's.
    #[func]
    pub fn delete_processor(&mut self, x: i64, y: i64) -> bool {
        let (Ok(xi), Ok(yi)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let (xi, yi) = (xi as i32, yi as i32);
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        if self.grid.tiles.getn(xi, yi).is_none() {
            return false;
        }
        let mut surrounded = true;
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let Some(neighbor) = self.grid.tiles.getn(xi + dx, yi + dy) else {
                continue; // map edge: `Tile.nearby` null counts as surrounded
            };
            let def = content.block(neighbor.block);
            let wall = def.is_some_and(|def| def.privileged || block_info::is_static(def));
            if !wall {
                surrounded = false;
                break;
            }
        }
        let replacement = if surrounded {
            content.block_id("stone-wall").unwrap_or(BlockId::AIR)
        } else {
            BlockId::AIR
        };
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        world.set_block(xi, yi, replacement, 0, 0);
        self.mark_tiles_changed();
        true
    }

    /// `LogicDialog.show` for an editor processor
    /// (`MapProcessorsDialog`'s name button -> `showEditDialog(true)`): the
    /// normalized program (`LAssembler.read` -> `write`) and privilege.
    #[func]
    pub fn logic_get_state(&self, x: i64, y: i64) -> VarDictionary {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return VarDictionary::new();
        };
        let Some(state) = self.ecs.0.get::<LogicBlockState>(entity) else {
            return VarDictionary::new();
        };
        let privileged = state.executor.privileged;
        let code = mind_core::logic::Assembler::read(&state.code, privileged)
            .map(|statements| mind_core::logic::Assembler::write(&statements))
            .unwrap_or_else(|_| state.code.clone());
        dict(vec![
            ("code", GString::from(code.as_str()).to_variant()),
            ("privileged", privileged.to_variant()),
            ("ipt", (state.ipt as i64).to_variant()),
            (
                "tag",
                match &state.tag {
                    Some(tag) => GString::from(tag.as_str()).to_variant(),
                    None => Variant::nil(),
                },
            ),
        ])
    }

    /// `LogicBuild.showEditDialog(true)`'s configure half for an editor
    /// processor: stores the compressed program + links in the editor's
    /// `LogicBlockState` (the editor does not run processors). Returns the
    /// `SimHost.logic_configure_code` error convention (`""` ok, `toolong:<n>`).
    #[func]
    pub fn logic_configure_code(&mut self, x: i64, y: i64, code: GString) -> GString {
        let Some(entity) = self.logic_entity_at(x, y) else {
            return GString::from("noprocessor");
        };
        let Some(state) = self.ecs.0.get::<LogicBlockState>(entity) else {
            return GString::from("noprocessor");
        };
        let links = state.relative_connections(&self.ecs.0, entity);
        let code = code.to_string();
        let bytes = match mind_core::logic::blocks::logic_block::encode_for_config(&code, &links) {
            Ok(bytes) => bytes,
            Err(len) => return GString::from(format!("toolong:{len}").as_str()),
        };
        let Some(mut state) = self.ecs.0.entity_mut(entity).take::<LogicBlockState>() else {
            return GString::from("noprocessor");
        };
        let result = state.read_compressed(&mut self.ecs.0, entity, &bytes, true);
        self.ecs.0.entity_mut(entity).insert(state);
        if result.is_ok() {
            GString::new()
        } else {
            GString::from("invalid")
        }
    }

    /// `MapAssetsDialog.assets()`: the loaded data-asset records.
    ///
    /// The plan-20 `ModDataManager` is not yet mounted on this node, so the list
    /// is empty until the asset host is wired (documented in the plan changelog).
    #[func]
    pub fn assets(&self) -> Array<VarDictionary> {
        Array::new()
    }

    /// `MapEditorDialog.export`: write the map with embedded data assets.
    #[func]
    pub fn export_map(&mut self, path: GString) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let name = self.editor.tags.get("name").cloned().unwrap_or_default();
        let file = Self::native_path(&path.to_string());
        let base = editor_base_tags(
            self.grid.tiles.width as u16,
            self.grid.tiles.height as u16,
            &name,
        );
        match save_editor_map_with_ecs(
            &NativeFs,
            &file,
            &self.grid,
            content,
            &self.ecs.0,
            base,
            self.editor.tags.clone(),
            true,
        ) {
            Ok(()) => {
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_map_error(&error);
                false
            }
        }
    }

    /// `MapEditorDialog.exportImage`: write the color-mapped PNG.
    #[func]
    pub fn export_image(&mut self, path: GString) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let file = Self::native_path(&path.to_string());
        let source = mind_core::editor::maps_glue::EditorMapSource::new(&self.grid, content);
        let image = self.editor.export_image(content, &source);
        match mind_core::io::map::encode_png(&image) {
            Ok(bytes) => match NativeFs.write(&file, &bytes) {
                Ok(()) => {
                    self.clear_error();
                    true
                }
                Err(error) => {
                    self.last_error_key = Some("@editor.errorload".to_owned());
                    self.last_error_message = Some(error.to_string());
                    false
                }
            },
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                false
            }
        }
    }

    /// `Maps.saveMap` shell: writes `user://maps/<name>.msav`.
    ///
    /// M3 keeps the tile+tag save path; the M7 rules/objectives reset and
    /// built-in overwrite guard append in their milestone.
    #[func]
    pub fn save(&mut self) -> VarDictionary {
        let Some(content) = self.content.as_ref() else {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("no content").to_variant()),
            ]);
        };
        let name = self.editor.tags.get("name").cloned().unwrap_or_default();
        if name.trim().is_empty() {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("noname").to_variant()),
            ]);
        }
        // `MapEditorDialog.save` -> `handleSaveBuiltin`: a built-in map is
        // never overwritten (`ui.showErrorMessage("@editor.save.overwrite")`,
        // `MapEditorDialog.java:442-445`).
        if let Some(current) = self.file.as_ref() {
            let user_maps = PathBuf::from(
                ProjectSettings::singleton()
                    .globalize_path("user://maps")
                    .to_string(),
            );
            if !current.starts_with(&user_maps) {
                return dict(vec![
                    ("ok", false.to_variant()),
                    ("error", GString::from("overwrite").to_variant()),
                ]);
            }
        }
        let dir = PathBuf::from(
            ProjectSettings::singleton()
                .globalize_path("user://maps")
                .to_string(),
        );
        if let Err(error) = std::fs::create_dir_all(&dir) {
            return dict(vec![
                ("ok", false.to_variant()),
                (
                    "error",
                    GString::from(error.to_string().as_str()).to_variant(),
                ),
            ]);
        }
        let file = dir.join(format!("{name}.msav"));
        let base = editor_base_tags(
            self.grid.tiles.width as u16,
            self.grid.tiles.height as u16,
            &name,
        );
        match save_editor_map_with_ecs(
            &NativeFs,
            &file,
            &self.grid,
            content,
            &self.ecs.0,
            base,
            self.editor.tags.clone(),
            false,
        ) {
            Ok(()) => {
                self.file = Some(file.clone());
                self.editor.saved = true;
                self.clear_error();
                dict(vec![
                    ("ok", true.to_variant()),
                    (
                        "path",
                        GString::from(file.display().to_string().as_str()).to_variant(),
                    ),
                    ("custom", true.to_variant()),
                ])
            }
            Err(error) => {
                self.set_map_error(&error);
                dict(vec![
                    ("ok", false.to_variant()),
                    (
                        "error",
                        GString::from(error.to_string().as_str()).to_variant(),
                    ),
                ])
            }
        }
    }

    // --- playtest lifecycle (plan 19 M7 §3.11) ---

    /// `MapEditorDialog.editInGame()`: snapshot rules, apply the hidden `editor`
    /// gamemode and enter a synthetic play session.
    #[func]
    pub fn edit_in_game(&mut self) -> VarDictionary {
        let events = self
            .play
            .edit_in_game(&mut self.editor, &mut self.play_session);
        self.clear_error();
        dict(vec![
            ("ok", true.to_variant()),
            ("playing", true.to_variant()),
            (
                "phase",
                GString::from(self.play_session.phase.name()).to_variant(),
            ),
            ("rules_editor", self.play_session.rules.editor.to_variant()),
            ("events", (events.len() as i64).to_variant()),
        ])
    }

    /// `MapEditorDialog.playtest()`: save first, then open `MapPlayDialog` in
    /// playtesting mode (`dialog: true`). Java's Shift shortcut (auto-pick a
    /// gamemode and play immediately) is not exposed by the facade.
    #[func]
    pub fn playtest(&mut self) -> VarDictionary {
        let name = self.editor.tags.get("name").cloned().unwrap_or_default();
        if name.trim().is_empty() {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("noname").to_variant()),
            ]);
        }
        // `MapEditorDialog.playtest` aborts when `save()` returns null (builtin
        // maps refuse the overwrite); the save error reaches the caller.
        let saved = self.save();
        if !saved
            .get("ok")
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false)
        {
            let error = saved
                .get("error")
                .and_then(|value| value.try_to::<GString>().ok())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "save".to_owned());
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from(error.as_str()).to_variant()),
            ]);
        }
        dict(vec![
            ("ok", true.to_variant()),
            ("dialog", true.to_variant()),
        ])
    }

    /// `MapEditorDialog.resumeEditing()`: return to the menu with the pre-playtest
    /// rules restored.
    #[func]
    pub fn resume_editing(&mut self) -> bool {
        self.mark_tiles_changed();
        self.play
            .resume_editing(&mut self.editor, &mut self.play_session);
        true
    }

    /// `MapEditorDialog.resumeAfterPlaytest(map)` (`MapEditorDialog.java:370-372`):
    /// `beginEditMap(map.file)` — `shownWithMap = true` and the map the
    /// playtest saved is reopened (`:508-519`). Called by
    /// `MindCampaign.check_playtest` when a playtest ends (pause-menu quit or
    /// game-over `@menu`); the caller shows the shell.
    #[func]
    pub fn resume_after_playtest(&mut self, path: GString) -> bool {
        self.play.playtesting = false;
        let opened = self.begin_edit_map(path);
        if opened {
            self.editor.shown_with_map = true;
        }
        opened
    }

    /// `EditorMapsDialog.tryImportMap` (`EditorMapsDialog.java:55-98`): rejects
    /// images, generates `unknown<N>` for nameless maps and reports name
    /// conflicts. Returns `""` on success, `"image"`, `"exists:<name>"` (a
    /// built-in), `"overwrite:<name>"` (a custom map; re-call with `overwrite`)
    /// or `"error"`.
    #[func]
    pub fn import_map(&mut self, path: GString, overwrite: bool) -> GString {
        let native = ProjectSettings::singleton().globalize_path(&path);
        let from = std::path::PathBuf::from(native.to_string());
        if !NativeFs.exists(&from) || MapIo::is_image(&NativeFs, &from) {
            return GString::from("image");
        }
        let Ok(header) = MapIo::create_map(&NativeFs, &from, true) else {
            return GString::from("error");
        };
        let root = crate::assets::loader::user_data_root();
        let paths = mind_core::io::fs::Paths::new(&root);
        let dir = paths.maps();
        let mut maps = crate::assets::loader::load_map_registry(&root);
        // Java's fallback loop returns `unknown<first free index + 1>`
        // (`while(maps.byName(result + number++) != null);`).
        let mut name = header.name.trim().to_owned();
        if name.is_empty() {
            let mut number = 0i32;
            loop {
                let candidate = format!("unknown{number}");
                number += 1;
                if !maps
                    .all()
                    .iter()
                    .any(|map| map.name().eq_ignore_ascii_case(&candidate))
                {
                    break;
                }
            }
            name = format!("unknown{number}");
        }
        if let Some(conflict) = maps
            .all()
            .iter()
            .find(|map| map.name().eq_ignore_ascii_case(&name))
            .cloned()
        {
            if !conflict.custom {
                return GString::from(format!("exists:{name}").as_str());
            }
            if !overwrite {
                return GString::from(format!("overwrite:{name}").as_str());
            }
            let _ = maps.remove_map(&NativeFs, &paths, conflict.name());
        }
        match maps.import_map(&NativeFs, &dir, &from) {
            Ok(_) => {
                self.clear_error();
                GString::new()
            }
            Err(error) => {
                log::warn!("import_map `{}`: {error}", from.display());
                GString::from("error")
            }
        }
    }

    /// `EditorMapsDialog.showMap`'s `@delete`: removes a custom map and its
    /// preview/cache (`Maps.removeMap`). Returns false for built-in/unknown
    /// maps (the button is disabled for those).
    #[func]
    pub fn delete_map(&mut self, name: GString) -> bool {
        let root = crate::assets::loader::user_data_root();
        let paths = mind_core::io::fs::Paths::new(&root);
        let mut maps = crate::assets::loader::load_map_registry(&root);
        let wanted = name.to_string();
        let Some(map) = maps.all().iter().find(|map| map.name() == wanted).cloned() else {
            return false;
        };
        if !map.custom {
            return false;
        }
        maps.remove_map(&NativeFs, &paths, map.name()).is_ok()
    }

    /// `MapEditorDialog.tryExit()`: always requires the unsaved-changes confirm.
    #[func]
    pub fn try_exit(&mut self) -> bool {
        self.play.try_exit()
    }

    /// Playtest lifecycle state for the inspector / MCP oracle.
    #[func]
    pub fn playtest_status(&self) -> VarDictionary {
        dict(vec![
            (
                "phase",
                GString::from(self.play_session.phase.name()).to_variant(),
            ),
            ("editor", self.play_session.rules.editor.to_variant()),
            ("playtesting", self.play.playtesting.to_variant()),
            (
                "has_snapshot",
                self.play.last_saved_rules.is_some().to_variant(),
            ),
            ("saved", self.editor.saved.to_variant()),
        ])
    }

    // --- map view (view-only, plan 19 §3.5) ---

    /// Pan the editor view by screen-space `(dx, dy)`.
    #[func]
    pub fn map_view_pan(&mut self, dx: f64, dy: f64) {
        self.view.pan(dx as f32, dy as f32);
    }

    /// Scroll-zoom the editor view (`axis` is the wheel axis).
    #[func]
    pub fn map_view_zoom(&mut self, axis: f64) {
        self.view.zoom_by(axis as f32);
    }

    /// Reset the view pan.
    #[func]
    pub fn map_view_center(&mut self) {
        self.view.center();
    }

    /// Current pan/zoom/grid state.
    #[func]
    pub fn map_view_state(&self) -> VarDictionary {
        dict(vec![
            ("offset_x", self.view.offset_x.to_variant()),
            ("offset_y", self.view.offset_y.to_variant()),
            ("zoom", self.view.zoom.to_variant()),
            ("grid", self.view.grid.to_variant()),
        ])
    }

    /// `MapView.project`: screen pixel → tile.
    #[func]
    pub fn map_view_project(
        &self,
        screen_x: f64,
        screen_y: f64,
        view_w: f64,
        view_h: f64,
    ) -> Vector2 {
        let even_block = self
            .content
            .as_ref()
            .and_then(|content| content.block(self.editor.draw_block))
            .is_some_and(|def| def.size % 2 == 0);
        let (x, y) = self.view.project(
            screen_x as f32,
            screen_y as f32,
            view_w as f32,
            view_h as f32,
            self.grid.width(),
            self.grid.height(),
            even_block,
            self.editor.tool == EditorTool::Eraser,
        );
        Vector2::new(x as f32, y as f32)
    }

    /// `MapView.unproject`: tile → screen pixel inside the view `Control`.
    #[func]
    pub fn map_view_unproject(
        &self,
        tile_x: i32,
        tile_y: i32,
        view_w: f64,
        view_h: f64,
    ) -> Vector2 {
        let (x, y) = self.view.unproject(
            tile_x,
            tile_y,
            view_w as f32,
            view_h as f32,
            self.grid.width(),
            self.grid.height(),
        );
        Vector2::new(x, y)
    }

    /// Brush outline polygon (screen space) for the cursor tile.
    #[func]
    pub fn map_view_brush_outline(
        &self,
        tile_x: i32,
        tile_y: i32,
        view_w: f64,
        view_h: f64,
    ) -> PackedVector2Array {
        let mut out = PackedVector2Array::new();
        let (cx, cy) = self.view.unproject(
            tile_x,
            tile_y,
            view_w as f32,
            view_h as f32,
            self.grid.width(),
            self.grid.height(),
        );
        let scale = self
            .view
            .tile_scale(view_w as f32, view_h as f32, self.grid.width());
        for point in brush_polygon(self.editor.brush_size) {
            out.push(Vector2::new(cx + point[0] * scale, cy + point[1] * scale));
        }
        out
    }

    /// Routes one cursor tile through the active tool
    /// (`MapView.touchDown` press; `Zoom`/`Line` are handled by the caller).
    #[func]
    pub fn map_view_touch(&mut self, tile_x: i32, tile_y: i32) -> bool {
        self.mark_tiles_changed();
        let tool = self.editor.tool;
        if tool == EditorTool::Line {
            return true;
        }
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        {
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            mind_core::editor::tool::touched(
                &mut self.editor,
                tool,
                &mut world,
                content,
                tile_x,
                tile_y,
            );
        }
        if !tool.draggable() {
            self.editor.flush_op();
        }
        true
    }

    /// Routes a drag segment through the active draggable tool
    /// (`MapView.touchDragged` + `Bresenham2.line`).
    #[func]
    pub fn map_view_drag(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        self.mark_tiles_changed();
        let tool = self.editor.tool;
        if !tool.draggable() {
            return false;
        }
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let mut points: Vec<(i32, i32)> = Vec::new();
        mind_core::world::raycast::raycast_each(x1, y1, x2, y2, |x, y| {
            points.push((x, y));
            false
        });
        {
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            for (x, y) in points {
                mind_core::editor::tool::touched(&mut self.editor, tool, &mut world, content, x, y);
            }
        }
        true
    }

    /// `MapView.touchUp`: `editor.flushOp()`.
    #[func]
    pub fn map_view_flush(&mut self) {
        self.mark_tiles_changed();
        self.editor.flush_op();
    }

    // --- dev / MCP oracle (plan 19 §3.1, §7b) ---

    /// Draws a line with the current draw block (`dev_draw_line`).
    #[func]
    pub fn dev_draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        self.mark_tiles_changed();
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let mut world = WorldEditorGrid::new(
            &mut self.grid,
            content,
            &mut self.ecs.0,
            &self.hooks,
            &self.render,
        );
        mind_core::editor::tool::touched_line(
            &mut self.editor,
            EditorTool::Line,
            &mut world,
            content,
            x1,
            y1,
            x2,
            y2,
        );
        true
    }

    /// Draws a filled rectangle with single-tile brushes (`dev_draw_rect`).
    #[func]
    pub fn dev_draw_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let previous = self.editor.brush_size;
        self.editor.brush_size = 0.5;
        {
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            for y in y0.min(y1)..=y0.max(y1) {
                for x in x0.min(x1)..=x0.max(x1) {
                    self.editor.draw_blocks(&mut world, content, x, y);
                }
            }
            self.editor.flush_op();
        }
        self.editor.brush_size = previous;
        true
    }

    /// Draws a circle of the current draw block with radius `radius`.
    #[func]
    pub fn dev_draw_circle(&mut self, x: i32, y: i32, radius: f64) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let previous = self.editor.brush_size;
        self.editor.brush_size = (radius as f32).clamp(0.5, 20.0);
        {
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            self.editor.draw_blocks(&mut world, content, x, y);
            self.editor.flush_op();
        }
        self.editor.brush_size = previous;
        true
    }

    /// The retained undo operations as packed op lists (`dev_op_log`).
    #[func]
    pub fn dev_op_log(&self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for operations in self.editor.op_log() {
            let mut packed = PackedInt64Array::new();
            for op in operations {
                packed.push(op as i64);
            }
            out.push(&packed.to_variant());
        }
        out
    }

    /// Applies an op log (same shape as `dev_op_log`) onto the world.
    #[func]
    pub fn dev_apply_op_log(&mut self, log: Array<Variant>) -> bool {
        let Some(content) = self.content.as_ref() else {
            return false;
        };
        let mut operations: Vec<Vec<u64>> = Vec::with_capacity(log.len());
        for index in 0..log.len() {
            let Some(entry) = log.get(index) else {
                return false;
            };
            let Ok(packed) = entry.try_to::<PackedInt64Array>() else {
                return false;
            };
            let mut ops = Vec::with_capacity(packed.len());
            for value in packed.as_slice() {
                ops.push(*value as u64);
            }
            operations.push(ops);
        }
        for ops in operations {
            let mut operation = DrawOperation::from_ops(ops);
            {
                let mut world = WorldEditorGrid::new(
                    &mut self.grid,
                    content,
                    &mut self.ecs.0,
                    &self.hooks,
                    &self.render,
                );
                operation.redo(&mut world, content);
            }
            self.editor.push_op(operation);
        }
        true
    }

    /// Undoes every retained operation (`dev_undo_all`).
    #[func]
    pub fn dev_undo_all(&mut self) -> i64 {
        let mut count = 0i64;
        while let Some(content) = self.content.as_ref() {
            if !self.editor.can_undo() {
                break;
            }
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            self.editor.undo(&mut world, content);
            count += 1;
        }
        count
    }

    /// Redoes every available operation (`dev_redo_all`).
    #[func]
    pub fn dev_redo_all(&mut self) -> i64 {
        let mut count = 0i64;
        while let Some(content) = self.content.as_ref() {
            if !self.editor.can_redo() {
                break;
            }
            let mut world = WorldEditorGrid::new(
                &mut self.grid,
                content,
                &mut self.ecs.0,
                &self.hooks,
                &self.render,
            );
            self.editor.redo(&mut world, content);
            count += 1;
        }
        count
    }

    /// FNV-1a checksum of the editor world (`dev_state_digest`).
    #[func]
    pub fn dev_state_digest(&self) -> i64 {
        let mut checksummer = Checksummer::new();
        checksummer.part(&self.grid);
        checksummer.finish().value() as i64
    }

    /// Saves under `name` (sets the tag first), returning the `save()` result.
    #[func]
    pub fn dev_save_as(&mut self, name: GString) -> VarDictionary {
        self.editor.tags.insert("name".to_owned(), name.to_string());
        self.save()
    }

    /// Opens `user://maps/<name>.msav` in the editor (`dev_open`).
    #[func]
    pub fn dev_open(&mut self, name: GString) -> bool {
        let path = format!("user://maps/{}.msav", name);
        self.begin_edit_map(GString::from(path.as_str()))
    }

    /// Editor world invariants for the MCP oracle (`check_invariants`).
    #[func]
    pub fn check_invariants(&self) -> VarDictionary {
        let content_ok = self.content.is_some();
        let draw_block_editable = self
            .content
            .as_ref()
            .and_then(|content| content.block(self.editor.draw_block))
            .is_some_and(|def| def.in_editor);
        let mut out = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        out.set(&key("content_ok"), &content_ok.to_variant());
        out.set(&key("width"), &(self.grid.width() as i64).to_variant());
        out.set(&key("height"), &(self.grid.height() as i64).to_variant());
        out.set(
            &key("retained_ops"),
            &(self.editor.retained_ops() as i64).to_variant(),
        );
        out.set(
            &key("current_ops"),
            &(self.editor.ops() as i64).to_variant(),
        );
        out.set(
            &key("recording_allowed"),
            &self
                .editor
                .should_record_ops(false, self.grid.generating)
                .to_variant(),
        );
        out.set(
            &key("draw_block_editable"),
            &draw_block_editable.to_variant(),
        );
        out.set(
            &key("ok"),
            &(content_ok && draw_block_editable).to_variant(),
        );
        out
    }
}

/// GDScript tag for one objective field kind (`FieldKind`).
fn objective_kind_tag(kind: &FieldKind) -> String {
    match kind {
        FieldKind::String => "string".to_owned(),
        FieldKind::Bool => "bool".to_owned(),
        FieldKind::Byte => "byte".to_owned(),
        FieldKind::Int => "int".to_owned(),
        FieldKind::Float => "float".to_owned(),
        FieldKind::Content(content_type) => format!("content:{}", content_type.name()),
        FieldKind::Team => "team".to_owned(),
        FieldKind::Color => "color".to_owned(),
        FieldKind::Vec2F => "vec2f".to_owned(),
        FieldKind::Vec2I => "vec2i".to_owned(),
        FieldKind::Objective(_) => "objective".to_owned(),
        FieldKind::Seq(inner) => format!("seq:{}", objective_kind_tag(inner)),
        FieldKind::Map(inner) => format!("map:{}", objective_kind_tag(inner)),
        FieldKind::IndexBool => "indexbool".to_owned(),
        FieldKind::LabelFlag => "labelflag".to_owned(),
        FieldKind::Alignment => "alignment".to_owned(),
        FieldKind::TextureHolder => "texture".to_owned(),
        FieldKind::Vertices => "verts".to_owned(),
        FieldKind::Marker => "marker".to_owned(),
    }
}

/// GDScript tags for one objective field's annotation flags (`FieldFlags`).
fn objective_flag_tags(flags: FieldFlags) -> Vec<&'static str> {
    let mut tags = Vec::new();
    for (flag, tag) in [
        (FieldFlags::SECOND, "second"),
        (FieldFlags::TILE_POS, "tilepos"),
        (FieldFlags::MULTILINE, "multiline"),
        (FieldFlags::LOGIC_CODE, "logiccode"),
        (FieldFlags::RESEARCHABLE, "researchable"),
        (FieldFlags::SYNTHETIC, "synthetic"),
        (FieldFlags::HIDDEN, "hidden"),
        (FieldFlags::UNORDERED, "unordered"),
        (FieldFlags::ALIGN_NO_VER, "align-nover"),
        (FieldFlags::ALIGN_NO_HOR, "align-nohor"),
    ] {
        if flags.contains(flag) {
            tags.push(tag);
        }
    }
    tags
}

/// Builds one palette/`block_info` row.
fn palette_row(def: &BlockDef) -> VarDictionary {
    let mut row = VarDictionary::new();
    let key = |name: &str| GString::from(name);
    row.set(&key("name"), &GString::from(def.name.as_str()).to_variant());
    row.set(&key("id"), &(def.id.raw() as i64).to_variant());
    row.set(&key("size"), &(def.size as i64).to_variant());
    row.set(&key("is_floor"), &block_info::is_floor(def).to_variant());
    row.set(
        &key("is_overlay"),
        &block_info::is_overlay(def).to_variant(),
    );
    row.set(&key("is_multiblock"), &(def.size > 1).to_variant());
    row.set(
        &key("is_core"),
        &(def.kind == BlockKind::CoreBlock).to_variant(),
    );
    row.set(&key("synthetic"), &block_info::synthetic(def).to_variant());
    row.set(&key("configurable"), &def.configurable.to_variant());
    row.set(&key("save_config"), &def.save_config.to_variant());
    row.set(
        &key("build_visibility"),
        &GString::from(def.build_visibility.name()).to_variant(),
    );
    row.set(
        &key("ui_icon"),
        &GString::from(format!("ui/block-{}-ui", def.name).as_str()).to_variant(),
    );
    row
}
