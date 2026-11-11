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

use mind_core::content::parser_hooks::PatchAsset;
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
use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map_with_ecs_and_patches};
use mind_core::editor::objectives::{FieldFlags, FieldKind};
use mind_core::editor::playtest::EditorPlayState;
use mind_core::editor::{BRUSH_SIZES, EditorTool, EditorWorldHooks, MapEditor};
use mind_core::game::play::PlaySession;
use mind_core::game::team::Team;
use mind_core::io::fs::{FileSystem, NativeFs};
use mind_core::io::map::{BlockPalette, MapIo};
use mind_core::io::save::state::PatchSetIo;
use mind_core::logic::blocks::LogicBlockState;
use mind_core::maps::Map;
use mind_core::maps::filters::BlockPredicate;
use mind_core::maps::filters::block_info;
use mind_core::mods::assets::{DataAssetData, DataAssetType};
use mind_core::world::{BlockKindData, BlockTable, NoopRenderHooks, WorldGrid};

mod editor_renderer;
mod map_view;
pub mod preview;

pub use editor_renderer::MindEditorRenderer;
use map_view::MapViewDriver;
pub use preview::MindPreview;

mod api_banned;
mod api_dev;
mod api_filters;
mod api_lifecycle;
mod api_map_view;
mod api_objectives;
mod api_palette;
mod api_patches;
mod api_playtest;
mod api_processors;
mod api_waves;
mod free;

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

/// Runs `f` with the owner's `state.data` patch set (`GameState.data`) while
/// the `MindMods` autoload stays bound; `None` when the autoload is absent.
fn data_patches<R>(f: impl FnOnce(Option<&dyn PatchSetIo>) -> R) -> R {
    match crate::content_boot::mods_node() {
        Some(mods) => {
            let guard = mods.bind();
            f(Some(guard.data() as &dyn PatchSetIo))
        }
        None => f(None),
    }
}

/// `ContentAsset.loadableContent` kind names (the content view's add types).
fn loadable_content_type(kind: &str) -> Option<mind_core::content::ContentType> {
    use mind_core::content::ContentType;
    match kind {
        "item" => Some(ContentType::Item),
        "block" => Some(ContentType::Block),
        "liquid" => Some(ContentType::Liquid),
        "status" => Some(ContentType::Status),
        "unit" => Some(ContentType::Unit),
        "weather" => Some(ContentType::Weather),
        _ => None,
    }
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

    /// The content registry, or `None` before [`Self::bootstrap`] built it.
    fn content(&self) -> Option<&ContentRegistry> {
        self.content.as_ref()
    }

    /// Records a `MapError` for the inspector / UI (`tryCatchMapError`).
    fn set_map_error(&mut self, error: &mind_core::maps::MapError) {
        let (key, message) = try_catch_map_error(error);
        self.last_error_key = Some(key.to_owned());
        self.last_error_message = Some(message);
    }

    /// Records a failed load (`@editor.errorload`) for the inspector / UI.
    fn set_load_error(&mut self, error: impl std::fmt::Display) {
        self.last_error_key = Some("@editor.errorload".to_owned());
        self.last_error_message = Some(error.to_string());
    }

    /// Clears the last error after a successful command.
    fn clear_error(&mut self) {
        self.last_error_key = None;
        self.last_error_message = None;
    }

    /// Runs `f` with the editor facade, a fresh [`WorldEditorGrid`] over the
    /// owned world and the content registry. The fields are destructured so the
    /// borrows stay split; `None` before the content boot ran.
    fn with_world<R>(
        &mut self,
        f: impl FnOnce(&mut MapEditor, &mut WorldEditorGrid, &ContentRegistry) -> R,
    ) -> Option<R> {
        let Self {
            content,
            grid,
            ecs,
            hooks,
            render,
            editor,
            ..
        } = self;
        let content = content.as_ref()?;
        let mut world = WorldEditorGrid::new(grid, content, &mut ecs.0, hooks, render);
        Some(f(editor, &mut world, content))
    }

    /// Marks the tile grid as changed (editor-renderer invalidation) and the
    /// map unsaved (`ui.editor.resetSaved()`).
    fn mark_tiles_changed(&mut self) {
        self.bump_tiles_rev();
        self.editor.saved = false;
    }

    /// Editor-renderer invalidation only; the cursor paths decide `saved`
    /// per tool kind (`MapView.java:96-98,114-116,135-137`).
    fn bump_tiles_rev(&mut self) {
        self.tiles_rev = self.tiles_rev.wrapping_add(1);
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
        let Some(content) = self.content() else {
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
        let _ = self.with_world(|editor, world, content| editor.undo(world, content));
    }

    /// Redoes one operation.
    #[func]
    pub fn redo(&mut self) {
        self.mark_tiles_changed();
        let _ = self.with_world(|editor, world, content| editor.redo(world, content));
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
}
