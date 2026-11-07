// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapEditorDialog.java,
//         core/src/mindustry/editor/MapView.java,
//         core/src/mindustry/editor/WaveGraph.java,
//         core/src/mindustry/editor/MapObjectivesCanvas.java.

//! Native scene roots for the four standalone map-editor views.
//!
//! Each class replaces one scene-attached `client/scenes/editor/*.gd` script;
//! they are plain `Control`/`PanelContainer` roots (not `DialogController`s),
//! so they keep the static `.tscn` structure and only rebuild the dynamic rows:
//!
//! - [`MindMapEditorDialog`] (`map_editor_dialog.gd`,
//!   `core/src/mindustry/editor/MapEditorDialog.java`): the editor shell — tool
//!   rail, team/brush pickers, block palette, rotation/undo/redo, the
//!   `MindUi` editor-dialog buttons and the `doInput` keyboard shortcuts.
//! - [`MindMapView`] (`map_view.gd`, `core/src/mindustry/editor/MapView.java`):
//!   the pan/zoom/paint surface, its brush-outline/grid overlay and the
//!   `SubViewport` camera mirror of the `MindEditor` projection.
//! - [`MindWaveGraph`] (`wave_graph.gd`,
//!   `core/src/mindustry/editor/WaveGraph.java`): the `Tex.pane` wave plot, the
//!   unit legend with its hide toggles and the counts/totals/health mode row.
//! - [`MindMapObjectivesCanvas`] (`map_objectives_canvas.gd`,
//!   `core/src/mindustry/editor/MapObjectivesCanvas.java`): the objective
//!   marker canvas — grid, connectors, drag-to-move and the `@add` query cursor.
//!
//! Everything the views need (`status`, palette order, projection, brush
//! outline, wave series, objectives graph) comes from `/root/Spine/MindEditor`;
//! the views only lay out and forward, exactly like the GDScript they replace.

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{
    PackedInt64Array, PackedVector2Array, Rect2, Side, VarArray, VarDictionary, VariantType,
    Vector2, Vector2i,
};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, Camera2D, ColorRect, Control, GridContainer, HBoxContainer, HSlider, IControl,
    IPanelContainer, Input, InputEvent, InputEventKey, InputEventMouseButton,
    InputEventMouseMotion, Json, Label, LineEdit, MarginContainer, PanelContainer, ScrollContainer,
    StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, TextureRect, ThemeDb, VBoxContainer,
};
use godot::global::{HorizontalAlignment, Key, MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll, icons, styles};

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// A signal emitted from inside a bound GDExtension method re-enters the same
/// object when its handler calls back into it, which panics ("already bound",
/// PAR-0066); the editor views defer exactly those calls.
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
    });
    callable.call_deferred(&[]);
}

/// Calls a `MindEditor` facade method (`None` when the facade/method is absent).
fn editor_call(editor: &Gd<Node>, method: &str, args: &[Variant]) -> Option<Variant> {
    let mut editor = editor.clone();
    if !editor.has_method(method) {
        log::warn!("[ui] MindEditor has no `{method}`");
        return None;
    }
    Some(editor.call(method, args))
}

/// GDScript `float(value)` coercion for JSON numbers.
fn variant_f64(value: &Variant) -> f64 {
    match value.get_type() {
        VariantType::FLOAT => value.try_to::<f64>().unwrap_or_default(),
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number as f64)
            .unwrap_or_default(),
        _ => 0.0,
    }
}

/// GDScript `int(value)` for JSON numbers.
fn variant_i64(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        _ => 0,
    }
}

/// `bool(row.get(key, false))` on a dictionary.
fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    match dict.get(key) {
        Some(value) => match value.get_type() {
            VariantType::BOOL => value.try_to::<bool>().unwrap_or(false),
            _ => false,
        },
        None => false,
    }
}

/// `int(row.get(key, 0))` on a dictionary (`0` when absent).
fn dict_i64(dict: &VarDictionary, key: &str) -> i64 {
    dict.get(key).map(|value| variant_i64(&value)).unwrap_or(0)
}

/// `float(row.get(key, 0))` on a dictionary (`0` when absent).
fn dict_f64(dict: &VarDictionary, key: &str) -> f64 {
    dict.get(key)
        .map(|value| variant_f64(&value))
        .unwrap_or(0.0)
}

/// `str(row.get(key, ""))` on a dictionary (`""` when absent).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    match dict.get(key) {
        Some(value) if value.get_type() == VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// `row.get(key, [])` as an untyped array.
fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    dict.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// `assets.bundle_get(key)` with the key echoed back when it resolves empty
/// (`MindWidgets` bundle lookups plus the wave-graph `_bundle` fallback).
fn bundle(key: &str) -> String {
    let Some(assets) = kit::assets() else {
        return key.to_string();
    };
    let value = assets.bind().bundle_get(GString::from(key)).to_string();
    if value.is_empty() {
        key.to_string()
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// MapEditorDialog (`map_editor_dialog.gd`, `MapEditorDialog.java`)
// ---------------------------------------------------------------------------

/// `build()`: tool button node name -> `EditorTool` name.
const TOOL_BUTTONS: [(&str, &str); 7] = [
    ("ZoomButton", "zoom"),
    ("PickButton", "pick"),
    ("LineButton", "line"),
    ("PencilButton", "pencil"),
    ("EraserButton", "eraser"),
    ("FillButton", "fill"),
    ("SprayButton", "spray"),
];
/// `float size = mobile ? 50f : 58f`.
const TOOL_SIZE: f32 = 58.0;
/// `rebuildBlockSelection`: `blockSelection.add(button).size(50f)`.
const BLOCK_BUTTON_SIZE: f32 = 50.0;
/// `button.resizeImage(8 * 4f)`.
const BLOCK_ICON_SIZE: f32 = 32.0;
/// `search.image(Icon.zoom)`: `Fonts.icon` glyphs are 30 px.
const SEARCH_ICON_SIZE: i32 = 30;
/// `Tex.underline` padding (the 9-patch's 12 px pads) around the block name.
const UNDERLINE_PAD: f32 = 12.0;
/// Arc's line box is 2 px taller than Godot's, so the name row splits 14/12.
const UNDERLINE_PAD_TOP: f32 = 14.0;
const UNDERLINE_PAD_BOTTOM: f32 = 12.0;
/// `scroll-knob-vertical-thin` width (`Styles.smallPane`).
const SMALL_PANE_BAR: f32 = 12.0;
/// `_last_block` value that never matches a block name (forces a refresh).
const NO_BLOCK: &str = "<none>";

/// `MindMapEditorDialog` — the full-screen editor shell over `Ui/EditorDialog`.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindMapEditorDialog {
    base: Base<Control>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    brush_sizes: Vec<i64>,
    /// Team id -> its `clearNoneTogglei` button (`teamgroup`).
    team_buttons: Vec<(i64, Gd<Button>)>,
    /// Block name -> its palette button (`blockSelection`).
    block_buttons: Vec<(String, Gd<Button>)>,
    /// Tool name -> the `M<n>` mode label stacked on its button.
    mode_labels: Vec<(String, Gd<Label>)>,
    /// Tool name -> last seen alternate mode (`EditorTool.mode`).
    tool_modes: Vec<(String, i64)>,
    undo_glyph: Option<Gd<Label>>,
    redo_glyph: Option<Gd<Label>>,
    rotate_glyph: Option<Gd<Label>>,
    last_block: String,
    /// A `MindUi` confirm for `@editor.unsaved` is awaiting an answer.
    pending_exit: bool,

    tool_grid: Option<Gd<GridContainer>>,
    search: Option<Gd<LineEdit>>,
    selected: Option<Gd<Label>>,
    config_panel: Option<Gd<VBoxContainer>>,
    config_label: Option<Gd<Label>>,
    block_grid: Option<Gd<GridContainer>>,
    team_grid: Option<Gd<GridContainer>>,
    brush: Option<Gd<HSlider>>,
}

#[godot_api]
impl IControl for MindMapEditorDialog {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            brush_sizes: Vec::new(),
            team_buttons: Vec::new(),
            block_buttons: Vec::new(),
            mode_labels: Vec::new(),
            tool_modes: Vec::new(),
            undo_glyph: None,
            redo_glyph: None,
            rotate_glyph: None,
            last_block: NO_BLOCK.to_string(),
            pending_exit: false,
            tool_grid: None,
            search: None,
            selected: None,
            config_panel: None,
            config_label: None,
            block_grid: None,
            team_grid: None,
            brush: None,
        }
    }

    fn ready(&mut self) {
        self.editor = self.resolve_editor();
        self.tool_grid = self.node_as::<GridContainer>("LeftTools/ToolGrid");
        self.search = self.node_as::<LineEdit>("RightPalette/SearchPad/SearchRow/SearchField");
        self.selected = self.node_as::<Label>("RightPalette/SelectedPanel/SelectedLabel");
        self.config_panel = self.node_as::<VBoxContainer>("RightPalette/ConfigPanel");
        self.config_label = self.node_as::<Label>("RightPalette/ConfigPanel/ConfigLabel");
        self.block_grid = self.node_as::<GridContainer>("RightPalette/BlockScroll/BlockGrid");
        self.team_grid = self.node_as::<GridContainer>("LeftTools/TeamGrid");
        self.brush = self.node_as::<HSlider>("LeftTools/BrushStack/BrushSlider");
        self.apply_theme();
        self.apply_labels();
        self.bind_toolbar();
        self.bind_toggles();
        self.build_teams();
        self.build_brush();
        if self.editor.is_some() {
            self.rebuild_palette();
        }
        self.refresh();
        self.connect_confirm();
    }

    fn process(&mut self, _delta: f64) {
        if self.base().is_visible() {
            self.refresh();
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible() || self.editor().is_none() {
            return;
        }
        // `update(() -> { if(hasKeyboard()) doInput(); })`: dialogs/prompts on
        // top own the keys (UiRoot's Escape closes them).
        if self.has_modal() {
            return;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() {
            return;
        }
        let code = key.get_keycode();
        if key.is_ctrl_pressed() {
            match code {
                Key::Z => {
                    if key.is_shift_pressed() {
                        self.call_editor("redo", &[]);
                    } else {
                        self.call_editor("undo", &[]);
                    }
                }
                Key::Y => self.call_editor("redo", &[]),
                Key::S => self.call_editor("save", &[]),
                Key::G => self.toggle_grid(),
                // `_call("set_tool_mode", _number_index(key))`.
                Key::KEY_1 => self.call_editor("set_tool_mode", &[0i64.to_variant()]),
                Key::KEY_2 => self.call_editor("set_tool_mode", &[1i64.to_variant()]),
                Key::KEY_3 => self.call_editor("set_tool_mode", &[2i64.to_variant()]),
                Key::KEY_4 => self.call_editor("set_tool_mode", &[3i64.to_variant()]),
                Key::KEY_5 => self.call_editor("set_tool_mode", &[4i64.to_variant()]),
                _ => {}
            }
            self.consume_input();
            return;
        }
        match code {
            // `MapEditorDialog.doInput`: Escape opens the menu; it does not exit.
            Key::ESCAPE => self.open_menu(),
            Key::V => self.apply_tool("zoom"),
            Key::I => self.apply_tool("pick"),
            Key::L => self.apply_tool("line"),
            Key::B => self.apply_tool("pencil"),
            Key::E => {
                self.apply_tool("eraser");
                self.rotate(-1);
            }
            Key::G => self.apply_tool("fill"),
            Key::R => {
                self.apply_tool("spray");
                self.rotate(1);
            }
            _ => return,
        }
        // `MapEditorDialog.doInput` consumes its keys; UiRoot's Escape handler
        // would otherwise open the pause menu behind the editor shell.
        self.consume_input();
    }
}

#[godot_api]
impl MindMapEditorDialog {
    /// Shows the editor dialog (MCP / menu / `EditorMapsDialog` entry point).
    ///
    /// `UiRoot.set_editor_open` hides the standalone menu and the in-game HUD,
    /// closes the config/inventory panels and lets editor dialogs take the
    /// pointer above the shell. The call is deferred because `UiRoot` invokes
    /// `show_dialog` from its own bound methods (`check_playtest`).
    #[func]
    pub fn show_dialog(&mut self) {
        self.base_mut().set_visible(true);
        self.refresh();
        self.set_editor_open_deferred(true);
    }

    /// Hides the editor dialog; the caller shows the next surface.
    #[func]
    pub fn hide_dialog(&mut self) {
        self.base_mut().set_visible(false);
        self.drop_temporary_input();
        self.set_editor_open_deferred(false);
    }

    /// True while a `MindUi` dialog or prompt is up over the shell.
    #[func]
    pub fn has_modal(&self) -> bool {
        let Some(mut ui_root) = self.ui_root() else {
            return false;
        };
        if !ui_root.has_method("has_modal") {
            return false;
        }
        ui_root
            .call("has_modal", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    }

    /// `MapEditorDialog.tryExit`: answers through the shared `MindUi` confirm.
    #[func]
    fn _confirm_exit(&mut self) {
        let Some(mut ui) = kit::ui() else {
            self.exit_now();
            return;
        };
        if !ui.has_method("show_confirm") {
            self.exit_now();
            return;
        }
        self.pending_exit = true;
        let text = kit::markup_text(&kit::t("@editor.unsaved").to_string());
        let _ = ui.call("show_confirm", &[text.to_variant()]);
    }

    /// The `MindUi.confirm_result` answer for a pending `_confirm_exit`.
    #[func]
    fn on_confirm_result(&mut self, confirmed: bool) {
        if !self.pending_exit {
            return;
        }
        self.pending_exit = false;
        if confirmed {
            self.exit_now();
        }
    }

    /// `build()`: the first tool-grid button is `Icon.menu` -> `menu.show()`.
    #[func]
    fn open_menu(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("open_dialog") {
            return;
        }
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("editor_menu").to_variant(),
                GString::new().to_variant(),
            ],
        );
    }

    /// One toolbar tool button (`button.pressed.connect(func(): _select_tool(tool))`).
    #[func]
    fn select_tool(&mut self, tool: GString) {
        self.apply_tool(&tool.to_string());
    }

    /// One team swatch (`button.pressed.connect(func(): _select_team(team))`).
    #[func]
    fn select_team(&mut self, team_name: GString) {
        self.call_editor("set_draw_team", &[team_name.to_variant()]);
        self.refresh();
    }

    /// One palette block button (`button.pressed.connect(func(): ...)`).
    #[func]
    fn on_block_selected(&mut self, block_key: GString) {
        self.call_editor("set_draw_block", &[block_key.to_variant()]);
        self.refresh();
    }

    /// `_search.text_changed.connect(func(_text): _rebuild_palette())`.
    #[func]
    fn on_search_changed(&mut self, _text: GString) {
        self.rebuild_palette();
    }

    /// `_brush.value_changed` -> `set_brush_size(sizes[index] / 10f)`.
    #[func]
    fn on_brush_changed(&mut self, value: f64) {
        if self.brush_sizes.is_empty() {
            return;
        }
        let last = self.brush_sizes.len() as i64 - 1;
        let index = (value as i64).clamp(0, last) as usize;
        let radius = self.brush_sizes.get(index).copied().unwrap_or(0) as f64 / 10.0;
        self.call_editor("set_brush_size", &[radius.to_variant()]);
    }

    /// `editor.rotation = (editor.rotation + 1) % 4`.
    #[func]
    fn rotate(&mut self, delta: i64) {
        let Some(editor) = self.editor() else {
            return;
        };
        let current = editor_call(&editor, "rotation", &[])
            .map(|value| variant_i64(&value))
            .unwrap_or(0);
        self.call_editor("set_rotation", &[(current + delta).to_variant()]);
    }

    /// `_toggle_grid()`: `set_grid(!editor.grid)`.
    #[func]
    fn toggle_grid(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let grid = editor_call(&editor, "grid", &[])
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false);
        self.call_editor("set_grid", &[(!grid).to_variant()]);
    }

    /// `mid.button("@editor.center", Icon.move, Styles.flatt, view::center)`.
    #[func]
    fn center_view(&mut self) {
        self.call_editor("map_view_center", &[]);
    }

    /// `undo` toolbar button.
    #[func]
    fn on_undo(&mut self) {
        self.call_editor("undo", &[]);
    }

    /// `redo` toolbar button.
    #[func]
    fn on_redo(&mut self) {
        self.call_editor("redo", &[]);
    }

    /// `set_show_buildings(pressed)`.
    #[func]
    fn on_show_blocks(&mut self, pressed: bool) {
        self.call_editor("set_show_buildings", &[pressed.to_variant()]);
    }

    /// `set_show_terrain(pressed)`.
    #[func]
    fn on_show_terrain(&mut self, pressed: bool) {
        self.call_editor("set_show_terrain", &[pressed.to_variant()]);
    }

    /// `set_show_floor(pressed)`.
    #[func]
    fn on_show_floor(&mut self, pressed: bool) {
        self.call_editor("set_show_floor", &[pressed.to_variant()]);
    }
}

impl MindMapEditorDialog {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// The live `MindEditor` facade from the exported path.
    fn resolve_editor(&self) -> Option<Gd<Node>> {
        let path = self.editor_path.clone();
        self.base().get_node_or_null(&path)
    }

    /// The stored facade, re-resolved when the exported path changed.
    fn editor(&self) -> Option<Gd<Node>> {
        if let Some(editor) = &self.editor
            && editor.is_instance_valid()
        {
            return Some(editor.clone());
        }
        self.resolve_editor()
    }

    /// `/root/Spine/Ui/UiRoot` (`_ui_root()`).
    fn ui_root(&self) -> Option<Gd<Node>> {
        kit::node("/root/Spine/Ui/UiRoot")
    }

    /// `_call(method, ...)` with the facade's graceful degradation.
    fn call_editor(&self, method: &str, args: &[Variant]) {
        let Some(editor) = self.editor() else {
            return;
        };
        let _ = editor_call(&editor, method, args);
    }

    /// `get_viewport().set_input_as_handled(); accept_event()`.
    fn consume_input(&mut self) {
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
        self.base_mut().accept_event();
    }

    /// `UiRoot.set_editor_open(...)`, queued out of the caller's borrow.
    fn set_editor_open_deferred(&self, open: bool) {
        let Some(mut ui_root) = self.ui_root() else {
            return;
        };
        defer_action("editor_open", move || {
            if !ui_root.is_instance_valid() || !ui_root.has_method("set_editor_open") {
                return;
            }
            let _ = ui_root.call("set_editor_open", &[open.to_variant()]);
        });
    }

    /// The shell is a sibling of `UiRoot`, so it takes the shared skin
    /// explicitly (`UiRoot._enter_tree` builds the theme before any `_ready`).
    fn apply_theme(&mut self) {
        let Some(ui_root) = self.ui_root() else {
            return;
        };
        let Ok(ui_root) = ui_root.try_cast::<Control>() else {
            return;
        };
        let Some(theme) = ui_root.get_theme() else {
            return;
        };
        self.base_mut().set_theme(&theme);
    }

    /// Bundle text and icon-font glyphs for the statically declared controls.
    fn apply_labels(&mut self) {
        self.set_glyph("LeftTools/ToolGrid/MenuButton", "menu", "menu");
        self.set_glyph("LeftTools/ToolGrid/GridButton", "grid", "grid");
        self.undo_glyph = self.set_glyph("LeftTools/ToolGrid/UndoButton", "undo", "undo");
        self.redo_glyph = self.set_glyph("LeftTools/ToolGrid/RedoButton", "redo", "redo");
        self.rotate_glyph = self.set_glyph("LeftTools/ToolGrid/RotateButton", "right", "rotate");
        for (button_name, tool) in TOOL_BUTTONS {
            let Some(button) = self.tool_button(button_name) else {
                continue;
            };
            self.set_glyph_on(&button, tool, tool);
            let label = self.add_mode_label(&button);
            self.mode_labels.push((tool.to_string(), label));
        }
        // `search.image(Icon.zoom).padRight(8)`: the icon at its natural size.
        if let Some(mut icon) = self.node_as::<Label>("RightPalette/SearchPad/SearchRow/SearchIcon")
        {
            icon.set_text(&icons::glyph("zoom"));
            let mut control: Gd<Control> = icon.clone().upcast();
            icons::apply_icon_font(&mut control);
            icon.add_theme_font_size_override("font_size", SEARCH_ICON_SIZE);
        }
        if let Some(mut search) = self.search.clone() {
            search.set_placeholder(&kit::t("@players.search"));
        }
        self.apply_block_panel_styles();
        if let Some(mut label) = self.node_as::<Label>("LeftTools/BrushStack/BrushLabel") {
            label.set_text(&kit::t("@editor.brush"));
        }
        for (path, key) in [
            (
                "LeftTools/ChecksPad/ShowChecks/ShowBlocks",
                "@editor.showblocks",
            ),
            (
                "LeftTools/ChecksPad/ShowChecks/ShowTerrain",
                "@editor.showterrain",
            ),
            (
                "LeftTools/ChecksPad/ShowChecks/ShowFloor",
                "@editor.showfloor",
            ),
        ] {
            if let Some(mut check) = self.node_as::<Button>(path) {
                check.set_text(&kit::t(key));
            }
        }
        let center = kit::t("@editor.center").to_string();
        self.set_icon_text("LeftTools/CenterButton", "move", &center);
    }

    /// `addBlockSelection` (MapEditorDialog.java:797-832) drawables the theme
    /// has no type for: the `underline` name table and the thin pane bars.
    fn apply_block_panel_styles(&mut self) {
        if let Some(underline) = kit::icon_texture("underline") {
            let mut box_ = StyleBoxTexture::new_gd();
            box_.set_texture(&underline);
            box_.set_texture_margin_all(UNDERLINE_PAD);
            box_.set_content_margin_all(UNDERLINE_PAD);
            box_.set_content_margin(Side::TOP, UNDERLINE_PAD_TOP);
            box_.set_content_margin(Side::BOTTOM, UNDERLINE_PAD_BOTTOM);
            if let Some(mut panel) = self.node_as::<PanelContainer>("RightPalette/SelectedPanel") {
                panel.add_theme_stylebox_override("panel", &box_);
            }
        }
        let Some(scroll) = self.node_as::<ScrollContainer>("RightPalette/BlockScroll") else {
            return;
        };
        let Some(mut bar) = scroll.get_v_scroll_bar() else {
            return;
        };
        let mut track = StyleBoxEmpty::new_gd();
        track.set_content_margin(Side::LEFT, SMALL_PANE_BAR * 0.5);
        track.set_content_margin(Side::RIGHT, SMALL_PANE_BAR * 0.5);
        bar.add_theme_stylebox_override("scroll", &track);
        bar.add_theme_stylebox_override("scroll_focus", &track);
        if let Some(thin) = kit::icon_texture("scroll-knob-vertical-thin") {
            let mut knob = StyleBoxTexture::new_gd();
            knob.set_texture(&thin);
            for slot in ["grabber", "grabber_highlight", "grabber_pressed"] {
                bar.add_theme_stylebox_override(slot, &knob);
            }
        }
    }

    /// `mid.button("@editor.center", Icon.move, Styles.flatt, view::center)`:
    /// an image text button — the icon at the left margin, the label centred.
    fn set_icon_text(&self, path: &str, icon_name: &str, text: &str) {
        let Some(mut button) = self.node_as::<Button>(path) else {
            return;
        };
        button.set_text(text);
        if icons::glyph(icon_name).is_empty() {
            return;
        }
        button.set_text("");
        // code-instantiated: icon-font glyphs need the `MindAssets` icon font.
        let mut row = HBoxContainer::new_alloc();
        row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        row.set_offset(Side::LEFT, 9.0);
        row.set_offset(Side::RIGHT, -9.0);
        row.set_mouse_filter(MouseFilter::IGNORE);
        // code-instantiated: the icon-font glyph cell of the icon text button.
        let mut glyph = factory::glyph_label(icon_name, 28, Color::WHITE);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&glyph);
        // code-instantiated: the centred label cell of the icon text button.
        let mut label = Label::new_alloc();
        label.set_text(text);
        label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        label.set_vertical_alignment(VerticalAlignment::CENTER);
        label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        label.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&label);
        button.add_child(&row);
    }

    /// `_set_glyph`: the icon-font glyph filling its button, or the text
    /// fallback when the glyph is unknown.
    fn set_glyph(&self, path: &str, icon_name: &str, fallback: &str) -> Option<Gd<Label>> {
        let button = self.node_as::<Button>(path)?;
        self.set_glyph_on(&button, icon_name, fallback)
    }

    /// [`Self::set_glyph`] on an already resolved button.
    fn set_glyph_on(
        &self,
        button: &Gd<Button>,
        icon_name: &str,
        fallback: &str,
    ) -> Option<Gd<Label>> {
        let mut button = button.clone();
        button.set_text("");
        if icons::glyph(icon_name).is_empty() {
            button.set_text(fallback);
            return None;
        }
        // code-instantiated: icon-font glyphs need the `MindAssets` icon font.
        let mut glyph = factory::glyph_label(icon_name, 28, Color::WHITE);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&glyph);
        Some(glyph)
    }

    /// `tools.stack(button, mode)`: the `M<n>` alt-mode label in `Pal.remove`.
    fn add_mode_label(&self, button: &Gd<Button>) -> Gd<Label> {
        // code-instantiated: the alternate-mode badge of one tool button.
        let mut label = Label::new_alloc();
        label.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        label.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        label.set_vertical_alignment(VerticalAlignment::BOTTOM);
        label.set_mouse_filter(MouseFilter::IGNORE);
        label.add_theme_color_override("font_color", styles::REMOVE);
        label.add_theme_font_size_override("font_size", 14);
        let mut button = button.clone();
        button.add_child(&label);
        label
    }

    /// `MapEditorDialog.tryExit` answers through `MindUi.confirm_result`.
    fn connect_confirm(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_signal("confirm_result") {
            return;
        }
        let target = self.to_gd();
        let callable = Callable::from_fn("map_editor_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            let target = target.clone();
            defer_action("map_editor_confirm_deferred", move || {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                let _ = node.call("on_confirm_result", &[confirmed.to_variant()]);
            });
        });
        ui.connect("confirm_result", &callable);
    }

    /// The toolbar wiring of `_bind_toolbar()`.
    fn bind_toolbar(&mut self) {
        if let Some(mut menu) = self.node_as::<Button>("LeftTools/ToolGrid/MenuButton") {
            menu.connect("pressed", &self.base().callable("open_menu"));
        }
        for (button_name, tool) in TOOL_BUTTONS {
            let Some(mut button) = self.tool_button(button_name) else {
                continue;
            };
            let mut args = VarArray::new();
            args.push(&GString::from(tool).to_variant());
            button.connect("pressed", &self.base().callable("select_tool").bindv(&args));
        }
        if let Some(mut grid) = self.node_as::<Button>("LeftTools/ToolGrid/GridButton") {
            grid.connect("pressed", &self.base().callable("toggle_grid"));
        }
        if let Some(mut undo) = self.node_as::<Button>("LeftTools/ToolGrid/UndoButton") {
            undo.connect("pressed", &self.base().callable("on_undo"));
        }
        if let Some(mut redo) = self.node_as::<Button>("LeftTools/ToolGrid/RedoButton") {
            redo.connect("pressed", &self.base().callable("on_redo"));
        }
        if let Some(mut rotate) = self.node_as::<Button>("LeftTools/ToolGrid/RotateButton") {
            let mut args = VarArray::new();
            args.push(&1i64.to_variant());
            rotate.connect("pressed", &self.base().callable("rotate").bindv(&args));
        }
        if let Some(mut center) = self.node_as::<Button>("LeftTools/CenterButton") {
            center.connect("pressed", &self.base().callable("center_view"));
        }
        if let Some(mut search) = self.search.clone() {
            search.connect("text_changed", &self.base().callable("on_search_changed"));
        }
    }

    /// The show toggles of `_bind_toggles()`.
    fn bind_toggles(&mut self) {
        for (path, handler) in [
            (
                "LeftTools/ChecksPad/ShowChecks/ShowBlocks",
                "on_show_blocks",
            ),
            (
                "LeftTools/ChecksPad/ShowChecks/ShowTerrain",
                "on_show_terrain",
            ),
            ("LeftTools/ChecksPad/ShowChecks/ShowFloor", "on_show_floor"),
        ] {
            if let Some(mut check) = self.node_as::<Button>(path) {
                check.connect("toggled", &self.base().callable(handler));
            }
        }
    }

    /// `for(Team team : Team.baseTeams)`: one tinted toggle per base team.
    fn build_teams(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let Some(mut grid) = self.team_grid.clone() else {
            return;
        };
        let rows = editor_call(&editor, "palette_teams", &[])
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        for row in rows.iter_shared() {
            let value = row.try_to::<VarDictionary>().unwrap_or_default();
            let id = dict_i64(&value, "id");
            let team_name = dict_string(&value, "name");
            // code-instantiated: team buttons are data-driven from the
            // base-team registry.
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("clearNoneTogglei");
            button.set_toggle_mode(true);
            button.set_custom_minimum_size(Vector2::new(TOOL_SIZE, TOOL_SIZE));
            // code-instantiated: the team colour swatch (`Tex.whiteui` tint).
            let mut swatch = ColorRect::new_alloc();
            swatch.set_anchors_preset(LayoutPreset::FULL_RECT);
            swatch.set_offset(Side::LEFT, 4.0);
            swatch.set_offset(Side::TOP, 4.0);
            swatch.set_offset(Side::RIGHT, -4.0);
            swatch.set_offset(Side::BOTTOM, -4.0);
            swatch.set_mouse_filter(MouseFilter::IGNORE);
            let packed = dict_i64(&value, "color");
            swatch.set_color(Color::from_rgba8(
                ((packed >> 24) & 0xFF) as u8,
                ((packed >> 16) & 0xFF) as u8,
                ((packed >> 8) & 0xFF) as u8,
                255,
            ));
            button.add_child(&swatch);
            let mut args = VarArray::new();
            args.push(&GString::from(team_name.as_str()).to_variant());
            button.connect("pressed", &self.base().callable("select_team").bindv(&args));
            grid.add_child(&button);
            self.team_buttons.push((id, button));
        }
    }

    /// `_build_brush()`: the brush radius slider over `editor.brush_sizes()`.
    fn build_brush(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        self.brush_sizes.clear();
        let sizes = editor_call(&editor, "brush_sizes", &[])
            .and_then(|value| value.try_to::<PackedInt64Array>().ok())
            .unwrap_or_default();
        self.brush_sizes.extend(sizes.as_slice().iter().copied());
        if self.brush_sizes.is_empty() {
            return;
        }
        let Some(mut brush) = self.brush.clone() else {
            return;
        };
        brush.set_max((self.brush_sizes.len() - 1) as f64);
        brush.set_value(0.0);
        brush.connect("value_changed", &self.base().callable("on_brush_changed"));
    }

    /// `rebuildBlockSelection(searchText)`: the Rust palette order, filtered by
    /// the localized block name, six 50 px toggles per row.
    fn rebuild_palette(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let Some(mut grid) = self.block_grid.clone() else {
            return;
        };
        for child in grid.get_children().iter_shared() {
            let mut child = child.clone();
            grid.remove_child(&child);
            child.queue_free();
        }
        self.block_buttons.clear();
        self.last_block = NO_BLOCK.to_string();
        let mut search = String::new();
        if let Some(field) = &self.search {
            search = field
                .get_text()
                .to_string()
                .trim()
                .split(' ')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
        }
        let rows = editor_call(&editor, "palette_blocks", &[GString::new().to_variant()])
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        for row in rows.iter_shared() {
            let value = row.try_to::<VarDictionary>().unwrap_or_default();
            let block_key = dict_string(&value, "name");
            let localized = self.block_name(&block_key);
            if !search.is_empty() && !localized.to_lowercase().contains(&search) {
                continue;
            }
            let Some(texture) = kit::icon_texture(&format!("block-{block_key}-ui")) else {
                continue;
            };
            // code-instantiated: one palette button per editor block
            // (content-driven count).
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("clearNoneTogglei");
            button.set_toggle_mode(true);
            button.set_custom_minimum_size(Vector2::new(BLOCK_BUTTON_SIZE, BLOCK_BUTTON_SIZE));
            button.set_tooltip_text(&localized);
            // code-instantiated: the block's `uiIcon` inside its palette cell.
            let mut icon = TextureRect::new_alloc();
            icon.set_texture(&texture);
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_mouse_filter(MouseFilter::IGNORE);
            icon.set_anchors_preset(LayoutPreset::CENTER);
            let half = BLOCK_ICON_SIZE / 2.0;
            icon.set_offset(Side::LEFT, -half);
            icon.set_offset(Side::TOP, -half);
            icon.set_offset(Side::RIGHT, half);
            icon.set_offset(Side::BOTTOM, half);
            button.add_child(&icon);
            let mut args = VarArray::new();
            args.push(&GString::from(block_key.as_str()).to_variant());
            button.connect(
                "pressed",
                &self.base().callable("on_block_selected").bindv(&args),
            );
            grid.add_child(&button);
            self.block_buttons.push((block_key, button));
        }
    }

    /// `block.localizedName`: `Core.bundle.get("block." + name + ".name", name)`.
    fn block_name(&self, block_key: &str) -> String {
        if block_key.is_empty() {
            return String::new();
        }
        let key = format!("block.{block_key}.name");
        let resolved = kit::t(&key).to_string();
        if resolved == key || resolved.is_empty() {
            block_key.to_string()
        } else {
            resolved
        }
    }

    /// `_select_tool(tool)`: `set_tool(tool)` + refresh.
    fn apply_tool(&mut self, tool: &str) {
        self.call_editor("set_tool", &[GString::from(tool).to_variant()]);
        self.refresh();
    }

    /// The per-frame `update(...)` hooks of `build()`.
    fn refresh(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let status = editor_call(&editor, "status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let tool = dict_string(&status, "tool");
        let tool_mode = dict_i64(&status, "tool_mode");
        self.set_tool_mode(&tool, tool_mode);
        for (button_name, tool_name) in TOOL_BUTTONS {
            let Some(mut button) = self.tool_button(button_name) else {
                continue;
            };
            button.set_pressed_no_signal(tool_name == tool.as_str());
            let mode = self.tool_mode_for(tool_name);
            if let Some(mut label) = self.mode_label(tool_name) {
                label.set_text(&if mode == -1 {
                    String::new()
                } else {
                    format!("M{} ", mode + 1)
                });
            }
        }
        if let Some(mut grid_button) = self.node_as::<Button>("LeftTools/ToolGrid/GridButton") {
            grid_button.set_pressed_no_signal(dict_bool(&status, "grid"));
        }
        let undo_glyph = self.undo_glyph.clone();
        self.refresh_undo(
            "LeftTools/ToolGrid/UndoButton",
            &undo_glyph,
            dict_bool(&status, "can_undo"),
        );
        let redo_glyph = self.redo_glyph.clone();
        self.refresh_undo(
            "LeftTools/ToolGrid/RedoButton",
            &redo_glyph,
            dict_bool(&status, "can_redo"),
        );
        if let Some(mut glyph) = self.rotate_glyph.clone() {
            // Arc turns counter-clockwise (y-up); Godot's y-down turns
            // clockwise.
            let size = glyph.get_size();
            glyph.set_pivot_offset(size / 2.0);
            glyph.set_rotation_degrees(-90.0 * dict_i64(&status, "rotation") as f32);
        }
        let team = dict_i64(&status, "draw_team");
        for (id, button) in &mut self.team_buttons {
            button.set_pressed_no_signal(*id == team);
        }
        let block = dict_string(&status, "draw_block");
        if block != self.last_block {
            self.last_block = block.clone();
            for (name, button) in &mut self.block_buttons {
                button.set_pressed_no_signal(*name == block);
            }
            if let Some(mut selected) = self.selected.clone() {
                // `extra.labelWrap(() -> editor.drawBlock.localizedName)`.
                selected.set_text(&self.block_name(&block));
            }
        }
        self.refresh_config(&block);
    }

    /// `undo.setDisabled(() -> !editor.canUndo())` + the gray image tint.
    fn refresh_undo(&self, path: &str, glyph: &Option<Gd<Label>>, enabled: bool) {
        if let Some(mut button) = self.node_as::<Button>(path) {
            button.set_disabled(!enabled);
        }
        if let Some(glyph) = glyph
            && glyph.is_instance_valid()
        {
            let mut glyph = glyph.clone();
            glyph.set_modulate(if enabled {
                Color::WHITE
            } else {
                styles::ARC_GRAY
            });
        }
    }

    /// `_refresh_config(block)`: the `block_info` configurable row.
    fn refresh_config(&mut self, block: &str) {
        let Some(editor) = self.editor() else {
            return;
        };
        let info = editor_call(&editor, "block_info", &[GString::from(block).to_variant()])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let configurable = dict_bool(&info, "configurable");
        if let Some(mut panel) = self.config_panel.clone() {
            panel.set_visible(configurable);
        }
        if configurable && let Some(mut label) = self.config_label.clone() {
            label.set_text(&format!(
                "config: {block} (size {})",
                dict_i64(&info, "size")
            ));
        }
    }

    /// Releases any in-progress stroke / temporary tool in the view.
    fn drop_temporary_input(&self) {
        let Some(mut view) = self.base().get_node_or_null("MapView") else {
            return;
        };
        if !view.has_method("cancel_input") {
            return;
        }
        let _ = view.call("cancel_input", &[]);
    }

    /// The accept path: `logic.reset(); hide()` (`MindEditor.try_exit`).
    fn exit_now(&mut self) {
        if let Some(editor) = self.editor() {
            let _ = editor_call(&editor, "try_exit", &[]);
        }
        self.exit_to_menu();
    }

    /// Closing the editor returns to the main menu (its `tryExit` confirm runs
    /// `logic.reset(); hide()` and `MenuFragment`'s visibility group brings the
    /// menu back). Every open dialog on top (the editor menu) closes too.
    fn exit_to_menu(&mut self) {
        self.hide_dialog();
        if let Some(mut ui) = kit::ui()
            && ui.has_method("close_dialog")
        {
            let _ = ui.call("close_dialog", &[GString::from("editor_menu").to_variant()]);
        }
        if let Some(mut ui_root) = self.ui_root()
            && ui_root.has_method("return_to_menu")
        {
            let _ = ui_root.call("return_to_menu", &[]);
        }
    }

    /// One tool button from the static tool grid.
    fn tool_button(&self, name: &str) -> Option<Gd<Button>> {
        let grid = self.tool_grid.as_ref()?;
        grid.get_node_or_null(name)?.try_cast::<Button>().ok()
    }

    /// The `M<n>` label of one tool (valid handles only).
    fn mode_label(&self, tool: &str) -> Option<Gd<Label>> {
        self.mode_labels
            .iter()
            .find(|(name, label)| name == tool && label.is_instance_valid())
            .map(|(_, label)| label.clone())
    }

    /// The last seen alternate mode of one tool (`-1` when unknown).
    fn tool_mode_for(&self, tool: &str) -> i64 {
        self.tool_modes
            .iter()
            .find(|(name, _)| name == tool)
            .map(|(_, mode)| *mode)
            .unwrap_or(-1)
    }

    /// `_tool_modes[tool] = status.tool_mode`.
    fn set_tool_mode(&mut self, tool: &str, mode: i64) {
        if let Some(entry) = self.tool_modes.iter_mut().find(|(name, _)| name == tool) {
            entry.1 = mode;
            return;
        }
        self.tool_modes.push((tool.to_string(), mode));
    }
}

// ---------------------------------------------------------------------------
// MapView (`map_view.gd`, `MapView.java`)
// ---------------------------------------------------------------------------

/// `MindMapView` — the editor's pan/zoom/paint surface.
///
/// Pointer events are forwarded to `/root/Spine/MindEditor`; all projection and
/// drawing primitives come back from Rust. The `ViewportContainer/SubViewport`
/// camera mirrors the editor projection (`_drive_view`) and the brush outline /
/// grid overlay is drawn on top (`_draw`).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindMapView {
    base: Base<Control>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    cursor: Vector2i,
    drawing: bool,
    last: Vector2i,
    start: Vector2i,
    last_tool: String,
    world_view: Option<Gd<Node>>,
    camera: Option<Gd<Camera2D>>,
    /// Middle-drag pan (`MapView.pan` while the temporary `zoom` tool is active).
    panning: bool,
    pan_position: Vector2,
}

#[godot_api]
impl IControl for MindMapView {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            cursor: Vector2i::new(-1, -1),
            drawing: false,
            last: Vector2i::new(-1, -1),
            start: Vector2i::new(-1, -1),
            last_tool: String::new(),
            world_view: None,
            camera: None,
            panning: false,
            pan_position: Vector2::ZERO,
        }
    }

    fn ready(&mut self) {
        self.editor = self.resolve_editor();
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
        self.camera = self.node_as::<Camera2D>("ViewportContainer/SubViewport/Camera2D");
        self.world_view = self.node_as::<Node>("ViewportContainer/SubViewport/EditorWorldView");
    }

    fn process(&mut self, delta: f64) {
        // Hidden with the shell (playtest / main menu): WASD belongs to the game.
        if !self.base().is_visible_in_tree() {
            return;
        }
        if self.editor().is_some() {
            self.pan_keys(delta);
            self.drive_view();
            self.base_mut().queue_redraw();
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if self.editor().is_none() {
            return;
        }
        if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            self.cursor = self.tile_at(motion.get_position());
            // `MapView.pan` while the middle-drag `zoom` tool is active.
            if self.panning {
                let delta = motion.get_position() - self.pan_position;
                self.pan_position = motion.get_position();
                self.call_editor(
                    "map_view_pan",
                    &[delta.x.to_variant(), delta.y.to_variant()],
                );
            } else if self.drawing {
                self.drag_to(self.cursor);
            }
            self.base_mut().queue_redraw();
            self.base_mut().accept_event();
        } else if let Ok(button) = event.try_cast::<InputEventMouseButton>() {
            let index = button.get_button_index();
            let pressed = button.is_pressed();
            if index == MouseButton::WHEEL_UP && pressed {
                self.call_editor("map_view_zoom", &[1.0f64.to_variant()]);
            } else if index == MouseButton::WHEEL_DOWN && pressed {
                self.call_editor("map_view_zoom", &[(-1.0f64).to_variant()]);
            } else if index == MouseButton::LEFT {
                if pressed {
                    let tile = self.tile_at(button.get_position());
                    self.begin(tile, false);
                } else {
                    self.end();
                }
            } else if index == MouseButton::RIGHT {
                if pressed {
                    let tile = self.tile_at(button.get_position());
                    self.begin(tile, true);
                } else {
                    self.end();
                }
            } else if index == MouseButton::MIDDLE {
                if pressed {
                    self.begin_pan(button.get_position());
                } else {
                    self.end_pan();
                }
            }
            self.base_mut().accept_event();
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if self.editor().is_none() || !self.base().is_visible_in_tree() {
            return;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if key.is_echo() {
            return;
        }
        let code = key.get_keycode();
        if code != Key::SHIFT && code != Key::ALT {
            return;
        }
        if key.is_pressed() {
            if self.modal() {
                return;
            }
            // `MapView.act`: Shift or Alt swaps to the temporary `pick` tool.
            if self.last_tool.is_empty() {
                self.last_tool = self.tool();
                self.set_tool("pick");
            }
        } else if !self.last_tool.is_empty() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
            self.last_tool.clear();
        }
    }

    fn draw(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let status = editor_call(&editor, "status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let map_w = dict_i64(&status, "width");
        let map_h = dict_i64(&status, "height");
        if map_w <= 0 || map_h <= 0 {
            return;
        }
        if dict_bool(&status, "grid") {
            self.draw_grid(map_w as i32, map_h as i32);
        }
        if self.cursor.x >= 0 && self.cursor.y >= 0 {
            let size = self.base().get_size();
            let points = editor_call(
                &editor,
                "map_view_brush_outline",
                &[
                    self.cursor.x.to_variant(),
                    self.cursor.y.to_variant(),
                    size.x.to_variant(),
                    size.y.to_variant(),
                ],
            )
            .and_then(|value| value.try_to::<PackedVector2Array>().ok())
            .unwrap_or_default();
            if points.as_slice().len() >= 2 {
                let mut closed = points.clone();
                if let Some(first) = points.as_slice().first() {
                    closed.push(*first);
                }
                // `Draw.color(Pal.accent); Lines.stroke(Scl.scl(2f))`.
                self.base_mut()
                    .draw_polyline_ex(&closed, styles::ACCENT)
                    .width(2.0)
                    .done();
            }
        }
        // The view frame, drawn last: `Draw.color(Pal.accent);
        // Lines.stroke(Scl.scl(3f)); Lines.rect(x, y, width, height)`. Inset so
        // the 3 px stroke stays inside the clipped view.
        let size = self.base().get_size();
        self.base_mut()
            .draw_rect_ex(
                Rect2::new(Vector2::new(1.5, 1.5), size - Vector2::new(3.0, 3.0)),
                styles::ACCENT,
            )
            .filled(false)
            .width(3.0)
            .done();
    }
}

#[godot_api]
impl MindMapView {
    /// Drops an in-progress stroke/pan and restores a temporary tool (the shell
    /// is hiding; the view would otherwise keep the eraser/zoom/pick swap).
    #[func]
    pub fn cancel_input(&mut self) {
        self.drawing = false;
        self.panning = false;
        self.cursor = Vector2i::new(-1, -1);
        if !self.last_tool.is_empty() && self.editor().is_some() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
        }
        self.last_tool.clear();
        self.base_mut().queue_redraw();
    }
}

impl MindMapView {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// The live `MindEditor` facade from the exported path.
    fn resolve_editor(&self) -> Option<Gd<Node>> {
        let path = self.editor_path.clone();
        self.base().get_node_or_null(&path)
    }

    /// The stored facade, re-resolved when the exported path changed.
    fn editor(&self) -> Option<Gd<Node>> {
        if let Some(editor) = &self.editor
            && editor.is_instance_valid()
        {
            return Some(editor.clone());
        }
        self.resolve_editor()
    }

    /// `_call(method, ...)` with the facade's graceful degradation.
    fn call_editor(&self, method: &str, args: &[Variant]) {
        let Some(editor) = self.editor() else {
            return;
        };
        let _ = editor_call(&editor, method, args);
    }

    /// `str(_editor.call("tool"))`.
    fn tool(&self) -> String {
        let Some(editor) = self.editor() else {
            return String::new();
        };
        editor_call(&editor, "tool", &[])
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    /// `_editor.call("set_tool", tool)`.
    fn set_tool(&self, tool: &str) {
        self.call_editor("set_tool", &[GString::from(tool).to_variant()]);
    }

    /// A dialog or prompt is up over the editor shell (`MapEditorDialog.has_modal`).
    fn modal(&self) -> bool {
        let Some(mut shell) = self.base().get_parent() else {
            return false;
        };
        if !shell.has_method("has_modal") {
            return false;
        }
        shell
            .call("has_modal", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    }

    /// `MapView.act` (`editor/MapView.java:180-185`): pan by the
    /// `move_x`/`move_y` binding axes, `offset -= axis * 15 * delta / zoom`,
    /// unless a text field has focus or Ctrl is held.
    fn pan_keys(&mut self, delta: f64) {
        if self.modal() {
            return;
        }
        if let Some(viewport) = self.base().get_viewport()
            && let Some(focus) = viewport.gui_get_focus_owner()
            && (focus.clone().try_cast::<LineEdit>().is_ok()
                || focus.clone().try_cast::<godot::classes::TextEdit>().is_ok())
        {
            return;
        }
        if Input::singleton().is_key_pressed(Key::CTRL) {
            return;
        }
        let Some(mut input) = kit::node("/root/Spine/Input") else {
            return;
        };
        if !input.has_method("binding_axis") {
            return;
        }
        let ax = variant_f64(&input.call("binding_axis", &[GString::from("move_x").to_variant()]));
        let ay = variant_f64(&input.call("binding_axis", &[GString::from("move_y").to_variant()]));
        if ax != 0.0 || ay != 0.0 {
            // `Time.delta` is in 60 Hz ticks, so one second of `delta` is 60.
            let step = 15.0 * delta * 60.0;
            // `offset_y` is Godot y-down (`-MapView.offsety`): Java's
            // `offsety -= ay * step` is `offset_y += ay * step`.
            self.call_editor(
                "map_view_pan",
                &[(-ax * step).to_variant(), (ay * step).to_variant()],
            );
        }
    }

    /// `MapView.draw`: the `EditorRenderer` is drawn into the view rect
    /// `(centerx - sclwidth/2, centery - sclheight/2, sclwidth, sclheight)`; the
    /// `SubViewport` camera mirrors that projection.
    fn drive_view(&mut self) {
        let (Some(mut camera), Some(mut world_view)) =
            (self.camera.clone(), self.world_view.clone())
        else {
            return;
        };
        let Some(editor) = self.editor() else {
            return;
        };
        let status = editor_call(&editor, "status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let map_w = dict_i64(&status, "width");
        let map_h = dict_i64(&status, "height");
        if map_w <= 0 || map_h <= 0 {
            return;
        }
        let state = editor_call(&editor, "map_view_state", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let zoom = dict_f64(&state, "zoom");
        let zoom = if state.get("zoom").is_some() {
            zoom
        } else {
            1.0
        };
        let offset_x = dict_f64(&state, "offset_x");
        let offset_y = dict_f64(&state, "offset_y");
        let size = self.base().get_size();
        let view = size.x.min(size.y) as f64;
        let scl_w = view * zoom;
        let scl_h = scl_w * map_h as f64 / map_w as f64;
        // View-pixel position of the world origin (tile 0,0 = world 0,0).
        let origin_x = offset_x * zoom - scl_w / 2.0 + size.x as f64 / 2.0;
        let origin_y = offset_y * zoom - scl_h / 2.0 + size.y as f64 / 2.0;
        let scale_px = scl_w / map_w as f64 / 8.0;
        if scale_px == 0.0 {
            return;
        }
        camera.set_position(Vector2::new(
            ((size.x as f64 / 2.0 - origin_x) / scale_px) as f32,
            ((size.y as f64 / 2.0 - origin_y) / scale_px) as f32,
        ));
        camera.set_zoom(Vector2::new(scale_px as f32, scale_px as f32));
        // Visible world-pixel rect for the chunked editor renderer.
        let _ = world_view.call(
            "set_view_rect",
            &[
                ((-origin_x / scale_px) as f32).to_variant(),
                ((-origin_y / scale_px) as f32).to_variant(),
                ((size.x as f64 / scale_px) as f32).to_variant(),
                ((size.y as f64 / scale_px) as f32).to_variant(),
            ],
        );
    }

    /// `MapView.touchDown`: middle swaps to the `zoom` tool until release.
    fn begin_pan(&mut self, pointer_pos: Vector2) {
        self.panning = true;
        self.pan_position = pointer_pos;
        if self.last_tool.is_empty() {
            self.last_tool = self.tool();
            self.set_tool("zoom");
        }
    }

    /// `MapView.touchUp`: the temporary `zoom` tool is restored.
    fn end_pan(&mut self) {
        self.panning = false;
        if !self.last_tool.is_empty() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
            self.last_tool.clear();
        }
    }

    /// `screenToLocalCoordinates`-style cursor tile (`MapView.project`).
    fn tile_at(&self, pointer: Vector2) -> Vector2i {
        let Some(editor) = self.editor() else {
            return Vector2i::new(-1, -1);
        };
        let size = self.base().get_size();
        let tile = editor_call(
            &editor,
            "map_view_project",
            &[
                pointer.x.to_variant(),
                pointer.y.to_variant(),
                size.x.to_variant(),
                size.y.to_variant(),
            ],
        )
        .and_then(|value| value.try_to::<Vector2>().ok());
        match tile {
            Some(tile) => Vector2i::new(tile.x as i32, tile.y as i32),
            None => Vector2i::new(-1, -1),
        }
    }

    /// `MapView.touchDown` on the paint surface.
    fn begin(&mut self, tile: Vector2i, temporary_eraser: bool) {
        self.drawing = true;
        self.last = tile;
        self.start = tile;
        if temporary_eraser {
            self.last_tool = self.tool();
            self.set_tool("eraser");
        }
        self.call_editor(
            "map_view_touch",
            &[tile.x.to_variant(), tile.y.to_variant()],
        );
        self.base_mut().queue_redraw();
    }

    /// `MapView.touchDragged` (`touchDragged` + `Bresenham2.line`).
    fn drag_to(&mut self, tile: Vector2i) {
        if !self.drawing || tile == self.last {
            return;
        }
        if self.tool() == "line" {
            // The line endpoint is committed on release (`touchedLine`).
            self.last = tile;
            return;
        }
        self.call_editor(
            "map_view_drag",
            &[
                self.last.x.to_variant(),
                self.last.y.to_variant(),
                tile.x.to_variant(),
                tile.y.to_variant(),
            ],
        );
        self.last = tile;
    }

    /// `MapView.touchUp`: commit the stroke and restore a temporary tool.
    fn end(&mut self) {
        if !self.drawing {
            return;
        }
        self.drawing = false;
        if self.tool() == "line" {
            self.call_editor(
                "dev_draw_line",
                &[
                    self.start.x.to_variant(),
                    self.start.y.to_variant(),
                    self.last.x.to_variant(),
                    self.last.y.to_variant(),
                ],
            );
        }
        self.call_editor("map_view_flush", &[]);
        if !self.last_tool.is_empty() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
            self.last_tool.clear();
        }
        self.base_mut().queue_redraw();
    }

    /// Tile → view pixel (`MapView.unproject`).
    fn unproject(&self, tile_x: i32, tile_y: i32) -> Option<Vector2> {
        let editor = self.editor()?;
        let size = self.base().get_size();
        editor_call(
            &editor,
            "map_view_unproject",
            &[
                tile_x.to_variant(),
                tile_y.to_variant(),
                size.x.to_variant(),
                size.y.to_variant(),
            ],
        )
        .and_then(|value| value.try_to::<Vector2>().ok())
    }

    /// The quarter/centre grid lines (`MapView.java` grid overlay).
    fn draw_grid(&mut self, map_w: i32, map_h: i32) {
        let quarter = Color::from_rgba(0.98, 0.78, 0.34, 0.65);
        let center = Color::from_rgba(1.0, 0.83, 0.5, 1.0);
        for (fraction, is_center) in [(0.25f64, false), (0.5, true), (0.75, false)] {
            let color = if is_center { center } else { quarter };
            let width = if is_center { 3.0 } else { 2.0 };
            let vx = (map_w as f64 * fraction) as i32;
            let vy = (map_h as f64 * fraction) as i32;
            if let (Some(top), Some(bottom)) = (self.unproject(vx, 0), self.unproject(vx, map_h)) {
                self.base_mut()
                    .draw_line_ex(top, bottom, color)
                    .width(width)
                    .done();
            }
            if let (Some(left), Some(right)) = (self.unproject(0, vy), self.unproject(map_w, vy)) {
                self.base_mut()
                    .draw_line_ex(left, right, color)
                    .width(width)
                    .done();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WaveGraph (`wave_graph.gd`, `WaveGraph.java`)
// ---------------------------------------------------------------------------

/// `Mode` names, indexed by the enum ord (`Mode.COUNTS`, `TOTALS`, `HEALTH`).
const MODE_NAMES: [&str; 3] = ["counts", "totals", "health"];
/// `defaultSpace = Scl.scl(40f)`: the column spacing at zoom 1.
const DEFAULT_SPACE: f32 = 40.0;
/// `Pal.health` (`ff341c`).
const HEALTH_COLOR: Color = Color::from_rgba8(0xff, 0x34, 0x1c, 0xff);
/// `Lines.stroke(Scl.scl(3f))`.
const STROKE: f32 = 3.0;

/// One legend toggle (`colors`): the `fullTogglet` button, its colour swatch
/// and the unit icon.
struct LegendEntry {
    name: String,
    button: Gd<Button>,
    swatch: Gd<ColorRect>,
    icon: Gd<TextureRect>,
}

/// `MindWaveGraph` — the editor's per-wave line plot (`WaveGraph`).
///
/// Presentation only: the per-wave series, the axis maxima (`nextStep`) and the
/// participating units come from `MindEditor.wave_graph(from, to)`; this
/// `PanelContainer` draws the `Tex.pane` table of `WaveGraph` — the line plot
/// (pan, wheel zoom, hover column), the unit legend with its hide toggles and
/// the counts/totals/health mode row.
#[derive(GodotClass)]
#[class(base=PanelContainer)]
pub struct MindWaveGraph {
    base: Base<PanelContainer>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    mode: i64,
    pan: f32,
    zoom: f32,
    from: i64,
    to: i64,
    last_from: i64,
    last_to: i64,
    /// `rebuild()` state: the Rust series for `from..to`.
    data: VarDictionary,
    /// `hidden`: unit names whose lines are hidden.
    hidden: Vec<String>,
    plot: Option<Gd<Control>>,
    colors: Option<Gd<HBoxContainer>>,
    hide_button: Option<Gd<Button>>,
    legend: Vec<LegendEntry>,
    mode_buttons: Vec<Gd<Button>>,
    dragging: bool,
}

#[godot_api]
impl IPanelContainer for MindWaveGraph {
    fn init(base: Base<PanelContainer>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            mode: 0,
            pan: 0.0,
            zoom: 1.0,
            from: 0,
            to: 20,
            last_from: -1,
            last_to: -1,
            data: VarDictionary::new(),
            hidden: Vec::new(),
            plot: None,
            colors: None,
            hide_button: None,
            legend: Vec::new(),
            mode_buttons: Vec::new(),
            dragging: false,
        }
    }

    fn ready(&mut self) {
        self.editor = self.resolve_editor();
        // `background(Tex.pane)`; `MindThemeBuilder._nine_patch` falls back to
        // a flat #141419 box when the region is missing.
        match self.nine_patch("pane", 12.0) {
            Some(pane) => self.base_mut().add_theme_stylebox_override("panel", &pane),
            None => {
                let mut flat = StyleBoxFlat::new_gd();
                flat.set_bg_color(Color::from_rgba(0.08, 0.08, 0.10, 1.0));
                self.base_mut().add_theme_stylebox_override("panel", &flat);
            }
        }
        // code-instantiated: the WaveGraph table rows (plot, legend, modes).
        let mut rows = VBoxContainer::new_alloc();
        rows.add_theme_constant_override("separation", 0);
        self.base_mut().add_child(&rows);
        // code-instantiated: the plot cell's `.pad(4).padBottom(10).grow()`.
        let mut plot_margin = MarginContainer::new_alloc();
        plot_margin.add_theme_constant_override("margin_left", 4);
        plot_margin.add_theme_constant_override("margin_top", 4);
        plot_margin.add_theme_constant_override("margin_right", 4);
        plot_margin.add_theme_constant_override("margin_bottom", 10);
        plot_margin.set_v_size_flags(SizeFlags::EXPAND_FILL);
        rows.add_child(&plot_margin);
        // code-instantiated: the clipped plot canvas (`_plot`), drawn from its
        // `draw` signal exactly like the GDScript.
        let mut plot = Control::new_alloc();
        plot.set_clip_contents(true);
        plot.set_mouse_filter(MouseFilter::STOP);
        plot.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let graph = self.to_gd();
        let plot_for_draw = plot.clone();
        let draw_callable =
            Callable::from_fn("wave_graph_plot_draw", move |_args: &[&Variant]| {
                if !graph.is_instance_valid() || !plot_for_draw.is_instance_valid() {
                    return;
                }
                let mut graph = graph.clone();
                let mut plot = plot_for_draw.clone();
                graph.bind_mut().draw_plot(&mut plot);
            });
        plot.connect("draw", &draw_callable);
        let graph = self.to_gd();
        let input_callable =
            Callable::from_fn("wave_graph_plot_input", move |args: &[&Variant]| {
                let Some(event) = args
                    .first()
                    .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
                else {
                    return;
                };
                if !graph.is_instance_valid() {
                    return;
                }
                let mut graph = graph.clone();
                graph.bind_mut().plot_input(event);
            });
        plot.connect("gui_input", &input_callable);
        let redraw = plot.callable("queue_redraw");
        plot.connect("mouse_exited", &redraw);
        plot_margin.add_child(&plot);
        self.plot = Some(plot);
        // `table(t -> colors = t).growX()`.
        // code-instantiated: the legend row rebuilt per participating unit.
        let mut colors = HBoxContainer::new_alloc();
        colors.add_theme_constant_override("separation", 0);
        rows.add_child(&colors);
        self.colors = Some(colors);
        // `t.left()` + one `fullTogglet` 130x35 button per `Mode`.
        // code-instantiated: the fixed counts/totals/health mode row.
        let mut modes = HBoxContainer::new_alloc();
        modes.add_theme_constant_override("separation", 0);
        rows.add_child(&modes);
        for (index, mode_name) in MODE_NAMES.iter().enumerate() {
            let mut button = factory::button(&bundle(&format!("wavemode.{mode_name}")));
            button.set_theme_type_variation("fullTogglet");
            button.set_toggle_mode(true);
            button.set_pressed_no_signal(index as i64 == self.mode);
            button.set_custom_minimum_size(Vector2::new(130.0, 35.0));
            let mut args = VarArray::new();
            args.push(&(index as i64).to_variant());
            button.connect("pressed", &self.base().callable("set_mode").bindv(&args));
            modes.add_child(&button);
            self.mode_buttons.push(button);
        }
    }
}

#[godot_api]
impl MindWaveGraph {
    /// Forces `rebuild()` on the next draw (the dialog's groups changed).
    #[func]
    pub fn refresh(&mut self) {
        self.last_from = -1;
        self.last_to = -1;
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// `modeRow` click: `_mode = mode` and re-check the row.
    #[func]
    pub fn set_mode(&mut self, mode: i64) {
        self.mode = mode;
        for (index, button) in self.mode_buttons.iter_mut().enumerate() {
            button.set_pressed_no_signal(index as i64 == self.mode);
        }
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// One legend toggle (`_toggle_unit(name)`).
    #[func]
    fn on_toggle_unit(&mut self, name: GString) {
        let name = name.to_string();
        if let Some(index) = self.hidden.iter().position(|entry| entry == &name) {
            self.hidden.remove(index);
        } else {
            self.hidden.push(name);
        }
        let units = dict_array(&self.data, "units");
        self.sync_legend(&units);
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// `hidden.size == used.size ? clear : addAll(used)`.
    #[func]
    fn on_toggle_all(&mut self) {
        let units = dict_array(&self.data, "units");
        if self.all_hidden(&units) {
            self.hidden.clear();
        } else {
            for unit_value in units.iter_shared() {
                let unit = unit_value.try_to::<VarDictionary>().unwrap_or_default();
                self.hidden.push(dict_string(&unit, "name"));
            }
        }
        self.sync_legend(&units);
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }
}

impl MindWaveGraph {
    /// The live `MindEditor` facade from the exported path.
    fn resolve_editor(&self) -> Option<Gd<Node>> {
        let path = self.editor_path.clone();
        self.base().get_node_or_null(&path)
    }

    /// The stored facade, re-resolved when the exported path changed.
    fn editor(&self) -> Option<Gd<Node>> {
        if let Some(editor) = &self.editor
            && editor.is_instance_valid()
        {
            return Some(editor.clone());
        }
        self.resolve_editor()
    }

    /// `MindEditor.wave_graph(from, to)` (`{}` when absent/invalid).
    fn wave_graph_data(&self, from: i64, to: i64) -> VarDictionary {
        let Some(mut editor) = self.editor() else {
            return VarDictionary::new();
        };
        if !editor.has_method("wave_graph") {
            return VarDictionary::new();
        }
        editor
            .call("wave_graph", &[from.to_variant(), to.to_variant()])
            .try_to::<VarDictionary>()
            .unwrap_or_default()
    }

    /// A `Tex.*` 9-patch with uniform texture margins (`_nine_patch(region, m)`).
    fn nine_patch(&self, region: &str, margin: f32) -> Option<Gd<StyleBoxTexture>> {
        let texture = kit::icon_texture(region)?;
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_texture_margin(side, margin);
        }
        Some(box_)
    }

    /// `scrolled`: `zoom -= scroll * 2 / 10 * zoom`; `pan -= deltaX / zoom`.
    fn plot_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
            let index = button.get_button_index();
            if button.is_pressed()
                && (index == MouseButton::WHEEL_DOWN || index == MouseButton::WHEEL_UP)
            {
                let scroll = if index == MouseButton::WHEEL_DOWN {
                    1.0
                } else {
                    -1.0
                };
                self.zoom = (self.zoom - scroll * 2.0 / 10.0 * self.zoom).clamp(0.5, 40.0);
                if let Some(mut plot) = self.plot.clone() {
                    plot.accept_event();
                }
            } else if index == MouseButton::LEFT {
                self.dragging = button.is_pressed();
            }
        } else if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
            && self.dragging
        {
            self.pan -= motion.get_relative().x / self.zoom;
        }
        if let Some(mut plot) = self.plot.clone() {
            plot.queue_redraw();
        }
    }

    /// `WaveGraph.rect(...)`, in Godot's y-down plot space (`y` = bottom - Java y).
    fn draw_plot(&mut self, plot: &mut Gd<Control>) {
        let width = plot.get_size().x;
        let height = plot.get_size().y;
        let Some(font) = self.base().get_theme_default_font() else {
            return;
        };
        let font_size = self.base().get_theme_default_font_size();
        // `lay.setText(font, "1"); fh = lay.height` (cap height).
        let fh = font.get_ascent_ex().font_size(font_size).done() * 0.7;
        let spacing = self.zoom * DEFAULT_SPACE;
        self.pan = self.pan.max((width / 2.0) / self.zoom - DEFAULT_SPACE);
        let offset_y = 22.0 + fh + 5.0;
        let graph_x = -self.pan * self.zoom + width / 2.0;
        let graph_h = height - offset_y;
        let base_y = graph_h;
        let left = (0.0 - graph_x) / spacing;
        let right = (width - graph_x) / spacing;
        self.from = left as i64 - 1;
        self.to = right as i64 + 1;
        if self.last_from != self.from || self.last_to != self.to {
            self.rebuild();
        }
        self.last_from = self.from;
        self.last_to = self.to;

        let series = dict_array(&self.data, "series");
        let units = dict_array(&self.data, "units");
        let max_y_values = self
            .data
            .get("max_y")
            .and_then(|value| value.try_to::<PackedInt64Array>().ok())
            .unwrap_or_default();
        let mode_index = usize::try_from(self.mode).unwrap_or(usize::MAX);
        let max_y = match max_y_values.as_slice().get(mode_index) {
            Some(value) => (*value).max(1) as f32,
            None => 1.0,
        };
        let unit_count = self
            .data
            .get("unit_count")
            .map(|value| variant_f64(&value))
            .unwrap_or(1.0)
            .max(1.0);
        let visible = self.visible_units(&units);

        let mouse = plot.get_local_mouse_position();
        let inside = Rect2::new(Vector2::ZERO, plot.get_size()).contains_point(mouse);
        let mut selcol = if inside {
            ((mouse.x - graph_x - self.from as f32 * spacing) / spacing).round() as i64
        } else {
            -1
        };
        if selcol + self.from <= -1 {
            selcol = -1;
        }

        let columns = series.len() as i64;
        if self.mode == 0 {
            for unit_index in &visible {
                let mut points = PackedVector2Array::new();
                for i in 0..columns {
                    let row = series
                        .at(i as usize)
                        .try_to::<PackedInt64Array>()
                        .unwrap_or_default();
                    let value = row
                        .as_slice()
                        .get(*unit_index as usize)
                        .copied()
                        .unwrap_or(0) as f32;
                    points.push(Vector2::new(
                        graph_x + (i + self.from) as f32 * spacing,
                        base_y - value * graph_h / max_y,
                    ));
                }
                let unit = units
                    .at(*unit_index as usize)
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let color = unit_color(&unit, unit_count);
                end_line(plot, &points, color);
            }
        } else {
            let mut points = PackedVector2Array::new();
            for i in 0..columns {
                let row = series
                    .at(i as usize)
                    .try_to::<PackedInt64Array>()
                    .unwrap_or_default();
                let mut sum = 0.0f32;
                for unit_index in &visible {
                    let mut weight = 1.0f32;
                    if self.mode == 2 {
                        let unit = units
                            .at(*unit_index as usize)
                            .try_to::<VarDictionary>()
                            .unwrap_or_default();
                        weight = dict_f64(&unit, "health") as f32;
                    }
                    let value = row
                        .as_slice()
                        .get(*unit_index as usize)
                        .copied()
                        .unwrap_or(0) as f32;
                    sum += weight * value;
                }
                points.push(Vector2::new(
                    graph_x + (i + self.from) as f32 * spacing,
                    base_y - sum * graph_h / max_y,
                ));
            }
            let color = if self.mode == 1 {
                styles::ACCENT
            } else {
                HEALTH_COLOR
            };
            end_line(plot, &points, color);
        }

        if selcol >= 0 && selcol < columns {
            let column_x = (selcol + self.from) as f32 * spacing + graph_x - spacing / 2.0;
            plot.draw_rect_ex(
                Rect2::new(Vector2::new(column_x, 0.0), Vector2::new(spacing, graph_h)),
                Color::from_rgba(1.0, 0.0, 0.0, 0.2),
            )
            .filled(true)
            .done();
            // `countStr`: `type.emoji() + " " + amount` per line, scale 1.5.
            let big = (font_size as f32 * 1.5) as i32;
            let line_h = font.get_height_ex().font_size(big).done();
            let mut y = 5.0;
            let row = series
                .at(selcol as usize)
                .try_to::<PackedInt64Array>()
                .unwrap_or_default();
            for unit_index in &visible {
                let amount = row
                    .as_slice()
                    .get(*unit_index as usize)
                    .copied()
                    .unwrap_or(0);
                if amount <= 0 {
                    continue;
                }
                let unit = units
                    .at(*unit_index as usize)
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let icon = kit::icon_texture(&format!("unit-{}-ui", dict_string(&unit, "name")));
                let mut x = column_x + 5.0;
                if let Some(icon) = &icon {
                    let icon_size = font.get_ascent_ex().font_size(big).done();
                    plot.draw_texture_rect(
                        icon,
                        Rect2::new(
                            Vector2::new(x, y + (line_h - icon_size) / 2.0),
                            Vector2::new(icon_size, icon_size),
                        ),
                        false,
                    );
                    x += font
                        .get_string_size_ex(" ")
                        .alignment(HorizontalAlignment::LEFT)
                        .width(-1.0)
                        .font_size(big)
                        .done()
                        .x;
                }
                let text = GString::from(amount.to_string().as_str());
                let ascent = font.get_ascent_ex().font_size(big).done();
                plot.draw_string_outline_ex(&font, Vector2::new(x, y + ascent), &text)
                    .alignment(HorizontalAlignment::LEFT)
                    .width(-1.0)
                    .font_size(big)
                    .size(3)
                    .modulate(Color::BLACK)
                    .done();
                plot.draw_string_ex(&font, Vector2::new(x, y + ascent), &text)
                    .alignment(HorizontalAlignment::LEFT)
                    .width(-1.0)
                    .font_size(big)
                    .modulate(Color::WHITE)
                    .done();
                y += line_h;
            }
        }

        // Horizontal marks: `totalMarks = clamp(maxY, 1, 10)`, faint light-gray
        // lines with the value at the left edge.
        let total_marks = max_y.clamp(1.0, 10.0);
        let mark_space = ((max_y / total_marks).ceil() as i64).max(1);
        let mut mark = 0i64;
        while (mark as f32) < max_y {
            let cy = base_y - mark as f32 * graph_h / max_y;
            let mut mark_color = styles::ARC_LIGHT_GRAY;
            mark_color.a = 0.1;
            plot.draw_line_ex(Vector2::new(0.0, cy), Vector2::new(width, cy), mark_color)
                .width(STROKE)
                .done();
            let text = GString::from(mark.to_string().as_str());
            plot.draw_string_ex(&font, Vector2::new(0.0, cy + fh / 2.0), &text)
                .alignment(HorizontalAlignment::LEFT)
                .width(-1.0)
                .font_size(font_size)
                .modulate(Color::WHITE)
                .done();
            mark += mark_space;
        }

        // Wave ticks along the bottom (`cy = y + fh`, `len = 4`), the hovered
        // column's wave number under its tick.
        let tick_y = height - fh;
        for i in 0..columns {
            let cx = graph_x + spacing * (i + self.from) as f32;
            if cx >= 0.0 && cx <= width {
                plot.draw_line_ex(
                    Vector2::new(cx, tick_y),
                    Vector2::new(cx, tick_y - 4.0),
                    styles::ARC_LIGHT_GRAY,
                )
                .width(STROKE)
                .done();
            }
            if i == selcol {
                let text = GString::from((i + self.from + 1).to_string().as_str());
                let text_w = font
                    .get_string_size_ex(&text)
                    .alignment(HorizontalAlignment::LEFT)
                    .width(-1.0)
                    .font_size(font_size)
                    .done()
                    .x;
                plot.draw_string_ex(
                    &font,
                    Vector2::new(cx - text_w / 2.0, tick_y + 2.0 + fh),
                    &text,
                )
                .alignment(HorizontalAlignment::LEFT)
                .width(-1.0)
                .font_size(font_size)
                .modulate(styles::ARC_LIGHT_GRAY)
                .done();
            }
        }
    }

    /// `used` minus `hidden`, as unit indices into the Rust `units` array.
    fn visible_units(&self, units: &VarArray) -> Vec<i64> {
        let mut out = Vec::new();
        for index in 0..units.len() {
            let unit = units
                .at(index)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            let name = dict_string(&unit, "name");
            if !self.hidden.iter().any(|hidden| hidden == &name) {
                out.push(index as i64);
            }
        }
        out
    }

    /// `rebuild()`: refetch the window and rebuild the legend.
    fn rebuild(&mut self) {
        self.data = self.wave_graph_data(self.from, self.to);
        self.rebuild_legend();
    }

    /// `colors`: the `@waves.units.hide`/`show` button (flatt, 130x32) and a
    /// horizontal pane of `fullTogglet` toggles (colour swatch + unit icon).
    fn rebuild_legend(&mut self) {
        let Some(mut colors) = self.colors.clone() else {
            return;
        };
        for child in colors.get_children().iter_shared() {
            let mut child = child.clone();
            colors.remove_child(&child);
            child.queue_free();
        }
        self.legend.clear();
        let units = dict_array(&self.data, "units");
        let unit_count = self
            .data
            .get("unit_count")
            .map(|value| variant_f64(&value))
            .unwrap_or(1.0)
            .max(1.0);
        // code-instantiated: the legend's hide/show-all button.
        let mut hide_button = factory::button("");
        hide_button.set_theme_type_variation("flatt");
        hide_button.set_custom_minimum_size(Vector2::new(130.0, 32.0));
        hide_button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        hide_button.connect("pressed", &self.base().callable("on_toggle_all"));
        colors.add_child(&hide_button);
        self.hide_button = Some(hide_button);
        // code-instantiated: the horizontally scrolling legend strip.
        let mut scroll = MindScroll::new_alloc();
        scroll.bind_mut().set_scrolling_disabled(false, true);
        let mut scroll_control: Gd<Control> = scroll.clone().upcast();
        scroll_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        colors.add_child(&scroll_control);
        // code-instantiated: one toggle per participating unit type (row).
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 0);
        scroll_control.add_child(&row);
        for unit_value in units.iter_shared() {
            let unit = unit_value.try_to::<VarDictionary>().unwrap_or_default();
            let name = dict_string(&unit, "name");
            // code-instantiated: one unit legend toggle (`fullTogglet`).
            let mut button = Button::new_alloc();
            button.set_theme_type_variation("fullTogglet");
            button.set_toggle_mode(true);
            // code-instantiated: the toggle's swatch + icon row.
            let mut content = HBoxContainer::new_alloc();
            content.set_mouse_filter(MouseFilter::IGNORE);
            content.add_theme_constant_override("separation", 0);
            // `b.image().size(32f)` has no drawable, so Arc draws nothing there:
            // the colour only tints an empty image. Keep the 32 px slot, invisible.
            // code-instantiated: the invisible colour swatch of one legend toggle.
            let mut swatch = ColorRect::new_alloc();
            swatch.set_self_modulate(Color::from_rgba(1.0, 1.0, 1.0, 0.0));
            swatch.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            swatch.set_mouse_filter(MouseFilter::IGNORE);
            swatch.set_meta("color", &unit_color(&unit, unit_count).to_variant());
            content.add_child(&swatch);
            // code-instantiated: the unit's `uiIcon` inside its legend toggle.
            let mut icon = TextureRect::new_alloc();
            icon.set_texture(kit::icon_texture(&format!("unit-{name}-ui")).as_ref());
            icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            icon.set_mouse_filter(MouseFilter::IGNORE);
            content.add_child(&icon);
            // code-instantiated: `.padRight(20)` spacer after the icon.
            let mut pad = Control::new_alloc();
            pad.set_custom_minimum_size(Vector2::new(20.0, 0.0));
            pad.set_mouse_filter(MouseFilter::IGNORE);
            content.add_child(&pad);
            button.add_child(&content);
            button.set_custom_minimum_size(Vector2::new(84.0, 32.0));
            let mut args = VarArray::new();
            args.push(&GString::from(name.as_str()).to_variant());
            button.connect(
                "pressed",
                &self.base().callable("on_toggle_unit").bindv(&args),
            );
            row.add_child(&button);
            self.legend.push(LegendEntry {
                name,
                button,
                swatch,
                icon,
            });
        }
        self.sync_legend(&units);
    }

    /// `hidden.size == used.size` (false for an empty unit list).
    fn all_hidden(&self, units: &VarArray) -> bool {
        for index in 0..units.len() {
            let unit = units
                .at(index)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            let name = dict_string(&unit, "name");
            if !self.hidden.iter().any(|hidden| hidden == &name) {
                return false;
            }
        }
        !units.is_empty()
    }

    /// The legend `update` lambdas: checked = hidden; the swatch halves its
    /// colour and the icon greys out; the hide button reads show/hide.
    fn sync_legend(&mut self, units: &VarArray) {
        if let Some(mut hide) = self.hide_button.clone() {
            hide.set_text(&bundle(if self.all_hidden(units) {
                "waves.units.show"
            } else {
                "waves.units.hide"
            }));
        }
        for entry in &mut self.legend {
            let hidden = self.hidden.iter().any(|name| name == &entry.name);
            entry.button.set_pressed_no_signal(hidden);
            if entry.swatch.has_meta("color") {
                let color = entry
                    .swatch
                    .get_meta("color")
                    .try_to::<Color>()
                    .unwrap_or(Color::WHITE);
                entry.swatch.set_color(if hidden {
                    Color::from_rgba(color.r * 0.5, color.g * 0.5, color.b * 0.5, color.a * 0.5)
                } else {
                    color
                });
            }
            entry.icon.set_modulate(if hidden {
                styles::ARC_GRAY
            } else {
                Color::WHITE
            });
        }
    }
}

/// `color(type)`: `fromHsv(type.id / units.size * 360, 0.7, 1)`.
fn unit_color(unit: &VarDictionary, unit_count: f64) -> Color {
    Color::from_hsv(dict_f64(unit, "id") / unit_count, 0.7, 1.0)
}

/// `endLine()`: 3 px segments with round joints, a bigger dot on the last point.
fn end_line(plot: &mut Gd<Control>, points: &PackedVector2Array, color: Color) {
    let slice = points.as_slice();
    if slice.is_empty() {
        return;
    }
    for pair in slice.windows(2) {
        plot.draw_line_ex(pair[0], pair[1], color)
            .width(STROKE)
            .done();
        plot.draw_circle_ex(pair[0], STROKE / 2.0, color)
            .filled(true)
            .done();
    }
    if let Some(last) = slice.last() {
        plot.draw_circle_ex(*last, STROKE, color)
            .filled(true)
            .done();
    }
}

// ---------------------------------------------------------------------------
// MapObjectivesCanvas (`map_objectives_canvas.gd`, `MapObjectivesCanvas.java`)
// ---------------------------------------------------------------------------

/// `MapObjectivesCanvas.{objWidth, objHeight}` in placement units.
const OBJ_WIDTH: i32 = 5;
const OBJ_HEIGHT: i32 = 2;
/// `MapObjectivesCanvas.bounds`.
const BOUNDS: i32 = 100;
/// `MapObjectivesCanvas.unitSize`.
const UNIT: f32 = 48.0;
/// `Pal.accent` marker border (`ffd37f`).
const MARKER_SELECTED: Color = Color::from_rgba8(0xff, 0xd3, 0x7f, 0xff);
/// The invalid-placement ghost (`ff6655`).
const MARKER_INVALID: Color = Color::from_rgba8(0xff, 0x66, 0x55, 0xff);
/// The default objective tile border (`Color(0.85, 0.42, 0.33)`).
const MARKER_BORDER: Color = Color::from_rgba(0.85, 0.42, 0.33, 1.0);

/// `MindMapObjectivesCanvas` — the objectives marker canvas.
///
/// Presentation only: pan, grid, node tiles with their type names, connector
/// lines, drag-to-move, the `@add` query cursor and paste targeting. The
/// objective graph data comes from `MindEditor.objectives_json`.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindMapObjectivesCanvas {
    base: Base<Control>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    pan: Vector2,
    dragging: bool,
    /// Index of the node under an active drag (`ObjectiveTilemap.moving`).
    moving: i64,
    move_offset: Vector2,
    /// While the `@add` query cursor is active (`isQuerying`).
    querying: bool,
    /// Highlighted node (`updateCard`-style selection state).
    selected: i64,
    /// Last mouse position delivered to this control (local space).
    cursor: Vector2,
}

#[godot_api]
impl IControl for MindMapObjectivesCanvas {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            pan: Vector2::ZERO,
            dragging: false,
            moving: -1,
            move_offset: Vector2::ZERO,
            querying: false,
            selected: -1,
            cursor: Vector2::ZERO,
        }
    }

    fn ready(&mut self) {
        self.editor = self.resolve_editor();
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
            self.cursor = button.get_position();
            self.mouse_button(button);
        } else if let Ok(motion) = event.try_cast::<InputEventMouseMotion>() {
            self.cursor = motion.get_position();
            self.mouse_motion(motion);
        }
    }

    fn draw(&mut self) {
        let objectives = self.objectives();
        self.draw_grid();
        let mut positions: Vec<Rect2> = Vec::new();
        for objective in objectives.iter_shared() {
            let value = objective.try_to::<VarDictionary>().unwrap_or_default();
            let editor_pos = dict_i64(&value, "editorPos");
            if !has_pos(editor_pos) {
                positions.push(Rect2::new(Vector2::ZERO, Vector2::ZERO));
                continue;
            }
            positions.push(self.rect_at(editor_pos));
        }
        for (from_index, to_index) in edges(&objectives) {
            let (Some(from_rect), Some(to_rect)) =
                (positions.get(from_index), positions.get(to_index))
            else {
                continue;
            };
            if from_rect.size == Vector2::ZERO || to_rect.size == Vector2::ZERO {
                continue;
            }
            self.base_mut()
                .draw_line_ex(
                    from_rect.center(),
                    to_rect.center(),
                    Color::from_rgba(0.98, 0.78, 0.34, 1.0),
                )
                .width(2.0)
                .done();
        }
        for (index, rect) in positions.iter().enumerate() {
            if rect.size == Vector2::ZERO {
                continue;
            }
            let value = objectives
                .at(index)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            self.base_mut()
                .draw_rect_ex(*rect, Color::from_rgba(0.2, 0.2, 0.25, 0.9))
                .filled(true)
                .done();
            let selected = index as i64 == self.selected;
            self.base_mut()
                .draw_rect_ex(
                    *rect,
                    if selected {
                        MARKER_SELECTED
                    } else {
                        MARKER_BORDER
                    },
                )
                .filled(false)
                .width(if selected { 3.0 } else { 2.0 })
                .done();
            let type_label = self.type_name(&dict_string(&value, "class"));
            if type_label.is_empty() {
                continue;
            }
            let Some(font) = ThemeDb::singleton().get_fallback_font() else {
                continue;
            };
            let origin = rect.position + Vector2::new(8.0, 24.0);
            self.base()
                .draw_string_ex(&font, origin, &GString::from(type_label.as_str()))
                .alignment(HorizontalAlignment::LEFT)
                .width(rect.size.x - 16.0)
                .font_size(16)
                .modulate(Color::from_rgba(0.9, 0.9, 0.9, 1.0))
                .done();
        }
        if self.querying {
            let cell = self.query_cell(self.cursor);
            let valid = self.valid_place(cell.x, cell.y, -1);
            let ghost = Rect2::new(
                self.origin() + Vector2::new(cell.x as f32, cell.y as f32) * UNIT,
                Vector2::new(OBJ_WIDTH as f32, OBJ_HEIGHT as f32) * UNIT,
            );
            let base_color = if valid {
                MARKER_SELECTED
            } else {
                MARKER_INVALID
            };
            let mut ghost_color = base_color;
            ghost_color.a = 0.4;
            self.base_mut()
                .draw_rect_ex(ghost, ghost_color)
                .filled(true)
                .done();
            self.base_mut()
                .draw_rect_ex(ghost, base_color)
                .filled(false)
                .width(3.0)
                .done();
        }
        if self.moving >= 0 {
            let cell = self.move_cell();
            let valid = self.valid_place(cell.x, cell.y, self.moving);
            let ghost = Rect2::new(
                self.origin() + Vector2::new(cell.x as f32, cell.y as f32) * UNIT,
                Vector2::new(OBJ_WIDTH as f32, OBJ_HEIGHT as f32) * UNIT,
            );
            let mut ghost_color = if valid {
                MARKER_SELECTED
            } else {
                MARKER_INVALID
            };
            ghost_color.a = 0.5;
            self.base_mut()
                .draw_rect_ex(ghost, ghost_color)
                .filled(true)
                .done();
        }
    }
}

#[godot_api]
impl MindMapObjectivesCanvas {
    /// Emitted on a left click over one objective node (`MapObjectivesCanvas`
    /// tile selection -> `MapObjectivesDialog`'s field editors).
    #[signal]
    fn node_selected(index: i64);

    /// A node was dragged to a new grid position (`Mover.touchUp` + `moveTile`).
    #[signal]
    fn node_moved(index: i64, x: i64, y: i64);

    /// The query cursor was clicked (`tap` -> `placeQuery`).
    #[signal]
    fn query_placed(x: i64, y: i64);

    /// `updateCard`-style selection highlight.
    #[func]
    pub fn set_selected(&mut self, index: i64) {
        self.selected = index;
        self.base_mut().queue_redraw();
    }

    /// `canvas.query(obj)` / `stopQuery`.
    #[func]
    pub fn set_querying(&mut self, active: bool) {
        self.querying = active;
        self.base_mut().queue_redraw();
    }

    /// Whether the `@add` query cursor is active.
    #[func]
    pub fn is_querying(&self) -> bool {
        self.querying
    }

    /// `screenToLocalCoordinates(tilemap)` query cursor cell (`tap`/paste math).
    #[func]
    pub fn cursor_cell(&self) -> Vector2i {
        self.query_cell(self.cursor)
    }

    /// The last mouse position in canvas-local space (`Core.input.mouse()`).
    #[func]
    pub fn cursor_position(&self) -> Vector2 {
        self.cursor
    }

    /// The node under the mouse (`doInput`'s copy hit-test).
    #[func]
    pub fn node_index_at(&self, point: Vector2) -> i64 {
        self.node_at(point)
    }

    /// `ObjectiveTilemap.validPlace` for a cell (public for paste/query).
    #[func]
    pub fn cell_valid(&self, x: i64, y: i64) -> bool {
        self.valid_place(x as i32, y as i32, -1)
    }
}

impl MindMapObjectivesCanvas {
    /// The live `MindEditor` facade from the exported path.
    fn resolve_editor(&self) -> Option<Gd<Node>> {
        let path = self.editor_path.clone();
        self.base().get_node_or_null(&path)
    }

    /// The stored facade, re-resolved when the exported path changed.
    fn editor(&self) -> Option<Gd<Node>> {
        if let Some(editor) = &self.editor
            && editor.is_instance_valid()
        {
            return Some(editor.clone());
        }
        self.resolve_editor()
    }

    /// One mouse-button branch of `_gui_input`.
    fn mouse_button(&mut self, button: Gd<InputEventMouseButton>) {
        let index = button.get_button_index();
        let pressed = button.is_pressed();
        if pressed && index == MouseButton::LEFT {
            if self.querying {
                // `tap`: place the pending objective at the cursor.
                let cell = self.query_cell(button.get_position());
                if self.valid_place(cell.x, cell.y, -1) {
                    self.emit_deferred(
                        "query_placed",
                        vec![cell.x.to_variant(), cell.y.to_variant()],
                    );
                }
                return;
            }
            // Left click selects a node; a press over one starts a drag, a
            // miss starts a pan (`Mover.touchDown` / pan gesture).
            let hit = self.node_at(button.get_position());
            if hit >= 0 {
                self.moving = hit;
                let node = Vector2::new(self.node_x(hit) as f32, self.node_y(hit) as f32) * UNIT;
                self.move_offset = self.origin() + node - button.get_position();
                self.emit_deferred("node_selected", vec![hit.to_variant()]);
                return;
            }
            self.dragging = true;
        } else if index == MouseButton::MIDDLE {
            self.dragging = pressed;
        } else if index == MouseButton::LEFT {
            if self.moving >= 0 {
                self.finish_move();
                return;
            }
            self.dragging = false;
        }
        self.base_mut().queue_redraw();
    }

    /// One mouse-motion branch of `_gui_input`.
    fn mouse_motion(&mut self, motion: Gd<InputEventMouseMotion>) {
        if self.moving >= 0 {
            self.base_mut().queue_redraw();
        } else if self.dragging {
            // `ElementGestureListener.pan`: clamp to the tilemap's valid range.
            self.pan += motion.get_relative();
            let size = self.base().get_size();
            self.pan.x = clampf(
                self.pan.x,
                -(BOUNDS as f32) * UNIT + size.x,
                BOUNDS as f32 * UNIT,
            );
            self.pan.y = clampf(
                self.pan.y,
                -(BOUNDS as f32) * UNIT + size.y,
                BOUNDS as f32 * UNIT,
            );
            self.base_mut().queue_redraw();
        }
    }

    /// Emits a canvas signal from the idle phase.
    ///
    /// `MapObjectivesDialog`'s handlers call back into this canvas
    /// (`set_querying`) and re-entering a bound GDExtension object panics
    /// (PAR-0066), so the emits leave `_gui_input`'s borrow first.
    fn emit_deferred(&self, name: &'static str, args: Vec<Variant>) {
        let target = self.to_gd();
        let callable =
            Callable::from_fn("objectives_canvas_signal", move |_args: &[&Variant]| {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                node.emit_signal(name, &args);
            });
        callable.call_deferred(&[]);
    }

    /// `Mover.touchUp` + `moveTile`: snap to the grid, reject overlaps/bounds.
    fn finish_move(&mut self) {
        let index = self.moving;
        self.moving = -1;
        let objectives = self.objectives();
        if index < 0 || index >= objectives.len() as i64 {
            return;
        }
        let cell = self.move_cell();
        let old_x = self.node_x(index);
        let old_y = self.node_y(index);
        if (cell.x != old_x || cell.y != old_y)
            && self.valid_place_in(&objectives, cell.x, cell.y, index)
        {
            self.emit_deferred(
                "node_moved",
                vec![index.to_variant(), cell.x.to_variant(), cell.y.to_variant()],
            );
        }
        self.base_mut().queue_redraw();
    }

    /// The drag's snapped tile (`Mover.touchUp`'s `round(moving.x / unitSize)`).
    fn move_cell(&self) -> Vector2i {
        let local = self.cursor - self.origin() + self.move_offset;
        Vector2i::new(
            (local.x / UNIT).round() as i32,
            (local.y / UNIT).round() as i32,
        )
    }

    /// `ObjectiveTilemap.validPlace`: in bounds and not overlapping another node.
    fn valid_place(&self, x: i32, y: i32, ignore: i64) -> bool {
        let objectives = self.objectives();
        self.valid_place_in(&objectives, x, y, ignore)
    }

    /// [`Self::valid_place`] against an already parsed objective list.
    fn valid_place_in(&self, objectives: &VarArray, x: i32, y: i32, ignore: i64) -> bool {
        let others: Vec<(i32, i32)> = (0..objectives.len())
            .filter(|index| *index as i64 != ignore)
            .filter_map(|index| {
                let value = objectives
                    .at(index)
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let pos = dict_i64(&value, "editorPos");
                has_pos(pos).then(|| (node_x_at(pos), node_y_at(pos)))
            })
            .collect();
        valid_place_rect(x, y, &others)
    }

    /// Index of the node whose rect contains `point`, or `-1`. Nodes paint in
    /// order, so the top-most (last) hit wins.
    fn node_at(&self, point: Vector2) -> i64 {
        let objectives = self.objectives();
        for index in (0..objectives.len() as i64).rev() {
            let value = objectives
                .at(index as usize)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            let pos = dict_i64(&value, "editorPos");
            if !has_pos(pos) {
                continue;
            }
            if self.rect_at(pos).contains_point(point) {
                return index;
            }
        }
        -1
    }

    /// `size * 0.5 + _pan`.
    fn origin(&self) -> Vector2 {
        self.base().get_size() * 0.5 + self.pan
    }

    /// The tile the mouse is over (`screenToLocalCoordinates(tilemap)` math).
    fn query_cell(&self, point: Vector2) -> Vector2i {
        let local = point - self.origin();
        Vector2i::new(
            (local.x / UNIT - OBJ_WIDTH as f32 * 0.5).round() as i32,
            (local.y / UNIT - 1.0).floor() as i32,
        )
    }

    /// `_node_x(index)`: the `editorPos` x of one objective (`-999` default).
    fn node_x(&self, index: i64) -> i32 {
        let objectives = self.objectives();
        let value = if index >= 0 && index < objectives.len() as i64 {
            objectives
                .at(index as usize)
                .try_to::<VarDictionary>()
                .unwrap_or_default()
        } else {
            VarDictionary::new()
        };
        node_x_at(dict_i64(&value, "editorPos"))
    }

    /// `_node_y(index)`: the `editorPos` y of one objective (`-999` default).
    fn node_y(&self, index: i64) -> i32 {
        let objectives = self.objectives();
        let value = if index >= 0 && index < objectives.len() as i64 {
            objectives
                .at(index as usize)
                .try_to::<VarDictionary>()
                .unwrap_or_default()
        } else {
            VarDictionary::new()
        };
        node_y_at(dict_i64(&value, "editorPos"))
    }

    /// One objective node's canvas rect from its packed `editorPos`.
    fn rect_at(&self, editor_pos: i64) -> Rect2 {
        Rect2::new(
            self.origin()
                + Vector2::new(node_x_at(editor_pos) as f32, node_y_at(editor_pos) as f32) * UNIT,
            Vector2::new(OBJ_WIDTH as f32, OBJ_HEIGHT as f32) * UNIT,
        )
    }

    /// `ObjectiveTilemap.draw`'s grid over the ±`bounds` play area.
    fn draw_grid(&mut self) {
        let origin = self.origin();
        // `Lines.stroke(3f); Draw.color(Pal.darkestGray)`.
        let color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
        let extent = BOUNDS as f32 * UNIT;
        for i in 0..(2 * BOUNDS + 1) {
            let unit = (-BOUNDS + i) as f32;
            let x = origin.x + unit * UNIT;
            self.base_mut()
                .draw_line_ex(
                    Vector2::new(x, origin.y - extent),
                    Vector2::new(x, origin.y + extent),
                    color,
                )
                .width(3.0)
                .done();
            let y = origin.y + unit * UNIT;
            self.base_mut()
                .draw_line_ex(
                    Vector2::new(origin.x - extent, y),
                    Vector2::new(origin.x + extent, y),
                    color,
                )
                .width(3.0)
                .done();
        }
    }

    /// `obj.typeName()` (`ObjectiveMarker`-style bundle lookup with a fallback).
    fn type_name(&self, tag: &str) -> String {
        if tag.is_empty() {
            return String::new();
        }
        let key = format!("objective.{}.name", tag.to_lowercase());
        if let Some(assets) = kit::assets() {
            let value = assets
                .bind()
                .bundle_get(GString::from(key.as_str()))
                .to_string();
            if value != key {
                return value;
            }
        }
        tag.to_string()
    }

    /// `MindEditor.objectives_json()` parsed as an array (`[]` when absent).
    fn objectives(&self) -> VarArray {
        let Some(mut editor) = self.editor() else {
            return VarArray::new();
        };
        if !editor.has_method("objectives_json") {
            return VarArray::new();
        }
        let text = editor
            .call("objectives_json", &[])
            .try_to::<GString>()
            .unwrap_or_default();
        Json::parse_string(&text)
            .try_to::<VarArray>()
            .unwrap_or_default()
    }
}

/// `ObjectiveTilemap.validPlace` (`MapObjectivesCanvas.java:305-326`): the
/// target is inside `[-bounds, bounds]` and its 5x2 tile rect does not overlap
/// any other placed objective's rect. Edge contact is allowed (strict
/// comparisons in Java).
pub(crate) fn valid_place_rect(x: i32, y: i32, others: &[(i32, i32)]) -> bool {
    if !(-BOUNDS..=BOUNDS).contains(&x) || !(-BOUNDS..=BOUNDS).contains(&y) {
        return false;
    }
    for &(nx, ny) in others {
        if nx < x + OBJ_WIDTH && nx + OBJ_WIDTH > x && ny < y + OBJ_HEIGHT && ny + OBJ_HEIGHT > y {
            return false;
        }
    }
    true
}

/// `Point2.pack`-style `editorPos` used by the canvas (`arc Point2.pack`:
/// **x in the high half, y in the low half**, `Point2.java:40-42`).
fn pack_pos(x: i64, y: i64) -> i64 {
    i64::from(((x as i16 as i32) << 16) | ((y as i16 as u16) as i32))
}

/// `_has_pos(editorPos)`: the packed `(-999, -999)` sentinel means "unplaced".
fn has_pos(editor_pos: i64) -> bool {
    editor_pos != pack_pos(-999, -999)
}

/// `_node_x_at`: the packed x with a signed 16-bit wrap.
fn node_x_at(editor_pos: i64) -> i32 {
    let x = (editor_pos >> 16) & 0xFFFF;
    if x > 32767 {
        (x - 65536) as i32
    } else {
        x as i32
    }
}

/// `_node_y_at`: the packed y with a signed 16-bit wrap.
fn node_y_at(editor_pos: i64) -> i32 {
    let y = editor_pos & 0xFFFF;
    if y > 32767 {
        (y - 65536) as i32
    } else {
        y as i32
    }
}

/// `_edges(objectives)`: `[parent, index]` pairs for every parent link.
fn edges(objectives: &VarArray) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for index in 0..objectives.len() {
        let value = objectives
            .at(index)
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        for parent in dict_array(&value, "parents").iter_shared() {
            let parent = variant_i64(&parent);
            if parent >= 0 && (parent as usize) < objectives.len() {
                out.push((parent as usize, index));
            }
        }
    }
    out
}

/// GDScript `clampf` semantics (`min`/`max` are never swapped, so a tiny canvas
/// never panics).
fn clampf(value: f32, min: f32, max: f32) -> f32 {
    if min > max {
        return value;
    }
    value.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `NodePosition.pack`/`unpack` (`MapObjectivesCanvas.NodePosition`): the
    /// signed 16-bit halves round-trip and the sentinel marks "unplaced".
    #[test]
    fn objective_editor_pos_packs_signed_halves() {
        for (x, y) in [(0, 0), (5, 2), (-1, -2), (-100, 99), (32767, -32768)] {
            let packed = pack_pos(i64::from(x), i64::from(y));
            assert_eq!(node_x_at(packed), x, "x for ({x},{y})");
            assert_eq!(node_y_at(packed), y, "y for ({x},{y})");
            assert!(has_pos(packed));
        }
        assert!(!has_pos(pack_pos(-999, -999)), "the default sentinel");
    }

    /// `ObjectiveTilemap.validPlace` (`MapObjectivesCanvas.java:305-326`): the
    /// 5x2 rects may touch but not overlap, and the `[-100, 100]` bounds hold.
    #[test]
    fn objective_placement_rejects_overlap_but_allows_edge_contact() {
        // Free space.
        assert!(valid_place_rect(0, 0, &[]));
        // Same cell overlaps.
        assert!(!valid_place_rect(0, 0, &[(0, 0)]));
        // Overlap by one tile in either axis.
        assert!(!valid_place_rect(1, 0, &[(0, 0)]));
        assert!(!valid_place_rect(0, 1, &[(0, 0)]));
        // Edge contact is legal (strict inequalities in Java).
        assert!(valid_place_rect(OBJ_WIDTH, 0, &[(0, 0)]));
        assert!(valid_place_rect(0, OBJ_HEIGHT, &[(0, 0)]));
        assert!(valid_place_rect(-OBJ_WIDTH, 0, &[(0, 0)]));
        // Bounds are inclusive and checked before the overlap scan.
        assert!(valid_place_rect(BOUNDS, BOUNDS, &[]));
        assert!(!valid_place_rect(BOUNDS + 1, 0, &[]));
        assert!(!valid_place_rect(0, -BOUNDS - 1, &[]));
    }
}
