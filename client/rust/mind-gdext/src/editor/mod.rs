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
//! Declared in `res://scenes/spine.tscn` at `/root/Spine/MindEditor` (HLP §6.6)
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
use godot::classes::{INode, Node, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::content::{
    BlockDef, BlockKind, BuildVisibility, ContentRegistry, MemoryBundle, MemoryUnlockStore,
    create_base_content,
};
use mind_core::determinism::Checksummer;
use mind_core::ecs::MindWorld;
use mind_core::editor::draw_op::DrawOperation;
use mind_core::editor::grid::WorldEditorGrid;
use mind_core::editor::lifecycle::try_catch_map_error;
use mind_core::editor::maps_glue::{editor_base_tags, save_editor_map};
use mind_core::editor::{BRUSH_SIZES, EditorTool, MapEditor};
use mind_core::game::team::Team;
use mind_core::io::fs::{FileSystem, NativeFs};
use mind_core::io::map::MapIo;
use mind_core::maps::Map;
use mind_core::maps::filters::block_info;
use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

mod map_view;

use map_view::{MapViewDriver, brush_polygon};

/// Builds a `{GString: Variant}` dictionary (mirrors the other facades).
fn dict(pairs: Vec<(&str, Variant)>) -> VarDictionary {
    let mut out = VarDictionary::new();
    for (key, value) in pairs {
        out.set(&GString::from(key), &value);
    }
    out
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
    hooks: NoopWorldHooks,
    render: NoopRenderHooks,
    file: Option<PathBuf>,
    last_error_key: Option<String>,
    last_error_message: Option<String>,
    view: MapViewDriver,
}

#[godot_api]
impl INode for MindEditor {
    fn init(base: Base<Node>) -> Self {
        let content =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(mut registry) => {
                    if let Err(error) = registry.init() {
                        log::warn!("MindEditor content init failed: {error}");
                        None
                    } else if let Err(error) = registry.post_init() {
                        log::warn!("MindEditor content post_init failed: {error}");
                        None
                    } else {
                        Some(registry)
                    }
                }
                Err(error) => {
                    log::warn!("MindEditor content boot failed: {error}");
                    None
                }
            };
        let mut editor = MapEditor::new();
        if let Some(registry) = content.as_ref() {
            editor.reset(registry);
        }
        Self {
            base,
            editor,
            grid: WorldGrid::new(0, 0),
            content,
            ecs: MindWorld::new(),
            hooks: NoopWorldHooks,
            render: NoopRenderHooks,
            file: None,
            last_error_key: None,
            last_error_message: None,
            view: MapViewDriver::new(),
        }
    }

    fn ready(&mut self) {
        let blocks = self.palette_blocks(GString::new()).len();
        log::info!("MindEditor ready ({blocks} editor blocks)");
    }
}

impl MindEditor {
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
        match save_editor_map(
            &NativeFs,
            &file,
            &self.grid,
            content,
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
        self.editor.flush_op();
    }

    // --- dev / MCP oracle (plan 19 §3.1, §7b) ---

    /// Draws a line with the current draw block (`dev_draw_line`).
    #[func]
    pub fn dev_draw_line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
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
