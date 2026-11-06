// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/logic/LogicDialog.java, LCanvas.java,
//         LStatement.java (plan 13 M4, I03 T7-T9).

//! The mlog logic editor (`LogicDialog` / `LCanvas`).
//!
//! The dialog is a full-screen statement column: every statement is an
//! `LCanvas.StatementElem` — a category-tinted `Tex.whitePane` frame with a
//! category-coloured header (`localizedName()`, address, `+`/copy/delete) over
//! the statement's `LStatement.build(Table)` field row. The column is
//! `getTargetWidth()` wide and centred; jump curves are `JumpCurve` trapezoids
//! driven by `DragLayout.setJumpHeights` lane prediction, drawn every frame by
//! the [`MindLogicOverlay`] child stacked above the statement list. The header
//! drag reorders statements (`DragLayout.insertPosition`), the jump node's
//! press-drag-release picks a `JumpStatement.dest`.
//!
//! `LogicDialog.setup`'s `@back`/`@edit`/`@variables`/`@add` row (160x64) sits in
//! the shared button row; `@add` opens the nested `showAddDialog` menu and
//! `@edit` the `@editor.export` menu (`@clear`/`@copy.clipboard`/
//! `@load.clipboard`/`@logic.restart`). The `@variables` button swaps between
//! the executor's variable panel and `@logic.globals`
//! (`GlobalVarsDialog`/`shouldShowVariables`). Content pencils open the
//! `showSelectTable` pickers (units, ores, statuses, weather, markers,
//! setprop/sensor tabs and the sound browser).
//!
//! Load/save is `MindLogic.parse_statements`/`write_statements` around
//! `SimHost.logic_get_state`/`logic_get_code`/`logic_configure_code` (the editor
//! tile uses `MindEditor`'s `logic_*` pair). The standalone
//! `show(code, null, true, consumer)` mode (`MapGenerateDialog`'s `LogicFilter`)
//! echoes the edited program through [`MindDialog`]'s `code_edited` signal
//! instead of writing a tile.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{CursorShape, FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CenterContainer, ColorPickerButton, ColorRect, Container, Control, DisplayServer,
    GridContainer, HBoxContainer, HFlowContainer, IControl, InputEvent, InputEventMouseButton,
    InputEventMouseMotion, Json, Label, LineEdit, MarginContainer, Node, PanelContainer,
    PopupPanel, ScrollContainer, StyleBox, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, Texture2D,
    TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, styles};

// ---------------------------------------------------------------------------
// constants (`LCategory` / `LogicDialog` / `LCanvas` / `LExecutor`)
// ---------------------------------------------------------------------------

/// `LExecutor.maxInstructions`.
const MAX_INSTRUCTIONS: usize = 1000;
/// `DragLayout.space`.
const ELEMENT_SPACE: i32 = 10;
/// `JumpButton` cell size (`JumpStatement.build`: `.size(30)`).
const JUMP_BUTTON_SIZE: f32 = 30.0;
/// `Pal.place` (jump curves/buttons while hovered).
const PLACE: Color = Color::from_rgba8(0x63, 0x35, 0xf8, 0xff);
/// `Pal.darkishGray` (Add dialog category headers).
const DARKISH_GRAY: Color = Color::from_rgba(0.3, 0.3, 0.3, 1.0);
/// Meta key marking the one `size_changed` hook (`LogicDialog.onResize`).
const RESIZE_META: &str = "mind_logic_resize";

/// `LCategory` colours (`Pal`; `LCategory.java:16-22`).
fn category_color(category: &str) -> Color {
    match category {
        "io" => Color::from_rgba8(0xa0, 0x8a, 0x8a, 0xff),
        "block" => Color::from_rgba8(0xd4, 0x81, 0x6b, 0xff),
        "operation" => Color::from_rgba8(0x87, 0x7b, 0xad, 0xff),
        "control" => Color::from_rgba8(0x6b, 0xb2, 0xb2, 0xff),
        "unit" => Color::from_rgba8(0xc7, 0xb5, 0x9d, 0xff),
        "world" => Color::from_rgba8(0x6b, 0x84, 0xd4, 0xff),
        _ => Color::from_rgba8(0x4d, 0x4d, 0x4d, 0xff),
    }
}

/// `LCategory` icons (`Icon.logicSmall`, `effectSmall`, `settingsSmall`,
/// `rotateSmall`, `unitsSmall`, `terrainSmall`; unknown has none).
fn category_icon(category: &str) -> &'static str {
    match category {
        "io" => "logic",
        "block" => "effect",
        "operation" => "settings",
        "control" => "rotate",
        "unit" => "units",
        "world" => "terrain",
        _ => "",
    }
}

/// `LStatement.showSelect` content pickers (Java `showSelectTable` grids,
/// `LStatements.java`): `name -> (field index, picker kind)`.
fn content_picker(statement: &str, field_index: i64) -> &'static str {
    let table: &[(&str, i64, &str)] = &[
        ("ubind", 0, "unit"),
        ("playsound", 1, "sound"),
        ("status", 1, "status"),
        ("weathersense", 1, "weather"),
        ("weatherset", 0, "weather"),
        ("setprop", 0, "prop"),
        ("sensor", 2, "sensor"),
        ("setmarker", 2, "layer"),
        ("ulocate", 3, "ore"),
    ];
    table
        .iter()
        .find(|(name, index, _)| *name == statement && *index == field_index)
        .map(|(_, _, kind)| *kind)
        .unwrap_or("")
}

// ---------------------------------------------------------------------------
// statement model (`MindLogic.parse_statements` rows)
// ---------------------------------------------------------------------------

/// One editor row: `{name, category, fields: [String]}`.
#[derive(Clone, Debug, Default)]
struct StatementRow {
    name: String,
    category: String,
    fields: Vec<String>,
}

impl StatementRow {
    /// The `jump` row's `dest_index` (`-1` when unset/out of range).
    fn jump_dest(&self, count: usize) -> i64 {
        if self.name != "jump" {
            return -1;
        }
        let Some(first) = self.fields.first() else {
            return -1;
        };
        let Ok(dest) = first.parse::<i64>() else {
            return -1;
        };
        if dest >= 0 && (dest as usize) < count {
            dest
        } else {
            -1
        }
    }
}

/// `{name, category, fields}` from a `MindLogic` row dictionary.
fn row_from_dict(dict: &VarDictionary) -> StatementRow {
    let name = dict
        .get("name")
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default();
    let category = dict
        .get("category")
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let fields = dict
        .get("fields")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|array| {
            array
                .iter_shared()
                .map(|value| value.stringify().to_string())
                .collect()
        })
        .unwrap_or_default();
    StatementRow {
        name,
        category,
        fields,
    }
}

/// `Array<Dictionary>` -> rows (`[]` for anything else).
fn rows_from_variant(value: Variant) -> Vec<StatementRow> {
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .filter_map(|entry| entry.try_to::<VarDictionary>().ok())
        .map(|dict| row_from_dict(&dict))
        .collect()
}

/// Rows -> the `Array<Dictionary>` `MindLogic.write_statements` consumes.
fn rows_to_variant(rows: &[StatementRow]) -> VarArray {
    let mut array = VarArray::new();
    for row in rows {
        let mut dict = VarDictionary::new();
        dict.set("name", &gs(row.name.as_str()).to_variant());
        let mut fields = VarArray::new();
        for field in &row.fields {
            fields.push(&gs(field.as_str()).to_variant());
        }
        dict.set("fields", &fields.to_variant());
        array.push(&dict.to_variant());
    }
    array
}

/// `PackedStringArray(values)` for the `MindLogic` field calls.
fn packed_strings(values: &[String]) -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for value in values {
        out.push(&gs(value.as_str()));
    }
    out
}

/// A `PackedStringArray` return as `Vec<String>`.
fn packed_to_vec(value: &Variant) -> Option<Vec<String>> {
    value.try_to::<PackedStringArray>().ok().map(|packed| {
        packed
            .as_slice()
            .iter()
            .map(|item| item.to_string())
            .collect()
    })
}

// ---------------------------------------------------------------------------
// dictionary helpers (`Dictionary.get(key, default)` semantics)
// ---------------------------------------------------------------------------

fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn dict_bool(dict: &VarDictionary, key: &str, fallback: bool) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(fallback)
}

fn dict_f64(dict: &VarDictionary, key: &str, fallback: f64) -> f64 {
    dict.get(key)
        .and_then(|value| value.try_to::<f64>().ok())
        .unwrap_or(fallback)
}

// ---------------------------------------------------------------------------
// editor state (`LogicDialog` fields)
// ---------------------------------------------------------------------------

/// A `JumpCurve`'s layout data while `DragLayout.setJumpHeights` runs.
#[derive(Clone, Default)]
struct CurveData {
    jump: i64,
    begin: i64,
    end: i64,
    flipped: bool,
    pred: i64,
    done: bool,
}

/// The shared scratch buffers of the recursive height prediction (Java's static
/// `tmpOccupiers2`/`tmpBits2` aliased across recursion levels).
#[derive(Default)]
struct HeightScratch {
    occupiers: Vec<usize>,
    bits: BTreeSet<i64>,
}

/// One content picker entry (`showSelectTable` row).
#[derive(Clone, Default)]
struct AddEntry {
    name: String,
    category: String,
    color: Color,
    label: String,
    type_name: String,
    tooltip: String,
}

/// One sound browser row (`PlaySoundStatement.showSoundSelect`).
#[derive(Clone, Default)]
struct SoundEntry {
    name: String,
    id: String,
    category: String,
}

/// The dialog's mutable model + live node handles (the GDScript's `_` fields).
struct LogicState {
    x: i64,
    y: i64,
    privileged: bool,
    code: String,
    editor_mode: bool,
    filter_mode: bool,
    force_restart: bool,
    statements: Vec<StatementRow>,
    list: Option<Gd<VBoxContainer>>,
    scroll: Option<Gd<ScrollContainer>>,
    modified: bool,
    variables_button: Option<Gd<Button>>,
    add_button: Option<Gd<Button>>,
    vars_popup: Option<Gd<PopupPanel>>,
    vars_rows: Vec<(String, Gd<Label>)>,
    vars_timer: i64,
    row_nodes: Vec<Gd<Control>>,
    jump_buttons: BTreeMap<i64, Gd<Button>>,
    jump_over: i64,
    overlay: Option<Gd<Control>>,
    pred_heights: BTreeMap<i64, i64>,
    curve_heights: BTreeMap<i64, f32>,
    drag_index: i64,
    drag_active: bool,
    drag_start: Vector2,
    drop_index: i64,
    drag_row: Option<Gd<Control>>,
    drag_grab: Vector2,
    jump_selecting: i64,
    jump_hover: i64,
    jump_pointer: Vector2,
    effect_target: (i64, i64),
    effect_connected: bool,
    confirm_connected: bool,
    sub_dialog: Option<Gd<Control>>,
    pending_clear: bool,
    meta_cache: HashMap<String, VarDictionary>,
    region_cache: HashMap<String, Option<Gd<Texture2D>>>,
    node_texture: Option<Gd<Texture2D>>,
}

impl Default for LogicState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            privileged: false,
            code: String::new(),
            editor_mode: false,
            filter_mode: false,
            force_restart: false,
            statements: Vec::new(),
            list: None,
            scroll: None,
            modified: false,
            variables_button: None,
            add_button: None,
            vars_popup: None,
            vars_rows: Vec::new(),
            vars_timer: 0,
            row_nodes: Vec::new(),
            jump_buttons: BTreeMap::new(),
            jump_over: -1,
            overlay: None,
            pred_heights: BTreeMap::new(),
            curve_heights: BTreeMap::new(),
            drag_index: -1,
            drag_active: false,
            drag_start: Vector2::ZERO,
            drop_index: -1,
            drag_row: None,
            drag_grab: Vector2::ZERO,
            jump_selecting: -1,
            jump_hover: -1,
            jump_pointer: Vector2::ZERO,
            effect_target: (-1, -1),
            effect_connected: false,
            confirm_connected: false,
            sub_dialog: None,
            pending_clear: false,
            meta_cache: HashMap::new(),
            region_cache: HashMap::new(),
            node_texture: None,
        }
    }
}

// ---------------------------------------------------------------------------
// text / bundle helpers (`LStatement` / `LCanvas`)
// ---------------------------------------------------------------------------

/// `LogicDialog` bundle lookup (`_t`).
fn bundle(key: &str) -> String {
    kit::t(key).to_string()
}

/// A `GString` from any string-like value (avoids the `From<String>` gap).
fn gs(text: impl AsRef<str>) -> GString {
    GString::from(text.as_ref())
}

/// `LStatement.statementKey()`: `typeName().toLowerCase()`.
fn statement_key(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> String {
    type_name(state, statement_name).to_lowercase()
}

/// `LStatement.typeName()`: the Java class name without `Statement`
/// (`InvalidStatement` -> `Invalid`, `OperationStatement` -> `Operation`).
fn type_name(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> String {
    let variant = dict_string(&meta(state, statement_name), "rust_variant");
    if variant.is_empty() {
        statement_name.to_string()
    } else {
        variant
    }
}

/// Registry metadata for a statement (`statement_meta`, cached).
fn meta(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> VarDictionary {
    if let Some(info) = state.borrow().meta_cache.get(statement_name) {
        return info.clone();
    }
    let mut info = VarDictionary::new();
    if let Some(mut logic) = kit::logic()
        && logic.has_method("statement_meta")
        && let Ok(dict) = logic
            .call("statement_meta", &[gs(statement_name).to_variant()])
            .try_to::<VarDictionary>()
    {
        info = dict;
    }
    state
        .borrow_mut()
        .meta_cache
        .insert(statement_name.to_string(), info.clone());
    info
}

/// `Strings.insertSpaces`: a space before every non-leading upper-case letter.
fn insert_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    for (index, ch) in text.chars().enumerate() {
        if index > 0 && ch.is_uppercase() {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// Arc colour markup (`[accent]`, `[#rrggbb]`, `[]`) removed for plain-text
/// tooltips; `[[` stays a literal bracket (`_plain_markup`).
fn plain_markup(text: &str) -> String {
    let escaped = text.replace("[[", "\u{1}");
    let mut out = String::with_capacity(escaped.len());
    let mut chars = escaped.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '[' {
            out.push(ch);
            continue;
        }
        let mut inner = String::new();
        let mut closed = false;
        for next in chars.by_ref() {
            if next == ']' {
                closed = true;
                break;
            }
            inner.push(next);
        }
        if closed && markup_tag(&inner) {
            continue;
        }
        out.push('[');
        out.push_str(&inner);
        if closed {
            out.push(']');
        }
    }
    out.replace('\u{1}', "[")
}

/// `\[(#[0-9a-fA-F]{3,8}|[a-zA-Z]*)\]`: a colour tag or an alphabetic one.
fn markup_tag(inner: &str) -> bool {
    if let Some(hex) = inner.strip_prefix('#') {
        return (3..=8).contains(&hex.len()) && hex.chars().all(|ch| ch.is_ascii_hexdigit());
    }
    inner.chars().all(|ch| ch.is_ascii_alphabetic())
}

/// `LStatement.logicLocalization(): Core.settings.getBool("logiclocalization")`.
fn logic_localization() -> bool {
    let Some(mut ui) = kit::ui() else {
        return true;
    };
    if !ui.has_method("settings_get_bool") {
        return true;
    }
    ui.call("settings_get_bool", &[gs("logiclocalization").to_variant()])
        .try_to::<bool>()
        .unwrap_or(true)
}

/// `LStatement.localizedName()` (`LStatement.java:371-374`):
/// `instruction.<statementKey>` with `insertSpaces(typeName)` as the fallback;
/// the raw name when the `logiclocalization` setting is off.
fn localized_name(state: &Rc<RefCell<LogicState>>, statement_name: &str) -> String {
    let plain = insert_spaces(&type_name(state, statement_name));
    if !logic_localization() {
        return plain;
    }
    let key = format!("instruction.{}", statement_key(state, statement_name));
    let resolved = bundle(&key);
    if resolved == key { plain } else { resolved }
}

/// `LCanvas.tooltip(cell, key)`: the bundle text when the key exists.
fn bundle_tooltip(key: &str) -> String {
    let lkey = key.to_lowercase().replace(' ', "");
    let resolved = bundle(&lkey);
    if resolved == lkey {
        String::new()
    } else {
        plain_markup(&resolved)
    }
}

/// `bundle(desc)`: `name.token.<token>` with the raw token as the fallback.
fn field_label_text(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    if !logic_localization() {
        return token.to_string();
    }
    let key = format!("name.token.{token}");
    let resolved = bundle(&key);
    if resolved == key {
        token.to_string()
    } else {
        resolved
    }
}

/// The header title: `localizedName()`, plus `-> <dest>` for a set jump.
fn header_text(state: &Rc<RefCell<LogicState>>, row: &StatementRow) -> String {
    let mut text = localized_name(state, &row.name);
    let count = state.borrow().statements.len();
    let dest = row.jump_dest(count);
    if row.name == "jump" && dest >= 0 {
        text.push_str(&format!(" -> {dest}"));
    }
    text
}

// ---------------------------------------------------------------------------
// small node helpers
// ---------------------------------------------------------------------------

/// Adds a `Control` child to any `Container` (the GDScript `add_child`).
fn add_any<C>(parent: &Gd<Container>, child: &Gd<C>)
where
    C: Inherits<Control>,
{
    let mut parent = parent.clone();
    let control: Gd<Control> = child.clone().upcast();
    parent.add_child(&control);
}

/// A sizing-only cell (`MindWidgets.space`) that never takes pointer input.
fn gap(width: f32) -> Gd<Control> {
    let mut gap = factory::space(width, 0.0);
    gap.set_mouse_filter(MouseFilter::IGNORE);
    gap
}

/// `StatementElem.draw`: `Fill.dropShadow` (10 px, 0.9 alpha) under a 30%
/// black fill.
fn shadow_box() -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.3));
    box_.set_shadow_color(Color::from_rgba(0.0, 0.0, 0.0, 0.45));
    box_.set_shadow_size(10);
    box_
}

/// `Styles.outlineLabel` tinted `color` (header name/address).
fn outline_label(text: &str, color: Color) -> Gd<Label> {
    // code-instantiated: header labels are rebuilt per statement element.
    let mut label = Label::new_alloc();
    label.set_text(&gs(text));
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label.add_theme_color_override("font_color", color);
    label.add_theme_color_override("font_outline_color", Color::BLACK);
    label.add_theme_constant_override("outline_size", 4);
    label
}

/// `t.button(Icon.x, Styles.logici, listener).size(24f)`: a black icon, no
/// background.
fn logici_button(icon: &str) -> Gd<Button> {
    // code-instantiated: header buttons are rebuilt per statement element.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(24.0, 24.0));
    button.set_focus_mode(FocusMode::NONE);
    for style_name in [
        "normal",
        "hover",
        "pressed",
        "disabled",
        "focus",
        "hover_pressed",
    ] {
        button.add_theme_stylebox_override(style_name, &StyleBoxEmpty::new_gd());
    }
    let mut glyph = factory::glyph_label(icon, 20, Color::BLACK);
    glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&glyph);
    button
}

/// `Tex.underlineWhite`: a 4 px bottom rule (`Styles.nodeField`/`logict`).
fn underline_box(color: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    box_.set_border_color(color);
    box_.set_border_width(Side::BOTTOM, 4);
    box_.set_content_margin(Side::LEFT, 6.0);
    box_.set_content_margin(Side::RIGHT, 6.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 6.0);
    box_
}

fn flat_box(color: Color) -> Gd<StyleBoxFlat> {
    flat_box_margins(color, 6.0)
}

fn flat_box_margins(color: Color, left: f32) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(color);
    box_.set_content_margin(Side::LEFT, left);
    box_.set_content_margin(Side::RIGHT, 6.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 4.0);
    box_
}

/// `Styles.nodeField` (`underlineWhite` background) tinted with the element
/// colour; the text stays white.
fn apply_node_field(edit: &mut Gd<LineEdit>, color: Color) {
    edit.add_theme_stylebox_override("normal", &underline_box(color));
    edit.add_theme_stylebox_override("focus", &underline_box(color));
    edit.add_theme_stylebox_override("read_only", &underline_box(styles::ARC_GRAY));
    edit.add_theme_color_override("font_color", Color::WHITE);
}

/// `Styles.logict` (`up = underlineWhite`, `over/down = flatOver`) with the
/// cell's `.color(table.color)` tint on the background only.
fn apply_logict(button: &mut Gd<Button>, color: Color) {
    button.add_theme_stylebox_override("normal", &underline_box(color));
    button.add_theme_stylebox_override("hover", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("pressed", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("hover_pressed", &flat_box(styles::FLAT_OVER));
    button.add_theme_color_override("font_color", Color::WHITE);
}

fn field_label(text: &str) -> Gd<Label> {
    // code-instantiated: one label per field-row item (count is layout-driven).
    let mut label = Label::new_alloc();
    label.set_text(&gs(text));
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    label.set_custom_minimum_size(Vector2::new(0.0, 40.0));
    label
}

// ---------------------------------------------------------------------------
// state queries (`LogicDialog` helpers)
// ---------------------------------------------------------------------------

/// `_target_width` / `LCanvas.getTargetWidth`: 410 when compact, else 95% of the
/// screen minus 80 clamped to 400..1200 (`isCompact`: width < 900 * 1.2).
fn target_width(dialog: &MindDialog) -> f32 {
    let width = dialog.base().get_viewport_rect().size.x;
    if width < 900.0 * 1.2 {
        410.0
    } else {
        (width * 0.95 - 80.0).clamp(400.0, 1200.0)
    }
}

/// The current option's display text for a select (`LStatement.bundle(Enum)`
/// then the symbol/name fallback).
fn select_text(statement_name: &str, field_index: i64, value: &str) -> String {
    let Some(mut logic) = kit::logic() else {
        return value.to_string();
    };
    if !logic.has_method("editor_options") {
        return value.to_string();
    }
    let options = logic.call(
        "editor_options",
        &[gs(statement_name).to_variant(), field_index.to_variant()],
    );
    let Ok(array) = options.try_to::<VarArray>() else {
        return value.to_string();
    };
    for option in array.iter_shared() {
        if let Ok(dict) = option.try_to::<VarDictionary>()
            && dict_string(&dict, "name") == value
        {
            return option_text(&dict);
        }
    }
    value.to_string()
}

fn option_text(option: &VarDictionary) -> String {
    let mut text = {
        let label = dict_string(option, "label");
        if label.is_empty() {
            dict_string(option, "name")
        } else {
            label
        }
    };
    let key = dict_string(option, "key");
    // `LStatement.bundle(Enum)`/`selectTranslate` (`LStatement.java:163-184`):
    // no translation when `logiclocalization` is off.
    if !key.is_empty() && logic_localization() {
        let resolved = bundle(&key);
        if resolved != key {
            text = resolved;
        }
    }
    text
}

/// `LCanvas.tooltip(Cell, Enum)` (`LCanvas.java:98-105`): the
/// `<enum-class>.<name>` bundle key first, then `lenum.<name>`.
fn select_tooltip(statement_name: &str, field_index: i64, option: &VarDictionary) -> String {
    let option_name = dict_string(option, "name");
    let mut enum_class = String::new();
    if statement_name == "op" {
        enum_class = "logicop".to_string();
    } else if (statement_name == "select" || statement_name == "jump") && field_index == 1 {
        enum_class = "conditionop".to_string();
    } else {
        let key = dict_string(option, "key");
        let suffix = format!(".label.{}", option_name.to_lowercase());
        if let Some(stripped) = key.strip_suffix(suffix.as_str()) {
            enum_class = stripped.to_string();
        }
    }
    if enum_class.is_empty() {
        return String::new();
    }
    let text = bundle_tooltip(&format!("{}.{}", enum_class, option_name.to_lowercase()));
    if text.is_empty() {
        bundle_tooltip(&format!("lenum.{option_name}"))
    } else {
        text
    }
}

/// `LStatement.showSelect`'s `cols` argument per call site.
fn select_columns(statement_name: &str, field_index: i64) -> i32 {
    match statement_name {
        "draw" | "control" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "radar" | "uradar" => {
            if field_index <= 3 {
                2
            } else {
                4
            }
        }
        "ucontrol" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "ulocate" => {
            if field_index <= 1 {
                2
            } else {
                4
            }
        }
        "query" => 4,
        "select" | "jump" => {
            if field_index == 1 {
                3
            } else {
                4
            }
        }
        "setmarker" => {
            if field_index == 0 {
                3
            } else {
                4
            }
        }
        "makemarker" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "setrule" | "message" | "fetch" => {
            if field_index == 0 {
                2
            } else {
                4
            }
        }
        "cutscene" if field_index == 0 => 3,
        _ => 4,
    }
}

/// The `sizer` cell override for a select (`showSelect(..., cols, sizer)`),
/// else the 60x38 default (`LStatement.showSelect`).
fn select_cell_size(statement_name: &str, field_index: i64) -> Vector2 {
    match statement_name {
        "draw" => {
            if field_index == 0 {
                Vector2::new(120.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "control" => {
            if field_index == 0 {
                Vector2::new(110.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "radar" | "uradar" => {
            if field_index <= 2 {
                Vector2::new(110.0, 50.0)
            } else if field_index == 3 {
                Vector2::new(100.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "ucontrol" => {
            if field_index == 0 {
                Vector2::new(120.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "ulocate" => {
            if field_index == 0 {
                Vector2::new(110.0, 50.0)
            } else if field_index == 1 {
                Vector2::new(120.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "query" => {
            if field_index == 1 {
                Vector2::new(100.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "select" | "jump" => {
            if field_index == 1 {
                Vector2::new(95.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "setmarker" => {
            if field_index == 0 {
                Vector2::new(140.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "makemarker" => {
            if field_index == 0 {
                Vector2::new(160.0, 50.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "setrule" | "message" | "fetch" => {
            if field_index == 0 {
                Vector2::new(150.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "cutscene" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        "op" => {
            if field_index == 0 {
                Vector2::new(64.0, 38.0)
            } else {
                Vector2::new(60.0, 38.0)
            }
        }
        _ => Vector2::new(60.0, 38.0),
    }
}

/// The select button's size (`table.button(...).size(...)` per call site).
fn select_button_size(statement_name: &str, field_index: i64, value: &str) -> Vector2 {
    match statement_name {
        "draw" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "control" => {
            if field_index == 0 {
                Vector2::new(110.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "radar" | "uradar" => {
            if field_index <= 2 {
                Vector2::new(110.0, 40.0)
            } else if field_index == 3 {
                Vector2::new(180.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "ucontrol" => {
            if field_index == 0 {
                Vector2::new(180.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "ulocate" => {
            if field_index == 0 {
                Vector2::new(110.0, 40.0)
            } else if field_index == 1 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "query" => {
            if field_index == 1 {
                Vector2::new(100.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "select" | "jump" => {
            if field_index == 1 {
                if value == "always" {
                    Vector2::new(90.0, 40.0)
                } else {
                    Vector2::new(48.0, 40.0)
                }
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "setmarker" => {
            if field_index == 0 {
                Vector2::new(190.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "makemarker" => {
            if field_index == 0 {
                Vector2::new(180.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "setrule" | "message" | "fetch" => {
            if field_index == 0 {
                Vector2::new(160.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "cutscene" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "op" | "lookup" => {
            if field_index == 0 {
                Vector2::new(64.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "getblock" => {
            if field_index == 0 {
                Vector2::new(120.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        "setblock" => {
            if field_index == 0 {
                Vector2::new(100.0, 40.0)
            } else {
                Vector2::new(90.0, 40.0)
            }
        }
        _ => Vector2::new(90.0, 40.0),
    }
}

fn current_field_value(state: &Rc<RefCell<LogicState>>, index: i64, field_index: i64) -> String {
    if index < 0 || field_index < 0 {
        return String::new();
    }
    let st = state.borrow();
    let Some(row) = st.statements.get(index as usize) else {
        return String::new();
    };
    row.fields
        .get(field_index as usize)
        .cloned()
        .unwrap_or_default()
}

/// `LStatement.field`'s setter: `setter.get(sanitize(text))`. Returns whether
/// the value changed.
fn commit_field_value(
    state: &Rc<RefCell<LogicState>>,
    text: &str,
    index: i64,
    field_index: i64,
) -> bool {
    if index < 0 || field_index < 0 {
        return false;
    }
    let Some(mut logic) = kit::logic() else {
        return false;
    };
    let sanitized = if logic.has_method("sanitize") {
        logic
            .call("sanitize", &[gs(text).to_variant()])
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_else(|_| text.to_string())
    } else {
        text.to_string()
    };
    let mut st = state.borrow_mut();
    let Some(row) = st.statements.get_mut(index as usize) else {
        return false;
    };
    let Some(slot) = row.fields.get_mut(field_index as usize) else {
        return false;
    };
    if *slot == sanitized {
        return false;
    }
    *slot = sanitized;
    st.modified = true;
    true
}

/// `JumpButton`'s setter: writes the `dest_index` field (`-1` clears).
fn set_jump_target(state: &Rc<RefCell<LogicState>>, index: i64, target: i64) -> bool {
    if index < 0 {
        return false;
    }
    let mut st = state.borrow_mut();
    let Some(row) = st.statements.get_mut(index as usize) else {
        return false;
    };
    let Some(slot) = row.fields.first_mut() else {
        return false;
    };
    let next = target.to_string();
    if *slot == next {
        return false;
    }
    *slot = next;
    st.modified = true;
    true
}

/// Element references (`JumpStatement.dest`) survive an insertion at `at`:
/// every target at or below it moves down one (`saveUI` recomputes the index).
fn shift_jumps_for_insert(state: &Rc<RefCell<LogicState>>, at: i64) {
    let mut st = state.borrow_mut();
    for row in st.statements.iter_mut() {
        if row.name != "jump" {
            continue;
        }
        let Some(slot) = row.fields.first_mut() else {
            continue;
        };
        let Ok(dest) = slot.parse::<i64>() else {
            continue;
        };
        if dest >= at {
            *slot = (dest + 1).to_string();
        }
    }
}

/// `StatementElem.copy` (`LCanvas.java:517-536`): the duplicate lands after the
/// element; a copied `jump`'s target bumps when it pointed below (`:521-526`).
fn copy_statement(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>, index: i64) {
    let (count, original) = {
        let st = state.borrow();
        (
            st.statements.len(),
            st.statements.get(index.max(0) as usize).cloned(),
        )
    };
    let Some(original) = original else {
        return;
    };
    if index < 0 || count >= MAX_INSTRUCTIONS {
        return;
    }
    let mut copied = original.clone();
    shift_jumps_for_insert(state, index + 1);
    state
        .borrow_mut()
        .statements
        .insert(index as usize + 1, copied.clone());
    if original.name == "jump"
        && let Some(first) = copied.fields.first()
        && let Ok(dest) = first.parse::<i64>()
        && dest > index
    {
        copied.fields[0] = (dest + 1).to_string();
        state.borrow_mut().statements[index as usize + 1] = copied;
    }
    state.borrow_mut().modified = true;
    rebuild_list(dialog, state);
}

/// `StatementElem`'s delete button: dropped targets become `-1` and later
/// indices shift down (Java's `saveUI` recomputes `indexOf(dest)`).
fn delete_statement(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>, index: i64) {
    if index < 0 || state.borrow().statements.len() <= index as usize {
        return;
    }
    {
        let mut st = state.borrow_mut();
        st.statements.remove(index as usize);
        for row in st.statements.iter_mut() {
            if row.name != "jump" {
                continue;
            }
            let Some(slot) = row.fields.first_mut() else {
                continue;
            };
            let Ok(dest) = slot.parse::<i64>() else {
                continue;
            };
            if dest == index {
                *slot = "-1".to_string();
            } else if dest > index {
                *slot = (dest - 1).to_string();
            }
        }
        st.modified = true;
    }
    rebuild_list(dialog, state);
}

/// `shouldShowVariables`: an executor with variables outside the menu state.
/// The map editor and the filter/objective code editors run in `State.menu`
/// (`MapEditorDialog.java:317`), so they show `@logic.globals`; a live
/// processor always has `@counter`.
fn should_show_variables(state: &Rc<RefCell<LogicState>>) -> bool {
    let (editor, filter, x, y) = {
        let st = state.borrow();
        (st.editor_mode, st.filter_mode, st.x, st.y)
    };
    if editor || filter {
        return false;
    }
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("logic_get_state") {
        return false;
    }
    let Ok(state_dict) = host
        .call("logic_get_state", &[x.to_variant(), y.to_variant()])
        .try_to::<VarDictionary>()
    else {
        return false;
    };
    let Some(vars) = state_dict.get("vars") else {
        return false;
    };
    vars.try_to::<VarArray>()
        .map(|array| !array.is_empty())
        .unwrap_or(false)
}

/// The variables button's label (`@variables` vs `@logic.globals`) and the
/// `@add` button's `disabled` (`>= maxInstructions`).
fn update_buttons(state: &Rc<RefCell<LogicState>>) {
    let show_variables = should_show_variables(state);
    let label = if show_variables {
        bundle("@variables")
    } else {
        bundle("@logic.globals")
    };
    let variables = state.borrow().variables_button.clone();
    if let Some(button) = variables {
        set_button_text(&button, &label);
    }
    let full = state.borrow().statements.len() >= MAX_INSTRUCTIONS;
    let add = state.borrow().add_button.clone();
    if let Some(mut button) = add {
        button.set_disabled(full);
    }
}

/// Sets an `icon_button`'s label (the glyph row's label child, else `text`).
fn set_button_text(button: &Gd<Button>, text: &str) {
    if !button.is_instance_valid() {
        return;
    }
    let children = button.get_children();
    if let Some(row) = children.iter_shared().next() {
        for (index, child) in row.get_children().iter_shared().enumerate() {
            if let Ok(label) = child.clone().try_cast::<Label>()
                && index > 0
                && !label.get_text().is_empty()
            {
                let mut label = label;
                label.set_text(&gs(text));
                return;
            }
        }
    }
    let mut button = button.clone();
    button.set_text(&gs(text));
}

// ---------------------------------------------------------------------------
// region / style helpers
// ---------------------------------------------------------------------------

/// Atlas region lookup cached per dialog (`MindAssets.find_region`).
fn region(state: &Rc<RefCell<LogicState>>, region_name: &str) -> Option<Gd<Texture2D>> {
    if let Some(texture) = state.borrow().region_cache.get(region_name) {
        return texture.clone();
    }
    let texture = kit::icon_texture(region_name);
    if texture.is_some() {
        state
            .borrow_mut()
            .region_cache
            .insert(region_name.to_string(), texture.clone());
    }
    texture
}

/// `Tex.whitePane` (5 px white 9-patch border, 12/11 splits) tinted with the
/// element colour; `margin(0f)` drops the drawable padding except the
/// `marginBottom(7)`.
fn white_pane_box(state: &Rc<RefCell<LogicState>>, color: Color) -> Gd<StyleBox> {
    if let Some(texture) = region(state, "white-pane") {
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, 12.0);
        box_.set_texture_margin(Side::TOP, 12.0);
        box_.set_texture_margin(Side::RIGHT, 11.0);
        box_.set_texture_margin(Side::BOTTOM, 11.0);
        box_.set_modulate(color);
        box_.set_content_margin(Side::LEFT, 0.0);
        box_.set_content_margin(Side::TOP, 0.0);
        box_.set_content_margin(Side::RIGHT, 0.0);
        box_.set_content_margin(Side::BOTTOM, 7.0);
        return box_.upcast();
    }
    let mut flat = StyleBoxFlat::new_gd();
    flat.set_draw_center(false);
    flat.set_border_color(color);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        flat.set_border_width(side, 5);
    }
    flat.set_content_margin(Side::BOTTOM, 7.0);
    flat.upcast()
}

/// `Tex.button` 9-patch (12/11 splits) for the Add/Edit content tables.
fn button_box(state: &Rc<RefCell<LogicState>>) -> Gd<StyleBox> {
    if let Some(texture) = region(state, "button") {
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, 12.0);
        box_.set_texture_margin(Side::TOP, 12.0);
        box_.set_texture_margin(Side::RIGHT, 11.0);
        box_.set_texture_margin(Side::BOTTOM, 11.0);
        return box_.upcast();
    }
    MindDialog::pane_box(Color::BLACK, styles::PANE_OUTLINE_GRAY, 4.0, 2.0, -1.0).upcast()
}

// ---------------------------------------------------------------------------
// signal plumbing (`Gd<MindDialog>` + shared state callbacks)
// ---------------------------------------------------------------------------

/// Connects a signal whose handler needs the dialog and the shared state.
///
/// The handler only runs from engine-emitted signals (never re-entrant into a
/// bound `MindDialog`), so binding the dialog here is safe.
fn on_signal<F>(
    node: &Gd<Node>,
    signal: &str,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    action: F,
) where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>, &[&Variant]) + 'static,
{
    let dialog = dialog.clone();
    let state = state.clone();
    let callable = Callable::from_fn("logic_action", move |args: &[&Variant]| {
        if !dialog.is_instance_valid() || dialog.is_queued_for_deletion() {
            return Variant::nil();
        }
        let mut handle = dialog.clone();
        let mut bound = handle.bind_mut();
        action(&mut bound, &state, args);
        Variant::nil()
    });
    let mut node = node.clone();
    node.connect(signal, &callable);
}

/// `on_signal` for a zero-argument `pressed` handler.
fn on_pressed<F>(
    button: &Gd<Button>,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    action: F,
) where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>) + 'static,
{
    let node: Gd<Node> = button.clone().upcast();
    on_signal(&node, "pressed", dialog, state, move |d, s, _args| {
        action(d, s);
    });
}

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// A `Callable` that queues `action` for the idle phase (`call_deferred`).
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}

// ---------------------------------------------------------------------------
// `LogicDialog` controller
// ---------------------------------------------------------------------------

/// `@logic` — the mlog logic editor.
pub struct LogicDialog {
    state: Rc<RefCell<LogicState>>,
}

impl Default for LogicDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(LogicState::default())),
        }
    }
}

impl DialogController for LogicDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        dialog.set_title_literal("");
        dialog.enable_full_dialog(true);
        dialog.should_pause = true;
        dialog.clear_content();
        dialog.clear_buttons();
        build(dialog, &state);
        connect_resize(dialog, &state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        show_loaded(dialog, &state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        close_sub_dialog(&state, false);
        state.borrow_mut().pending_clear = false;
        let needs_save = {
            let st = state.borrow();
            st.modified || st.force_restart
        };
        if !needs_save {
            return;
        }
        save(dialog, &state);
    }

    fn blocks_escape(&self) -> bool {
        self.state.borrow().sub_dialog.is_some()
    }
}

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "logic" => Some(Box::new(LogicDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// shell (`LogicDialog.setup` / `_build`)
// ---------------------------------------------------------------------------

/// `_build`: the clipped canvas holder (scroll + centred statement column +
/// jump/drop overlay) and the 160x64 `@back`/`@edit`/`@variables`/`@add` row.
fn build(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    if let Some(mut content) = dialog.content() {
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
    }
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
    }
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the canvas holder clips the jump overlay to the pane
    // (`JumpCurve` draws inside `canvas.pane`).
    let mut holder = Control::new_alloc();
    holder.set_clip_contents(true);
    holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
    holder.set_v_size_flags(SizeFlags::EXPAND_FILL);
    holder.set_custom_minimum_size(Vector2::new(0.0, 320.0));
    if let Some(mut content) = dialog.content() {
        content.add_child(&holder);
    } else {
        dialog.base_mut().add_child(&holder);
    }
    // code-instantiated: the canvas pane owns the runtime-sized statement list.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    holder.add_child(&scroll);
    // code-instantiated: `t.center()` inside the pane centres the column.
    let mut center = CenterContainer::new_alloc();
    center.set_h_size_flags(SizeFlags::EXPAND_FILL);
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&center);
    // code-instantiated: one element per statement; the count is the program
    // length (`DragLayout`, `space = 10`).
    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", ELEMENT_SPACE);
    list.set_custom_minimum_size(Vector2::new(target_width(dialog), 0.0));
    center.add_child(&list);
    {
        let mut st = state.borrow_mut();
        st.list = Some(list);
        st.scroll = Some(scroll);
    }
    // code-instantiated: the jump-arrow and drop-line overlay sits above the
    // statement list (`LCanvas.DragLayout.draw` + `JumpCurve`); it never takes
    // mouse events and redraws every frame.
    let mut overlay = MindLogicOverlay::new_alloc();
    {
        let mut bound = overlay.bind_mut();
        bound.state = state.clone();
        bound.dialog = Some(dialog_gd.clone());
    }
    let mut overlay_control: Gd<Control> = overlay.upcast();
    overlay_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay_control.set_mouse_filter(MouseFilter::IGNORE);
    holder.add_child(&overlay_control);
    state.borrow_mut().overlay = Some(overlay_control);

    // `LogicDialog.setup`: `buttons.defaults().size(160f, 64f)`.
    bottom_button(dialog, state, "left", "@back", |d, _s| d.close());
    bottom_button(dialog, state, "edit", "@edit", |d, s| {
        open_edit_menu(d, s);
    });
    let variables = bottom_button(dialog, state, "menu", "@variables", |d, s| {
        open_variables_panel(d, s);
    });
    let add = bottom_button(dialog, state, "add", "@add", |d, s| {
        open_add_dialog(d, s, -1);
    });
    {
        let mut st = state.borrow_mut();
        st.variables_button = Some(variables);
        st.add_button = Some(add);
    }
}

/// One `buttons.button(text, icon, listener)` cell at 160x64.
fn bottom_button<F>(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    icon: &str,
    key: &str,
    action: F,
) -> Gd<Button>
where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>) + 'static,
{
    // code-instantiated: the editor's button row is built by `setup()`.
    let mut button = factory::icon_button(icon, &bundle(key), "defaultt", AlignmentMode::BEGIN);
    button.set_custom_minimum_size(Vector2::new(160.0, 64.0));
    let dialog_gd = dialog.to_gd();
    on_pressed(&button, &dialog_gd, state, action);
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
    button
}

/// `LogicDialog.onResize`: the column follows `getTargetWidth`.
fn on_resize(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let width = target_width(dialog);
    let (list, rows) = {
        let st = state.borrow();
        (st.list.clone(), st.row_nodes.clone())
    };
    if let Some(mut list) = list
        && list.is_instance_valid()
    {
        let mut min = list.get_custom_minimum_size();
        min.x = width;
        list.set_custom_minimum_size(min);
    }
    for mut row in rows {
        if !row.is_instance_valid() {
            continue;
        }
        let mut min = row.get_custom_minimum_size();
        min.x = width;
        row.set_custom_minimum_size(min);
    }
}

/// Connects the viewport's `size_changed` -> [`on_resize`] once per node.
fn connect_resize(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let dialog_gd = dialog.to_gd();
    if dialog_gd.has_meta(RESIZE_META) {
        return;
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = dialog_gd.clone();
    let state_for_resize = state.clone();
    let callable = Callable::from_fn("logic_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return Variant::nil();
        }
        let mut handle = target.clone();
        let mut bound = handle.bind_mut();
        on_resize(&mut bound, &state_for_resize);
        Variant::nil()
    });
    viewport.connect("size_changed", &callable);
    let mut node = dialog_gd;
    node.set_meta(RESIZE_META, &true.to_variant());
}

// ---------------------------------------------------------------------------
// statement column (`LCanvas`/`DragLayout`)
// ---------------------------------------------------------------------------

/// Rebuilds the statement column from [`LogicState::statements`].
fn rebuild_list(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let (list, drag_row, keep_scroll) = {
        let st = state.borrow();
        (
            st.list.clone(),
            st.drag_row.clone(),
            st.scroll
                .as_ref()
                .map(|scroll| scroll.get_v_scroll())
                .unwrap_or(0),
        )
    };
    // A dragged element lives under the overlay while it follows the pointer;
    // the rebuild always owns the final element list (`finishLayout`).
    if let Some(mut row) = drag_row
        && row.is_instance_valid()
    {
        if let Some(mut parent) = row.get_parent() {
            parent.remove_child(&row);
        }
        row.queue_free();
    }
    {
        let mut st = state.borrow_mut();
        st.drag_row = None;
        st.drag_active = false;
    }
    let Some(mut list) = list else {
        return;
    };
    for child in list.get_children().iter_shared() {
        let mut child = child.clone();
        list.remove_child(&child);
        child.queue_free();
    }
    {
        let mut st = state.borrow_mut();
        st.row_nodes.clear();
        st.jump_buttons.clear();
        st.jump_over = -1;
    }
    let rows = state.borrow().statements.clone();
    for (index, row) in rows.iter().enumerate() {
        let panel = statement_row(dialog, state, index as i64, row);
        list.add_child(&panel);
        state.borrow_mut().row_nodes.push(panel);
    }
    compute_jump_heights(state);
    if let Some(mut scroll) = state.borrow().scroll.clone() {
        scroll.set_deferred("scroll_vertical", &keep_scroll.to_variant());
    }
    update_buttons(state);
    if let Some(mut overlay) = state.borrow().overlay.clone() {
        overlay.queue_redraw();
    }
}

/// One `LCanvas.StatementElem` (`LCanvas.java:417-510`): a `Tex.whitePane`
/// frame tinted with the category colour over a drop shadow and a 30% black
/// fill (`draw`), the 38 px `Tex.whiteui` header (name, growX spacer, address,
/// `+`/copy/delete `logici` buttons) and the statement's field row.
fn statement_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    row: &StatementRow,
) -> Gd<Control> {
    let statement_name = row.name.clone();
    let category = row.category.clone();
    let color = category_color(&category);
    let full = state.borrow().statements.len() >= MAX_INSTRUCTIONS;
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one element per statement; the count is the program
    // length, not scene-authorable.
    let mut elem = PanelContainer::new_alloc();
    elem.set_custom_minimum_size(Vector2::new(target_width(dialog), 0.0));
    elem.set_mouse_filter(MouseFilter::PASS);
    elem.add_theme_stylebox_override("panel", &shadow_box());
    let mut frame = PanelContainer::new_alloc();
    frame.set_mouse_filter(MouseFilter::PASS);
    frame.add_theme_stylebox_override("panel", &white_pane_box(state, color));
    elem.add_child(&frame);
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 0);
    column.set_mouse_filter(MouseFilter::PASS);
    frame.add_child(&column);

    let mut header = PanelContainer::new_alloc();
    header.set_custom_minimum_size(Vector2::new(0.0, 38.0));
    header.set_mouse_filter(MouseFilter::STOP);
    header.set_default_cursor_shape(CursorShape::POINTING_HAND);
    let mut header_box = StyleBoxFlat::new_gd();
    header_box.set_bg_color(color);
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        header_box.set_content_margin(side, 6.0);
    }
    header.add_theme_stylebox_override("panel", &header_box);
    column.add_child(&header);
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 0);
    box_.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&box_);
    let mut hbox: Gd<Container> = box_.clone().upcast();
    add_any(&hbox, &outline_label(&header_text(state, row), color));
    add_any(&hbox, &gap(8.0));
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    hbox.add_child(&spacer);
    add_any(&hbox, &outline_label(&index.to_string(), color));
    add_any(&hbox, &gap(8.0));
    // "taken from foo's client": `showAddDialog(index + 1)`.
    let mut add = logici_button("add");
    add.set_disabled(full);
    add.set_tooltip_text(&gs(bundle("@add")));
    on_pressed(&add, &dialog_gd, state, move |d, s| {
        open_add_dialog(d, s, index + 1);
    });
    hbox.add_child(&add);
    add_any(&hbox, &gap(6.0));
    let mut copy = logici_button("copy");
    copy.set_disabled(full);
    copy.set_tooltip_text(&gs(bundle("@copy")));
    on_pressed(&copy, &dialog_gd, state, move |d, s| {
        copy_statement(d, s, index);
    });
    hbox.add_child(&copy);
    add_any(&hbox, &gap(6.0));
    let mut delete = logici_button("cancel");
    delete.set_tooltip_text(&gs(bundle("@delete")));
    on_pressed(&delete, &dialog_gd, state, move |d, s| {
        delete_statement(d, s, index);
    });
    hbox.add_child(&delete);
    // `StatementElem`'s header drag: left-drag reorders, middle-click copies.
    let header_node: Gd<Node> = header.clone().upcast();
    {
        let target = dialog_gd.clone();
        let state_for_input = state.clone();
        let header_for_input = header.clone();
        let callable = Callable::from_fn("logic_header_input", move |args: &[&Variant]| {
            let Some(event) = args
                .first()
                .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
            else {
                return Variant::nil();
            };
            if !target.is_instance_valid() {
                return Variant::nil();
            }
            let mut handle = target.clone();
            let mut bound = handle.bind_mut();
            header_input(
                &mut bound,
                &state_for_input,
                event,
                index,
                &header_for_input,
            );
            Variant::nil()
        });
        let mut header_node = header_node;
        header_node.connect("gui_input", &callable);
    }

    // `add(t).pad(4).padTop(2).left().grow()` with `marginLeft(4)` (+
    // `marginRight(4)` when wrapping); `marginBottom(7)` sits on the frame.
    let mut body_margin = MarginContainer::new_alloc();
    body_margin.set_mouse_filter(MouseFilter::PASS);
    let wraps = dict_bool(&meta(state, &statement_name), "use_wrapping", true);
    body_margin.add_theme_constant_override("margin_left", 8);
    body_margin.add_theme_constant_override("margin_right", if wraps { 8 } else { 4 });
    body_margin.add_theme_constant_override("margin_top", 2);
    body_margin.add_theme_constant_override("margin_bottom", 4);
    column.add_child(&body_margin);
    let fields = field_row(dialog, state, index, row, color, wraps);
    body_margin.add_child(&fields);
    elem.upcast()
}

// ---------------------------------------------------------------------------
// field row (`LStatement.build(Table)`)
// ---------------------------------------------------------------------------

/// The per-statement field row: `MindLogic.editor_layout` returns the exact Java
/// item order (labels, `=`/`linknum` separators, enum selects, and the
/// colour/align/char pencils). Wrapping statements flow like `WrapTable`; the
/// rest (`jump`) stay on one row.
fn field_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    row: &StatementRow,
    color: Color,
    wraps: bool,
) -> Gd<Control> {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the field row is rebuilt per statement on every list
    // rebuild (`LCanvas.StatementElem` calls `st.build(t)`).
    let mut fields_box: Gd<Container> = if wraps {
        let mut flow = HFlowContainer::new_alloc();
        flow.add_theme_constant_override("h_separation", 4);
        flow.add_theme_constant_override("v_separation", 2);
        flow.upcast()
    } else {
        let mut hbox = HBoxContainer::new_alloc();
        hbox.add_theme_constant_override("separation", 4);
        hbox.upcast()
    };
    fields_box.set_mouse_filter(MouseFilter::PASS);
    let Some(mut logic) = kit::logic() else {
        return fields_box.upcast();
    };
    let statement_name = row.name.clone();
    let mut values = row.fields.clone();
    // `SetRuleStatement.rebuild` normalizes `p1` to `@sharded` for team rules;
    // Java mutates the statement at build time, so the dialog mirrors it.
    if logic.has_method("editor_normalize") {
        let normalized = logic.call(
            "editor_normalize",
            &[
                gs(statement_name.as_str()).to_variant(),
                packed_strings(&values).to_variant(),
            ],
        );
        if let Some(changed) = packed_to_vec(&normalized)
            && changed != values
        {
            values = changed;
            if let Some(row) = state.borrow_mut().statements.get_mut(index.max(0) as usize) {
                row.fields = values.clone();
            }
            state.borrow_mut().modified = true;
        }
    }
    if !logic.has_method("editor_layout") {
        return fields_box.upcast();
    }
    let items = logic.call(
        "editor_layout",
        &[
            gs(statement_name.as_str()).to_variant(),
            packed_strings(&values).to_variant(),
        ],
    );
    let Ok(items) = items.try_to::<VarArray>() else {
        return fields_box.upcast();
    };
    let items = filter_layout(&statement_name, &values, items);
    for entry in items.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let kind = dict_string(&entry, "kind");
        let field_index = entry
            .get("index")
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(-1);
        let label = field_label_text(&dict_string(&entry, "label"));
        let label_before = dict_bool(&entry, "label_before", true);
        let value = if field_index >= 0 && (field_index as usize) < values.len() {
            values[field_index as usize].clone()
        } else {
            String::new()
        };
        match kind.as_str() {
            "text" => {
                if !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
            }
            "field" => {
                if label_before && !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
                // `LStatement.field`: `Styles.nodeField`, `size(180f, 40f)`,
                // tinted with the element colour; `print`/`format` grow.
                // code-instantiated: one text field per statement field.
                let mut edit = LineEdit::new_alloc();
                edit.set_text(&gs(value.as_str()));
                let mut width = 180.0_f32;
                // `ApplyStatusStatement`'s effect field is `.width(240f)`
                // (`LStatements.java:1447`).
                if statement_name == "status" && field_index == 1 {
                    width = 240.0;
                }
                edit.set_custom_minimum_size(Vector2::new(width, 40.0));
                if dict_bool(&entry, "wide", false) {
                    edit.set_h_size_flags(SizeFlags::EXPAND_FILL);
                }
                apply_node_field(&mut edit, color);
                edit.set_tooltip_text(&gs(bundle_tooltip(&format!(
                    "{}.{}",
                    insert_spaces(&type_name(state, &statement_name)),
                    label
                ))));
                on_signal(
                    &edit.clone().upcast(),
                    "text_submitted",
                    &dialog_gd,
                    state,
                    move |_d, s, args| {
                        let text = args
                            .first()
                            .map(|value| value.stringify().to_string())
                            .unwrap_or_default();
                        commit_field_value(s, &text, index, field_index);
                    },
                );
                let edit_for_blur = edit.clone();
                on_signal(
                    &edit.clone().upcast(),
                    "focus_exited",
                    &dialog_gd,
                    state,
                    move |_d, s, _args| {
                        commit_field_value(
                            s,
                            &edit_for_blur.get_text().to_string(),
                            index,
                            field_index,
                        );
                    },
                );
                add_any(&fields_box, &edit);
                let picker = picker_kind(&statement_name, field_index, &values);
                if !picker.is_empty() {
                    add_any(
                        &fields_box,
                        &picker_button(&dialog_gd, state, index, field_index, picker, color),
                    );
                }
                if !label_before && !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
            }
            "select" => {
                if label_before && !label.is_empty() {
                    add_any(&fields_box, &field_label(&label));
                }
                // `EffectStatement.build` swaps the enum select for a 150-wide
                // wrapped button that opens `ui.effects`
                // (`LStatements.java:1801-1812`).
                if statement_name == "effect" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &effect_button(&dialog_gd, state, index, field_index, color),
                    );
                    continue;
                }
                // Java's two-state `table.button`s, not `showSelect` popups:
                // apply/clear (`:1437-1444`), global/positional (`:2192-2196`),
                // circle/rect (`:1227-1231`).
                if statement_name == "status" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &field_toggle(
                            &dialog_gd,
                            state,
                            index,
                            field_index,
                            &value,
                            ToggleSpec {
                                on_value: "true",
                                off_value: "false",
                                on_label: "clear",
                                off_label: "apply",
                                size: Vector2::new(80.0, 40.0),
                            },
                            color,
                        ),
                    );
                    continue;
                }
                if statement_name == "playsound" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &field_toggle(
                            &dialog_gd,
                            state,
                            index,
                            field_index,
                            &value,
                            ToggleSpec {
                                on_value: "true",
                                off_value: "false",
                                on_label: "positional",
                                off_label: "global",
                                size: Vector2::new(160.0, 40.0),
                            },
                            color,
                        ),
                    );
                    continue;
                }
                if statement_name == "query" && field_index == 0 {
                    add_any(
                        &fields_box,
                        &field_toggle(
                            &dialog_gd,
                            state,
                            index,
                            field_index,
                            &value,
                            ToggleSpec {
                                on_value: "rect",
                                off_value: "circle",
                                on_label: "rect",
                                off_label: "circle",
                                size: Vector2::new(80.0, 40.0),
                            },
                            color,
                        ),
                    );
                    continue;
                }
                let mut button =
                    factory::button(&select_text(&statement_name, field_index, &value));
                // `Styles.logict` tinted with the element colour.
                button.set_theme_type_variation("logict");
                apply_logict(&mut button, color);
                let size = select_button_size(&statement_name, field_index, &value);
                button.set_custom_minimum_size(size);
                let statement_for_select = statement_name.clone();
                on_pressed(&button, &dialog_gd, state, move |d, s| {
                    open_select(d, s, index, field_index, &statement_for_select);
                });
                add_any(&fields_box, &button);
            }
            "color" => {
                // code-instantiated: one colour picker per colour field.
                let mut picker = ColorPickerButton::new_alloc();
                picker.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                picker.set_pick_color(parse_color(&value));
                on_signal(
                    &picker.clone().upcast(),
                    "color_changed",
                    &dialog_gd,
                    state,
                    move |_d, s, args| {
                        let color = args
                            .first()
                            .and_then(|value| value.try_to::<Color>().ok())
                            .unwrap_or(Color::WHITE);
                        set_color(s, color, index, field_index);
                    },
                );
                add_any(&fields_box, &picker);
            }
            "align" => {
                let mut button = factory::image_button("pencil", "logict");
                apply_logict(&mut button, color);
                button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                button.set_tooltip_text(&gs(field_label_text("align")));
                on_pressed(&button, &dialog_gd, state, move |d, s| {
                    open_align_select(d, s, index, field_index);
                });
                add_any(&fields_box, &button);
            }
            "char" => {
                let mut button = factory::image_button("pencil", "logict");
                apply_logict(&mut button, color);
                button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                on_pressed(&button, &dialog_gd, state, move |d, s| {
                    open_char_select(d, s, index, field_index);
                });
                add_any(&fields_box, &button);
            }
            "jump" => {
                // `table.add().growX(); table.add(new JumpButton(...)).size(30).right()`.
                // code-instantiated: the growX spacer that pushes the node right.
                let mut push = Control::new_alloc();
                push.set_h_size_flags(SizeFlags::EXPAND_FILL);
                push.set_mouse_filter(MouseFilter::IGNORE);
                fields_box.add_child(&push);
                add_any(&fields_box, &jump_button(&dialog_gd, state, index));
            }
            _ => {}
        }
    }
    fields_box.upcast()
}

/// `LStatements.java:1483-1485` (`ApplyStatusStatement.isPermanent`): the
/// `seconds` field disappears for a `permanent` status.
fn filter_layout(statement_name: &str, values: &[String], items: VarArray) -> VarArray {
    if statement_name != "status" || values.len() < 2 || values[0] == "true" {
        return items;
    }
    if !status_permanent(&values[1]) {
        return items;
    }
    let mut out = VarArray::new();
    for entry in items.iter_shared() {
        let keep = match entry.try_to::<VarDictionary>() {
            Ok(dict) => dict_string(&dict, "label") != "seconds",
            Err(_) => true,
        };
        if keep {
            out.push(&entry);
        }
    }
    out
}

/// `content.statusEffect(effect.substring(8)).permanent`, through the sim's
/// content snapshot (`MindSimHost.status_permanent`).
fn status_permanent(effect: &str) -> bool {
    if !effect.starts_with("@status-") {
        return false;
    }
    let Some(mut host) = kit::sim_host() else {
        return false;
    };
    if !host.has_method("status_permanent") {
        return false;
    }
    host.call("status_permanent", &[gs(effect).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// The content picker bound to a statement field, if any (`CONTENT_PICKERS`;
/// `LStatements.java` `showSelectTable` call sites).
fn picker_kind(statement_name: &str, field_index: i64, values: &[String]) -> &'static str {
    let first = values.first().map(String::as_str).unwrap_or("");
    if statement_name == "setmarker" && first != "drawLayer" {
        return "";
    }
    if statement_name == "ulocate" && first != "ore" {
        return "";
    }
    content_picker(statement_name, field_index)
}

/// The two-state `logict` button data.
struct ToggleSpec {
    on_value: &'static str,
    off_value: &'static str,
    on_label: &'static str,
    off_label: &'static str,
    size: Vector2,
}

/// A two-state `logict` button (`Table.button(label, style, ...)`): pressing
/// commits the other value and rebuilds the element (`build(table)`).
fn field_toggle(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    value: &str,
    spec: ToggleSpec,
    color: Color,
) -> Gd<Button> {
    // code-instantiated: one toggle per two-state statement field.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("logict");
    apply_logict(&mut button, color);
    button.set_custom_minimum_size(spec.size);
    let label = if value == spec.on_value {
        spec.on_label
    } else {
        spec.off_label
    };
    button.set_text(&gs(field_label_text(label)));
    let next = if value == spec.on_value {
        spec.off_value
    } else {
        spec.on_value
    };
    on_pressed(&button, dialog, state, move |d, s| {
        commit_field_value(s, next, index, field_index);
        s.borrow_mut().modified = true;
        rebuild_list(d, s);
    });
    button
}

/// A `Icon.pencilSmall` content-picker button (`LStatements.java`
/// `t.button(b -> { b.image(Icon.pencilSmall); ... }, Styles.logict, ...)`).
fn picker_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    kind: &str,
    color: Color,
) -> Gd<Button> {
    // code-instantiated: one picker pencil per statement field.
    let mut button = factory::icon_button("pencil", "", "logict", AlignmentMode::BEGIN);
    apply_logict(&mut button, color);
    button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    let dialog = dialog.clone();
    let state = state.clone();
    let kind = kind.to_string();
    let callable = Callable::from_fn("logic_picker", move |_args: &[&Variant]| {
        if dialog.is_instance_valid() {
            let mut handle = dialog.clone();
            let mut bound = handle.bind_mut();
            open_picker(&mut bound, &state, index, field_index, &kind);
        }
        Variant::nil()
    });
    let mut node: Gd<Node> = button.clone().upcast();
    node.connect("pressed", &callable);
    button
}

/// Dispatches the content pencil to its `showSelectTable` equivalent
/// (`LStatements.java` `showSelectTable` call sites).
fn open_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    kind: &str,
) {
    // Content pickers (`showSelectTable`): dispatched per `CONTENT_PICKERS`.
    match kind {
        "unit" => open_icon_grid(dialog, state, index, field_index, "unit", "unit-control"),
        "ore" => open_icon_grid(dialog, state, index, field_index, "item", "item-ore"),
        "status" => open_status_picker(dialog, state, index, field_index),
        "weather" => open_weather_picker(dialog, state, index, field_index),
        "layer" => open_layer_picker(dialog, state, index, field_index),
        "prop" => open_prop_picker(dialog, state, index, field_index),
        "sensor" => open_sensor_picker(dialog, state, index, field_index),
        "sound" => open_sound_picker(dialog, state, index, field_index),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// select popups (`LStatement.showSelect`)
// ---------------------------------------------------------------------------

/// `showSelect`: a popup of `logicTogglet` buttons (`LStatement.java:233-253`).
fn open_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    statement_name: &str,
) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("editor_options") {
        return;
    }
    let options = logic.call(
        "editor_options",
        &[gs(statement_name).to_variant(), field_index.to_variant()],
    );
    let Ok(options) = options.try_to::<VarArray>() else {
        return;
    };
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one transient enum-select popup per open.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(select_columns(statement_name, field_index));
    popup.add_child(&grid);
    let unit_actions = unit_actions_enabled();
    let cell_size = select_cell_size(statement_name, field_index);
    let current = current_field_value(state, index, field_index);
    for option in options.iter_shared() {
        let Ok(option) = option.try_to::<VarDictionary>() else {
            continue;
        };
        let option_name = dict_string(&option, "name");
        // `UnitControlStatement` hides build/deconstruct when the rules disable them.
        if statement_name == "ucontrol"
            && !unit_actions
            && (option_name == "build" || option_name == "deconstruct")
        {
            continue;
        }
        // code-instantiated: one toggle per enum option.
        let mut toggle = Button::new_alloc();
        toggle.set_text(&gs(option_text(&option)));
        toggle.set_toggle_mode(true);
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_pressed(option_name == current);
        toggle.set_custom_minimum_size(cell_size);
        toggle.set_tooltip_text(&gs(select_tooltip(statement_name, field_index, &option)));
        {
            let dialog_for_select = dialog_gd.clone();
            let state_for_select = state.clone();
            let selected = option_name.clone();
            let popup_for_select = popup.clone();
            let callable = Callable::from_fn("logic_select", move |_args: &[&Variant]| {
                let mut popup = popup_for_select.clone();
                popup.hide();
                popup.queue_free();
                if !dialog_for_select.is_instance_valid() {
                    return Variant::nil();
                }
                let mut handle = dialog_for_select.clone();
                let mut bound = handle.bind_mut();
                select_enum(&mut bound, &state_for_select, index, field_index, &selected);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&toggle);
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// Frees a transient popup when it hides.
fn connect_popup_free(popup: &Gd<PopupPanel>) {
    let popup_for_hide = popup.clone();
    let callable = Callable::from_fn("logic_popup_free", move |_args: &[&Variant]| {
        if popup_for_hide.is_instance_valid() {
            let mut popup = popup_for_hide.clone();
            popup.queue_free();
        }
        Variant::nil()
    });
    let mut popup = popup.clone();
    popup.connect("popup_hide", &callable);
}

/// `fieldAlignSelect` (`LStatement.java:259-268`): the nine align presets, set as
/// `@<name>` (`LStatement.afterRead` normalizes them).
fn open_align_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one transient align-select popup per open.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    popup.add_child(&grid);
    let current = current_field_value(state, index, field_index);
    let aligns = [
        "topLeft",
        "top",
        "topRight",
        "left",
        "center",
        "right",
        "bottomLeft",
        "bottom",
        "bottomRight",
    ];
    for align in aligns {
        // code-instantiated: one align toggle per preset.
        let mut toggle = Button::new_alloc();
        toggle.set_text(&gs(field_label_text(align)));
        toggle.set_toggle_mode(true);
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_pressed(current == format!("@{align}"));
        toggle.set_custom_minimum_size(Vector2::new(150.0, 40.0));
        {
            let dialog_for_select = dialog_gd.clone();
            let state_for_select = state.clone();
            let value = format!("@{align}");
            let popup_for_select = popup.clone();
            let callable = Callable::from_fn("logic_align", move |_args: &[&Variant]| {
                let mut popup = popup_for_select.clone();
                popup.hide();
                popup.queue_free();
                if !dialog_for_select.is_instance_valid() {
                    return Variant::nil();
                }
                let mut handle = dialog_for_select.clone();
                let mut bound = handle.bind_mut();
                select_enum(&mut bound, &state_for_select, index, field_index, &value);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&toggle);
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// `PrintCharStatement`'s pencil grid (`LStatements.java:290-311`).
fn open_char_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one transient character-picker popup per open.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(8);
    popup.add_child(&grid);
    for code in 32u32..127u32 {
        let Some(ch) = char::from_u32(code) else {
            continue;
        };
        // code-instantiated: one character toggle per printable code point.
        let mut toggle = Button::new_alloc();
        toggle.set_text(&gs(ch.to_string().as_str()));
        toggle.set_theme_type_variation("flatt");
        toggle.set_custom_minimum_size(Vector2::new(32.0, 32.0));
        {
            let dialog_for_select = dialog_gd.clone();
            let state_for_select = state.clone();
            let value = code.to_string();
            let popup_for_select = popup.clone();
            let callable = Callable::from_fn("logic_char", move |_args: &[&Variant]| {
                let mut popup = popup_for_select.clone();
                popup.hide();
                popup.queue_free();
                if !dialog_for_select.is_instance_valid() {
                    return Variant::nil();
                }
                let mut handle = dialog_for_select.clone();
                let mut bound = handle.bind_mut();
                select_enum(&mut bound, &state_for_select, index, field_index, &value);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&toggle);
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// A select changed: commit, apply the Java change fixups and rebuild the
/// element (`DrawStatement.rebuild`, `rebuild(table)` in each `build`).
fn select_enum(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    option: &str,
) {
    if !commit_field_value(state, option, index, field_index) {
        return;
    }
    let Some(mut logic) = kit::logic() else {
        return;
    };
    let (name, values) = {
        let st = state.borrow();
        let Some(row) = st.statements.get(index.max(0) as usize) else {
            return;
        };
        (row.name.clone(), row.fields.clone())
    };
    if logic.has_method("apply_type_change") {
        let changed = logic.call(
            "apply_type_change",
            &[
                gs(name.as_str()).to_variant(),
                packed_strings(&values).to_variant(),
            ],
        );
        if let Some(new_values) = packed_to_vec(&changed)
            && let Some(row) = state.borrow_mut().statements.get_mut(index.max(0) as usize)
        {
            row.fields = new_values;
        }
    }
    state.borrow_mut().modified = true;
    rebuild_list(dialog, state);
}

/// `col(table, x, res -> ...)`: the colour picker commits `%rrggbb[aa]`.
fn set_color(state: &Rc<RefCell<LogicState>>, color: Color, index: i64, field_index: i64) {
    let alpha = color.a >= 1.0;
    let mut packed = format!(
        "{:02x}{:02x}{:02x}",
        (color.r * 255.0) as i64,
        (color.g * 255.0) as i64,
        (color.b * 255.0) as i64
    );
    if !alpha {
        packed.push_str(&format!("{:02x}", (color.a * 255.0) as i64));
    }
    commit_field_value(state, &format!("%{packed}"), index, field_index);
}

fn parse_color(value: &str) -> Color {
    let text = value.strip_prefix('%').unwrap_or(value);
    if text.len() < 6 {
        return Color::WHITE;
    }
    let component = |range: std::ops::Range<usize>| -> u8 {
        text.get(range)
            .and_then(|slice| u8::from_str_radix(slice, 16).ok())
            .unwrap_or_default()
    };
    let r = component(0..2);
    let g = component(2..4);
    let b = component(4..6);
    let a = if text.len() >= 8 {
        component(6..8)
    } else {
        255
    };
    Color::from_rgba8(r, g, b, a)
}

/// `_unit_actions_enabled`: the `logicUnitBuild`/`logicUnitDeconstruct` rules
/// probe (`UnitControlStatement.build`).
fn unit_actions_enabled() -> bool {
    let Some(mut host) = kit::sim_host() else {
        return true;
    };
    if !host.has_method("logic_rules_probe") {
        return true;
    }
    let probe = host.call("logic_rules_probe", &[]);
    let text = probe
        .try_to::<GString>()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let Ok(dict) = Json::parse_string(&gs(text.as_str())).try_to::<VarDictionary>() else {
        return true;
    };
    dict_bool(&dict, "logicUnitBuild", true) || dict_bool(&dict, "logicUnitDeconstruct", true)
}

// ---------------------------------------------------------------------------
// jump nodes (`LCanvas.JumpButton`)
// ---------------------------------------------------------------------------

/// `LCanvas.JumpButton` (`LCanvas.java:551-624`): a `Tex.logicNode` image, white
/// at rest and `Pal.place` while hovered.
fn jump_button(dialog: &Gd<MindDialog>, state: &Rc<RefCell<LogicState>>, index: i64) -> Gd<Button> {
    // code-instantiated: one jump node per jump statement element.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(JUMP_BUTTON_SIZE, JUMP_BUTTON_SIZE));
    button.set_focus_mode(FocusMode::NONE);
    button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    for style_name in [
        "normal",
        "hover",
        "pressed",
        "disabled",
        "focus",
        "hover_pressed",
    ] {
        button.add_theme_stylebox_override(style_name, &StyleBoxEmpty::new_gd());
    }
    let needs_node_texture = state.borrow().node_texture.is_none();
    if needs_node_texture {
        let texture = kit::icon_texture("logic-node");
        state.borrow_mut().node_texture = texture;
    }
    let texture = state.borrow().node_texture.clone();
    if let Some(texture) = texture {
        button.set_button_icon(&texture);
        button.set_expand_icon(true);
        button.set_icon_alignment(HorizontalAlignment::CENTER);
    } else {
        button.set_text("o");
    }
    button.add_theme_color_override("icon_normal_color", Color::WHITE);
    button.add_theme_color_override("icon_hover_color", PLACE);
    button.add_theme_color_override("icon_pressed_color", PLACE);
    button.add_theme_color_override("icon_hover_pressed_color", PLACE);
    {
        let state_for_enter = state.clone();
        let callable = Callable::from_fn("logic_jump_over", move |_args: &[&Variant]| {
            state_for_enter.borrow_mut().jump_over = index;
            Variant::nil()
        });
        let mut node: Gd<Node> = button.clone().upcast();
        node.connect("mouse_entered", &callable);
    }
    {
        let state_for_exit = state.clone();
        let callable = Callable::from_fn("logic_jump_out", move |_args: &[&Variant]| {
            let mut st = state_for_exit.borrow_mut();
            if st.jump_over == index {
                st.jump_over = -1;
            }
            Variant::nil()
        });
        let mut node: Gd<Node> = button.clone().upcast();
        node.connect("mouse_exited", &callable);
    }
    state
        .borrow_mut()
        .jump_buttons
        .insert(index, button.clone());
    // `JumpButton`: press-drag-release onto a target element.
    {
        let dialog_for_input = dialog.clone();
        let state_for_input = state.clone();
        let button_for_input = button.clone();
        let callable = Callable::from_fn("logic_jump_input", move |args: &[&Variant]| {
            let Some(event) = args
                .first()
                .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
            else {
                return Variant::nil();
            };
            if !dialog_for_input.is_instance_valid() {
                return Variant::nil();
            }
            let mut handle = dialog_for_input.clone();
            let mut bound = handle.bind_mut();
            jump_button_input(
                &mut bound,
                &state_for_input,
                event,
                index,
                &button_for_input,
            );
            Variant::nil()
        });
        let mut node: Gd<Node> = button.clone().upcast();
        node.connect("gui_input", &callable);
    }
    button
}

/// `JumpButton`'s touch handlers (`LCanvas.java:570-600`): press clears the
/// target and starts selecting (the curve follows the pointer / hovered
/// element), release sets the hovered element (not the jump's own) or leaves the
/// target cleared.
fn jump_button_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    event: Gd<InputEvent>,
    index: i64,
    button: &Gd<Button>,
) {
    let mut button = button.clone();
    if let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>()
        && mouse.get_button_index() == MouseButton::LEFT
    {
        button.accept_event();
        let global = button.get_global_transform() * mouse.get_position();
        if mouse.is_pressed() {
            state.borrow_mut().jump_selecting = index;
            set_jump_target(state, index, -1);
            let hover = row_at(state, global);
            {
                let mut st = state.borrow_mut();
                st.jump_hover = hover;
                st.jump_pointer = global;
            }
            compute_jump_heights(state);
        } else {
            let hovered = row_at(state, global);
            if hovered >= 0 && hovered != index {
                set_jump_target(state, index, hovered);
            }
            {
                let mut st = state.borrow_mut();
                st.jump_selecting = -1;
                st.jump_hover = -1;
            }
            rebuild_list(dialog, state);
        }
        if let Some(mut overlay) = state.borrow().overlay.clone() {
            overlay.queue_redraw();
        }
        return;
    }
    let selecting = state.borrow().jump_selecting == index;
    if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
        && selecting
    {
        let pointer = button.get_global_transform() * motion.get_position();
        let hover = row_at(state, pointer);
        {
            let mut st = state.borrow_mut();
            st.jump_pointer = pointer;
            st.jump_hover = hover;
        }
        if let Some(mut overlay) = state.borrow().overlay.clone() {
            overlay.queue_redraw();
        }
        button.accept_event();
    }
}

/// The statement element under a global pointer position (`canvas.hovered`).
fn row_at(state: &Rc<RefCell<LogicState>>, global: Vector2) -> i64 {
    let (rows, drag_row) = {
        let st = state.borrow();
        (st.row_nodes.clone(), st.drag_row.clone())
    };
    for (index, row) in rows.iter().enumerate() {
        if let Some(drag) = &drag_row
            && drag.instance_id() == row.instance_id()
        {
            continue;
        }
        if row.is_instance_valid() && row.get_global_rect().contains_point(global) {
            return index as i64;
        }
    }
    -1
}

// ---------------------------------------------------------------------------
// drag reorder (`StatementElem` header + `DragLayout`)
// ---------------------------------------------------------------------------

/// `StatementElem`'s header `touchDown`/`touchDragged`/`touchUp`
/// (`LCanvas.java:456-494`): left-drag reorders (the drop lands at
/// `insertPosition`), middle-click copies.
fn header_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    event: Gd<InputEvent>,
    index: i64,
    header: &Gd<PanelContainer>,
) {
    let mut header = header.clone();
    if let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() {
        if mouse.get_button_index() == MouseButton::MIDDLE && mouse.is_pressed() {
            header.accept_event();
            let can_copy = state.borrow().statements.len() < MAX_INSTRUCTIONS;
            if can_copy {
                copy_statement(dialog, state, index);
            }
            return;
        }
        if mouse.get_button_index() == MouseButton::LEFT {
            if mouse.is_pressed() {
                {
                    let mut st = state.borrow_mut();
                    st.drag_index = index;
                    st.drag_active = false;
                    st.drag_start = mouse.get_position();
                }
                header.accept_event();
            } else {
                header.accept_event();
                let finish = state.borrow().drag_active;
                state.borrow_mut().drag_index = -1;
                if finish {
                    state.borrow_mut().drag_index = index;
                    finish_drag(dialog, state);
                    state.borrow_mut().drag_index = -1;
                }
                state.borrow_mut().drag_active = false;
                if let Some(mut overlay) = state.borrow().overlay.clone() {
                    overlay.queue_redraw();
                }
            }
            return;
        }
    }
    let drag_index = state.borrow().drag_index;
    if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
        && drag_index == index
    {
        let transform = header.get_global_transform();
        let global = transform * motion.get_position();
        let start_global = transform * state.borrow().drag_start;
        let drag_started = !state.borrow().drag_active && global.distance_to(start_global) > 4.0;
        if drag_started {
            begin_drag(dialog, state, index, global);
        }
        let drag_active = state.borrow().drag_active;
        if drag_active {
            // `touchDragged`: `translation.add(v - last)` keeps the element under
            // the pointer while `DragLayout` skips it in the normal layout.
            let (drag_row, overlay) = {
                let st = state.borrow();
                (st.drag_row.clone(), st.overlay.clone())
            };
            if let (Some(mut row), Some(overlay)) = (drag_row, overlay)
                && row.is_instance_valid()
            {
                let local = overlay.get_global_transform().affine_inverse() * global;
                let grab = state.borrow().drag_grab;
                row.set_position(local - grab);
            }
            let drop = drop_index_at(state, global);
            state.borrow_mut().drop_index = drop;
            if let Some(mut overlay) = state.borrow().overlay.clone() {
                overlay.queue_redraw();
            }
        }
        header.accept_event();
    }
}

/// `StatementElem.touchDown` + `touchDragged`: remove the element from the
/// `DragLayout` flow and translate it under the pointer.
fn begin_drag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    global: Vector2,
) {
    if index < 0 {
        return;
    }
    let (row, overlay, list) = {
        let st = state.borrow();
        if index as usize >= st.row_nodes.len() {
            return;
        }
        (
            st.row_nodes.get(index as usize).cloned(),
            st.overlay.clone(),
            st.list.clone(),
        )
    };
    let (Some(row), Some(overlay), Some(list)) = (row, overlay, list) else {
        return;
    };
    if !row.is_instance_valid() {
        return;
    }
    let overlay_local = overlay.get_global_transform().affine_inverse();
    let row_rect = row.get_global_rect();
    let grab = overlay_local * global - overlay_local * row_rect.position;
    let mut row = row;
    {
        let mut st = state.borrow_mut();
        st.drag_grab = grab;
        st.drag_row = Some(row.clone());
    }
    let mut list = list;
    list.remove_child(&row);
    let mut overlay_parent = overlay;
    overlay_parent.add_child(&row);
    row.set_position(overlay_local * row_rect.position);
    let size = row.get_size();
    row.set_size(Vector2::new(size.x.max(target_width(dialog)), size.y));
    state.borrow_mut().drag_active = true;
}

/// The per-statement drag insertion index (`DragLayout.layout`'s
/// `insertPosition`).
fn drop_index_at(state: &Rc<RefCell<LogicState>>, global: Vector2) -> i64 {
    let (rows, drag_row) = {
        let st = state.borrow();
        (st.row_nodes.clone(), st.drag_row.clone())
    };
    let mut insert_index = 0;
    for row in &rows {
        if let Some(drag) = &drag_row
            && drag.instance_id() == row.instance_id()
        {
            continue;
        }
        if !row.is_instance_valid() {
            continue;
        }
        let rect = row.get_global_rect();
        if global.y < rect.position.y + rect.size.y * 0.5 {
            return insert_index;
        }
        insert_index += 1;
    }
    insert_index
}

/// `LCanvas.DragLayout.finishLayout`: drop the dragged element at
/// `insertPosition` and remap every `jump` target like Java's element refs do
/// (`JumpStatement.saveUI` recomputes `indexOf(dest)`).
fn finish_drag(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let from = state.borrow().drag_index;
    let count = state.borrow().statements.len() as i64;
    if from < 0 || from >= count {
        return;
    }
    // `_drop_index_at` already excludes the dragged element, so the index is
    // the insertion position after removal (`DragLayout.insertPosition`).
    let to = state.borrow().drop_index.clamp(0, count - 1);
    if to != from {
        {
            let mut st = state.borrow_mut();
            for row in st.statements.iter_mut() {
                if row.name != "jump" {
                    continue;
                }
                let Some(slot) = row.fields.first_mut() else {
                    continue;
                };
                let Ok(mut dest) = slot.parse::<i64>() else {
                    continue;
                };
                if dest == from {
                    dest = to;
                } else if to >= from && from < dest && dest <= to {
                    dest -= 1;
                } else if to < from && to <= dest && dest < from {
                    dest += 1;
                }
                *slot = dest.to_string();
            }
            let moved = st.statements.remove(from as usize);
            st.statements.insert(to as usize, moved);
            st.modified = true;
        }
    }
    state.borrow_mut().drop_index = -1;
    rebuild_list(dialog, state);
}

// ---------------------------------------------------------------------------
// jump-curve lane prediction (`DragLayout.setJumpHeights`)
// ---------------------------------------------------------------------------

/// `DragLayout.setJumpHeights` (`LCanvas.java:274-321`): representative curves
/// (widest forward jump per end, widest backward jump per begin) get nested lane
/// heights; every curve then takes its representative's height.
fn compute_jump_heights(state: &Rc<RefCell<LogicState>>) {
    let statements = state.borrow().statements.clone();
    let selecting = state.borrow().jump_selecting;
    state.borrow_mut().pred_heights.clear();
    let mut curves: Vec<CurveData> = Vec::new();
    for (index, row) in statements.iter().enumerate() {
        if index as i64 == selecting {
            continue;
        }
        let dest = row.jump_dest(statements.len());
        if dest < 0 {
            continue;
        }
        curves.push(CurveData {
            jump: index as i64,
            begin: (index as i64).min(dest),
            end: (index as i64).max(dest),
            flipped: index as i64 >= dest,
            pred: 0,
            done: false,
        });
    }
    let mut repr_before: Vec<(i64, usize)> = Vec::new();
    let mut repr_after: Vec<(i64, usize)> = Vec::new();
    for (curve_index, curve) in curves.iter().enumerate() {
        if curve.flipped {
            if let Some((_, existing)) = repr_after.iter().find(|(begin, _)| *begin == curve.begin)
                && curves[*existing].end >= curve.end
            {
                continue;
            }
            if let Some(existing) = repr_after
                .iter_mut()
                .find(|(begin, _)| *begin == curve.begin)
            {
                existing.1 = curve_index;
            } else {
                repr_after.push((curve.begin, curve_index));
            }
        } else {
            if let Some((_, existing)) = repr_before.iter().find(|(end, _)| *end == curve.end)
                && curves[*existing].begin <= curve.begin
            {
                continue;
            }
            if let Some(existing) = repr_before.iter_mut().find(|(end, _)| *end == curve.end) {
                existing.1 = curve_index;
            } else {
                repr_before.push((curve.end, curve_index));
            }
        }
    }
    let mut processed: Vec<usize> = repr_before.iter().map(|(_, index)| *index).collect();
    processed.extend(repr_after.iter().map(|(_, index)| *index));
    let mut order: Vec<usize> = (0..processed.len()).collect();
    order.sort_by_key(|slot| {
        let curve = &curves[processed[*slot]];
        (curve.begin, *slot)
    });
    let processed: Vec<usize> = order.into_iter().map(|slot| processed[slot]).collect();
    let mut occupiers: Vec<usize> = Vec::new();
    let mut occupied: BTreeSet<i64> = BTreeSet::new();
    let mut scratch = HeightScratch::default();
    for index in 0..processed.len() {
        let cur = processed[index];
        let mut kept: Vec<usize> = Vec::new();
        let mut dropped: Vec<i64> = Vec::new();
        for occupier in occupiers.drain(..) {
            if curves[occupier].end > curves[cur].begin {
                kept.push(occupier);
            } else {
                dropped.push(curves[occupier].pred);
            }
        }
        for pred in dropped {
            occupied.remove(&pred);
        }
        occupiers = kept;
        let height = jump_height(
            &mut curves,
            &processed,
            index,
            Some((&occupiers, &occupied)),
            &mut scratch,
        );
        occupiers.push(cur);
        occupied.insert(height);
    }
    let mut pred_heights: BTreeMap<i64, i64> = BTreeMap::new();
    for curve in &curves {
        let representative = if curve.flipped {
            repr_after
                .iter()
                .find(|(begin, _)| *begin == curve.begin)
                .map(|(_, index)| *index)
        } else {
            repr_before
                .iter()
                .find(|(end, _)| *end == curve.end)
                .map(|(_, index)| *index)
        };
        if let Some(representative) = representative {
            pred_heights.insert(curve.jump, curves[representative].pred);
        }
    }
    state.borrow_mut().pred_heights = pred_heights;
}

/// `DragLayout.getJumpHeight` (`LCanvas.java:323-353`): the lowest free lane
/// above every jump nested inside this one. The scratch buffers are shared
/// across recursion levels exactly like Java's static
/// `tmpOccupiers2`/`tmpBits2`; `seed` is `Some` only for the top-level call
/// (whose `occupied` set is a frozen copy, not the scratch bits).
fn jump_height(
    curves: &mut [CurveData],
    processed: &[usize],
    index: usize,
    seed: Option<(&[usize], &BTreeSet<i64>)>,
    scratch: &mut HeightScratch,
) -> i64 {
    let jmp = processed[index];
    if curves[jmp].done {
        return curves[jmp].pred;
    }
    let frozen: Option<BTreeSet<i64>> = match seed {
        Some((occupiers, occupied)) => {
            scratch.occupiers = occupiers.to_vec();
            scratch.bits = occupied.clone();
            Some(occupied.clone())
        }
        None => None,
    };
    let mut max_height: i64 = -1;
    let end_limit = curves[jmp].end;
    let mut cursor = index + 1;
    while cursor < processed.len() {
        let cur = processed[cursor];
        if curves[cur].end <= end_limit {
            let mut kept: Vec<usize> = Vec::new();
            let mut dropped: Vec<i64> = Vec::new();
            for occupier in scratch.occupiers.drain(..) {
                if curves[occupier].end > curves[cur].begin {
                    kept.push(occupier);
                } else {
                    dropped.push(curves[occupier].pred);
                }
            }
            for pred in dropped {
                scratch.bits.remove(&pred);
            }
            scratch.occupiers = kept;
            let height = jump_height(curves, processed, cursor, None, scratch);
            scratch.occupiers.push(cur);
            scratch.bits.insert(height);
            max_height = max_height.max(height);
        }
        cursor += 1;
    }
    let mut lane = max_height + 1;
    loop {
        let used = match &frozen {
            Some(set) => set.contains(&lane),
            None => scratch.bits.contains(&lane),
        };
        if !used {
            break;
        }
        lane += 1;
    }
    curves[jmp].pred = lane;
    curves[jmp].done = true;
    scratch.occupiers.clear();
    lane
}

// ---------------------------------------------------------------------------
// overlay (`LCanvas.DragLayout.draw` + `JumpCurve.draw`)
// ---------------------------------------------------------------------------

/// `MindLogicOverlay` — the per-frame jump-curve / drop-line canvas above the
/// statement column (`LCanvas.DragLayout.draw`, `JumpCurve.act`).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindLogicOverlay {
    base: Base<Control>,
    state: Rc<RefCell<LogicState>>,
    dialog: Option<Gd<MindDialog>>,
}

#[godot_api]
impl IControl for MindLogicOverlay {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            state: Rc::new(RefCell::new(LogicState::default())),
            dialog: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        if !self.base().is_visible_in_tree() {
            return;
        }
        let state = self.state.clone();
        update_curve_heights(&state, delta, portrait_of(self));
        self.base_mut().queue_redraw();
        refresh_variables(&state);
    }

    fn draw(&mut self) {
        let state = self.state.clone();
        draw_overlay(self, &state);
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible_in_tree() {
            return;
        }
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        if !dialog.is_instance_valid() {
            return;
        }
        let Ok(key) = event.try_cast::<godot::classes::InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() {
            return;
        }
        let sub_open = self.state.borrow().sub_dialog.is_some();
        if key.get_keycode() == godot::global::Key::ESCAPE && sub_open {
            close_sub_dialog(&self.state, true);
            if let Some(mut viewport) = self.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        let enter = key.get_keycode() == godot::global::Key::ENTER
            || key.get_keycode() == godot::global::Key::KP_ENTER;
        let no_sub = self.state.borrow().sub_dialog.is_none();
        if enter && key.is_shift_pressed() && no_sub {
            let state = self.state.clone();
            let mut handle = dialog.clone();
            let mut bound = handle.bind_mut();
            if bound.is_top() {
                open_add_dialog(&mut bound, &state, -1);
                if let Some(mut viewport) = self.base().get_viewport() {
                    viewport.set_input_as_handled();
                }
            }
        }
    }
}

/// `Core.graphics.isPortrait()` (`get_viewport_rect().size.y > size.x`).
fn portrait_of(overlay: &MindLogicOverlay) -> bool {
    let size = overlay.base().get_viewport_rect().size;
    size.y > size.x
}

/// `JumpCurve.drawCurve`'s height smoothing:
/// `uiHeight = lerp(40 + 10 * predHeight, uiHeight, 0.9^delta)` (portrait:
/// `20 + 8 * predHeight`).
fn update_curve_heights(state: &Rc<RefCell<LogicState>>, delta: f64, portrait: bool) {
    let base = if portrait { 20.0 } else { 40.0 };
    let step = if portrait { 8.0 } else { 10.0 };
    let keep = 0.9_f32.powf((delta * 60.0) as f32);
    let jumps: Vec<i64> = state.borrow().jump_buttons.keys().copied().collect();
    let mut st = state.borrow_mut();
    for jump in jumps {
        let target = base + step * *st.pred_heights.get(&jump).unwrap_or(&0) as f32;
        let current = *st.curve_heights.get(&jump).unwrap_or(&60.0);
        st.curve_heights
            .insert(jump, target + (current - target) * keep);
    }
}

/// The translated overlay position of a global point.
fn to_overlay(overlay: &MindLogicOverlay, global: Vector2) -> Vector2 {
    overlay.base().get_global_transform().affine_inverse() * global
}

/// `LCanvas.DragLayout.draw` + `JumpCurve.draw`: the drop line while dragging
/// and one trapezoid per `jump` from its node to the target element's right
/// edge (`JumpCurve.act`: `t.set(hover.getWidth(), hover.getHeight() / 2f)`).
fn draw_overlay(overlay: &mut MindLogicOverlay, state: &Rc<RefCell<LogicState>>) {
    let (
        rows,
        drag_active,
        drop_index,
        jump_buttons,
        jump_over,
        statements,
        row_nodes,
        curve_heights,
        jump_selecting,
        jump_hover,
        jump_pointer,
    ) = {
        let st = state.borrow();
        (
            visible_rows(&st),
            st.drag_active,
            st.drop_index,
            st.jump_buttons.clone(),
            st.jump_over,
            st.statements.clone(),
            st.row_nodes.clone(),
            st.curve_heights.clone(),
            st.jump_selecting,
            st.jump_hover,
            st.jump_pointer,
        )
    };
    if drag_active && drop_index >= 0 && !rows.is_empty() {
        let first = rows[0].get_global_rect();
        let y = if drop_index <= 0 {
            first.position.y - 3.0
        } else if drop_index as usize >= rows.len() {
            let last = rows[rows.len() - 1].get_global_rect();
            last.end().y + 3.0
        } else {
            let above = rows[drop_index as usize - 1].get_global_rect();
            let below = rows[drop_index as usize].get_global_rect();
            (above.end().y + below.position.y) * 0.5
        };
        let left = first.position.x;
        let right = left + first.size.x;
        let from = to_overlay(overlay, Vector2::new(left, y));
        let to = to_overlay(overlay, Vector2::new(right, y));
        overlay
            .base_mut()
            .draw_line_ex(from, to, Color::from_rgba(1.0, 1.0, 1.0, 0.7))
            .width(3.0)
            .done();
    }
    let mut hovered_last: Option<i64> = None;
    for jump in jump_buttons.keys().copied() {
        // `if(button.listener.isOver()) toFront()`: the hovered curve draws last.
        if jump == jump_over {
            hovered_last = Some(jump);
            continue;
        }
        draw_jump(
            overlay,
            state,
            jump,
            &jump_buttons,
            &row_nodes,
            &statements,
            &curve_heights,
            jump_selecting,
            jump_hover,
            jump_pointer,
        );
    }
    if let Some(jump) = hovered_last {
        draw_jump(
            overlay,
            state,
            jump,
            &jump_buttons,
            &row_nodes,
            &statements,
            &curve_heights,
            jump_selecting,
            jump_hover,
            jump_pointer,
        );
    }
}

/// The rows currently in the normal layout (the dragged element is under the
/// overlay, not in `_list`).
fn visible_rows(st: &LogicState) -> Vec<Gd<Control>> {
    let mut rows = Vec::new();
    for row in &st.row_nodes {
        if let Some(drag) = &st.drag_row
            && drag.instance_id() == row.instance_id()
        {
            continue;
        }
        if row.is_instance_valid() {
            rows.push(row.clone());
        }
    }
    rows
}

/// One `JumpCurve.draw` (`LCanvas.java:678-723`).
#[allow(clippy::too_many_arguments)]
fn draw_jump(
    overlay: &mut MindLogicOverlay,
    state: &Rc<RefCell<LogicState>>,
    jump: i64,
    jump_buttons: &BTreeMap<i64, Gd<Button>>,
    row_nodes: &[Gd<Control>],
    statements: &[StatementRow],
    curve_heights: &BTreeMap<i64, f32>,
    jump_selecting: i64,
    jump_hover: i64,
    jump_pointer: Vector2,
) {
    let Some(button) = jump_buttons.get(&jump) else {
        return;
    };
    if !button.is_instance_valid() {
        return;
    }
    let src = to_overlay(overlay, button.get_global_rect().center());
    let mut dst = Vector2::ZERO;
    let target: i64;
    if jump == jump_selecting {
        target = jump_hover;
        if target < 0 {
            dst = to_overlay(overlay, jump_pointer);
        }
    } else if (jump as usize) < statements.len() {
        target = statements[jump as usize].jump_dest(statements.len());
        if target < 0 {
            return;
        }
    } else {
        return;
    }
    if target >= 0 {
        if target as usize >= row_nodes.len() {
            return;
        }
        let row = &row_nodes[target as usize];
        if !row.is_instance_valid() {
            return;
        }
        let rect = row.get_global_rect();
        dst = to_overlay(
            overlay,
            Vector2::new(rect.end().x, rect.position.y + rect.size.y * 0.5),
        );
    }
    if src.distance_squared_to(dst) < 0.0001 {
        return;
    }
    let color = if jump == state.borrow().jump_over {
        PLACE
    } else {
        Color::WHITE
    };
    let ui_height = *curve_heights.get(&jump).unwrap_or(&60.0);
    // Trapezoidal jumps (`drawCurve`): out by `uiHeight`, across, back in.
    let mut dy = 0.0_f32;
    if dst.y != src.y {
        dy = if dst.y > src.y { 1.0 } else { -1.0 } * ui_height * 0.5;
    }
    let corner_a = Vector2::new(src.x + ui_height, src.y + dy);
    let corner_b = Vector2::new(src.x + ui_height, dst.y - dy);
    let mut points = PackedVector2Array::new();
    if let Some(crossing) = segment_intersection(src, corner_a, dst, corner_b) {
        points.push(src);
        points.push(crossing);
        points.push(dst);
    } else {
        points.push(src);
        points.push(corner_a);
        points.push(corner_b);
        points.push(dst);
    }
    overlay
        .base_mut()
        .draw_polyline_ex(&points, color)
        .width(4.0)
        .done();
    // `Tex.logicNode.draw(t.x + s * 0.75f, t.y - s / 2f, -s, s)`: the mirrored
    // node at the target.
    let texture = state.borrow().node_texture.clone();
    let size = button.get_size();
    let s = if size.x > 0.0 {
        size.x
    } else {
        JUMP_BUTTON_SIZE
    };
    if let Some(texture) = texture {
        overlay
            .base_mut()
            .draw_set_transform_ex(Vector2::new(dst.x + s * 0.75, dst.y - s * 0.5))
            .rotation(0.0)
            .scale(Vector2::new(-1.0, 1.0))
            .done();
        overlay
            .base_mut()
            .draw_texture_rect_ex(
                &texture,
                Rect2::new(Vector2::ZERO, Vector2::new(s, s)),
                false,
            )
            .modulate(color)
            .done();
        overlay.base_mut().draw_set_transform(Vector2::ZERO);
    }
}

/// `Geometry2D.segment_intersects_segment`: the intersection point of two
/// segments, or `None` when they are parallel / do not cross.
fn segment_intersection(a1: Vector2, a2: Vector2, b1: Vector2, b2: Vector2) -> Option<Vector2> {
    let denominator = (b2.y - b1.y) * (a2.x - a1.x) - (b2.x - b1.x) * (a2.y - a1.y);
    if denominator.abs() < f32::EPSILON {
        return None;
    }
    let ua = ((b2.x - b1.x) * (a1.y - b1.y) - (b2.y - b1.y) * (a1.x - b1.x)) / denominator;
    let ub = ((a2.x - a1.x) * (a1.y - b1.y) - (a2.y - a1.y) * (a1.x - b1.x)) / denominator;
    if !(0.0..=1.0).contains(&ua) || !(0.0..=1.0).contains(&ub) {
        return None;
    }
    Some(Vector2::new(
        a1.x + ua * (a2.x - a1.x),
        a1.y + ua * (a2.y - a1.y),
    ))
}

// ---------------------------------------------------------------------------
// variables panel (`LogicDialog` `@variables` / `GlobalVarsDialog`)
// ---------------------------------------------------------------------------

/// `LogicDialog`'s `@variables` button (`:162-243`): the variables panel when
/// `shouldShowVariables()`, else the globals panel; the button label follows.
fn open_variables_panel(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    if !should_show_variables(state) {
        open_globals_panel(dialog, state);
        return;
    }
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    if !host.has_method("logic_get_state") {
        return;
    }
    let (x, y) = {
        let st = state.borrow();
        (st.x, st.y)
    };
    let state_value = host.call("logic_get_state", &[x.to_variant(), y.to_variant()]);
    let mut vars = VarArray::new();
    if let Ok(dict) = state_value.try_to::<VarDictionary>()
        && let Some(list) = dict.get("vars")
        && let Ok(array) = list.try_to::<VarArray>()
    {
        vars = array;
    }
    open_vars_panel(dialog, state, &vars);
}

/// The per-variable rows (`LogicDialog.java:183-247`): name (accent), the value
/// (140 wide, refreshed every 15 frames) and the type.
fn open_vars_panel(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>, vars: &VarArray) {
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the variables panel is transient and data-driven.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut column = VBoxContainer::new_alloc();
    column.set_custom_minimum_size(Vector2::new(420.0, 320.0));
    popup.add_child(&column);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    column.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&list);
    // code-instantiated: one row per executor variable (count is data-driven).
    let mut rows: Vec<(String, Gd<Label>)> = Vec::new();
    for entry in vars.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let mut row = HBoxContainer::new_alloc();
        let mut name_label = Label::new_alloc();
        name_label.set_text(&gs(format!(" {} ", dict_string(&entry, "name"))));
        name_label.add_theme_color_override("font_color", styles::ACCENT);
        row.add_child(&name_label);
        let mut value_label = Label::new_alloc();
        value_label.set_text(&gs(format_var_value(&entry)));
        value_label.set_custom_minimum_size(Vector2::new(140.0, 0.0));
        value_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        row.add_child(&value_label);
        let mut type_label = Label::new_alloc();
        type_label.set_text(&gs(format!(" {} ", dict_string(&entry, "type"))));
        type_label.set_custom_minimum_size(Vector2::new(120.0, 0.0));
        row.add_child(&type_label);
        list.add_child(&row);
        rows.push((dict_string(&entry, "name"), value_label));
    }
    let mut buttons_row = HBoxContainer::new_alloc();
    column.add_child(&buttons_row);
    let globals = factory::icon_button(
        "list",
        &bundle("@logic.globals"),
        "cleart",
        AlignmentMode::BEGIN,
    );
    {
        let dialog_for_globals = dialog_gd.clone();
        let state_for_globals = state.clone();
        let popup_for_globals = popup.clone();
        let callable = Callable::from_fn("logic_globals", move |_args: &[&Variant]| {
            let mut popup = popup_for_globals.clone();
            popup.hide();
            if dialog_for_globals.is_instance_valid() {
                let mut handle = dialog_for_globals.clone();
                let mut bound = handle.bind_mut();
                open_globals_panel(&mut bound, &state_for_globals);
            }
            Variant::nil()
        });
        let mut node: Gd<Node> = globals.clone().upcast();
        node.connect("pressed", &callable);
    }
    buttons_row.add_child(&globals);
    {
        let mut st = state.borrow_mut();
        st.vars_popup = Some(popup.clone());
        st.vars_rows = rows;
        st.vars_timer = 0;
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// `LogicDialog`'s value text: objects print through the sim, numbers as
/// integers within 0.00001 (`LogicDialog.java:206-215`).
fn format_var_value(entry: &VarDictionary) -> String {
    entry
        .get("value")
        .map(|value| value.stringify().to_string())
        .unwrap_or_default()
}

/// The globals panel (`GlobalVarsDialog`): `@name` + its `lglobal.<name>`
/// description.
fn open_globals_panel(dialog: &mut MindDialog, _state: &Rc<RefCell<LogicState>>) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("globals") {
        return;
    }
    // code-instantiated: the globals panel is transient and data-driven.
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    let mut column = VBoxContainer::new_alloc();
    column.set_custom_minimum_size(Vector2::new(520.0, 380.0));
    popup.add_child(&column);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    column.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&list);
    // code-instantiated: one row per built-in global (count is data-driven).
    let rows = logic.call("globals", &[]);
    if let Ok(rows) = rows.try_to::<VarArray>() {
        for entry in rows.iter_shared() {
            let Ok(entry) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let mut row = HBoxContainer::new_alloc();
            let mut name_label = Label::new_alloc();
            name_label.set_text(&gs(format!(" {} ", dict_string(&entry, "name"))));
            name_label.set_custom_minimum_size(Vector2::new(150.0, 0.0));
            row.add_child(&name_label);
            let mut desc_label = Label::new_alloc();
            desc_label.set_text(&gs(bundle(&dict_string(&entry, "description"))));
            desc_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
            desc_label.set_custom_minimum_size(Vector2::new(320.0, 0.0));
            row.add_child(&desc_label);
            list.add_child(&row);
        }
    }
    connect_popup_free(&popup);
    popup.popup_centered();
}

/// The 15-frame variables refresh while the panel is shown
/// (`LogicDialog.java:204-222`).
fn refresh_variables(state: &Rc<RefCell<LogicState>>) {
    let (visible, x, y) = {
        let st = state.borrow();
        (
            st.vars_popup
                .as_ref()
                .map(|popup| popup.is_instance_valid() && popup.is_visible())
                .unwrap_or(false),
            st.x,
            st.y,
        )
    };
    if !visible {
        let mut st = state.borrow_mut();
        if st
            .vars_popup
            .as_ref()
            .is_none_or(|popup| !popup.is_instance_valid())
        {
            st.vars_popup = None;
            st.vars_rows.clear();
        }
        return;
    }
    {
        let mut st = state.borrow_mut();
        st.vars_timer += 1;
        if st.vars_timer < 15 {
            return;
        }
        st.vars_timer = 0;
    }
    let Some(mut host) = kit::sim_host() else {
        return;
    };
    if !host.has_method("logic_get_state") {
        return;
    }
    let Ok(state_dict) = host
        .call("logic_get_state", &[x.to_variant(), y.to_variant()])
        .try_to::<VarDictionary>()
    else {
        return;
    };
    let Some(vars) = state_dict.get("vars") else {
        return;
    };
    let Ok(vars) = vars.try_to::<VarArray>() else {
        return;
    };
    for entry in vars.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let var_name = dict_string(&entry, "name");
        let value = format_var_value(&entry);
        let rows = state.borrow().vars_rows.clone();
        for (row_name, mut label) in rows {
            if row_name == var_name && label.is_instance_valid() {
                label.set_text(&gs(value.as_str()));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// edit menu (`LogicDialog`'s `@edit` / `@editor.export`)
// ---------------------------------------------------------------------------

/// `_open_edit_menu`: `BaseDialog("@editor.export")` with a `Tex.button` table
/// of `flatt` 280x60 rows (`@clear`, `@copy.clipboard`, `@load.clipboard`,
/// `@logic.restart`) and `addCloseButton`.
fn open_edit_menu(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let dialog_gd = dialog.to_gd();
    let parts = open_sub_dialog(dialog, state, "@editor.export");
    let cont: Gd<Container> = parts.cont.clone().upcast();
    // code-instantiated: the `Tex.button` table of the edit menu (one per open).
    let mut margin = MarginContainer::new_alloc();
    for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
        margin.add_theme_constant_override(side, 10);
    }
    add_any(&cont, &margin);
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &button_box(state));
    margin.add_child(&panel);
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 0);
    panel.add_child(&column);
    column.add_child(&edit_row(&dialog_gd, state, "cancel", "@clear", |d, s| {
        edit_clear(d, s);
    }));
    column.add_child(&edit_row(
        &dialog_gd,
        state,
        "copy",
        "@copy.clipboard",
        |d, s| {
            edit_copy(d, s);
        },
    ));
    let mut load_row = edit_row(&dialog_gd, state, "download", "@load.clipboard", |d, s| {
        edit_load(d, s);
    });
    load_row.set_disabled(DisplayServer::singleton().clipboard_get().is_empty());
    column.add_child(&load_row);
    column.add_child(&edit_row(
        &dialog_gd,
        state,
        "refresh",
        "@logic.restart",
        |d, s| {
            edit_restart(d, s);
        },
    ));
}

/// One `t.button(text, icon, Styles.flatt, ...).size(280f, 60f).left()
/// .marginLeft(12f)`.
fn edit_row<F>(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    icon: &str,
    key: &str,
    action: F,
) -> Gd<Button>
where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>) + 'static,
{
    // code-instantiated: the edit menu rows are built per open.
    let mut button = factory::icon_button(icon, &bundle(key), "flatt", AlignmentMode::BEGIN);
    button.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    button.add_theme_stylebox_override("normal", &flat_box_margins(Color::BLACK, 12.0));
    button.add_theme_stylebox_override("hover", &flat_box_margins(styles::FLAT_OVER, 12.0));
    button.add_theme_stylebox_override("pressed", &flat_box_margins(styles::FLAT_OVER, 12.0));
    on_pressed(&button, dialog, state, action);
    button
}

/// `@clear`: `ui.showConfirm("@logic.clear.confirm", canvas::clearStatements)`
/// then `dialog.hide()`.
fn edit_clear(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    close_sub_dialog(state, true);
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("show_confirm") {
        return;
    }
    let needs_confirm_binding = !state.borrow().confirm_connected;
    if ui.has_signal("confirm_result") && needs_confirm_binding {
        let dialog_gd = dialog.to_gd();
        let state_for_confirm = state.clone();
        let callable = Callable::from_fn("logic_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| value.try_to::<bool>().unwrap_or(false))
                .unwrap_or(false);
            if !state_for_confirm.borrow().pending_clear {
                return Variant::nil();
            }
            state_for_confirm.borrow_mut().pending_clear = false;
            if !confirmed {
                return Variant::nil();
            }
            // `LCanvas.clearStatements`.
            state_for_confirm.borrow_mut().statements.clear();
            state_for_confirm.borrow_mut().modified = true;
            if dialog_gd.is_instance_valid() {
                let mut handle = dialog_gd.clone();
                let mut bound = handle.bind_mut();
                rebuild_list(&mut bound, &state_for_confirm);
            }
            Variant::nil()
        });
        ui.connect("confirm_result", &callable);
        state.borrow_mut().confirm_connected = true;
    }
    state.borrow_mut().pending_clear = true;
    let _ = ui.call(
        "show_confirm",
        &[gs(bundle("@logic.clear.confirm")).to_variant()],
    );
}

/// `@copy.clipboard`: `Core.app.setClipboardText(canvas.save())`.
fn edit_copy(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let _ = dialog;
    close_sub_dialog(state, true);
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("write_statements") {
        return;
    }
    let rows = state.borrow().statements.clone();
    let write = rows_to_variant(&rows);
    let code = logic
        .call("write_statements", &[write.to_variant()])
        .try_to::<GString>()
        .unwrap_or_default();
    DisplayServer::singleton().clipboard_set(&code);
}

/// `@load.clipboard`: `canvas.load(Core.app.getClipboardText())`.
fn edit_load(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    close_sub_dialog(state, true);
    load_from_clipboard(dialog, state);
}

/// `@logic.restart`: `forceRestart = true; dialog.hide(); hide();` — the editor
/// closes and the consumer writes the program back even if unchanged.
fn edit_restart(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    state.borrow_mut().force_restart = true;
    close_sub_dialog(state, true);
    dialog.close();
}

/// `@load.clipboard`: parse the clipboard text into the statement list.
fn load_from_clipboard(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("parse_statements") {
        return;
    }
    let text = DisplayServer::singleton().clipboard_get();
    if text.to_string().trim().is_empty() {
        return;
    }
    let privileged = state.borrow().privileged;
    let parsed = logic.call(
        "parse_statements",
        &[text.to_variant(), privileged.to_variant()],
    );
    if let Ok(array) = parsed.try_to::<VarArray>() {
        let mut rows = rows_from_variant(array.to_variant());
        rows.truncate(MAX_INSTRUCTIONS);
        {
            let mut st = state.borrow_mut();
            st.statements = rows;
            st.modified = true;
        }
        rebuild_list(dialog, state);
    }
}

// ---------------------------------------------------------------------------
// add menu (`LogicDialog.showAddDialog`)
// ---------------------------------------------------------------------------

/// The nested sub-dialog shell: `black9` stage, accent title + 3 px rule, a
/// centred content cell and the button row with `addCloseButton()`.
struct SubDialog {
    cont: Gd<CenterContainer>,
}

/// `_open_sub_dialog`: a nested full-screen `BaseDialog` stacked over the editor.
fn open_sub_dialog(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    title_key: &str,
) -> SubDialog {
    close_sub_dialog(state, false);
    let dialog_gd = dialog.to_gd();
    // code-instantiated: a transient nested dialog (one per open), stacked
    // over the editor like Java's `new BaseDialog(...)`.
    let mut root = Control::new_alloc();
    root.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    root.set_mouse_filter(MouseFilter::STOP);
    dialog.base_mut().add_child(&root);
    let mut dim = ColorRect::new_alloc();
    dim.set_color(styles::BLACK_9);
    dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    dim.set_mouse_filter(MouseFilter::IGNORE);
    root.add_child(&dim);
    let mut layout = VBoxContainer::new_alloc();
    layout.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    layout.add_theme_constant_override("separation", 0);
    root.add_child(&layout);
    let mut title = Label::new_alloc();
    title.set_text(&gs(bundle(title_key)));
    title.set_horizontal_alignment(HorizontalAlignment::CENTER);
    title.add_theme_color_override("font_color", styles::ACCENT);
    layout.add_child(&title);
    let mut rule_margin = MarginContainer::new_alloc();
    for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"] {
        rule_margin.add_theme_constant_override(side, 4);
    }
    layout.add_child(&rule_margin);
    let mut rule = ColorRect::new_alloc();
    rule.set_color(styles::ACCENT);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    rule_margin.add_child(&rule);
    let mut center = CenterContainer::new_alloc();
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&center);
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(AlignmentMode::CENTER);
    layout.add_child(&row);
    let mut back = factory::icon_button("left", &bundle("@back"), "defaultt", AlignmentMode::BEGIN);
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    on_pressed(&back, &dialog_gd, state, |_d, s| {
        close_sub_dialog(s, true);
    });
    row.add_child(&back);
    let mut bottom = Control::new_alloc();
    bottom.set_custom_minimum_size(Vector2::new(0.0, 8.0));
    layout.add_child(&bottom);
    state.borrow_mut().sub_dialog = Some(root);
    SubDialog { cont: center }
}

/// Hides the nested dialog (`BaseDialog.hidden` plays `uiBack`).
fn close_sub_dialog(state: &Rc<RefCell<LogicState>>, play_sound: bool) {
    let sub = state.borrow_mut().sub_dialog.take();
    let Some(mut root) = sub else {
        return;
    };
    if root.is_instance_valid() {
        root.queue_free();
    }
    if play_sound
        && let Some(mut ui) = kit::ui()
        && ui.has_method("play_ui_sound")
    {
        let _ = ui.call("play_ui_sound", &[gs("uiBack").to_variant()]);
    }
}

/// `showAddDialog(position)` (`LogicDialog.java:260-347`): a nested
/// `BaseDialog("@add")` whose `Tex.button` table holds the search row
/// (`Icon.zoom` + field, `@players.search`) over a pane of category sections:
/// a header (`Pal.darkishGray` icon 15 px, name with the description tooltip,
/// 5 px rule) then 130x50 `flatt` buttons in the category colour, three per
/// row, each with its `lst.<key>` tooltip. Enter adds the first match.
fn open_add_dialog(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>, at_index: i64) {
    if state.borrow().statements.len() >= MAX_INSTRUCTIONS {
        return;
    }
    let entries = add_entries(state);
    let dialog_gd = dialog.to_gd();
    let parts = open_sub_dialog(dialog, state, "@add");
    let cont: Gd<Container> = parts.cont.clone().upcast();
    // code-instantiated: the `Tex.button` table of the add menu (one per open).
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &button_box(state));
    add_any(&cont, &panel);
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 4);
    panel.add_child(&column);
    // code-instantiated: the `Icon.zoom` search row of the add menu.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 8);
    column.add_child(&search_row);
    let glyph = factory::glyph_label("zoom", 22, Color::WHITE);
    search_row.add_child(&glyph);
    let mut search = factory::field(&bundle("@players.search"));
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    search_row.add_child(&search);
    // code-instantiated: the scrolling category list of the add menu.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    column.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", 0);
    scroll.add_child(&list);
    let matched: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let rebuild: Rc<dyn Fn()> = {
        let list = list.clone();
        let search = search.clone();
        let entries = entries.clone();
        let matched = matched.clone();
        let dialog_gd = dialog_gd.clone();
        let state = state.clone();
        Rc::new(move || {
            rebuild_add_list(
                &list,
                &search,
                &entries,
                &matched,
                &dialog_gd,
                &state,
                at_index,
                &search_row,
            );
        })
    };
    {
        let rebuild = rebuild.clone();
        let callable = Callable::from_fn("logic_add_filter", move |_args: &[&Variant]| {
            rebuild();
            Variant::nil()
        });
        search.connect("text_changed", &callable);
    }
    {
        let matched = matched.clone();
        let dialog_gd = dialog_gd.clone();
        let state = state.clone();
        let callable = Callable::from_fn("logic_add_enter", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| value.stringify().to_string())
                .unwrap_or_default();
            let name = matched.borrow().clone();
            if !text.is_empty() && !name.is_empty() && dialog_gd.is_instance_valid() {
                let mut handle = dialog_gd.clone();
                let mut bound = handle.bind_mut();
                add_statement(&mut bound, &state, &name, at_index);
            }
            Variant::nil()
        });
        search.connect("text_submitted", &callable);
    }
    rebuild();
    let mut search = search;
    search.call_deferred("grab_focus", &[]);
}

/// The filtered category list rebuild.
#[allow(clippy::too_many_arguments)]
fn rebuild_add_list(
    list: &Gd<VBoxContainer>,
    search: &Gd<LineEdit>,
    entries: &[AddEntry],
    matched: &Rc<RefCell<String>>,
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    at_index: i64,
    search_row: &Gd<HBoxContainer>,
) {
    let mut list = list.clone();
    for child in list.get_children().iter_shared() {
        let mut child = child.clone();
        list.remove_child(&child);
        child.queue_free();
    }
    let text = search.get_text().to_string().to_lowercase();
    let mut first_match: Option<String> = None;
    let mut sections: Vec<(String, Gd<GridContainer>)> = Vec::new();
    for entry in entries {
        if !text.is_empty()
            && !entry.label.to_lowercase().contains(&text)
            && !entry.type_name.to_lowercase().contains(&text)
        {
            continue;
        }
        if first_match.is_none() {
            first_match = Some(entry.name.clone());
        }
        let existing = sections
            .iter()
            .find(|(category, _)| category == &entry.category)
            .map(|(_, grid)| grid.clone());
        let grid = match existing {
            Some(grid) => grid,
            None => {
                let header = add_category_header(&entry.category);
                list.add_child(&header);
                // code-instantiated: one button grid per listed category.
                let mut grid = GridContainer::new_alloc();
                grid.set_columns(3);
                grid.add_theme_constant_override("h_separation", 0);
                grid.add_theme_constant_override("v_separation", 0);
                list.add_child(&grid);
                sections.push((entry.category.clone(), grid.clone()));
                grid
            }
        };
        let button = add_statement_button(dialog, state, entry, at_index);
        let mut grid = grid;
        grid.add_child(&button);
    }
    *matched.borrow_mut() = first_match.unwrap_or_default();
    // `.maxHeight(Core.graphics.getHeight() * 0.8f)` on the content table.
    let scroll_target = list
        .get_parent()
        .and_then(|node| node.try_cast::<ScrollContainer>().ok());
    let dialog_for_fit = dialog.clone();
    let search_for_fit = search_row.clone();
    defer_action("logic_fit_add_scroll", move || {
        if let Some(scroll) = scroll_target {
            fit_add_scroll(&scroll, &list, &search_for_fit, &dialog_for_fit);
        }
    });
}

/// The pane takes the list's size up to 80% of the screen
/// (`.maxHeight(Core.graphics.getHeight() * 0.8f)`).
fn fit_add_scroll(
    scroll: &Gd<ScrollContainer>,
    list: &Gd<VBoxContainer>,
    search_row: &Gd<HBoxContainer>,
    dialog: &Gd<MindDialog>,
) {
    if !scroll.is_instance_valid() || !list.is_instance_valid() {
        return;
    }
    let wanted = list.get_combined_minimum_size();
    let mut cap = dialog.get_viewport_rect().size.y * 0.8 - 30.0;
    if search_row.is_instance_valid() {
        cap -= search_row.get_size().y + 4.0;
    }
    let height = wanted.y.min(cap.max(100.0));
    let mut width = wanted.x;
    if wanted.y > height {
        width += scroll
            .get_v_scroll_bar()
            .map(|bar| bar.get_combined_minimum_size().x)
            .unwrap_or_default();
    }
    let mut scroll = scroll.clone();
    scroll.set_custom_minimum_size(Vector2::new(width, height));
}

/// The category header row (`LogicDialog.java:313-319`).
fn add_category_header(category: &str) -> Gd<Control> {
    // code-instantiated: one header per listed category.
    let mut margin = MarginContainer::new_alloc();
    margin.add_theme_constant_override("margin_left", 5);
    margin.add_theme_constant_override("margin_right", 5);
    margin.add_theme_constant_override("margin_top", 10);
    margin.add_theme_constant_override("margin_bottom", 5);
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 0);
    margin.add_child(&row);
    let icon = category_icon(category);
    if !icon.is_empty() {
        let mut glyph = factory::glyph_label(icon, 15, DARKISH_GRAY);
        glyph.set_custom_minimum_size(Vector2::new(15.0, 15.0));
        row.add_child(&glyph);
        row.add_child(&factory::space(10.0, 0.0));
    }
    let mut title = Label::new_alloc();
    title.set_text(&gs(bundle(&format!("lcategory.{category}"))));
    title.add_theme_color_override("font_color", DARKISH_GRAY);
    title.set_mouse_filter(MouseFilter::STOP);
    title.set_tooltip_text(&gs(plain_markup(&bundle(&format!(
        "lcategory.{category}.description"
    )))));
    row.add_child(&title);
    row.add_child(&factory::space(10.0, 0.0));
    let mut line = ColorRect::new_alloc();
    line.set_color(DARKISH_GRAY);
    line.set_custom_minimum_size(Vector2::new(0.0, 5.0));
    line.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    line.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&line);
    margin.upcast()
}

/// One `cat.button(example.localizedName(), style, ...).size(130f, 50f)` with
/// `style = flatt` + `fontColor = category.color`, `font = Fonts.outline`.
fn add_statement_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    entry: &AddEntry,
    at_index: i64,
) -> Gd<Button> {
    // code-instantiated: one button per listed statement.
    let mut button = Button::new_alloc();
    button.set_text(&gs(entry.label.as_str()));
    button.set_custom_minimum_size(Vector2::new(130.0, 50.0));
    button.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_theme_type_variation("flatt");
    button.add_theme_stylebox_override("normal", &flat_box(Color::BLACK));
    button.add_theme_stylebox_override("hover", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("pressed", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("hover_pressed", &flat_box(styles::FLAT_OVER));
    for state_name in [
        "font_color",
        "font_hover_color",
        "font_pressed_color",
        "font_focus_color",
        "font_hover_pressed_color",
    ] {
        button.add_theme_color_override(state_name, entry.color);
    }
    button.add_theme_color_override("font_outline_color", Color::BLACK);
    button.add_theme_constant_override("outline_size", 4);
    button.set_tooltip_text(&gs(entry.tooltip.as_str()));
    let name = entry.name.clone();
    on_pressed(&button, dialog, state, move |d, s| {
        add_statement(d, s, &name, at_index);
    });
    button
}

/// Candidate statements for the add dialog in `LogicIO.allStatements` order,
/// after the Java filters (`LogicDialog.java:300-304`).
fn add_entries(state: &Rc<RefCell<LogicState>>) -> Vec<AddEntry> {
    let mut out: Vec<AddEntry> = Vec::new();
    let Some(mut logic) = kit::logic() else {
        return out;
    };
    if !logic.has_method("statement_names") {
        return out;
    }
    let (privileged, editor, filter) = {
        let st = state.borrow();
        (st.privileged, st.editor_mode, st.filter_mode)
    };
    let mut unit_allowed = true;
    if let Some(mut host) = kit::sim_host()
        && !editor
        && !filter
        && host.has_method("logic_rules_probe")
    {
        let probe = host.call("logic_rules_probe", &[]);
        let text = probe
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if let Ok(dict) = Json::parse_string(&gs(text.as_str())).try_to::<VarDictionary>() {
            unit_allowed = dict_bool(&dict, "logicUnitControl", true);
        }
    }
    let names = logic
        .call("statement_names", &[])
        .try_to::<PackedStringArray>()
        .unwrap_or_default();
    for name_value in names.as_slice() {
        let statement_name = name_value.to_string();
        let info = meta(state, &statement_name);
        if info.is_empty() {
            continue;
        }
        if type_name(state, &statement_name) == "Invalid" {
            continue;
        }
        if dict_bool(&info, "hidden", false) {
            continue;
        }
        if dict_bool(&info, "privileged", false) && !privileged {
            continue;
        }
        if dict_bool(&info, "non_privileged", false) && privileged {
            continue;
        }
        let category = {
            let value = dict_string(&info, "category");
            if value.is_empty() {
                "unknown".to_string()
            } else {
                value
            }
        };
        if category == "unit" && !privileged && !unit_allowed {
            continue;
        }
        out.push(AddEntry {
            name: statement_name.clone(),
            category: category.clone(),
            color: category_color(&category),
            label: localized_name(state, &statement_name),
            type_name: type_name(state, &statement_name),
            tooltip: bundle_tooltip(&format!("lst.{}", statement_key(state, &statement_name))),
        });
    }
    out
}

/// Adds a default instance of `name` at `at_index` (`-1` appends):
/// `canvas.addAt(position == -1 ? size : position, prov.get())`, then
/// `dialog.hide()`.
fn add_statement(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    statement_name: &str,
    at_index: i64,
) {
    if state.borrow().statements.len() >= MAX_INSTRUCTIONS {
        return;
    }
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("default_statement_row") {
        return;
    }
    let value = logic.call("default_statement_row", &[gs(statement_name).to_variant()]);
    let Ok(dict) = value.try_to::<VarDictionary>() else {
        return;
    };
    if dict.is_empty() {
        return;
    }
    let row = row_from_dict(&dict);
    let count = state.borrow().statements.len() as i64;
    let at = if at_index < 0 {
        count
    } else {
        at_index.clamp(0, count)
    };
    shift_jumps_for_insert(state, at);
    state.borrow_mut().statements.insert(at as usize, row);
    state.borrow_mut().modified = true;
    close_sub_dialog(state, true);
    rebuild_list(dialog, state);
}

// ---------------------------------------------------------------------------
// load / save (`LogicDialog.show` / `LCanvas.save` / `LogicBuild`)
// ---------------------------------------------------------------------------

/// `LogicDialog.show`: loads the processor's program into the statement list.
fn show_loaded(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let (x, y) = (dialog.context_int("x"), dialog.context_int("y"));
    let editor_mode = dialog
        .context_value("editor")
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    let filter_mode = dialog
        .context_value("filter")
        .map(|value| value.try_to::<bool>().unwrap_or(false))
        .unwrap_or(false);
    {
        let mut st = state.borrow_mut();
        st.x = x;
        st.y = y;
        st.editor_mode = editor_mode;
        st.filter_mode = filter_mode;
        st.force_restart = false;
    }
    close_sub_dialog(state, false);
    on_resize(dialog, state);
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if filter_mode {
        let privileged = dialog
            .context_value("privileged")
            .map(|value| value.try_to::<bool>().unwrap_or(true))
            .unwrap_or(true);
        let code = dialog.context_string("code").to_string();
        {
            let mut st = state.borrow_mut();
            st.privileged = privileged;
            st.code = code;
        }
    } else if editor_mode {
        let Some(mut editor) = kit::editor() else {
            return;
        };
        if !editor.has_method("logic_get_state") {
            return;
        }
        let value = editor.call("logic_get_state", &[x.to_variant(), y.to_variant()]);
        let (privileged, code) = if let Ok(dict) = value.try_to::<VarDictionary>() {
            (
                dict_bool(&dict, "privileged", false),
                dict_string(&dict, "code"),
            )
        } else {
            (false, String::new())
        };
        {
            let mut st = state.borrow_mut();
            st.privileged = privileged;
            st.code = code;
        }
    } else {
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("logic_get_state") || !host.has_method("logic_get_code") {
            return;
        }
        let value = host.call("logic_get_state", &[x.to_variant(), y.to_variant()]);
        let privileged = value
            .try_to::<VarDictionary>()
            .map(|dict| dict_bool(&dict, "privileged", false))
            .unwrap_or(false);
        let code = host
            .call("logic_get_code", &[x.to_variant(), y.to_variant()])
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        {
            let mut st = state.borrow_mut();
            st.privileged = privileged;
            st.code = code;
        }
    }
    if !logic.has_method("parse_statements") {
        return;
    }
    let (code, privileged) = {
        let st = state.borrow();
        (st.code.clone(), st.privileged)
    };
    let parsed = logic.call(
        "parse_statements",
        &[gs(code.as_str()).to_variant(), privileged.to_variant()],
    );
    let mut rows = rows_from_variant(parsed);
    // `LCanvas.load`: `statements.truncate(LExecutor.maxInstructions)`.
    rows.truncate(MAX_INSTRUCTIONS);
    {
        let mut st = state.borrow_mut();
        st.statements = rows;
        st.modified = false;
        st.curve_heights.clear();
    }
    if let Some(mut scroll) = state.borrow().scroll.clone() {
        scroll.set_v_scroll(0);
    }
    rebuild_list(dialog, state);
    update_buttons(state);
}

/// `LCanvas.save` -> `LogicBuild.showEditDialog`'s consumer.
fn save(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("write_statements") {
        return;
    }
    let rows = state.borrow().statements.clone();
    let code = logic
        .call("write_statements", &[rows_to_variant(&rows).to_variant()])
        .try_to::<GString>()
        .map(|value| value.to_string())
        .unwrap_or_default();
    state.borrow_mut().force_restart = false;
    let (filter_mode, editor_mode, x, y) = {
        let st = state.borrow();
        (st.filter_mode, st.editor_mode, st.x, st.y)
    };
    if filter_mode {
        // `modified.get(result)`: the generate dialog owns the filter JSON.
        {
            let mut st = state.borrow_mut();
            st.code = code.clone();
            st.modified = false;
        }
        let mut node = dialog.to_gd();
        node.emit_signal("code_edited", &[gs(code.as_str()).to_variant()]);
        return;
    }
    let error = if editor_mode {
        let Some(mut editor) = kit::editor() else {
            return;
        };
        if !editor.has_method("logic_configure_code") {
            return;
        }
        editor
            .call(
                "logic_configure_code",
                &[
                    x.to_variant(),
                    y.to_variant(),
                    gs(code.as_str()).to_variant(),
                ],
            )
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default()
    } else {
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("logic_configure_code") {
            return;
        }
        host.call(
            "logic_configure_code",
            &[
                x.to_variant(),
                y.to_variant(),
                gs(code.as_str()).to_variant(),
            ],
        )
        .try_to::<GString>()
        .map(|value| value.to_string())
        .unwrap_or_default()
    };
    if error.is_empty() {
        let mut st = state.borrow_mut();
        st.code = code;
        st.modified = false;
    } else if let Some(length) = error.strip_prefix("toolong:") {
        show_toolong(length.trim().parse::<i64>().unwrap_or_default());
    }
}

fn show_toolong(length: i64) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_method("toast") {
        return;
    }
    let mut text = "too long".to_string();
    if let Some(assets) = kit::assets() {
        let mut args = PackedStringArray::new();
        args.push(&gs("16000"));
        args.push(&gs(length.to_string().as_str()));
        text = assets
            .bind()
            .bundle_format(gs("logic.error.toolong"), args)
            .to_string();
    }
    let _ = ui.call(
        "toast",
        &[gs(text.as_str()).to_variant(), gs("error").to_variant()],
    );
}

// ---------------------------------------------------------------------------
// content pickers (`showSelectTable`)
// ---------------------------------------------------------------------------

/// A content picker callback (`on_pick(entry_name)` at each `showSelectTable`
/// call site).
type PickFn = Rc<dyn Fn(&str)>;

/// `showSelectTable`: a transient `PaneSolid`-style popup anchored over the
/// editor. Godot's `PopupPanel` owns the click-outside dismissal.
fn picker_popup(dialog: &mut MindDialog) -> (Gd<PopupPanel>, Gd<VBoxContainer>) {
    // code-instantiated: one transient picker per open (`showSelectTable`).
    let mut popup = PopupPanel::new_alloc();
    dialog.base_mut().add_child(&popup);
    connect_popup_free(&popup);
    let mut root = VBoxContainer::new_alloc();
    root.add_theme_constant_override("separation", 2);
    popup.add_child(&root);
    (popup, root)
}

/// Commits one picker selection and rebuilds the element (`build(table)` after
/// every content selection in `LStatements.java`).
fn picker_handler(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    popup: &Gd<PopupPanel>,
    index: i64,
    field_index: i64,
    prefix: &'static str,
) -> PickFn {
    let dialog = dialog.clone();
    let state = state.clone();
    let popup = popup.clone();
    Rc::new(move |value: &str| {
        if popup.is_instance_valid() {
            let mut popup = popup.clone();
            popup.hide();
        }
        commit_field_value(&state, &format!("{prefix}{value}"), index, field_index);
        if dialog.is_instance_valid() {
            let mut handle = dialog.clone();
            let mut bound = handle.bind_mut();
            rebuild_list(&mut bound, &state);
        }
    })
}

/// `Picker` rows from the live content snapshot (`MindSimHost.logic_picker_json`
/// `{name, hidden, unlocked_now, ...}`); falls back to `content_list` when the
/// extension predates the picker seam.
fn picker_rows(kind: &str) -> Vec<VarDictionary> {
    let Some(mut host) = kit::sim_host() else {
        return Vec::new();
    };
    if host.has_method("logic_picker_json") {
        let text = host
            .call("logic_picker_json", &[gs(kind).to_variant()])
            .try_to::<GString>()
            .unwrap_or_default();
        let parsed = Json::parse_string(&text);
        let Ok(array) = parsed.try_to::<VarArray>() else {
            return Vec::new();
        };
        return array
            .iter_shared()
            .filter_map(|entry| entry.try_to::<VarDictionary>().ok())
            .collect();
    }
    if host.has_method("content_list") {
        let names = host.call("content_list", &[gs(kind).to_variant()]);
        let Ok(names) = names.try_to::<PackedStringArray>() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for name in names.as_slice() {
            let mut row = VarDictionary::new();
            row.set("name", &gs(name.to_string()).to_variant());
            row.set("unlocked_now", true);
            rows.push(row);
        }
        return rows;
    }
    Vec::new()
}

/// Java content filters at each `showSelectTable` call site.
fn picker_visible(row: &VarDictionary, filter: &str) -> bool {
    let entry_name = dict_string(row, "name");
    if entry_name.is_empty() {
        return false;
    }
    let is_hidden = dict_bool(row, "hidden", false);
    let unlocked_now = dict_bool(row, "unlocked_now", true);
    match filter {
        "unit-control" => unlocked_now && !is_hidden && dict_bool(row, "logic_controllable", true),
        "unit" | "block" | "item" => unlocked_now && !is_hidden,
        "item-prop" => !is_hidden,
        "item-ore" => unlocked_now,
        "liquid-prop" => unlocked_now && !is_hidden,
        "status" => entry_name != "none",
        "status-prop" => entry_name != "none" && unlocked_now && dict_bool(row, "show", true),
        _ => true,
    }
}

fn content_label(kind: &str, entry_name: &str) -> String {
    if kind == "weather" {
        return field_label_text(entry_name);
    }
    if kind == "laccess" {
        let access_key = format!("laccess.label.{}", entry_name.to_lowercase());
        let access_text = bundle(&access_key);
        return if access_text == access_key {
            entry_name.to_string()
        } else {
            access_text
        };
    }
    let key = format!("{kind}.{entry_name}.name");
    let label = bundle(&key);
    if label == key {
        entry_name.to_string()
    } else {
        label
    }
}

fn content_texture(kind: &str, entry_name: &str) -> Option<Gd<Texture2D>> {
    let mut texture = kit::icon_texture(&format!("{kind}-{entry_name}-ui"));
    if texture.is_none() && kind == "block" {
        texture = kit::icon_texture(&format!("block-{entry_name}-full"));
    }
    texture
}

/// `t.button(new TextureRegionDrawable(item.uiIcon), Styles.flati, iconSmall,
/// ...).size(40f)` (`LStatements.java:544-552`, 6 per row).
fn icon_grid(
    kind: &str,
    filter: &str,
    on_pick: &PickFn,
    columns: i32,
    cell: f32,
    height: f32,
) -> Gd<Control> {
    // code-instantiated: the scrolling content grid of one picker.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(300.0, height));
    // code-instantiated: the content grid of one picker (cell count is data-driven).
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(columns);
    scroll.add_child(&grid);
    let mut count = 0;
    for row in picker_rows(kind) {
        if !picker_visible(&row, filter) {
            continue;
        }
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one content cell per picker entry.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flati");
        button.set_custom_minimum_size(Vector2::new(cell, cell));
        if let Some(texture) = content_texture(kind, &entry_name) {
            // code-instantiated: the content icon inside a picker cell.
            let mut image = TextureRect::new_alloc();
            image.set_texture(&texture);
            image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            image.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&image);
        } else {
            let label = content_label(kind, &entry_name);
            let first = label
                .chars()
                .next()
                .map(|ch| ch.to_uppercase().to_string())
                .unwrap_or_default();
            button.set_text(&gs(first));
        }
        button.set_tooltip_text(&gs(content_label(kind, &entry_name)));
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&button);
        count += 1;
    }
    if count == 0 {
        grid.add_child(&factory::label(&bundle("@none.found")));
    }
    scroll.upcast()
}

/// `showSelectTable` icon grid (ubind units, ore items).
fn open_icon_grid(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    kind: &str,
    filter: &str,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, root) = picker_popup(dialog);
    let prefix: &'static str = if kind == "status" { "@status-" } else { "@" };
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, prefix);
    let grid = icon_grid(kind, filter, &on_pick, 6, 40.0, 300.0);
    let root: Gd<Container> = root.upcast();
    add_any(&root, &grid);
    popup.popup_centered();
}

/// `ApplyStatusStatement`'s 240x40 status rows (`LStatements.java:1450-1461`).
fn open_status_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@status-");
    // code-instantiated: the status list pane of the picker.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(260.0, 360.0));
    root.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    scroll.add_child(&list);
    for row in picker_rows("status") {
        if !picker_visible(&row, "status") {
            continue;
        }
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one status row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(content_label("status", &entry_name)));
        if let Some(texture) = content_texture("status", &entry_name) {
            button.set_button_icon(&texture);
        }
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick_status", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    if list.get_child_count() == 0 {
        list.add_child(&factory::label(&bundle("@none.found")));
    }
    popup.popup_centered();
}

/// `WeatherSense`/`WeatherSet`'s two-column weather list
/// (`LStatements.java:1512-1531`, `:1560-1579`).
fn open_weather_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@");
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(2);
    root.add_child(&grid);
    for row in picker_rows("weather") {
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one weather row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(210.0, 40.0));
        button.set_text(&gs(content_label("weather", &entry_name)));
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick_weather", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        grid.add_child(&button);
    }
    if grid.get_child_count() == 0 {
        grid.add_child(&factory::label(&bundle("@none.found")));
    }
    popup.popup_centered();
}

/// `SetMarkerStatement`'s draw-layer list (`LStatements.java:2415-2428`):
/// every `Layer` field as `name = value` (`Reflect.get` + `Float.toString`).
fn open_layer_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let Some(mut logic) = kit::logic() else {
        return;
    };
    if !logic.has_method("marker_layers") {
        return;
    }
    let rows = logic.call("marker_layers", &[]);
    let Ok(rows) = rows.try_to::<VarArray>() else {
        return;
    };
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "");
    let mut list = VBoxContainer::new_alloc();
    root.add_child(&list);
    for row in rows.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        let layer_name = dict_string(&row, "name");
        let value = godot_num1(dict_f64(&row, "value", 0.0));
        // code-instantiated: one layer row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("logicTogglet");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(format!("{layer_name} = {value}")));
        {
            let pick = on_pick.clone();
            let value = value.clone();
            let callable = Callable::from_fn("logic_pick_layer", move |_args: &[&Variant]| {
                pick(&value);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    popup.popup_centered();
}

/// `String.num(value, 1)`: one decimal, trailing zeros trimmed.
fn godot_num1(value: f64) -> String {
    let text = format!("{value:.1}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        text
    } else {
        trimmed.to_string()
    }
}

/// A `squareTogglei` tab row over a `Stack` of content tables
/// (`LStatements.java:2160-2176` sensor, `:2110-2125` setprop).
fn picker_tabs(root: &Gd<VBoxContainer>, icons: &[&str], tables: Vec<Gd<Control>>) {
    let mut root = root.clone();
    // code-instantiated: the `squareTogglei` tab row and its page stack.
    let mut tabs = HBoxContainer::new_alloc();
    tabs.add_theme_constant_override("separation", 0);
    root.add_child(&tabs);
    let mut holder = VBoxContainer::new_alloc();
    holder.set_v_size_flags(SizeFlags::EXPAND_FILL);
    root.add_child(&holder);
    let mut pages: Vec<Gd<Control>> = Vec::new();
    for _ in icons {
        // code-instantiated: one tab page per picker tab.
        let mut page = Control::new_alloc();
        page.set_h_size_flags(SizeFlags::EXPAND_FILL);
        page.set_v_size_flags(SizeFlags::EXPAND_FILL);
        pages.push(page);
    }
    for (index, page) in pages.iter().enumerate() {
        let mut page = page.clone();
        page.set_visible(index == 0);
        holder.add_child(&page);
    }
    let pages: Rc<Vec<Gd<Control>>> = Rc::new(pages);
    for (index, icon) in icons.iter().enumerate() {
        // code-instantiated: one square toggle per picker tab.
        let mut toggle = factory::glyph_button(icon, "", "squareTogglei");
        toggle.set_custom_minimum_size(Vector2::new(0.0, 50.0));
        toggle.set_h_size_flags(SizeFlags::EXPAND_FILL);
        toggle.set_toggle_mode(true);
        toggle.set_pressed(index == 0);
        {
            let pages_for_tab = pages.clone();
            let tabs_for_tab = tabs.clone();
            let callable = Callable::from_fn("logic_picker_tab", move |_args: &[&Variant]| {
                for (page_index, page) in pages_for_tab.iter().enumerate() {
                    let mut page = page.clone();
                    page.set_visible(page_index == index);
                }
                for (tab_index, child) in tabs_for_tab.get_children().iter_shared().enumerate() {
                    if let Ok(mut button) = child.try_cast::<Button>() {
                        button.set_pressed(tab_index == index);
                    }
                }
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        tabs.add_child(&toggle);
        if let (Some(table), Some(page)) = (tables.get(index), pages.get(index)) {
            let mut page = page.clone();
            page.add_child(table);
        }
    }
}

/// `SetPropStatement`'s item/liquid/status/sensor tabs (`LStatements.java`
/// `:2106-2175`; the sensor tab is `LAccess.settable`).
fn open_prop_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, root) = picker_popup(dialog);
    // The item/liquid/sensor rows commit `@name`; statuses commit `@status-name`.
    let content_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@");
    let status_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@status-");
    let item = icon_grid("item", "item-prop", &content_pick, 6, 40.0, 300.0);
    let liquid = icon_grid("liquid", "liquid-prop", &content_pick, 6, 40.0, 300.0);
    let status = legacy_status_list(&status_pick);
    let sensors = access_list("settable", state.borrow().privileged, &content_pick);
    picker_tabs(
        &root,
        &["box", "liquid", "effect", "tree"],
        vec![item, liquid, status, sensors],
    );
    popup.popup_centered();
}

/// `SensorStatement`'s item/liquid/unit+block/sensor tabs
/// (`LStatements.java:545-630`; the unit and block lists share the third tab).
fn open_sensor_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let dialog_gd = dialog.to_gd();
    let (mut popup, root) = picker_popup(dialog);
    // Every sensor row commits `@name` (`LStatements.java:545-630`).
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@");
    let item = icon_grid("item", "item", &on_pick, 6, 40.0, 300.0);
    let liquid = icon_grid("liquid", "liquid-prop", &on_pick, 6, 40.0, 300.0);
    // code-instantiated: the unit and block grids share the third sensor tab.
    let mut units_blocks = VBoxContainer::new_alloc();
    units_blocks.add_child(&icon_grid("unit", "unit", &on_pick, 6, 40.0, 220.0));
    units_blocks.add_child(&icon_grid("block", "block", &on_pick, 6, 40.0, 220.0));
    let sensors = access_list("senseable", state.borrow().privileged, &on_pick);
    picker_tabs(
        &root,
        &["box", "liquid", "units", "tree"],
        vec![item, liquid, units_blocks.upcast(), sensors],
    );
    popup.popup_centered();
}

/// A 240x40 name list from `MindLogic.settable_accesses`/`senseable_accesses`
/// (`bundle(LAccess)` + `LCanvas.tooltip(cell, sensor)`).
fn access_list(kind: &str, privileged: bool, on_pick: &PickFn) -> Gd<Control> {
    let Some(mut logic) = kit::logic() else {
        return factory::label(&bundle("@none.found")).upcast();
    };
    // code-instantiated: the access list pane of a picker tab.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(260.0, 300.0));
    let mut list = VBoxContainer::new_alloc();
    scroll.add_child(&list);
    let mut names: Vec<String> = Vec::new();
    if kind == "settable" && logic.has_method("settable_accesses") {
        names = logic
            .call("settable_accesses", &[])
            .try_to::<PackedStringArray>()
            .map(|packed| {
                packed
                    .as_slice()
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default();
    } else if kind == "senseable" && logic.has_method("senseable_accesses") {
        names = logic
            .call("senseable_accesses", &[privileged.to_variant()])
            .try_to::<PackedStringArray>()
            .map(|packed| {
                packed
                    .as_slice()
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default();
    }
    for access_name in names {
        // code-instantiated: one access row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(content_label("laccess", &access_name)));
        button.set_tooltip_text(&gs(bundle_tooltip(&format!(
            "laccess.{}",
            access_name.to_lowercase()
        ))));
        {
            let pick = on_pick.clone();
            let name = access_name.clone();
            let callable = Callable::from_fn("logic_pick_access", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    if list.get_child_count() == 0 {
        list.add_child(&factory::label(&bundle("@none.found")));
    }
    scroll.upcast()
}

/// ApplyStatus's effect rows are reused by SetProp's status tab
/// (`LStatements.java:2138-2150`).
fn legacy_status_list(on_pick: &PickFn) -> Gd<Control> {
    // code-instantiated: the status list pane of the setprop picker tab.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(260.0, 300.0));
    let mut list = VBoxContainer::new_alloc();
    scroll.add_child(&list);
    for row in picker_rows("status") {
        if !picker_visible(&row, "status-prop") {
            continue;
        }
        let entry_name = dict_string(&row, "name");
        // code-instantiated: one status row.
        let mut button = Button::new_alloc();
        button.set_theme_type_variation("flatt");
        button.set_custom_minimum_size(Vector2::new(240.0, 40.0));
        button.set_text_alignment(HorizontalAlignment::LEFT);
        button.set_text(&gs(content_label("status", &entry_name)));
        if let Some(texture) = content_texture("status", &entry_name) {
            button.set_button_icon(&texture);
        }
        {
            let pick = on_pick.clone();
            let name = entry_name.clone();
            let callable = Callable::from_fn("logic_pick_status", move |_args: &[&Variant]| {
                pick(&name);
                Variant::nil()
            });
            let mut node: Gd<Node> = button.clone().upcast();
            node.connect("pressed", &callable);
        }
        list.add_child(&button);
    }
    if list.get_child_count() == 0 {
        list.add_child(&factory::label(&bundle("@none.found")));
    }
    scroll.upcast()
}

/// `PlaySoundStatement.showSoundSelect` (`LStatements.java:2282-2380`): the
/// category tab list on the left and the sound rows (preview + select) on the
/// right, with the current sound's category preselected.
fn open_sound_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let Some(assets) = kit::assets() else {
        return;
    };
    let mut assets_node: Gd<Node> = assets.upcast();
    if !assets_node.has_method("sound_entries") {
        return;
    }
    let entries = assets_node.call("sound_entries", &[]);
    let Ok(entries) = entries.try_to::<VarArray>() else {
        return;
    };
    let mut choices: Vec<SoundEntry> = Vec::new();
    let mut categories: BTreeMap<String, Vec<SoundEntry>> = BTreeMap::new();
    for entry in entries.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let name = dict_string(&entry, "name");
        if name.is_empty() {
            continue;
        }
        let mut id = dict_string(&entry, "id");
        if id.is_empty() {
            id = name.clone();
        }
        let sound = SoundEntry {
            name,
            id,
            category: dict_string(&entry, "category"),
        };
        choices.push(sound.clone());
        categories
            .entry(sound.category.clone())
            .or_default()
            .push(sound);
    }
    if choices.is_empty() {
        return;
    }
    choices.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    let mut category_names: Vec<String> = vec!["all".to_string()];
    category_names.extend(categories.keys().cloned());
    category_names.sort();
    let current_full = current_field_value(state, index, field_index);
    let current = current_full
        .strip_prefix("@sfx-")
        .unwrap_or(&current_full)
        .to_string();
    let current_category = choices
        .iter()
        .find(|sound| sound.name == current)
        .map(|sound| sound.category.clone())
        .unwrap_or_else(|| "all".to_string());
    let dialog_gd = dialog.to_gd();
    let (mut popup, mut root) = picker_popup(dialog);
    // code-instantiated: the category rail + sound list of the sound browser.
    let mut body = HBoxContainer::new_alloc();
    body.add_theme_constant_override("separation", 4);
    root.add_child(&body);
    let mut tabs = VBoxContainer::new_alloc();
    tabs.set_custom_minimum_size(Vector2::new(160.0, 0.0));
    body.add_child(&tabs);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(450.0, 420.0));
    body.add_child(&scroll);
    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", 3);
    scroll.add_child(&list);
    let selected: Rc<RefCell<String>> = Rc::new(RefCell::new(current_category));
    let on_pick = picker_handler(&dialog_gd, state, &popup, index, field_index, "@sfx-");
    let rebuild: Rc<dyn Fn()> = {
        let list = list.clone();
        let selected = selected.clone();
        let choices = choices.clone();
        let categories = categories.clone();
        let current = current.clone();
        let on_pick = on_pick.clone();
        Rc::new(move || {
            rebuild_sound_list(&list, &selected, &choices, &categories, &current, &on_pick);
        })
    };
    for category in &category_names {
        // code-instantiated: one sound category tab.
        let mut toggle = Button::new_alloc();
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_custom_minimum_size(Vector2::new(140.0, 34.0));
        toggle.set_text(&gs(field_label_text(category)));
        toggle.set_toggle_mode(true);
        toggle.set_meta("category", &gs(category.as_str()).to_variant());
        toggle.set_pressed(*selected.borrow() == *category);
        {
            let selected_for_tab = selected.clone();
            let tabs_for_tab = tabs.clone();
            let rebuild_for_tab = rebuild.clone();
            let category_for_tab = category.clone();
            let callable =
                Callable::from_fn("logic_sound_category", move |_args: &[&Variant]| {
                    *selected_for_tab.borrow_mut() = category_for_tab.clone();
                    for child in tabs_for_tab.get_children().iter_shared() {
                        if let Ok(mut button) = child.try_cast::<Button>() {
                            if !button.has_meta("category") {
                                continue;
                            }
                            let current_category = button
                                .get_meta("category")
                                .try_to::<GString>()
                                .map(|value| value.to_string())
                                .unwrap_or_default();
                            button.set_pressed(current_category == category_for_tab);
                        }
                    }
                    rebuild_for_tab();
                    Variant::nil()
                });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        tabs.add_child(&toggle);
    }
    {
        let callable = Callable::from_fn("logic_sound_stop", move |_args: &[&Variant]| {
            if let Some(mut audio) = kit::node("/root/Spine/MindAudio")
                && audio.has_method("stop_sound_preview")
            {
                let _ = audio.call("stop_sound_preview", &[]);
            }
            Variant::nil()
        });
        popup.connect("popup_hide", &callable);
    }
    rebuild();
    popup.popup_centered();
}

/// The sound rows of the selected category (rebuilds the right-hand list).
fn rebuild_sound_list(
    list: &Gd<VBoxContainer>,
    selected: &Rc<RefCell<String>>,
    choices: &[SoundEntry],
    categories: &BTreeMap<String, Vec<SoundEntry>>,
    current: &str,
    on_pick: &PickFn,
) {
    let mut list = list.clone();
    for child in list.get_children().iter_shared() {
        let mut child = child.clone();
        list.remove_child(&child);
        child.queue_free();
    }
    let category = selected.borrow().clone();
    let shown: Vec<SoundEntry> = if category == "all" {
        choices.to_vec()
    } else {
        categories.get(&category).cloned().unwrap_or_default()
    };
    if shown.is_empty() {
        list.add_child(&factory::label(&bundle("@none.found")));
        return;
    }
    for sound in shown {
        // code-instantiated: one sound row (preview + select).
        let mut row = HBoxContainer::new_alloc();
        row.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        list.add_child(&row);
        let mut play = factory::icon_button("play", "", "cleari", AlignmentMode::BEGIN);
        play.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        {
            let sound_id = sound.id.clone();
            let callable = Callable::from_fn("logic_sound_play", move |_args: &[&Variant]| {
                if let Some(mut audio) = kit::node("/root/Spine/MindAudio") {
                    if audio.has_method("preview_sound") {
                        let _ = audio.call(
                            "preview_sound",
                            &[gs(sound_id.as_str()).to_variant(), 1.0f64.to_variant()],
                        );
                    } else if audio.has_method("on_ui_sound") {
                        let _ = audio.call("on_ui_sound", &[gs(sound_id.as_str()).to_variant()]);
                    }
                }
                Variant::nil()
            });
            let mut node: Gd<Node> = play.clone().upcast();
            node.connect("pressed", &callable);
        }
        row.add_child(&play);
        let label = if category == "all" {
            format!("{}/{}", field_label_text(&sound.category), sound.name)
        } else {
            sound.name.clone()
        };
        // code-instantiated: one sound select toggle.
        let mut toggle = Button::new_alloc();
        toggle.set_theme_type_variation("logicTogglet");
        toggle.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        toggle.set_h_size_flags(SizeFlags::EXPAND_FILL);
        toggle.set_text_alignment(HorizontalAlignment::LEFT);
        toggle.set_text(&gs(label));
        toggle.set_toggle_mode(true);
        toggle.set_pressed(sound.name == current);
        {
            let pick = on_pick.clone();
            let sound_name = sound.name.clone();
            let callable = Callable::from_fn("logic_sound_pick", move |_args: &[&Variant]| {
                pick(&sound_name);
                Variant::nil()
            });
            let mut node: Gd<Node> = toggle.clone().upcast();
            node.connect("pressed", &callable);
        }
        row.add_child(&toggle);
    }
}

/// `ui.effects.show(entry -> ...)` for `EffectStatement`
/// (`LStatements.java:1806`).
fn effect_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
    color: Color,
) -> Gd<Button> {
    // code-instantiated: one effect picker button per element.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("logict");
    apply_logict(&mut button, color);
    button.set_custom_minimum_size(Vector2::new(150.0, 40.0));
    button.set_autowrap_mode(AutowrapMode::WORD_SMART);
    button.set_text(&gs(field_label_text(&current_field_value(
        state,
        index,
        field_index,
    ))));
    on_pressed(&button, dialog, state, move |d, s| {
        open_effects(d, s, index, field_index);
    });
    button
}

/// A sibling manifest dialog from the static `UiRoot` tree
/// (`wave_info_dialog.gd:_dialog`).
fn sibling_dialog(dialog_name: &str) -> Option<Gd<Node>> {
    let mut ui_root = kit::node("/root/Spine/Ui/UiRoot")?;
    if !ui_root.has_method("dialog") {
        return None;
    }
    ui_root
        .call("dialog", &[gs(dialog_name).to_variant()])
        .try_to::<Gd<Node>>()
        .ok()
}

fn open_effects(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    field_index: i64,
) {
    let ui = kit::ui();
    let effects = sibling_dialog("effects");
    let (Some(mut ui), Some(mut effects)) = (ui, effects) else {
        open_select(dialog, state, index, field_index, "effect");
        return;
    };
    if !ui.has_method("open_dialog") {
        open_select(dialog, state, index, field_index, "effect");
        return;
    }
    state.borrow_mut().effect_target = (index, field_index);
    let needs_effect_binding = !state.borrow().effect_connected;
    if effects.has_signal("effect_selected") && needs_effect_binding {
        let dialog_gd = dialog.to_gd();
        let state_for_effect = state.clone();
        let callable = Callable::from_fn("logic_effect_selected", move |args: &[&Variant]| {
            let effect_name = args
                .first()
                .map(|value| value.stringify().to_string())
                .unwrap_or_default();
            let target = state_for_effect.borrow().effect_target;
            if target.0 < 0 || effect_name.is_empty() {
                return Variant::nil();
            }
            state_for_effect.borrow_mut().effect_target = (-1, -1);
            commit_field_value(&state_for_effect, &effect_name, target.0, target.1);
            if dialog_gd.is_instance_valid() {
                let mut handle = dialog_gd.clone();
                let mut bound = handle.bind_mut();
                rebuild_list(&mut bound, &state_for_effect);
            }
            Variant::nil()
        });
        effects.connect("effect_selected", &callable);
        state.borrow_mut().effect_connected = true;
    }
    let _ = ui.call(
        "open_dialog",
        &[gs("effects").to_variant(), gs("{}").to_variant()],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_and_sound_fields_have_list_pickers() {
        // `LStatements.java` `showSelectTable` sites: `playsound` field 1 is the
        // sound browser, `ubind` field 0 the unit grid (GAPS #46: these are list
        // pickers, not raw text fields).
        let flag = [String::from("false")];
        assert_eq!(picker_kind("playsound", 1, &flag), "sound");
        assert_eq!(content_picker("ubind", 0), "unit");
        assert_eq!(content_picker("status", 1), "status");
        assert_eq!(content_picker("weatherset", 0), "weather");
        assert_eq!(content_picker("sensor", 2), "sensor");
        assert_eq!(content_picker("setprop", 0), "prop");
        // Discriminator-gated pickers stay off for the other variants.
        assert_eq!(content_picker("setmarker", 2), "layer");
        assert_eq!(
            picker_kind("setmarker", 2, &[String::from("drawLayer")]),
            "layer"
        );
        assert_eq!(picker_kind("setmarker", 2, &[String::from("flush")]), "");
        assert_eq!(content_picker("ulocate", 3), "ore");
        assert_eq!(picker_kind("ulocate", 3, &[String::from("building")]), "");
        // A field with no picker renders as a plain styled widget.
        assert_eq!(content_picker("noop", 0), "");
    }
}
